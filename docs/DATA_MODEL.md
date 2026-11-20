# แบบจำลองข้อมูล

Schema รุ่น 2 — ต้นฉบับคือ [`0001_init.sql`](../crates/core/migrations/0001_init.sql) และ [`0002_faces.sql`](../crates/core/migrations/0002_faces.sql)
คลังของรุ่น 2.0.0-alpha.1 (schema 1) ถูกอัปเกรดในที่เมื่อเปิดด้วยรุ่นใหม่ (มีเทสต์ `library_from_v1_upgrades_in_place`)
เอกสารนี้อธิบายกติกาที่ schema บังคับ และส่วนของ Blueprint หมวด 8 ที่ยังไม่มีในรุ่นนี้

## ที่เก็บ

| อะไร | ที่ไหน (macOS) | สร้างใหม่ได้ |
|---|---|---|
| ฐานข้อมูลคลัง | `~/Library/Application Support/com.purivaro.puriphotosearch2/library/library.sqlite` (+ `-wal`, `-shm`) | ไม่ได้ — ข้อมูลหลัก |
| ไฟล์ lock | `…/library/library.lock` | — |
| ภาพย่อ | `~/Library/Caches/com.purivaro.puriphotosearch2/thumbs/<2 ตัวท้ายของ ID>/<revision id>.jpg` | ได้ เมื่อต้นทางพร้อม |
| ครอปใบหน้า | `~/Library/Caches/com.purivaro.puriphotosearch2/faces/<face id>.jpg` | ได้ เมื่อต้นทางพร้อม |
| ไฟล์โมเดลใบหน้า | `~/Library/Application Support/com.purivaro.puriphotosearch2/library/models/buffalo_l/` (`det_10g.onnx`, `w600k_r50.onnx`, `verified`) | ได้ — ดาวน์โหลดใหม่ |
| ไฟล์รูปต้นฉบับ | อยู่ที่เดิมของผู้ใช้ แอปอ่านอย่างเดียว — ยกเว้นเมื่อผู้ใช้สั่ง "ย่อไฟล์ต้นฉบับ" (DECISIONS ข้อ 15): แทนด้วยไฟล์ที่ย่อ ต้นฉบับไปถังขยะ | — |

แยกจากข้อมูลของรุ่น Python (`~/Library/Application Support/PuriPhotoSearch/`) ทั้งสองรุ่นไม่เขียนไฟล์เดียวกัน
debug build ที่ตั้ง `PURI_DATA_DIR` จะใช้โฟลเดอร์นั้นแทน

## ตาราง

