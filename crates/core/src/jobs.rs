//! งานเบื้องหลังแบบถาวร (Blueprint หมวด 9)
//!
//! สถานะของงานอยู่ในตาราง `jobs` ทั้งหมด: worker อ่านคำขอพัก/ยกเลิกจากฐานข้อมูลที่ขอบของแต่ละรายการ
//! จึงไม่มีสถานะในหน่วยความจำที่หายเมื่อแอปปิด และ UI ถาม snapshot ใหม่ได้ทุกเมื่อ

use crate::error::{AppError, Result};
use crate::import::is_hidden;
use crate::media::{self, FileKind, FileStamp};
use crate::{db, util, App, EventSink};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

pub const ACTIVE_STATES: &str = "('queued','running','pausing','paused','blocked','cancel_requested')";
const DISCOVERY_BATCH: usize = 200;
/// ไม่ส่งความคืบหน้าถี่เกิน ~4 ครั้ง/วินาที (Blueprint 9.3)
const FLUSH_EVERY: Duration = Duration::from_millis(250);

pub(crate) enum Outcome {
    Done,
    Paused,
    Cancelled,
    /// แอปกำลังปิด: คืนงานเข้าคิว
    Stopped,
    Blocked(String),
}

#[derive(PartialEq, Eq)]
pub(crate) enum Control {
    Continue,
    Pause,
    Cancel,
    Stop,
}

pub(crate) struct Inner {
    db_path: PathBuf,
    cache_dir: PathBuf,
    sink: Arc<dyn EventSink>,
    ai: Arc<crate::ai::AiState>,
    stop: AtomicBool,
    wake: Mutex<bool>,
    cv: Condvar,
}

pub struct Worker {
    inner: Arc<Inner>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

pub(crate) struct JobRow {
    pub id: String,
    pub kind: String,
    pub event_id: Option<String>,
    pub done: i64,
    pub failed: i64,
    pub skipped: i64,
    pub dedup_key: Option<String>,
}

/// สิ่งที่ตัวรันงานแต่ละชนิดใช้ร่วมกัน: ตรวจคำขอพัก/ยกเลิก บันทึกความคืบหน้า และบันทึกข้อผิดพลาดรายไฟล์
pub(crate) struct JobCtx<'a> {
    inner: &'a Inner,
    pub conn: &'a Connection,
    pub job: &'a JobRow,
}

impl JobCtx<'_> {
    pub fn ai(&self) -> &crate::ai::AiState {
        &self.inner.ai
    }

    pub fn cache_dir(&self) -> &Path {
        &self.inner.cache_dir
    }

    pub fn control(&self) -> Result<Control> {
        if self.inner.stop.load(Ordering::SeqCst) {
            return Ok(Control::Stop);
        }
        let state: String =
            self.conn.query_row("SELECT state FROM jobs WHERE id = ?1", [&self.job.id], |r| r.get(0))?;
        Ok(match state.as_str() {
            "pausing" => Control::Pause,
            "cancel_requested" => Control::Cancel,
            _ => Control::Continue,
        })
    }

    pub fn outcome_for(c: Control) -> Outcome {
        match c {
            Control::Pause => Outcome::Paused,
            Control::Cancel => Outcome::Cancelled,
            _ => Outcome::Stopped,
        }
    }

    pub fn stage(&self, stage: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET stage = ?2, updated_at = ?3 WHERE id = ?1",
            params![self.job.id, stage, util::now()],
        )?;
        Ok(())
    }

    pub fn progress(&self, total: i64, done: i64, failed: i64, skipped: i64, total_known: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET total = ?2, done = ?3, failed = ?4, skipped = ?5, total_known = ?6, updated_at = ?7 WHERE id = ?1",
            params![self.job.id, total, done, failed, skipped, total_known, util::now()],
        )?;
        let rev = db::bump_revision(self.conn)?;
        self.inner.sink.data_changed(rev, &["jobs", "events", "photos"]);
        Ok(())
    }

    pub fn error(&self, photo_id: Option<&str>, filename: &str, e: &AppError) -> Result<()> {
        self.conn.execute(
            "INSERT INTO job_errors(job_id, photo_id, filename, code, message, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![self.job.id, photo_id, filename, e.code, e.message, util::now()],
        )?;
        Ok(())
    }
}

impl Worker {
    pub fn new(db_path: PathBuf, cache_dir: PathBuf, sink: Arc<dyn EventSink>, ai: Arc<crate::ai::AiState>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db_path,
                cache_dir,
                sink,
                ai,
                stop: AtomicBool::new(false),
                wake: Mutex::new(false),
                cv: Condvar::new(),
            }),
            handle: Mutex::new(None),
        }
    }

    pub fn start(&self) {
        let mut handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        if handle.is_some() {
            return;
        }
        let inner = self.inner.clone();
        *handle = std::thread::Builder::new()
            .name("puri-jobs".into())
            .spawn(move || {
                let Ok(conn) = db::connect(&inner.db_path) else { return };
                while !inner.stop.load(Ordering::SeqCst) {
                    match run_next(&inner, &conn) {
                        Ok(true) => continue,
                        Ok(false) => {
                            if maintenance(&inner, &conn).unwrap_or(false) {
                                continue;
                            }
                        }
                        Err(e) => eprintln!("puri-jobs: {e}"),
                    }
                    let mut woken = inner.wake.lock().unwrap_or_else(|e| e.into_inner());
                    if !*woken {
                        woken = inner
                            .cv
                            .wait_timeout(woken, Duration::from_secs(2))
                            .map(|r| r.0)
                            .unwrap_or_else(|e| e.into_inner().0);
                    }
                    *woken = false;
                }
            })
            .ok();
    }

    pub fn wake(&self) {
        *self.inner.wake.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.inner.cv.notify_all();
    }

    pub fn stop(&self) {
        self.inner.stop.store(true, Ordering::SeqCst);
        self.wake();
        if let Some(h) = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = h.join();
        }
    }

    /// ทำงานที่รอคิวจนหมดบน thread ผู้เรียก — สำหรับเทสต์และเครื่องมือที่ต้องการผลแน่นอน
    pub fn run_pending(&self) -> Result<()> {
        let conn = db::connect(&self.inner.db_path)?;
        while run_next(&self.inner, &conn)? || maintenance(&self.inner, &conn)? {}
        Ok(())
    }
}

