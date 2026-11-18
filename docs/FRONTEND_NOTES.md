# Frontend notes — Puri Photo Search 2

บันทึกจากฝั่ง UI (`src/`) สำหรับคนที่ดูแลสัญญา IPC และ Rust backend
ไฟล์สัญญา `src/lib/ipc/types.ts` และ `src/lib/ipc/client.ts` ไม่ถูกแก้จากฝั่ง UI

## 1. สิ่งที่ต้องตัดสินใจ / แก้ฝั่งสัญญา

แก้แล้วเมื่อ 18 พ.ย. 2569: `client.ts` ผ่าน `noUncheckedIndexedAccess` (เปิด flag กลับแล้ว) · เพิ่ม `Capabilities.folderWatch` และสวิตช์ติดตามรูปใหม่ปิดใช้งานตามจริง ·
เพิ่มคำสั่ง `reveal_export_destination` + ปุ่ม `export-reveal` ในรายงานส่งออก · ระบุใน `PhotoResult` ว่า `width/height` คือขนาดหลังหมุนตาม EXIF

ที่ยังค้าง:

| เรื่อง | สถานะใน UI ตอนนี้ | ข้อเสนอ |
|---|---|---|
| `preview_import` ตอบครั้งเดียว | ระหว่างรอแสดง "กำลังตรวจรายการ…" + ยกเลิกได้ แต่ไม่มีตัวเลข "พบแล้ว X ไฟล์" | ถ้าต้องการตัวนับ ต้องมี event/คำสั่ง poll ของ preview |
| ไม่มีคำสั่งขอรายการใบหน้าของบุคคล | S09 ไม่มีแท็บ "ใบหน้าตัวอย่าง"; ตั้งภาพตัวแทนได้จากเมนูของใบหน้าในตัวดูภาพ (`set_person_cover`) | เพิ่มคำสั่ง list faces/samples ของบุคคลใน Phase 2 |
| ไม่มีคำสั่ง archive กิจกรรม/แหล่งรูป, relink | S12/S13 ไม่มีเมนู "นำออกจากคลัง" และไม่มี S14 | ตามแผน Phase 3 |
| ไม่มีคำสั่ง undo ทั่วไป | "เลิกทำ" มีเฉพาะหลัง "ไม่ใช่บุคคลนี้" (เรียก `assign_faces` กลับคนเดิม) | `undo_operation` ใน Phase 2 |
| ไม่มีคำสั่งเปิด/เลือกคลังอื่น | S01 ไม่มีปุ่ม "เปิดคลังเดิม" | — |
| **`reviewRevision` เปลี่ยนทุกรูปที่วิเคราะห์เสร็จ** | ระหว่างงานวิเคราะห์วิ่ง `get_review_faces` ตอบ `PREVIEW_STALE` แทบทุกครั้ง (overview ใช้ ~1.2 วิ แล้ว revision ขยับไปแล้ว) UI ลองใหม่เองไม่เกิน 2 ครั้ง จากนั้นแสดง "คิวกำลังเปลี่ยนระหว่างที่โหลดกลุ่มนี้" + ปุ่ม `review-reload-group`; overview ไม่ refetch เอง มีแบนเนอร์ `review-refresh` ให้ผู้ใช้กด | ให้ backend เสิร์ฟ snapshot ของ revision ที่ออกไป (อายุสั้น) หรือไม่ bump `reviewRevision` จากความคืบหน้าการวิเคราะห์ — bump เฉพาะเมื่อการตัดสินของผู้ใช้เปลี่ยนสมาชิกกลุ่ม |
| ครอปใบหน้า `face/{id}` ช้า | grid ใน S16 (สูงสุด ~100 ใบต่อกลุ่ม) และภาพย่อในคิวขึ้นช้า เห็นช่องว่างระหว่างรอ | สร้างครอปตอนวิเคราะห์แล้วเก็บ cache แทนการถอดรหัสต้นฉบับทุกครั้ง |
| ไม่มีคำสั่งขอ "ตัวอย่างที่ยืนยันแล้ว" ของบุคคล | แผงขวาของ S16 (`review-suggested-person`) แสดงได้แค่ภาพตัวแทน + จำนวนรูป + ลิงก์ไปหน้าบุคคล ไม่มีแถวตัวอย่างให้เทียบ | คำสั่ง list faces/samples ของบุคคล (เรื่องเดียวกับแถวถัดไป) |
| S16 มีแค่สองคิว (`suggested`, `clusters`) | ไม่มีแท็บ "อาจเป็นคนเดียวกัน" และ "รายการที่ข้ามไว้"; ไม่มีปุ่ม "ข้าม"/"เลิกทำ"; `singleFaceCount` แสดงเป็นข้อความ + ลิงก์ไปกิจกรรม (ไม่มีรายการให้เปิด) | เพิ่มเมื่อ backend มี merge-candidate / skip / undo |
| `get_review_faces` ตัดรายการ (`total` > `faces.length`) | แสดง `review-truncated` "แสดง X จาก Y ใบหน้า" การยืนยันใช้กับใบที่แสดงและเลือกไว้เท่านั้น แล้วกลุ่มจะโหลดชุดถัดไปเอง | ไม่ต้องแก้ ถ้าตั้งใจให้ทำเป็นรอบ |
| `Capabilities.faceAnalysis.state = 'installing'` ไม่มีเปอร์เซ็นต์ในตัว | `FaceModelNotice` อ่านความคืบหน้าจาก `get_job(installJobId)` ทุก 1 วิ | — |
| `delete_group` | ยังไม่ถูกเรียก (S11 จัดกลุ่มยังไม่ทำ; สร้างกลุ่มได้จาก S10) | Phase 2 |

