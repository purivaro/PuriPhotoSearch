//! เปิดฐานข้อมูลคลังและทำ schema migration (Blueprint 8.4)

use crate::error::{AppError, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// "PURI" — ใช้แยกไฟล์คลังของเราออกจาก SQLite อื่น (เช่นฐานของรุ่น Python)
pub const APPLICATION_ID: i64 = 0x5055_5249;

const MIGRATIONS: &[(i64, &str)] =
    &[(1, include_str!("../migrations/0001_init.sql")), (2, include_str!("../migrations/0002_faces.sql"))];

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map(|m| m.0).unwrap_or(0)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(15))?;
    let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    if fk != 1 {
        return Err(AppError::internal("เปิดการบังคับ foreign key ไม่สำเร็จ"));
    }
    Ok(())
}

/// เปิดการเชื่อมต่อเพิ่ม (เช่นของ worker) กับคลังที่ migrate แล้ว
pub fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

/// เปิดหรือสร้างคลัง: ไม่เขียนทับไฟล์ที่ไม่ใช่คลังของเรา และไม่ลด schema ของคลังที่ใหม่กว่า
pub fn open_library(path: &Path) -> Result<Connection> {
    let existed = path.exists();
    if existed {
        // ตรวจแบบอ่านอย่างเดียวก่อน จะได้ไม่แตะไฟล์ที่ผู้ใช้เลือกผิด
        let ro = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let app_id: i64 = ro.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let version: i64 = ro.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let tables: i64 = ro.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'", [], |r| r.get(0))?;
        if tables > 0 && app_id != APPLICATION_ID {
            return Err(AppError::new("DATABASE_CORRUPT", "ไฟล์นี้ไม่ใช่คลังของ Puri Photo Search 2 จึงไม่เปิดและไม่แก้ไขไฟล์"));
        }
        if version > latest_version() {
            return Err(AppError::new("LIBRARY_TOO_NEW", "คลังนี้สร้างด้วยแอปรุ่นใหม่กว่า กรุณาอัปเดตแอปก่อนเปิด"));
        }
    }
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    // migration ที่สร้างตารางใหม่แทนของเดิมต้องปิด foreign keys ชั่วคราว (ทำได้เฉพาะนอก transaction)
    // แล้วตรวจความสมบูรณ์ด้วย foreign_key_check หลังเปิดกลับ
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    for (v, sql) in MIGRATIONS {
        if *v > version {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", v)?;
            tx.commit()?;
        }
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    let violations: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
    if violations > 0 {
        return Err(AppError::new("DATABASE_CORRUPT", "พบความสัมพันธ์ของข้อมูลในคลังไม่สมบูรณ์ กรุณากู้จากชุดสำรอง"));
    }
    if !existed || meta_get(&conn, "library_id")?.is_none() {
        meta_set(&conn, "library_id", &crate::util::new_id())?;
        meta_set(&conn, "name", "คลังภาพของฉัน")?;
        meta_set(&conn, "created_at", &crate::util::now())?;
        meta_set(&conn, "data_revision", "1")?;
    }
    Ok(conn)
}

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

pub fn data_revision(conn: &Connection) -> Result<i64> {
    Ok(meta_get(conn, "data_revision")?.and_then(|v| v.parse().ok()).unwrap_or(1))
}

/// เรียกภายใน transaction ที่แก้ข้อมูล เพื่อให้ UI รู้ว่า cache เก่าแล้ว
pub fn bump_revision(conn: &Connection) -> Result<i64> {
    conn.execute("UPDATE meta SET value = CAST(value AS INTEGER) + 1 WHERE key = 'data_revision'", [])?;
    data_revision(conn)
}

/// ตัวนับที่เพิ่มเมื่อใบหน้า เวกเตอร์ คำยืนยัน หรือคำปฏิเสธเปลี่ยน — ใช้ทำให้ผลจัดกลุ่ม/ข้อเสนอที่ cache ไว้หมดอายุ
pub fn identity_revision(conn: &Connection) -> Result<i64> {
    Ok(meta_get(conn, "identity_revision")?.and_then(|v| v.parse().ok()).unwrap_or(1))
}

pub fn bump_identity(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES ('identity_revision', '2')
         ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1",
        [],
    )?;
    Ok(())
}
