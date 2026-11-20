//! การเรียกระบบปฏิบัติการ: เปิดไฟล์/แสดงใน Finder
//! path มาจากฐานข้อมูลของคลังเสมอ (UI ส่งมาแค่ ID) และส่งเป็น argument ตรง ๆ ไม่ผ่าน shell

use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

fn trash_failed(detail: impl std::fmt::Display) -> AppError {
    AppError::new("ACCESS_DENIED", format!("ย้ายไฟล์ต้นฉบับไปถังขยะไม่ได้ จึงไม่ย่อรูปนี้ ({detail})"))
}

/// ย้ายไฟล์ไปถังขยะของระบบ (กู้คืนได้จนกว่าผู้ใช้จะล้างถังขยะ) — ใช้กับต้นฉบับที่ผู้ใช้สั่งย่อในที่เดิมเท่านั้น
///
/// ไม่มีทางลบถาวร: ที่ที่ไม่มีถังขยะ (โฟลเดอร์บนเครือข่าย, ไดรฟ์ USB บน Windows) คืนข้อผิดพลาดโดยไฟล์ยังอยู่ที่เดิม
/// คืนตำแหน่งของไฟล์ในถังขยะเมื่อระบบบอก (macOS)
pub fn move_to_trash(path: &Path) -> Result<Option<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSFileManager, NSURL};
        // NSFileManager โดยตรง ไม่ผ่าน Finder: ไม่ต้องขอสิทธิ์ควบคุม Finder และไม่มีเสียงทิ้งไฟล์ทุกรูป
        objc2::rc::autoreleasepool(|_| {
            let url = NSURL::from_file_path(path).ok_or_else(|| trash_failed("ตำแหน่งไฟล์ไม่ถูกต้อง"))?;
            let mut moved = None;
            NSFileManager::defaultManager()
                .trashItemAtURL_resultingItemURL_error(&url, Some(&mut moved))
                .map_err(|e| trash_failed(e.localizedDescription()))?;
            Ok(moved.and_then(|u| u.to_file_path()))
        })
    }
    #[cfg(windows)]
    {
        if !has_recycle_bin(path) {
            return Err(trash_failed("ไดรฟ์นี้ไม่มีถังขยะ"));
        }
        trash::delete(path).map_err(trash_failed)?;
        // ตัวจัดการไฟล์ของ Windows ข้ามรายการที่ทิ้งไม่ได้โดยไม่ถือเป็นข้อผิดพลาด
        if path.exists() {
            return Err(trash_failed("ระบบไม่ยอมย้ายไฟล์"));
        }
        Ok(None)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        Err(trash_failed("ระบบนี้ยังไม่รองรับ"))
    }
}

/// Windows มีถังขยะเฉพาะไดรฟ์ในเครื่อง (รวมฮาร์ดดิสก์ภายนอก) — ไดรฟ์เครือข่ายและแฟลชไดรฟ์ USB ลบถาวรทันที
/// จึงยอมเฉพาะไดรฟ์ชนิด "fixed"
#[cfg(windows)]
fn has_recycle_bin(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    const DRIVE_FIXED: u32 = 3;
    let Some(Component::Prefix(prefix)) = path.components().next() else { return false };
    let letter = match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
        _ => return false, // \\server\share
    };
    let root: Vec<u16> = format!("{}:\\", letter as char).encode_utf16().chain([0]).collect();
    // SAFETY: root เป็นสตริง UTF-16 ที่ปิดท้ายด้วย NUL และมีชีวิตอยู่ตลอดการเรียก
    unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
}

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
