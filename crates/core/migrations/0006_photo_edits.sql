-- แต่งภาพแบบไม่แตะไฟล์ (DECISIONS ข้อ 17): ค่าที่แต่งต่อรูป (JSON ของ edit::EditSettings)
-- edit_id เปลี่ยนทุกครั้งที่บันทึก ใช้เป็นชื่อ cache ของภาพที่แต่ง (ไม่มีวันชนกับภาพที่แต่งครั้งก่อน)
CREATE TABLE photo_edits (
  photo_id   TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  edit_id    TEXT NOT NULL,
  settings   TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
