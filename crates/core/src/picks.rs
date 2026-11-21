//! คัดก่อนแต่ง (DECISIONS ข้อ 33): "เลือก" / "ไม่เอา" แบบ Pick/Reject ของ Lightroom
//! เป็นข้อมูลของรูปในคลัง — ไฟล์ไม่ถูกแตะ "ไม่เอา" ไม่ลบหรือย้ายไฟล์ ใช้กรองเท่านั้น

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::export::SelectionSpec;
use crate::{db, util, App};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flag {
    Pick,
    Reject,
}

impl Flag {
    fn as_str(self) -> &'static str {
        match self {
            Flag::Pick => "pick",
            Flag::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagResult {
    /// รูปที่สถานะเปลี่ยน
    pub updated: usize,
}

/// ตั้งการคัดให้รูปที่เลือก (`None` = ล้างกลับเป็นยังไม่คัด)
pub fn set(app: &App, selection: &SelectionSpec, flag: Option<Flag>) -> Result<FlagResult> {
    let mut updated = 0;
    {
        let conn = app.db();
        let rows = crate::export::load_items(&conn, selection)?;
        if rows.is_empty() {
            return Err(AppError::invalid("ยังไม่ได้เลือกรูป"));
        }
        let tx = conn.unchecked_transaction()?;
        let now = util::now();
        for row in &rows {
            updated += match flag {
                None => tx.execute("DELETE FROM photo_picks WHERE photo_id = ?1", [&row.photo_id])?,
                // ผู้ใช้คัดเอง = เป็นของผู้ใช้ (แม้ค่าเดียวกับที่ AI คัดไว้) — AI ปรับระดับใหม่จะไม่แตะรูปนี้อีก (DECISIONS ข้อ 37)
                Some(flag) => tx.execute(
                    "INSERT INTO photo_picks(photo_id, flag, updated_at, source) VALUES (?1, ?2, ?3, 'user')
                     ON CONFLICT(photo_id) DO UPDATE SET flag = excluded.flag, updated_at = excluded.updated_at,
                       source = 'user', reason = NULL, reason_photo_id = NULL
                     WHERE photo_picks.flag <> excluded.flag OR photo_picks.source <> 'user'",
                    params![row.photo_id, flag.as_str(), now],
                )?,
            };
        }
        if updated > 0 {
            db::bump_revision(&tx)?;
        }
        tx.commit()?;
    }
    if updated > 0 {
        app.notify(&["photos"]);
    }
    Ok(FlagResult { updated })
}
