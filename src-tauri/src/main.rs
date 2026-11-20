// ไม่เปิดหน้าต่าง console บน Windows ใน release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // โหมด bridge ของผู้ช่วย AI: CLI ของ AI รันแอปนี้เป็น MCP server แบบ stdio ที่ส่งต่อไปยังแอปที่เปิดอยู่ (ไม่เปิดหน้าต่าง)
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--mcp-bridge") {
        let port = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
        std::process::exit(if puri_core::assistant::mcp::bridge(port).is_ok() { 0 } else { 1 });
    }
    puri_photo_search_lib::run()
}
