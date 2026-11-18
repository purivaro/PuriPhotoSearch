//! เปลือก Tauri ของ Puri Photo Search 2 — บางที่สุดเท่าที่ทำได้
//!
//! ตรรกะทั้งหมดอยู่ใน `puri-core`; ไฟล์นี้ทำแค่: เปิดคลัง, ลงทะเบียนคำสั่ง IPC หนึ่งตัวต่อคำสั่งโดเมน,
//! เสิร์ฟรูปผ่าน protocol `puri://` ด้วย ID, เปิด native folder picker และส่ง event หลังข้อมูลเปลี่ยน

use puri_core::{api, App, AppError, EventSink, Options};
use serde_json::Value;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

type Shared = Arc<App>;

struct TauriSink(tauri::AppHandle);

impl EventSink for TauriSink {
    fn data_changed(&self, data_revision: i64, scopes: &[&str]) {
        // event เป็นเพียงสัญญาณให้ UI ถามข้อมูลใหม่ ไม่ใช่ที่เก็บสถานะ
        let _ = self.0.emit("data-changed", serde_json::json!({ "dataRevision": data_revision, "scopes": scopes }));
    }
}

/// สร้างคำสั่ง Tauri หนึ่งตัวต่อชื่อคำสั่งโดเมน แต่ละตัวเป็น wrapper ไปยัง `api::dispatch`
/// ซึ่งตรวจชนิดของ argument อีกชั้น; งานทำบน blocking pool ไม่ขวาง thread ของหน้าต่าง
macro_rules! domain_commands {
    ($($name:ident),* $(,)?) => {
        $(
            #[tauri::command]
            async fn $name(state: tauri::State<'_, Shared>, args: Option<Value>) -> Result<Value, AppError> {
                let app = state.inner().clone();
                tauri::async_runtime::spawn_blocking(move || api::dispatch(&app, stringify!($name), args.unwrap_or(Value::Null)))
                    .await
                    .map_err(AppError::internal)?
            }
        )*

        #[cfg(test)]
        const REGISTERED: &[&str] = &[$(stringify!($name)),*];

        fn register(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
            builder.invoke_handler(tauri::generate_handler![$($name,)* pick_folder, e2e_report])
        }
    };
}

domain_commands!(
    get_capabilities,
    get_library_summary,
    preview_import,
    cancel_import_preview,
    commit_import,
    list_events,
    get_event,
    update_event,
    rescan_event,
    set_event_cover,
    reveal_event_folder,
    query_photos,
    get_photo_details,
    reveal_photo,
    open_photo,
    search_suggestions,
    list_people,
    get_person,
    save_person,
    set_person_archived,
    set_person_cover,
    list_home_people,
    set_person_pinned,
    record_person_view,
    set_home_people_tab,
    get_person_quick_view,
    list_groups,
    create_group,
    delete_group,
    create_manual_face,
    assign_faces,
    unassign_faces,
    ignore_faces,
    list_jobs,
    get_job,
    list_job_errors,
    pause_job,
    resume_job,
    cancel_job,
    retry_failed_items,
    create_selection,
    preview_export,
    start_export,
    get_export_report,
    reveal_export_destination,
    preview_legacy_import,
);

/// เปิด native folder picker; คืน path ที่ผู้ใช้เลือกเอง (หรือ None เมื่อยกเลิก)
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle, title: String) -> Option<String> {
    #[cfg(debug_assertions)]
    if let Some(answer) = e2e::next_pick() {
        return answer;
    }
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_folder().and_then(|p| p.into_path().ok())
    })
    .await
    .ok()
    .flatten()
    .map(|p| p.to_string_lossy().into_owned())
}

/// รับรายงานจากสคริปต์ทดสอบ end-to-end (debug build เท่านั้น; release ไม่ทำอะไร)
#[tauri::command]
fn e2e_report(text: String) {
    #[cfg(debug_assertions)]
    e2e::write_report(&text);
    #[cfg(not(debug_assertions))]
    let _ = text;
}

