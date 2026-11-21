# ต้นแบบวัดความเร็วตัวอย่างสด (21 พ.ย. 2569 — DECISIONS ข้อ 30)

ไม่ใช่ส่วนของแอป · เก็บไว้ให้วัดซ้ำได้

- `src/lib.rs` — สูตรของ `crates/core` (adjust/local/masks ไฟล์เดียวกัน) คอมไพล์เป็น WASM:
  `RUSTFLAGS="-C target-feature=+simd128" cargo build --release --target wasm32-unknown-unknown` แล้วคัดลอกไปที่ `src/bench/preview_wasm.wasm`
- `e2e/bench-preview.js` — สคริปต์ที่ฉีดเข้าหน้าต่างแอป (debug build ที่ optimize):
  `CARGO_TARGET_DIR=target/bench CARGO_PROFILE_DEV_OPT_LEVEL=3 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_OVERFLOW_CHECKS=false pnpm tauri build --debug --no-bundle`
  แล้ว `PURI_DATA_DIR=<คลังทดสอบ> PURI_E2E_SCRIPT=e2e/bench-preview.js PURI_E2E_REPORT=report.json src-tauri/target/bench/debug/puri-photo-search`
  (คลังอยู่ที่ `<PURI_DATA_DIR>/data/library/` · สคริปต์ใช้รูป MOT03115 ของคลังทดสอบ — เปลี่ยน id ในสคริปต์ได้)
