# การตัดสินใจด้านสถาปัตยกรรม

บันทึกสิ่งที่เลือก เหตุผล และผลที่ตามมา ณ 18 พ.ย. 2569 ข้อที่ขึ้นต้นด้วย D01–D13 คือข้อเสนอเริ่มต้นของ Blueprint หมวด 0.2
ข้อที่ต่างจาก Blueprint ระบุไว้ชัดว่า "ต่างจากเอกสาร" พร้อมเหตุผล

| # | เรื่อง | ที่เลือก | สถานะ |
|---|---|---|---|
| 1 | เปลือกแอป | Tauri 2 + React 19 + TypeScript + Vite + Tailwind 4 (D01–D02) | ใช้แล้ว |
| 2 | โครง Rust | `crates/core` ไม่ขึ้นกับ Tauri + `src-tauri` เป็นเปลือกบาง | ใช้แล้ว — ต่างจากผัง path ของหมวด 15 |
| 3 | IPC | หนึ่งคำสั่ง Tauri ต่อคำสั่งโดเมน ทุกตัวส่งต่อไป `api::dispatch` ที่ตรวจชนิดด้วย serde | ใช้แล้ว |
| 4 | ฐานข้อมูล | SQLite ผ่าน `rusqlite` (bundled), WAL, foreign keys เปิดทุก connection (D04) | ใช้แล้ว |
| 5 | ตัวตนของไฟล์ | path ที่ `realpath` คืน เป็นคีย์ตำแหน่ง; `photo_id` ไม่ผูกกับ path | ใช้แล้ว บางส่วน |
| 6 | สิทธิ์ไฟล์บน macOS | แจกนอก App Store ไม่เปิด App Sandbox | เสนอ — รอยืนยันช่องทางแจก |
| 7 | การเสิร์ฟรูป | protocol `puri://` รับเฉพาะ ID | ใช้แล้ว |
| 8 | ถอดรหัสรูป | crate `image` (JPEG/PNG) เป็นฐาน; วางแผน adapter ImageIO | ใช้แล้ว / วางแผน |
| 9 | โมเดลใบหน้า | **ยังไม่เลือก** — ติดสัญญาอนุญาต | รอเจ้าของผลิตภัณฑ์ |
| 10 | การทดสอบแอปจริง | ฉีดสคริปต์เข้า WKWebView ของ debug build (แบบ PuriCat) | ใช้แล้ว |
| 11 | Dev bridge | binary แยกสำหรับพัฒนา UI ในเบราว์เซอร์กับ Rust จริง | ใช้แล้ว |
| 12 | แพลตฟอร์ม | macOS Apple Silicon ก่อน (D03) | ทดสอบเฉพาะเครื่องนี้ |

## 1. เปลือกแอป: Tauri 2 + React

ตามข้อเสนอ D01–D02 และตรงกับ PuriCat (`~/Dev/Rust/PuriCat`) ที่เจ้าของผลิตภัณฑ์ชี้ให้ใช้เป็นแนวทาง
เวอร์ชัน frontend ใช้ชุดเดียวกับ PuriCat ซึ่ง build ผ่านบนเครื่องนี้แล้ว เวอร์ชันจริงถูกตรึงด้วย `Cargo.lock` และ `pnpm-lock.yaml`

Tauri วาด UI ด้วย WebView ของระบบ (WKWebView) จึงเป็น "แอป desktop ที่มี Rust backend" ไม่ใช่ native controls

## 2. `crates/core` แยกจาก Tauri — ต่างจากเอกสาร

Blueprint หมวด 15 วางโมดูลไว้ใต้ `src-tauri/src/{commands,domain,services,storage,…}` งานนี้ย้ายตรรกะทั้งหมดไปที่ crate `puri-core`
แล้วให้ `src-tauri` เหลือไฟล์เดียว

- เหตุผล: ทดสอบกติกาข้อมูลได้ด้วย `cargo test` โดยไม่ต้องมีหน้าต่าง, compile เร็ว, ใช้ core ร่วมกับ dev bridge และเครื่องมือ CLI ในอนาคตได้
  (เหตุผลเดียวกับ ADR 0002 ของ PuriCat)
