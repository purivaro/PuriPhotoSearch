-- Puri Photo Search 2 — schema v1 (Blueprint หมวด 8)
-- ชื่อ (กิจกรรม/บุคคล) ไม่เป็น key; path ไม่เป็นตัวตนของรูป; ทุก ID เป็น UUIDv7 แบบข้อความ

CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE sources (
  id                 TEXT PRIMARY KEY,
  root_path          TEXT NOT NULL,
  path_key           TEXT NOT NULL UNIQUE,
  import_mode        TEXT NOT NULL CHECK (import_mode IN ('single_event', 'child_events')),
  watch_enabled      INTEGER NOT NULL DEFAULT 1,
  analyze_faces      INTEGER NOT NULL DEFAULT 1,
  catalog_state      TEXT NOT NULL DEFAULT 'active' CHECK (catalog_state IN ('active', 'archived')),
  last_reconciled_at TEXT,
  created_at         TEXT NOT NULL
);

CREATE TABLE photos (
  id                  TEXT PRIMARY KEY,
  current_revision_id TEXT,
  catalog_state       TEXT NOT NULL DEFAULT 'active' CHECK (catalog_state IN ('active', 'archived')),
  favorite            INTEGER NOT NULL DEFAULT 0,
  record_version      INTEGER NOT NULL DEFAULT 1,
  created_at          TEXT NOT NULL
);

CREATE INDEX ix_photos_revision ON photos(current_revision_id);

CREATE TABLE events (
  id             TEXT PRIMARY KEY,
  title          TEXT NOT NULL,
  title_key      TEXT NOT NULL,
  event_date     TEXT,
  date_source    TEXT NOT NULL DEFAULT 'none' CHECK (date_source IN ('folder_name', 'user', 'none')),
  cover_photo_id TEXT REFERENCES photos(id) ON DELETE SET NULL,
  notes          TEXT NOT NULL DEFAULT '',
  catalog_state  TEXT NOT NULL DEFAULT 'active' CHECK (catalog_state IN ('active', 'archived')),
  record_version INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL
);

-- กิจกรรมผูกกับขอบเขตโฟลเดอร์; recursive = 0 คือ "รูปที่วางตรงโฟลเดอร์แม่"
CREATE TABLE event_source_bindings (
  event_id    TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  source_id   TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  folder_path TEXT NOT NULL,
  folder_key  TEXT NOT NULL,
  recursive   INTEGER NOT NULL DEFAULT 1,
  active      INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (event_id, source_id)
);
CREATE UNIQUE INDEX ux_binding_folder ON event_source_bindings(folder_key, recursive) WHERE active = 1;

-- หนึ่ง file instance = หนึ่งตำแหน่งที่ใช้งานอยู่
CREATE TABLE photo_locations (
  photo_id     TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  os_path      TEXT NOT NULL,
  path_key     TEXT NOT NULL UNIQUE,
  filename     TEXT NOT NULL,
  filename_key TEXT NOT NULL,
  available    INTEGER NOT NULL DEFAULT 1,
  verified_at  TEXT
);
CREATE INDEX ix_location_filename ON photo_locations(filename_key);

CREATE TABLE source_photos (
  source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  photo_id  TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  PRIMARY KEY (source_id, photo_id)
) WITHOUT ROWID;

-- หนึ่งกิจกรรมหลักต่อ file instance (photo_id เป็น PRIMARY KEY)
CREATE TABLE event_photos (
  photo_id TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE
) WITHOUT ROWID;
CREATE INDEX ix_event_photos_event ON event_photos(event_id);

