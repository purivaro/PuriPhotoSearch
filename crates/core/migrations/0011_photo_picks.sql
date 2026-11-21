-- คัดก่อนแต่ง (DECISIONS ข้อ 33): รูปที่ "เลือก" หรือ "ไม่เอา" แบบ Pick/Reject ของ Lightroom — ข้อมูลของรูปในคลัง ไฟล์ไม่ถูกแตะ
-- ไม่มีแถว = ยังไม่คัด · "ไม่เอา" ไม่ลบหรือย้ายไฟล์ ใช้กรองและซ่อนเท่านั้น
CREATE TABLE photo_picks (
  photo_id   TEXT PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  flag       TEXT NOT NULL CHECK (flag IN ('pick', 'reject')),
  updated_at TEXT NOT NULL
);
