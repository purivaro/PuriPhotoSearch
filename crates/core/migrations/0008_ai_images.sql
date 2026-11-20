-- ภาพที่ผู้ช่วย AI สร้างจากรูปในคลัง (DECISIONS ข้อ 23): ผูกกับไฟล์ที่ได้ด้วย SHA-256 ของไฟล์ (ย้ายโฟลเดอร์แล้วยังรู้ที่มา)
-- source_photo_id อาจเป็นรูปที่ถูกลบจากคลังไปแล้ว จึงเก็บชื่อไฟล์ต้นทางไว้ด้วย
CREATE TABLE ai_images (
  sha256          TEXT PRIMARY KEY,
  source_photo_id TEXT,
  source_filename TEXT NOT NULL,
  prompt          TEXT NOT NULL,
  template        TEXT,
  provider        TEXT NOT NULL,
  model           TEXT,
  created_at      TEXT NOT NULL
);