## 2. ข้อสมมติของ UI ต่อ shell/แอป

- **Title bar แบบ Overlay** (`titleBarStyle: "Overlay"`, `hiddenTitle: true`, `trafficLightPosition {x:18,y:20}`): sidebar เว้นแถบบน 44px เมื่อ `isTauri`; แถบบนของทุกหน้า (`PageHeader`), หัวหน้าหลัก, แถบเครื่องมือตัวดูภาพ และหน้า Welcome ติด `data-tauri-drag-region`
- **CSP** ต้องอนุญาต `img-src 'self' puri:` (asset ของ Rust) และ `font-src 'self'` (Noto Sans Thai ถูก bundle ในเครื่อง ไม่มี CDN)
- **Asset**: `thumb/{photoId}` ถูกเรียกทั้งแบบมี `?v={revisionId}` (grid) และไม่มี (filmstrip, ปกกิจกรรม); ภาพตัวแทนบุคคลใช้ `PersonRef.coverAsset` ผ่าน `assetPath()` เท่านั้น (ไม่ประกอบ URL จาก `coverFaceId` แล้ว; ไม่มีค่า = ตัวอักษรย่อ); `face/{faceId}` ใช้เป็นครอปของใบหน้าในแผง "คนในรูป" และ grid ของ S16; `draft/{previewId}/{draftId}` ใช้ใน S03
- `.gitignore` มี `node_modules/`, `dist/` และ **`screenshots/`** แล้ว — ภาพใน `screenshots/fe-*.jpg` ถ่ายจากคลังทดสอบที่ใช้รูปจริงของหลวงพี่ (มีใบหน้าบุคคลจริง) จึงไม่ควร commit
- ธีม (ตามระบบ/สว่าง/มืด) และการยุบ sidebar เก็บใน `localStorage` (`puriphoto.theme`, `puriphoto.sidebarCollapsed`) ไม่ใช่ข้อมูลคลัง

## 3. พฤติกรรมที่ควรรู้ตอนเขียน e2e

