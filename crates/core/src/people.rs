//! บุคคล กลุ่ม ปักหมุด เปิดล่าสุด และดูข้อมูลเร็ว (Blueprint หมวด 6.5, 11.7)
//! ชื่อไม่ใช่ key: บุคคลชื่อซ้ำกันได้ ทุกอย่างอ้างด้วย ID

use crate::catalog::{self, PersonRef, PhotoResult, SearchParams, PERSON_COVER_SQL};
use crate::error::{AppError, Result};
use crate::import::{remember, replay};
use crate::{db, util, App};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MAX_NAME_CHARS: usize = 120;
const RECENT_KEEP: i64 = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub member_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonSummary {
    #[serde(flatten)]
    pub person: PersonRef,
    pub nickname: Option<String>,
    pub confirmed_photo_count: i64,
    /// รูปที่ AI จัดให้คนนี้และยังไม่มีคำยืนยันของคนนี้ในรูปนั้น (0 จนกว่า `review::ensure_matches` จะถูกเรียก)
    pub ai_photo_count: i64,
    pub pinned: bool,
    pub groups: Vec<GroupRef>,
    pub archived: bool,
    pub record_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonEventCount {
    pub event_id: String,
    pub title: String,
    pub event_date: Option<String>,
    pub photo_count: i64,
    pub cover_photo_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoappearanceItem {
    pub person: PersonSummary,
    pub shared_photo_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonDetail {
    #[serde(flatten)]
    pub summary: PersonSummary,
    pub aliases: Vec<String>,
    pub notes: String,
    pub confirmed_event_count: i64,
    pub has_confirmed_faces: bool,
    pub events: Vec<PersonEventCount>,
    pub coappearances: Vec<CoappearanceItem>,
    /// รูปที่ผู้ใช้แจ้งว่าไม่ใช่คนนี้ (คืนได้จากหน้ารายละเอียดบุคคล)
    pub rejected_photo_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickView {
    pub person: PersonSummary,
    pub confirmed_event_count: i64,
    pub has_confirmed_faces: bool,
    pub notes: String,
    pub recent_photos: Vec<PhotoResult>,
    pub coappearances: Vec<CoappearanceItem>,
    pub scope: &'static str,
    pub data_revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomePeople {
    pub tab: String,
    pub pinned: Vec<PersonSummary>,
    pub recent: Vec<PersonSummary>,
}

/// จำนวนรูป = COUNT(DISTINCT photo) จากใบหน้าที่ยืนยันแล้วและยังมีผล — คนเดียวหลายหน้าในภาพเดียวนับครั้งเดียว
fn person_select() -> String {
    format!(
        "SELECT pe.id, pe.display_name, {PERSON_COVER_SQL}, pe.nickname,
           (SELECT COUNT(DISTINCT v.photo_id) FROM v_confirmed v WHERE v.person_id = pe.id),
           EXISTS(SELECT 1 FROM pinned_persons pp WHERE pp.person_id = pe.id),
           pe.catalog_state = 'archived', pe.record_version,
           (SELECT COUNT(DISTINCT m.revision_id) FROM temp.ai_matches m WHERE m.person_id = pe.id AND NOT EXISTS (
              SELECT 1 FROM v_confirmed v WHERE v.person_id = pe.id AND v.revision_id = m.revision_id))
         FROM persons pe"
    )
}

fn person_from_row(r: &rusqlite::Row) -> rusqlite::Result<PersonSummary> {
    Ok(PersonSummary {
        person: PersonRef::new(r.get(0)?, r.get(1)?, r.get(2)?),
        nickname: r.get(3)?,
        confirmed_photo_count: r.get(4)?,
        ai_photo_count: r.get(8)?,
        pinned: r.get(5)?,
        groups: vec![],
        archived: r.get(6)?,
        record_version: r.get(7)?,
    })
}

fn attach_groups(conn: &Connection, people: &mut [PersonSummary]) -> Result<()> {
    if people.is_empty() {
        return Ok(());
    }
    let mut map: HashMap<String, Vec<GroupRef>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT pg.person_id, g.id, g.name FROM person_groups pg JOIN groups g ON g.id = pg.group_id ORDER BY g.name_key")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, GroupRef { id: r.get(1)?, name: r.get(2)? })))? {
        let (person_id, group) = row?;
        map.entry(person_id).or_default().push(group);
    }
    for p in people {
        p.groups = map.remove(&p.person.id).unwrap_or_default();
    }
    Ok(())
}

fn query_people(conn: &Connection, tail: &str, args: Vec<Value>) -> Result<Vec<PersonSummary>> {
    let sql = format!("{} {tail}", person_select());
    let mut people: Vec<PersonSummary> =
        conn.prepare(&sql)?.query_map(params_from_iter(args), person_from_row)?.collect::<rusqlite::Result<_>>()?;
    attach_groups(conn, &mut people)?;
    Ok(people)
}

pub fn summary(conn: &Connection, person_id: &str) -> Result<PersonSummary> {
    query_people(conn, "WHERE pe.id = ?1", vec![Value::Text(person_id.to_string())])?
        .pop()
        .ok_or_else(|| AppError::not_found("บุคคล"))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListArgs {
    pub text: Option<String>,
    pub include_archived: bool,
    pub group_id: Option<String>,
    pub pinned_only: bool,
}

pub fn list(conn: &Connection, a: &ListArgs) -> Result<Vec<PersonSummary>> {
    let mut sql = String::from("WHERE 1 = 1");
    let mut args: Vec<Value> = vec![];
    if !a.include_archived {
        sql.push_str(" AND pe.catalog_state = 'active'");
    }
    if let Some(t) = a.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        args.push(Value::Text(util::like_pattern(t)));
        let n = args.len();
        sql.push_str(&format!(
            " AND (pe.name_key LIKE ?{n} ESCAPE '\\' OR pe.nickname LIKE ?{n} ESCAPE '\\'
               OR EXISTS(SELECT 1 FROM person_aliases pa WHERE pa.person_id = pe.id AND pa.alias_key LIKE ?{n} ESCAPE '\\'))"
        ));
    }
    if let Some(g) = &a.group_id {
        args.push(Value::Text(g.clone()));
        sql.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM person_groups pg WHERE pg.person_id = pe.id AND pg.group_id = ?{})",
            args.len()
        ));
    }
    if a.pinned_only {
        sql.push_str(" AND EXISTS(SELECT 1 FROM pinned_persons pp WHERE pp.person_id = pe.id)");
    }
    sql.push_str(" ORDER BY pe.name_key, pe.id");
    query_people(conn, &sql, args)
}

