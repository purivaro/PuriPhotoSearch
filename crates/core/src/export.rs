//! ส่งออกสำเนา (Blueprint หมวด 13.1–13.2)
//! คัดลอก bytes ของต้นฉบับไปปลายทาง หรือสร้างสำเนาที่ย่อขนาดเป็น JPEG เมื่อผู้ใช้เลือก
//! ไม่ย้าย/แก้/ลบต้นฉบับ และไม่เขียนทับไฟล์ปลายทางที่มีอยู่

use crate::error::{AppError, Result};
use crate::import::{remember, replay};
use crate::jobs::{self, Control, JobCtx, JobSnapshot, Outcome};
use crate::watermark::WatermarkSpec;
use crate::{db, media, util, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PLAN_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_EXPLICIT: usize = 100_000;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionSpec {
    #[serde(rename_all = "camelCase")]
    Explicit { photo_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Snapshot {
        selection_token: String,
        #[serde(default)]
        excluded_photo_ids: Vec<String>,
    },
}

/// วิธีสร้างสำเนา: คัดลอกไฟล์เดิมทุกไบต์ หรือย่อเป็น JPEG (รูปที่เล็กพออยู่แล้วคัดลอกไฟล์เดิม)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Resize {
    #[default]
    Original,
    /// ด้านยาวไม่เกิน `px` พิกเซล
    LongEdge { px: u32 },
    /// ขนาดไฟล์ไม่เกิน `bytes` ต่อรูป
    MaxBytes { bytes: u64 },
}

impl Resize {
    pub(crate) fn validate(self) -> Result<()> {
        match self {
            Resize::LongEdge { px } if !(320..=10_000).contains(&px) => {
                Err(AppError::invalid("ด้านยาวต้องอยู่ระหว่าง 320 ถึง 10,000 พิกเซล"))
            }
            // KB/MB ฐาน 1024 ตรงกับที่ UI แสดงขนาดไฟล์
            Resize::MaxBytes { bytes } if !(50 * 1024..=100 * 1024 * 1024).contains(&bytes) => {
                Err(AppError::invalid("ขนาดไฟล์ต้องอยู่ระหว่าง 50 KB ถึง 100 MB"))
            }
            _ => Ok(()),
        }
    }

    /// วิธีย่อของรูปนี้ หรือ None เมื่อคัดลอกไฟล์เดิมได้เลย (ไม่ได้เลือกย่อ หรือรูปเล็กพออยู่แล้ว)
    /// `width`/`height` เป็น 0 เมื่อยังไม่รู้ขนาดภาพ: ถือว่าต้องย่อ (ตัวย่อไม่ขยายรูปที่เล็กกว่า)
    pub(crate) fn fit_for(self, size: u64, width: u32, height: u32) -> Option<media::Fit> {
        match self {
            Resize::Original => None,
            Resize::LongEdge { px } => {
                (width == 0 || height == 0 || width.max(height) > px).then_some(media::Fit::LongEdge(px))
            }
            Resize::MaxBytes { bytes } => (size > bytes).then_some(media::Fit::MaxBytes(bytes)),
        }
    }
}

/// สำเนาที่ย่อเป็น JPEG เสมอ: ใช้นามสกุลเดิมถ้าเป็น JPEG อยู่แล้ว ไม่งั้นเปลี่ยนเป็น .jpg
fn jpeg_name(filename: &str) -> String {
    let (stem, ext) = split_name(filename);
    if ext.eq_ignore_ascii_case(".jpg") || ext.eq_ignore_ascii_case(".jpeg") {
        filename.to_string()
    } else {
        format!("{stem}.jpg")
    }
}

/// ขนาดไฟล์โดยประมาณของสำเนา: ย่อด้านยาวคิดจากจำนวนพิกเซลของสำเนา × ไบต์ต่อพิกเซลของ JPEG คุณภาพ 90
/// (ไม่เกินขนาดต้นฉบับ) · โหมดขนาดไฟล์ใช้เพดานที่ตั้งไว้
pub(crate) fn estimate_bytes(fit: Option<media::Fit>, size: u64, width: u32, height: u32) -> u64 {
    let px = match fit {
        None => return size,
        Some(media::Fit::MaxBytes(max)) => return max,
        Some(media::Fit::LongEdge(px)) => px,
        Some(media::Fit::Original) => u32::MAX,
    };
    if width == 0 || height == 0 {
        return size;
    }
    let scale = (px as f64 / width.max(height) as f64).min(1.0);
    let pixels = width as f64 * height as f64 * scale * scale;
    ((pixels * media::BYTES_PER_PIXEL_GUESS) as u64).min(size)
}

/// วิธีเขียนสำเนาของรูปนี้ (None = คัดลอกไฟล์เดิมทุกไบต์) — ใส่ลายน้ำต้องบันทึกใหม่เสมอแม้ไม่ย่อ
/// ใช้ทั้งตอนวางแผนและตอนทำ นามสกุลที่วางแผนไว้จึงตรงกับไฟล์ที่เขียน
fn fit_with(resize: Resize, watermark: bool, size: u64, width: u32, height: u32) -> Option<media::Fit> {
    resize.fit_for(size, width, height).or(watermark.then_some(media::Fit::Original))
}

struct PlanItem {
    photo_id: String,
    revision_id: String,
    filename: String,
    source_path: String,
    size: u64,
    /// None = ต้นฉบับไม่พร้อม จึงไม่มีไฟล์ปลายทาง
    planned_name: Option<String>,
}

pub struct PlanData {
    destination: PathBuf,
    resize: Resize,
    watermark: Option<WatermarkSpec>,
    items: Vec<PlanItem>,
    created: Instant,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleName {
    pub source: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlan {
    pub plan_id: String,
    pub destination: String,
    pub input_count: usize,
    pub planned_output_count: usize,
    pub unavailable_inputs: usize,
    pub renamed_count: usize,
    pub estimated_bytes: u64,
    /// ขนาดรวมของต้นฉบับที่จะส่งออก ให้เทียบกับขนาดของสำเนาได้
    pub source_bytes: u64,
    /// วิธีสร้างสำเนาของแผนนี้
    pub resize: Resize,
    /// ไฟล์ที่จะถูกย่อ (ที่เหลือเล็กพออยู่แล้ว คัดลอกไฟล์เดิม หรือบันทึกใหม่ขนาดเดิมเมื่อใส่ลายน้ำ)
    pub resized_count: usize,
    /// ลายน้ำของแผนนี้ (ใส่ทุกสำเนา)
    pub watermark: Option<WatermarkSpec>,
    pub sample_names: Vec<SampleName>,
    pub can_start: bool,
    pub blocked_reason: Option<String>,
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

/// ชื่อไฟล์ปลายทางที่ไม่ชนกับ `taken` (เทียบแบบไม่สนตัวพิมพ์ เพราะดิสก์ Mac ส่วนใหญ่ไม่แยกตัวพิมพ์)
fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    let (stem, ext) = split_name(name);
    let mut candidate = name.to_string();
    let mut n = 2;
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("{stem} ({n}){ext}");
        n += 1;
    }
    candidate
}

fn existing_names(dir: &Path) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    for entry in std::fs::read_dir(dir)? {
        names.insert(entry?.file_name().to_string_lossy().to_lowercase());
    }
    Ok(names)
}

pub(crate) struct SourceRow {
    pub photo_id: String,
    pub revision_id: String,
    pub filename: String,
    pub source_path: String,
    pub size: u64,
    /// ขนาดภาพหลังหมุนตาม EXIF (0 = ยังไม่รู้)
    pub width: u32,
    pub height: u32,
}

pub(crate) fn load_items(conn: &Connection, selection: &SelectionSpec) -> Result<Vec<SourceRow>> {
    const ROW: &str = "p.id, p.current_revision_id, l.filename, l.os_path, r.size, r.width, r.height
        FROM photos p JOIN photo_locations l ON l.photo_id = p.id JOIN photo_revisions r ON r.id = p.current_revision_id";
    let map = |r: &rusqlite::Row| {
        Ok(SourceRow {
            photo_id: r.get(0)?,
            revision_id: r.get(1)?,
            filename: r.get(2)?,
            source_path: r.get(3)?,
            size: r.get::<_, i64>(4)? as u64,
            width: r.get(5)?,
            height: r.get(6)?,
        })
    };
    match selection {
        SelectionSpec::Explicit { photo_ids } => {
            if photo_ids.len() > MAX_EXPLICIT {
                return Err(AppError::invalid("เลือกรูปมากเกินไป ใช้ “เลือกผลค้นหาทั้งหมด” แทน"));
            }
            let mut seen = HashSet::new();
            let mut stmt = conn.prepare(&format!("SELECT {ROW} WHERE p.id = ?1 AND p.catalog_state = 'active'"))?;
            let mut out = vec![];
            for id in photo_ids.iter().filter(|id| seen.insert(id.as_str())) {
                out.push(
                    stmt.query_row([id], map)
                        .optional()?
                        .ok_or_else(|| AppError::not_found("รูป").ids(vec![id.clone()]))?,
                );
            }
            Ok(out)
        }
        SelectionSpec::Snapshot { selection_token, excluded_photo_ids } => {
            if !conn.query_row("SELECT EXISTS(SELECT 1 FROM selections WHERE token = ?1)", [selection_token], |r| {
                r.get::<_, bool>(0)
            })? {
                return Err(
                    AppError::new("PREVIEW_STALE", "ชุดรูปที่เลือกไว้หมดอายุแล้ว กรุณาเลือกผลค้นหาอีกครั้ง").next("refresh_preview")
                );
            }
            let excluded: HashSet<&str> = excluded_photo_ids.iter().map(String::as_str).collect();
            let rows: Vec<SourceRow> = conn
                .prepare(&format!(
                    "SELECT {ROW} JOIN selection_items si ON si.photo_id = p.id WHERE si.token = ?1 AND p.catalog_state = 'active' ORDER BY si.ord"
                ))?
                .query_map([selection_token], map)?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows.into_iter().filter(|r| !excluded.contains(r.photo_id.as_str())).collect())
        }
    }
}

