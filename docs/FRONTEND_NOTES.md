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
| `Coverage` มีเฉพาะสถานะภาพย่อ | S06 แสดง "ข้อมูลรูปพร้อม X จาก Y" เมื่อยังเตรียมไม่ครบ และ "นับเฉพาะใบหน้าที่ยืนยันแล้ว" เมื่อกรองด้วยคน | เพิ่ม field ฝั่งใบหน้าเมื่อมี AI |
| `delete_group` | ยังไม่ถูกเรียก (S11 จัดกลุ่มยังไม่ทำ; สร้างกลุ่มได้จาก S10) | Phase 2 |

## 2. ข้อสมมติของ UI ต่อ shell/แอป

- **Title bar แบบ Overlay** (`titleBarStyle: "Overlay"`, `hiddenTitle: true`, `trafficLightPosition {x:18,y:20}`): sidebar เว้นแถบบน 44px เมื่อ `isTauri`; แถบบนของทุกหน้า (`PageHeader`), หัวหน้าหลัก, แถบเครื่องมือตัวดูภาพ และหน้า Welcome ติด `data-tauri-drag-region`
- **CSP** ต้องอนุญาต `img-src 'self' puri:` (asset ของ Rust) และ `font-src 'self'` (Noto Sans Thai ถูก bundle ในเครื่อง ไม่มี CDN)
- **Asset**: `thumb/{photoId}` ถูกเรียกทั้งแบบมี `?v={revisionId}` (grid) และไม่มี (filmstrip, ปกกิจกรรม); `face/{faceId}` ใช้ทั้งเป็นภาพตัวแทนบุคคล (`coverFaceId`) และครอปของใบหน้าในแผง "คนในรูป"; `draft/{previewId}/{draftId}` ใช้ใน S03
- `.gitignore` มี `node_modules/`, `dist/` และ **`screenshots/`** แล้ว — ภาพใน `screenshots/fe-*.jpg` ถ่ายจากคลังทดสอบที่ใช้รูปจริงของหลวงพี่ (มีใบหน้าบุคคลจริง) จึงไม่ควร commit
- ธีม (ตามระบบ/สว่าง/มืด) และการยุบ sidebar เก็บใน `localStorage` (`puriphoto.theme`, `puriphoto.sidebarCollapsed`) ไม่ใช่ข้อมูลคลัง

## 3. พฤติกรรมที่ควรรู้ตอนเขียน e2e

- Router เป็น HashRouter: `#/home`, `#/search?p=<id>,<id>&pm=any&e=<id>&q=…&df=event_date&from=YYYY-MM-DD&to=…&unk=1&face=no_person&sort=filename`, `#/people`, `#/people/:id`, `#/events`, `#/events/:id?tab=people|info|history&person=<id>`, `#/import/review`, `#/photos/:id`, `#/activity?tab=history&job=<id>`, `#/settings?tab=library|ai|migration`, `#/review`, `#/welcome`
- เปิดแอปที่ `#/` → `get_library_summary` → `eventCount === 0` ไป `#/welcome` ไม่งั้น `#/home`
- ตัวดูภาพเป็น overlay ที่ portal ลงใน `#app-main`; หน้าเบื้องหลังยัง mount อยู่ (ติด `inert`) จึงคง scroll/ตัวกรอง/รูปที่เลือก ปิดแล้วกลับที่เดิม ลำดับ ←/→ มาจากชุดผลของหน้าที่เปิด และโหลดหน้าถัดไปเองเมื่อใกล้ท้าย
- โหมด dev (`folderPicker === 'dev_path_input'`): ปุ่มเพิ่มโฟลเดอร์/เลือกปลายทางส่งออกเปิด dialog ที่มีช่อง `dev-folder-input` → พิมพ์ path แล้ว Enter
- การเพิ่มกรอบใบหน้า: กด `viewer-add-face` → ลากเมาส์บน `viewer-stage` (pointer events; กรอบที่ด้านใดด้านหนึ่งเล็กกว่า 1.2% ของภาพถือเป็นคลิกพลาด) → popover มี `person-picker-input` → Enter เลือกรายการที่ไฮไลต์ (รายการแรก หรือ "สร้างบุคคลใหม่ …" เมื่อไม่มีคนชื่อตรง)
- งานเบื้องหลัง: poll `list_jobs {activeOnly:true}` ทุก 1 วิ เมื่อมีงานที่ยังทำอยู่ (5 วิ เมื่อไม่มี) และ invalidate เมื่อ counts เปลี่ยน/ได้ `data-changed`
- Toast (sonner) อยู่กลางล่าง; ข้อความสำเร็จขึ้นหลัง backend ตอบเท่านั้น

## 4. `data-testid`

