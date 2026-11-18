-- schema v2: วิเคราะห์ใบหน้าอัตโนมัติ, ข้อเสนอของ AI, ใบหน้าตัวอย่างจากรุ่นเดิม, งานดาวน์โหลดโมเดล

ALTER TABLE faces ADD COLUMN score REAL;

-- เวกเตอร์ใบหน้า แยกตาม embedding space: ห้ามเทียบเวกเตอร์ต่าง space กัน (Blueprint 10.6)
CREATE TABLE face_embeddings (
  face_id TEXT PRIMARY KEY REFERENCES faces(id) ON DELETE CASCADE,
  space   TEXT NOT NULL,
  vector  BLOB NOT NULL
) WITHOUT ROWID;

-- ใบหน้าตัวอย่างของบุคคลที่ไม่ได้มาจากรูปในคลัง (นำเข้าจากรุ่น Python): ไม่นับเป็นรูปกิจกรรม
-- vector เป็น NULL จนกว่าจะสร้างด้วยโมเดลปัจจุบันจากภาพครอป
CREATE TABLE person_reference_samples (
  id         TEXT PRIMARY KEY,
  person_id  TEXT NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
  thumbnail  BLOB NOT NULL,
  space      TEXT,
  vector     BLOB,
  attempts   INTEGER NOT NULL DEFAULT 0,
  source     TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX ix_reference_person ON person_reference_samples(person_id);

-- "ไม่ใช่บุคคลนี้" ต่อข้อเสนอของ AI: จำคู่ ใบหน้า–บุคคล ไว้ไม่เสนอซ้ำ แม้สร้างดัชนีใหม่
CREATE TABLE face_rejections (
  face_id    TEXT NOT NULL REFERENCES faces(id) ON DELETE CASCADE,
  person_id  TEXT NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  PRIMARY KEY (face_id, person_id)
) WITHOUT ROWID;

-- ระเบียนของรุ่นเดิม → ID ใหม่ ใช้กันนำเข้าซ้ำ
CREATE TABLE legacy_id_map (
  kind      TEXT NOT NULL,
  legacy_id TEXT NOT NULL,
  new_id    TEXT NOT NULL,
  PRIMARY KEY (kind, legacy_id)
) WITHOUT ROWID;

-- jobs: เพิ่มชนิด 'model' (SQLite แก้ CHECK ไม่ได้ จึงสร้างตารางใหม่แล้วย้ายแถว)
CREATE TABLE jobs_v2 (
  id             TEXT PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('catalog', 'export', 'model')),
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
INSERT INTO jobs_v2 SELECT id, kind, state, stage, title, event_id, dedup_key, total, done, failed, skipped, total_known,
  recovered, blocked_reason, created_at, started_at, finished_at, updated_at FROM jobs;
DROP TABLE jobs;
ALTER TABLE jobs_v2 RENAME TO jobs;
CREATE UNIQUE INDEX ux_jobs_dedup ON jobs(dedup_key)
  WHERE state IN ('queued', 'running', 'pausing', 'paused', 'blocked', 'cancel_requested');
CREATE INDEX ix_jobs_state ON jobs(state, created_at);

INSERT INTO meta(key, value) VALUES ('identity_revision', '1') ON CONFLICT(key) DO NOTHING;
