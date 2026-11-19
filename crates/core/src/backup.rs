//! สำรองและกู้คืนคลัง (Blueprint S20, 13.3–13.4)
//!
//! ชุดสำรอง = โฟลเดอร์ที่มี `library.sqlite` (snapshot ที่ SQLite รับรองว่าสอดคล้อง แม้แอปกำลังเขียนอยู่) กับ `manifest.json`
//! ไม่รวมรูปต้นฉบับ ภาพย่อ ครอปใบหน้า และไฟล์โมเดล — สามอย่างหลังสร้างใหม่หรือดาวน์โหลดใหม่ได้
//!
//! การกู้คืนไม่เขียนทับคลังที่เปิดอยู่: ไฟล์ที่ตรวจแล้วถูกวางรอไว้ แล้วสลับเข้าตอนเปิดแอปครั้งถัดไป
//! หลังเก็บคลังปัจจุบันเป็นชุดสำรองก่อนเสมอ (ยังไม่มี "เปิดเป็นคลังใหม่" เพราะแอปมีคลังเดียว)

use crate::error::{AppError, Result};
use crate::{db, util, App};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DB_FILE: &str = "library.sqlite";
const MANIFEST_FILE: &str = "manifest.json";
const PENDING_DB: &str = "restore-pending.sqlite";
const PENDING_MANIFEST: &str = "restore-pending.json";
const FORMAT: u32 = 1;
/// สำรองอัตโนมัติไม่ถี่กว่านี้ และเก็บชุดล่าสุดไว้เท่านี้ (ฐานของคลังใหญ่มีขนาดหลายร้อย MB)
const AUTO_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const AUTO_KEEP: usize = 3;
/// ชุดที่เก็บก่อนกู้คืน/ก่อนอัปเกรด เก็บไว้ชนิดละเท่านี้
const SAFETY_KEEP: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub events: i64,
    pub photos: i64,
    pub persons: i64,
    pub confirmations: i64,
    pub faces: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    /// manual | auto | before_restore | before_upgrade
    pub kind: String,
    pub library_id: String,
    pub library_name: String,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    pub counts: Counts,
    pub bytes: u64,
    pub sha256: String,
    /// สิ่งที่ชุดสำรองนี้ไม่รวม (แสดงให้ผู้ใช้เห็นตามจริง)
    pub excluded: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: String,
    #[serde(flatten)]
    pub manifest: Manifest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub last_backup_at: Option<String>,
    pub last_backup_path: Option<String>,
    /// ที่เก็บชุดสำรองอัตโนมัติและชุดที่เก็บก่อนกู้คืน/อัปเกรด
    pub backups_dir: String,
    /// ชุดที่แอปเก็บเอง ใหม่สุดก่อน
    pub automatic: Vec<BackupInfo>,
    /// ชุดที่รอสลับเข้าเมื่อเปิดแอปครั้งถัดไป
    pub pending_restore: Option<Manifest>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub backup: BackupInfo,
    /// จำนวนในคลังที่เปิดอยู่ตอนนี้ ซึ่งจะถูกแทนที่
    pub current: Counts,
    pub same_library: bool,
    /// ชุดสำรองมาจาก schema รุ่นเก่ากว่า: จะถูกอัปเกรดเมื่อเปิด
    pub needs_upgrade: bool,
}

fn stamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(time::macros::format_description!("[year][month][day]-[hour][minute][second]")).unwrap_or_default()
}

fn backups_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("backups")
}

/// นับแบบทนต่อ schema รุ่นเก่า (ตารางที่ยังไม่มี = 0)
fn read_counts(conn: &Connection) -> Counts {
    let n = |sql: &str| conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0);
    Counts {
        events: n("SELECT COUNT(*) FROM events WHERE catalog_state = 'active'"),
        photos: n("SELECT COUNT(*) FROM photos WHERE catalog_state = 'active'"),
        persons: n("SELECT COUNT(*) FROM persons"),
        confirmations: n("SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL"),
        faces: n("SELECT COUNT(*) FROM faces WHERE status = 'active'"),
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(util::hex(&hasher.finalize()))
}