/// เปิดแอปใหม่หลังปิดกลางคัน (Blueprint 9.2): งานที่กำลังทำถูกคืนเข้าคิวพร้อมป้าย "กู้กลับมา";
/// งานที่ผู้ใช้สั่งพักยังพักอยู่; งานที่สั่งยกเลิกค้างไว้ถือว่ายกเลิกแล้ว
pub fn recover_after_restart(conn: &Connection) -> Result<()> {
    let now = util::now();
    conn.execute("UPDATE jobs SET state = 'queued', recovered = 1, updated_at = ?1 WHERE state = 'running'", [&now])?;
    conn.execute("UPDATE jobs SET state = 'paused', recovered = 1, updated_at = ?1 WHERE state = 'pausing'", [&now])?;
    let cancelled: Vec<String> = conn
        .prepare("SELECT id FROM jobs WHERE state = 'cancel_requested'")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in cancelled {
        finish_cancelled(conn, &id)?;
    }
    Ok(())
}

fn finish_cancelled(conn: &Connection, job_id: &str) -> Result<()> {
    let now = util::now();
    conn.execute(
        "UPDATE jobs SET state = 'cancelled', finished_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![job_id, now],
    )?;
    conn.execute(
        "UPDATE export_items SET status = 'cancelled' WHERE status = 'planned' AND run_id IN (SELECT id FROM export_runs WHERE job_id = ?1)",
        [job_id],
    )?;
    conn.execute(
        "UPDATE shrink_items SET status = 'cancelled' WHERE status = 'planned' AND run_id IN (SELECT id FROM shrink_runs WHERE job_id = ?1)",
        [job_id],
    )?;
    Ok(())
}

fn run_next(inner: &Inner, conn: &Connection) -> Result<bool> {
    let job = conn
        .query_row(
            "SELECT id, kind, event_id, done, failed, skipped, dedup_key FROM jobs WHERE state = 'queued'
             ORDER BY created_at, id LIMIT 1",
            [],
            |r| {
                Ok(JobRow {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    event_id: r.get(2)?,
                    done: r.get(3)?,
                    failed: r.get(4)?,
                    skipped: r.get(5)?,
                    dedup_key: r.get(6)?,
                })
            },
        )
        .optional()?;
    let Some(job) = job else { return Ok(false) };
    let now = util::now();
    let claimed = conn.execute(
        "UPDATE jobs SET state = 'running', started_at = COALESCE(started_at, ?2), updated_at = ?2, blocked_reason = NULL
         WHERE id = ?1 AND state = 'queued'",
        params![job.id, now],
    )?;
    if claimed == 0 {
        return Ok(true); // ผู้ใช้เพิ่งพัก/ยกเลิกงานนี้
    }
    let ctx = JobCtx { inner, conn, job: &job };
    let outcome = match job.kind.as_str() {
        "catalog" => run_catalog(&ctx),
        "export" => crate::export::run(&ctx),
        "shrink" => crate::shrink::run(&ctx),
        "model" if crate::subject::is_install_job(job.dedup_key.as_deref()) => crate::subject::run_install(&ctx),
        "model" => crate::ai::run_install(&ctx),
        "cull" => crate::cull::run(&ctx),
        other => Err(AppError::internal(format!("ไม่รู้จักชนิดงาน {other}"))),
    };
    let now = util::now();
    match outcome {
        Ok(Outcome::Done) => {
            conn.execute(
                "UPDATE jobs SET state = CASE WHEN failed > 0 THEN 'completed_with_errors' ELSE 'completed' END,
                   stage = 'done', total_known = 1, finished_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![job.id, now],
            )?;
        }
        Ok(Outcome::Paused) => {
            conn.execute("UPDATE jobs SET state = 'paused', updated_at = ?2 WHERE id = ?1", params![job.id, now])?;
        }
        Ok(Outcome::Cancelled) => finish_cancelled(conn, &job.id)?,
        Ok(Outcome::Stopped) => {
            conn.execute(
                "UPDATE jobs SET state = 'queued', updated_at = ?2 WHERE id = ?1 AND state = 'running'",
                params![job.id, now],
            )?;
        }
        Ok(Outcome::Blocked(reason)) => {
            conn.execute(
                "UPDATE jobs SET state = 'blocked', blocked_reason = ?2, updated_at = ?3 WHERE id = ?1",
                params![job.id, reason, now],
            )?;
        }
        Err(e) => {
            conn.execute(
                "UPDATE jobs SET state = 'failed', blocked_reason = ?2, finished_at = ?3, updated_at = ?3 WHERE id = ?1",
                params![job.id, e.message, now],
            )?;
        }
    }
    let rev = db::bump_revision(conn)?;
    inner.sink.data_changed(rev, &["jobs", "events", "photos"]);
    Ok(true)
}