CREATE TABLE photo_revisions (
  id                TEXT PRIMARY KEY,
  photo_id          TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  size              INTEGER NOT NULL,
  mtime_ns          INTEGER NOT NULL,
  sha256            TEXT,
  width             INTEGER NOT NULL DEFAULT 0,
  height            INTEGER NOT NULL DEFAULT 0,
  orientation       INTEGER NOT NULL DEFAULT 1,
  taken_at_local    TEXT,
  tz_offset         TEXT,
  date_source       TEXT NOT NULL DEFAULT 'none' CHECK (date_source IN ('exif', 'none')),
  thumbnail_status  TEXT NOT NULL DEFAULT 'pending' CHECK (thumbnail_status IN ('pending', 'ready', 'failed')),
  decode_error_code TEXT,
  decode_error      TEXT,
  face_status       TEXT NOT NULL DEFAULT 'not_analyzed',
  pipeline_version  INTEGER NOT NULL DEFAULT 1,
  created_at        TEXT NOT NULL
);
CREATE INDEX ix_revision_photo ON photo_revisions(photo_id);
CREATE INDEX ix_revision_taken ON photo_revisions(taken_at_local);
CREATE INDEX ix_revision_sha ON photo_revisions(sha256);

-- bbox: สัดส่วน [0,1] บนภาพที่หมุนตาม EXIF แล้ว
CREATE TABLE faces (
  id                TEXT PRIMARY KEY,
  photo_revision_id TEXT NOT NULL REFERENCES photo_revisions(id) ON DELETE CASCADE,
  origin            TEXT NOT NULL CHECK (origin IN ('manual', 'detector', 'legacy')),
  bbox_x            REAL NOT NULL CHECK (bbox_x >= 0),
  bbox_y            REAL NOT NULL CHECK (bbox_y >= 0),
  bbox_w            REAL NOT NULL CHECK (bbox_w > 0),
  bbox_h            REAL NOT NULL CHECK (bbox_h > 0),
  status            TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'ignored', 'superseded')),
  record_version    INTEGER NOT NULL DEFAULT 1,
  created_at        TEXT NOT NULL,
  CHECK (bbox_x + bbox_w <= 1.0001 AND bbox_y + bbox_h <= 1.0001)
);
CREATE INDEX ix_faces_revision ON faces(photo_revision_id);

CREATE TABLE persons (
  id             TEXT PRIMARY KEY,
  display_name   TEXT NOT NULL,
  name_key       TEXT NOT NULL,
  nickname       TEXT,
  notes          TEXT NOT NULL DEFAULT '',
  cover_face_id  TEXT REFERENCES faces(id) ON DELETE SET NULL,
  catalog_state  TEXT NOT NULL DEFAULT 'active' CHECK (catalog_state IN ('active', 'archived')),
  record_version INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL
);
CREATE INDEX ix_persons_name ON persons(name_key);

CREATE TABLE person_aliases (
  person_id TEXT NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
  alias     TEXT NOT NULL,
  alias_key TEXT NOT NULL,
  PRIMARY KEY (person_id, alias_key)
) WITHOUT ROWID;

-- คำยืนยันของผู้ใช้; หนึ่ง face มี active assignment ได้ไม่เกินหนึ่งคน
CREATE TABLE face_assignments (
  id                   TEXT PRIMARY KEY,
  face_id              TEXT NOT NULL REFERENCES faces(id) ON DELETE CASCADE,
  person_id            TEXT NOT NULL REFERENCES persons(id),
  decision_origin      TEXT NOT NULL,
  operation_id         TEXT NOT NULL,
  confirmed_at         TEXT NOT NULL,
  revoked_at           TEXT,
  revoked_operation_id TEXT
);
CREATE UNIQUE INDEX ux_assignment_active ON face_assignments(face_id) WHERE revoked_at IS NULL;
CREATE INDEX ix_assignment_person ON face_assignments(person_id, face_id) WHERE revoked_at IS NULL;

