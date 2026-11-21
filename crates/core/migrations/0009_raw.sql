-- ไฟล์ RAW จากกล้อง (DECISIONS ข้อ 25): นำเข้าเฉพาะกิจกรรมที่ผู้ใช้เลือกให้รวม RAW ตอนเพิ่มโฟลเดอร์
-- กิจกรรมเดิมเป็น 0 — ตรวจรูปใหม่แล้วไม่ได้ RAW ซ้ำกับ JPEG ที่ export ไว้โดยไม่ได้ตั้งใจ
ALTER TABLE event_source_bindings ADD COLUMN include_raw INTEGER NOT NULL DEFAULT 0;