/// ปลายทางอยู่ในโฟลเดอร์ที่คลังติดตามอยู่หรือไม่ (กันวงจร คัดลอก → ถูกเพิ่มเข้าคลัง → คัดลอกอีก)
pub(crate) fn inside_tracked_source(conn: &Connection, dest: &Path) -> Result<bool> {
    let mut roots: Vec<String> = conn
        .prepare("SELECT root_path FROM sources WHERE catalog_state = 'active'")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    roots.extend(
        conn.prepare("SELECT folder_path FROM event_source_bindings WHERE active = 1")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    );
    Ok(roots.iter().any(|root| dest.starts_with(root)))
}

pub fn preview(
    app: &App,
    selection: &SelectionSpec,
    destination: &str,
    resize: Resize,
    watermark: Option<WatermarkSpec>,
) -> Result<ExportPlan> {
    resize.validate()?;
    if let Some(w) = &watermark {
        w.validate(&app.db())?;
    }
    let dest = util::canonical(destination)
        .map_err(|_| AppError::new("SOURCE_UNAVAILABLE", "ไม่พบโฟลเดอร์ปลายทาง กรุณาเลือกโฟลเดอร์อีกครั้ง"))?;
    if !dest.is_dir() {
        return Err(AppError::invalid("ปลายทางต้องเป็นโฟลเดอร์"));
    }
    let (rows, inside) = {
        let conn = app.db();
        (load_items(&conn, selection)?, inside_tracked_source(&conn, &dest)?)
    };
    if rows.is_empty() {
        return Err(AppError::invalid("ยังไม่ได้เลือกรูปที่จะส่งออก"));
    }

    let mut taken = existing_names(&dest)?;
    let (mut items, mut samples) = (Vec::with_capacity(rows.len()), Vec::new());
    let (mut planned, mut unavailable, mut renamed, mut resized) = (0usize, 0usize, 0usize, 0usize);
    let (mut bytes, mut source_bytes) = (0u64, 0u64);
    for SourceRow { photo_id, revision_id, filename, source_path, size, width, height } in rows {
        let planned_name = if Path::new(&source_path).is_file() {
            let fit = fit_with(resize, watermark.is_some(), size, width, height);
            let wanted = if fit.is_some() { jpeg_name(&filename) } else { filename.clone() };
            let name = unique_name(&wanted, &mut taken);
            planned += 1;
            bytes += estimate_bytes(fit, size, width, height);
            source_bytes += size;
            resized += resize.fit_for(size, width, height).is_some() as usize;
            if name != wanted {
                renamed += 1;
            }
            // ตัวอย่างชื่อ: ให้เห็นรายการที่ถูกเติมเลขท้ายก่อน
            if samples.len() < 5
                || (name != filename && samples.iter().filter(|s: &&SampleName| s.source != s.output).count() < 3)
            {
                samples.push(SampleName { source: filename.clone(), output: name.clone() });
            }
            Some(name)
        } else {
            unavailable += 1;
            None
        };
        items.push(PlanItem { photo_id, revision_id, filename, source_path, size, planned_name });
    }
    samples.sort_by_key(|s| s.source == s.output);
    samples.truncate(5);

    let blocked_reason = if inside {
        Some("ปลายทางอยู่ในโฟลเดอร์ที่คลังติดตามอยู่ กรุณาเลือกโฟลเดอร์อื่น เพื่อไม่ให้สำเนาถูกเพิ่มกลับเข้าคลังซ้ำ".to_string())
    } else if planned == 0 {
        Some("ต้นฉบับของรูปที่เลือกไม่พร้อมใช้งาน เชื่อมต่อโฟลเดอร์ต้นทางก่อนส่งออก".to_string())
    } else if let Err(e) = probe_writable(&dest) {
        Some(e.message)
    } else {
        None
    };

    let plan = ExportPlan {
        plan_id: util::new_id(),
        destination: dest.to_string_lossy().into_owned(),
        input_count: items.len(),
        planned_output_count: planned,
        unavailable_inputs: unavailable,
        renamed_count: renamed,
        estimated_bytes: bytes,
        source_bytes,
        resize,
        resized_count: resized,
        watermark: watermark.clone(),
        sample_names: samples,
        can_start: blocked_reason.is_none(),
        blocked_reason,
    };
    let mut plans = app.export_plans.lock().unwrap_or_else(|e| e.into_inner());
    plans.retain(|_, p| p.created.elapsed() < PLAN_TTL);
    plans.insert(
        plan.plan_id.clone(),
        PlanData { destination: dest, resize, watermark, items, created: Instant::now() },
    );
    Ok(plan)
}

