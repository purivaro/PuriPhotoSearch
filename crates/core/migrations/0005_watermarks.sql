-- ลายน้ำของสำเนาที่ส่งออก (DECISIONS ข้อ 16): โลโก้เก็บในคลังเป็น PNG (ด้านยาวไม่เกิน 2048 px)
-- อยู่ในไฟล์คลังเอง ชุดสำรองของคลังจึงมีครบโดยไม่ต้องตามไฟล์ข้างนอก
CREATE TABLE watermarks (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  width       INTEGER NOT NULL,
  height      INTEGER NOT NULL,
  transparent INTEGER NOT NULL,
  image       BLOB NOT NULL,
  created_at  TEXT NOT NULL
);

-- ลายน้ำของงานส่งออก (JSON ของ watermark::WatermarkSpec, NULL = ไม่ใส่)
ALTER TABLE export_runs ADD COLUMN watermark TEXT;
