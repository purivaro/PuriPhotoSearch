//! ย่อไฟล์ต้นฉบับในที่เดิม (DECISIONS ข้อ 15) — ข้อยกเว้นเดียวของกฎ "ไม่แก้ไฟล์ต้นฉบับ" และทำเมื่อผู้ใช้สั่งเท่านั้น
//!
//! แทนไฟล์ JPEG ด้วยไฟล์ที่ย่อแล้ว (เก็บ EXIF โปรไฟล์สี เวลาแก้ไขไฟล์ และสิทธิ์ไฟล์) ต้นฉบับขนาดเต็มไปถังขยะ
//! แล้วปรับขนาด/hash ของ revision เดิมในคลัง — ไม่สร้าง revision ใหม่ ใบหน้า เวกเตอร์ คำยืนยัน
//! และคำปฏิเสธจึงอยู่ครบโดยไม่ต้องวิเคราะห์ใหม่ (กรอบใบหน้าเก็บเป็นสัดส่วนบนภาพที่หมุนตั้งตรงแล้ว)
//!
//! ด้านยาวไม่ต่ำกว่า [`MIN_EDGE`]: ตัววิเคราะห์ใบหน้าย่อรูปเหลือเท่านี้ก่อนวิเคราะห์อยู่แล้ว
//! วิเคราะห์ใหม่ภายหลังจึงได้ผลเท่ากับต้นฉบับ
//!
//! ลำดับต่อรูป — แอปปิดหรือไฟดับตรงไหนก็ไม่มีรูปหาย ([`recover`] ทำต่อให้ตอนเปิดแอป):
//! 1. บันทึกตำแหน่งไฟล์ชั่วคราว แล้วเขียนไฟล์ที่ย่อไว้ข้างต้นฉบับ อ่านกลับมาตรวจ
//! 2. บันทึก `replacing` พร้อม hash ของไฟล์ที่ย่อ
//! 3. ย้ายต้นฉบับไปถังขยะ (ทำไม่ได้ = ลบไฟล์ชั่วคราว ต้นฉบับอยู่ที่เดิม)
//! 4. เปลี่ยนชื่อไฟล์ชั่วคราวเป็นชื่อต้นฉบับ
//! 5. ปรับ revision และบันทึก `shrunk` ใน transaction เดียว

