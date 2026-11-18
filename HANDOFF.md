# HANDOFF — Puri Photo Search 2

เขียนเมื่อ 18 พ.ย. 2569 ตอนส่งต่องานกลางชุด "วิเคราะห์ใบหน้าอัตโนมัติ" อ่านไฟล์นี้ก่อน แล้วอ่าน `AGENTS.md` และ `docs/IMPLEMENTATION_STATUS.md`
ลบไฟล์นี้เมื่อชุดงานนี้ merge แล้ว

## อยู่ตรงไหน

| | สถานะ |
|---|---|
| `main` (GitHub: purivaro/PuriPhotoSearch, public) | รุ่น 2.0.0-alpha.1 เผยแพร่แล้วเป็น pre-release พร้อม DMG ที่เซ็น + notarize (ยังไม่มี AI ใบหน้า) |
| branch **`feat/face-analysis`** (งานปัจจุบัน) | Rust ฝั่งใบหน้า commit แล้ว (`46a303c`); UI ของชุดนี้ commit เป็น WIP ต่อท้าย — **ยังไม่ได้เปิด PR** |

คำสั่งของหลวงพี่ในชุดนี้ (ตามลำดับ): ต่อการตรวจใบหน้าเข้าแอป → "ใช้โมเดลที่แม่นที่สุด" → "ไม่มีโมเดลในตัวติดตั้งต้องไม่ยากสำหรับผู้ใช้ ทำให้ง่ายที่สุด"
→ "ให้ GitHub เซ็นให้อัตโนมัติ" รายละเอียดการตัดสินใจอยู่ใน `docs/DECISIONS.md` ข้อ 9

## ที่เสร็จและทดสอบแล้วใน branch นี้ (Rust)

- โมเดล InsightFace `buffalo_l` ผ่าน ONNX Runtime (`crates/faces`), interface อยู่ใน `crates/core/src/ai.rs`
- ติดตั้งโมเดลด้วยคำสั่งเดียว `install_face_model` (ดาวน์โหลด → แตกไฟล์ → ตรวจ SHA-256 → วิเคราะห์รูปที่มีต่อเอง)
- วิเคราะห์ใบหน้าในงานเบื้องหลังเดียวกับการสร้างภาพย่อ (`crates/core/src/jobs.rs`)
- ข้อเสนอชื่อ / กลุ่มยังไม่ทราบชื่อ / ปฏิเสธข้อเสนอ (`crates/core/src/review.rs`), schema v2 (`migrations/0002_faces.sql`)
- นำรายชื่อ กลุ่ม ใบหน้าตัวอย่างจากฐานรุ่น Python (`legacy::import_people`) — ไม่ใช้เวกเตอร์เดิม
- เทสต์: `cargo test --workspace` ผ่าน 38 ข้อ (core 6 + workflow 23 + faces_ai 6 + faces 2 + tauri 1), clippy สะอาด
- รันจริงผ่าน dev bridge กับรูปจริง 292 รูป: 1,096 ใบหน้า, 2.4 รูป/วินาที; นำเข้า 105 คนจากฐานจริงแล้วได้ข้อเสนอ 304 ใบ — ตัวเลขอยู่ใน `docs/TEST_REPORT.md`
- `.github/workflows/release.yml` + `scripts/setup-release-secrets.sh` (ยังไม่เคยรันทั้งคู่)

## ที่ค้าง — ทำต่อจากตรงนี้

1. **UI ของชุดนี้ทำครบ 9 ข้อแล้วแต่ session หลักยังไม่ได้ตรวจรับ** agent ฝั่ง UI รายงานว่า `pnpm typecheck` ผ่าน, `pnpm test` 41 ข้อผ่าน,
   `pnpm build` ผ่าน และเปิดดูกับ bridge จริงแล้วบางหน้า (`/review` สองแท็บ, ข้อเสนอในตัวดูภาพ, ตั้งค่าแท็บ AI, หน้าหลัก) — ผมยังไม่ได้อ่าน diff
   หรือรันซ้ำเอง รายการ testid และสิ่งที่ยังไม่ได้ดูด้วยตาอยู่ใน `docs/FRONTEND_NOTES.md` (ยังไม่ได้ดู: ปุ่มดาวน์โหลดโมเดลทุกสถานะ,
   การกด ใช่/ไม่ใช่/ไม่ใช่ใบหน้า ผ่านปุ่มจริง, ผลนำรายชื่อเข้า, ธีมมืด, หน้าต่างแคบ)
   - **บั๊กฝั่ง Rust ที่ agent พบ (ต้องแก้ก่อนใช้จริง):** `reviewRevision` (= `identity_revision`) เพิ่มทุกครั้งที่รูปหนึ่งวิเคราะห์เสร็จ
     (`ai::save_analysis` เรียก `bump_identity`) ระหว่างงานวิเคราะห์วิ่ง `get_review_faces` จึงตอบ `PREVIEW_STALE` แทบทุกครั้ง หน้า S16 ใช้ไม่ได้
     จนกว่างานจบ ทางแก้ที่เสนอ: ให้ cache ใน `review.rs` เสิร์ฟ snapshot ของ revision ที่ออกไปแล้ว หรือแยกตัวนับ "การตัดสินของผู้ใช้" ออกจาก
     "มีใบหน้าใหม่" แล้วให้ `get_review_faces` ตรวจเฉพาะตัวแรก — เพิ่มเทสต์ใน `faces_ai.rs` ด้วย
   - ครอป `face/{id}` สร้างตอนถูกขอ (ถอดรหัสรูปเต็ม) จึงโหลดช้าในคิวตรวจสอบ: ควรสร้างครอปตอนวิเคราะห์ (ใน `prepare_photo`) หรือครอปจากภาพที่ย่อไว้
