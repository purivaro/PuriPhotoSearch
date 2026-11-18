//! Dev bridge — เครื่องมือพัฒนาเท่านั้น ไม่ถูกรวมในแอปที่แจก
//!
//! เปิด `puri_core::api::dispatch` ตัวเดียวกับแอปจริงผ่าน HTTP บน 127.0.0.1:1421 เพื่อให้ UI ที่รันด้วย
//! `pnpm dev` ในเบราว์เซอร์ทำงานกับ Rust จริงและคลังทดสอบจริง (ไม่มี mock data ฝั่ง UI)
//!
//!   cargo run -p puri-devbridge                 คลังทดสอบที่ target/dev-library
//!   PURI_DATA_DIR=/some/dir cargo run -p puri-devbridge

use puri_core::{api, App, AppError, NoopSink, Options};
use std::path::PathBuf;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

const ALLOWED_ORIGIN: &str = "http://localhost:1420";

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn respond(request: Request, status: u16, content_type: &str, body: Vec<u8>) {
    let response = Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", content_type))
        .with_header(header("Access-Control-Allow-Origin", ALLOWED_ORIGIN))
        .with_header(header("Access-Control-Allow-Headers", "content-type"))
        .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = request.respond(response);
}

fn json_error(e: &AppError) -> Vec<u8> {
    serde_json::to_vec(e).unwrap_or_default()
}

fn handle(app: &App, mut request: Request) {
    let url = request.url().to_string();
    match (request.method().clone(), url.as_str()) {
        (Method::Options, _) => respond(request, 204, "text/plain", vec![]),
        (Method::Post, "/rpc") => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let result = serde_json::from_str::<serde_json::Value>(&body).map_err(AppError::from).and_then(|v| {
                let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or_default().to_string();
                api::dispatch(app, &cmd, v.get("args").cloned().unwrap_or_default())
            });
            match result {
                Ok(value) => respond(request, 200, "application/json", serde_json::to_vec(&value).unwrap_or_default()),
                Err(e) => respond(request, 400, "application/json", json_error(&e)),
            }
        }
        (Method::Get, path) if path.starts_with("/asset/") => match api::asset(app, &path["/asset/".len()..]) {
            Ok(asset) => respond(request, 200, asset.mime, asset.bytes),
            Err(e) => respond(request, 404, "application/json", json_error(&e)),
        },
        _ => respond(request, 404, "text/plain", b"not found".to_vec()),
    }
}

fn main() {
    let base =
        std::env::var_os("PURI_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/dev-library"));
    let app = App::open(Options {
        data_dir: base.join("data"),
        cache_dir: base.join("cache"),
        sink: Arc::new(NoopSink),
        folder_picker: "dev_path_input",
        start_worker: true,
        face_engine: puri_faces::app::provider(),
        face_engine_unavailable: puri_faces::app::UNAVAILABLE,
    })
    .unwrap_or_else(|e| panic!("เปิดคลังทดสอบไม่ได้: {e}"));
    let server = Arc::new(Server::http("127.0.0.1:1421").expect("bind 127.0.0.1:1421"));
    eprintln!("puri-devbridge: http://127.0.0.1:1421  (คลังทดสอบ: {})", base.display());
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let (server, app) = (server.clone(), app.clone());
            std::thread::spawn(move || {
                while let Ok(request) = server.recv() {
                    handle(&app, request);
                }
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
}