use crate::error::{AppError, Result};
use crate::export::{load_items, load_revision, rename_noreplace, Resize, Revision, SelectionSpec, SourceRow};
use crate::import::{remember, replay};
use crate::jobs::{self, Control, JobCtx, JobSnapshot, Outcome};
use crate::{db, media, platform, util, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// ด้านยาวต่ำสุดของไฟล์ที่ย่อ = ด้านยาวที่ตัววิเคราะห์ใบหน้าใช้ (`puri_faces::MAX_IMAGE_DIM`, มีเทสต์ตรวจว่าตรงกัน)
pub const MIN_EDGE: u32 = 1280;
/// เพดานขนาดไฟล์ต่ำสุด: ต่ำกว่านี้รูปถ่ายทั่วไปต้องย่อจนด้านยาวต่ำกว่า MIN_EDGE
const MIN_BYTES: u64 = 300 * 1024;
const PLAN_TTL: Duration = Duration::from_secs(30 * 60);

pub(crate) struct PlanItem {
    row: SourceRow,
    status: &'static str,
    note: Option<&'static str>,
}

pub struct PlanData {
    resize: Resize,
    items: Vec<PlanItem>,
    created: Instant,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShrinkPlan {
    pub plan_id: String,
    pub resize: Resize,
    pub input_count: usize,
    /// รูปที่จะย่อ (ต้นฉบับของรูปเหล่านี้ไปถังขยะ)
    pub planned_count: usize,
    pub already_small: usize,
    pub not_jpeg: usize,
    pub unavailable: usize,
    /// ขนาดรวมของต้นฉบับที่จะย่อ และขนาดโดยประมาณหลังย่อ
    pub source_bytes: u64,
    pub estimated_bytes: u64,
}

fn check(resize: Resize) -> Result<()> {
    resize.validate()?;
    match resize {
        Resize::Original => Err(AppError::invalid("เลือกขนาดที่จะย่อก่อน")),
        Resize::LongEdge { px } if px < MIN_EDGE => {
            Err(AppError::invalid("ย่อไฟล์ต้นฉบับได้ไม่ต่ำกว่าด้านยาว 1,280 พิกเซล เพื่อให้การจดจำใบหน้าแม่นเท่าเดิม"))
        }
        Resize::MaxBytes { bytes } if bytes < MIN_BYTES => {
            Err(AppError::invalid("ย่อไฟล์ต้นฉบับได้ไม่ต่ำกว่า 300 KB ต่อรูป เพื่อให้ภาพยังใหญ่พอสำหรับการจดจำใบหน้า"))
        }
        _ => Ok(()),
    }
}

fn is_jpeg_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

const NOTE_SMALL: &str = "เล็กพออยู่แล้ว";
const NOTE_NOT_JPEG: &str = "ไม่ใช่ไฟล์ JPEG (ย่อในที่เดิมเฉพาะ JPEG เพื่อไม่ให้นามสกุลไฟล์เปลี่ยน)";

pub fn preview(app: &App, selection: &SelectionSpec, resize: Resize) -> Result<ShrinkPlan> {
    check(resize)?;
    let rows = load_items(&app.db(), selection)?;
    if rows.is_empty() {
        return Err(AppError::invalid("ยังไม่ได้เลือกรูปที่จะย่อ"));
    }
    let (mut planned, mut small, mut not_jpeg, mut unavailable) = (0, 0, 0, 0);
    let (mut source_bytes, mut estimated_bytes) = (0u64, 0u64);
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let fit = resize.fit_for(row.size, row.width, row.height);
        let (status, note) = if !Path::new(&row.source_path).is_file() {
            unavailable += 1;
            ("unavailable", None)
        } else if !is_jpeg_name(&row.filename) {
            not_jpeg += 1;
            ("skipped", Some(NOTE_NOT_JPEG))
        } else if fit.is_none() {
            small += 1;
            ("skipped", Some(NOTE_SMALL))
        } else {
            planned += 1;
            source_bytes += row.size;
            estimated_bytes += crate::export::estimate_bytes(fit, row.size, row.width, row.height);
            ("planned", None)
        };
        items.push(PlanItem { row, status, note });
    }
    let plan = ShrinkPlan {
        plan_id: util::new_id(),
        resize,
        input_count: items.len(),
        planned_count: planned,
        already_small: small,
        not_jpeg,
        unavailable,
        source_bytes,
        estimated_bytes,
    };
    let mut plans = app.shrink_plans.lock().unwrap_or_else(|e| e.into_inner());
    plans.retain(|_, p| p.created.elapsed() < PLAN_TTL);
    plans.insert(plan.plan_id.clone(), PlanData { resize, items, created: Instant::now() });
    Ok(plan)
}

pub fn start(app: &App, plan_id: &str, request_id: &str) -> Result<JobSnapshot> {
    let job_id = {
        let mut conn = app.db();
        if let Some(id) = replay::<String>(&conn, request_id)? {
            return jobs::get(&conn, &id);
        }
        let plan = app
            .shrink_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plan_id)
            .filter(|p| p.created.elapsed() < PLAN_TTL)
            .ok_or_else(|| AppError::new("PREVIEW_STALE", "แผนหมดอายุแล้ว กรุณาตรวจแผนอีกครั้ง").next("refresh_preview"))?;
        let planned = plan.items.iter().filter(|i| i.status == "planned").count();
        if planned == 0 {
            return Err(AppError::invalid("ไม่มีรูปที่ต้องย่อในชุดที่เลือก"));
        }
        let tx = conn.transaction()?;
        let (job_id, run_id, now) = (util::new_id(), util::new_id(), util::now());
        tx.execute(
            // ตัวนับของงานครอบเฉพาะรูปที่จะย่อ (รูปที่ข้ามตั้งแต่วางแผนอยู่ในรายงาน)
            "INSERT INTO jobs(id, kind, state, stage, title, dedup_key, total, total_known, created_at, updated_at)
             VALUES (?1, 'shrink', 'queued', 'shrink', ?2, ?3, ?4, 1, ?5, ?5)",
            params![job_id, format!("ย่อไฟล์ต้นฉบับ {planned} รูป"), format!("shrink:{run_id}"), planned as i64, now],
        )?;
        tx.execute(
            "INSERT INTO shrink_runs(id, job_id, resize, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![run_id, job_id, serde_json::to_string(&plan.resize)?, now],
        )?;
        for (ord, item) in plan.items.iter().enumerate() {
            let r = &item.row;
            tx.execute(
                "INSERT INTO shrink_items(run_id, ord, photo_id, revision_id, filename, path, status, note, old_size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    run_id,
                    ord as i64,
                    r.photo_id,
                    r.revision_id,
                    r.filename,
                    r.source_path,
                    item.status,
                    item.note,
                    r.size as i64
                ],
            )?;
        }
        remember(&tx, request_id, "start_shrink", &job_id)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        job_id
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    jobs::get(&app.db(), &job_id)
}

