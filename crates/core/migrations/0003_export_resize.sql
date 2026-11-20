-- ส่งออกแบบย่อขนาด: เก็บวิธีย่อของแต่ละงาน (JSON ของ export::Resize, NULL = คัดลอกต้นฉบับ)
-- และขนาดไฟล์ที่เขียนจริงของแต่ละรูป เพื่อรายงานว่าลดขนาดไปเท่าไร
ALTER TABLE export_runs ADD COLUMN resize TEXT;
ALTER TABLE export_items ADD COLUMN output_bytes INTEGER;
