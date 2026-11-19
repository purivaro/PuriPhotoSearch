//! อัปเดตแอปเอง: ตรวจรุ่นใหม่จาก manifest ที่ workflow `Release` เผยแพร่ ดาวน์โหลดเบื้องหลัง
//! ตรวจลายเซ็นกับกุญแจสาธารณะใน `tauri.conf.json` แล้วติดตั้งเมื่อผู้ใช้กด
//!
//! UI สั่งผ่านสามคำสั่งในไฟล์นี้เท่านั้น — web view ไม่ได้รับสิทธิ์ของ plugin updater โดยตรง
//! debug build ไม่ตรวจหารุ่นใหม่ เว้นแต่เทสต์ตั้ง `PURI_UPDATE_ENDPOINT` ชี้ไปที่ manifest ของตัวเอง (`e2e/update.sh`)

use crate::Shared;
use puri_core::AppError;
use serde::Serialize;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

/// สถานะที่ UI แสดง; ส่งซ้ำเป็น event `update-status` ทุกครั้งที่เปลี่ยน
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// `unsupported` | `idle` | `checking` | `up_to_date` | `downloading` | `ready` | `installing` | `failed`
    state: &'static str,
    current_version: String,
    /// รุ่นใหม่ที่พบ (ตั้งแต่ `downloading`)
    version: Option<String>,
    /// ข้อความ "มีอะไรใหม่" จาก manifest
    notes: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    /// เหตุที่ตรวจหรือดาวน์โหลดไม่สำเร็จ (`failed`)
    message: Option<String>,
    /// เวลาที่ตรวจกับเซิร์ฟเวอร์สำเร็จครั้งล่าสุด (มิลลิวินาทีจาก Unix epoch)
    checked_at: Option<u64>,
}

#[derive(Default)]
pub struct Updates(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    status: UpdateStatus,
    /// รุ่นใหม่ที่ดาวน์โหลดและตรวจลายเซ็นแล้ว รอผู้ใช้กดติดตั้ง
    pending: Option<(Update, Vec<u8>)>,
}

const BUSY: [&str; 4] = ["checking", "downloading", "ready", "installing"];

fn supported() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("PURI_UPDATE_ENDPOINT").is_some()
}

fn inner(app: &AppHandle) -> MutexGuard<'_, Inner> {
    app.state::<Updates>().inner().0.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// แก้สถานะแล้วแจ้ง UI; คืนสถานะหลังแก้
fn set(app: &AppHandle, change: impl FnOnce(&mut UpdateStatus)) -> UpdateStatus {
    let status = {
        let mut guard = inner(app);
        change(&mut guard.status);
        guard.status.clone()
    };
    let _ = app.emit("update-status", &status);
    status
}

/// ข้อความที่ผู้ใช้อ่านได้ของข้อผิดพลาดตอนตรวจหรือดาวน์โหลด
fn describe(error: &Error) -> String {
    match error {
        // ยังไม่มี manifest หรือปลายทางไม่ตอบ ก็คือ "ถามเซิร์ฟเวอร์ไม่ได้" สำหรับผู้ใช้
        Error::Reqwest(_) | Error::Network(_) | Error::ReleaseNotFound => {
            "ติดต่อเซิร์ฟเวอร์เพื่อตรวจหารุ่นใหม่ไม่ได้ ตรวจการเชื่อมต่ออินเทอร์เน็ตแล้วลองใหม่".into()
        }
        Error::Minisign(_) | Error::SignedVersionMismatch { .. } | Error::MissingSignedVersion => {
            "ไฟล์รุ่นใหม่ที่ดาวน์โหลดมาไม่ผ่านการตรวจลายเซ็น จึงไม่ถูกติดตั้ง ลองใหม่ภายหลัง".into()
        }
        other => format!("ตรวจหารุ่นใหม่ไม่สำเร็จ ({other})"),
    }
}

async fn find(app: &AppHandle) -> Result<Option<Update>, Error> {
    let handle = app.clone();
    let builder = app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        // Windows: ตัวติดตั้งปิด process นี้ทันที — หยุด worker ที่ขอบรายการก่อน เหมือนตอน Quit
        .on_before_exit(move || {
            if let Some(core) = handle.try_state::<Shared>() {
                core.shutdown();
            }
        });
    #[cfg(debug_assertions)]
    let builder = match std::env::var("PURI_UPDATE_ENDPOINT") {
        Ok(url) => builder.endpoints(vec![url.parse()?])?,
        Err(_) => builder,
    };
    builder.build()?.check().await
}