struct Item {
    ord: i64,
    photo_id: String,
    revision_id: String,
    filename: String,
    path: PathBuf,
    temp: PathBuf,
}

/// ไฟล์ที่ย่อแล้วรอแทนที่ต้นฉบับ (อยู่ที่ `Item::temp`)
struct Prepared {
    size: u64,
    sha256: String,
    width: u32,
    height: u32,
}

enum Step {
    Ready(Prepared),
    /// ไม่ย่อรูปนี้ พร้อมเหตุผล — ต้นฉบับไม่ถูกแตะ
    Skip(String),
}

fn source_changed() -> AppError {
    AppError::new("REVISION_CHANGED", "ไฟล์ต้นฉบับเปลี่ยนหลังจากเพิ่มเข้าคลัง สั่ง “ตรวจรูปใหม่” ก่อนย่อ").retryable()
}

/// เวลาแก้ไขไฟล์และสิทธิ์ไฟล์เท่าต้นฉบับ (macOS: รวมป้ายสีและข้อมูลเสริมของ Finder)
fn copy_file_info(src: &Path, dst: &Path, file: &File) {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        if let (Ok(s), Ok(d)) =
            (std::ffi::CString::new(src.as_os_str().as_bytes()), std::ffi::CString::new(dst.as_os_str().as_bytes()))
        {
            // SAFETY: ทั้งสองเป็น C string ที่ปิดท้ายด้วย NUL มีชีวิตตลอดการเรียก; state เป็น NULL ได้ตามเอกสาร
            unsafe {
                libc::copyfile(s.as_ptr(), d.as_ptr(), std::ptr::null_mut(), libc::COPYFILE_ACL | libc::COPYFILE_XATTR)
            };
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = dst;
    if let Ok(meta) = std::fs::metadata(src) {
        #[cfg(unix)]
        let _ = file.set_permissions(meta.permissions());
        if let Ok(modified) = meta.modified() {
            let _ = file.set_modified(modified);
        }
    }
}

/// อ่านต้นฉบับ ตรวจว่าตรงกับที่คลังรู้จัก แล้วเขียนไฟล์ที่ย่อลง `item.temp` — ไม่แตะฐานข้อมูล จึงทำหลายรูปพร้อมกันได้
fn prepare(item: &Item, rev: &Revision, resize: Resize) -> Result<Step> {
    let path = &item.path;
    if std::fs::symlink_metadata(path).map_err(|_| AppError::source_unavailable())?.file_type().is_symlink() {
        return Ok(Step::Skip("เป็นทางลัด (symlink) ไปยังไฟล์อื่น".into()));
    }
    let stamp = media::stamp(path)?;
    if stamp.size != rev.size as u64 || stamp.mtime_ns != rev.mtime_ns {
        return Err(source_changed());
    }
    let bytes = media::read_file(path)?;
    if bytes.len() as u64 != stamp.size
        || media::stamp(path).ok() != Some(stamp)
        || rev.sha.as_deref().is_some_and(|s| s != util::hex(&Sha256::digest(&bytes)))
    {
        return Err(source_changed());
    }
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return Ok(Step::Skip(NOTE_NOT_JPEG.into()));
    }
    let Some(fit) = resize.fit_for(stamp.size, rev.width, rev.height) else {
        return Ok(Step::Skip(NOTE_SMALL.into()));
    };
    let out = match media::export_jpeg(&bytes, fit) {
        Ok(out) => out,
        // ตั้งขนาดไฟล์เล็กเกินกว่ารูปนี้จะทำได้: ไม่ใช่ความผิดพลาดของไฟล์
        Err(e) if e.code == "INVALID_INPUT" => return Ok(Step::Skip(e.message)),
        Err(e) => return Err(e),
    };
    let (width, height) = media::jpeg_dimensions(&out).ok_or_else(|| AppError::internal("ไฟล์ที่ย่ออ่านขนาดไม่ได้"))?;
    if width.max(height) < MIN_EDGE {
        return Ok(Step::Skip("ต้องย่อจนด้านยาวต่ำกว่า 1,280 พิกเซล การจดจำใบหน้าจะแม่นน้อยลง จึงไม่ย่อ".into()));
    }
    if media::metadata_count(&out) < media::metadata_count(&bytes) {
        return Ok(Step::Skip("ย่อแล้วจะเสียข้อมูลกำกับภาพ (วันที่ถ่าย ข้อมูลกล้อง) จึงไม่ย่อ".into()));
    }
    if out.len() as u64 * 10 > stamp.size * 9 {
        return Ok(Step::Skip("ย่อแล้วเล็กลงไม่ถึง 10% จึงคงไฟล์เดิม".into()));
    }
    let sha256 = util::hex(&Sha256::digest(&out));
    drop(bytes);

    let written = (|| -> Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&item.temp)?;
        file.write_all(&out)?;
        copy_file_info(path, &item.temp, &file);
        file.sync_all()?;
        Ok(())
    })();
    // อ่านกลับมาตรวจว่าลงดิสก์ครบ ก่อนจะยอมให้ต้นฉบับไปถังขยะ
    let verified = written.and_then(|_| {
        if util::hex(&Sha256::digest(media::read_file(&item.temp)?)) == sha256 {
            Ok(())
        } else {
            Err(AppError::internal("ไฟล์ที่ย่อเขียนลงดิสก์ไม่ครบ"))
        }
    });
    if let Err(e) = verified {
        let _ = std::fs::remove_file(&item.temp);
        return Err(e);
    }
    Ok(Step::Ready(Prepared { size: out.len() as u64, sha256, width, height }))
}