fn probe_writable(dir: &Path) -> Result<()> {
    let probe = dir.join(format!(".puri-export-probe-{}", util::new_id()));
    OpenOptions::new().write(true).create_new(true).open(&probe)?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

pub fn start(app: &App, plan_id: &str, request_id: &str) -> Result<JobSnapshot> {
    let job_id = {
        let mut conn = app.db();
        if let Some(id) = replay::<String>(&conn, request_id)? {
            return jobs::get(&conn, &id);
        }
        let plan = app
            .export_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plan_id)
            .filter(|p| p.created.elapsed() < PLAN_TTL)
            .ok_or_else(|| {
                AppError::new("PREVIEW_STALE", "แผนส่งออกหมดอายุแล้ว กรุณาตรวจแผนอีกครั้ง").next("refresh_preview")
            })?;
        let tx = conn.transaction()?;
        if inside_tracked_source(&tx, &plan.destination)? {
            return Err(AppError::new("DESTINATION_INSIDE_SOURCE", "ปลายทางอยู่ในโฟลเดอร์ที่คลังติดตามอยู่ กรุณาเลือกโฟลเดอร์อื่น"));
        }
        let (job_id, run_id, now) = (util::new_id(), util::new_id(), util::now());
        let planned = plan.items.iter().filter(|i| i.planned_name.is_some()).count();
        if let Some(w) = &plan.watermark {
            w.validate(&tx)?;
        }
        let title = match (plan.resize != Resize::Original, plan.watermark.is_some()) {
            (false, false) => format!("ส่งออกสำเนา {planned} รูป"),
            (true, false) => format!("ส่งออกสำเนาแบบย่อขนาด {planned} รูป"),
            (false, true) => format!("ส่งออกสำเนาใส่ลายน้ำ {planned} รูป"),
            (true, true) => format!("ส่งออกสำเนาแบบย่อขนาดและใส่ลายน้ำ {planned} รูป"),
        };
        tx.execute(
            "INSERT INTO jobs(id, kind, state, stage, title, dedup_key, total, skipped, total_known, created_at, updated_at)
             VALUES (?1, 'export', 'queued', 'copy', ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![job_id, title, format!("export:{run_id}"), planned as i64, (plan.items.len() - planned) as i64, now],
        )?;
        let resize = (plan.resize != Resize::Original).then(|| serde_json::to_string(&plan.resize)).transpose()?;
        let watermark = plan.watermark.as_ref().map(serde_json::to_string).transpose()?;
        tx.execute(
            "INSERT INTO export_runs(id, job_id, destination, created_at, resize, watermark) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![run_id, job_id, plan.destination.to_string_lossy(), now, resize, watermark],
        )?;
        for (ord, item) in plan.items.iter().enumerate() {
            tx.execute(
                "INSERT INTO export_items(run_id, ord, photo_id, revision_id, filename, source_path, planned_name, status, source_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    run_id,
                    ord as i64,
                    item.photo_id,
                    item.revision_id,
                    item.filename,
                    item.source_path,
                    item.planned_name,
                    if item.planned_name.is_some() { "planned" } else { "unavailable" },
                    item.size as i64
                ],
            )?;
        }
        remember(&tx, request_id, "start_export", &job_id)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
        job_id
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    jobs::get(&app.db(), &job_id)
}

/// rename ที่ไม่เขียนทับไฟล์ปลายทาง (atomic บน APFS/HFS+ ผ่าน `renamex_np(RENAME_EXCL)`)
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let c =
            |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput);
        let (f, t) = (c(from)?, c(to)?);
        // SAFETY: ทั้งสองเป็น C string ที่ปิดท้ายด้วย NUL และมีชีวิตอยู่ตลอดการเรียก
        if unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) } == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOTSUP) {
            return Err(err);
        }
    }
    // ระบบไฟล์ที่ไม่รองรับ (เช่น exFAT): ตรวจก่อนแล้วค่อย rename
    if to.exists() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    std::fs::rename(from, to)
}

