//! การเรียกระบบปฏิบัติการ: เปิดไฟล์/แสดงใน Finder
//! path มาจากฐานข้อมูลของคลังเสมอ (UI ส่งมาแค่ ID) และส่งเป็น argument ตรง ๆ ไม่ผ่าน shell

use crate::error::{AppError, Result};
use std::path::Path;
use std::process::Command;

fn run(mut cmd: Command) -> Result<()> {
    // เทสต์อัตโนมัติตั้งตัวแปรนี้ เพื่อไม่ให้หน้าต่าง Finder/Preview เด้งขึ้นจริง
    if std::env::var_os("PURI_DISABLE_OPEN").is_some() {
        return Ok(());
    }
    let status = cmd.status().map_err(AppError::internal)?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::internal("ระบบเปิดรายการนี้ไม่ได้"))
    }
}

/// แสดงไฟล์หรือโฟลเดอร์ในตัวจัดการไฟล์ของระบบ
pub fn reveal(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(AppError::source_unavailable());
    }
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("/usr/bin/open");
        c.arg("-R").arg(path);
        run(c)
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        let _ = run(c); // explorer คืน exit code ไม่เป็นศูนย์แม้สำเร็จ
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let mut c = Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        run(c)
    }
}

/// เปิดไฟล์ต้นฉบับด้วยโปรแกรมเริ่มต้นของระบบ (แอปไม่แก้ไฟล์เอง)
pub fn open(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(AppError::source_unavailable());
    }
    #[cfg(target_os = "macos")]
    let c = {
        let mut c = Command::new("/usr/bin/open");
        c.arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let c = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let c = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    run(c)
}