/// คนที่ปรากฏร่วมในภาพกับ A: นับรูปไม่ซ้ำจากใบหน้ายืนยันแล้วเท่านั้น ไม่รวม A เองและคนที่เก็บไว้
pub fn coappearances(conn: &Connection, person_id: &str, limit: i64) -> Result<Vec<CoappearanceItem>> {
    let counts: Vec<(String, i64)> = conn
        .prepare(
            "SELECT b.person_id, COUNT(DISTINCT b.photo_id) AS n
             FROM v_confirmed a JOIN v_confirmed b ON b.photo_id = a.photo_id AND b.person_id <> a.person_id
             JOIN persons pe ON pe.id = b.person_id AND pe.catalog_state = 'active'
             WHERE a.person_id = ?1 GROUP BY b.person_id ORDER BY n DESC, pe.name_key, pe.id LIMIT ?2",
        )?
        .query_map(params![person_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    counts
        .into_iter()
        .map(|(id, n)| Ok(CoappearanceItem { person: summary(conn, &id)?, shared_photo_count: n }))
        .collect()
}

/// รูปของบุคคลหนึ่ง (?1) แยกตามกิจกรรม: ที่ยืนยันแล้ว รวมกับที่ AI จัดให้ (`temp.ai_matches`)
const PERSON_PHOTOS_CTE: &str = "WITH mine(photo_id, event_id) AS (
       SELECT v.photo_id, v.event_id FROM v_confirmed v WHERE v.person_id = ?1
       UNION
       SELECT p.id, ep.event_id FROM temp.ai_matches m
       JOIN photos p ON p.current_revision_id = m.revision_id AND p.catalog_state = 'active'
       JOIN event_photos ep ON ep.photo_id = p.id
       JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
       WHERE m.person_id = ?1)";

fn event_count(conn: &Connection, person_id: &str) -> Result<i64> {
    Ok(conn.query_row(&format!("{PERSON_PHOTOS_CTE} SELECT COUNT(DISTINCT event_id) FROM mine"), [person_id], |r| {
        r.get(0)
    })?)
}

pub fn get(conn: &Connection, person_id: &str) -> Result<PersonDetail> {
    let summary = summary(conn, person_id)?;
    let aliases = conn
        .prepare("SELECT alias FROM person_aliases WHERE person_id = ?1 ORDER BY alias_key")?
        .query_map([person_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let notes: String = conn.query_row("SELECT notes FROM persons WHERE id = ?1", [person_id], |r| r.get(0))?;
    let events = conn
        .prepare(&format!(
            "{PERSON_PHOTOS_CTE}
             SELECT e.id, e.title, e.event_date, COUNT(DISTINCT mine.photo_id), MIN(mine.photo_id)
             FROM mine JOIN events e ON e.id = mine.event_id
             GROUP BY e.id ORDER BY e.event_date IS NULL, e.event_date DESC, e.id DESC"
        ))?
        .query_map([person_id], |r| {
            Ok(PersonEventCount {
                event_id: r.get(0)?,
                title: r.get(1)?,
                event_date: r.get(2)?,
                photo_count: r.get(3)?,
                cover_photo_id: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(PersonDetail {
        has_confirmed_faces: summary.confirmed_photo_count > 0,
        confirmed_event_count: events.len() as i64,
        coappearances: coappearances(conn, person_id, 24)?,
        rejected_photo_count: conn.query_row(
            "SELECT COUNT(DISTINCT p.id) FROM face_rejections x JOIN faces f ON f.id = x.face_id AND f.status = 'active'
             JOIN photos p ON p.current_revision_id = f.photo_revision_id AND p.catalog_state = 'active'
             JOIN event_photos ep ON ep.photo_id = p.id JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
             WHERE x.person_id = ?1",
            [person_id],
            |r| r.get(0),
        )?,
        summary,
        aliases,
        notes,
        events,
    })
}

pub fn quick_view(conn: &Connection, person_id: &str) -> Result<QuickView> {
    let person = summary(conn, person_id)?;
    let notes: String = conn.query_row("SELECT notes FROM persons WHERE id = ?1", [person_id], |r| r.get(0))?;
    let recent = catalog::query_photos(
        conn,
        &SearchParams {
            person_ids: vec![person_id.to_string()],
            limit: Some(4),
            include_suggested: true,
            ..Default::default()
        },
    )?;
    Ok(QuickView {
        confirmed_event_count: event_count(conn, person_id)?,
        has_confirmed_faces: person.confirmed_photo_count > 0,
        notes,
        recent_photos: recent.items,
        coappearances: coappearances(conn, person_id, 8)?,
        scope: "all_events",
        data_revision: recent.data_revision,
        person,
    })
}

pub(crate) fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("ชื่อบุคคลต้องไม่ว่าง"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::invalid(format!("ชื่อบุคคลยาวได้ไม่เกิน {MAX_NAME_CHARS} ตัวอักษร")));
    }
    Ok(name.to_string())
}

/// สร้างระเบียนบุคคล (ใช้ร่วมกับการตั้งชื่อจากใบหน้า)
pub(crate) fn insert_person(conn: &Connection, name: &str) -> Result<String> {
    let name = clean_name(name)?;
    let id = util::new_id();
    conn.execute(
        "INSERT INTO persons(id, display_name, name_key, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, util::text_key(&name), util::now()],
    )?;
    Ok(id)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveArgs {
    pub person_id: Option<String>,
    pub expected_version: Option<i64>,
    pub request_id: String,
    pub display_name: String,
    pub nickname: Option<String>,
    pub notes: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub group_ids: Option<Vec<String>>,
}

pub fn save(app: &App, a: SaveArgs) -> Result<PersonDetail> {
    let detail = {
        let mut conn = app.db();
        if let Some(id) = replay::<String>(&conn, &a.request_id)? {
            return get(&conn, &id);
        }
        let name = clean_name(&a.display_name)?;
        let tx = conn.transaction()?;
        let id = match &a.person_id {
            Some(id) => {
                let current: i64 = tx
                    .query_row("SELECT record_version FROM persons WHERE id = ?1", [id], |r| r.get(0))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("บุคคล"))?;
                if Some(current) != a.expected_version {
                    return Err(AppError::version_conflict());
                }
                tx.execute(
                    "UPDATE persons SET display_name = ?2, name_key = ?3, record_version = record_version + 1 WHERE id = ?1",
                    params![id, name, util::text_key(&name)],
                )?;
                id.clone()
            }
            None => insert_person(&tx, &name)?,
        };
        if a.person_id.is_none() || a.nickname.is_some() {
            let nick = a.nickname.as_deref().map(str::trim).filter(|n| !n.is_empty());
            if nick.is_some_and(|n| n.chars().count() > MAX_NAME_CHARS) {
                return Err(AppError::invalid("ชื่อเล่นยาวเกินไป"));
            }
            tx.execute("UPDATE persons SET nickname = ?2 WHERE id = ?1", params![id, nick])?;
        }
        if let Some(notes) = &a.notes {
            if notes.chars().count() > 5000 {
                return Err(AppError::invalid("หมายเหตุยาวได้ไม่เกิน 5,000 ตัวอักษร"));
            }
            tx.execute("UPDATE persons SET notes = ?2 WHERE id = ?1", params![id, notes])?;
        }
        if let Some(aliases) = &a.aliases {
            tx.execute("DELETE FROM person_aliases WHERE person_id = ?1", [&id])?;
            for alias in aliases.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).take(20) {
                if alias.chars().count() > MAX_NAME_CHARS {
                    return Err(AppError::invalid("ชื่ออื่นยาวเกินไป"));
                }
                tx.execute(
                    "INSERT OR IGNORE INTO person_aliases(person_id, alias, alias_key) VALUES (?1, ?2, ?3)",
                    params![id, alias, util::text_key(alias)],
                )?;
            }
        }
        if let Some(groups) = &a.group_ids {
            tx.execute("DELETE FROM person_groups WHERE person_id = ?1", [&id])?;
            for g in groups {
                let ok = tx.execute(
                    "INSERT OR IGNORE INTO person_groups(person_id, group_id) SELECT ?1, id FROM groups WHERE id = ?2",
                    params![id, g],
                )?;
                if ok == 0
                    && !tx
                        .query_row("SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)", [g], |r| r.get::<_, bool>(0))?
                {
                    return Err(AppError::not_found("กลุ่ม"));
                }
            }
        }
        remember(&tx, &a.request_id, "save_person", &id)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        get(&conn, &id)?
    };
    app.notify(&["people"]);
    Ok(detail)
}

fn mutate(app: &App, person_id: &str, f: impl FnOnce(&Connection) -> Result<()>) -> Result<PersonSummary> {
    let s = {
        let conn = app.db();
        summary(&conn, person_id)?;
        f(&conn)?;
        db::bump_revision(&conn)?;
        summary(&conn, person_id)?
    };
    app.notify(&["people"]);
    Ok(s)
}

/// เก็บบุคคลออกจากรายชื่อหลัก โดยคงชื่อและคำยืนยันในภาพไว้ (ไม่ใช่การลบ)
pub fn set_archived(app: &App, person_id: &str, archived: bool) -> Result<PersonSummary> {
    mutate(app, person_id, |conn| {
        conn.execute(
            "UPDATE persons SET catalog_state = ?2, record_version = record_version + 1 WHERE id = ?1",
            params![person_id, if archived { "archived" } else { "active" }],
        )?;
        // บุคคลที่เก็บแล้วต้องไม่ถูกเสนอชื่อ: คิวตรวจสอบที่ออกไปแล้วใช้ไม่ได้
        db::bump_decision(conn)
    })
}

pub fn set_cover(app: &App, person_id: &str, face_id: &str) -> Result<PersonSummary> {
    mutate(app, person_id, |conn| {
        let ok: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM v_confirmed WHERE person_id = ?1 AND face_id = ?2)",
            params![person_id, face_id],
            |r| r.get(0),
        )?;
        if !ok {
            return Err(AppError::invalid("เลือกภาพตัวแทนได้เฉพาะใบหน้าที่ยืนยันแล้วของบุคคลนี้"));
        }
        conn.execute(
            "UPDATE persons SET cover_face_id = ?2, record_version = record_version + 1 WHERE id = ?1",
            params![person_id, face_id],
        )?;
        Ok(())
    })
}

/// ปักหมุดเป็น preference ที่ผู้ใช้เลือกเอง: ไม่เพิ่มจำนวนรูป ไม่ยืนยันอะไร และจำข้ามการเปิดแอป
pub fn set_pinned(app: &App, person_id: &str, pinned: bool) -> Result<PersonSummary> {
    mutate(app, person_id, |conn| {
        if pinned {
            conn.execute(
                "INSERT OR IGNORE INTO pinned_persons(person_id, position, created_at)
                 VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM pinned_persons), ?2)",
                params![person_id, util::now()],
            )?;
        } else {
            conn.execute("DELETE FROM pinned_persons WHERE person_id = ?1", [person_id])?;
        }
        Ok(())
    })
}