/// เขียนชุดสำรองหนึ่งชุดลง `parent/name`: เขียนในโฟลเดอร์ชั่วคราวก่อน แล้วเปลี่ยนชื่อเมื่อครบ ชุดที่เห็นจึงสมบูรณ์เสมอ
fn write_set(db_path: &Path, parent: &Path, kind: &str, name: &str) -> Result<BackupInfo> {
    std::fs::create_dir_all(parent)?;
    let mut target = parent.join(name);
    let mut n = 2;
    while target.exists() {
        target = parent.join(format!("{name}-{n}"));
        n += 1;
    }
    let tmp = parent.join(format!(".{}.partial", util::file_name(&target)));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let result = (|| -> Result<Manifest> {
        let copy = tmp.join(DB_FILE);
        {
            // การเชื่อมต่อของตัวเอง: ไม่ขวางการทำงานของแอป และได้ภาพ ณ ขณะเดียวแม้มีการเขียนอยู่ (WAL)
            let source = Connection::open(db_path)?;
            source.busy_timeout(Duration::from_secs(30))?;
            source.execute("VACUUM INTO ?1", [copy.to_string_lossy()])?;
        }
        let manifest = {
            let snap = Connection::open(&copy)?;
            // สำเนาไม่ใช้ WAL: เปิดอ่านภายหลังได้โดยไม่มีไฟล์ -wal/-shm งอกในชุดสำรอง
            snap.pragma_update(None, "journal_mode", "DELETE")?;
            Manifest {
                format: FORMAT,
                kind: kind.to_string(),
                library_id: db::meta_get(&snap, "library_id")?.unwrap_or_default(),
                library_name: db::meta_get(&snap, "name")?.unwrap_or_default(),
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                schema_version: snap.query_row("PRAGMA user_version", [], |r| r.get(0))?,
                created_at: util::now(),
                counts: read_counts(&snap),
                bytes: 0,
                sha256: String::new(),
                excluded: ["originals", "thumbnails", "face_crops", "models"].map(String::from).to_vec(),
            }
        };
        let manifest = Manifest { bytes: std::fs::metadata(&copy)?.len(), sha256: sha256_file(&copy)?, ..manifest };
        std::fs::write(tmp.join(MANIFEST_FILE), serde_json::to_vec_pretty(&manifest).map_err(AppError::internal)?)?;
        Ok(manifest)
    })();
    match result {
        Ok(manifest) => {
            std::fs::rename(&tmp, &target)?;
            Ok(BackupInfo { path: util::path_key(&target), manifest })
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// ลบชุดเก่าของชนิดเดียวกันในที่เก็บของแอปเอง เหลือ `keep` ชุดล่าสุด (ชื่อโฟลเดอร์ขึ้นต้นด้วยเวลา จึงเรียงตามชื่อได้)
fn prune(dir: &Path, prefix: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut sets: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && util::file_name(p).starts_with(prefix))
        .collect();
    sets.sort();
    sets.reverse();
    for old in sets.into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// สำรองตามคำสั่งผู้ใช้ ลงโฟลเดอร์ที่เลือก
pub fn create(app: &App, destination: &str) -> Result<BackupInfo> {
    let dest = util::canonical(destination)
        .map_err(|_| AppError::new("SOURCE_UNAVAILABLE", "ไม่พบโฟลเดอร์ปลายทาง กรุณาเลือกโฟลเดอร์อีกครั้ง"))?;
    if !dest.is_dir() {
        return Err(AppError::invalid("ปลายทางต้องเป็นโฟลเดอร์"));
    }
    if crate::export::inside_tracked_source(&app.db(), &dest)? {
        // แอปไม่เขียนไฟล์ลงโฟลเดอร์รูปของผู้ใช้
        return Err(AppError::invalid("ปลายทางอยู่ในโฟลเดอร์รูปที่คลังติดตามอยู่ กรุณาเลือกโฟลเดอร์อื่นสำหรับเก็บชุดสำรอง"));
    }
    let info = write_set(&app.db_path, &dest, "manual", &format!("PuriPhotoSearch-backup-{}", stamp()))?;
    let conn = app.db();
    db::meta_set(&conn, "last_backup_at", &info.manifest.created_at)?;
    db::meta_set(&conn, "last_backup_path", &info.path)?;
    Ok(info)
}

/// สำรองอัตโนมัติวันละไม่เกินหนึ่งครั้ง เมื่อคลังมีข้อมูล; คืน None เมื่อยังไม่ถึงเวลา
pub fn auto_backup(db_path: &Path) -> Result<Option<BackupInfo>> {
    let dir = backups_dir(db_path);
    let newest = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("auto-"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max();
    if newest.is_some_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < AUTO_EVERY) {
        return Ok(None);
    }
    {
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let counts = read_counts(&conn);
        if counts.events == 0 && counts.persons == 0 {
            return Ok(None);
        }
    }
    let info = write_set(db_path, &dir, "auto", &format!("auto-{}", stamp()))?;
    prune(&dir, "auto-", AUTO_KEEP);
    Ok(Some(info))
}

/// เก็บคลังก่อน migration เปลี่ยน schema (เรียกจาก `db::open_library`)
pub(crate) fn before_upgrade(db_path: &Path, from_version: i64) -> Result<()> {
    let dir = backups_dir(db_path);
    write_set(db_path, &dir, "before_upgrade", &format!("before-upgrade-v{from_version}-{}", stamp()))?;
    prune(&dir, "before-upgrade-", SAFETY_KEEP);
    Ok(())
}

fn read_manifest(path: &Path) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// ตรวจชุดสำรอง: เป็นชุดของเรา ไฟล์ไม่ถูกแก้ เปิดได้ด้วยแอปรุ่นนี้ และข้อมูลภายในสมบูรณ์
fn verify_set(dir: &Path) -> Result<Manifest> {
    let not_ours = || {
        AppError::invalid(
            "โฟลเดอร์นี้ไม่ใช่ชุดสำรองของ Puri Photo Search กรุณาเลือกโฟลเดอร์ที่มีไฟล์ manifest.json และ library.sqlite",
        )
    };
    let manifest = read_manifest(&dir.join(MANIFEST_FILE)).ok_or_else(not_ours)?;
    let db_file = dir.join(DB_FILE);
    if manifest.format != FORMAT || !db_file.is_file() {
        return Err(not_ours());
    }
    let damaged = || AppError::new("DATABASE_CORRUPT", "ชุดสำรองนี้เสียหายหรือถูกแก้ไข จึงไม่นำมาใช้ กรุณาเลือกชุดสำรองอื่น");
    if sha256_file(&db_file)? != manifest.sha256 {
        return Err(damaged());
    }
    check_db(&db_file).map_err(|e| if e.code == "LIBRARY_TOO_NEW" { e } else { damaged() })?;
    Ok(manifest)
}

fn check_db(db_file: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(db_file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let app_id: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    if app_id != db::APPLICATION_ID {
        return Err(AppError::new("DATABASE_CORRUPT", "ไฟล์นี้ไม่ใช่คลังของ Puri Photo Search 2"));
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > db::latest_version() {
        return Err(AppError::new("LIBRARY_TOO_NEW", "ชุดสำรองนี้สร้างด้วยแอปรุ่นใหม่กว่า กรุณาอัปเดตแอปก่อนกู้คืน"));
    }
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    let violations: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
    if check != "ok" || violations > 0 {
        return Err(AppError::new("DATABASE_CORRUPT", "ข้อมูลในไฟล์ไม่สมบูรณ์"));
    }
    Ok(())
}

fn list_sets(dir: &Path) -> Vec<BackupInfo> {
    let mut sets: Vec<BackupInfo> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| Some(BackupInfo { manifest: read_manifest(&p.join(MANIFEST_FILE))?, path: util::path_key(&p) }))
        .collect();
    sets.sort_by(|a, b| b.manifest.created_at.cmp(&a.manifest.created_at));
    sets
}

pub fn status(app: &App) -> Result<BackupStatus> {
    let dir = backups_dir(&app.db_path);
    let data_dir = app.db_path.parent().unwrap_or(Path::new("."));
    let conn = app.db();
    Ok(BackupStatus {
        last_backup_at: db::meta_get(&conn, "last_backup_at")?,
        last_backup_path: db::meta_get(&conn, "last_backup_path")?,
        backups_dir: util::path_key(&dir),
        automatic: list_sets(&dir),
        pending_restore: data_dir
            .join(PENDING_DB)
            .is_file()
            .then(|| read_manifest(&data_dir.join(PENDING_MANIFEST)))
            .flatten(),
    })
}

pub fn preview_restore(app: &App, path: &str) -> Result<RestorePreview> {
    let dir = PathBuf::from(path);
    if !dir.is_dir() {
        return Err(AppError::new("SOURCE_UNAVAILABLE", "ไม่พบโฟลเดอร์ชุดสำรอง กรุณาเลือกอีกครั้ง"));
    }
    let manifest = verify_set(&dir)?;
    let conn = app.db();
    Ok(RestorePreview {
        same_library: db::meta_get(&conn, "library_id")?.as_deref() == Some(manifest.library_id.as_str()),
        needs_upgrade: manifest.schema_version < db::latest_version(),
        current: read_counts(&conn),
        backup: BackupInfo { path: util::path_key(&dir), manifest },
    })
}

/// วางชุดสำรองที่ตรวจแล้วรอไว้ในโฟลเดอร์คลัง คลังที่เปิดอยู่ยังไม่ถูกแตะจนกว่าจะเปิดแอปครั้งถัดไป
pub fn stage_restore(app: &App, path: &str) -> Result<RestorePreview> {
    let preview = preview_restore(app, path)?;
    let data_dir = app.db_path.parent().unwrap_or(Path::new("."));
    let tmp = data_dir.join(format!("{PENDING_DB}.partial"));
    std::fs::copy(Path::new(path).join(DB_FILE), &tmp)?;
    if sha256_file(&tmp)? != preview.backup.manifest.sha256 {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::internal("คัดลอกชุดสำรองไม่ครบ กรุณาลองอีกครั้ง"));
    }
    std::fs::write(
        data_dir.join(PENDING_MANIFEST),
        serde_json::to_vec_pretty(&preview.backup.manifest).map_err(AppError::internal)?,
    )?;
    std::fs::rename(&tmp, data_dir.join(PENDING_DB))?;
    Ok(preview)
}

pub fn cancel_restore(app: &App) -> Result<()> {
    let data_dir = app.db_path.parent().unwrap_or(Path::new("."));
    for name in [PENDING_DB, PENDING_MANIFEST] {
        match std::fs::remove_file(data_dir.join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

/// สลับชุดที่รอกู้คืนเข้าเป็นคลัง ก่อนเปิดฐานข้อมูล (เรียกจาก `App::open` หลังได้ lock ของคลังแล้ว)
/// คลังปัจจุบันถูกเก็บเป็นชุดสำรองก่อนเสมอ ถ้าเก็บไม่ได้หรือไฟล์ที่รอไว้ไม่ผ่านการตรวจ จะไม่สลับและเปิดคลังเดิมต่อ
pub(crate) fn apply_pending_restore(data_dir: &Path) -> bool {
    let (pending, db_path) = (data_dir.join(PENDING_DB), data_dir.join(DB_FILE));
    if !pending.is_file() {
        return false;
    }
    if check_db(&pending).is_err() {
        let _ = std::fs::rename(&pending, data_dir.join("restore-rejected.sqlite"));
        let _ = std::fs::remove_file(data_dir.join(PENDING_MANIFEST));
        return false;
    }
    if db_path.is_file() {
        let dir = backups_dir(&db_path);
        if write_set(&db_path, &dir, "before_restore", &format!("before-restore-{}", stamp())).is_err() {
            return false;
        }
        prune(&dir, "before-restore-", SAFETY_KEEP);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(data_dir.join(format!("{DB_FILE}{suffix}")));
        }
    }
    if std::fs::rename(&pending, &db_path).is_err() {
        return false;
    }
    let _ = std::fs::remove_file(data_dir.join(PENDING_MANIFEST));
    true
}