CREATE TABLE groups (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  name_key   TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE person_groups (
  person_id TEXT NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
  group_id  TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
  PRIMARY KEY (person_id, group_id)
) WITHOUT ROWID;

CREATE TABLE pinned_persons (
  person_id  TEXT PRIMARY KEY REFERENCES persons(id) ON DELETE CASCADE,
  position   INTEGER NOT NULL,
  created_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE recent_person_views (
  person_id      TEXT PRIMARY KEY REFERENCES persons(id) ON DELETE CASCADE,
  last_viewed_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE ui_preferences (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE jobs (
  id             TEXT PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('catalog', 'export')),
  state          TEXT NOT NULL,
  stage          TEXT NOT NULL,
  title          TEXT NOT NULL,
  event_id       TEXT REFERENCES events(id) ON DELETE SET NULL,
  dedup_key      TEXT NOT NULL,
  total          INTEGER NOT NULL DEFAULT 0,
  done           INTEGER NOT NULL DEFAULT 0,
  failed         INTEGER NOT NULL DEFAULT 0,
  skipped        INTEGER NOT NULL DEFAULT 0,
  total_known    INTEGER NOT NULL DEFAULT 0,
  recovered      INTEGER NOT NULL DEFAULT 0,
  blocked_reason TEXT,
  created_at     TEXT NOT NULL,
  started_at     TEXT,
  finished_at    TEXT,
  updated_at     TEXT NOT NULL
);
-- งานที่ยังไม่จบมีได้หนึ่งงานต่อ dedup key (กันกดซ้ำ/สร้างงานซ้ำ)
CREATE UNIQUE INDEX ux_jobs_dedup ON jobs(dedup_key)
  WHERE state IN ('queued', 'running', 'pausing', 'paused', 'blocked', 'cancel_requested');
CREATE INDEX ix_jobs_state ON jobs(state, created_at);

CREATE TABLE job_errors (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id     TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  photo_id   TEXT,
  filename   TEXT NOT NULL,
  code       TEXT NOT NULL,
  message    TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX ix_job_errors_job ON job_errors(job_id);

-- บันทึกการแก้ข้อมูล (audit); undo ยังไม่เปิดใช้ในชุดงานนี้
CREATE TABLE operations (
  id         TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  summary    TEXT NOT NULL,
  created_at TEXT NOT NULL
);

-- ผลของคำสั่งที่มี request ID เก็บไว้ตอบซ้ำเมื่อ retry
CREATE TABLE requests (
  request_id TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  result     TEXT NOT NULL,
  created_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE selections (
  token         TEXT PRIMARY KEY,
  data_revision INTEGER NOT NULL,
  created_at    TEXT NOT NULL
);

CREATE TABLE selection_items (
  token       TEXT NOT NULL REFERENCES selections(token) ON DELETE CASCADE,
  ord         INTEGER NOT NULL,
  photo_id    TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  PRIMARY KEY (token, ord)
) WITHOUT ROWID;

CREATE TABLE export_runs (
  id          TEXT PRIMARY KEY,
  job_id      TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  destination TEXT NOT NULL,
  created_at  TEXT NOT NULL
);

CREATE TABLE export_items (
  run_id       TEXT NOT NULL REFERENCES export_runs(id) ON DELETE CASCADE,
  ord          INTEGER NOT NULL,
  photo_id     TEXT NOT NULL,
  revision_id  TEXT NOT NULL,
  filename     TEXT NOT NULL,
  source_path  TEXT NOT NULL,
  planned_name TEXT,
  status       TEXT NOT NULL CHECK (status IN ('planned', 'copied', 'failed', 'cancelled', 'unavailable')),
  output_path  TEXT,
  error        TEXT,
  PRIMARY KEY (run_id, ord)
) WITHOUT ROWID;

-- ใบหน้าที่ยืนยันแล้วและยังมีผล: assignment ที่ไม่ถูกเพิกถอน บน face ที่ active ของ revision ปัจจุบัน
-- ในรูปและกิจกรรมที่ยังอยู่ในคลัง — ทุกจำนวน/การค้นตามบุคคลใช้ view นี้ชุดเดียว (หมวด 6.1, 11.7)
CREATE VIEW v_confirmed AS
SELECT a.person_id, p.id AS photo_id, ep.event_id, f.id AS face_id, f.photo_revision_id AS revision_id
FROM face_assignments a
JOIN faces f ON f.id = a.face_id AND f.status = 'active'
JOIN photos p ON p.current_revision_id = f.photo_revision_id AND p.catalog_state = 'active'
JOIN event_photos ep ON ep.photo_id = p.id
JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
WHERE a.revoked_at IS NULL;