- ขอบเขตความรับผิดชอบของหมวด 7.2 ยังอยู่ครบในรูปโมดูลของ core: `db` `import` `jobs` `media` `catalog` `people` `faces` `export`
  `legacy` `platform` `assets` `api`
- ผลที่ตามมา: โค้ดใน core ห้าม `use tauri`

## 3. IPC: คำสั่งโดเมนที่มีชนิด ผ่าน dispatcher เดียว

`src-tauri` ลงทะเบียนคำสั่ง Tauri ตามรายชื่อใน `api::COMMAND_NAMES` (45 คำสั่ง) ด้วย macro `domain_commands!` แต่ละตัวส่ง
`(ชื่อ, args)` ไป `puri_core::api::dispatch` ซึ่ง deserialize เป็น enum `Command` — argument ผิดชนิดหรือชื่อที่ไม่รู้จักถูกปฏิเสธ

- ไม่มีคำสั่ง SQL, shell หรืออ่านไฟล์ด้วย path อิสระ; UI ส่ง ID เท่านั้น ยกเว้น path ที่ผู้ใช้เลือกเอง (โฟลเดอร์นำเข้า, ปลายทางส่งออก)
- เทสต์ `ipc_contract_matches_typescript_command_table` เทียบรายชื่อใน `src/lib/ipc/types.ts` กับ `COMMAND_NAMES`
  และเทสต์ใน `src-tauri` เทียบ macro กับ `COMMAND_NAMES` — กัน Rust/TypeScript ไม่ตรงกันในระดับชื่อคำสั่ง
- **ยังไม่ทำ:** ตรวจรูปร่างของ DTO ทั้งสองฝั่งอัตโนมัติ (ตอนนี้ตรงกันด้วยการเขียนมือ) และยังไม่ได้จำกัดคำสั่งด้วย `AppManifest`
  ของ Tauri (Blueprint 7.3) — หน้าต่างมีเดียวและโหลดเฉพาะ asset ในแอปภายใต้ CSP

## 4. ฐานข้อมูล

- ไฟล์ `library/library.sqlite` ใน app data; cache (ภาพย่อ, ครอปใบหน้า) อยู่ใน app cache แยกจากฐาน และสร้างใหม่ได้
- `PRAGMA application_id` = `0x50555249` ใช้แยกคลังของเราจาก SQLite อื่น: เปิดไฟล์ที่ไม่ใช่คลังจะถูกปฏิเสธโดยไม่แก้ไฟล์
- migration เรียงตาม `PRAGMA user_version`; คลังที่ใหม่กว่าแอปถูกปฏิเสธ ไม่ downgrade
- `library.lock` (file lock ของระบบ) กันสอง process เปิดคลังเดียวกัน
- ผู้เขียนมีสองตัว: connection ของคำสั่ง UI (หลัง mutex) และ connection ของ worker; ใช้ WAL + `busy_timeout` 15 วินาที
- **ยังไม่ทำ:** สำรองก่อน migrate (ยังมี schema รุ่นเดียว), checksum ของ migration

## 5. ตัวตนของไฟล์

`photo_locations.path_key` คือ path จาก `std::fs::canonicalize` ไม่ lowercase และไม่ normalize Unicode ทับ (หมวด 7.4)
path เดิมที่พบผ่าน parent หรือ child ใช้ `photo_id` เดิม สำเนาเหมือนกันทุก byte ที่คนละ path เป็นคนละ `photo_id` และมี SHA-256 เดียวกัน

**ยังไม่ทำ:** `volume_key` / filesystem file ID สำหรับตามไฟล์ที่ถูกเปลี่ยนชื่อ และ relink (Phase 3)
ไฟล์ที่หายถูกทำเครื่องหมาย `available = 0` ไม่ลบ

## 6. สิทธิ์ไฟล์บน macOS

รุ่น Python แจกนอก App Store ด้วย Developer ID และไม่ใช้ sandbox รุ่นนี้เสนอแบบเดียวกัน: path ที่ผู้ใช้เลือกผ่าน native picker
ถูกเก็บเป็น path ธรรมดา ไม่ใช้ security-scoped bookmark