- Router เป็น HashRouter: `#/home`, `#/search?p=<id>,<id>&pm=any&e=<id>&q=…&df=event_date&from=YYYY-MM-DD&to=…&unk=1&face=no_person&sort=filename`, `#/people`, `#/people/:id`, `#/events`, `#/events/:id?tab=people|info|history&person=<id>`, `#/import/review`, `#/photos/:id`, `#/activity?tab=history&job=<id>`, `#/settings?tab=library|ai|migration`, `#/review?event=<id>&tab=suggested|unknown`, `#/welcome`
- เปิดแอปที่ `#/` → `get_library_summary` → `eventCount === 0` ไป `#/welcome` ไม่งั้น `#/home`
- ตัวดูภาพเป็น overlay ที่ portal ลงใน `#app-main`; หน้าเบื้องหลังยัง mount อยู่ (ติด `inert`) จึงคง scroll/ตัวกรอง/รูปที่เลือก ปิดแล้วกลับที่เดิม ลำดับ ←/→ มาจากชุดผลของหน้าที่เปิด และโหลดหน้าถัดไปเองเมื่อใกล้ท้าย
- โหมด dev (`folderPicker === 'dev_path_input'`): ปุ่มเพิ่มโฟลเดอร์/เลือกปลายทางส่งออกเปิด dialog ที่มีช่อง `dev-folder-input` → พิมพ์ path แล้ว Enter
- การเพิ่มกรอบใบหน้า: กด `viewer-add-face` → ลากเมาส์บน `viewer-stage` (pointer events; กรอบที่ด้านใดด้านหนึ่งเล็กกว่า 1.2% ของภาพถือเป็นคลิกพลาด) → popover มี `person-picker-input` → Enter เลือกรายการที่ไฮไลต์ (รายการแรก หรือ "สร้างบุคคลใหม่ …" เมื่อไม่มีคนชื่อตรง)
- งานเบื้องหลัง: poll `list_jobs {activeOnly:true}` ทุก 1 วิ เมื่อมีงานที่ยังทำอยู่ (5 วิ เมื่อไม่มี) และ invalidate เมื่อ counts เปลี่ยน/ได้ `data-changed`
- S16 `#/review`: คิว (`get_review_overview`) ถูกตรึงไว้จนกว่าผู้ใช้จะตัดสินหรือกด `review-refresh` (ไม่ refetch ตอนโฟกัสหน้าต่าง/งานจบ) เพื่อไม่ให้รายการสลับใต้มือ; ทุกกลุ่มเริ่มด้วย**เลือกทุกใบ**; หลังยืนยัน/ปฏิเสธ/ตั้งชื่อ คิวโหลดใหม่และคงกลุ่มเดิมไว้ถ้ายังอยู่ ไม่งั้นไปกลุ่มที่ตำแหน่งเดิม; คลิกครอป (`review-face-open`) เปิดตัวดูภาพทับหน้าเดิม
- ข้อเสนอของ AI ไม่ถูกนับเป็นชื่อ: `face-box`/`face-row` มี `data-face-state="confirmed|suggested|unknown"`; ใบที่ยังไม่ยืนยันคลิกที่กรอบแล้วได้ `face-name-popover`
- ติดตั้งตัววิเคราะห์ใบหน้า: `face-model-install` → `install_face_model` → งาน kind `model` (counts เป็น MB) → เมื่อจบ `capabilities` ถูกโหลดใหม่และ `face-model-notice` หายไป (หรือเป็น `face-model-ready` ในหน้าตั้งค่า)
- Toast (sonner) อยู่กลางล่าง; ข้อความสำเร็จขึ้นหลัง backend ตอบเท่านั้น

## 4. `data-testid`

**Shell / นำทาง** — `nav-home` `nav-search` `nav-people` `nav-events` `nav-review` `nav-settings` · `add-folder-button` (ปุ่มเพิ่มโฟลเดอร์บนทุกหน้า) · `welcome-add-folder` `welcome-legacy` · `global-search-trigger` (ช่องที่เปิด ⌘K) · `global-search-input` (ช่องพิมพ์ใน ⌘K) · `palette-search-text` `palette-person` `palette-event` `palette-file` · `job-indicator` · `dev-folder-input` · `library-summary` · `nav-review-badge` (จำนวนใบหน้าที่ยังไม่มีชื่อ) · `home-review-strip`

**S02–S03 เพิ่มโฟลเดอร์** — `import-pick-folder` · `import-scanning` · `import-review` · `import-root-path` `import-change-folder` · `import-mode-single` `import-mode-children` (radio) · `import-select-all` · `import-draft-row` (มี `data-draft-id`, `data-draft-kind`) → ข้างใน: `import-draft-select` (checkbox, `aria-checked`/`data-state`) `import-draft-title` (input) `import-draft-date` · `import-analyze-faces` `import-watch` (switch; `import-analyze-faces` เปิดได้เมื่อ state เป็น `ready`/`installing`) · `import-summary` · `import-cancel` · `import-commit` · `import-refresh-preview`

**S12–S13 กิจกรรม** — `events-search` `events-clear-search` `events-show-all` · `event-grid` `home-event-grid` · `event-card` (`data-event-id`) · `event-title` `event-photo-count` · `event-reveal` `event-rescan` · `event-tab-photos|people|info|history` · `event-person-all` `event-person-chip` · `event-title-input` `event-save` · `event-face-progress` (การ์ดและหัวหน้ารายละเอียด: "วิเคราะห์ใบหน้า X/Y") `event-analyze-faces` `event-review-link`

**Grid รูป / เลือกหลายรูป** — `photo-grid` · `photo-card` (`data-photo-id`) → `photo-card-open` (เปิดตัวดูภาพ) `photo-card-select` (checkbox, `aria-checked`) · `photo-grid-load-more` · `selection-bar` `selection-count` `select-all-results` `selection-clear` · `export-button`

