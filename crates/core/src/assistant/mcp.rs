//! MCP server ขนาดเล็กพอให้ CLI ของ AI เรียกเครื่องมือของแอปได้ (แนวทางเดียวกับ PuriCat `mcp.rs`)
//!
//! JSON-RPC 2.0 บรรทัดละหนึ่งข้อความ · เปิดเฉพาะระหว่างคุยหนึ่งรอบ บน 127.0.0.1 port สุ่ม พร้อม token สุ่ม
//! (บรรทัดแรกของทุกการเชื่อมต่อต้องเป็น token — process อื่นที่ไม่รู้ token ใช้เครื่องมือไม่ได้)
//! CLI เปิด socket เองไม่ได้ จึงสั่งให้มันรันตัวแอปเองในโหมด [`bridge`] เป็น MCP server แบบ stdio ที่ส่งต่อมาที่นี่
//! ตอบเฉพาะ `initialize`, `ping`, `tools/list`, `tools/call`

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// ชื่อที่เครื่องมือถูกลงทะเบียนใน CLI (Claude เห็นเป็น `mcp__puriphoto__…`)
pub const SERVER_NAME: &str = "puriphoto";
/// ตัวแปรสภาพแวดล้อมที่ส่ง token ให้ bridge (ไม่ใส่ใน argument เพราะ process อื่นอ่าน argument ได้)
pub const TOKEN_ENV: &str = "PURI_MCP_TOKEN";
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-11-25", "2025-03-26", "2024-11-05"];
const MAX_LINE_BYTES: usize = 1 << 20;

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub read_only: bool,
}

/// ผลของเครื่องมือ: เนื้อหาแบบ MCP (`text` / `image`)
pub struct ToolOutput {
    pub content: Vec<Value>,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        ToolOutput { content: vec![json!({ "type": "text", "text": text.into() })], is_error: false }
    }
    pub fn error(text: impl Into<String>) -> Self {
        ToolOutput { content: vec![json!({ "type": "text", "text": text.into() })], is_error: true }
    }
}

pub trait ToolHost: Send + Sync + 'static {
    fn tools(&self) -> Vec<ToolSpec>;
    fn call(&self, name: &str, arguments: Value) -> ToolOutput;
}

/// ตอบข้อความหนึ่งข้อความ — None = ไม่ต้องตอบ (notification หรือไม่ใช่ JSON-RPC)
pub fn respond(host: &dyn ToolHost, message: &Value) -> Option<Value> {
    let method = message.get("method")?.as_str()?;
    let id = message.get("id").filter(|id| !id.is_null())?.clone();
    let params = message.get("params");
    let outcome: Result<Value, (i64, &str)> = match method {
        "initialize" => {
            let asked = params.and_then(|p| p.get("protocolVersion")).and_then(Value::as_str);
            let version = PROTOCOL_VERSIONS.iter().find(|v| Some(**v) == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": host.tools().into_iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
                "annotations": { "readOnlyHint": t.read_only },
            })).collect::<Vec<_>>(),
        })),
        "tools/call" => {
            let name = params.and_then(|p| p.get("name")).and_then(Value::as_str);
            match name.filter(|n| host.tools().iter().any(|t| t.name == *n)) {
                Some(name) => {
                    let arguments = params.and_then(|p| p.get("arguments")).cloned().unwrap_or_else(|| json!({}));
                    let out = host.call(name, arguments);
                    Ok(json!({ "content": out.content, "isError": out.is_error }))
                }
                None => Err((-32602, "Unknown tool")),
            }
        }
        _ => Err((-32601, "Method not found")),
    };
    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    })
}

/// ปลายทางของเครื่องมือสำหรับการคุยหนึ่งรอบ — drop แล้วปิด port และตัดทุกการเชื่อมต่อ
pub struct Endpoint {
    pub port: u16,
    pub token: String,
    stop: Arc<AtomicBool>,
    accept: Option<std::thread::JoinHandle<()>>,
}