/// ดาวน์โหลดเบื้องหลังจนพร้อมติดตั้ง (`Update::download` ตรวจลายเซ็นก่อนคืนไฟล์)
fn download(app: AppHandle, update: Update) {
    tauri::async_runtime::spawn(async move {
        let mut downloaded = 0u64;
        let mut reported = 0u64;
        let progress = app.clone();
        let result = update
            .download(
                |chunk, total| {
                    downloaded += chunk as u64;
                    // แจ้ง UI ทุก ~256 KB ไม่ใช่ทุก chunk
                    if downloaded - reported >= 256 * 1024 {
                        reported = downloaded;
                        set(&progress, |s| (s.downloaded, s.total) = (downloaded, total));
                    }
                },
                || {},
            )
            .await;
        match result {
            Ok(bytes) => {
                inner(&app).pending = Some((update, bytes));
                set(&app, |s| s.state = "ready");
            }
            Err(e) => {
                set(&app, |s| (s.state, s.message) = ("failed", Some(describe(&e))));
            }
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    inner(&app).status.clone()
}

/// ตรวจหารุ่นใหม่; พบแล้วเริ่มดาวน์โหลดเบื้องหลังเอง คืนสถานะหลังตรวจเสร็จ (ไม่รอดาวน์โหลด)
#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateStatus {
    {
        let mut guard = inner(&app);
        if !supported() || BUSY.contains(&guard.status.state) {
            return guard.status.clone();
        }
        guard.status.state = "checking";
        guard.status.message = None;
    }
    set(&app, |_| {});
    match find(&app).await {
        // รุ่นที่เคยพบแต่ดาวน์โหลดไม่สำเร็จอาจถูกถอนไปแล้ว: ไม่ค้างเลขรุ่นนั้นไว้
        Ok(None) => set(&app, |s| {
            (s.state, s.checked_at) = ("up_to_date", Some(now_ms()));
            (s.version, s.notes) = (None, None);
        }),
        Ok(Some(update)) => {
            let status = set(&app, |s| {
                s.state = "downloading";
                s.version = Some(update.version.clone());
                s.notes = update.body.clone().filter(|notes| !notes.trim().is_empty());
                (s.downloaded, s.total, s.checked_at) = (0, None, Some(now_ms()));
            });
            download(app, update);
            status
        }
        Err(e) => set(&app, |s| (s.state, s.message) = ("failed", Some(describe(&e)))),
    }
}

/// ติดตั้งรุ่นที่ดาวน์โหลดไว้ แล้วเปิดแอปใหม่ (สำเร็จแล้วคำสั่งนี้ไม่คืนค่า)
/// macOS: สลับ `.app` ในที่เดิมแล้ว restart · Windows: ตัวติดตั้ง NSIS ปิดแอป ติดตั้ง แล้วเปิดให้ใหม่
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), AppError> {
    let Some((update, bytes)) = inner(&app).pending.take() else {
        return Err(AppError::invalid("ยังไม่มีรุ่นใหม่ที่ดาวน์โหลดเสร็จ"));
    };
    set(&app, |s| s.state = "installing");
    let installed = tauri::async_runtime::spawn_blocking(move || {
        let result = update.install(&bytes);
        (result, update, bytes)
    })
    .await;
    let failure = match installed {
        Ok((Ok(()), ..)) => app.restart(),
        Ok((Err(e), update, bytes)) => {
            // ไฟล์ยังอยู่ครบ ให้กดลองใหม่ได้โดยไม่ต้องดาวน์โหลดซ้ำ
            inner(&app).pending = Some((update, bytes));
            e.to_string()
        }
        Err(e) => e.to_string(),
    };
    let ready = inner(&app).pending.is_some();
    set(&app, |s| s.state = if ready { "ready" } else { "failed" });
    Err(AppError::new("INTERNAL", install_failure(&failure)).retryable())
}

fn install_failure(detail: &str) -> String {
    let moved = std::env::current_exe().ok().is_some_and(|exe| {
        let path = exe.to_string_lossy();
        path.starts_with("/Volumes/") || path.contains("/AppTranslocation/")
    });
    if moved {
        // เปิดจากไฟล์ .dmg โดยตรง: macOS ให้แอปอยู่ในที่อ่านอย่างเดียว จึงสลับไฟล์ไม่ได้
        format!(
            "ติดตั้งรุ่นใหม่ไม่ได้ เพราะแอปยังไม่ได้อยู่ในโฟลเดอร์ Applications — ลากแอปไปไว้ใน Applications แล้วเปิดจากที่นั่น ({detail})"
        )
    } else {
        format!("ติดตั้งรุ่นใหม่ไม่สำเร็จ แอปยังเป็นรุ่นเดิมและใช้งานต่อได้ ({detail})")
    }
}

/// สถานะเริ่มต้นตอนเปิดแอป
pub fn initial(app: &AppHandle) -> Updates {
    let status = UpdateStatus {
        state: if supported() { "idle" } else { "unsupported" },
        current_version: app.package_info().version.to_string(),
        ..Default::default()
    };
    Updates(Mutex::new(Inner { status, pending: None }))
}
