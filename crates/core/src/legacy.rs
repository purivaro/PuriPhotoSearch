//! ตรวจฐานข้อมูลของ PuriPhotoSearch รุ่น Python แบบอ่านอย่างเดียว (Blueprint หมวด 14)
//!
//! ชุดงานนี้ทำเฉพาะ dry-run: นับและรายงานสิ่งที่พบ ยังไม่นำเข้าคลังใหม่
//! ฐานเดิมเปิดด้วย `immutable=1` + read-only จึงไม่มีไฟล์ -wal/-shm เกิดข้างฐานเดิม

use crate::error::{AppError, Result};
use crate::import::{remember, replay};
use crate::{db, people, util, App};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyReport {
    pub path: String,
    pub recognized: bool,
    pub persons: i64,
    pub duplicate_names: i64,
    pub groups: i64,
    pub reference_samples: i64,
    pub event_folders: i64,
    pub photos: i64,
    pub photos_missing_on_disk: i64,
    pub faces: i64,
    pub embedding_dim: Option<i64>,
    pub rejections: i64,
    pub dismissed_faces: i64,
    pub notes: Vec<String>,
}

/// ตำแหน่งฐานข้อมูลเริ่มต้นของรุ่น Python บน macOS (`app/config.py`: APP_NAME = "PuriPhotoSearch")
pub fn default_path() -> Option<PathBuf> {
    // เทสต์ชี้ debug build ไปที่ฐานจำลอง (หรือ path ที่ไม่มีไฟล์) แทนฐานจริงของผู้ใช้
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("PURI_LEGACY_DB") {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join("Library/Application Support/PuriPhotoSearch/photosearch.db"))
}

fn resolve(path: Option<&str>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(PathBuf::from(p)),
        None => default_path().ok_or_else(|| AppError::invalid("ไม่ทราบตำแหน่งข้อมูลของรุ่นเดิม")),
    }
}

fn percent_encode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn open_read_only(path: &Path) -> Result<Connection> {
    if !path.is_file() {
        return Err(AppError::new("SOURCE_UNAVAILABLE", "ไม่พบไฟล์ฐานข้อมูลของรุ่นเดิมในตำแหน่งนี้"));
    }
    let uri = format!("file:{}?mode=ro&immutable=1", percent_encode(&path.to_string_lossy()));
    Ok(Connection::open_with_flags(uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI)?)
}

pub fn dry_run(path: Option<&str>) -> Result<LegacyReport> {
    let path = resolve(path)?;
    let conn = open_read_only(&path)?;
    let has = |table: &str| -> Result<bool> {
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |r| r.get(0),
        )?)
    };
    let mut report = LegacyReport {
        path: path.to_string_lossy().into_owned(),
        recognized: false,
        persons: 0,
        duplicate_names: 0,
        groups: 0,
        reference_samples: 0,
        event_folders: 0,
        photos: 0,
        photos_missing_on_disk: 0,
        faces: 0,
        embedding_dim: None,
        rejections: 0,
        dismissed_faces: 0,
        notes: vec![],
    };
    let required = ["persons", "person_embeddings", "event_folders", "photos", "faces"];
    for t in required {
        if !has(t)? {
            report.notes.push(format!("ไม่พบตาราง {t} — ไฟล์นี้ไม่ใช่ฐานข้อมูลของ PuriPhotoSearch รุ่น Python ที่รู้จัก"));
            return Ok(report);
        }
    }
    report.recognized = true;
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    report.persons = count("SELECT COUNT(*) FROM persons")?;
    report.duplicate_names =
        count("SELECT COUNT(*) FROM (SELECT name FROM persons GROUP BY name HAVING COUNT(*) > 1)")?;
    report.reference_samples = count("SELECT COUNT(*) FROM person_embeddings")?;
    report.event_folders = count("SELECT COUNT(*) FROM event_folders")?;
    report.photos = count("SELECT COUNT(*) FROM photos")?;
    report.faces = count("SELECT COUNT(*) FROM faces")?;
    let has_group_column: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('persons') WHERE name = 'group_name')",
        [],
        |r| r.get(0),
    )?;
    if has_group_column {
        let named = if has("person_groups")? { "SELECT name FROM person_groups UNION " } else { "" };
        report.groups = count(&format!(
            "SELECT COUNT(*) FROM ({named}SELECT DISTINCT group_name FROM persons WHERE group_name IS NOT NULL)"
        ))?;
    }
    if has("face_exclusions")? {
        report.rejections = count("SELECT COUNT(*) FROM face_exclusions")?;
    }
    if has("dismissed_faces")? {
        report.dismissed_faces = count("SELECT COUNT(*) FROM dismissed_faces")?;
    }
    let dims: Vec<i64> = conn
        .prepare("SELECT DISTINCT length(embedding) / 4 FROM faces UNION SELECT DISTINCT length(embedding) / 4 FROM person_embeddings")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if dims.len() == 1 {
        report.embedding_dim = dims.first().copied();
    } else if dims.len() > 1 {
        report.notes.push(format!("พบเวกเตอร์ใบหน้าหลายขนาด {dims:?} — ต้องตรวจก่อนนำไปใช้ต่อ"));
    }
    let mut stmt = conn.prepare("SELECT file_path FROM photos")?;
    for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
        if !Path::new(&row?).is_file() {
            report.photos_missing_on_disk += 1;
        }
    }

    report.notes.push(
        "รุ่นเดิมไม่มีตาราง “ใบหน้านี้คือใคร” รายใบหน้า: บุคคลถูกกำหนดด้วยใบหน้าตัวอย่าง แล้วจับคู่ด้วยความคล้ายทุกครั้งที่ค้น \
         จึงย้ายได้เฉพาะรายชื่อ กลุ่ม และใบหน้าตัวอย่าง ส่วนผลจับคู่ต้องเข้าคิวตรวจใหม่ ไม่นับเป็นยืนยันแล้ว"
            .into(),
    );
    if report.photos_missing_on_disk > 0 {
        report.notes.push(format!(
            "มี {} จาก {} รูปที่ไม่พบไฟล์ในตำแหน่งเดิม — ต้องเชื่อมตำแหน่งใหม่ก่อนจึงจะสร้างภาพย่อได้",
            report.photos_missing_on_disk, report.photos
        ));
    }
    report.notes.push("การตรวจนี้อ่านอย่างเดียว ไม่แก้ฐานข้อมูลเดิม และยังไม่ได้นำข้อมูลเข้าคลังใหม่".into());
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportResult {
    pub persons_created: usize,
    pub persons_skipped: usize,
    pub groups_created: usize,
    pub samples_imported: usize,
    pub samples_pending_embedding: i64,
    pub notes: Vec<String>,
}

