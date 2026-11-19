//! จุดรับคำสั่งทั้งหมดจาก UI (Blueprint หมวด 12)
//!
//! ทุกคำสั่งเป็นคำสั่งระดับโดเมนที่มี argument แบบมีชนิด: ไม่มี SQL, shell หรือ path อ่านไฟล์อิสระจากฝั่ง UI
//! `src-tauri` ลงทะเบียนหนึ่งคำสั่ง Tauri ต่อหนึ่งชื่อใน [`COMMAND_NAMES`] แล้วส่งมาที่ [`dispatch`];
//! dev bridge ใช้ dispatch ตัวเดียวกัน จึงไม่มีพฤติกรรมสองชุด

use crate::error::{AppError, Result};
use crate::{
    ai, assets, backup, catalog, db, export, faces, import, jobs, legacy, media, people, platform, relink, review,
    setup, App,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub use assets::Asset;

#[derive(Deserialize)]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
enum Command {
    GetCapabilities {},
    GetLibrarySummary {},

    PreviewImport {
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    CancelImportPreview {
        preview_id: String,
    },
    CommitImport(import::CommitArgs),

    ListEvents {
        text: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    GetEvent {
        event_id: String,
    },
    UpdateEvent(catalog::UpdateEventArgs),
    #[serde(rename_all = "camelCase")]
    RescanEvent {
        event_id: String,
    },
    #[serde(rename_all = "camelCase")]
    SetEventCover {
        event_id: String,
        photo_id: String,
    },
    #[serde(rename_all = "camelCase")]
    RevealEventFolder {
        event_id: String,
    },

    QueryPhotos(catalog::SearchParams),
    #[serde(rename_all = "camelCase")]
    GetPhotoDetails {
        photo_id: String,
    },
    #[serde(rename_all = "camelCase")]
    RevealPhoto {
        photo_id: String,
    },
    #[serde(rename_all = "camelCase")]
    OpenPhoto {
        photo_id: String,
    },
    SearchSuggestions {
        text: String,
        limit: Option<i64>,
    },

    ListPeople(people::ListArgs),
    #[serde(rename_all = "camelCase")]
    GetPerson {
        person_id: String,
    },
    SavePerson(people::SaveArgs),
    #[serde(rename_all = "camelCase")]
    SetPersonArchived {
        person_id: String,
        archived: bool,
    },
    #[serde(rename_all = "camelCase")]
    SetPersonCover {
        person_id: String,
        face_id: String,
    },
    ListHomePeople {
        limit: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    SetPersonPinned {
        person_id: String,
        pinned: bool,
    },
    #[serde(rename_all = "camelCase")]
    RecordPersonView {
        person_id: String,
    },
    SetHomePeopleTab {
        tab: String,
    },
    #[serde(rename_all = "camelCase")]
    GetPersonQuickView {
        person_id: String,
    },
    ListGroups {},
    CreateGroup {
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    DeleteGroup {
        group_id: String,
    },

    CreateManualFace(faces::CreateManualArgs),
    AssignFaces(faces::AssignArgs),
    #[serde(rename_all = "camelCase")]
    UnassignFaces {
        face_ids: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    IgnoreFaces {
        face_ids: Vec<String>,
    },

    #[serde(rename_all = "camelCase")]
    ListJobs {
        active_only: Option<bool>,
        limit: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    GetJob {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    ListJobErrors {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    PauseJob {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    ResumeJob {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CancelJob {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    RetryFailedItems {
        job_id: String,
    },

    CreateSelection(catalog::SearchParams),
    PreviewExport {
        selection: export::SelectionSpec,
        destination: String,
    },
    #[serde(rename_all = "camelCase")]
    StartExport {
        plan_id: String,
        request_id: String,
    },
    #[serde(rename_all = "camelCase")]
    GetExportReport {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    RevealExportDestination {
        job_id: String,
    },

    PreviewLegacyImport {
        path: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ImportLegacyPeople {
        path: Option<String>,
        request_id: String,
    },
    GetSetupState {},
    DismissSetup {},

    #[serde(rename_all = "camelCase")]
    PreviewRelink {
        event_id: String,
        new_folder: String,
    },
    #[serde(rename_all = "camelCase")]
    CommitRelink {
        event_id: String,
        new_folder: String,
        #[serde(default)]
        include_siblings: bool,
    },

    GetBackupStatus {},
    CreateBackup {
        destination: String,
    },
    PreviewRestore {
        path: String,
    },
    StageRestore {
        path: String,
    },
    CancelRestore {},

    InstallFaceModel {},
    #[serde(rename_all = "camelCase")]
    AnalyzeFaces {
        event_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    GetReviewOverview {
        event_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    GetReviewFaces {
        kind: String,
        id: String,
        event_id: Option<String>,
        review_revision: i64,
    },
    #[serde(rename_all = "camelCase")]
    RejectSuggestions {
        face_ids: Vec<String>,
        person_id: String,
    },
    #[serde(rename_all = "camelCase")]
    RestoreSuggestions {
        face_ids: Vec<String>,
        person_id: String,
    },
}

/// ชื่อคำสั่งทั้งหมด — ต้องตรงกับ `Commands` ใน `src/lib/ipc/types.ts` (มีเทสต์ตรวจ)
pub const COMMAND_NAMES: &[&str] = &[
    "get_capabilities",
    "get_library_summary",
    "preview_import",
    "cancel_import_preview",
    "commit_import",
    "list_events",
    "get_event",
    "update_event",
    "rescan_event",
    "set_event_cover",
    "reveal_event_folder",
    "query_photos",
    "get_photo_details",
    "reveal_photo",
    "open_photo",
    "search_suggestions",
    "list_people",
    "get_person",
    "save_person",
    "set_person_archived",
    "set_person_cover",
    "list_home_people",
    "set_person_pinned",
    "record_person_view",
    "set_home_people_tab",
    "get_person_quick_view",
    "list_groups",
    "create_group",
    "delete_group",
    "create_manual_face",
    "assign_faces",
    "unassign_faces",
    "ignore_faces",
    "list_jobs",
    "get_job",
    "list_job_errors",
    "pause_job",
    "resume_job",
    "cancel_job",
    "retry_failed_items",
    "create_selection",
    "preview_export",
    "start_export",
    "get_export_report",
    "reveal_export_destination",
    "preview_legacy_import",
    "import_legacy_people",
    "get_setup_state",
    "dismiss_setup",
    "preview_relink",
    "commit_relink",
    "get_backup_status",
    "create_backup",
    "preview_restore",
    "stage_restore",
    "cancel_restore",
    "install_face_model",
    "analyze_faces",
    "get_review_overview",
    "get_review_faces",
    "reject_suggestions",
    "restore_suggestions",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Availability {
    available: bool,
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    app_version: &'static str,
    folder_watch: Availability,
    schema_version: i64,
    platform: &'static str,
    image_formats: &'static [&'static str],
    face_analysis: ai::FaceAnalysisStatus,
    folder_picker: &'static str,
    library_path: String,
    cache_path: String,
}

fn ok<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(AppError::internal)
}

pub fn dispatch(app: &App, cmd: &str, args: Value) -> Result<Value> {
    let args = if args.is_null() { Value::Object(Default::default()) } else { args };
    let command: Command = serde_json::from_value(serde_json::json!({ "cmd": cmd, "args": args }))?;
    use Command::*;
    match command {
        GetCapabilities {} => ok(Capabilities {
            app_version: env!("CARGO_PKG_VERSION"),
            schema_version: db::latest_version(),
            platform: std::env::consts::OS,
            image_formats: media::SUPPORTED_EXTS,
            face_analysis: ai::status(app)?,
            folder_watch: Availability {
                available: false,
                reason: "รุ่นนี้ยังไม่ติดตามรูปใหม่อัตโนมัติ กด “ตรวจรูปใหม่” ที่กิจกรรมเมื่อเพิ่มรูปในโฟลเดอร์",
            },
            folder_picker: app.folder_picker,
            library_path: app.db_path.to_string_lossy().into_owned(),
            cache_path: app.cache_dir.to_string_lossy().into_owned(),
        }),
        GetLibrarySummary {} => ok(catalog::library_summary(&app.db())?),

        PreviewImport { path } => ok(import::preview(app, &path)?),
        CancelImportPreview { preview_id } => {
            import::cancel_preview(app, &preview_id);
            ok(())
        }
        CommitImport(a) => ok(import::commit(app, a)?),

        ListEvents { text } => ok(catalog::list_events(&app.db(), text.as_deref())?),
        GetEvent { event_id } => ok(catalog::get_event(&app.db(), &event_id)?),
        UpdateEvent(a) => ok(catalog::update_event(app, a)?),
        RescanEvent { event_id } => ok(jobs::rescan_event(app, &event_id)?),
        SetEventCover { event_id, photo_id } => ok(catalog::set_event_cover(app, &event_id, &photo_id)?),
        RevealEventFolder { event_id } => {
            let folder = catalog::event_folder(&app.db(), &event_id)?;
            ok(platform::reveal(Path::new(&folder))?)
        }

        QueryPhotos(p) => {
            if p.include_suggested {
                review::ensure_matches(app)?;
            }
            ok(catalog::query_photos(&app.db(), &p)?)
        }
        GetPhotoDetails { photo_id } => ok(catalog::get_photo_details(app, &photo_id)?),
        RevealPhoto { photo_id } => {
            let path = catalog::photo_path(&app.db(), &photo_id)?;
            ok(platform::reveal(Path::new(&path))?)
        }
        OpenPhoto { photo_id } => {
            let path = catalog::photo_path(&app.db(), &photo_id)?;
            ok(platform::open(Path::new(&path))?)
        }
        SearchSuggestions { text, limit } => ok(catalog::suggestions(&app.db(), &text, limit)?),

        ListPeople(a) => {
            review::ensure_matches(app)?;
            ok(people::list(&app.db(), &a)?)
        }
        GetPerson { person_id } => {
            review::ensure_matches(app)?;
            ok(people::get(&app.db(), &person_id)?)
        }
        SavePerson(a) => ok(people::save(app, a)?),
        SetPersonArchived { person_id, archived } => ok(people::set_archived(app, &person_id, archived)?),
        SetPersonCover { person_id, face_id } => ok(people::set_cover(app, &person_id, &face_id)?),
        ListHomePeople { limit } => {
            review::ensure_matches(app)?;
            ok(people::home_people(&app.db(), limit)?)
        }
        SetPersonPinned { person_id, pinned } => ok(people::set_pinned(app, &person_id, pinned)?),
        RecordPersonView { person_id } => ok(people::record_view(app, &person_id)?),
        SetHomePeopleTab { tab } => ok(people::set_home_tab(app, &tab)?),
        GetPersonQuickView { person_id } => {
            review::ensure_matches(app)?;
            ok(people::quick_view(&app.db(), &person_id)?)
        }
        ListGroups {} => ok(people::list_groups(&app.db())?),
        CreateGroup { name } => ok(people::create_group(app, &name)?),
        DeleteGroup { group_id } => ok(people::delete_group(app, &group_id)?),

        CreateManualFace(a) => ok(faces::create_manual(app, a)?),
        AssignFaces(a) => ok(faces::assign(app, a)?),
        UnassignFaces { face_ids } => ok(faces::unassign(app, &face_ids)?),
        IgnoreFaces { face_ids } => ok(faces::ignore(app, &face_ids)?),

        ListJobs { active_only, limit } => {
            ok(jobs::list(&app.db(), active_only.unwrap_or(false), limit.unwrap_or(100))?)
        }
        GetJob { job_id } => ok(jobs::get(&app.db(), &job_id)?),
        ListJobErrors { job_id } => ok(jobs::list_errors(&app.db(), &job_id)?),
        PauseJob { job_id } => ok(jobs::pause(app, &job_id)?),
        ResumeJob { job_id } => ok(jobs::resume(app, &job_id)?),
        CancelJob { job_id } => ok(jobs::cancel(app, &job_id)?),
        RetryFailedItems { job_id } => ok(jobs::retry_failed(app, &job_id)?),

        CreateSelection(p) => ok(catalog::create_selection(app, &p)?),
        PreviewExport { selection, destination } => ok(export::preview(app, &selection, &destination)?),
        StartExport { plan_id, request_id } => ok(export::start(app, &plan_id, &request_id)?),
        GetExportReport { job_id } => ok(export::report(&app.db(), &job_id)?),
        RevealExportDestination { job_id } => {
            let destination = export::report(&app.db(), &job_id)?.destination;
            ok(platform::reveal(Path::new(&destination))?)
        }

        PreviewLegacyImport { path } => ok(legacy::dry_run(path.as_deref())?),
        ImportLegacyPeople { path, request_id } => ok(legacy::import_people(app, path.as_deref(), &request_id)?),
        GetSetupState {} => ok(setup::state(app)?),
        DismissSetup {} => ok(setup::dismiss(app)?),

        PreviewRelink { event_id, new_folder } => ok(relink::preview(app, &event_id, &new_folder)?),
        CommitRelink { event_id, new_folder, include_siblings } => {
            ok(relink::commit(app, &event_id, &new_folder, include_siblings)?)
        }

        GetBackupStatus {} => ok(backup::status(app)?),
        CreateBackup { destination } => ok(backup::create(app, &destination)?),
        PreviewRestore { path } => ok(backup::preview_restore(app, &path)?),
        StageRestore { path } => ok(backup::stage_restore(app, &path)?),
        CancelRestore {} => {
            backup::cancel_restore(app)?;
            ok(backup::status(app)?)
        }

        InstallFaceModel {} => ok(ai::install(app)?),
        AnalyzeFaces { event_id } => ok(serde_json::json!({ "jobIds": ai::analyze(app, event_id.as_deref())? })),
        GetReviewOverview { event_id } => ok(review::overview(app, event_id.as_deref())?),
        GetReviewFaces { kind, id, event_id, review_revision } => {
            ok(review::group_faces(app, &kind, &id, event_id.as_deref(), review_revision)?)
        }
        RejectSuggestions { face_ids, person_id } => {
            ok(serde_json::json!({ "dataRevision": review::reject(app, &face_ids, &person_id)? }))
        }
        RestoreSuggestions { face_ids, person_id } => {
            ok(serde_json::json!({ "dataRevision": review::restore(app, &face_ids, &person_id)? }))
        }
    }
}

/// ภาพสำหรับ `<img>`: `thumb/{photoId}`, `photo/{photoId}`, `face/{faceId}`, `draft/{previewId}/{draftId}`
pub fn asset(app: &App, request_path: &str) -> Result<Asset> {
    assets::resolve(app, request_path)
}
