//! ตรวจฐานข้อมูลของ PuriPhotoSearch รุ่น Python แบบอ่านอย่างเดียว (Blueprint หมวด 14)
//!
//! ชุดงานนี้ทำเฉพาะ dry-run: นับและรายงานสิ่งที่พบ ยังไม่นำเข้าคลังใหม่
//! ฐานเดิมเปิดด้วย `immutable=1` + read-only จึงไม่มีไฟล์ -wal/-shm เกิดข้างฐานเดิม

use crate::error::{AppError, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
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
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join("Library/Application Support/PuriPhotoSearch/photosearch.db"))
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
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => default_path().ok_or_else(|| AppError::invalid("ไม่ทราบตำแหน่งข้อมูลของรุ่นเดิม"))?,
    };
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
