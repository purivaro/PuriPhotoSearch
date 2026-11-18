# HANDOFF — Puri Photo Search 2

อัปเดต 18 พ.ย. 2569 (รอบสองของชุด "วิเคราะห์ใบหน้าอัตโนมัติ") อ่านไฟล์นี้ก่อน แล้วอ่าน `AGENTS.md` และ `docs/IMPLEMENTATION_STATUS.md`
ลบไฟล์นี้เมื่อชุดงานนี้ merge แล้ว

## อยู่ตรงไหน

| | สถานะ |
|---|---|
| `main` (GitHub: purivaro/PuriPhotoSearch, public) | รุ่น 2.0.0-alpha.1 เผยแพร่แล้วเป็น pre-release (ยังไม่มี AI ใบหน้า); โลโก้/ไอคอนใหม่ (น้องโพลารอยด์) merge แล้วใน PR #5 |
| branch **`feat/face-analysis`** | Rust + UI ของชุดใบหน้าครบ ตรวจรับแล้ว เปิดเป็น Draft PR #4 รอหลวงพี่ดู (ยังไม่มีโลโก้ใหม่จนกว่าจะรวมกับ `main`) |

## ที่ทำในรอบสอง (รายละเอียดและตัวเลขอยู่ใน `docs/TEST_REPORT.md` หมวดวิเคราะห์ใบหน้า)

- แก้ `PREVIEW_STALE` ระหว่างงานวิเคราะห์วิ่ง (แยก `decision_revision`), ครอปใบหน้าทำตอนวิเคราะห์ — มีเทสต์ทั้งสองเรื่อง
- ตรวจรับ UI ด้วยการกดจริงกับรูปจริง 292 รูป: ใช่/ไม่ใช่/ตั้งชื่อ/ไม่ใช่ใบหน้า, ข้อเสนอในตัวดูภาพ, นำรายชื่อเข้า, ดาวน์โหลดโมเดลผ่านเครือข่ายจริง
  (ยกเลิก/ลองใหม่/พร้อมใช้), ธีมมืด/สว่าง, หน้าต่างแคบ — เจอและแก้บั๊ก Enter ในช่องเลือกบุคคล
- ความเร็วใน release: 11.6 รูป/วินาที (ตัวเลข 2.4 เดิมเป็น debug build) — ไม่ต้องทำ CoreML ตอนนี้
- e2e ผ่าน 14 ขั้น; ตัววิเคราะห์จริงทำงานในหน้าต่าง Tauri จริง (debug) แล้ว
- ไล่ดูข้อเสนอชื่อด้วยตา: กลุ่มใหญ่ตรง; ข้อเสนอผิดมาจากใบหน้าที่ตัวตรวจให้คะแนน 0.5–0.6

## ที่ค้าง

1. ~~กรองใบหน้าคะแนนตัวตรวจต่ำกว่า 0.6~~ — หลวงพี่ตัดสินให้กรอง ทำแล้ว (`MIN_DETECTION_SCORE` ใน `review.rs`, มีเทสต์, วัดกับรูปจริงแล้ว)
2. **รอหลวงพี่ดู PR** แล้วค่อย merge และออกรุ่น alpha.2 ด้วย `scripts/release-mac.sh` (หรือ workflow เมื่อมี secret) — งานใหญ่ เปลี่ยน schema
3. **หลวงพี่ต้องรันเอง** (ผมไม่แตะรหัส/ใบรับรอง): `scripts/setup-release-secrets.sh` หนึ่งครั้ง เพื่อให้ GitHub Actions เซ็น + notarize ได้
   (`.github/workflows/release.yml` ยังไม่เคยรัน)
4. ยังไม่ได้ทดสอบ: ตัววิเคราะห์ใน release build ที่เซ็นแล้ว, e2e แบบ `.app` หลัง UI ชุดนี้, คลังใหญ่ทั้ง 19,687 รูป, Mac Intel
5. ขั้นวิเคราะห์ใบหน้ายังไม่อยู่ใน e2e อัตโนมัติ (ต้องมีวิธีใช้โมเดลจริงโดยไม่ดาวน์โหลด 289 MB ทุกรอบ)

## สิ่งที่ต้องรู้ / กับดัก

- PATH ของ Rust: `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`
- **วัดความเร็วด้วย release เท่านั้น**: `cargo build --release -p puri-devbridge` — debug ช้ากว่าราว 5 เท่าในขั้นใบหน้า
- ไฟล์โมเดลสำหรับพัฒนา: ดาวน์โหลด `https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip` (288,621,354 bytes)
  แล้วตั้ง `PURI_MODEL_URL=file:///path/buffalo_l.zip` เพื่อไม่ต้องให้แอปดาวน์โหลดซ้ำตอนทดสอบ
- dev bridge: `PURI_DATA_DIR=<โฟลเดอร์ทิ้งได้> PURI_DISABLE_OPEN=1 PURI_MODEL_URL=file://… target/release/puri-devbridge` แล้ว `pnpm dev`
  (port 1421 ตายตัว: `pkill -f puri-devbridge` ตัวเก่าและรอให้ port ว่างก่อนเปิดตัวใหม่)
- ทดสอบตัววิเคราะห์จริงในหน้าต่าง Tauri: ตั้ง `PURI_E2E_SCRIPT` เป็นสคริปต์ของตัวเองแล้วรัน `pnpm tauri dev` (อย่าตั้ง `PURI_DISABLE_FACE_ENGINE`)
- Mac Intel ไม่มี ONNX Runtime สำเร็จรูป → build นั้นไม่มีตัววิเคราะห์ใบหน้า (cfg ใน `crates/faces`) — compile ผ่าน ยังไม่เคยรัน
- **ห้าม** เปิดแอปกับคลังจริงของหลวงพี่ (`~/Library/Application Support/com.purivaro.puriphotosearch2/`) โดยไม่ได้รับคำสั่ง — ใช้ `PURI_DATA_DIR`
  (debug) หรือ `HOME=<โฟลเดอร์ชั่วคราว>` (release) และ **ห้ามเขียน** ลง `~/Library/Application Support/PuriPhotoSearch/` (ฐานรุ่น Python)
- เซ็น/notarize ในเครื่อง: identity `Developer ID Application: Phra Anavach Purivaro (7ZFZ2CSF6M)`, โปรไฟล์ notarytool `puri-notary`
- คำถามที่ยังไม่ได้ถามหลวงพี่: จะให้รัน `codegraph init -i` ไหม (กติกาใน `~/.claude/CLAUDE.md`)