/// สร้างงาน "เตรียมข้อมูลรูป" ของกิจกรรม หรือคืนงานเดิมที่ยังไม่จบ (ไม่สร้างงานซ้ำ)
pub fn enqueue_catalog(conn: &Connection, event_id: &str, title: &str) -> Result<String> {
    let dedup = format!("catalog:{event_id}");
    let existing: Option<String> = conn
        .query_row(&format!("SELECT id FROM jobs WHERE dedup_key = ?1 AND state IN {ACTIVE_STATES}"), [&dedup], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = util::new_id();
    let now = util::now();
    conn.execute(
        "INSERT INTO jobs(id, kind, state, stage, title, event_id, dedup_key, created_at, updated_at)
         VALUES (?1, 'catalog', 'queued', 'discovery', ?2, ?3, ?4, ?5, ?5)",
        params![id, title, event_id, dedup, now],
    )?;
    Ok(id)
}

// ───────────────────────── งาน catalog: ตรวจรายการ → เตรียมข้อมูลรูป ─────────────────────────

struct Binding {
    source_id: String,
    folder: PathBuf,
    recursive: bool,
    /// นำไฟล์ RAW เข้าด้วย
    include_raw: bool,
}

struct ThumbItem {
    need_thumb: bool,
    need_faces: bool,
    revision_id: String,
    photo_id: String,
    path: PathBuf,
    filename: String,
    stamp: FileStamp,
    /// แต่งก่อน สแกนทีหลัง (หลวงพี่เลือก 21 พ.ย. 2569): รูปที่แต่งแสงสีไว้แล้วหาใบหน้าจากภาพที่แต่ง (เฉพาะแสงสี/เฉพาะส่วน/รีทัช
    /// ไม่ตัดไม่หมุน — ตำแหน่งใบหน้ายังเป็นพิกัดของภาพเดิม และใบหน้าที่อยู่นอกกรอบตัดยังถูกหา) · None = หาจากไฟล์ตามเดิม
    edited: Option<crate::edit::EditSettings>,
}

/// ค่าที่ใช้หาใบหน้าจากภาพที่แต่ง: การแต่งของรูปที่ไม่ได้แต่งจาก RAW คู่ (รูปคู่หาใบหน้าจาก JPEG ที่แต่งมาแล้ว) โดยไม่ตัด/หมุน/เอียง
fn edited_for_faces(settings_json: Option<String>) -> Option<crate::edit::EditSettings> {
    let s: crate::edit::EditSettings = serde_json::from_str(&settings_json?).ok()?;
    if s.from_raw {
        return None;
    }
    let mut light = crate::edit::EditSettings { rotate: 0, straighten: 0.0, crop: None, ..s };
    // Mask ตัวคนต้องใช้โมเดลอีกตัว (ราว 7 วินาทีต่อรูป) แต่แค่ลดรายละเอียดบนตัวคน — ตรวจใบหน้าจากภาพที่ไม่มีส่วนนี้
    light.masks.retain(|m| !matches!(m.area, crate::masks::Area::Subject { .. }));
    (!light.adjust.is_neutral() || light.has_masks() || light.has_retouch()).then_some(light)
}

fn run_catalog(ctx: &JobCtx) -> Result<Outcome> {
    let conn = ctx.conn;
    let event_id = ctx.job.event_id.clone().ok_or_else(|| AppError::internal("งานไม่มีกิจกรรม"))?;
    let bindings: Vec<Binding> = conn
        .prepare(
            "SELECT source_id, folder_path, recursive, include_raw FROM event_source_bindings WHERE event_id = ?1 AND active = 1",
        )?
        .query_map([&event_id], |r| {
            Ok(Binding {
                source_id: r.get(0)?,
                folder: PathBuf::from(r.get::<_, String>(1)?),
                recursive: r.get(2)?,
                include_raw: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    for b in &bindings {
        if !b.folder.is_dir() {
            return Ok(Outcome::Blocked("โฟลเดอร์ต้นทางไม่พร้อมใช้งาน เชื่อมต่อไดรฟ์แล้วสั่งทำต่อ".into()));
        }
    }

    // ── ขั้นตรวจรายการ: ลงทะเบียนไฟล์ใหม่ และตรวจไฟล์เดิมที่เปลี่ยน
    ctx.stage("discovery")?;
    let scan_stamp = util::now();
    let (mut found, mut skipped) = (0i64, 0i64);
    // ไฟล์ RAW รอจับคู่กับ JPEG หลังเดินครบทุกโฟลเดอร์ของกิจกรรม (DECISIONS ข้อ 26): (path, source, รวม RAW)
    let mut raws: Vec<(PathBuf, String, bool)> = vec![];
    let mut jpegs: Vec<PathBuf> = vec![];
    for b in &bindings {
        // โฟลเดอร์ย่อยที่ผูกกับกิจกรรมอื่นอยู่แล้วเป็นของกิจกรรมนั้น: หนึ่งตำแหน่งไฟล์มีหนึ่งกิจกรรมหลัก
        let folder_key = util::path_key(&b.folder);
        let prefix = format!("{folder_key}{}", std::path::MAIN_SEPARATOR);
        let other_dirs: Vec<String> = conn
            .prepare(
                "SELECT folder_key FROM event_source_bindings
                 WHERE active = 1 AND recursive = 1 AND event_id <> ?1 AND substr(folder_key, 1, ?2) = ?3",
            )?
            .query_map(params![event_id, prefix.chars().count() as i64, prefix], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let loose_taken: bool = b.recursive
            && conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM event_source_bindings WHERE active = 1 AND recursive = 0 AND event_id <> ?1 AND folder_key = ?2)",
                params![event_id, folder_key],
                |r| r.get(0),
            )?;

        let mut walker = WalkDir::new(&b.folder).min_depth(1).sort_by_file_name();
        if !b.recursive {
            walker = walker.max_depth(1);
        }
        let mut batch: Vec<PathBuf> = Vec::with_capacity(DISCOVERY_BATCH);
        let iter = walker.into_iter().filter_entry(|e| {
            !is_hidden(e.file_name()) && !(e.file_type().is_dir() && other_dirs.contains(&util::path_key(e.path())))
        });
        for entry in iter {
            let Ok(entry) = entry else { continue };
            let kind = media::classify(entry.path()).0;
            if !entry.file_type().is_file() || !matches!(kind, FileKind::Supported | FileKind::Raw) {
                continue;
            }
            if loose_taken && entry.depth() == 1 {
                continue;
            }
            if kind == FileKind::Raw {
                raws.push((entry.into_path(), b.source_id.clone(), b.include_raw));
                continue;
            }
            jpegs.push(entry.path().to_path_buf());
            batch.push(entry.into_path());
            if batch.len() >= DISCOVERY_BATCH {
                let (f, s) = register_batch(ctx, &event_id, &b.source_id, &batch, &scan_stamp)?;
                found += f;
                skipped += s;
                batch.clear();
                ctx.progress(found, ctx.job.done, ctx.job.failed, skipped, false)?;
                let c = ctx.control()?;
                if c != Control::Continue {
                    return Ok(JobCtx::outcome_for(c));
                }
            }
        }
        let (f, s) = register_batch(ctx, &event_id, &b.source_id, &batch, &scan_stamp)?;
        found += f;
        skipped += s;
    }
    // RAW ที่ไม่มีคู่: เป็นรูปเมื่อกิจกรรมเลือกรวม RAW (DECISIONS ข้อ 25) หรือเป็นรูปในคลังอยู่แล้ว
    let mut by_source: Vec<(String, Vec<PathBuf>)> = vec![];
    for (path, source_id) in pair_raws(ctx, &event_id, &jpegs, raws, &scan_stamp)? {
        match by_source.iter_mut().find(|(s, _)| *s == source_id) {
            Some((_, paths)) => paths.push(path),
            None => by_source.push((source_id, vec![path])),
        }
    }
    for (source_id, paths) in &by_source {
        for chunk in paths.chunks(DISCOVERY_BATCH) {
            let (f, s) = register_batch(ctx, &event_id, source_id, chunk, &scan_stamp)?;
            found += f;
            skipped += s;
            ctx.progress(found, ctx.job.done, ctx.job.failed, skipped, false)?;
            let c = ctx.control()?;
            if c != Control::Continue {
                return Ok(JobCtx::outcome_for(c));
            }
        }
    }
    // ไฟล์ของกิจกรรมที่รอบนี้ไม่พบ: ทำเครื่องหมายว่าไม่พร้อม แต่ไม่ลบข้อมูล (Missing is not deleted)
    conn.execute(
        "UPDATE photo_locations SET available = 0
         WHERE photo_id IN (SELECT photo_id FROM event_photos WHERE event_id = ?1) AND (verified_at IS NULL OR verified_at <> ?2)",
        params![event_id, scan_stamp],
    )?;
    conn.execute(
        "UPDATE sources SET last_reconciled_at = ?2 WHERE id IN (SELECT source_id FROM event_source_bindings WHERE event_id = ?1)",
        params![event_id, scan_stamp],
    )?;

    // ── ขั้นเตรียมข้อมูลรูป (+ วิเคราะห์ใบหน้าเมื่อโมเดลพร้อม): ทำเฉพาะ revision ที่ยังค้าง จึงทำต่อจากเดิมได้โดยไม่ประมวลผลซ้ำ
    // ไม่มีโมเดล = ทำเฉพาะข้อมูลรูป; รูปยังดูและค้นได้ และจะถูกวิเคราะห์เมื่อโมเดลพร้อม
    // วิเคราะห์ใบหน้าเมื่อผู้ใช้ต้องการ (สวิตช์ตอนเพิ่มโฟลเดอร์ / ปุ่มสแกนใบหน้า — `sources.analyze_faces`) · กิจกรรมที่ไม่ได้ผูกกับ
    // โฟลเดอร์ใดวิเคราะห์ตามเดิม · "แต่งก่อน สแกนทีหลัง" = ปิดไว้ แล้วกดสแกนเมื่อแต่งเสร็จ
    let wanted: bool = conn.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM event_source_bindings WHERE event_id = ?1)
             OR EXISTS(SELECT 1 FROM event_source_bindings b JOIN sources s ON s.id = b.source_id
                       WHERE b.event_id = ?1 AND s.analyze_faces = 1)",
        [&event_id],
        |r| r.get(0),
    )?;
    let analyzer = ctx.ai().analyzer().filter(|_| wanted);
    let space = ctx.ai().space().unwrap_or_default();
    ctx.stage("thumbnail")?;
    let items: Vec<ThumbItem> = conn
        .prepare(
            "SELECT r.id, p.id, l.os_path, l.filename, r.size, r.mtime_ns, r.thumbnail_status = 'pending',
                    ?2 AND r.face_status = 'not_analyzed', pe.settings
             FROM event_photos ep
             JOIN photos p ON p.id = ep.photo_id
             JOIN photo_revisions r ON r.id = p.current_revision_id
             JOIN photo_locations l ON l.photo_id = p.id
             LEFT JOIN photo_edits pe ON pe.photo_id = p.id
             WHERE ep.event_id = ?1 AND l.available = 1 AND (r.thumbnail_status = 'pending'
                    OR (?2 AND r.face_status = 'not_analyzed' AND r.thumbnail_status = 'ready'))
             ORDER BY l.filename_key",
        )?
        .query_map(params![event_id, analyzer.is_some()], |r| {
            Ok(ThumbItem {
                need_thumb: r.get(6)?,
                need_faces: r.get(7)?,
                revision_id: r.get(0)?,
                photo_id: r.get(1)?,
                path: PathBuf::from(r.get::<_, String>(2)?),
                filename: r.get(3)?,
                stamp: FileStamp { size: r.get::<_, i64>(4)? as u64, mtime_ns: r.get(5)? },
                edited: edited_for_faces(r.get(8)?),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let (mut done, mut failed) = (ctx.job.done, ctx.job.failed);
    let total = done + failed + items.len() as i64;
    ctx.progress(total, done, failed, skipped, true)?;
    if items.is_empty() {
        set_cover_if_missing(conn, &event_id)?;
        return Ok(Outcome::Done);
    }

    let threads = std::thread::available_parallelism().map(|n| n.get() / 2).unwrap_or(2).clamp(1, 6);
    let queue = Mutex::new(items.into_iter());
    let halt = AtomicBool::new(false);
    let cache_dir = ctx.inner.cache_dir.as_path();
    let mut interrupted: Option<Control> = None;
    let mut first_error: Option<AppError> = None;
    let mut last_flush = Instant::now();

    // งานท้ายของทุกรายการ: ส่งความคืบหน้าเป็นช่วง และตรวจคำขอพัก/ยกเลิก
    let finish = |conn: &Connection,
                  last_flush: &mut Instant,
                  interrupted: &mut Option<Control>,
                  done: i64,
                  failed: i64|
     -> Result<()> {
        if last_flush.elapsed() >= FLUSH_EVERY {
            *last_flush = Instant::now();
            set_cover_if_missing(conn, &event_id)?;
            ctx.progress(total, done, failed, skipped, true)?;
        }
        if interrupted.is_none() {
            let c = ctx.control()?;
            if c != Control::Continue {
                *interrupted = Some(c);
                halt.store(true, Ordering::SeqCst);
            }
        }
        Ok(())
    };

    std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<(ThumbItem, Result<Prepared>)>();
        for _ in 0..threads {
            let tx = tx.clone();
            let (queue, halt, analyzer) = (&queue, &halt, &analyzer);
            s.spawn(move || loop {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Some(item) = queue.lock().unwrap_or_else(|e| e.into_inner()).next() else { break };
                let result = prepare_photo(&item, cache_dir, analyzer.as_deref());
                if tx.send((item, result)).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        // thread นี้เป็นผู้เขียนฐานข้อมูลเพียงผู้เดียวของงาน: transaction สั้น ไม่ถือค้างระหว่าง decode
        for (item, result) in rx {
            let saved = (|| -> Result<()> {
                match &result {
                    Ok(prepared) => {
                        // เขียนเฉพาะเมื่อ revision ยังเป็นรุ่นเดียวกับที่อ่าน: ผลที่มาช้าไม่ทับรุ่นใหม่
                        let current: bool = conn.query_row(
                            "SELECT EXISTS(SELECT 1 FROM photo_revisions WHERE id = ?1 AND size = ?2 AND mtime_ns = ?3)",
                            params![item.revision_id, item.stamp.size as i64, item.stamp.mtime_ns],
                            |r| r.get(0),
                        )?;
                        if current {
                            done += 1;
                        }
                        match &prepared.faces {
                            Some(Ok(found)) if current => crate::ai::save_analysis(
                                conn,
                                cache_dir,
                                &item.revision_id,
                                space,
                                found,
                                &prepared.crops,
                            )?,
                            Some(Err(e)) if current => {
                                // วิเคราะห์ใบหน้าไม่สำเร็จ ≠ ไม่มีใบหน้า: บันทึกเป็นข้อผิดพลาด รูปยังดูได้ตามปกติ
                                conn.execute(
                                    "UPDATE photo_revisions SET face_status = 'failed' WHERE id = ?1",
                                    [&item.revision_id],
                                )?;
                                ctx.error(Some(&item.photo_id), &item.filename, e)?;
                            }
                            _ => {}
                        }
                        let Some(p) = &prepared.processed else {
                            return finish(conn, &mut last_flush, &mut interrupted, done, failed);
                        };
                        conn.execute(
                            "UPDATE photo_revisions SET sha256 = ?2, width = ?3, height = ?4, orientation = ?5,
                               taken_at_local = ?6, tz_offset = ?7, date_source = ?8, thumbnail_status = 'ready',
                               decode_error_code = NULL, decode_error = NULL
                             WHERE id = ?1 AND size = ?9 AND mtime_ns = ?10",
                            params![
                                item.revision_id,
                                p.sha256,
                                p.width,
                                p.height,
                                p.exif.orientation,
                                p.exif.taken_at_local,
                                p.exif.tz_offset,
                                if p.exif.taken_at_local.is_some() { "exif" } else { "none" },
                                item.stamp.size as i64,
                                item.stamp.mtime_ns
                            ],
                        )?;
                    }
                    Err(e) => {
                        failed += 1;
                        ctx.error(Some(&item.photo_id), &item.filename, e)?;
                        if e.code == "SOURCE_UNAVAILABLE" {
                            conn.execute(
                                "UPDATE photo_locations SET available = 0 WHERE photo_id = ?1",
                                [&item.photo_id],
                            )?;
                        } else if e.code != "REVISION_CHANGED" {
                            conn.execute(
                                "UPDATE photo_revisions SET thumbnail_status = 'failed', decode_error_code = ?2, decode_error = ?3 WHERE id = ?1",
                                params![item.revision_id, e.code, e.message],
                            )?;
                        }
                    }
                }
                finish(conn, &mut last_flush, &mut interrupted, done, failed)
            })();
            if let Err(e) = saved {
                halt.store(true, Ordering::SeqCst);
                first_error.get_or_insert(e);
            }
        }
    });

    set_cover_if_missing(conn, &event_id)?;
    ctx.progress(total, done, failed, skipped, true)?;
    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(match interrupted {
        Some(c) => JobCtx::outcome_for(c),
        None => Outcome::Done,
    })
}

/// จับคู่ RAW กับ JPEG ของกิจกรรม (DECISIONS ข้อ 26) แล้วบันทึกคู่ไว้กับรูป JPEG — คืน RAW ที่จะลงทะเบียนเป็นรูป:
/// RAW ที่ไม่มีคู่ของกิจกรรมที่เลือกรวม RAW และ RAW ที่เป็นรูปในคลังอยู่แล้ว (นำเข้าแยกไว้ก่อนมีการจับคู่ — คงเป็นรูปเดิม
/// ไม่ย้ายใบหน้า/ชื่อ/การแต่งไปไหน) · คู่ของรูปในกิจกรรมที่รอบนี้ไม่พบไฟล์ RAW ทำเครื่องหมายว่าไม่พร้อม
fn pair_raws(
    ctx: &JobCtx,
    event_id: &str,
    jpegs: &[PathBuf],
    raws: Vec<(PathBuf, String, bool)>,
    scan_stamp: &str,
) -> Result<Vec<(PathBuf, String)>> {
    let conn = ctx.conn;
    let is_photo = |path: &Path| -> Result<bool> {
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM photo_locations WHERE path_key = ?1)",
            [util::path_key(path)],
            |r| r.get(0),
        )?)
    };
    let mut keep = vec![];
    let mut candidates: Vec<(PathBuf, String, bool)> = vec![];
    for raw in raws {
        if is_photo(&raw.0)? {
            keep.push((raw.0, raw.1));
        } else {
            candidates.push(raw);
        }
    }
    let paths: Vec<PathBuf> = candidates.iter().map(|c| c.0.clone()).collect();
    let matches = if paths.is_empty() { vec![] } else { crate::pair::match_files(jpegs, &paths) };
    let mut paired = vec![false; paths.len()];
    let tx = conn.unchecked_transaction()?;
    for (jpeg, found) in jpegs.iter().zip(matches) {
        let Some(found) = found else { continue };
        let photo_id: Option<String> = tx
            .query_row("SELECT photo_id FROM photo_locations WHERE path_key = ?1", [util::path_key(jpeg)], |r| r.get(0))
            .optional()?;
        if let Some(photo_id) = photo_id {
            crate::pair::upsert(&tx, &photo_id, &paths[found.raw], found.lightroom.as_ref(), scan_stamp)?;
            paired[found.raw] = true;
        }
    }
    tx.execute(
        "UPDATE photo_raw_pairs SET available = 0
         WHERE photo_id IN (SELECT photo_id FROM event_photos WHERE event_id = ?1) AND (verified_at IS NULL OR verified_at <> ?2)",
        params![event_id, scan_stamp],
    )?;
    tx.commit()?;
    keep.extend(
        candidates
            .into_iter()
            .zip(paired)
            .filter(|((_, _, include), paired)| *include && !paired)
            .map(|((p, s, _), _)| (p, s)),
    );
    Ok(keep)
}

/// ลงทะเบียนไฟล์หนึ่งชุดใน transaction เดียว; คืน (จำนวนที่พบ, จำนวนที่มีอยู่แล้วและไม่เปลี่ยน)
fn register_batch(
    ctx: &JobCtx,
    event_id: &str,
    source_id: &str,
    paths: &[PathBuf],
    scan_stamp: &str,
) -> Result<(i64, i64)> {
    if paths.is_empty() {
        return Ok((0, 0));
    }
    let tx = ctx.conn.unchecked_transaction()?;
    let (mut found, mut unchanged) = (0i64, 0i64);
    for path in paths {
        let stamp = match media::stamp(path) {
            Ok(s) => s,
            Err(e) => {
                ctx.error(None, &util::file_name(path), &AppError::from(e))?;
                continue;
            }
        };
        found += 1;
        if register_file(&tx, event_id, source_id, path, stamp, scan_stamp)? {
            unchanged += 1;
        }
    }
    tx.commit()?;
    Ok((found, unchanged))
}

/// คืน true เมื่อไฟล์นี้อยู่ในคลังแล้วและเนื้อหาไม่เปลี่ยน
fn register_file(
    conn: &Connection,
    event_id: &str,
    source_id: &str,
    path: &Path,
    stamp: FileStamp,
    scan_stamp: &str,
) -> Result<bool> {
    let key = util::path_key(path);
    let existing: Option<(String, String, i64, i64)> = conn
        .query_row(
            "SELECT p.id, r.id, r.size, r.mtime_ns FROM photo_locations l
             JOIN photos p ON p.id = l.photo_id JOIN photo_revisions r ON r.id = p.current_revision_id
             WHERE l.path_key = ?1",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let now = util::now();
    match existing {
        Some((photo_id, revision_id, size, mtime_ns)) => {
            // ตำแหน่งเดิม → ใช้ photo ID เดิม ไม่เพิ่มแถวซ้ำ และไม่ย้ายกิจกรรมเงียบ ๆ
            conn.execute(
                "INSERT OR IGNORE INTO source_photos(source_id, photo_id) VALUES (?1, ?2)",
                params![source_id, photo_id],
            )?;
            conn.execute(
                "UPDATE photo_locations SET available = 1, verified_at = ?2 WHERE photo_id = ?1",
                params![photo_id, scan_stamp],
            )?;
            if size as u64 == stamp.size && mtime_ns == stamp.mtime_ns {
                return Ok(true);
            }
            // เนื้อหาเปลี่ยนที่ path เดิม: ถ้ามีใบหน้า/คำยืนยันผูกอยู่ ให้สร้าง revision ใหม่และเก็บของเดิมเป็นประวัติ
            let has_faces: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM faces WHERE photo_revision_id = ?1)",
                [&revision_id],
                |r| r.get(0),
            )?;
            if has_faces {
                let new_rev = util::new_id();
                conn.execute(
                    "INSERT INTO photo_revisions(id, photo_id, size, mtime_ns, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![new_rev, photo_id, stamp.size as i64, stamp.mtime_ns, now],
                )?;
                conn.execute(
                    "UPDATE photos SET current_revision_id = ?2, record_version = record_version + 1 WHERE id = ?1",
                    params![photo_id, new_rev],
                )?;
            } else {
                conn.execute(
                    "UPDATE photo_revisions SET size = ?2, mtime_ns = ?3, sha256 = NULL, thumbnail_status = 'pending',
                       decode_error_code = NULL, decode_error = NULL WHERE id = ?1",
                    params![revision_id, stamp.size as i64, stamp.mtime_ns],
                )?;
                // ผลวัดสำหรับการคัดเป็นของเนื้อหาเดิม (DECISIONS ข้อ 37)
                conn.execute("DELETE FROM photo_quality WHERE revision_id = ?1", [&revision_id])?;
            }
            Ok(false)
        }
        None => {
            let (photo_id, revision_id) = (util::new_id(), util::new_id());
            let filename = util::file_name(path);
            conn.execute(
                "INSERT INTO photos(id, current_revision_id, created_at) VALUES (?1, ?2, ?3)",
                params![photo_id, revision_id, now],
            )?;
            conn.execute(
                "INSERT INTO photo_locations(photo_id, os_path, path_key, filename, filename_key, available, verified_at)
                 VALUES (?1, ?2, ?2, ?3, ?4, 1, ?5)",
                params![photo_id, key, filename, util::text_key(&filename), scan_stamp],
            )?;
            conn.execute(
                "INSERT INTO photo_revisions(id, photo_id, size, mtime_ns, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![revision_id, photo_id, stamp.size as i64, stamp.mtime_ns, now],
            )?;
            conn.execute("INSERT INTO event_photos(photo_id, event_id) VALUES (?1, ?2)", params![photo_id, event_id])?;
            conn.execute(
                "INSERT INTO source_photos(source_id, photo_id) VALUES (?1, ?2)",
                params![source_id, photo_id],
            )?;
            Ok(false)
        }
    }
}

/// อ่านไฟล์หนึ่งรูปและสร้างภาพย่อ; ตรวจ size/mtime ก่อนและหลังอ่าน เพื่อไม่บันทึกผลของไฟล์ที่กำลังถูกเขียน
/// ผลของการเตรียมหนึ่งรูป: ข้อมูลรูป (เมื่อยังไม่มีภาพย่อ) และผลวิเคราะห์ใบหน้า (เมื่อโมเดลพร้อมและยังไม่เคยวิเคราะห์)
struct Prepared {
    processed: Option<media::Processed>,
    faces: Option<Result<Vec<crate::ai::DetectedFace>>>,
    /// ภาพครอปของ `faces` ตามลำดับ; ว่างเมื่อไม่พบใบหน้าหรือทำครอปไม่ได้
    crops: Vec<Vec<u8>>,
}

/// อ่านไฟล์หนึ่งรูปครั้งเดียว แล้วสร้างภาพย่อและ/หรือวิเคราะห์ใบหน้า;
/// ตรวจ size/mtime ก่อนและหลังอ่าน เพื่อไม่บันทึกผลของไฟล์ที่กำลังถูกเขียน
fn prepare_photo(
    item: &ThumbItem,
    cache_dir: &Path,
    analyzer: Option<&dyn crate::ai::FaceAnalyzer>,
) -> Result<Prepared> {
    let changed = || AppError::new("REVISION_CHANGED", "ไฟล์นี้เปลี่ยนระหว่างเตรียมข้อมูล สั่ง “ตรวจรูปใหม่” อีกครั้ง").retryable();
    if media::stamp(&item.path)? != item.stamp {
        return Err(changed());
    }
    let bytes = media::read_file(&item.path)?;
    let processed = if item.need_thumb { Some(media::process(&bytes)?) } else { None };
    if media::stamp(&item.path)? != item.stamp {
        return Err(changed());
    }
    if let Some(p) = &processed {
        media::write_cache_file(&crate::thumb_path(cache_dir, &item.revision_id), &p.thumbnail_jpeg)?;
    }
    let faces = match analyzer {
        Some(a) if item.need_faces => Some(match &item.edited {
            Some(settings) => edited_jpeg(&bytes, &item.path, settings).and_then(|jpeg| a.analyze(&jpeg)),
            None => a.analyze(&bytes),
        }),
        _ => None,
    };
    // ทำครอปตอนที่ไฟล์ยังอยู่ในมือ (ถอดรหัสครั้งเดียวต่อรูป): คิวตรวจสอบใบหน้าไม่ต้องเปิดต้นฉบับทีละใบอีก
    let crops = match &faces {
        Some(Ok(found)) => {
            let boxes: Vec<_> = found.iter().map(|f| (f.bbox.x, f.bbox.y, f.bbox.w, f.bbox.h)).collect();
            media::face_crops(&bytes, &boxes, false).unwrap_or_default()
        }
        _ => vec![],
    };
    Ok(Prepared { processed, faces, crops })
}

/// ภาพที่แต่งแล้ว (แสงสี เฉพาะส่วน รีทัช — ไม่ตัดไม่หมุน) เป็น JPEG ด้านยาว 1600 สำหรับหาใบหน้า
fn edited_jpeg(bytes: &[u8], path: &Path, settings: &crate::edit::EditSettings) -> Result<Vec<u8>> {
    let img = media::decode_upright(bytes, 1600)?;
    let profile =
        if crate::raw::is_raw_path(path) { crate::adjust::Profile::Raw } else { crate::adjust::Profile::Display };
    let out = crate::edit::render_with(img, settings, profile).into_rgb8();
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 92).encode_image(&out).map_err(AppError::internal)?;
    Ok(jpeg.into_inner())
}

/// งานเบา ๆ ตอน worker ว่าง: สร้างเวกเตอร์ให้ใบหน้าตัวอย่างที่นำเข้าไว้ เมื่อโมเดลพร้อม; คืน true ถ้ายังมีงานเหลือ
fn maintenance(inner: &Inner, conn: &Connection) -> Result<bool> {
    let Some(space) = inner.ai.space() else { return Ok(false) };
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM person_reference_samples WHERE (vector IS NULL OR space <> ?1) AND attempts < 2",
        [space],
        |r| r.get(0),
    )?;
    if pending == 0 || inner.stop.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let Some(analyzer) = inner.ai.analyzer() else { return Ok(false) };
    crate::review::embed_pending_samples(conn, analyzer.as_ref(), space, 16)?;
    inner.sink.data_changed(db::data_revision(conn)?, &["people"]);
    Ok(true)
}

fn set_cover_if_missing(conn: &Connection, event_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE events SET cover_photo_id = (
           SELECT p.id FROM event_photos ep
           JOIN photos p ON p.id = ep.photo_id
           JOIN photo_revisions r ON r.id = p.current_revision_id
           JOIN photo_locations l ON l.photo_id = p.id
           WHERE ep.event_id = ?1 AND r.thumbnail_status = 'ready' ORDER BY l.filename_key LIMIT 1)
         WHERE id = ?1 AND cover_photo_id IS NULL",
        [event_id],
    )?;
    Ok(())
}

