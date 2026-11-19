<img src="src-tauri/icons/128x128@2x.png" width="96" alt="โลโก้ Puri Photo Search">

# Puri Photo Search 2

## ดาวน์โหลด — รุ่นทดลอง 2.0.0-alpha.3

| ระบบ | ดาวน์โหลด | ควรรู้ก่อนติดตั้ง |
|---|---|---|
| **macOS** 13.4 ขึ้นไป (ชิป Apple และ Intel) | **[⬇ PuriPhotoSearch-mac.dmg](https://github.com/purivaro/PuriPhotoSearch/releases/download/v2.0.0-alpha.3/PuriPhotoSearch-mac.dmg)** (19 MB) | เซ็นและผ่านการตรวจของ Apple แล้ว เปิดติดตั้งได้ตามปกติ |
| **Windows** 10 / 11 (64-bit) | **[⬇ PuriPhotoSearch-windows-setup.exe](https://github.com/purivaro/PuriPhotoSearch/releases/download/v2.0.0-alpha.3/PuriPhotoSearch-windows-setup.exe)** (9 MB) | ยังไม่ได้เซ็นและยังไม่เคยลองบนเครื่องจริง — เมื่อ SmartScreen เตือน กด "More info" แล้ว "Run anyway" |

วิธีติดตั้งอยู่[ด้านล่าง](#ติดตั้ง-macos) · รุ่นอื่นและ `SHA256SUMS.txt` อยู่ที่หน้า [Releases](https://github.com/purivaro/PuriPhotoSearch/releases)

แอป desktop สำหรับคลังภาพกิจกรรม: เพิ่มโฟลเดอร์งานที่มีอยู่แล้วในเครื่อง ตั้งชื่อคนจากใบหน้า ค้นรูปข้ามกิจกรรม และส่งออกสำเนา
รูปต้นฉบับอยู่ที่เดิมเสมอ แอปเก็บข้อมูลสำหรับค้นหาไว้แยกต่างหากในเครื่อง ไม่มีบัญชี ไม่มีการส่งข้อมูลออก

เขียนใหม่ด้วย Rust + Tauri 2 + React แทนรุ่น Python (PuriPhotoSearch 1.9.3)

> **สถานะ: รุ่นทดลอง (2.0.0-alpha.3) ยังไม่พร้อมใช้กับงานจริง**
> วิเคราะห์ใบหน้าอัตโนมัติใช้ได้บน Mac ชิป Apple และ Windows 64-bit (กดดาวน์โหลดตัววิเคราะห์ครั้งเดียว ~290 MB) AI จัดรูปให้บุคคลเอง ผู้ใช้แจ้งเฉพาะรูปที่ไม่ใช่
> สิ่งที่ทำแล้ว ทดสอบแล้ว และยังไม่ได้ทำ ดูที่ [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)

## ติดตั้ง (macOS)

1. ดาวน์โหลด `PuriPhotoSearch-mac.dmg` จากลิงก์ด้านบน
2. เปิดไฟล์ แล้วลาก **Puri Photo Search** ไปไว้ใน Applications
3. เปิดแอปได้ตามปกติ — ตัวติดตั้งเซ็นด้วย Developer ID และผ่านการ notarize ของ Apple แล้ว จึงไม่ต้องปลดล็อกใน System Settings

ต้องการ macOS 13.4 ขึ้นไป (ข้อกำหนดของตัววิเคราะห์ใบหน้า; รุ่น 2.0.0-alpha.1 ใช้กับ macOS 12 ได้)
ไฟล์เป็น universal (Apple Silicon + Intel — บน Intel ไม่มีการวิเคราะห์ใบหน้าอัตโนมัติ) **ทดสอบจริงเฉพาะ macOS 26.7 บน Apple Silicon**
ไม่ต้องติดตั้ง Python, Node หรือ Rust ตรวจไฟล์ที่ดาวน์โหลดได้ด้วย `shasum -a 256 PuriPhotoSearch-mac.dmg` เทียบกับ `SHA256SUMS.txt`

รุ่นนี้เป็นคนละแอปกับ PuriPhotoSearch 1.x (Python): ติดตั้งคู่กันได้ และไม่แตะข้อมูลของรุ่นเดิม

## ติดตั้ง (Windows) — ยังไม่เคยลองบนเครื่องจริง

ตั้งแต่รุ่น 2.0.0-alpha.3 มี `PuriPhotoSearch-windows-setup.exe` (ลิงก์ด้านบน) สำหรับ Windows 10 รุ่น 1803 ขึ้นไป หรือ Windows 11, 64-bit

1. ดาวน์โหลดแล้วเปิดไฟล์ ตัวติดตั้ง**ไม่ได้เซ็น** (ยังไม่มีใบรับรองของ Windows) SmartScreen จะเตือนว่าไม่รู้จักผู้เผยแพร่ — กด "More info" แล้ว "Run anyway"
2. ติดตั้งเฉพาะผู้ใช้ปัจจุบัน ไม่ต้องใช้สิทธิ์ผู้ดูแลเครื่อง ถ้าเครื่องยังไม่มี WebView2 ตัวติดตั้งจะดาวน์โหลดให้

**หลักฐานทั้งหมดของรุ่น Windows มาจากเครื่อง runner ของ GitHub** (Windows Server 2025): เทสต์ Rust ทั้งชุด, เส้นทางหลักในหน้าต่าง WebView2 จริง
และตัววิเคราะห์ใบหน้ากับโมเดลจริง ยังไม่มีใครติดตั้งและใช้บนเครื่อง Windows จริง — รายละเอียดใน [docs/TEST_REPORT.md](docs/TEST_REPORT.md)
การนำรายชื่อจากรุ่น Python เข้ายังใช้ได้เฉพาะ macOS (ไม่ทราบตำแหน่งข้อมูลของรุ่นเดิมบน Windows)

## เอกสาร

| ไฟล์ | เนื้อหา |
|---|---|
| [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) | หน้าจอ S01–S22 และข้อกำหนด → ทำแล้ว / บางส่วน / ยังไม่ทำ พร้อมหลักฐาน |
| [docs/TEST_REPORT.md](docs/TEST_REPORT.md) | สิ่งที่รันจริง ผล และสิ่งที่ยังไม่ได้รัน |
| [docs/LEGACY_AUDIT.md](docs/LEGACY_AUDIT.md) | ผลตรวจรุ่น Python และฐานข้อมูลเดิม |
| [docs/DECISIONS.md](docs/DECISIONS.md) | การตัดสินใจด้านสถาปัตยกรรมและเหตุผล |
| [docs/DATA_MODEL.md](docs/DATA_MODEL.md) | schema, กติกาข้อมูล, ตำแหน่งข้อมูลจริง |
| [docs/MIGRATION.md](docs/MIGRATION.md) | แผนย้ายข้อมูลจากรุ่น Python (ตอนนี้ทำได้แค่ตรวจแบบอ่านอย่างเดียว) |
| [AGENTS.md](AGENTS.md) | กติกาสำหรับผู้พัฒนา |

## สิ่งที่ต้องมีเพื่อพัฒนา

- macOS (ทดสอบบน macOS 26.7, Apple Silicon เท่านั้น) พร้อม Xcode Command Line Tools — หรือ Windows พร้อม Visual Studio Build Tools (build และทดสอบผ่าน CI เท่านั้น)
- Rust stable ≥ 1.89 (ทดสอบด้วย 1.99.1) — เครื่องนี้ติดตั้ง rustup ผ่าน Homebrew จึงต้อง
  `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`
- Node ≥ 22 และ pnpm 10

ผู้ใช้ปลายทางไม่ต้องติดตั้งสิ่งเหล่านี้

## รัน

```sh
pnpm install
pnpm tauri dev                 # แอปจริง ใช้คลังจริงของเครื่อง
PURI_DATA_DIR=/tmp/puri-test pnpm tauri dev   # แอปจริง กับคลังทิ้งได้ (debug build เท่านั้น)
```

พัฒนา UI ในเบราว์เซอร์กับ Rust จริง (ไม่มี mock data):

```sh
cargo run -p puri-devbridge    # http://127.0.0.1:1421 — คลังทดสอบที่ target/dev-library
pnpm dev                       # http://localhost:1420
```

โฟลเดอร์รูปทดสอบ (รูปที่สร้างขึ้น มีงานลงวันที่ รูปแนวตั้ง ไฟล์เสีย และไฟล์ที่ยังไม่รองรับ):

```sh
cargo run -p puri-core --example make_fixtures -- /tmp/puri-photos
```

## ทดสอบ

```sh
pnpm check                     # typecheck + vitest + cargo fmt --check + clippy -D warnings + cargo test
e2e/run.sh                     # เส้นทางหลักในหน้าต่างแอปจริง (ต้องมี desktop session)
```

## สร้างตัวติดตั้ง

```sh
APPLE_SIGNING_IDENTITY="Developer ID Application: …" PURI_NOTARY_PROFILE=<โปรไฟล์ของ notarytool> scripts/release-mac.sh
```

ได้ `target/release-download/PuriPhotoSearch-mac.dmg` (universal, เซ็น + notarize + staple) และ `SHA256SUMS.txt`
ใบรับรองและรหัสอยู่ใน keychain ของเครื่องที่ build สคริปต์ไม่อ่านหรือเขียนความลับ

ให้ GitHub Actions ทำแทน: เจ้าของใบรับรองรัน `scripts/setup-release-secrets.sh` หนึ่งครั้ง (ส่งใบรับรองและรหัส notarize ขึ้นเป็น secret ของ repo)
จากนั้น push tag `v*` แล้ว workflow `Release` จะ build + เซ็น + notarize และแนบตัวติดตั้งกับ release ให้ ถ้ายังไม่มี secret
workflow จะได้แอปที่เซ็นแบบ ad-hoc เท่านั้น ซึ่งเครื่องอื่นเปิดตามปกติไม่ได้

ลิงก์ดาวน์โหลดบนสุดของไฟล์นี้ชี้ไปที่ tag ของรุ่นล่าสุดโดยตรง (ทุกรุ่นยังเป็น pre-release ลิงก์ `releases/latest` ของ GitHub จึงใช้ไม่ได้) — **แก้เลขรุ่นในลิงก์ทุกครั้งที่ออกรุ่นใหม่**

## ข้อมูลอยู่ที่ไหน

| อะไร | ตำแหน่ง |
|---|---|
| คลัง (ชื่อคน, คำยืนยัน, กิจกรรม) | `~/Library/Application Support/com.purivaro.puriphotosearch2/library/` |
| ภาพย่อและครอปใบหน้า (ลบได้ สร้างใหม่ได้) | `~/Library/Caches/com.purivaro.puriphotosearch2/` |
| คลัง / ภาพย่อ บน Windows | `%APPDATA%\com.purivaro.puriphotosearch2\library\` / `%LOCALAPPDATA%\com.purivaro.puriphotosearch2\` |
| รูปต้นฉบับ | โฟลเดอร์เดิมของผู้ใช้ — แอปอ่านอย่างเดียว |
| ข้อมูลของรุ่น Python | `~/Library/Application Support/PuriPhotoSearch/` — รุ่นนี้ไม่เขียนลงที่นี่ |

ยังไม่มีคำสั่งสำรองข้อมูลในแอป วิธีสำรองด้วยมืออยู่ท้าย [docs/DATA_MODEL.md](docs/DATA_MODEL.md)

## โครง

```
crates/core      กติกาข้อมูลทั้งหมด (ไม่ขึ้นกับ Tauri) + เทสต์กับไฟล์จริง
crates/devbridge เครื่องมือพัฒนา: คำสั่งชุดเดียวกันผ่าน HTTP ในเครื่อง
crates/faces     ตัววิเคราะห์ใบหน้า (ONNX Runtime + โมเดล buffalo_l ที่แอปดาวน์โหลดเอง)
src-tauri        เปลือก Tauri: ลงทะเบียนคำสั่ง, protocol puri://, native picker
src              React UI
e2e              ทดสอบแอปจริงด้วยสคริปต์ที่ฉีดเข้า debug build
```
