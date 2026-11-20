-- preset ของการแต่งภาพ (DECISIONS ข้อ 20): ค่าแสงและสี (JSON ของ adjust::Adjust)
-- source: user = บันทึกจากแอป · lightroom = นำเข้าจาก .xmp
-- unsupported = ส่วนของ preset ที่นำเข้าไม่ได้ (JSON array ของข้อความ แสดงให้ผู้ใช้เห็นตามจริง)
CREATE TABLE edit_presets (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  adjust      TEXT NOT NULL,
  source      TEXT NOT NULL CHECK (source IN ('user', 'lightroom')),
  unsupported TEXT NOT NULL DEFAULT '[]',
  created_at  TEXT NOT NULL
);
