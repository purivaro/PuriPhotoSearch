//! ส่งออกสำเนา (Blueprint หมวด 13.1–13.2)
//! คัดลอก bytes ของต้นฉบับไปปลายทางโดยไม่ re-encode ไม่ย้าย/แก้/ลบต้นฉบับ และไม่เขียนทับไฟล์ปลายทางที่มีอยู่

use crate::error::{AppError, Result};
use crate::import::{remember, replay};
use crate::jobs::{self, Control, JobCtx, JobSnapshot, Outcome};
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

struct PlanItem {
    photo_id: String,
    revision_id: String,
    filename: String,
    source_path: String,
    /// None = ต้นฉบับไม่พร้อม จึงไม่มีไฟล์ปลายทาง
    planned_name: Option<String>,
}

pub struct PlanData {
    destination: PathBuf,
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

/// (photo_id, revision_id, filename, source_path, size)
type SourceRow = (String, String, String, String, u64);

fn load_items(conn: &Connection, selection: &SelectionSpec) -> Result<Vec<SourceRow>> {
    const ROW: &str = "p.id, p.current_revision_id, l.filename, l.os_path, r.size
        FROM photos p JOIN photo_locations l ON l.photo_id = p.id JOIN photo_revisions r ON r.id = p.current_revision_id";
    let map = |r: &rusqlite::Row| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? as u64));
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
            Ok(rows.into_iter().filter(|r| !excluded.contains(r.0.as_str())).collect())
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

pub fn preview(app: &App, selection: &SelectionSpec, destination: &str) -> Result<ExportPlan> {
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
    let (mut planned, mut unavailable, mut renamed, mut bytes) = (0usize, 0usize, 0usize, 0u64);
    for (photo_id, revision_id, filename, source_path, size) in rows {
        let planned_name = if Path::new(&source_path).is_file() {
            let name = unique_name(&filename, &mut taken);
            planned += 1;
            bytes += size;
            if name != filename {
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
        items.push(PlanItem { photo_id, revision_id, filename, source_path, planned_name });
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
        sample_names: samples,
        can_start: blocked_reason.is_none(),
        blocked_reason,
    };
    let mut plans = app.export_plans.lock().unwrap_or_else(|e| e.into_inner());
    plans.retain(|_, p| p.created.elapsed() < PLAN_TTL);
    plans.insert(plan.plan_id.clone(), PlanData { destination: dest, items, created: Instant::now() });
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
        tx.execute(
            "INSERT INTO jobs(id, kind, state, stage, title, dedup_key, total, skipped, total_known, created_at, updated_at)
             VALUES (?1, 'export', 'queued', 'copy', ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![job_id, format!("ส่งออกสำเนา {planned} รูป"), format!("export:{run_id}"), planned as i64, (plan.items.len() - planned) as i64, now],
        )?;
        tx.execute(
            "INSERT INTO export_runs(id, job_id, destination, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![run_id, job_id, plan.destination.to_string_lossy(), now],
        )?;
        for (ord, item) in plan.items.iter().enumerate() {
            tx.execute(
                "INSERT INTO export_items(run_id, ord, photo_id, revision_id, filename, source_path, planned_name, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    run_id,
                    ord as i64,
                    item.photo_id,
                    item.revision_id,
                    item.filename,
                    item.source_path,
                    item.planned_name,
                    if item.planned_name.is_some() { "planned" } else { "unavailable" }
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
fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
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

fn copy_one(conn: &Connection, item: &CopyItem, dest: &Path) -> Result<PathBuf> {
    let changed =
        || AppError::new("REVISION_CHANGED", "ไฟล์ต้นฉบับเปลี่ยนหลังจากเพิ่มเข้าคลัง สั่ง “ตรวจรูปใหม่” ก่อนส่งออก").retryable();
    let (size, mtime_ns, sha): (i64, i64, Option<String>) =
        conn.query_row("SELECT size, mtime_ns, sha256 FROM photo_revisions WHERE id = ?1", [&item.revision_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    let src = Path::new(&item.source_path);
    let stamp = media::stamp(src)?;
    if stamp.size != size as u64 || stamp.mtime_ns != mtime_ns {
        return Err(changed());
    }

    // เขียนไฟล์ชั่วคราวในปลายทางก่อน แล้วค่อยเปลี่ยนเป็นชื่อจริงเมื่อครบ: ไฟล์ชื่อจริงจึงสมบูรณ์เสมอ
    let tmp = dest.join(format!(".puri-export-{}.part", util::new_id()));
    let copied = (|| -> Result<(u64, String)> {
        let mut input = File::open(src)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
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
        Ok((total, util::hex(&hasher.finalize())))
    })();
    let verified = copied.and_then(|(total, hash)| {
        let same_file = total == stamp.size && media::stamp(src).ok() == Some(stamp);
        if !same_file || sha.as_deref().is_some_and(|s| s != hash) {
            return Err(changed());
        }
        Ok(())
    });
    if let Err(e) = verified {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    let mut name = item.planned_name.clone();
    let mut attempt = 2;
    loop {
        let target = dest.join(&name);
        match rename_noreplace(&tmp, &target) {
            Ok(()) => return Ok(target),
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
    let (run_id, destination): (String, String) =
        conn.query_row("SELECT id, destination FROM export_runs WHERE job_id = ?1", [&ctx.job.id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let dest = PathBuf::from(destination);
    if !dest.is_dir() {
        return Ok(Outcome::Blocked("โฟลเดอร์ปลายทางไม่พร้อมใช้งาน เชื่อมต่อไดรฟ์แล้วสั่งทำต่อ".into()));
    }
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
    for item in &items {
        let c = ctx.control()?;
        if c != Control::Continue {
            ctx.progress(total, done, failed, ctx.job.skipped, true)?;
            return Ok(JobCtx::outcome_for(c));
        }
        match copy_one(conn, item, &dest) {
            Ok(path) => {
                done += 1;
                conn.execute(
                    "UPDATE export_items SET status = 'copied', output_path = ?3 WHERE run_id = ?1 AND ord = ?2",
                    params![run_id, item.ord, path.to_string_lossy()],
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
    pub items: Vec<ReportItem>,
}

/// รายงานผลครบทุกรูปที่เลือก (N inputs): สำเร็จ / ผิดพลาด / ต้นฉบับไม่พร้อม / ถูกยกเลิก
pub fn report(conn: &Connection, job_id: &str) -> Result<ExportReport> {
    let (run_id, destination): (String, String) = conn
        .query_row("SELECT id, destination FROM export_runs WHERE job_id = ?1", [job_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("งานส่งออก"))?;
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
        items,
    })
}