**Shell / นำทาง** — `nav-home` `nav-search` `nav-people` `nav-events` `nav-review` `nav-settings` · `add-folder-button` (ปุ่มเพิ่มโฟลเดอร์บนทุกหน้า) · `welcome-add-folder` `welcome-legacy` · `global-search-trigger` (ช่องที่เปิด ⌘K) · `global-search-input` (ช่องพิมพ์ใน ⌘K) · `palette-search-text` `palette-person` `palette-event` `palette-file` · `job-indicator` · `dev-folder-input` · `library-summary`

**S02–S03 เพิ่มโฟลเดอร์** — `import-pick-folder` · `import-scanning` · `import-review` · `import-root-path` `import-change-folder` · `import-mode-single` `import-mode-children` (radio) · `import-select-all` · `import-draft-row` (มี `data-draft-id`, `data-draft-kind`) → ข้างใน: `import-draft-select` (checkbox, `aria-checked`/`data-state`) `import-draft-title` (input) `import-draft-date` · `import-analyze-faces` `import-watch` (switch) · `import-summary` · `import-cancel` · `import-commit` · `import-refresh-preview`

**S12–S13 กิจกรรม** — `events-search` `events-clear-search` `events-show-all` · `event-grid` `home-event-grid` · `event-card` (`data-event-id`) · `event-title` `event-photo-count` · `event-reveal` `event-rescan` · `event-tab-photos|people|info|history` · `event-person-all` `event-person-chip` · `event-title-input` `event-save`

**Grid รูป / เลือกหลายรูป** — `photo-grid` · `photo-card` (`data-photo-id`) → `photo-card-open` (เปิดตัวดูภาพ) `photo-card-select` (checkbox, `aria-checked`) · `photo-grid-load-more` · `selection-bar` `selection-count` `select-all-results` `selection-clear` · `export-button`

**S15 ตัวดูภาพ** — `photo-viewer` · `viewer-close` (ปุ่มกลับ) `viewer-close-x` · `viewer-prev` `viewer-next` · `viewer-toggle-faces` · `viewer-fit` · `viewer-export` · `viewer-stage` `viewer-image-box` `viewer-resolution` · `viewer-add-face` · `face-draft-box` · `face-box` (`data-face-id`) · `face-row` (`data-face-id`) → `face-assign` `face-unassign` · `viewer-reveal` `viewer-open-original`

**เลือกบุคคล (popover)** — `person-picker-input` · `person-picker-option` (`data-person-id`) · `person-picker-create`

**S05/S08–S10 บุคคล** — `home-people-grid` `people-grid` · `home-people-tab-pinned|recent` · `person-card` (`data-person-id`) → `person-card-open` `person-quickview` (ปุ่มตา) `person-pin` (`aria-pressed`) · `person-quickview-panel` `person-quickview-close` `person-quickview-open-all` · `coappearance-item` · `people-search` `people-filter-pinned` `people-add` · `person-name` `person-counts` `person-detail-pin` `person-edit` · `person-tab-photos|events|together|info` · `person-event-card` · `person-name-input` `person-save-button`

**S06 ค้นหา** — `search-input` · `search-add-person` · `search-person-chip` (`data-person-id`) · `search-person-mode-all` `search-person-mode-any` (แท็บ `aria-selected`) · `search-add-event` `search-event-chip` · `search-date` · `search-clear` `search-switch-any` · `search-total`

**S18 ส่งออก** — `export-button` (ในแถบเลือก) / `viewer-export` · `export-pick-destination` · `export-plan` · `export-confirm` · `export-progress` · `export-report` · `export-done`

**S04/S19 งาน** — `activity-tab-current|history` · `job-list` · `job-card` (`data-job-id`, `data-job-state`) → `job-pause` `job-resume` `job-retry` `job-cancel` · `job-errors` · `job-strip` `job-strip-open`

**ตั้งค่า / ตรวจสอบใบหน้า** — `settings-tab-general|library|ai|migration` · `theme-system|light|dark` · `legacy-inspect` `legacy-report` · `review-open-events`

## 5. ยังไม่ได้ทำใน UI (ไม่มีปุ่มลวงสำหรับรายการเหล่านี้)

S07 ค้นด้วยใบหน้าตัวอย่าง · S11 จัดกลุ่มบุคคล (หน้าเต็ม) · S14 แหล่งรูป/relink · S16 คิวตรวจสอบ (หน้านี้บอกสถานะจริงและพาไประบุใบหน้าเอง) · S17 รวม/แยกบุคคล · S20 สำรอง/กู้คืน/นำเข้าข้อมูลเดิมจริง (มีเฉพาะ dry-run) · S21 ตั้งค่าโมเดล · S22 วิธีใช้/diagnostics · virtualization ของ grid (ตอนนี้โหลดทีละ 100 รูปด้วย cursor + lazy image) · การจัดลำดับ pin ด้วยการลาก