2. **e2e**: `e2e/run.sh` ตั้ง `PURI_DISABLE_FACE_ENGINE=1` แล้ว แต่ยังไม่ได้รันหลัง UI เปลี่ยน — `e2e/smoke.js` อาจต้องแก้ (เช่นบรรทัดที่คาดว่า
   สวิตช์ `import-analyze-faces` ปิดใช้งาน และ `caps.faceAnalysis.available === false`)
3. **ยังไม่ได้ทดสอบการดาวน์โหลดโมเดลผ่านเครือข่ายจากในแอปจริง** (ทดสอบผ่าน `file://` เท่านั้น) และยังไม่ได้เปิดแอป Tauri จริงกับ ONNX Runtime
4. **ยังไม่ได้ดูด้วยตาว่าข้อเสนอชื่อถูกแค่ไหน** — เปิดหน้าตรวจสอบกับคลังทดสอบแล้วไล่ดูก่อนบอกหลวงพี่ว่าใช้ได้
5. ความเร็ว 2.4 รูป/วินาที (คลัง 19,687 รูป ≈ 2.3 ชม.): กำลังจะวัดแยกขั้นด้วย `crates/faces/examples/bench.rs` (เขียนแล้ว **ยังไม่ได้รัน**)
   ทางเร่งที่คิดไว้: CoreML execution provider, รวมหลายใบหน้าต่อการรัน — ยังไม่ได้ลอง
6. เอกสาร: `docs/TEST_REPORT.md` และ `docs/IMPLEMENTATION_STATUS.md` อัปเดตฝั่ง Rust แล้ว ต้องเติมผล UI/e2e หลังข้อ 1–2
7. เปิด PR จาก `feat/face-analysis` → `main` เป็นงานใหญ่ (เปลี่ยน schema + เพิ่ม AI): **ให้หลวงพี่ดูก่อน merge** แล้วค่อยออกรุ่น alpha.2
   ด้วย `scripts/release-mac.sh` (หรือ workflow เมื่อมี secret)
8. **หลวงพี่ต้องรันเอง** (ผมไม่แตะรหัส/ใบรับรอง): `scripts/setup-release-secrets.sh` หนึ่งครั้ง เพื่อให้ GitHub Actions เซ็น + notarize ได้

## สิ่งที่ต้องรู้ / กับดัก

- PATH ของ Rust: `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`
- ไฟล์โมเดลสำหรับพัฒนาอยู่ในโฟลเดอร์ชั่วคราวของ session เดิม (หายได้เมื่อรีบูต) ดาวน์โหลดใหม่:
  `https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip` (288,621,354 bytes) แล้วตั้ง `PURI_MODEL_URL=file:///path/buffalo_l.zip`
  เพื่อไม่ต้องให้แอปดาวน์โหลดซ้ำตอนทดสอบ
- dev bridge: `PURI_DATA_DIR=<โฟลเดอร์ทิ้งได้> PURI_DISABLE_OPEN=1 PURI_MODEL_URL=file://… cargo run -p puri-devbridge` แล้ว `pnpm dev`
  (ตอนส่งต่อมี bridge ค้างอยู่หนึ่งตัวที่ port 1421 — `pkill -f puri-devbridge` ได้)
- Mac Intel ไม่มี ONNX Runtime สำเร็จรูป → build นั้นไม่มีตัววิเคราะห์ใบหน้า (cfg ใน `crates/faces`) — compile ผ่าน ยังไม่เคยรัน
- `minimumSystemVersion` ขยับเป็น 13.4 แล้วใน `src-tauri/tauri.conf.json` (ข้อกำหนดของ ONNX Runtime)
- **ห้าม** เปิดแอปกับคลังจริงของหลวงพี่ (`~/Library/Application Support/com.purivaro.puriphotosearch2/`) โดยไม่ได้รับคำสั่ง — ใช้ `PURI_DATA_DIR`
  (debug) หรือ `HOME=<โฟลเดอร์ชั่วคราว>` (release) และ **ห้ามเขียน** ลง `~/Library/Application Support/PuriPhotoSearch/` (ฐานรุ่น Python)
- เซ็น/notarize ในเครื่อง: identity `Developer ID Application: Phra Anavach Purivaro (7ZFZ2CSF6M)`, โปรไฟล์ notarytool `puri-notary`
- คำถามที่ยังไม่ได้ถามหลวงพี่: จะให้รัน `codegraph init -i` ไหม (กติกาใน `~/.claude/CLAUDE.md`)
