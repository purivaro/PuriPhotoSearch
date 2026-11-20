-- ย่อไฟล์ต้นฉบับในที่เดิม (DECISIONS ข้อ 15): แทนไฟล์ด้วยไฟล์ที่ย่อแล้ว ต้นฉบับขนาดเต็มไปถังขยะ
-- ปรับขนาด/hash ของ revision เดิมแทนการสร้าง revision ใหม่ ใบหน้า เวกเตอร์ และคำตัดสินของผู้ใช้จึงอยู่ครบ

-- jobs: เพิ่มชนิด 'shrink' (SQLite แก้ CHECK ไม่ได้ จึงสร้างตารางใหม่แล้วย้ายแถว แบบเดียวกับ 0002)
CREATE TABLE jobs_v3 (
  id             TEXT PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('catalog', 'export', 'model', 'shrink')),
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
INSERT INTO jobs_v3 SELECT id, kind, state, stage, title, event_id, dedup_key, total, done, failed, skipped, total_known,
  recovered, blocked_reason, created_at, started_at, finished_at, updated_at FROM jobs;
DROP TABLE jobs;
ALTER TABLE jobs_v3 RENAME TO jobs;
CREATE UNIQUE INDEX ux_jobs_dedup ON jobs(dedup_key)
  WHERE state IN ('queued', 'running', 'pausing', 'paused', 'blocked', 'cancel_requested');
CREATE INDEX ix_jobs_state ON jobs(state, created_at);

CREATE TABLE shrink_runs (
  id         TEXT PRIMARY KEY,
  job_id     TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  resize     TEXT NOT NULL,
  created_at TEXT NOT NULL
);

-- status: planned → replacing (บันทึกก่อนแตะไฟล์ เผื่อแอปปิดกลางคัน) → shrunk
--         หรือ skipped (เล็กพอแล้ว/ย่อไม่ได้ประโยชน์ เหตุผลใน note) / unavailable / failed / cancelled
CREATE TABLE shrink_items (
  run_id       TEXT NOT NULL REFERENCES shrink_runs(id) ON DELETE CASCADE,
  ord          INTEGER NOT NULL,
  photo_id     TEXT NOT NULL,
  revision_id  TEXT NOT NULL,
  filename     TEXT NOT NULL,
  path         TEXT NOT NULL,
  status       TEXT NOT NULL
    CHECK (status IN ('planned', 'replacing', 'shrunk', 'skipped', 'unavailable', 'failed', 'cancelled')),
  note         TEXT,
  old_size     INTEGER NOT NULL,
  new_size     INTEGER,
  temp_path    TEXT,
  new_sha256   TEXT,
  new_width    INTEGER,
  new_height   INTEGER,
  trashed_path TEXT,
  PRIMARY KEY (run_id, ord)
) WITHOUT ROWID;
CREATE INDEX ix_shrink_replacing ON shrink_items(status) WHERE status = 'replacing';

-- ขนาดต้นฉบับ ณ ตอนส่งออก: รายงานเก่ายังถูกแม้ภายหลังต้นฉบับถูกย่อในที่เดิม
ALTER TABLE export_items ADD COLUMN source_bytes INTEGER;