fn set_status(conn: &Connection, run_id: &str, ord: i64, status: &str, note: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE shrink_items SET status = ?3, note = ?4, temp_path = NULL WHERE run_id = ?1 AND ord = ?2",
        params![run_id, ord, status, note],
    )?;
    Ok(())
}

/// ผลของการดูสภาพไฟล์ของรายการที่ค้างอยู่ระหว่างแทนที่
enum Settled {
    Shrunk,
    /// ต้นฉบับยังอยู่ที่เดิม: กลับเข้าคิว
    Reverted,
    Failed(String),
}

struct Replacing {
    run_id: String,
    ord: i64,
    revision_id: String,
    path: PathBuf,
    temp: Option<PathBuf>,
    sha256: String,
    width: u32,
    height: u32,
}

/// ทำรายการที่ผ่านขั้น `replacing` แล้วให้จบตามสภาพไฟล์จริง — ใช้ทั้งระหว่างทำงานและตอนเปิดแอปหลังปิดกลางคัน
fn settle(conn: &Connection, r: &Replacing) -> Result<Settled> {
    let at_path = r.path.exists();
    let temp = r.temp.as_deref().filter(|t| t.exists());
    let finished = match (temp, at_path) {
        // ต้นฉบับยังไม่ถูกย้าย: ทิ้งไฟล์ชั่วคราว
        (Some(t), true) => {
            std::fs::remove_file(t)?;
            set_status(conn, &r.run_id, r.ord, "planned", None)?;
            return Ok(Settled::Reverted);
        }
        // ต้นฉบับอยู่ในถังขยะแล้ว: วางไฟล์ที่ย่อแทน
        (Some(t), false) => {
            rename_noreplace(t, &r.path)?;
            true
        }
        // แทนที่แล้วแต่ยังไม่ได้บันทึก — ตรวจว่าไฟล์ที่อยู่เป็นไฟล์ที่ย่อจริง
        (None, true) => util::hex(&Sha256::digest(media::read_file(&r.path)?)) == r.sha256,
        (None, false) => false,
    };
    if !finished {
        if at_path {
            set_status(conn, &r.run_id, r.ord, "planned", None)?;
            return Ok(Settled::Reverted);
        }
        let note = "ไม่พบไฟล์ที่ย่อ ต้นฉบับอยู่ในถังขยะ — ย้ายกลับจากถังขยะได้";
        set_status(conn, &r.run_id, r.ord, "failed", Some(note))?;
        return Ok(Settled::Failed(note.into()));
    }
    let stamp = media::stamp(&r.path)?;
    let tx = conn.unchecked_transaction()?;
    // revision เดิม: ใบหน้า คำยืนยัน และภาพย่อที่ผูกกับ revision นี้ใช้ต่อได้ (ภาพเดิมที่ความละเอียดต่ำลง หมุนตั้งตรงแล้ว)
    tx.execute(
        "UPDATE photo_revisions SET size = ?2, mtime_ns = ?3, sha256 = ?4, width = ?5, height = ?6, orientation = 1
         WHERE id = ?1",
        params![r.revision_id, stamp.size as i64, stamp.mtime_ns, r.sha256, r.width, r.height],
    )?;
    tx.execute(
        "UPDATE shrink_items SET status = 'shrunk', new_size = ?3, temp_path = NULL WHERE run_id = ?1 AND ord = ?2",
        params![r.run_id, r.ord, stamp.size as i64],
    )?;
    db::bump_revision(&tx)?;
    tx.commit()?;
    Ok(Settled::Shrunk)
}

