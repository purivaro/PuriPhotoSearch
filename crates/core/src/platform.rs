//! การเรียกระบบปฏิบัติการ: เปิดไฟล์/แสดงใน Finder
//! path มาจากฐานข้อมูลของคลังเสมอ (UI ส่งมาแค่ ID) และส่งเป็น argument ตรง ๆ ไม่ผ่าน shell

use crate::error::{AppError, Result};
use std::path::Path;
use std::process::Command;

/// โปรแกรมของระบบ (`curl`, `tar`, …) ที่เรียกโดยไม่ผ่าน shell
/// Windows: ใช้ตัวใน System32 เสมอ (ไม่ขึ้นกับ PATH ของผู้ใช้) และไม่ให้หน้าต่าง console เด้งขึ้นมา
pub(crate) fn system_tool(name: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let mut c = Command::new(Path::new(&root).join("System32").join(format!("{name}.exe")));
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
    #[cfg(not(windows))]
    Command::new(name)
}

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
    {
        // Explorer เปิดไฟล์ด้วยโปรแกรมเริ่มต้นเมื่อได้ path ของไฟล์ (ไม่ผ่าน cmd จึงไม่ต้อง escape ชื่อไฟล์)
        let mut c = Command::new("explorer");
        c.arg(path);
        let _ = run(c); // explorer คืน exit code ไม่เป็นศูนย์แม้สำเร็จ
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let c = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    #[cfg(not(target_os = "windows"))]
    run(c)
}