fn asset_response(app: Option<Shared>, path: &str) -> tauri::http::Response<Vec<u8>> {
    let result = match app {
        Some(app) => api::asset(&app, path),
        None => Err(AppError::internal("คลังยังไม่พร้อม")),
    };
    let builder = tauri::http::Response::builder().header("Access-Control-Allow-Origin", "*");
    match result {
        Ok(asset) => builder
            .status(200)
            .header("Content-Type", asset.mime)
            // URL มี ?v= ตามรุ่นข้อมูล จึงให้ web view เก็บภาพไว้ใช้ซ้ำตอนเลื่อน grid ได้
            .header("Cache-Control", "private, max-age=300")
            .body(asset.bytes),
        Err(e) => builder
            .status(if e.code == "SOURCE_UNAVAILABLE" { 503 } else { 404 })
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&e).unwrap_or_default()),
    }
    .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("puri", |ctx, request, responder| {
            let app = ctx.app_handle().try_state::<Shared>().map(|s| s.inner().clone());
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(asset_response(app, &path)));
        })
        .setup(|app| {
            let (data, cache) = (app.path().app_data_dir()?, app.path().app_cache_dir()?);
            // เทสต์ชี้ debug build ไปที่โฟลเดอร์ทิ้งได้ แทนคลังจริงของผู้ใช้
            #[cfg(debug_assertions)]
            let (data, cache) = match std::env::var_os("PURI_DATA_DIR").map(std::path::PathBuf::from) {
                Some(dir) => (dir.join("data"), dir.join("cache")),
                None => (data, cache),
            };
            let opened = App::open(Options {
                data_dir: data.join("library"),
                cache_dir: cache,
                sink: Arc::new(TauriSink(app.handle().clone())),
                folder_picker: "native",
                start_worker: true,
            });
            match opened {
                Ok(core) => {
                    app.manage(core);
                    Ok(())
                }
                Err(e) => {
                    // เปิดคลังไม่ได้: บอกเหตุผลแล้วปิด ไม่สร้างคลังเปล่าทับของเดิม
                    app.dialog()
                        .message(e.message.clone())
                        .title("เปิดคลังภาพไม่ได้")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    Err(e.into())
                }
            }
        });

    #[cfg(debug_assertions)]
    let builder = e2e::install(builder);

    register(builder).build(tauri::generate_context!()).expect("error while building Puri Photo Search").run(
        |handle, event| {
            // Quit: หยุด worker ที่ขอบรายการ งานที่ค้างจะทำต่อเมื่อเปิดครั้งหน้า
            if let tauri::RunEvent::Exit = event {
                if let Some(app) = handle.try_state::<Shared>() {
                    app.shutdown();
                }
            }
        },
    );
}

/// hook ทดสอบ end-to-end เฉพาะ debug build (แนวทางเดียวกับ PuriCat): WKWebView ไม่มี WebDriver
/// จึงฉีดสคริปต์ที่ `PURI_E2E_SCRIPT` ชี้ เข้าไปในหน้าต่างจริงหลังหน้าโหลดเสร็จ release build ไม่มีโค้ดส่วนนี้
#[cfg(debug_assertions)]
mod e2e {
    use std::sync::Mutex;
    use tauri::webview::PageLoadEvent;

    static PICKS: Mutex<Option<Vec<String>>> = Mutex::new(None);

    /// native dialog ขับด้วยสคริปต์ไม่ได้: เทสต์เตรียมคำตอบของ picker ไว้ใน `PURI_E2E_PICK` (คั่นด้วยขึ้นบรรทัดใหม่)
    /// คืน None เมื่อไม่ได้อยู่ในเทสต์; เมื่ออยู่ในเทสต์จะไม่เปิด dialog จริงเลย — คำตอบหมดแล้วถือว่าผู้ใช้กดยกเลิก
    pub fn next_pick() -> Option<Option<String>> {
        let answers = std::env::var("PURI_E2E_PICK").ok()?;
        let mut picks = PICKS.lock().unwrap_or_else(|e| e.into_inner());
        let list = picks.get_or_insert_with(|| answers.lines().map(str::to_string).collect());
        Some((!list.is_empty()).then(|| list.remove(0)))
    }

    pub fn write_report(text: &str) {
        if let Some(path) = std::env::var_os("PURI_E2E_REPORT") {
            let _ = std::fs::write(path, text);
        }
    }

    pub fn install(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
        let Some(script) = std::env::var_os("PURI_E2E_SCRIPT").and_then(|path| std::fs::read_to_string(path).ok())
        else {
            return builder;
        };
        builder.on_page_load(move |webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                // WKWebView ที่ถูกบังจะพัก timer ทำให้เทสต์ค้าง
                let _ = webview.window().set_always_on_top(true);
                let _ = webview.eval(&script);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_domain_command_is_registered() {
        assert_eq!(super::REGISTERED, puri_core::api::COMMAND_NAMES, "คำสั่ง Tauri ต้องตรงกับ api::COMMAND_NAMES ทุกตัว");
    }
}
