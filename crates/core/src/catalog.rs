//! กิจกรรม รูป และการค้นหา (Blueprint หมวด 11)

use crate::error::{AppError, Result};
use crate::faces::{self, FaceBox};
use crate::jobs::{self, JobSnapshot, ACTIVE_STATES};
use crate::people::{self, PersonSummary};
use crate::{db, util, App};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonRef {
    pub id: String,
    pub display_name: String,
    pub cover_face_id: Option<String>,
    pub cover_asset: Option<String>,
}

impl PersonRef {
    /// `cover` คือค่าจาก [`PERSON_COVER_SQL`]
    pub fn new(id: String, display_name: String, cover: Option<String>) -> Self {
        let cover_face_id = cover.as_deref().and_then(|c| c.strip_prefix("face/")).map(str::to_string);
        Self { id, display_name, cover_face_id, cover_asset: cover }
    }
}

/// ภาพตัวแทนของบุคคลเป็น path ของ asset: ใบหน้าที่ผู้ใช้เลือก → ใบหน้ายืนยันใบแรก → ใบหน้าตัวอย่างที่นำเข้า
pub const PERSON_COVER_SQL: &str = "COALESCE(\
    'face/' || COALESCE(pe.cover_face_id, (SELECT MIN(v.face_id) FROM v_confirmed v WHERE v.person_id = pe.id)), \
    'sample/' || (SELECT MIN(s.id) FROM person_reference_samples s WHERE s.person_id = pe.id))";

/// เงื่อนไข "รูปนี้มีใบหน้าที่ยืนยันแล้วของ …" เมื่อมองจากแถวรูป (ความหมายเดียวกับ view v_confirmed)
const CONFIRMED_ON_PHOTO: &str = "FROM faces f JOIN face_assignments a ON a.face_id = f.id AND a.revoked_at IS NULL \
                                  WHERE f.photo_revision_id = r.id AND f.status = 'active'";
/// เงื่อนไข "รูปนี้มีใบหน้าที่ AI จัดให้ …" (ตารางชั่วคราวที่ `review::ensure_matches` เติม)
const AI_ON_PHOTO: &str = "FROM temp.ai_matches m WHERE m.revision_id = r.id";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummary {
    pub library_id: String,
    pub name: String,
    pub event_count: i64,
    pub photo_count: i64,
    pub person_count: i64,
    pub unnamed_face_count: i64,
    pub data_revision: i64,
}

pub fn library_summary(conn: &Connection) -> Result<LibrarySummary> {
    Ok(LibrarySummary {
        library_id: db::meta_get(conn, "library_id")?.unwrap_or_default(),
        name: db::meta_get(conn, "name")?.unwrap_or_default(),
        event_count: conn.query_row("SELECT COUNT(*) FROM events WHERE catalog_state = 'active'", [], |r| r.get(0))?,
        photo_count: conn.query_row(
            "SELECT COUNT(*) FROM photos p JOIN event_photos ep ON ep.photo_id = p.id JOIN events e ON e.id = ep.event_id
             WHERE p.catalog_state = 'active' AND e.catalog_state = 'active'",
            [],
            |r| r.get(0),
        )?,
        person_count: conn.query_row("SELECT COUNT(*) FROM persons WHERE catalog_state = 'active'", [], |r| r.get(0))?,
        unnamed_face_count: conn.query_row(
            "SELECT COUNT(*) FROM faces f
             JOIN photos p ON p.current_revision_id = f.photo_revision_id AND p.catalog_state = 'active'
             JOIN event_photos ep ON ep.photo_id = p.id JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
             WHERE f.status = 'active'
               AND NOT EXISTS (SELECT 1 FROM face_assignments a WHERE a.face_id = f.id AND a.revoked_at IS NULL)",
            [],
            |r| r.get(0),
        )?,
        data_revision: db::data_revision(conn)?,
    })
}