ผลที่ตามมา: ถ้าภายหลังต้องเข้า App Store ต้องเพิ่ม bookmark ใน `platform` และทดสอบสิทธิ์หลังเปิดแอปใหม่
macOS อาจถามสิทธิ์เข้าโฟลเดอร์ Desktop/Documents/ไดรฟ์ภายนอกครั้งแรก — **ยังไม่ได้ทดสอบกับ build ที่เซ็นแล้ว**

## 7. การเสิร์ฟรูป

`puri://localhost/{thumb|photo|face}/{id}` และ `draft/{previewId}/{draftId}` — Rust แปลง ID เป็นไฟล์เอง
ไม่เปิด asset scope ของ Tauri และไม่ให้สิทธิ์ `fs` กับ web view ตัวดูภาพโหลด bytes ของต้นฉบับ (ไม่ re-encode)
และ WebView หมุนตาม EXIF เอง จึงใช้ transform เดียวกับพิกัดกรอบใบหน้า (สัดส่วนบนภาพที่หมุนแล้ว)

## 8. ถอดรหัสรูปและภาพย่อ

crate `image` 0.25 เปิดเฉพาะ JPEG และ PNG; UI รายงานชนิดอื่นว่า "ยังไม่รองรับ" พร้อมจำนวน ไม่ทำให้หายเงียบ

ผลวัดจริง (debug build ที่ dependency ใช้ `opt-level = 3`, MacBook Apple M4 / 24 GB, macOS 26.7, SSD ในเครื่อง, 5 threads):
**292 รูป JPEG 8096×5397 ใน 34.9 วินาที = 8.4 รูป/วินาที** รวม hash SHA-256, EXIF และภาพย่อ 512px ไม่มีรูปผิดพลาด
คลัง 19,687 รูปของผู้ใช้จะใช้ราว 40 นาที (ประมาณจากอัตรานี้ ยังไม่ได้รันทั้งคลัง)

แผน: adapter `PhotoDecoder` ที่เรียก ImageIO ของ macOS ซึ่งย่อภาพระหว่างถอดรหัสและอ่าน HEIC/RAW ได้ —
**ยังไม่ได้ทดลอง** จึงยังไม่ประกาศรองรับ HEIC/RAW (ผู้ใช้มี RAW 317 ไฟล์ใน `~/Photos/2569`)

## 9. โมเดลใบหน้า — ยังไม่เลือก

ข้อเท็จจริง ([LEGACY_AUDIT.md](LEGACY_AUDIT.md) หมวด 3.3–3.4):

- แอปเดิมใช้ `buffalo_sc` ของ InsightFace ผ่าน ONNX Runtime; ไฟล์ ONNX สองไฟล์อยู่ในเครื่องแล้ว
- โมเดลชุดนี้ระบุว่าใช้ได้เฉพาะ "non-commercial research" และไม่พบข้อความอนุญาตให้แจกจ่ายต่อ
- ถ้าใช้โมเดลเดิม มีโอกาสใช้เวกเตอร์ 46,375 ตัวของฐานเดิมต่อได้ (ต้องพิสูจน์); ถ้าเปลี่ยนโมเดลต้องวิเคราะห์รูปทั้งหมดใหม่

ทางเลือก:

| ทาง | ข้อดี | ข้อเสีย |
|---|---|---|
| A. ใช้ `buffalo_sc` ต่อ โดยไม่รวมใน installer ผู้ใช้ดาวน์โหลด/ชี้ไฟล์เอง | มีโอกาสใช้ข้อมูลเดิมต่อ; ไม่แจกไฟล์โมเดลเอง | ผู้ใช้ยังอยู่ใต้เงื่อนไข non-commercial; ต้องมีหน้าติดตั้งโมเดล |
| B. ขออนุญาตจาก InsightFace | ชัดเจนที่สุด | ต้องติดต่อและรอ |
| C. เปลี่ยนเป็นโมเดลที่สัญญาอนุญาตให้แจกได้ | แจกใน installer ได้ | ต้องหาและประเมินความแม่นยำใหม่; วิเคราะห์รูปใหม่ทั้งหมด |

