//! ใบหน้าและการยืนยันตัวตน (Blueprint หมวด 6.2, 10.5)
//! - คำยืนยันของผู้ใช้เป็นข้อมูลหลัก: หนึ่งใบหน้ามีบุคคลที่ยืนยันได้ไม่เกินหนึ่งคน
//! - เปลี่ยนคน = เพิกถอนของเดิมแล้วสร้างใหม่ใน transaction เดียว พร้อม operation ID
//! - "ไม่ใช่บุคคลนี้" ไม่ลบกรอบใบหน้า; "ไม่ใช่ใบหน้า" เป็นคำสั่งแยก

use crate::catalog::{PersonRef, PERSON_COVER_SQL};
use crate::error::{AppError, Result};
use crate::import::{remember, replay};
use crate::{db, people, util, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceBox {
    pub id: String,
    pub bbox: BBox,
    pub origin: String,
    pub person: Option<PersonRef>,
    pub suggestion: Option<Suggestion>,
    pub record_version: i64,
}

/// ข้อเสนอของ AI สำหรับใบหน้าที่ยังไม่มีคนที่ยืนยัน — ไม่ใช่คำยืนยัน
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub person: PersonRef,
    pub level: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMutationResult {
    pub operation_id: String,
    pub faces: Vec<FaceBox>,
    pub created_person: Option<PersonRef>,
    pub data_revision: i64,
}

/// ใบหน้าที่ยังมีผลของ revision หนึ่ง พร้อมบุคคลที่ยืนยันแล้ว (ถ้ามี)
pub fn for_revision(conn: &Connection, revision_id: &str) -> Result<Vec<FaceBox>> {
    let sql = format!(
        "SELECT f.id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.origin, f.record_version, pe.id, pe.display_name, {PERSON_COVER_SQL}
         FROM faces f
         LEFT JOIN face_assignments a ON a.face_id = f.id AND a.revoked_at IS NULL
         LEFT JOIN persons pe ON pe.id = a.person_id
         WHERE f.photo_revision_id = ?1 AND f.status = 'active' ORDER BY f.bbox_x, f.id"
    );
    Ok(conn
        .prepare(&sql)?
        .query_map([revision_id], |r| {
            let person_id: Option<String> = r.get(7)?;
            Ok(FaceBox {
                id: r.get(0)?,
                bbox: BBox { x: r.get(1)?, y: r.get(2)?, w: r.get(3)?, h: r.get(4)? },
                origin: r.get(5)?,
                suggestion: None,
                record_version: r.get(6)?,
                person: match person_id {
                    Some(id) => Some(PersonRef::new(id, r.get(8)?, r.get(9)?)),
                    None => None,
                },
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// ใบหน้าของ revision พร้อมข้อเสนอ "อาจเป็น …" สำหรับใบที่ยังไม่มีชื่อ
pub fn for_revision_with_suggestions(app: &App, conn: &Connection, revision_id: &str) -> Result<Vec<FaceBox>> {
    let mut faces = for_revision(conn, revision_id)?;
    if faces.iter().any(|f| f.person.is_none()) {
        let suggestions = crate::review::suggestions_for_revision(app, conn, revision_id)?;
        for face in faces.iter_mut().filter(|f| f.person.is_none()) {
            if let Some((person_id, level)) = suggestions.get(&face.id) {
                face.suggestion = Some(Suggestion { person: crate::review::person_ref(conn, person_id)?, level });
            }
        }
    }
    Ok(faces)
}

/// ตรวจและหนีบกรอบให้อยู่ในภาพ: พิกัดเป็นสัดส่วน [0,1] บนภาพที่หมุนตาม EXIF แล้ว
fn clean_bbox(b: BBox) -> Result<BBox> {
    if ![b.x, b.y, b.w, b.h].iter().all(|v| v.is_finite()) {
        return Err(AppError::invalid("กรอบใบหน้าไม่ถูกต้อง"));
    }
    // หนีบเฉพาะส่วนที่ล้นขอบ ค่าที่อยู่ในภาพอยู่แล้วเก็บตามที่ผู้ใช้วาดทุกหลัก
    let x = b.x.clamp(0.0, 1.0);
    let y = b.y.clamp(0.0, 1.0);
    let w = (b.w - (x - b.x)).min(1.0 - x);
    let h = (b.h - (y - b.y)).min(1.0 - y);
    if w < 0.005 || h < 0.005 {
        return Err(AppError::invalid("กรอบใบหน้าเล็กเกินไป ลากกรอบให้ครอบใบหน้าอีกครั้ง"));
    }
    Ok(BBox { x, y, w, h })
}

fn record_operation(conn: &Connection, kind: &str, summary: serde_json::Value) -> Result<String> {
    let id = util::new_id();
    conn.execute(
        "INSERT INTO operations(id, kind, summary, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, kind, summary.to_string(), util::now()],
    )?;
    Ok(id)
}

/// เลือกบุคคลปลายทาง: คนเดิมตาม ID หรือสร้างคนใหม่จากชื่อ (อย่างใดอย่างหนึ่ง)
fn resolve_person(
    conn: &Connection,
    person_id: Option<&str>,
    new_name: Option<&str>,
) -> Result<(String, Option<PersonRef>)> {
    match (person_id, new_name) {
        (Some(id), None) => {
            if !conn.query_row("SELECT EXISTS(SELECT 1 FROM persons WHERE id = ?1)", [id], |r| r.get::<_, bool>(0))? {
                return Err(AppError::not_found("บุคคล"));
            }
            Ok((id.to_string(), None))
        }
        (None, Some(name)) => {
            let id = people::insert_person(conn, name)?;
            let created = PersonRef::new(id.clone(), people::clean_name(name)?, None);
            Ok((id, Some(created)))
        }
        _ => Err(AppError::invalid("เลือกบุคคลเดิม หรือพิมพ์ชื่อเพื่อสร้างบุคคลใหม่ อย่างใดอย่างหนึ่ง")),
    }
}

/// ผูกใบหน้ากับบุคคล; ถ้ามีคำยืนยันเดิมของคนอื่นให้เพิกถอนก่อน (ทำภายใน transaction ของผู้เรียก)
fn assign_one(conn: &Connection, face_id: &str, person_id: &str, origin: &str, operation_id: &str) -> Result<()> {
    let current: Option<(String, String)> = conn
        .query_row(
            "SELECT id, person_id FROM face_assignments WHERE face_id = ?1 AND revoked_at IS NULL",
            [face_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let now = util::now();
    match current {
        Some((_, existing)) if existing == person_id => return Ok(()),
        Some((assignment_id, _)) => {
            conn.execute(
                "UPDATE face_assignments SET revoked_at = ?2, revoked_operation_id = ?3 WHERE id = ?1",
                params![assignment_id, now, operation_id],
            )?;
        }
        None => {}
    }
    conn.execute(
        "INSERT INTO face_assignments(id, face_id, person_id, decision_origin, operation_id, confirmed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![util::new_id(), face_id, person_id, origin, operation_id, now],
    )?;
    conn.execute("UPDATE faces SET record_version = record_version + 1 WHERE id = ?1", [face_id])?;
    // ภาพตัวแทนเดิมที่ไม่ใช่ของคนนั้นแล้วให้กลับไปใช้ค่าอัตโนมัติ
    conn.execute(
        "UPDATE persons SET cover_face_id = NULL WHERE cover_face_id = ?1 AND id <> ?2",
        params![face_id, person_id],
    )?;
    Ok(())
}

fn result_for(
    app: &App,
    conn: &Connection,
    operation_id: String,
    revision_ids: &[String],
    created: Option<PersonRef>,
) -> Result<FaceMutationResult> {
    let mut faces = vec![];
    for rev in revision_ids {
        faces.extend(for_revision_with_suggestions(app, conn, rev)?);
    }
    Ok(FaceMutationResult { operation_id, faces, created_person: created, data_revision: db::data_revision(conn)? })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateManualArgs {
    pub request_id: String,
    pub photo_id: String,
    pub bbox: BBox,
    pub person_id: Option<String>,
    pub new_person_name: Option<String>,
}

/// "ระบุใบหน้าเอง": วาดกรอบบนภาพแล้วตั้งชื่อ — บันทึกเป็น origin = manual และยืนยันทันที
pub fn create_manual(app: &App, a: CreateManualArgs) -> Result<FaceMutationResult> {
    let result = {
        let mut conn = app.db();
        if let Some((op, rev)) = replay::<(String, String)>(&conn, &a.request_id)? {
            return result_for(app, &conn, op, &[rev], None);
        }
        let bbox = clean_bbox(a.bbox)?;
        let tx = conn.transaction()?;
        let revision_id: String = tx
            .query_row(
                "SELECT current_revision_id FROM photos WHERE id = ?1 AND catalog_state = 'active'",
                [&a.photo_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("รูป"))?;
        let (person_id, created) = resolve_person(&tx, a.person_id.as_deref(), a.new_person_name.as_deref())?;
        let face_id = util::new_id();
        let op = record_operation(
            &tx,
            "create_manual_face",
            serde_json::json!({ "faceId": face_id, "photoId": a.photo_id, "personId": person_id, "createdPerson": created.is_some() }),
        )?;
        tx.execute(
            "INSERT INTO faces(id, photo_revision_id, origin, bbox_x, bbox_y, bbox_w, bbox_h, created_at)
             VALUES (?1, ?2, 'manual', ?3, ?4, ?5, ?6, ?7)",
            params![face_id, revision_id, bbox.x, bbox.y, bbox.w, bbox.h, util::now()],
        )?;
        assign_one(&tx, &face_id, &person_id, "user_manual", &op)?;
        remember(&tx, &a.request_id, "create_manual_face", &(op.clone(), revision_id.clone()))?;
        db::bump_identity(&tx)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        result_for(app, &conn, op, &[revision_id], created)?
    };
    app.notify(&["people", "photos"]);
    Ok(result)
}

fn revisions_of(conn: &Connection, face_ids: &[String]) -> Result<Vec<String>> {
    if face_ids.is_empty() {
        return Err(AppError::invalid("ยังไม่ได้เลือกใบหน้า"));
    }
    if face_ids.len() > 500 {
        return Err(AppError::invalid("เลือกใบหน้าได้ไม่เกิน 500 ใบต่อครั้ง"));
    }
    let mut revs: Vec<String> = vec![];
    for id in face_ids {
        let rev: String = conn
            .query_row("SELECT photo_revision_id FROM faces WHERE id = ?1 AND status = 'active'", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::not_found("ใบหน้า").ids(vec![id.clone()]))?;
        if !revs.contains(&rev) {
            revs.push(rev);
        }
    }
    Ok(revs)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignArgs {
    pub request_id: String,
    pub face_ids: Vec<String>,
    pub person_id: Option<String>,
    pub new_person_name: Option<String>,
}

/// ยืนยันเฉพาะใบหน้าที่ส่งมาในคำสั่งนี้เท่านั้น
pub fn assign(app: &App, a: AssignArgs) -> Result<FaceMutationResult> {
    let result = {
        let mut conn = app.db();
        if let Some((op, revs)) = replay::<(String, Vec<String>)>(&conn, &a.request_id)? {
            return result_for(app, &conn, op, &revs, None);
        }
        let tx = conn.transaction()?;
        let revs = revisions_of(&tx, &a.face_ids)?;
        let (person_id, created) = resolve_person(&tx, a.person_id.as_deref(), a.new_person_name.as_deref())?;
        let op =
            record_operation(&tx, "assign_faces", serde_json::json!({ "faceIds": a.face_ids, "personId": person_id }))?;
        for face_id in &a.face_ids {
            assign_one(&tx, face_id, &person_id, "user_confirmed", &op)?;
        }
        remember(&tx, &a.request_id, "assign_faces", &(op.clone(), revs.clone()))?;
        db::bump_identity(&tx)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        result_for(app, &conn, op, &revs, created)?
    };
    app.notify(&["people", "photos"]);
    Ok(result)
}

fn revoke(app: &App, face_ids: &[String], kind: &str, ignore: bool) -> Result<FaceMutationResult> {
    let result = {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        let revs = revisions_of(&tx, face_ids)?;
        let op = record_operation(&tx, kind, serde_json::json!({ "faceIds": face_ids }))?;
        let now = util::now();
        for face_id in face_ids {
            tx.execute(
                "UPDATE face_assignments SET revoked_at = ?2, revoked_operation_id = ?3 WHERE face_id = ?1 AND revoked_at IS NULL",
                params![face_id, now, op],
            )?;
            tx.execute("UPDATE persons SET cover_face_id = NULL WHERE cover_face_id = ?1", [face_id])?;
            let status = if ignore { "ignored" } else { "active" };
            tx.execute(
                "UPDATE faces SET status = ?2, record_version = record_version + 1 WHERE id = ?1",
                params![face_id, status],
            )?;
        }
        db::bump_identity(&tx)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        result_for(app, &conn, op, &revs, None)?
    };
    app.notify(&["people", "photos"]);
    Ok(result)
}

/// "ไม่ใช่บุคคลนี้": เพิกถอนชื่อของใบหน้านั้น ไม่กระทบคนอื่นในภาพเดียวกัน และกรอบยังอยู่
pub fn unassign(app: &App, face_ids: &[String]) -> Result<FaceMutationResult> {
    revoke(app, face_ids, "unassign_faces", false)
}

/// "ไม่ใช่ใบหน้า": เก็บการตัดสินไว้ (status = ignored) แทนการลบแถว
pub fn ignore(app: &App, face_ids: &[String]) -> Result<FaceMutationResult> {
    revoke(app, face_ids, "ignore_faces", true)
}
