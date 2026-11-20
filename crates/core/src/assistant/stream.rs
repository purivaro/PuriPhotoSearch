//! อ่านบรรทัด JSON ที่ CLI พิมพ์ออกมาระหว่างคุยหนึ่งรอบ → ข้อความคำตอบ และคำตัดสินว่าจบแบบสำเร็จหรือล้ม
//!
//! - Claude Code `-p --output-format stream-json --verbose --include-partial-messages` — รูปแบบเดียวกับที่ PuriCat เก็บจากของจริง (2.1.303)
//! - Codex `exec --json` — `thread.started` / `item.completed` (agent_message) / `turn.completed` / `turn.failed` (ยืนยันกับ 0.171 เมื่อ 21 พ.ย. 2569)
//! - Gemini CLI `--output-format stream-json` — ตามเอกสาร (init / message / tool_use / tool_result / result) **ยังไม่ได้ยืนยันกับของจริง**

use super::detect::Provider;
use serde_json::Value;

pub struct StreamParser {
    provider: Provider,
    /// คำตอบที่ประกอบแล้ว
    pub text: String,
    /// ข้อความของ Claude: ข้อความหนึ่งรอบแบ่งเป็นหลายข้อความเมื่อมีการเรียกเครื่องมือ — แต่ละข้อความเป็นย่อหน้าของตัวเอง
    base: usize,
    message_id: Option<String>,
    message_text: String,
    pub model: Option<String>,
    /// Codex: รหัสของรอบ (ใช้หาภาพที่ Codex สร้าง)
    pub thread_id: Option<String>,
    /// Some(Ok) เมื่อ CLI บอกว่าสำเร็จ · Some(Err) พร้อมคำอธิบายของ CLI เอง
    pub outcome: Option<Result<(), String>>,
    /// ข้อผิดพลาดที่ CLI แจ้งแต่ยังไม่จบรอบ (อาจกำลังลองใหม่) — ใช้อธิบายเมื่อรอบจบโดยไม่มีคำตัดสิน
    pub notice: Option<String>,
}

impl StreamParser {
    pub fn new(provider: Provider) -> Self {
        StreamParser {
            provider,
            text: String::new(),
            base: 0,
            message_id: None,
            message_text: String::new(),
            model: None,
            thread_id: None,
            outcome: None,
            notice: None,
        }
    }

    /// อ่านหนึ่งบรรทัด — true เมื่อข้อความคำตอบเปลี่ยน
    pub fn line(&mut self, line: &str) -> bool {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return false };
        let before = self.text.clone();
        match self.provider {
            Provider::Claude => self.claude(&v),
            Provider::Codex => self.codex(&v),
            Provider::Gemini => self.gemini(&v),
        }
        self.text != before
    }

    fn start_message(&mut self, id: Option<&str>) {
        self.message_id = id.map(str::to_string);
        self.base = self.text.len();
        self.message_text.clear();
    }

    fn claude(&mut self, v: &Value) {
        let at = |path: &[&str]| path.iter().try_fold(v, |x, k| x.get(k)).and_then(Value::as_str);
        match at(&["type"]) {
            Some("system") if at(&["subtype"]) == Some("init") => self.model = at(&["model"]).map(str::to_string),
            Some("stream_event") if at(&["event", "type"]) == Some("message_start") => {
                self.start_message(at(&["event", "message", "id"]));
            }
            Some("stream_event")
                if at(&["event", "type"]) == Some("content_block_delta")
                    && at(&["event", "delta", "type"]) == Some("text_delta") =>
            {
                let delta = at(&["event", "delta", "text"]).unwrap_or("");
                if delta.is_empty() {
                    return;
                }
                if self.base > 0 && self.text.len() == self.base {
                    self.text.push_str("\n\n");
                }
                self.text.push_str(delta);
                self.message_text.push_str(delta);
            }
            Some("assistant") if v.get("error").is_none_or(Value::is_null) => {
                let text: String = v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                            .filter_map(|b| b.get("text").and_then(Value::as_str))
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                if text.is_empty() {
                    return;
                }
                let id = at(&["message", "id"]);
                if id.is_some() && self.message_id.as_deref() != id {
                    // ข้อความใหม่ที่ไม่ได้ stream มาก่อน
                    if !self.message_text.is_empty() {
                        self.start_message(id);
                    } else {
                        self.message_id = id.map(str::to_string);
                    }
                }
                // ข้อความเต็มแทนสิ่งที่ stream มา (ถ้าต่างกัน)
                if self.message_text != text {
                    self.text.truncate(self.base);
                    if self.base > 0 {
                        self.text.push_str("\n\n");
                    }
                    self.text.push_str(&text);
                    self.message_text = text;
                }
            }
            Some("result") => {
                let result = at(&["result"]).unwrap_or("").trim().to_string();
                let failed = v.get("is_error").and_then(Value::as_bool).unwrap_or(false)
                    || at(&["subtype"]).is_some_and(|s| s != "success");
                if failed {
                    self.outcome = Some(Err(if result.is_empty() {
                        "CLI แจ้งข้อผิดพลาดโดยไม่มีข้อความ".into()
                    } else {
                        result
                    }));
                } else {
                    if self.text.is_empty() && !result.is_empty() {
                        self.text = result;
                    }
                    self.outcome = Some(Ok(()));
                }
            }
            _ => {}
        }
    }

    fn codex(&mut self, v: &Value) {
        match v.get("type").and_then(Value::as_str) {
            Some("thread.started") => self.thread_id = v.get("thread_id").and_then(Value::as_str).map(str::to_string),
            Some("item.completed") => {
                let item = v.get("item");
                if item.and_then(|i| i.get("type")).and_then(Value::as_str) != Some("agent_message") {
                    return;
                }
                let text = item.and_then(|i| i.get("text")).and_then(Value::as_str).unwrap_or("");
                if text.is_empty() {
                    return;
                }
                if !self.text.is_empty() {
                    self.text.push_str("\n\n");
                }
                self.text.push_str(text);
            }
            Some("turn.completed") => self.outcome = Some(Ok(())),
            Some(kind @ ("turn.failed" | "error")) => {
                let raw = v
                    .pointer("/error/message")
                    .or_else(|| v.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("CLI แจ้งข้อผิดพลาดโดยไม่มีข้อความ");
                let message = inner_error(raw);
                if kind == "turn.failed" {
                    self.outcome = Some(Err(message));
                } else {
                    self.notice = Some(message);
                }
            }
            _ => {}
        }
    }

    fn gemini(&mut self, v: &Value) {
        match v.get("type").and_then(Value::as_str) {
            Some("init") => self.model = v.get("model").and_then(Value::as_str).map(str::to_string),
            Some("message") if v.get("role").and_then(Value::as_str) == Some("assistant") => {
                let content = v.get("content").and_then(Value::as_str).unwrap_or("");
                if v.get("delta").and_then(Value::as_bool).unwrap_or(false) {
                    self.text.push_str(content);
                } else if !content.is_empty() {
                    if !self.text.is_empty() {
                        self.text.push_str("\n\n");
                    }
                    self.text.push_str(content);
                }
            }
            Some("error") => {
                let message = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
                if v.get("severity").and_then(Value::as_str) == Some("error") {
                    self.outcome = Some(Err(message));
                } else if !message.is_empty() {
                    self.notice = Some(message);
                }
            }
            Some("result") => {
                if v.get("status").and_then(Value::as_str) == Some("success") {
                    self.outcome = Some(Ok(()));
                } else {
                    let message = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("Gemini แจ้งว่าไม่สำเร็จ");
                    self.outcome = Some(Err(message.to_string()));
                }
            }
            _ => {}
        }
    }
}

