-- คู่ RAW + JPEG แบบ Lightroom (DECISIONS ข้อ 26): ไฟล์ RAW ที่ JPEG ของรูปส่งออกมา (Lightroom บอกชื่อไว้ใน XMP)
-- หรือชื่อเดียวกันในโฟลเดอร์เดียวกัน — ไม่เป็นรูปแยกในคลัง ใช้ล้างภาพใหม่ตอนแต่ง (ไฟล์ RAW ไม่ถูกแตะ)
-- lightroom: JSON ของ pair::Lightroom (กรอบ + ค่าที่แต่งใน Lightroom) · NULL = JPEG จากกล้อง (ไม่มีค่าของ Lightroom)
-- start: ค่าเริ่มต้นเมื่อแต่งจาก RAW (คิดจากไฟล์ RAW ครั้งแรกที่ต้องใช้) · NULL = ยังไม่ได้คิด
CREATE TABLE photo_raw_pairs (
  photo_id    TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  os_path     TEXT NOT NULL,
  path_key    TEXT NOT NULL,
  filename    TEXT NOT NULL,
  lightroom   TEXT,
  start       TEXT,
  available   INTEGER NOT NULL DEFAULT 1,
  verified_at TEXT
);
CREATE INDEX ix_raw_pairs_path ON photo_raw_pairs(path_key);