impl Endpoint {
    pub fn start(host: Arc<dyn ToolHost>) -> std::io::Result<Endpoint> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, token2) = (stop.clone(), token.clone());
        let accept = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (host, stop, token) = (host.clone(), stop2.clone(), token2.clone());
                        std::thread::spawn(move || {
                            let _ = serve(stream, host.as_ref(), &token, &stop);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(40))
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(40)),
                }
            }
        });
        Ok(Endpoint { port, token, stop, accept: Some(accept) })
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
    }
}

fn serve(stream: TcpStream, host: &dyn ToolHost, token: &str, stop: &AtomicBool) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut authed = false;
    let started = std::time::Instant::now();
    let mut line = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = stream.shutdown(Shutdown::Both);
            return Ok(());
        }
        match (&mut reader).take(MAX_LINE_BYTES as u64 + 1).read_until(b'\n', &mut line) {
            Ok(0) => return Ok(()),
            Ok(_) if line.len() > MAX_LINE_BYTES => return Ok(()),
            Ok(_) if line.ends_with(b"\n") => {}
            Ok(_) => continue, // ยังไม่ครบบรรทัด
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                if !authed && started.elapsed() > Duration::from_secs(10) {
                    return Ok(());
                }
                continue;
            }
            Err(e) => return Err(e),
        }
        let text = String::from_utf8_lossy(&line).trim().to_string();
        line.clear();
        if !authed {
            if text != token {
                let _ = stream.shutdown(Shutdown::Both);
                return Ok(());
            }
            authed = true;
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&text) else { continue };
        if let Some(response) = respond(host, &message) {
            let mut out = serde_json::to_vec(&response).unwrap_or_default();
            out.push(b'\n');
            writer.write_all(&out)?;
            writer.flush()?;
        }
    }
}

/// โหมด bridge (`<แอป> --mcp-bridge <port>`): ต่อไปที่ปลายทางของแอปที่กำลังคุย แล้วส่ง stdin → socket และ socket → stdout
/// ไม่เปิดหน้าต่าง ไม่เปิดคลัง — token อ่านจาก [`TOKEN_ENV`]
pub fn bridge(port: u16) -> std::io::Result<()> {
    let token = std::env::var(TOKEN_ENV).map_err(|_| std::io::Error::other("missing token"))?;
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.write_all(format!("{token}\n").as_bytes())?;
    let mut upstream = stream.try_clone()?;
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut upstream);
        let _ = upstream.shutdown(Shutdown::Write);
    });
    let mut out = std::io::stdout().lock();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        out.write_all(&buf[..n])?;
        out.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;
    impl ToolHost for Echo {
        fn tools(&self) -> Vec<ToolSpec> {
            vec![ToolSpec {
                name: "echo",
                description: "echo",
                input_schema: json!({ "type": "object" }),
                read_only: true,
            }]
        }
        fn call(&self, _name: &str, arguments: Value) -> ToolOutput {
            ToolOutput::text(arguments.to_string())
        }
    }

    fn rpc(stream: &mut TcpStream, reader: &mut BufReader<TcpStream>, message: Value) -> Value {
        stream.write_all(format!("{message}\n").as_bytes()).unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn answers_the_protocol_only_after_the_token() {
        let endpoint = Endpoint::start(Arc::new(Echo)).unwrap();
        // ไม่รู้ token: ถูกตัดการเชื่อมต่อ ไม่ได้คำตอบ
        let mut wrong = TcpStream::connect(("127.0.0.1", endpoint.port)).unwrap();
        wrong.write_all(b"guess\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n").unwrap();
        let mut buf = String::new();
        wrong.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        assert_eq!(BufReader::new(wrong).read_line(&mut buf).unwrap_or(0), 0);

        let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        stream.write_all(format!("{}\n", endpoint.token).as_bytes()).unwrap();
        let init = rpc(
            &mut stream,
            &mut reader,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        let list = rpc(&mut stream, &mut reader, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
        assert_eq!(list["result"]["tools"][0]["name"], "echo");
        let call = rpc(
            &mut stream,
            &mut reader,
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "echo", "arguments": { "a": 1 } } }),
        );
        assert_eq!(call["result"]["content"][0]["text"], "{\"a\":1}");
        let unknown = rpc(
            &mut stream,
            &mut reader,
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "rm" } }),
        );
        assert_eq!(unknown["error"]["code"], -32602);
        drop(endpoint);
    }
}