**S15 ตัวดูภาพ** — `photo-viewer` · `viewer-close` (ปุ่มกลับ) `viewer-close-x` · `viewer-prev` `viewer-next` · `viewer-toggle-faces` · `viewer-fit` · `viewer-export` · `viewer-stage` `viewer-image-box` `viewer-resolution` · `viewer-add-face` · `face-draft-box` · `face-box` (`data-face-id`) · `face-row` (`data-face-id`) → `face-assign` `face-unassign` `face-suggestion-accept` `face-suggestion-reject` · `face-name-popover` (เปิดจากกรอบของใบหน้าที่ยังไม่ยืนยัน) → `face-suggestion-yes` `face-suggestion-no` + ตัวเลือกบุคคล · `viewer-reveal` `viewer-open-original`

**เลือกบุคคล (popover)** — `person-picker-input` · `person-picker-option` (`data-person-id`) · `person-picker-create`

**S05/S08–S10 บุคคล** — `home-people-grid` `people-grid` · `home-people-tab-pinned|recent` · `person-card` (`data-person-id`) → `person-card-open` `person-quickview` (ปุ่มตา) `person-pin` (`aria-pressed`) · `person-quickview-panel` `person-quickview-close` `person-quickview-open-all` · `coappearance-item` · `people-search` `people-filter-pinned` `people-add` · `person-name` `person-counts` `person-detail-pin` `person-edit` · `person-tab-photos|events|together|info` · `person-event-card` · `person-name-input` `person-save-button`

**S06 ค้นหา** — `search-input` · `search-add-person` · `search-person-chip` (`data-person-id`) · `search-person-mode-all` `search-person-mode-any` (แท็บ `aria-selected`) · `search-add-event` `search-event-chip` · `search-date` · `search-clear` `search-switch-any` · `search-total` · `search-coverage`

**S18 ส่งออก** — `export-button` (ในแถบเลือก) / `viewer-export` · `export-pick-destination` · `export-plan` · `export-confirm` · `export-progress` · `export-report` · `export-done`

**S04/S19 งาน** — `activity-tab-current|history` · `job-list` · `job-card` (`data-job-id`, `data-job-state`) → `job-pause` `job-resume` `job-retry` `job-cancel` · `job-errors` · `job-strip` `job-strip-open`

**ตัววิเคราะห์ใบหน้า (`FaceModelNotice`: Welcome, S03, หน้าหลัก, S16, ตั้งค่า→AI)** — `face-model-notice` → `face-model-install` · `face-model-installing` `face-model-progress` `face-model-cancel` · `face-model-failed` · `face-model-ready` · `face-model-unsupported`

**S16 ตรวจสอบใบหน้า** — `review-event-scope` (select ขอบเขตกิจกรรม) · `review-unnamed-count` · `review-refresh` · `review-analyze` (วิเคราะห์รูปที่ค้าง) · `review-workspace` · `review-tab-suggested` `review-tab-unknown` · `review-queue` → `review-group` (`data-group-id`, `data-group-kind`) · `review-group-panel` → `review-select-all` `review-selection-count` `review-truncated` · `review-face-grid` → `review-face` (`data-face-id`) → `review-face-open` `review-face-select` (checkbox) · `review-actions` → `review-confirm` (ใช่) `review-reject` (ไม่ใช่) `review-name` (เป็นคนอื่น…/ตั้งชื่อ → `person-picker-*`) `review-more` (เมนู "ไม่ใช่ใบหน้า") · `review-suggested-person` · `review-reload-group` · `review-single-faces` · `review-switch-tab` · `review-empty` `review-open-events`

**ตั้งค่า** — `settings-tab-general|library|ai|migration` · `theme-system|light|dark` · `settings-face-analysis` `settings-analyze-faces` · `legacy-inspect` `legacy-report` · `legacy-import` `legacy-import-run` `legacy-import-result` `legacy-import-open-people`

## 5. ยังไม่ได้ทำใน UI (ไม่มีปุ่มลวงสำหรับรายการเหล่านี้)

S07 ค้นด้วยใบหน้าตัวอย่าง · S11 จัดกลุ่มบุคคล (หน้าเต็ม) · S14 แหล่งรูป/relink · S16 เฉพาะส่วน "อาจเป็นคนเดียวกัน", "รายการที่ข้ามไว้", ข้าม/เลิกทำ และแถวตัวอย่างที่ยืนยันแล้ว (ดูหมวด 1) · S17 รวม/แยกบุคคล · S20 สำรอง/กู้คืน (นำเข้ารายชื่อบุคคลจากรุ่นเดิมทำแล้ว — `import_legacy_people`; รูป/กิจกรรมเดิมไม่ย้าย) · S21 ตั้งค่าโมเดลขั้นสูง (มีเฉพาะติดตั้ง/สถานะ) · S22 วิธีใช้/diagnostics · virtualization ของ grid (ตอนนี้โหลดทีละ 100 รูปด้วย cursor + lazy image) · การจัดลำดับ pin ด้วยการลาก