struct CopyItem {
    ord: i64,
    photo_id: String,
    revision_id: String,
    filename: String,
    source_path: String,
    planned_name: String,
}

fn revision_changed() -> AppError {
    AppError::new("REVISION_CHANGED", "ไฟล์ต้นฉบับเปลี่ยนหลังจากเพิ่มเข้าคลัง สั่ง “ตรวจรูปใหม่” ก่อนส่งออก").retryable()
}

/// คัดลอก bytes ของต้นฉบับลง `tmp` พร้อม hash ระหว่างคัดลอก แล้วตรวจว่าตรงกับที่คลังรู้จัก
fn write_copy(src: &Path, stamp: media::FileStamp, sha: Option<&str>, tmp: &Path) -> Result<u64> {
    let mut input = File::open(src)?;
    let mut output = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        output.write_all(&buf[..n])?;
        total += n as u64;
    }
    if let Ok(modified) = input.metadata().and_then(|m| m.modified()) {
        let _ = output.set_modified(modified);
    }
    output.sync_all()?;
    let same_file = total == stamp.size && media::stamp(src).ok() == Some(stamp);
    if !same_file || sha.is_some_and(|s| s != util::hex(&hasher.finalize())) {
        return Err(revision_changed());
    }
    Ok(total)
}

/// อ่านต้นฉบับทั้งไฟล์ ตรวจว่าตรงกับที่คลังรู้จัก แล้วเขียนสำเนาที่ย่อแล้วลง `tmp` (เวลาแก้ไขไฟล์เท่าต้นฉบับ)
fn write_resized(
    src: &Path,
    stamp: media::FileStamp,
    sha: Option<&str>,
    fit: media::Fit,
    watermark: Option<&media::Watermark>,
    tmp: &Path,
) -> Result<u64> {
    let bytes = media::read_file(src)?;
    let same_file = bytes.len() as u64 == stamp.size && media::stamp(src).ok() == Some(stamp);
    if !same_file || sha.is_some_and(|s| s != util::hex(&Sha256::digest(&bytes))) {
        return Err(revision_changed());
    }
    let jpeg = media::export_jpeg(&bytes, fit, watermark)?;
    drop(bytes);
    let mut output = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    output.write_all(&jpeg)?;
    if let Ok(modified) = std::fs::metadata(src).and_then(|m| m.modified()) {
        let _ = output.set_modified(modified);
    }
    output.sync_all()?;
    Ok(jpeg.len() as u64)
}