/// ขั้น 2–5 ของรูปที่เตรียมไฟล์ที่ย่อไว้แล้ว
fn replace(conn: &Connection, run_id: &str, item: &Item, p: &Prepared) -> Result<Settled> {
    conn.execute(
        "UPDATE shrink_items SET status = 'replacing', new_size = ?3, new_sha256 = ?4, new_width = ?5, new_height = ?6
         WHERE run_id = ?1 AND ord = ?2",
        params![run_id, item.ord, p.size as i64, p.sha256, p.width, p.height],
    )?;
    match platform::move_to_trash(&item.path) {
        Ok(trashed) => {
            conn.execute(
                "UPDATE shrink_items SET trashed_path = ?3 WHERE run_id = ?1 AND ord = ?2",
                params![run_id, item.ord, trashed.map(|t| t.to_string_lossy().into_owned())],
            )?;
        }
        // ระบบแจ้งว่าย้ายไม่ได้และไฟล์ยังอยู่: ไม่ย่อรูปนี้
        Err(e) if item.path.exists() => {
            let _ = std::fs::remove_file(&item.temp);
            set_status(conn, run_id, item.ord, "failed", Some(&e.message))?;
            return Ok(Settled::Failed(e.message));
        }
        // แจ้งว่าผิดพลาดแต่ไฟล์ไม่อยู่ที่เดิมแล้ว: วางไฟล์ที่ย่อแทนให้รูปยังอยู่ในคลัง
        Err(_) => {}
    }
    let r = Replacing {
        run_id: run_id.to_string(),
        ord: item.ord,
        revision_id: item.revision_id.clone(),
        path: item.path.clone(),
        temp: Some(item.temp.clone()),
        sha256: p.sha256.clone(),
        width: p.width,
        height: p.height,
    };
    settle(conn, &r)
}

