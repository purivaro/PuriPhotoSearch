# AGENTS.md — กติกาสำหรับผู้พัฒนา (คนและ AI) ของ Puri Photo Search 2

ข้อกำหนดผลิตภัณฑ์คือ `Puri_Photo_Search_2_UX_UI_Blueprint.md` ฉบับ 2.2 (ของเจ้าของผลิตภัณฑ์ อยู่นอก repo นี้)
สถานะจริงของงานอยู่ที่ [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) — อ่านก่อนเริ่มงานทุกครั้ง

## ห้ามเด็ดขาด

1. **ห้ามแก้ ย้าย เปลี่ยนชื่อ หรือลบไฟล์รูปต้นฉบับของผู้ใช้** โค้ดที่เขียนไฟล์ได้มีสามที่เท่านั้น: โฟลเดอร์ cache ของแอป
   (`media::write_cache_file`), โฟลเดอร์ปลายทางที่ผู้ใช้เลือกตอนส่งออก (`export.rs`) และชุดสำรองของคลัง (`backup.rs`: โฟลเดอร์ข้อมูลของแอป
   หรือโฟลเดอร์ที่ผู้ใช้เลือก ซึ่งต้องไม่อยู่ในโฟลเดอร์รูปที่คลังติดตาม) เทสต์ `t34_…` ต้องผ่านเสมอ
2. **ห้ามให้การสแกนซ้ำ การเปลี่ยนโมเดล หรือการย้ายข้อมูล ทับคำยืนยันของผู้ใช้** (`face_assignments`)
3. **ห้ามแสดงผลจำลอง** — ไม่มี mock data ใน UI; ความสามารถที่ยังไม่มีต้องซ่อนหรือบอกตามจริงผ่าน `get_capabilities`
4. **ห้ามเขียนลงข้อมูลของรุ่น Python** (`~/Library/Application Support/PuriPhotoSearch/`, `/Volumes/WD Puri/Dev/Mac/photosearch-mac`)
5. ห้าม push เข้า `main`, ห้ามเผยแพร่ installer และห้ามแตะคลังจริงของผู้ใช้โดยไม่ได้รับคำสั่งจากเจ้าของผลิตภัณฑ์
   (กติกากลาง: `~/Dev/PURI-AI-OFFICE.md`)

## โครง

| ที่ | หน้าที่ | ห้าม |
|---|---|---|
| `crates/core` | กติกาข้อมูลทั้งหมด, SQLite, งานเบื้องหลัง, คำสั่ง (`api::dispatch`) | `use tauri` |
| `src-tauri` | เปิดคลัง, ลงทะเบียนคำสั่ง, protocol `puri://`, native picker, hook e2e | มีตรรกะข้อมูล |
| `crates/devbridge` | เครื่องมือพัฒนา: dispatch เดียวกันผ่าน HTTP ในเครื่อง | ถูกอ้างจาก `src-tauri` |
| `src/` | React UI; เรียก Rust ผ่าน `src/lib/ipc/client.ts` เท่านั้น | เปิด SQLite, อ่านไฟล์, สร้างข้อมูลตัวอย่าง |
| `e2e/` | ทดสอบแอปจริง (ฉีดสคริปต์เข้า debug build) | |

เพิ่มคำสั่งใหม่ต้องแก้สามที่พร้อมกัน: `Command` + `COMMAND_NAMES` ใน `crates/core/src/api.rs`, `domain_commands!` ใน
`src-tauri/src/lib.rs`, และ `Commands` ใน `src/lib/ipc/types.ts` (มีเทสต์จับเมื่อไม่ตรงกัน)

## ก่อนส่งงาน

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # เครื่องนี้ติดตั้ง rustup ผ่าน Homebrew
pnpm check          # typecheck + vitest + cargo fmt --check + clippy -D warnings + cargo test
e2e/run.sh          # เส้นทางหลักในแอปจริง (ต้องมี desktop session)
```

- ทุกพฤติกรรมที่แตะข้อมูลต้องมีเทสต์ใน `crates/core/tests/` ที่ใช้ไฟล์จริงและวัดผลจริง
- อัปเดต `docs/IMPLEMENTATION_STATUS.md` และ `docs/TEST_REPORT.md` ด้วยสิ่งที่รันจริง; สิ่งที่ไม่ได้รันให้เขียนว่าไม่ได้รัน
- UI: ภาษาไทย, ไม่ใช้ `alert/confirm/prompt`, ทุกหน้ามี loading / empty / error ที่ลองใหม่ได้, ไม่ hardcode จำนวนหรือชื่อ
- commit เป็นชุดที่ review ได้บน branch แล้วเปิด Draft PR; ไม่ commit ภาพ screenshot (อยู่ใน `screenshots/` ซึ่ง gitignore)