/// บันทึก "เปิดล่าสุด" — UI เรียกเฉพาะเมื่อผู้ใช้ตั้งใจเปิดดู ไม่เรียกจาก hover/prefetch
pub fn record_view(app: &App, person_id: &str) -> Result<()> {
    mutate(app, person_id, |conn| {
        conn.execute(
            "INSERT INTO recent_person_views(person_id, last_viewed_at) VALUES (?1, ?2)
             ON CONFLICT(person_id) DO UPDATE SET last_viewed_at = excluded.last_viewed_at",
            params![person_id, util::now()],
        )?;
        conn.execute(
            "DELETE FROM recent_person_views WHERE person_id NOT IN
               (SELECT person_id FROM recent_person_views ORDER BY last_viewed_at DESC LIMIT ?1)",
            [RECENT_KEEP],
        )?;
        Ok(())
    })
    .map(|_| ())
}

pub fn set_home_tab(app: &App, tab: &str) -> Result<()> {
    if !matches!(tab, "pinned" | "recent") {
        return Err(AppError::invalid("แท็บไม่ถูกต้อง"));
    }
    app.db().execute(
        "INSERT INTO ui_preferences(key, value) VALUES ('home_people_tab', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [tab],
    )?;
    Ok(())
}

pub fn home_people(conn: &Connection, limit: Option<i64>) -> Result<HomePeople> {
    let limit = limit.unwrap_or(12).clamp(1, 60);
    let pinned = query_people(
        conn,
        "JOIN pinned_persons pp ON pp.person_id = pe.id WHERE pe.catalog_state = 'active' ORDER BY pp.position, pe.id LIMIT ?1",
        vec![Value::Integer(limit)],
    )?;
    let recent = query_people(
        conn,
        "JOIN recent_person_views rv ON rv.person_id = pe.id WHERE pe.catalog_state = 'active' ORDER BY rv.last_viewed_at DESC, pe.id LIMIT ?1",
        vec![Value::Integer(limit)],
    )?;
    let saved: Option<String> = conn
        .query_row("SELECT value FROM ui_preferences WHERE key = 'home_people_tab'", [], |r| r.get(0))
        .optional()?;
    // ยังไม่เคยเลือก: ใช้ปักหมุดเมื่อมีรายการ มิฉะนั้นเปิดล่าสุด (Blueprint 6.5)
    let tab = saved.unwrap_or_else(|| if pinned.is_empty() { "recent".into() } else { "pinned".into() });
    Ok(HomePeople { tab, pinned, recent })
}