pub(crate) fn run(ctx: &JobCtx) -> Result<Outcome> {
    let conn = ctx.conn;
    let (run_id, resize): (String, String) =
        conn.query_row("SELECT id, resize FROM shrink_runs WHERE job_id = ?1", [&ctx.job.id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let resize: Resize = serde_json::from_str(&resize)?;
    let items: Vec<Item> = conn
        .prepare(
            "SELECT ord, photo_id, revision_id, filename, path FROM shrink_items
             WHERE run_id = ?1 AND status = 'planned' ORDER BY ord",
        )?
        .query_map([&run_id], |r| {
            let path = PathBuf::from(r.get::<_, String>(4)?);
            let temp = path.with_file_name(format!(".puri-shrink-{}.part", util::new_id()));
            Ok(Item { ord: r.get(0)?, photo_id: r.get(1)?, revision_id: r.get(2)?, filename: r.get(3)?, path, temp })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let (mut done, mut failed, mut skipped) = (ctx.job.done, ctx.job.failed, ctx.job.skipped);
    let total = done + failed + skipped + items.len() as i64;
    let mut last_flush = Instant::now();
    // ย่อพร้อมกันทีละชุด (ใช้ CPU มาก) แล้วแทนที่ทีละรูปบนเธรดนี้ · หยุด/ยกเลิกมีผลหลังชุดที่กำลังทำ
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    for chunk in items.chunks(workers) {
        let c = ctx.control()?;
        if c != Control::Continue {
            ctx.progress(total, done, failed, skipped, true)?;
            return Ok(JobCtx::outcome_for(c));
        }
        // บันทึกตำแหน่งไฟล์ชั่วคราวก่อนเขียน: แอปปิดกลางคันก็ตามลบได้ ไม่มีไฟล์ค้างในโฟลเดอร์ของผู้ใช้
        for item in chunk {
            conn.execute(
                "UPDATE shrink_items SET temp_path = ?3 WHERE run_id = ?1 AND ord = ?2",
                params![run_id, item.ord, item.temp.to_string_lossy()],
            )?;
        }
        let revisions: Vec<Result<Revision>> = chunk.iter().map(|i| load_revision(conn, &i.revision_id)).collect();
        let results: Vec<Result<Step>> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .zip(revisions)
                .map(|(item, rev)| scope.spawn(move || rev.and_then(|rev| prepare(item, &rev, resize))))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err(AppError::internal("ย่อรูปนี้ไม่สำเร็จ (ตัวประมวลผลภาพหยุดทำงาน)"))))
                .collect()
        });
        for (item, result) in chunk.iter().zip(results) {
            let outcome = match result {
                Ok(Step::Ready(p)) => replace(conn, &run_id, item, &p),
                Ok(Step::Skip(note)) => {
                    set_status(conn, &run_id, item.ord, "skipped", Some(&note))?;
                    skipped += 1;
                    continue;
                }
                Err(e) => Err(e),
            };
            match outcome {
                Ok(Settled::Shrunk) => done += 1,
                Ok(Settled::Reverted) => {
                    failed += 1;
                    set_status(conn, &run_id, item.ord, "failed", Some("แทนที่ไฟล์ไม่สำเร็จ ต้นฉบับอยู่ที่เดิม"))?;
                }
                Ok(Settled::Failed(note)) => {
                    failed += 1;
                    ctx.error(Some(&item.photo_id), &item.filename, &AppError::new("ACCESS_DENIED", note))?;
                }
                Err(e) => {
                    failed += 1;
                    // รายการที่ผ่านขั้น replacing แล้วคงสถานะไว้ (ห้ามลบไฟล์ชั่วคราว — อาจเป็นไฟล์ที่ย่อชุดเดียวที่มี
                    // ขณะต้นฉบับอยู่ในถังขยะ): recover ทำต่อตอนเปิดแอปครั้งหน้า
                    conn.execute(
                        "UPDATE shrink_items SET status = 'failed', note = ?3, temp_path = NULL
                         WHERE run_id = ?1 AND ord = ?2 AND status = 'planned'",
                        params![run_id, item.ord, e.message],
                    )?;
                    ctx.error(Some(&item.photo_id), &item.filename, &e)?;
                }
            }
        }
        if last_flush.elapsed() >= Duration::from_millis(250) {
            last_flush = Instant::now();
            ctx.progress(total, done, failed, skipped, true)?;
        }
    }
    ctx.progress(total, done, failed, skipped, true)?;
    Ok(Outcome::Done)
}