ชุดงานนี้ทำเส้นทาง "ระบุใบหน้าเอง" ให้ครบก่อน และ `get_capabilities` รายงาน `faceAnalysis.available = false` ตามจริง
ONNX Runtime ผ่าน Rust binding (D07) **ยังไม่ได้ทดลอง**

## 10. การทดสอบแอปจริง

WKWebView ไม่มี WebDriver จึงใช้วิธีของ PuriCat (ADR 0012): debug build อ่าน `PURI_E2E_SCRIPT` แล้วฉีดสคริปต์เข้าในหน้าต่างจริง
สคริปต์ขับ DOM และรายงานผ่านคำสั่ง `e2e_report`; `PURI_E2E_PICK` เตรียมคำตอบของ native folder picker; `PURI_DATA_DIR`
ชี้ไปคลังทิ้งได้ release build ไม่มีโค้ดส่วนนี้ (`#[cfg(debug_assertions)]`)

## 11. Dev bridge

`crates/devbridge` เปิด `api::dispatch` ตัวเดียวกันผ่าน HTTP ที่ `127.0.0.1:1421` (CORS เฉพาะ `http://localhost:1420`)
ให้ UI ที่รันด้วย `pnpm dev` ในเบราว์เซอร์ทำงานกับ Rust และคลังทดสอบจริง จึงไม่ต้องมี mock data ฝั่ง UI
เป็น binary แยกที่ `src-tauri` ไม่ได้อ้างถึง จึงไม่อยู่ในแอปที่แจก ในโหมดนี้ไม่มี native picker: UI ใช้ช่องพิมพ์ path ที่ติดป้าย "โหมดพัฒนา"

## 12. แพลตฟอร์ม

build และทดสอบบน macOS 26.7 / Apple M4 (arm64) / Rust 1.99.1 เท่านั้น
`platform.rs` มีโค้ดของ Windows/Linux สำหรับ "เปิด/แสดงในตัวจัดการไฟล์" และ `client.ts` มี URL ของ protocol สำหรับ Windows
**ทั้งสองส่วนไม่เคยถูกรัน** จึงไม่อ้างว่ารองรับ Windows หรือ Intel Mac

## เรื่องย่อยอื่น

- **ID:** UUIDv7 แบบข้อความทุกตาราง (เรียงตามเวลาได้ และไม่ชนเมื่อรวมข้อมูลจากฐานเดิม)
- **วันที่จากชื่อโฟลเดอร์:** `YYYY-MM-DD` / `YYYYMMDD` เสนอเมื่อเป็นวันจริง (ปี 2400–2699 ถือเป็น พ.ศ.); `YYMMDD` เสนอเฉพาะเมื่อชื่อโฟลเดอร์แม่
  เป็นปีสี่หลักที่ลงท้ายตรงกัน มิฉะนั้นไม่เสนอ (หมวด 2.3)
- **ค้นข้อความ:** `LIKE` แบบ substring บนคีย์ที่ NFC + ตัวพิมพ์เล็ก พร้อม escape `%` `_` — ยังไม่ใช้ FTS5 เพราะคำไทยสั้นกว่าสามตัวอักษร
  ใช้ trigram ไม่ได้ และขนาดคลังปัจจุบันยังไม่ต้องการ (ยังไม่ได้วัดกับ 10,000+ รูป)
- **หน้าถัดไปของผลค้น:** cursor เป็น offset บนลำดับที่มี ID เป็นตัวตัดสิน — ยังไม่ผูกกับ data revision (หมวด 11.5); ถ้ารูปเพิ่มระหว่างเลื่อน
  อาจเห็นรายการเลื่อนตำแหน่ง ส่วน "เลือกผลค้นหาทั้งหมด" ใช้ snapshot ที่แช่แข็งแล้ว
- **ไอคอนแอป:** ใช้ `resources/appLogo.png` ของแอปเดิมชั่วคราว ยังไม่ใช่โลโก้รูรับแสงตามภาพ S05
- **ปิดหน้าต่าง = ออกจากแอป** ตามค่าเริ่มต้นของ Tauri; การแยกปิดหน้าต่างกับ Quit ตามหมวด 2.4 ยังไม่ทำ