/// Codex ห่อข้อผิดพลาด JSON ของบริการไว้ในข้อความ: ดึงประโยคข้างในออกมา
fn inner_error(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_streams_text_and_joins_messages_around_tool_calls() {
        let mut p = StreamParser::new(Provider::Claude);
        for line in [
            r#"{"type":"system","subtype":"init","model":"claude-opus-5-5","tools":[]}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"จะค้นรูป"}}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"จะค้นรูป"},{"type":"tool_use","name":"x"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"m2"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"เจอ 3 รูป"}}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"เจอ 3 รูป"}"#,
        ] {
            p.line(line);
        }
        assert_eq!(p.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(p.text, "จะค้นรูป\n\nเจอ 3 รูป");
        assert_eq!(p.outcome, Some(Ok(())));

        let mut failed = StreamParser::new(Provider::Claude);
        failed.line(r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"There's an issue with the selected model"}"#);
        assert_eq!(failed.outcome, Some(Err("There's an issue with the selected model".into())));
    }

    #[test]
    fn codex_lines_captured_from_the_real_cli() {
        // บรรทัดจริงจาก codex-cli 0.171.0-alpha.13 (21 พ.ย. 2569)
        let mut p = StreamParser::new(Provider::Codex);
        for line in [
            r#"{"type":"thread.started","thread_id":"01a1fa30-d031-79e2-bb7e-5d177b0c9346"}"#,
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"I’ll use the image generation tool."}}"#,
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Here’s your scene."}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":49053}}"#,
        ] {
            p.line(line);
        }
        assert_eq!(p.thread_id.as_deref(), Some("01a1fa30-d031-79e2-bb7e-5d177b0c9346"));
        assert_eq!(p.text, "I’ll use the image generation tool.\n\nHere’s your scene.");
        assert_eq!(p.outcome, Some(Ok(())));

        // ข้อผิดพลาดจริงจาก codex-cli 0.133.0 (รุ่นเก่า)
        let mut old = StreamParser::new(Provider::Codex);
        old.line(r#"{"type":"turn.failed","error":{"message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'gpt-5.5' model is not supported when using Codex with a ChatGPT account.\",\"code\":\"model_not_found\"}}"}}"#);
        assert_eq!(
            old.outcome,
            Some(Err("The 'gpt-5.5' model is not supported when using Codex with a ChatGPT account.".into()))
        );
    }

    #[test]
    fn gemini_documented_shape() {
        let mut p = StreamParser::new(Provider::Gemini);
        for line in [
            r#"{"type":"init","session_id":"s","model":"gemini-2.5-pro"}"#,
            r#"{"type":"message","role":"user","content":"hi"}"#,
            r#"{"type":"message","role":"assistant","content":"สวัส","delta":true}"#,
            r#"{"type":"message","role":"assistant","content":"ดีครับ","delta":true}"#,
            r#"{"type":"result","status":"success","stats":{}}"#,
        ] {
            p.line(line);
        }
        assert_eq!(p.text, "สวัสดีครับ");
        assert_eq!(p.outcome, Some(Ok(())));
    }
}
