-- schema v12 (หลวงพี่อนุมัติ 22 พ.ย. 2569)

-- 1) AI ช่วยคัดภาพ (DECISIONS ข้อ 37): การคัดจำว่าใครคัด — ปรับระดับใหม่แล้ว AI แก้เฉพาะรูปที่ AI คัดเอง ไม่แตะรูปที่ผู้ใช้คัด
--    และบอกเหตุผลบนรูปได้: best = ดีที่สุดของช่วงนั้น · blur = เบลอ · duplicate = ซ้ำกับ reason_photo_id (การคัดของผู้ใช้ไม่มีเหตุผล)
ALTER TABLE photo_picks ADD COLUMN source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('user', 'ai'));
ALTER TABLE photo_picks ADD COLUMN reason TEXT CHECK (reason IN ('best', 'blur', 'duplicate'));
ALTER TABLE photo_picks ADD COLUMN reason_photo_id TEXT;

-- ผลวัดของรูปที่ใช้คัด (ความคม ความสว่าง ภาพย่อสำหรับเทียบความเหมือน) ต่อ revision — คำนวณใหม่ได้ ไม่ใช่ข้อมูลหลัก
-- version = รุ่นของสูตรวัด (`cull::VERSION`) รุ่นไม่ตรง = วัดใหม่
CREATE TABLE photo_quality (
  revision_id    TEXT PRIMARY KEY REFERENCES photo_revisions(id) ON DELETE CASCADE,
  version        INTEGER NOT NULL,
  sharpness      REAL NOT NULL,
  face_sharpness REAL,
  face_detail    REAL,
  faces          INTEGER NOT NULL,
  brightness     REAL NOT NULL,
  clipped        REAL NOT NULL,
  portrait       INTEGER NOT NULL CHECK (portrait IN (0, 1)),
  thumb          BLOB NOT NULL,
  colors         BLOB NOT NULL,
  measured_at    TEXT NOT NULL
) WITHOUT ROWID;

-- jobs: เพิ่มชนิด 'cull' = วัดรูปของกิจกรรมเพื่อคัด (SQLite แก้ CHECK ไม่ได้ จึงสร้างตารางใหม่แล้วย้ายแถว แบบเดียวกับ 0004)
CREATE TABLE jobs_v4 (
  id             TEXT PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('catalog', 'export', 'model', 'shrink', 'cull')),
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
INSERT INTO jobs_v4 SELECT id, kind, state, stage, title, event_id, dedup_key, total, done, failed, skipped, total_known,
  recovered, blocked_reason, created_at, started_at, finished_at, updated_at FROM jobs;
DROP TABLE jobs;
ALTER TABLE jobs_v4 RENAME TO jobs;
CREATE UNIQUE INDEX ux_jobs_dedup ON jobs(dedup_key)
  WHERE state IN ('queued', 'running', 'pausing', 'paused', 'blocked', 'cancel_requested');
CREATE INDEX ix_jobs_state ON jobs(state, created_at);

-- 2) preset พา Mask ไปด้วย: JSON ของ `Vec<masks::Mask>` — Mask ตัวคนใน preset จาก Lightroom ใช้กับรูปไหนก็หาตัวคนของรูปนั้นเอง
ALTER TABLE edit_presets ADD COLUMN masks TEXT NOT NULL DEFAULT '[]';