struct LegacyPerson {
    key: String,
    id: i64,
    name: String,
    group: Option<String>,
}

/// รายชื่อบุคคลของฐานเดิม; ผิดพลาดเมื่อไฟล์ไม่ใช่ฐานข้อมูลของรุ่น Python ที่รู้จัก
fn read_persons(legacy: &Connection) -> Result<Vec<LegacyPerson>> {
    let has_table = |t: &str| -> Result<bool> {
        Ok(legacy.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [t],
            |r| r.get(0),
        )?)
    };
    if !has_table("persons")? || !has_table("person_embeddings")? {
        return Err(AppError::invalid("ไฟล์นี้ไม่ใช่ฐานข้อมูลของ PuriPhotoSearch รุ่น Python ที่รู้จัก"));
    }
    let has_group: bool = legacy.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('persons') WHERE name = 'group_name')",
        [],
        |r| r.get(0),
    )?;
    let group_col = if has_group { "group_name" } else { "NULL" };
    let persons = legacy
        .prepare(&format!("SELECT id, name, {group_col}, COALESCE(created_at, '') FROM persons ORDER BY id"))?
        .query_map([], |r| {
            let (id, created): (i64, String) = (r.get(0)?, r.get(3)?);
            Ok(LegacyPerson { key: format!("{id}|{created}"), id, name: r.get(1)?, group: r.get(2)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(persons)
}

/// จำนวนบุคคลในฐานเดิมที่ยังไม่ได้นำเข้าคลังนี้ (อ่านฐานเดิมอย่างเดียว) — ใช้ตัดสินว่าจะเสนอให้นำเข้าหรือไม่
pub fn pending_people(app: &App, path: Option<&str>) -> Result<i64> {
    let persons = read_persons(&open_read_only(&resolve(path)?)?)?;
    let conn = app.db();
    let mut known =
        conn.prepare("SELECT EXISTS(SELECT 1 FROM legacy_id_map WHERE kind = 'person' AND legacy_id = ?1)")?;
    let mut pending = 0;
    for person in &persons {
        if !known.query_row([&person.key], |r| r.get::<_, bool>(0))? {
            pending += 1;
        }
    }
    Ok(pending)
}

/// นำรายชื่อ กลุ่ม และภาพใบหน้าตัวอย่างจากรุ่น Python เข้าคลัง (Blueprint 14.3 เฉพาะส่วนที่ผู้ใช้ยืนยันไว้จริง)
///
/// - ฐานเดิมเปิดแบบอ่านอย่างเดียว ไม่ถูกแก้
/// - รันซ้ำไม่สร้างคนซ้ำ: จำคู่ (รหัสเดิม + เวลาสร้างเดิม) → ID ใหม่ ไว้ใน `legacy_id_map`
/// - เวกเตอร์ของรุ่นเดิมไม่ถูกนำมาใช้ (คนละโมเดล คนละ space): ภาพครอปตัวอย่างจะถูกสร้างเวกเตอร์ใหม่ด้วยโมเดลปัจจุบัน
/// - ผลจับคู่ของรุ่นเดิมไม่ถูกนำมาเป็นคำยืนยัน และรูป/โฟลเดอร์ไม่ถูกย้ายมา (ผู้ใช้เพิ่มโฟลเดอร์เอง)
pub fn import_people(app: &App, path: Option<&str>, request_id: &str) -> Result<LegacyImportResult> {
    if let Some(prev) = replay::<LegacyImportResult>(&app.db(), request_id)? {
        return Ok(prev);
    }
    let path = resolve(path)?;
    let legacy = open_read_only(&path)?;
    let persons = read_persons(&legacy)?;
    let mut samples = legacy.prepare(
        "SELECT thumbnail FROM person_embeddings WHERE person_id = ?1 AND thumbnail IS NOT NULL ORDER BY is_primary DESC, id",
    )?;

    let mut result = LegacyImportResult {
        persons_created: 0,
        persons_skipped: 0,
        groups_created: 0,
        samples_imported: 0,
        samples_pending_embedding: 0,
        notes: vec![],
    };
    {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        let now = util::now();
        for person in &persons {
            let known: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM legacy_id_map WHERE kind = 'person' AND legacy_id = ?1)",
                [&person.key],
                |r| r.get(0),
            )?;
            if known {
                result.persons_skipped += 1;
                continue;
            }
            let name = if person.name.trim().is_empty() { "ไม่มีชื่อ" } else { person.name.trim() };
            let name: String = name.chars().take(120).collect();
            let new_id = people::insert_person(&tx, &name)?;
            tx.execute(
                "INSERT INTO legacy_id_map(kind, legacy_id, new_id) VALUES ('person', ?1, ?2)",
                params![person.key, new_id],
            )?;
            result.persons_created += 1;

            if let Some(group) = person.group.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
                let key = util::text_key(group);
                let existing: Option<String> =
                    tx.query_row("SELECT id FROM groups WHERE name_key = ?1", [&key], |r| r.get(0)).optional()?;
                let group_id = match existing {
                    Some(id) => id,
                    None => {
                        let id = util::new_id();
                        tx.execute(
                            "INSERT INTO groups(id, name, name_key, created_at) VALUES (?1, ?2, ?3, ?4)",
                            params![id, group, key, now],
                        )?;
                        result.groups_created += 1;
                        id
                    }
                };
                tx.execute(
                    "INSERT OR IGNORE INTO person_groups(person_id, group_id) VALUES (?1, ?2)",
                    params![new_id, group_id],
                )?;
            }
            let thumbs: Vec<Vec<u8>> =
                samples.query_map([person.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for thumbnail in thumbs.into_iter().filter(|t| !t.is_empty()) {
                tx.execute(
                    "INSERT INTO person_reference_samples(id, person_id, thumbnail, source, created_at) VALUES (?1, ?2, ?3, 'legacy_python', ?4)",
                    params![util::new_id(), new_id, thumbnail, now],
                )?;
                result.samples_imported += 1;
            }
        }
        result.samples_pending_embedding =
            tx.query_row("SELECT COUNT(*) FROM person_reference_samples WHERE vector IS NULL", [], |r| r.get(0))?;
        result.notes.push("ฐานข้อมูลของรุ่นเดิมไม่ถูกแก้ไข".into());
        result.notes.push("รูปและโฟลเดอร์ของรุ่นเดิมไม่ได้ถูกย้ายมา เพิ่มโฟลเดอร์กิจกรรมในแอปนี้เพื่อให้ค้นรูปได้".into());
        result.notes.push(
            "ใบหน้าตัวอย่างที่นำเข้าใช้เป็นข้อเสนอ “อาจเป็น …” เท่านั้น ต้องกดยืนยันในหน้าตรวจสอบใบหน้าจึงจะนับเป็นรูปของบุคคลนั้น".into(),
        );
        remember(&tx, request_id, "import_legacy_people", &result)?;
        db::bump_identity(&tx)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
    }
    app.worker.wake();
    app.notify(&["people"]);
    Ok(result)
}