/// เปิดแอปหลังปิดกลางคัน: ลบไฟล์ชั่วคราวที่ยังไม่ถูกใช้ และทำรายการที่กำลังแทนที่ให้จบ
/// ต้องเรียกก่อน worker เริ่มสแกน — ไม่งั้นตัวสแกนจะเห็นไฟล์ที่ย่อเป็นเนื้อหาใหม่
pub(crate) fn recover(conn: &Connection) -> Result<()> {
    let orphans: Vec<(String, i64, String)> = conn
        .prepare("SELECT run_id, ord, temp_path FROM shrink_items WHERE status = 'planned' AND temp_path IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (run_id, ord, temp) in orphans {
        let _ = std::fs::remove_file(&temp);
        conn.execute("UPDATE shrink_items SET temp_path = NULL WHERE run_id = ?1 AND ord = ?2", params![run_id, ord])?;
    }
    let pending: Vec<Replacing> = conn
        .prepare(
            "SELECT run_id, ord, revision_id, path, temp_path, new_sha256, new_width, new_height
             FROM shrink_items WHERE status = 'replacing'",
        )?
        .query_map([], |r| {
            Ok(Replacing {
                run_id: r.get(0)?,
                ord: r.get(1)?,
                revision_id: r.get(2)?,
                path: PathBuf::from(r.get::<_, String>(3)?),
                temp: r.get::<_, Option<String>>(4)?.map(PathBuf::from),
                sha256: r.get(5)?,
                width: r.get(6)?,
                height: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    for r in pending {
        if let Err(e) = settle(conn, &r) {
            // ไดรฟ์ยังไม่ได้ต่อ ฯลฯ: คงสถานะไว้ ลองใหม่ตอนเปิดแอปครั้งหน้า
            eprintln!("shrink recover {}:{}: {}", r.run_id, r.ord, e.message);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportItem {
    pub photo_id: String,
    pub filename: String,
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShrinkReport {
    pub job_id: String,
    pub resize: Resize,
    pub input_count: usize,
    pub shrunk: usize,
    pub skipped: usize,
    pub failed: usize,
    pub unavailable: usize,
    pub cancelled: usize,
    /// ยังไม่ได้ทำ หรือค้างระหว่างแทนที่ (ทำต่อเมื่อเปิดแอปครั้งหน้า)
    pub pending: usize,
    /// ขนาดรวมก่อนและหลังย่อ ของรูปที่ย่อสำเร็จ
    pub source_bytes: u64,
    pub output_bytes: u64,
    pub items: Vec<ReportItem>,
}

pub fn report(conn: &Connection, job_id: &str) -> Result<ShrinkReport> {
    let (run_id, resize): (String, String) = conn
        .query_row("SELECT id, resize FROM shrink_runs WHERE job_id = ?1", [job_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("งานย่อไฟล์ต้นฉบับ"))?;
    let (source_bytes, output_bytes): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(old_size), 0), COALESCE(SUM(new_size), 0) FROM shrink_items
         WHERE run_id = ?1 AND status = 'shrunk'",
        [&run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let items: Vec<ReportItem> = conn
        .prepare("SELECT photo_id, filename, status, note FROM shrink_items WHERE run_id = ?1 ORDER BY ord")?
        .query_map([&run_id], |r| {
            Ok(ReportItem { photo_id: r.get(0)?, filename: r.get(1)?, status: r.get(2)?, note: r.get(3)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let count = |s: &str| items.iter().filter(|i| i.status == s).count();
    Ok(ShrinkReport {
        job_id: job_id.to_string(),
        resize: serde_json::from_str(&resize)?,
        input_count: items.len(),
        shrunk: count("shrunk"),
        skipped: count("skipped"),
        failed: count("failed"),
        unavailable: count("unavailable"),
        cancelled: count("cancelled"),
        pending: count("planned") + count("replacing"),
        source_bytes: source_bytes as u64,
        output_bytes: output_bytes as u64,
        items,
    })
}