| กลุ่ม | ตาราง | หน้าที่ |
|---|---|---|
| คลัง | `meta` | `library_id`, `name`, `created_at`, `data_revision` |
| แหล่งรูป | `sources` | โฟลเดอร์ที่ผู้ใช้เลือก (หนึ่งแถวต่อ path), โหมดนำเข้า, ความตั้งใจติดตาม/วิเคราะห์ |
| | `event_source_bindings` | กิจกรรม ↔ ขอบเขตโฟลเดอร์; `recursive = 0` คือ "รูปที่วางตรงโฟลเดอร์แม่" |
| กิจกรรม | `events` | ชื่อ, วันกิจกรรม (ISO ค.ศ.) + ที่มา, ปก, หมายเหตุ, `record_version` |
| รูป | `photos` | file instance หนึ่งตำแหน่ง; ชี้ `current_revision_id` |
| | `photo_locations` | ตำแหน่งที่ใช้งานอยู่ (หนึ่งต่อรูป), ชื่อไฟล์ + คีย์ค้นหา, `available` |
| | `photo_revisions` | รุ่นเนื้อหา: ขนาด, mtime, SHA-256, ขนาดภาพหลังหมุน, orientation, วันถ่ายท้องถิ่น + ที่มา, สถานะภาพย่อ |
| | `event_photos` | กิจกรรมหลักของรูป (หนึ่งกิจกรรมต่อรูป) |
| | `source_photos` | รูปนี้เข้าถึงได้ผ่านแหล่งใด |
| ใบหน้า | `faces` | กรอบบน revision หนึ่ง, `origin` (`manual`/`detector`/`legacy`), `status` (`active`/`ignored`/`superseded`) |
| | `face_assignments` | คำยืนยัน ใบหน้า → บุคคล พร้อม `operation_id`; เพิกถอนด้วย `revoked_at` ไม่ลบแถว |
| | `face_embeddings` (v2) | เวกเตอร์ของใบหน้า + `space`; เทียบได้เฉพาะ space เดียวกัน |
| | `face_rejections` (v2) | "ไม่ใช่บุคคลนี้" ต่อข้อเสนอของ AI: คู่ ใบหน้า–บุคคล ที่ไม่ให้เสนอซ้ำ |
| บุคคล | `persons`, `person_aliases` | ชื่อแสดง, ชื่อเล่น, ชื่ออื่น, หมายเหตุ, ปก, เก็บ/ไม่เก็บ |
| | `person_reference_samples` (v2) | ภาพครอปใบหน้าตัวอย่างที่นำเข้าจากรุ่น Python + เวกเตอร์ที่สร้างใหม่; ไม่นับเป็นรูปกิจกรรม |
| | `legacy_id_map` (v2) | ระเบียนของรุ่นเดิม → ID ใหม่ กันนำเข้าซ้ำ |
| | `groups`, `person_groups` | กลุ่มที่ผู้ใช้ตั้ง; คนหนึ่งอยู่ได้หลายกลุ่ม |
| | `pinned_persons`, `recent_person_views`, `ui_preferences` | ปักหมุด, เปิดล่าสุด, แท็บหน้าหลัก |
| งาน | `jobs`, `job_errors` | คิวถาวร, ตัวนับ, ข้อผิดพลาดรายไฟล์ |
| ส่งออก | `selections`, `selection_items` | snapshot ของ "เลือกผลค้นหาทั้งหมด" |
| | `export_runs`, `export_items` | ผลรายรูปของการส่งออกแต่ละครั้ง |
| | `photo_edits` | การแต่งภาพต่อรูป (JSON ของ `edit::EditSettings` + `edit_id` ที่ใช้เป็นชื่อ cache) ไฟล์ต้นฉบับไม่ถูกแตะ — DECISIONS ข้อ 17 |
| | `edit_presets` | preset ของค่าแสงและสี (JSON ของ `adjust::Adjust`) · `source` = user/lightroom · `unsupported` = ส่วนของ .xmp ที่นำเข้าไม่ได้ (schema v7) — DECISIONS ข้อ 20 |
| | `watermarks` | โลโก้ลายน้ำที่ผู้ใช้เพิ่ม (PNG ในคลัง ด้านยาวไม่เกิน 2048) · `export_runs.watermark` = ลายน้ำของงานส่งออก — DECISIONS ข้อ 16 |
| | `shrink_runs`, `shrink_items` | ผลรายรูปของการย่อไฟล์ต้นฉบับ (สถานะ เหตุผลที่ข้าม ตำแหน่งต้นฉบับในถังขยะ) — DECISIONS ข้อ 15 |
| บันทึก | `operations` | audit ของการแก้ใบหน้า (ยังไม่มี undo) |
| | `requests` | ผลของคำสั่งที่มี request ID สำหรับตอบซ้ำเมื่อ retry |
| view | `v_confirmed` | ใบหน้าที่ยืนยันแล้วและยังมีผล — ที่มาเดียวของจำนวนรูปของบุคคล, กิจกรรมของบุคคล และ "ปรากฏร่วมในภาพ" |

## กติกาที่ฐานข้อมูลบังคับ

