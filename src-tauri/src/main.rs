// ไม่เปิดหน้าต่าง console บน Windows ใน release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    puri_photo_search_lib::run()
}
