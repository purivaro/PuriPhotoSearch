//! เชื่อมตำแหน่งโฟลเดอร์ใหม่ (Blueprint S14, T13): โฟลเดอร์รูปถูกย้ายหรือเปลี่ยนชื่อจากนอกแอป
//!
//! ผู้ใช้ชี้ตำแหน่งใหม่ของโฟลเดอร์กิจกรรมหนึ่ง แอปจับคู่รูปเดิมกับไฟล์ที่ตำแหน่งใหม่ด้วย
//! **ตำแหน่งสัมพัทธ์ในโฟลเดอร์ + ขนาดไฟล์** แล้วเปลี่ยนเฉพาะตำแหน่งของรูปที่จับคู่ได้
//! ID ของกิจกรรม รูป ใบหน้า และคำยืนยันไม่เปลี่ยน ไม่มีการอ่านหรือแตะเนื้อไฟล์ต้นฉบับ
//! กิจกรรมอื่นจากโฟลเดอร์แม่เดียวกันที่หาไม่เจอเหมือนกัน และพบที่ตำแหน่งใหม่แบบเดียวกัน ย้ายไปพร้อมกันได้

use crate::error::{AppError, Result};
use crate::{db, util, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelinkEvent {
    pub event_id: String,
    pub title: String,
    pub old_folder: String,
    pub new_folder: String,
    pub photos: usize,
    /// พบไฟล์ที่ตำแหน่งสัมพัทธ์เดิมและขนาดตรงกัน
    pub matched: usize,
    /// ไม่พบไฟล์ที่ตำแหน่งสัมพัทธ์เดิม
    pub missing: usize,
    /// พบไฟล์ชื่อเดิมแต่ขนาดต่างจากที่บันทึกไว้ (ไม่ถือว่าเป็นรูปเดิม)
    pub changed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelinkPreview {
    pub event: RelinkEvent,
    /// กิจกรรมอื่นจากโฟลเดอร์แม่เดียวกัน ที่ตำแหน่งเดิมหาไม่เจอและพบที่ตำแหน่งใหม่แบบเดียวกัน
    pub siblings: Vec<RelinkEvent>,
    /// กิจกรรมอื่นจากโฟลเดอร์แม่เดียวกันที่ยังหาไม่เจอทั้งที่เดิมและที่ใหม่
    pub siblings_not_found: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelinkResult {
    pub events: usize,
    pub photos_relinked: usize,
    pub photos_unresolved: usize,
    pub data_revision: i64,
}

struct Binding {
    event_id: String,
    source_id: String,
    title: String,
    folder: PathBuf,
}

/// รูปของกิจกรรมหนึ่งกับตำแหน่งใหม่ที่จับคู่ได้ (None = ไม่พบหรือขนาดไม่ตรง)
struct Plan {
    summary: RelinkEvent,
    source_id: String,
    moves: Vec<(String, PathBuf)>,
}

fn binding(conn: &Connection, event_id: &str) -> Result<Binding> {
    conn.query_row(
        "SELECT b.event_id, b.source_id, e.title, b.folder_path FROM event_source_bindings b
         JOIN events e ON e.id = b.event_id AND e.catalog_state = 'active'
         WHERE b.event_id = ?1 AND b.active = 1 LIMIT 1",
        [event_id],
        |r| {
            Ok(Binding {
                event_id: r.get(0)?,
                source_id: r.get(1)?,
                title: r.get(2)?,
                folder: PathBuf::from(r.get::<_, String>(3)?),
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("กิจกรรม"))
}

fn plan(conn: &Connection, b: &Binding, new_folder: &Path) -> Result<Plan> {
    let rows: Vec<(String, String, i64)> = conn
        .prepare(
            "SELECT p.id, l.os_path, r.size FROM event_photos ep JOIN photos p ON p.id = ep.photo_id
             JOIN photo_locations l ON l.photo_id = p.id JOIN photo_revisions r ON r.id = p.current_revision_id
             WHERE ep.event_id = ?1 AND p.catalog_state = 'active'",
        )?
        .query_map([&b.event_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let (mut moves, mut missing, mut changed) = (vec![], 0, 0);
    for (photo_id, old_path, size) in &rows {
        // รูปที่ไม่ได้อยู่ใต้โฟลเดอร์ของกิจกรรม (ไม่ควรมี) นับเป็นไม่พบ ไม่เดาตำแหน่งให้
        let Ok(rel) = Path::new(old_path).strip_prefix(&b.folder) else {
            missing += 1;
            continue;
        };
        let candidate = new_folder.join(rel);
        match std::fs::metadata(&candidate) {
            Ok(m) if m.is_file() && m.len() as i64 == *size => moves.push((photo_id.clone(), candidate)),
            Ok(m) if m.is_file() => changed += 1,
            _ => missing += 1,
        }
    }
    Ok(Plan {
        summary: RelinkEvent {
            event_id: b.event_id.clone(),
            title: b.title.clone(),
            old_folder: util::path_key(&b.folder),
            new_folder: util::path_key(new_folder),
            photos: rows.len(),
            matched: moves.len(),
            missing,
            changed,
        },
        source_id: b.source_id.clone(),
        moves,
    })
}

/// โฟลเดอร์ใหม่ต้องไม่ใช่ที่ของกิจกรรมอื่นในคลัง (ทั้งตัวมันเอง อยู่ใน หรือครอบโฟลเดอร์ของกิจกรรมอื่น)
fn check_free(conn: &Connection, new_folder: &Path, moving: &[&str]) -> Result<()> {
    let others: Vec<(String, String)> = conn
        .prepare("SELECT event_id, folder_path FROM event_source_bindings WHERE active = 1")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (event_id, folder) in others {
        let folder = Path::new(&folder);
        if !moving.contains(&event_id.as_str()) && (new_folder.starts_with(folder) || folder.starts_with(new_folder)) {
            return Err(AppError::invalid("โฟลเดอร์ที่เลือกเป็นของกิจกรรมอื่นในคลังอยู่แล้ว กรุณาเลือกโฟลเดอร์ที่ย้ายกิจกรรมนี้ไป"));
        }
    }
    Ok(())
}

/// แผนของกิจกรรมที่ถาม + กิจกรรมพี่น้องจากโฟลเดอร์แม่เดียวกันที่พบที่ตำแหน่งใหม่แบบเดียวกัน
fn plans(conn: &Connection, event_id: &str, new_folder: &str) -> Result<(Plan, Vec<Plan>, usize)> {
    let b = binding(conn, event_id)?;
    let new = util::canonical(new_folder)
        .map_err(|_| AppError::new("SOURCE_UNAVAILABLE", "ไม่พบโฟลเดอร์ที่เลือก กรุณาเลือกอีกครั้ง"))?;
    if !new.is_dir() {
        return Err(AppError::invalid("กรุณาเลือกโฟลเดอร์"));
    }
    if new == b.folder {
        return Err(AppError::invalid("โฟลเดอร์ที่เลือกคือตำแหน่งเดิมของกิจกรรมนี้"));
    }
    let main = plan(conn, &b, &new)?;

    // โฟลเดอร์แม่ใหม่ = ตำแหน่งใหม่ ตัดส่วนที่เป็นตำแหน่งของกิจกรรมนี้ใต้โฟลเดอร์แม่เดิมออก
    let root: String = conn.query_row("SELECT root_path FROM sources WHERE id = ?1", [&b.source_id], |r| r.get(0))?;
    let new_root = b.folder.strip_prefix(&root).ok().and_then(|rel| {
        let mut base = new.clone();
        for _ in rel.components() {
            if !base.pop() {
                return None;
            }
        }
        (base.join(rel) == new).then_some(base)
    });
    let (mut siblings, mut not_found) = (vec![], 0);
    if let Some(new_root) = new_root {
        let others: Vec<(String, String, String)> = conn
            .prepare(
                "SELECT b.event_id, e.title, b.folder_path FROM event_source_bindings b
                 JOIN events e ON e.id = b.event_id AND e.catalog_state = 'active'
                 WHERE b.source_id = ?1 AND b.active = 1 AND b.event_id <> ?2 ORDER BY e.title, b.event_id",
            )?
            .query_map(params![b.source_id, b.event_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (other_id, title, folder) in others {
            let folder = PathBuf::from(folder);
            // กิจกรรมที่โฟลเดอร์เดิมยังอยู่ไม่ถูกย้าย
            if folder.is_dir() {
                continue;
            }
            let Ok(rel) = folder.strip_prefix(&root) else { continue };
            let target = new_root.join(rel);
            if !target.is_dir() {
                not_found += 1;
                continue;
            }
            let other = Binding { event_id: other_id, source_id: b.source_id.clone(), title, folder };
            let p = plan(conn, &other, &target)?;
            if p.summary.matched > 0 {
                siblings.push(p);
            } else {
                not_found += 1;
            }
        }
    }
    let moving: Vec<&str> = std::iter::once(main.summary.event_id.as_str())
        .chain(siblings.iter().map(|p| p.summary.event_id.as_str()))
        .collect();
    check_free(conn, &new, &moving)?;
    Ok((main, siblings, not_found))
}

pub fn preview(app: &App, event_id: &str, new_folder: &str) -> Result<RelinkPreview> {
    let (main, siblings, siblings_not_found) = plans(&app.db(), event_id, new_folder)?;
    Ok(RelinkPreview {
        event: main.summary,
        siblings: siblings.into_iter().map(|p| p.summary).collect(),
        siblings_not_found,
    })
}

pub fn commit(app: &App, event_id: &str, new_folder: &str, include_siblings: bool) -> Result<RelinkResult> {
    let result = {
        let mut conn = app.db();
        let (main, siblings, _) = plans(&conn, event_id, new_folder)?;
        if main.summary.matched == 0 {
            return Err(AppError::invalid("ไม่พบรูปของกิจกรรมนี้ในโฟลเดอร์ที่เลือก จึงไม่เปลี่ยนตำแหน่ง"));
        }
        let mut all = vec![main];
        if include_siblings {
            all.extend(siblings);
        }
        let tx = conn.transaction()?;
        let now = util::now();
        let (mut relinked, mut unresolved) = (0, 0);
        for p in &all {
            tx.execute(
                "UPDATE event_source_bindings SET folder_path = ?2, folder_key = ?3 WHERE event_id = ?1 AND active = 1",
                params![p.summary.event_id, p.summary.new_folder, util::path_key(Path::new(&p.summary.new_folder))],
            )?;
            for (photo_id, path) in &p.moves {
                tx.execute(
                    "UPDATE photo_locations SET os_path = ?2, path_key = ?3, available = 1, verified_at = ?4 WHERE photo_id = ?1",
                    params![photo_id, util::path_key(path), util::path_key(path), now],
                )?;
            }
            relinked += p.moves.len();
            unresolved += p.summary.photos - p.moves.len();
        }
        // โฟลเดอร์แม่ของแหล่งรูป: ย้ายตามเมื่อกิจกรรมทุกงานของแหล่งนี้อยู่ใต้ตำแหน่งใหม่แล้ว
        let source_id = &all[0].source_id;
        let old_root: String =
            tx.query_row("SELECT root_path FROM sources WHERE id = ?1", [source_id], |r| r.get(0))?;
        let old_event_folder = PathBuf::from(&all[0].summary.old_folder);
        if let Ok(rel) = old_event_folder.strip_prefix(&old_root) {
            let mut new_root = PathBuf::from(&all[0].summary.new_folder);
            let fits = rel.components().all(|_| new_root.pop());
            let folders: Vec<String> = tx
                .prepare("SELECT folder_path FROM event_source_bindings WHERE source_id = ?1 AND active = 1")?
                .query_map([source_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let key = util::path_key(&new_root);
            let taken: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE path_key = ?1 AND id <> ?2)",
                params![key, source_id],
                |r| r.get(0),
            )?;
            if fits && !taken && folders.iter().all(|f| Path::new(f).starts_with(&new_root)) {
                tx.execute(
                    "UPDATE sources SET root_path = ?2, path_key = ?3 WHERE id = ?1",
                    params![source_id, key, key],
                )?;
            }
        }
        let summary: Vec<_> = all.iter().map(|p| &p.summary).collect();
        tx.execute(
            "INSERT INTO operations(id, kind, summary, created_at) VALUES (?1, 'relink', ?2, ?3)",
            params![util::new_id(), serde_json::to_string(&summary).map_err(AppError::internal)?, now],
        )?;
        let data_revision = db::bump_revision(&tx)?;
        tx.commit()?;
        RelinkResult { events: all.len(), photos_relinked: relinked, photos_unresolved: unresolved, data_revision }
    };
    app.notify(&["events", "photos"]);
    Ok(result)
}