| กติกา (Blueprint 6.2 / 8.3) | บังคับด้วย |
|---|---|
| หนึ่งใบหน้ามีบุคคลที่ยืนยันได้ไม่เกินหนึ่งคน | unique index `ux_assignment_active (face_id) WHERE revoked_at IS NULL` |
| หนึ่งตำแหน่งไฟล์ = หนึ่งรูป | `photo_locations.path_key UNIQUE` |
| หนึ่งกิจกรรมหลักต่อรูป | `event_photos.photo_id PRIMARY KEY` |
| โฟลเดอร์หนึ่งผูกกิจกรรมที่ใช้งานอยู่ได้หนึ่งกิจกรรมต่อโหมด | unique index `ux_binding_folder (folder_key, recursive) WHERE active = 1` |
| งานที่ยังไม่จบมีหนึ่งงานต่อ dedup key | unique index `ux_jobs_dedup … WHERE state IN (…)` |
| กรอบใบหน้าอยู่ในภาพ | `CHECK` บน `bbox_*` (สัดส่วน [0,1], กว้าง/สูง > 0, ไม่ล้นขอบ) |
| ชื่อกลุ่มไม่ซ้ำ | `groups.name_key UNIQUE` |
| ชื่อบุคคล/กิจกรรมซ้ำได้ | ไม่มี unique บนชื่อ |
| ลบกลุ่มไม่ลบคน; ลบคนไม่ได้ถ้ามีคำยืนยัน | `ON DELETE CASCADE` เฉพาะตารางสมาชิก; `face_assignments.person_id` ไม่มี cascade |
| foreign keys ทำงานจริง | `PRAGMA foreign_keys = ON` ทุก connection + ตรวจตอนเปิด + `pragma_foreign_key_check` |

กติกาที่โค้ดบังคับ (มีเทสต์): ไฟล์ที่ไม่พบไม่ถูกลบจากคลัง · เนื้อหาเปลี่ยนที่ path เดิมและมีคำยืนยัน → revision ใหม่ คำยืนยันเดิมไม่ถูกย้ายเอง ·
ผลภาพย่อเขียนเฉพาะเมื่อ size/mtime ของ revision ยังตรงกับที่อ่าน · ไฟล์ในโฟลเดอร์ที่ผูกกับกิจกรรมอื่นเป็นของกิจกรรมนั้น

## หน่วยนับ

- **จำนวนรูปของบุคคล** = `COUNT(DISTINCT photo_id)` จาก `v_confirmed` — คนเดียวหลายหน้าในภาพเดียวนับครั้งเดียว
- **จำนวนกิจกรรมของบุคคล** = `COUNT(DISTINCT event_id)` จาก `v_confirmed`
- **ใบหน้ารอตรวจสอบ** = ใบหน้า `active` บน revision ปัจจุบันที่ไม่มีคำยืนยัน — ข้อเสนอของ AI ไม่ถูกนับเป็นรูปของบุคคลในทุกกรณี
- **ปรากฏร่วมในภาพ** = `COUNT(DISTINCT photo_id)` ที่ A และ B อยู่ใน `v_confirmed` ของรูปเดียวกัน ไม่รวม A เองและคนที่เก็บไว้
- **จำนวนรูปของคลัง/กิจกรรม** = file instances ที่ `catalog_state = 'active'`

## พิกัดและเวลา

- กรอบใบหน้า: `(x, y, w, h)` เป็นสัดส่วนบนภาพ **ที่หมุนตาม EXIF แล้ว** มุมซ้ายบนเป็นจุดเริ่ม — ใช้ชุดเดียวกันทั้งตัวดูภาพ ครอป และการเก็บ
- `events.event_date`: `YYYY-MM-DD` ค.ศ.; การแสดง พ.ศ. เป็นหน้าที่ของ UI
- `photo_revisions.taken_at_local`: `YYYY-MM-DDTHH:MM:SS` ตาม EXIF `DateTimeOriginal` ไม่เติม timezone; `tz_offset` เก็บเมื่อ EXIF มี
  ไม่ใช้ mtime แทนวันถ่าย (`date_source = 'none'` เมื่อไม่มี EXIF)
- เวลาทางเทคนิค (`created_at` ฯลฯ): RFC 3339 UTC

## Versioning

- `record_version` บน `events`, `persons`, `photos`, `faces`; การแก้กิจกรรมและบุคคลต้องส่ง `expectedVersion` ไม่ตรง → `VERSION_CONFLICT`
- `meta.data_revision` เพิ่มทุกครั้งที่ข้อมูลเปลี่ยน ส่งกลับในผลค้นและ event `data-changed`

