//! Puri Photo Search 2 — แกนกลางของแอป (ไม่ขึ้นกับ Tauri จึงทดสอบได้โดยไม่มีหน้าต่าง)
//!
//! `src-tauri` เป็น wrapper บาง ๆ ที่ส่งคำสั่งมายัง [`api::dispatch`];
//! กติกาข้อมูลทั้งหมด (Blueprint หมวด 6, 8, 9, 11, 13) อยู่ใน crate นี้

pub mod ai;
pub mod api;
pub mod assets;
pub mod catalog;
pub mod db;
pub mod error;
pub mod export;
pub mod faces;
pub mod import;
pub mod jobs;
pub mod legacy;
pub mod media;
pub mod people;
pub mod platform;
pub mod review;
pub mod util;

pub use error::{AppError, Result};

use rusqlite::Connection;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// ช่องทางแจ้ง UI หลัง commit สำเร็จ; UI ต้องทำงานได้แม้ไม่ได้รับ (query snapshot ใหม่ได้เสมอ)
pub trait EventSink: Send + Sync {
    fn data_changed(&self, data_revision: i64, scopes: &[&str]);
}

pub struct NoopSink;
impl EventSink for NoopSink {
    fn data_changed(&self, _: i64, _: &[&str]) {}
}

pub struct Options {
    /// โฟลเดอร์ข้อมูลหลัก (ฐานข้อมูลคลัง)
    pub data_dir: PathBuf,
    /// โฟลเดอร์ cache ที่สร้างใหม่ได้ (ภาพย่อ/ครอปใบหน้า)
    pub cache_dir: PathBuf,
    pub sink: Arc<dyn EventSink>,
    /// 'native' ในแอปจริง, 'dev_path_input' ใน dev bridge
    pub folder_picker: &'static str,
    /// เริ่ม worker งานเบื้องหลังทันที (ปิดได้ในเทสต์ที่ต้องคุมจังหวะเอง)
    pub start_worker: bool,
    /// ตัววิเคราะห์ใบหน้าของ build นี้ (None = build/เครื่องนี้ไม่มี พร้อมเหตุผลใน `face_engine_unavailable`)
    pub face_engine: Option<ai::FaceEngineProvider>,
    pub face_engine_unavailable: &'static str,
}

pub struct App {
    db: Mutex<Connection>,
    pub db_path: PathBuf,
    pub cache_dir: PathBuf,
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) folder_picker: &'static str,
    pub(crate) previews: Mutex<HashMap<String, Arc<import::PreviewSession>>>,
    pub(crate) export_plans: Mutex<HashMap<String, export::PlanData>>,
    pub(crate) worker: jobs::Worker,
    pub(crate) ai: Arc<ai::AiState>,
    pub(crate) review_cache: Mutex<Option<Arc<review::ReviewData>>>,
    /// กันสอง process เปิดคลังเดียวกัน (Blueprint 8.4)
    _lock: File,
}

impl App {
    pub fn open(opts: Options) -> Result<Arc<App>> {
        std::fs::create_dir_all(&opts.data_dir)?;
        std::fs::create_dir_all(opts.cache_dir.join("thumbs"))?;
        std::fs::create_dir_all(opts.cache_dir.join("faces"))?;

        let lock = File::options().create(true).truncate(false).write(true).open(opts.data_dir.join("library.lock"))?;
        if lock.try_lock().is_err() {
            return Err(AppError::new(
                "LIBRARY_LOCKED",
                "คลังนี้กำลังเปิดอยู่ในหน้าต่างอื่นของ Puri Photo Search กรุณาปิดหน้าต่างนั้นก่อน",
            ));
        }

        let db_path = opts.data_dir.join("library.sqlite");
        let conn = db::open_library(&db_path)?;
        jobs::recover_after_restart(&conn)?;

        let ai =
            Arc::new(ai::AiState::new(opts.face_engine, opts.face_engine_unavailable, &opts.data_dir.join("models")));
        let app = Arc::new(App {
            db: Mutex::new(conn),
            db_path: db_path.clone(),
            cache_dir: opts.cache_dir.clone(),
            sink: opts.sink.clone(),
            folder_picker: opts.folder_picker,
            previews: Mutex::new(HashMap::new()),
            export_plans: Mutex::new(HashMap::new()),
            worker: jobs::Worker::new(db_path, opts.cache_dir, opts.sink, ai.clone()),
            ai,
            review_cache: Mutex::new(None),
            _lock: lock,
        });
        if opts.start_worker {
            app.worker.start();
        }
        Ok(app)
    }

    pub(crate) fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// หยุด worker อย่างปลอดภัย (Quit): งานที่ทำค้างถูกคืนเข้าคิวและทำต่อเมื่อเปิดครั้งหน้า
    pub fn shutdown(&self) {
        self.worker.stop();
    }

    /// ทำงานเบื้องหลังที่รอคิวจนหมดบน thread ผู้เรียก (เทสต์/เครื่องมือ)
    pub fn run_pending_jobs(&self) -> Result<()> {
        self.worker.run_pending()
    }

    pub(crate) fn notify(&self, scopes: &[&str]) {
        if let Ok(rev) = db::data_revision(&self.db()) {
            self.sink.data_changed(rev, scopes);
        }
    }

    pub fn thumb_path(&self, revision_id: &str) -> PathBuf {
        thumb_path(&self.cache_dir, revision_id)
    }

    pub fn face_crop_path(&self, face_id: &str) -> PathBuf {
        self.cache_dir.join("faces").join(format!("{face_id}.jpg"))
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.worker.stop();
    }
}

/// ตำแหน่งภาพย่อของ revision; ผูกกับ revision จึงไม่ใช้ภาพเก่าเมื่อไฟล์เปลี่ยน (Blueprint 8.4)
pub fn thumb_path(cache_dir: &Path, revision_id: &str) -> PathBuf {
    let shard = revision_id.get(revision_id.len().saturating_sub(2)..).unwrap_or("00");
    cache_dir.join("thumbs").join(shard).join(format!("{revision_id}.jpg"))
}