/// ข้อมูลของ revision ที่คลังรู้จัก ใช้ตรวจว่าต้นฉบับไม่เปลี่ยน และตัดสินว่าต้องย่อหรือไม่
pub(crate) struct Revision {
    pub size: i64,
    pub mtime_ns: i64,
    pub sha: Option<String>,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn load_revision(conn: &Connection, revision_id: &str) -> Result<Revision> {
    Ok(conn.query_row(
        "SELECT size, mtime_ns, sha256, width, height FROM photo_revisions WHERE id = ?1",
        [revision_id],
        |r| Ok(Revision { size: r.get(0)?, mtime_ns: r.get(1)?, sha: r.get(2)?, width: r.get(3)?, height: r.get(4)? }),
    )?)
}

/// คืน (ไฟล์ที่สร้าง, จำนวนไบต์ที่เขียน) — ไม่แตะฐานข้อมูล จึงทำหลายรูปพร้อมกันได้
fn copy_one(
    item: &CopyItem,
    rev: &Revision,
    dest: &Path,
    resize: Resize,
    watermark: Option<&media::Watermark>,
) -> Result<(PathBuf, u64)> {
    let Revision { size, mtime_ns, ref sha, width, height } = *rev;
    let src = Path::new(&item.source_path);
    let stamp = media::stamp(src)?;
    if stamp.size != size as u64 || stamp.mtime_ns != mtime_ns {
        return Err(revision_changed());
    }

    // เขียนไฟล์ชั่วคราวในปลายทางก่อน แล้วค่อยเปลี่ยนเป็นชื่อจริงเมื่อครบ: ไฟล์ชื่อจริงจึงสมบูรณ์เสมอ
    // วิธีย่อตัดสินจากข้อมูลของ revision เดียวกับตอนวางแผน จึงตรงกับนามสกุลที่วางแผนไว้
    let tmp = dest.join(format!(".puri-export-{}.part", util::new_id()));
    let written = match fit_with(resize, watermark.is_some(), size as u64, width, height) {
        Some(fit) => write_resized(src, stamp, sha.as_deref(), fit, watermark, &tmp),
        None => write_copy(src, stamp, sha.as_deref(), &tmp),
    };
    let written = match written {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };

    let mut name = item.planned_name.clone();
    let mut attempt = 2;
    loop {
        let target = dest.join(&name);
        match rename_noreplace(&tmp, &target) {
            Ok(()) => return Ok((target, written)),
            // มีไฟล์ชื่อนี้เกิดขึ้นหลังวางแผน: เติมเลขท้ายต่อ ไม่เขียนทับ
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 10_000 => {
                let (stem, ext) = split_name(&item.planned_name);
                name = format!("{stem} ({attempt}){ext}");
                attempt += 1;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
    }
}

pub(crate) fn run(ctx: &JobCtx) -> Result<Outcome> {
    let conn = ctx.conn;
    let (run_id, destination, resize, watermark): (String, String, Option<String>, Option<String>) = conn.query_row(
        "SELECT id, destination, resize, watermark FROM export_runs WHERE job_id = ?1",
        [&ctx.job.id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let resize: Resize = resize.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default();
    let dest = PathBuf::from(destination);
    if !dest.is_dir() {
        return Ok(Outcome::Blocked("โฟลเดอร์ปลายทางไม่พร้อมใช้งาน เชื่อมต่อไดรฟ์แล้วสั่งทำต่อ".into()));
    }
    // โหลดโลโก้ครั้งเดียวต่องาน · ถูกลบไปแล้ว: ไม่ส่งออกสำเนาที่ไม่มีลายน้ำแทนโดยไม่บอก
    let watermark = match watermark.as_deref().map(serde_json::from_str::<WatermarkSpec>).transpose()? {
        Some(spec) => match spec.load(conn)? {
            Some(w) => Some(w),
            None => {
                return Ok(Outcome::Blocked(
                    "ไม่พบลายน้ำที่เลือก (ถูกลบออกจากรายการแล้ว) ยกเลิกงานนี้แล้วส่งออกใหม่พร้อมเลือกลายน้ำ".into(),
                ))
            }
        },
        None => None,
    };
    let watermark = watermark.as_ref();
    let items: Vec<CopyItem> = conn
        .prepare(
            "SELECT ord, photo_id, revision_id, filename, source_path, planned_name FROM export_items
             WHERE run_id = ?1 AND status = 'planned' ORDER BY ord",
        )?
        .query_map([&run_id], |r| {
            Ok(CopyItem {
                ord: r.get(0)?,
                photo_id: r.get(1)?,
                revision_id: r.get(2)?,
                filename: r.get(3)?,
                source_path: r.get(4)?,
                planned_name: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let (mut done, mut failed) = (ctx.job.done, ctx.job.failed);
    let total = done + failed + items.len() as i64;
    let mut last_flush = Instant::now();
    // ทำทีละชุดพร้อมกันหลายรูป (การย่อใช้ CPU มาก) · จำกัดไว้ 4 เพื่อคุมหน่วยความจำของภาพที่ถอดรหัสค้างไว้
    // ฐานข้อมูลอ่าน/เขียนในเธรดนี้เท่านั้น · หยุด/ยกเลิกมีผลหลังชุดที่กำลังทำ
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    for chunk in items.chunks(workers) {
        let c = ctx.control()?;
        if c != Control::Continue {
            ctx.progress(total, done, failed, ctx.job.skipped, true)?;
            return Ok(JobCtx::outcome_for(c));
        }
        let revisions: Vec<Result<Revision>> =
            chunk.iter().map(|item| load_revision(conn, &item.revision_id)).collect();
        let results: Vec<Result<(PathBuf, u64)>> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .zip(revisions)
                .map(|(item, rev)| {
                    let dest = &dest;
                    scope.spawn(move || rev.and_then(|rev| copy_one(item, &rev, dest, resize, watermark)))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| Err(AppError::internal("สร้างสำเนารูปนี้ไม่สำเร็จ (ตัวประมวลผลภาพหยุดทำงาน)")))
                })
                .collect()
        });
        for (item, result) in chunk.iter().zip(results) {
            match result {
                Ok((path, written)) => {
                    done += 1;
                    conn.execute(
                    "UPDATE export_items SET status = 'copied', output_path = ?3, output_bytes = ?4 WHERE run_id = ?1 AND ord = ?2",
                    params![run_id, item.ord, path.to_string_lossy(), written as i64],
                )?;
                }
                Err(e) => {
                    failed += 1;
                    conn.execute(
                        "UPDATE export_items SET status = 'failed', error = ?3 WHERE run_id = ?1 AND ord = ?2",
                        params![run_id, item.ord, e.message],
                    )?;
                    ctx.error(Some(&item.photo_id), &item.filename, &e)?;
                }
            }
        }
        if last_flush.elapsed() >= Duration::from_millis(250) {
            last_flush = Instant::now();
            ctx.progress(total, done, failed, ctx.job.skipped, true)?;
        }
    }
    ctx.progress(total, done, failed, ctx.job.skipped, true)?;
    Ok(Outcome::Done)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportItem {
    pub photo_id: String,
    pub filename: String,
    pub status: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub job_id: String,
    pub destination: String,
    pub input_count: usize,
    pub copied: usize,
    pub failed: usize,
    pub unavailable: usize,
    pub cancelled: usize,
    pub resize: Resize,
    pub watermark: Option<WatermarkSpec>,
    /// ขนาดรวมของต้นฉบับและของสำเนา (เฉพาะรูปที่ส่งออกสำเร็จ)
    pub source_bytes: u64,
    pub output_bytes: u64,
    pub items: Vec<ReportItem>,
}

/// รายงานผลครบทุกรูปที่เลือก (N inputs): สำเร็จ / ผิดพลาด / ต้นฉบับไม่พร้อม / ถูกยกเลิก
pub fn report(conn: &Connection, job_id: &str) -> Result<ExportReport> {
    let (run_id, destination, resize, watermark): (String, String, Option<String>, Option<String>) = conn
        .query_row("SELECT id, destination, resize, watermark FROM export_runs WHERE job_id = ?1", [job_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("งานส่งออก"))?;
    let resize: Resize = resize.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default();
    let watermark: Option<WatermarkSpec> = watermark.as_deref().map(serde_json::from_str).transpose()?;
    // งานที่ทำก่อนมีคอลัมน์ output_bytes คัดลอกไฟล์เดิมเสมอ: ขนาดสำเนาเท่าต้นฉบับ
    // งานที่ทำก่อนมี source_bytes ใช้ขนาดปัจจุบันของ revision (ตรงเว้นแต่ต้นฉบับถูกย่อในที่เดิมภายหลัง)
    let (source_bytes, output_bytes): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(COALESCE(i.source_bytes, r.size)), 0), COALESCE(SUM(COALESCE(i.output_bytes, r.size)), 0)
         FROM export_items i JOIN photo_revisions r ON r.id = i.revision_id
         WHERE i.run_id = ?1 AND i.status = 'copied'",
        [&run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let items: Vec<ReportItem> = conn
        .prepare(
            "SELECT photo_id, filename, status, output_path, error FROM export_items WHERE run_id = ?1 ORDER BY ord",
        )?
        .query_map([&run_id], |r| {
            Ok(ReportItem {
                photo_id: r.get(0)?,
                filename: r.get(1)?,
                status: r.get(2)?,
                output_path: r.get(3)?,
                error: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let count = |s: &str| items.iter().filter(|i| i.status == s).count();
    Ok(ExportReport {
        job_id: job_id.to_string(),
        destination,
        input_count: items.len(),
        copied: count("copied"),
        failed: count("failed"),
        unavailable: count("unavailable"),
        cancelled: count("cancelled"),
        resize,
        watermark,
        source_bytes: source_bytes as u64,
        output_bytes: output_bytes as u64,
        items,
    })
}