## ส่วนของ Blueprint หมวด 8 ที่ยังไม่มีใน schema นี้

`libraries` (หลายคลัง) · `source_exclusions` · `catalog_suppressions` · `face_analysis_runs` · `embedding_spaces` (ตอนนี้เป็นคอลัมน์ `space`) ·
`person_redirects` · `person_samples` · `match_candidates` · `face_clusters` · `cluster_constraints` (ข้อเสนอและกลุ่มคำนวณตอนเปิดหน้าตรวจสอบ ไม่เก็บลงตาราง) ·
`tags` · `job_items` · `operation_items` · `model_manifests` · `index_manifests` · `migration_runs` · `legacy_identity_claims` · `photo_relocations`

จะเพิ่มเป็น migration ถัดไปเมื่อถึงงานที่ใช้ (AI, รวม/แยกบุคคล, relink, archive, ย้ายข้อมูลเดิม) ไม่สร้างตารางเปล่าไว้ล่วงหน้า

## สำรองข้อมูล

ชุดสำรองหนึ่งชุด = โฟลเดอร์ที่มี `library.sqlite` (snapshot ด้วย `VACUUM INTO` ผ่านการเชื่อมต่อแยก จึงสอดคล้องแม้แอปกำลังเขียน)
กับ `manifest.json` (รหัสคลัง, รุ่นแอปและ schema, เวลาที่สร้าง, จำนวนกิจกรรม/รูป/บุคคล/คำยืนยัน/ใบหน้า, ขนาด และ SHA-256 ของไฟล์ฐานข้อมูล)
**ไม่รวม** รูปต้นฉบับ ภาพย่อ ครอปใบหน้า และไฟล์โมเดล เขียนลงโฟลเดอร์ชั่วคราว `.<ชื่อ>.partial` ก่อนแล้วเปลี่ยนชื่อเมื่อครบ

| ชนิด (`kind`) | เกิดเมื่อ | ที่เก็บ | เก็บไว้ |
|---|---|---|---|
| `manual` | ผู้ใช้กด "สำรองไปยังโฟลเดอร์…" | โฟลเดอร์ที่เลือก (`PuriPhotoSearch-backup-<เวลา>`) | ไม่ลบให้ |
| `auto` | เปิดแอป และชุดอัตโนมัติล่าสุดเก่ากว่า 24 ชั่วโมง (คลังว่างไม่สำรอง) | `<โฟลเดอร์คลัง>/backups/auto-<เวลา>` | 3 ชุดล่าสุด |
| `before_restore` | ก่อนสลับคลังที่กู้คืนเข้า | `<โฟลเดอร์คลัง>/backups/before-restore-<เวลา>` | 2 ชุดล่าสุด |
| `before_upgrade` | ก่อน migration เปลี่ยน schema | `<โฟลเดอร์คลัง>/backups/before-upgrade-v<รุ่นเดิม>-<เวลา>` | 2 ชุดล่าสุด |

**กู้คืน** (`preview_restore` → `stage_restore`): ตรวจ SHA-256 กับ manifest, `application_id`, รุ่น schema (ใหม่กว่าแอป = ปฏิเสธ), `quick_check` และ foreign keys
แล้วคัดลอกไฟล์ไปวางรอเป็น `restore-pending.sqlite` ในโฟลเดอร์คลัง คลังที่เปิดอยู่ไม่ถูกแตะ เมื่อเปิดแอปครั้งถัดไป (`App::open` หลังได้ lock)
แอปเก็บคลังปัจจุบันเป็นชุด `before_restore` ก่อน แล้วจึงสลับไฟล์ ถ้าเก็บไม่ได้หรือไฟล์ที่รอไว้ไม่ผ่านการตรวจ จะไม่สลับและเปิดคลังเดิมต่อ
`cancel_restore` ลบไฟล์ที่รอไว้ ชุดสำรองอัตโนมัติอยู่บนดิสก์เดียวกับคลัง จึงกันได้แค่ความผิดพลาดของข้อมูล ไม่กันดิสก์เสีย