// ───────────────────────── กลุ่ม ─────────────────────────

pub fn list_groups(conn: &Connection) -> Result<Vec<GroupSummary>> {
    Ok(conn
        .prepare(
            "SELECT g.id, g.name, (SELECT COUNT(*) FROM person_groups pg JOIN persons pe ON pe.id = pg.person_id
               WHERE pg.group_id = g.id AND pe.catalog_state = 'active')
             FROM groups g ORDER BY g.name_key, g.id",
        )?
        .query_map([], |r| Ok(GroupSummary { id: r.get(0)?, name: r.get(1)?, member_count: r.get(2)? }))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn create_group(app: &App, name: &str) -> Result<GroupSummary> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::invalid("ชื่อกลุ่มต้องไม่ว่างและยาวไม่เกิน 80 ตัวอักษร"));
    }
    let conn = app.db();
    let key = util::text_key(name);
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM groups WHERE name_key = ?1)", [&key], |r| r.get::<_, bool>(0))? {
        return Err(AppError::invalid("มีกลุ่มชื่อนี้อยู่แล้ว"));
    }
    let id = util::new_id();
    conn.execute(
        "INSERT INTO groups(id, name, name_key, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, key, util::now()],
    )?;
    db::bump_revision(&conn)?;
    Ok(GroupSummary { id, name: name.to_string(), member_count: 0 })
}

/// ลบกลุ่ม = ลบเฉพาะความเป็นสมาชิกของกลุ่มนั้น บุคคลและรูปยังอยู่
pub fn delete_group(app: &App, group_id: &str) -> Result<()> {
    let conn = app.db();
    if conn.execute("DELETE FROM groups WHERE id = ?1", [group_id])? == 0 {
        return Err(AppError::not_found("กลุ่ม"));
    }
    db::bump_revision(&conn)?;
    Ok(())
}