// ───────────────────────── กิจกรรม ─────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    pub id: String,
    pub title: String,
    pub event_date: Option<String>,
    pub date_source: String,
    pub photo_count: i64,
    pub thumb_ready_count: i64,
    pub failed_count: i64,
    pub face_analyzed_count: i64,
    pub cover_photo_id: Option<String>,
    pub source_available: bool,
    pub folder_path: String,
    pub active_job_id: Option<String>,
    pub created_at: String,
    pub record_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonCount {
    pub person: PersonRef,
    pub photo_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDetail {
    #[serde(flatten)]
    pub summary: EventSummary,
    pub notes: String,
    pub people: Vec<PersonCount>,
    pub jobs: Vec<JobSnapshot>,
}

fn event_select() -> String {
    let count = |cond: &str| {
        format!(
            "(SELECT COUNT(*) FROM event_photos ep JOIN photos p ON p.id = ep.photo_id
              JOIN photo_revisions r ON r.id = p.current_revision_id
              WHERE ep.event_id = e.id AND p.catalog_state = 'active'{cond})"
        )
    };
    format!(
        "SELECT e.id, e.title, e.event_date, e.date_source, {total}, {ready}, {failed}, e.cover_photo_id,
           COALESCE((SELECT b.folder_path FROM event_source_bindings b WHERE b.event_id = e.id AND b.active = 1 LIMIT 1), ''),
           (SELECT j.id FROM jobs j WHERE j.event_id = e.id AND j.state IN {ACTIVE_STATES} ORDER BY j.created_at LIMIT 1),
           e.created_at, e.record_version, e.notes, {analyzed}
         FROM events e WHERE e.catalog_state = 'active'",
        total = count(""),
        ready = count(" AND r.thumbnail_status = 'ready'"),
        failed = count(" AND r.thumbnail_status = 'failed'"),
        analyzed = count(" AND r.face_status = 'done'"),
    )
}

fn event_from_row(r: &rusqlite::Row) -> rusqlite::Result<(EventSummary, String)> {
    let folder_path: String = r.get(8)?;
    Ok((
        EventSummary {
            id: r.get(0)?,
            title: r.get(1)?,
            event_date: r.get(2)?,
            date_source: r.get(3)?,
            photo_count: r.get(4)?,
            thumb_ready_count: r.get(5)?,
            failed_count: r.get(6)?,
            face_analyzed_count: r.get(13)?,
            cover_photo_id: r.get(7)?,
            // ตรวจสด: ไดรฟ์หลุดแล้วต่อกลับ สถานะต้องตามจริงโดยไม่ต้องสแกนใหม่
            source_available: !folder_path.is_empty() && Path::new(&folder_path).is_dir(),
            folder_path,
            active_job_id: r.get(9)?,
            created_at: r.get(10)?,
            record_version: r.get(11)?,
        },
        r.get(12)?,
    ))
}

