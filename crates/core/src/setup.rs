//! สิ่งที่แอปเสนอให้ทำตอนเปิดครั้งแรก (S01): ดาวน์โหลดตัววิเคราะห์ใบหน้า และนำรายชื่อจากรุ่น Python เข้าคลัง
//!
//! สถานะของตัววิเคราะห์ใบหน้าอยู่ใน `get_capabilities` แล้ว ที่นี่ตอบเฉพาะส่วนที่เหลือ:
//! มีรายชื่อของรุ่นเดิมรอนำเข้ากี่คน และผู้ใช้ปิดข้อเสนอนี้ไปแล้วหรือยัง (จำไว้กับคลัง ไม่ถามซ้ำทุกครั้งที่เปิดแอป)

use crate::error::Result;
use crate::{legacy, App};
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    /// บุคคลในฐานของรุ่น Python บนเครื่องนี้ที่ยังไม่ได้นำเข้า (0 = ไม่พบฐานเดิม หรือนำเข้าครบแล้ว)
    pub legacy_pending: i64,
    /// ผู้ใช้ปิดหน้าต่างข้อเสนอแล้ว: ไม่เปิดเองอีก
    pub dismissed: bool,
}

pub fn state(app: &App) -> Result<SetupState> {
    let dismissed = app
        .db()
        .query_row("SELECT 1 FROM ui_preferences WHERE key = 'setup_dismissed'", [], |_| Ok(()))
        .optional()?
        .is_some();
    // ไม่พบหรืออ่านฐานเดิมไม่ได้ = ไม่มีอะไรให้เสนอ ไม่ใช่ข้อผิดพลาดของแอปนี้
    let legacy_pending = legacy::pending_people(app, None).unwrap_or(0);
    Ok(SetupState { legacy_pending, dismissed })
}

pub fn dismiss(app: &App) -> Result<()> {
    app.db().execute("INSERT OR IGNORE INTO ui_preferences(key, value) VALUES ('setup_dismissed', '1')", [])?;
    Ok(())
}