// ───────────────────────── คำสั่งจาก UI ─────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub skipped: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorCount {
    pub code: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub stage: String,
    pub title: String,
    pub event_id: Option<String>,
    pub counts: Counts,
    pub total_known: bool,
    pub recovered: bool,
    pub blocked_reason: Option<String>,
    pub error_summary: Vec<ErrorCount>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobErrorItem {
    pub photo_id: Option<String>,
    pub filename: String,
    pub code: String,
    pub message: String,
}

const JOB_COLS: &str =
    "id, kind, state, stage, title, event_id, total, done, failed, skipped, total_known, recovered, \
                        blocked_reason, created_at, started_at, finished_at, updated_at";

fn job_from_row(r: &rusqlite::Row) -> rusqlite::Result<JobSnapshot> {
    Ok(JobSnapshot {
        id: r.get(0)?,
        kind: r.get(1)?,
        state: r.get(2)?,
        stage: r.get(3)?,
        title: r.get(4)?,
        event_id: r.get(5)?,
        counts: Counts { total: r.get(6)?, done: r.get(7)?, failed: r.get(8)?, skipped: r.get(9)? },
        total_known: r.get(10)?,
        recovered: r.get(11)?,
        blocked_reason: r.get(12)?,
        error_summary: vec![],
        created_at: r.get(13)?,
        started_at: r.get(14)?,
        finished_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

fn with_errors(conn: &Connection, mut job: JobSnapshot) -> Result<JobSnapshot> {
    job.error_summary = conn
        .prepare("SELECT code, COUNT(*) FROM job_errors WHERE job_id = ?1 GROUP BY code ORDER BY 2 DESC")?
        .query_map([&job.id], |r| Ok(ErrorCount { code: r.get(0)?, count: r.get(1)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(job)
}

pub fn get(conn: &Connection, job_id: &str) -> Result<JobSnapshot> {
    let job = conn
        .query_row(&format!("SELECT {JOB_COLS} FROM jobs WHERE id = ?1"), [job_id], job_from_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("งาน"))?;
    with_errors(conn, job)
}

pub fn list(conn: &Connection, active_only: bool, limit: i64) -> Result<Vec<JobSnapshot>> {
    let filter = if active_only { format!("WHERE state IN {ACTIVE_STATES}") } else { String::new() };
    let jobs: Vec<JobSnapshot> = conn
        .prepare(&format!("SELECT {JOB_COLS} FROM jobs {filter} ORDER BY created_at DESC, id DESC LIMIT ?1"))?
        .query_map([limit.clamp(1, 500)], job_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    jobs.into_iter().map(|j| with_errors(conn, j)).collect()
}

pub fn list_for_event(conn: &Connection, event_id: &str) -> Result<Vec<JobSnapshot>> {
    let jobs: Vec<JobSnapshot> = conn
        .prepare(&format!(
            "SELECT {JOB_COLS} FROM jobs WHERE event_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 20"
        ))?
        .query_map([event_id], job_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    jobs.into_iter().map(|j| with_errors(conn, j)).collect()
}

pub fn list_errors(conn: &Connection, job_id: &str) -> Result<Vec<JobErrorItem>> {
    Ok(conn
        .prepare("SELECT photo_id, filename, code, message FROM job_errors WHERE job_id = ?1 ORDER BY id LIMIT 2000")?
        .query_map([job_id], |r| {
            Ok(JobErrorItem { photo_id: r.get(0)?, filename: r.get(1)?, code: r.get(2)?, message: r.get(3)? })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

fn transition(app: &App, job_id: &str, sql_case: &str) -> Result<JobSnapshot> {
    let snapshot = {
        let conn = app.db();
        get(&conn, job_id)?;
        conn.execute(
            &format!("UPDATE jobs SET state = {sql_case}, updated_at = ?2 WHERE id = ?1"),
            params![job_id, util::now()],
        )?;
        if conn.query_row("SELECT state FROM jobs WHERE id = ?1", [job_id], |r| r.get::<_, String>(0))? == "cancelled" {
            finish_cancelled(&conn, job_id)?;
        }
        db::bump_revision(&conn)?;
        get(&conn, job_id)?
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    Ok(snapshot)
}

/// พัก: งานที่กำลังทำจะเป็น "กำลังหยุดหลังรายการปัจจุบัน" จนกว่า worker ยืนยัน
pub fn pause(app: &App, job_id: &str) -> Result<JobSnapshot> {
    transition(app, job_id, "CASE state WHEN 'queued' THEN 'paused' WHEN 'blocked' THEN 'paused' WHEN 'running' THEN 'pausing' ELSE state END")
}

pub fn resume(app: &App, job_id: &str) -> Result<JobSnapshot> {
    transition(app, job_id, "CASE state WHEN 'paused' THEN 'queued' WHEN 'blocked' THEN 'queued' WHEN 'pausing' THEN 'running' ELSE state END")
}

/// ยกเลิกส่วนที่เหลือ; ส่วนที่ทำสำเร็จแล้วยังอยู่ในคลัง
pub fn cancel(app: &App, job_id: &str) -> Result<JobSnapshot> {
    transition(
        app,
        job_id,
        "CASE WHEN state IN ('queued','paused','blocked') THEN 'cancelled' WHEN state IN ('running','pausing') THEN 'cancel_requested' ELSE state END",
    )
}

/// ลองใหม่เฉพาะรูปที่เตรียมข้อมูลไม่สำเร็จของกิจกรรมนั้น (ไม่ประมวลผลรูปที่สำเร็จแล้วซ้ำ)
pub fn retry_failed(app: &App, job_id: &str) -> Result<JobSnapshot> {
    let new_id = {
        let conn = app.db();
        let job = get(&conn, job_id)?;
        let event_id = match (job.kind.as_str(), job.event_id) {
            ("catalog", Some(e)) => e,
            _ => return Err(AppError::invalid("งานนี้ลองใหม่เฉพาะรายการไม่ได้ กรุณาสั่งงานใหม่จากรายการที่ต้องการ")),
        };
        conn.execute(
            "UPDATE photo_revisions SET thumbnail_status = 'pending', decode_error_code = NULL, decode_error = NULL
             WHERE thumbnail_status = 'failed' AND id IN (
               SELECT p.current_revision_id FROM event_photos ep JOIN photos p ON p.id = ep.photo_id WHERE ep.event_id = ?1)",
            [&event_id],
        )?;
        let id = enqueue_catalog(&conn, &event_id, &job.title)?;
        db::bump_revision(&conn)?;
        id
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    get(&app.db(), &new_id)
}

/// "ตรวจรูปใหม่" ของกิจกรรม
pub fn rescan_event(app: &App, event_id: &str) -> Result<JobSnapshot> {
    let id = {
        let conn = app.db();
        let title: String = conn
            .query_row("SELECT title FROM events WHERE id = ?1 AND catalog_state = 'active'", [event_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::not_found("กิจกรรม"))?;
        let id = enqueue_catalog(&conn, event_id, &title)?;
        db::bump_revision(&conn)?;
        id
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    get(&app.db(), &id)
}