/// เรียงวันกิจกรรมล่าสุดก่อน; งานไม่ทราบวันที่อยู่ท้ายเสมอ และ ID เป็นตัวตัดสินเมื่อเท่ากัน
pub fn list_events(conn: &Connection, text: Option<&str>) -> Result<Vec<EventSummary>> {
    let mut sql = event_select();
    let mut args: Vec<Value> = vec![];
    if let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) {
        sql.push_str(" AND e.title_key LIKE ?1 ESCAPE '\\'");
        args.push(Value::Text(util::like_pattern(t)));
    }
    sql.push_str(" ORDER BY e.event_date IS NULL, e.event_date DESC, e.created_at DESC, e.id DESC");
    let rows =
        conn.prepare(&sql)?.query_map(params_from_iter(args), event_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub fn get_event(conn: &Connection, event_id: &str) -> Result<EventDetail> {
    let (summary, notes) = conn
        .query_row(&format!("{} AND e.id = ?1", event_select()), [event_id], event_from_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("กิจกรรม"))?;
    let people = conn
        .prepare(&format!(
            "SELECT pe.id, pe.display_name, {PERSON_COVER_SQL}, COUNT(DISTINCT v.photo_id) AS n
             FROM v_confirmed v JOIN persons pe ON pe.id = v.person_id
             WHERE v.event_id = ?1 AND pe.catalog_state = 'active'
             GROUP BY pe.id ORDER BY n DESC, pe.name_key, pe.id"
        ))?
        .query_map([event_id], |r| {
            Ok(PersonCount { person: PersonRef::new(r.get(0)?, r.get(1)?, r.get(2)?), photo_count: r.get(3)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(EventDetail { summary, notes, people, jobs: jobs::list_for_event(conn, event_id)? })
}

/// แยก "ไม่ส่ง field" (ไม่แก้) ออกจาก "ส่ง null" (ล้างค่า)
fn double_option<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::deserialize(d)?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEventArgs {
    pub event_id: String,
    pub expected_version: i64,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub event_date: Option<Option<String>>,
    pub notes: Option<String>,
}

/// แก้ข้อมูลกิจกรรมในแอปเท่านั้น: ไม่เปลี่ยนชื่อโฟลเดอร์จริง และไม่แตะ EXIF
pub fn update_event(app: &App, a: UpdateEventArgs) -> Result<EventDetail> {
    let detail = {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        let current: i64 = tx
            .query_row(
                "SELECT record_version FROM events WHERE id = ?1 AND catalog_state = 'active'",
                [&a.event_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("กิจกรรม"))?;
        if current != a.expected_version {
            return Err(AppError::version_conflict());
        }
        if let Some(title) = &a.title {
            let title = title.trim();
            if title.is_empty() || title.chars().count() > 200 {
                return Err(AppError::invalid("ชื่อกิจกรรมต้องไม่ว่างและยาวไม่เกิน 200 ตัวอักษร"));
            }
            tx.execute(
                "UPDATE events SET title = ?2, title_key = ?3 WHERE id = ?1",
                params![a.event_id, title, util::text_key(title)],
            )?;
        }
        if let Some(date) = &a.event_date {
            if date.as_deref().is_some_and(|d| !util::is_valid_date(d)) {
                return Err(AppError::invalid("วันที่กิจกรรมไม่ถูกต้อง"));
            }
            let source = if date.is_some() { "user" } else { "none" };
            tx.execute(
                "UPDATE events SET event_date = ?2, date_source = ?3 WHERE id = ?1",
                params![a.event_id, date, source],
            )?;
        }
        if let Some(notes) = &a.notes {
            if notes.chars().count() > 5000 {
                return Err(AppError::invalid("หมายเหตุยาวได้ไม่เกิน 5,000 ตัวอักษร"));
            }
            tx.execute("UPDATE events SET notes = ?2 WHERE id = ?1", params![a.event_id, notes])?;
        }
        tx.execute("UPDATE events SET record_version = record_version + 1 WHERE id = ?1", [&a.event_id])?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        get_event(&conn, &a.event_id)?
    };
    app.notify(&["events"]);
    Ok(detail)
}

pub fn set_event_cover(app: &App, event_id: &str, photo_id: &str) -> Result<EventDetail> {
    let detail = {
        let conn = app.db();
        let n = conn.execute(
            "UPDATE events SET cover_photo_id = ?2, record_version = record_version + 1
             WHERE id = ?1 AND EXISTS(SELECT 1 FROM event_photos WHERE event_id = ?1 AND photo_id = ?2)",
            params![event_id, photo_id],
        )?;
        if n == 0 {
            return Err(AppError::invalid("เลือกภาพปกได้เฉพาะรูปที่อยู่ในกิจกรรมนี้"));
        }
        db::bump_revision(&conn)?;
        get_event(&conn, event_id)?
    };
    app.notify(&["events"]);
    Ok(detail)
}

// ───────────────────────── รูป / ค้นหา ─────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateFilter {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default)]
    pub include_unknown: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchParams {
    pub text: Option<String>,
    pub person_ids: Vec<String>,
    pub person_mode: Option<String>,
    pub event_ids: Vec<String>,
    pub date: Option<DateFilter>,
    pub face_filter: Option<String>,
    pub sort: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    /// นับรูปที่ AI จัดให้บุคคลนั้นด้วย (ยังไม่มีคนยืนยัน); ผู้เรียกต้องเรียก `review::ensure_matches` ก่อน
    pub include_suggested: bool,
}

/// บุคคลที่อยู่ในรูปตามที่ AI จัดให้ พร้อมใบหน้าที่เป็นเหตุ (ใช้แจ้งว่า "ไม่ใช่คนนี้")
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedPerson {
    pub person: PersonRef,
    pub face_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoResult {
    pub id: String,
    pub revision_id: String,
    pub filename: String,
    pub event_id: String,
    pub event_title: String,
    pub taken_at: Option<String>,
    pub taken_source: String,
    pub width: i64,
    pub height: i64,
    pub thumb_status: String,
    pub available: bool,
    pub confirmed_people: Vec<PersonRef>,
    /// บุคคลที่ AI จัดให้ในรูปนี้ (มีเมื่อค้นแบบรวมรูปที่ AI จัดให้)
    pub suggested_people: Vec<SuggestedPerson>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub eligible_photos: i64,
    pub thumbnails_ready: i64,
    pub thumbnails_pending: i64,
    pub thumbnails_failed: i64,
    pub with_confirmed_people: i64,
    pub faces_analyzed: i64,
    pub faces_pending: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPage {
    pub items: Vec<PhotoResult>,
    pub next_cursor: Option<String>,
    pub total: i64,
    pub coverage: Coverage,
    pub data_revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDetails {
    #[serde(flatten)]
    pub photo: PhotoResult,
    pub path: String,
    pub file_size: i64,
    pub event_date: Option<String>,
    pub original_available: bool,
    pub decode_error: Option<String>,
    pub faces: Vec<FaceBox>,
}

const PHOTO_FROM: &str = "FROM photos p
  JOIN photo_revisions r ON r.id = p.current_revision_id
  JOIN photo_locations l ON l.photo_id = p.id
  JOIN event_photos ep ON ep.photo_id = p.id
  JOIN events e ON e.id = ep.event_id
  WHERE p.catalog_state = 'active' AND e.catalog_state = 'active'";

const PHOTO_COLS: &str = "p.id, r.id, l.filename, e.id, e.title, r.taken_at_local, r.date_source, r.width, r.height, \
                          r.thumbnail_status, l.available, l.os_path, r.size, e.event_date, r.decode_error";

fn photo_from_row(r: &rusqlite::Row) -> rusqlite::Result<PhotoResult> {
    Ok(PhotoResult {
        id: r.get(0)?,
        revision_id: r.get(1)?,
        filename: r.get(2)?,
        event_id: r.get(3)?,
        event_title: r.get(4)?,
        taken_at: r.get(5)?,
        taken_source: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        thumb_status: r.get(9)?,
        available: r.get(10)?,
        confirmed_people: vec![],
        suggested_people: vec![],
    })
}

struct Filter {
    sql: String,
    args: Vec<Value>,
}

impl Filter {
    fn arg(&mut self, v: impl Into<String>) -> String {
        self.args.push(Value::Text(v.into()));
        format!("?{}", self.args.len())
    }
}

fn dedup(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// แปลงตัวกรองเป็น SQL: ตัวกรองต่างประเภทใช้ AND; หลายกิจกรรมใช้ OR ภายในชุด (Blueprint 11.3)
fn build_filter(p: &SearchParams) -> Result<Filter> {
    let mut f = Filter { sql: String::new(), args: vec![] };

    if let Some(text) = p.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        // ข้อความดิบค้นแบบ substring จากชื่อไฟล์ ชื่อกิจกรรม และชื่อ/ชื่ออื่นของคนที่ยืนยันแล้ว — ไม่ตีความเป็นคำสั่ง
        let pat = f.arg(util::like_pattern(text));
        let named = format!(
            "SELECT pe.id FROM persons pe WHERE pe.name_key LIKE {pat} ESCAPE '\\' OR pe.nickname LIKE {pat} ESCAPE '\\'
             UNION SELECT pa.person_id FROM person_aliases pa WHERE pa.alias_key LIKE {pat} ESCAPE '\\'"
        );
        let by_ai = if p.include_suggested {
            format!(" OR EXISTS (SELECT 1 {AI_ON_PHOTO} AND m.person_id IN ({named}))")
        } else {
            String::new()
        };
        f.sql.push_str(&format!(
            " AND (l.filename_key LIKE {pat} ESCAPE '\\' OR e.title_key LIKE {pat} ESCAPE '\\'
               OR EXISTS (SELECT 1 {CONFIRMED_ON_PHOTO} AND a.person_id IN ({named})){by_ai})"
        ));
    }

    let persons = dedup(&p.person_ids);
    if persons.len() > 20 {
        return Err(AppError::invalid("เลือกบุคคลได้ไม่เกิน 20 คนต่อการค้นหา"));
    }
    if !persons.is_empty() {
        // "รูปนี้มีคนในกลุ่มนี้": ยืนยันแล้ว หรือ (เมื่อขอรวม) AI จัดให้
        let has_any = |f: &mut Filter, group: &[String]| {
            let list = group.iter().map(|id| f.arg(id.clone())).collect::<Vec<_>>().join(", ");
            let confirmed = format!("EXISTS (SELECT 1 {CONFIRMED_ON_PHOTO} AND a.person_id IN ({list}))");
            if p.include_suggested {
                format!("({confirmed} OR EXISTS (SELECT 1 {AI_ON_PHOTO} AND m.person_id IN ({list})))")
            } else {
                confirmed
            }
        };
        if p.person_mode.as_deref() == Some("any") {
            // มีคนใดคนหนึ่ง: P ∩ C(photo) ≠ ∅
            let cond = has_any(&mut f, &persons);
            f.sql.push_str(&format!(" AND {cond}"));
        } else {
            // มีทุกคนในภาพเดียวกัน: P ⊆ C(photo)
            for id in &persons {
                let cond = has_any(&mut f, std::slice::from_ref(id));
                f.sql.push_str(&format!(" AND {cond}"));
            }
        }
    }

    let events = dedup(&p.event_ids);
    if events.len() > 500 {
        return Err(AppError::invalid("เลือกกิจกรรมมากเกินไปสำหรับการค้นหาครั้งเดียว"));
    }
    if !events.is_empty() {
        let list: Vec<String> = events.iter().map(|id| f.arg(id.clone())).collect();
        f.sql.push_str(&format!(" AND e.id IN ({})", list.join(", ")));
    }

    if let Some(d) = &p.date {
        let col = match d.field.as_str() {
            "photo_taken" => "substr(r.taken_at_local, 1, 10)",
            "event_date" => "e.event_date",
            _ => return Err(AppError::invalid("ชนิดวันที่ไม่ถูกต้อง")),
        };
        let mut parts = vec![];
        for (value, op) in [(&d.from, ">="), (&d.to, "<=")] {
            if let Some(v) = value.as_deref().filter(|v| !v.is_empty()) {
                if !util::is_valid_date(v) {
                    return Err(AppError::invalid("ช่วงวันที่ไม่ถูกต้อง"));
                }
                let arg = f.arg(v);
                parts.push(format!("{col} {op} {arg}"));
            }
        }
        match (parts.is_empty(), d.include_unknown) {
            (true, true) => {}
            (true, false) => f.sql.push_str(&format!(" AND {col} IS NOT NULL")),
            // รูปไม่ทราบวันที่ไม่ถูกแทรกเข้าช่วงเงียบ ๆ: รวมเฉพาะเมื่อผู้ใช้เลือก
            (false, true) => f.sql.push_str(&format!(" AND (({}) OR {col} IS NULL)", parts.join(" AND "))),
            (false, false) => f.sql.push_str(&format!(" AND ({})", parts.join(" AND "))),
        }
    }

    match p.face_filter.as_deref().unwrap_or("any") {
        "any" => {}
        "has_faces" => f
            .sql
            .push_str(" AND EXISTS (SELECT 1 FROM faces f WHERE f.photo_revision_id = r.id AND f.status = 'active')"),
        "no_person" => f.sql.push_str(&format!(" AND NOT EXISTS (SELECT 1 {CONFIRMED_ON_PHOTO})")),
        _ => return Err(AppError::invalid("ตัวกรองใบหน้าไม่ถูกต้อง")),
    }
    Ok(f)
}

fn order_sql(sort: Option<&str>) -> Result<&'static str> {
    Ok(match sort.unwrap_or("taken_desc") {
        "taken_desc" => "r.taken_at_local IS NULL, r.taken_at_local DESC, l.filename_key, p.id",
        "taken_asc" => "r.taken_at_local IS NULL, r.taken_at_local ASC, l.filename_key, p.id",
        "added_desc" => "p.created_at DESC, p.id DESC",
        "filename" => "l.filename_key, p.id",
        _ => return Err(AppError::invalid("วิธีเรียงไม่ถูกต้อง")),
    })
}

fn attach_people(conn: &Connection, items: &mut [PhotoResult]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let marks = vec!["?"; items.len()].join(", ");
    let sql = format!(
        "SELECT DISTINCT v.revision_id, pe.id, pe.display_name, {PERSON_COVER_SQL}
         FROM v_confirmed v JOIN persons pe ON pe.id = v.person_id
         WHERE v.revision_id IN ({marks}) ORDER BY pe.name_key, pe.id"
    );
    let mut by_rev: HashMap<String, Vec<PersonRef>> = HashMap::new();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(items.iter().map(|i| i.revision_id.clone())), |r| {
        Ok((r.get::<_, String>(0)?, PersonRef::new(r.get(1)?, r.get(2)?, r.get(3)?)))
    })?;
    for row in rows {
        let (rev, person) = row?;
        by_rev.entry(rev).or_default().push(person);
    }
    for item in items {
        item.confirmed_people = by_rev.remove(&item.revision_id).unwrap_or_default();
    }
    Ok(())
}

/// ใส่ว่าในรูปมีใครตามที่ AI จัดให้ (ไม่ใช่คำยืนยัน)
fn attach_suggested(conn: &Connection, items: &mut [PhotoResult]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let marks = vec!["?"; items.len()].join(", ");
    let sql = format!(
        "SELECT m.revision_id, m.face_id, pe.id, pe.display_name, {PERSON_COVER_SQL}
         FROM temp.ai_matches m JOIN persons pe ON pe.id = m.person_id
         WHERE m.revision_id IN ({marks}) ORDER BY pe.name_key, pe.id, m.face_id"
    );
    let mut by_rev: HashMap<String, Vec<SuggestedPerson>> = HashMap::new();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(items.iter().map(|i| i.revision_id.clone())), |r| {
        Ok((
            r.get::<_, String>(0)?,
            SuggestedPerson { face_id: r.get(1)?, person: PersonRef::new(r.get(2)?, r.get(3)?, r.get(4)?) },
        ))
    })?;
    for row in rows {
        let (revision, suggested) = row?;
        by_rev.entry(revision).or_default().push(suggested);
    }
    for item in items {
        item.suggested_people = by_rev.remove(&item.revision_id).unwrap_or_default();
    }
    Ok(())
}

pub fn query_photos(conn: &Connection, p: &SearchParams) -> Result<PhotoPage> {
    let f = build_filter(p)?;
    let order = order_sql(p.sort.as_deref())?;
    let limit = p.limit.unwrap_or(100).clamp(1, 200);
    let offset: i64 = p.cursor.as_deref().and_then(|c| c.parse().ok()).filter(|o| *o >= 0).unwrap_or(0);

    let coverage_sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(r.thumbnail_status = 'ready'), 0), COALESCE(SUM(r.thumbnail_status = 'pending'), 0),
           COALESCE(SUM(r.thumbnail_status = 'failed'), 0), COALESCE(SUM(EXISTS (SELECT 1 {CONFIRMED_ON_PHOTO})), 0),
           COALESCE(SUM(r.face_status = 'done'), 0), COALESCE(SUM(r.face_status = 'not_analyzed'), 0)
         {PHOTO_FROM}{}",
        f.sql
    );
    let coverage = conn.query_row(&coverage_sql, params_from_iter(f.args.iter()), |r| {
        Ok(Coverage {
            eligible_photos: r.get(0)?,
            thumbnails_ready: r.get(1)?,
            thumbnails_pending: r.get(2)?,
            thumbnails_failed: r.get(3)?,
            with_confirmed_people: r.get(4)?,
            faces_analyzed: r.get(5)?,
            faces_pending: r.get(6)?,
        })
    })?;

    let mut args = f.args.clone();
    args.push(Value::Integer(limit));
    args.push(Value::Integer(offset));
    let sql = format!(
        "SELECT {PHOTO_COLS} {PHOTO_FROM}{} ORDER BY {order} LIMIT ?{} OFFSET ?{}",
        f.sql,
        args.len() - 1,
        args.len()
    );
    let mut items: Vec<PhotoResult> =
        conn.prepare(&sql)?.query_map(params_from_iter(args), photo_from_row)?.collect::<rusqlite::Result<_>>()?;
    attach_people(conn, &mut items)?;
    if p.include_suggested {
        attach_suggested(conn, &mut items)?;
    }

    let total = coverage.eligible_photos;
    let next = offset + items.len() as i64;
    Ok(PhotoPage {
        next_cursor: (next < total && !items.is_empty()).then(|| next.to_string()),
        items,
        total,
        coverage,
        data_revision: db::data_revision(conn)?,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSnapshot {
    pub selection_token: String,
    pub count: i64,
    pub data_revision: i64,
}

/// "เลือกผลค้นหาทั้งหมด": แช่แข็ง photo ID + revision ของผลทั้งหมด ณ ตอนนี้ (Blueprint 11.5)
/// รูปที่สแกนเพิ่มภายหลังจะไม่เข้าชุดนี้เอง
pub fn create_selection(app: &App, p: &SearchParams) -> Result<SelectionSnapshot> {
    if p.include_suggested {
        crate::review::ensure_matches(app)?;
    }
    let f = build_filter(p)?;
    let order = order_sql(p.sort.as_deref())?;
    let mut conn = app.db();
    let tx = conn.transaction()?;
    let token = util::new_id();
    let revision = db::data_revision(&tx)?;
    tx.execute(
        "DELETE FROM selections WHERE created_at < ?1",
        [(time::OffsetDateTime::now_utc() - time::Duration::days(2))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()],
    )?;
    tx.execute(
        "INSERT INTO selections(token, data_revision, created_at) VALUES (?1, ?2, ?3)",
        params![token, revision, util::now()],
    )?;
    let mut args = f.args.clone();
    args.push(Value::Text(token.clone()));
    let sql = format!(
        "INSERT INTO selection_items(token, ord, photo_id, revision_id)
         SELECT ?{}, ROW_NUMBER() OVER (ORDER BY {order}), p.id, r.id {PHOTO_FROM}{}",
        args.len(),
        f.sql
    );
    let count = tx.execute(&sql, params_from_iter(args))? as i64;
    tx.commit()?;
    Ok(SelectionSnapshot { selection_token: token, count, data_revision: revision })
}

pub fn get_photo_details(app: &App, photo_id: &str) -> Result<PhotoDetails> {
    let conn = &app.db();
    let sql = format!("SELECT {PHOTO_COLS} {PHOTO_FROM} AND p.id = ?1");
    let (photo, path, file_size, event_date, decode_error) = conn
        .query_row(&sql, [photo_id], |r| {
            Ok((
                photo_from_row(r)?,
                r.get::<_, String>(11)?,
                r.get::<_, i64>(12)?,
                r.get::<_, Option<String>>(13)?,
                r.get::<_, Option<String>>(14)?,
            ))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("รูป"))?;
    let mut one = [photo];
    attach_people(conn, &mut one)?;
    let [photo] = one;
    let faces = faces::for_revision_with_suggestions(app, conn, &photo.revision_id)?;
    let original_available = Path::new(&path).is_file();
    Ok(PhotoDetails { photo, path, file_size, event_date, original_available, decode_error, faces })
}

/// ตำแหน่งไฟล์ต้นฉบับของรูป (ใช้ภายใน Rust เท่านั้น; UI ส่งมาแค่ ID)
pub fn photo_path(conn: &Connection, photo_id: &str) -> Result<String> {
    conn.query_row("SELECT os_path FROM photo_locations WHERE photo_id = ?1", [photo_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("รูป"))
}

pub fn event_folder(conn: &Connection, event_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT folder_path FROM event_source_bindings WHERE event_id = ?1 AND active = 1 LIMIT 1",
        [event_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("กิจกรรม"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSuggestions {
    pub people: Vec<PersonSummary>,
    pub events: Vec<EventSummary>,
    pub files: Vec<PhotoResult>,
}

/// คำแนะนำของช่องค้นหารวม แยกกลุ่ม บุคคล / กิจกรรม / ชื่อไฟล์ (Blueprint 11.1)
pub fn suggestions(conn: &Connection, text: &str, limit: Option<i64>) -> Result<SearchSuggestions> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(SearchSuggestions { people: vec![], events: vec![], files: vec![] });
    }
    let n = limit.unwrap_or(6).clamp(1, 20) as usize;
    let mut people = people::list(conn, &people::ListArgs { text: Some(text.to_string()), ..Default::default() })?;
    people.truncate(n);
    let mut events = list_events(conn, Some(text))?;
    events.truncate(n);
    let sql = format!("SELECT {PHOTO_COLS} {PHOTO_FROM} AND l.filename_key LIKE ?1 ESCAPE '\\' ORDER BY l.filename_key, p.id LIMIT ?2");
    let files = conn
        .prepare(&sql)?
        .query_map(params![util::like_pattern(text), n as i64], photo_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(SearchSuggestions { people, events, files })
}
