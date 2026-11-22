//! ตัวคน (Subject) แบบ "Select Subject" ของ Lightroom (DECISIONS ข้อ 40)
//!
//! โมเดลแยกตัวคน/สิ่งที่เป็นจุดสนใจออกจากฉาก ให้ภาพน้ำหนัก 0–255 ต่อพิกเซลของภาพเดิม (หลังหมุนตาม EXIF ก่อนตัด/หมุน
//! ของการแต่ง) — Mask "ตัวคน (AI)" ([`crate::masks::Area::Subject`]) ใช้ภาพนี้เป็นบริเวณ
//!
//! - ภาพน้ำหนักเป็นของรูป ไม่ใช่ของค่าที่แต่ง: ไม่เก็บใน `photo_edits` (ค่าที่แต่งเก็บแค่ `{"kind":"subject"}`)
//!   แต่เก็บใน cache ตาม revision ของไฟล์ (`cache/subject/`) แล้วแนบให้ค่าที่แต่งตอนสร้างภาพ ([`attach`])
//! - core ไม่รู้จัก ONNX Runtime: เปลือกแอปส่ง [`SubjectProvider`] เข้ามา (ไฟล์โมเดล + ตัวสร้าง [`Segmenter`])
//! - ไฟล์โมเดลไม่อยู่ในตัวติดตั้ง แอปดาวน์โหลดเมื่อผู้ใช้กดครั้งเดียว แล้วตรวจ SHA-256 ก่อนใช้ (แบบเดียวกับตัววิเคราะห์ใบหน้า)
//! - คิดทีละรูป (โมเดลใหญ่ ใช้ CPU ทุกแกนราว 7 วินาทีต่อรูป) — รูปอื่นที่ขอพร้อมกันรอคิว

use crate::body::Fill;
use crate::edit::EditSettings;
use crate::error::{AppError, Result};
use crate::jobs::{self, JobCtx, JobSnapshot, Outcome, ACTIVE_STATES};
use crate::masks::Area;
use crate::{db, media, util, App};
use image::{GrayImage, RgbImage};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// ด้านยาวของภาพที่ส่งให้โมเดลและของภาพน้ำหนักที่เก็บ (โมเดลใช้ 1024×1024)
pub const MAP_EDGE: u32 = 1024;
/// จำภาพน้ำหนักที่ใช้ล่าสุดไว้ในหน่วยความจำ (หน้าแต่งภาพขอทุกเฟรม)
const RECENT: usize = 6;
/// น้ำหนักที่ถือว่า "มี" ตอนหากรอบของบริเวณ (0–255)
const PRESENT: u8 = 8;

/// ตัวแยกตัวคนออกจากฉาก
pub trait Segmenter: Send + Sync {
    /// ความเป็นตัวคน/จุดสนใจของแต่ละพิกเซล 0–255 — ภาพผลขนาดเท่าภาพที่ส่งเข้า
    fn segment(&self, image: &RgbImage) -> Result<GrayImage>;
}

/// ภาพน้ำหนักของรูปหนึ่ง (แชร์ได้ ไม่คัดลอกภาพ)
#[derive(Clone)]
pub struct SubjectMap {
    image: Arc<GrayImage>,
    /// กรอบ [x0, y0, x1, y1] (สัดส่วน) ของส่วนที่มีน้ำหนัก — None = ไม่พบตัวคนเลย
    bounds: Option<[f64; 4]>,
    /// คนแต่ละคนและลำตัว (ตัวผอม) — คิดครั้งแรกที่ใช้
    people: Arc<std::sync::OnceLock<Vec<crate::body::Person>>>,
}

impl SubjectMap {
    pub fn new(image: GrayImage) -> SubjectMap {
        let (w, h) = image.dimensions();
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y, p) in image.enumerate_pixels() {
            if p[0] >= PRESENT {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
        let bounds = (x1 > x0 && y1 > y0)
            .then(|| [x0 as f64 / w as f64, y0 as f64 / h as f64, x1 as f64 / w as f64, y1 as f64 / h as f64]);
        SubjectMap { image: Arc::new(image), bounds, people: Default::default() }
    }

    /// คนแต่ละคนในภาพน้ำหนัก (ตัวผอม — [`crate::body`])
    pub fn people(&self) -> &[crate::body::Person] {
        self.people.get_or_init(|| crate::body::people(&self.image))
    }

    /// น้ำหนัก 0–1 ที่จุด (u, v) บนภาพเดิม (bilinear · นอกภาพ = 0)
    pub fn weight(&self, u: f64, v: f64) -> f64 {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return 0.0;
        }
        let (w, h) = self.image.dimensions();
        let x = (u * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let y = (v * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let at = |xx: u32, yy: u32| self.image.get_pixel(xx, yy)[0] as f64;
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        (top * (1.0 - fy) + bottom * fy) / 255.0
    }

    /// กรอบของส่วนที่มีน้ำหนัก (สัดส่วนบนภาพเดิม) — None = ไม่พบตัวคน
    pub fn bounds(&self) -> Option<[f64; 4]> {
        self.bounds
    }

    pub fn image(&self) -> &GrayImage {
        &self.image
    }
}

/// ภาพน้ำหนักเป็นของรูป ไม่ใช่ส่วนของค่าที่แต่ง: ค่าที่แต่งสองชุดเท่ากันไม่ว่าจะแนบภาพไว้หรือไม่
impl PartialEq for SubjectMap {
    fn eq(&self, _: &SubjectMap) -> bool {
        true
    }
}

impl std::fmt::Debug for SubjectMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubjectMap({}×{})", self.image.width(), self.image.height())
    }
}

// ───────────────────────── โมเดล ─────────────────────────

pub struct SubjectModel {
    pub id: &'static str,
    pub display_name: &'static str,
    /// ไฟล์ .onnx ไฟล์เดียว (ดาวน์โหลดตรง ไม่ต้องแตก)
    pub file: &'static str,
    pub sha256: &'static str,
    pub download_url: String,
    pub download_bytes: u64,
    pub download_source: &'static str,
    pub license_note: &'static str,
}

pub type SegmenterFactory = Arc<dyn Fn(&Path) -> Result<Arc<dyn Segmenter>> + Send + Sync>;

/// ตัวเติมฉากหลัง (ตัวผอม — DECISIONS ข้อ 41): เติมส่วนที่ตัวคนบังอยู่ให้เป็นฉากหลัง
pub trait Inpainter: Send + Sync {
    /// ด้านของภาพสี่เหลี่ยมจัตุรัสที่ตัวเติมรับ
    fn side(&self) -> u32;
    /// เติมช่องของภาพ `side`×`side` (`holes` มากกว่า 127 = ต้องเติม) คืนภาพขนาดเดียวกัน
    fn inpaint(&self, image: &RgbImage, holes: &GrayImage) -> Result<RgbImage>;
}

pub type InpainterFactory = Arc<dyn Fn(&Path) -> Result<Arc<dyn Inpainter>> + Send + Sync>;

pub struct FillProvider {
    pub model: SubjectModel,
    pub factory: InpainterFactory,
}

pub struct SubjectProvider {
    pub model: SubjectModel,
    pub factory: SegmenterFactory,
    /// ตัวเติมฉากหลังของตัวผอม — None = build นี้ทำตัวผอมไม่ได้ (Mask ตัวคนยังใช้ได้)
    pub fill: Option<FillProvider>,
}

/// ตัวโหลดโมเดลจากโฟลเดอร์ของมัน
type Factory<T> = Arc<dyn Fn(&Path) -> Result<Arc<T>> + Send + Sync>;

/// โมเดลหนึ่งตัว: ติดตั้งแล้วหรือยัง + ตัวที่โหลดแล้ว (โหลดครั้งแรกที่ถูกขอ)
struct Slot<T: ?Sized> {
    model: Option<SubjectModel>,
    factory: Option<Factory<T>>,
    /// ส่วนของ dedup_key ของงานติดตั้ง: subject | fill
    kind: &'static str,
    /// ชื่อที่ผู้ใช้เห็น
    name: &'static str,
    dir: PathBuf,
    loaded: Mutex<Option<Arc<T>>>,
    load_error: Mutex<Option<String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl<T: ?Sized> Slot<T> {
    fn new(spec: Option<(SubjectModel, Factory<T>)>, kind: &'static str, name: &'static str, root: &Path) -> Slot<T> {
        let dir = spec.as_ref().map_or_else(|| root.to_path_buf(), |(m, _)| root.join(m.id));
        let (model, factory) = match spec {
            Some((m, f)) => (Some(m), Some(f)),
            None => (None, None),
        };
        Slot { model, factory, kind, name, dir, loaded: Mutex::new(None), load_error: Mutex::new(None) }
    }

    fn marker(&self) -> PathBuf {
        self.dir.join("verified")
    }

    /// ไฟล์โมเดลอยู่ครบและผ่านการตรวจ SHA-256 แล้ว
    fn installed(&self) -> bool {
        self.model.as_ref().is_some_and(|m| self.marker().is_file() && self.dir.join(m.file).is_file())
    }

    /// ตัวที่พร้อมใช้ — None เมื่อยังไม่ติดตั้งหรือโหลดไม่ได้
    fn get(&self) -> Option<Arc<T>> {
        let factory = self.factory.as_ref()?;
        if !self.installed() {
            return None;
        }
        let mut loaded = lock(&self.loaded);
        if loaded.is_none() && lock(&self.load_error).is_none() {
            match factory(&self.dir) {
                Ok(s) => *loaded = Some(s),
                Err(e) => *lock(&self.load_error) = Some(e.message),
            }
        }
        loaded.clone()
    }

    fn dedup_key(&self) -> Option<String> {
        self.model.as_ref().map(|m| format!("model:{}:{}", self.kind, m.id))
    }
}

pub struct SubjectState {
    segment: Slot<dyn Segmenter>,
    fill: Slot<dyn Inpainter>,
    unsupported_reason: String,
    /// คิดทีละรูป (โมเดลใหญ่ทั้งสองตัวใช้ CPU ทุกแกน)
    busy: Mutex<()>,
    recent: Mutex<Vec<(String, SubjectMap)>>,
    /// ฉากหลังที่เติมแล้ว (ตัวผอม) ตาม revision และภาพตั้งต้น
    fills: Mutex<Vec<(String, crate::body::Fill)>>,
    /// ภาพที่เติมของส่วนที่ลบ (ลบคน/สิ่งของ) ตาม revision ภาพตั้งต้น และกรอบ — (key, ภาพ, พบสิ่งที่เด่นในกรอบ)
    patches: Mutex<Vec<(String, crate::remove::Patch, bool)>>,
}

impl SubjectState {
    pub fn new(provider: Option<SubjectProvider>, unsupported_reason: &str, models_root: &Path) -> SubjectState {
        let (segment, fill) = match provider {
            Some(p) => (Some((p.model, p.factory)), p.fill.map(|f| (f.model, f.factory))),
            None => (None, None),
        };
        SubjectState {
            segment: Slot::new(segment, "subject", "ตัวแยกตัวคน", models_root),
            fill: Slot::new(fill, "fill", "ตัวเติมฉากหลัง", models_root),
            unsupported_reason: unsupported_reason.to_string(),
            busy: Mutex::new(()),
            recent: Mutex::new(Vec::new()),
            fills: Mutex::new(Vec::new()),
            patches: Mutex::new(Vec::new()),
        }
    }

    pub fn model(&self) -> Option<&SubjectModel> {
        self.segment.model.as_ref()
    }

    /// ไฟล์ตัวแยกตัวคนอยู่ครบและผ่านการตรวจ SHA-256 แล้ว
    pub fn installed(&self) -> bool {
        self.segment.installed()
    }

    /// ตัวแยกที่พร้อมใช้ (โหลดโมเดลครั้งแรกที่ถูกขอ) — None เมื่อยังไม่ติดตั้งหรือโหลดไม่ได้
    pub fn segmenter(&self) -> Option<Arc<dyn Segmenter>> {
        self.segment.get()
    }

    /// ตัวเติมฉากหลังที่พร้อมใช้
    pub fn inpainter(&self) -> Option<Arc<dyn Inpainter>> {
        self.fill.get()
    }

    fn remember(&self, revision_id: &str, map: &SubjectMap) {
        let mut recent = lock(&self.recent);
        recent.retain(|(r, _)| r != revision_id);
        recent.insert(0, (revision_id.to_string(), map.clone()));
        recent.truncate(RECENT);
    }

    fn recall(&self, revision_id: &str) -> Option<SubjectMap> {
        lock(&self.recent).iter().find(|(r, _)| r == revision_id).map(|(_, m)| m.clone())
    }
}

/// สถานะของโมเดลหนึ่งตัว
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// unsupported | not_installed | installing | ready | failed
    pub state: &'static str,
    pub reason: String,
    pub model_name: String,
    pub download_size_mb: u64,
    pub download_source: String,
    pub license_note: String,
    pub install_job_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectStatus {
    /// ตัวแยกตัวคน (Mask ตัวคน และตัวผอม)
    #[serde(flatten)]
    pub segment: ModelStatus,
    /// ตัวเติมฉากหลัง (ตัวผอมใช้ร่วมกับตัวแยกตัวคน)
    pub fill: ModelStatus,
}

/// งานติดตั้งของตัวแยกตัวคน/ตัวเติมฉากหลัง (คนละงานกับตัววิเคราะห์ใบหน้า — ชนิดงาน `model` เหมือนกัน แยกด้วย dedup_key)
pub(crate) fn is_install_job(dedup_key: Option<&str>) -> bool {
    dedup_key.is_some_and(|k| k.starts_with("model:subject:") || k.starts_with("model:fill:"))
}

fn install_job(app: &App, key: &str) -> Result<Option<String>> {
    Ok(app
        .db()
        .query_row(
            &format!(
                "SELECT id FROM jobs WHERE kind = 'model' AND dedup_key = ?1 AND state IN {ACTIVE_STATES} LIMIT 1"
            ),
            [key],
            |r| r.get(0),
        )
        .optional()?)
}

fn slot_status<T: ?Sized>(
    app: &App,
    slot: &Slot<T>,
    unsupported: String,
    ready: &str,
    offer: &str,
) -> Result<ModelStatus> {
    let mut s = ModelStatus {
        state: "unsupported",
        reason: unsupported,
        model_name: String::new(),
        download_size_mb: 0,
        download_source: String::new(),
        license_note: String::new(),
        install_job_id: None,
    };
    let (Some(model), Some(key)) = (slot.model.as_ref(), slot.dedup_key()) else { return Ok(s) };
    s.model_name = model.display_name.to_string();
    s.download_size_mb = model.download_bytes.div_ceil(1_000_000);
    s.download_source = model.download_source.to_string();
    s.license_note = model.license_note.to_string();
    if slot.installed() {
        if slot.get().is_some() {
            s.state = "ready";
            s.reason = ready.to_string();
        } else {
            s.state = "failed";
            let detail = lock(&slot.load_error).clone().unwrap_or_default();
            s.reason = format!("โหลด{}ไม่สำเร็จ ({detail})", slot.name);
        }
        return Ok(s);
    }
    s.install_job_id = install_job(app, &key)?;
    if s.install_job_id.is_some() {
        s.state = "installing";
        s.reason = format!("กำลังดาวน์โหลด{} แต่งภาพส่วนอื่นได้ระหว่างรอ", slot.name);
    } else {
        s.state = "not_installed";
        s.reason = format!("ดาวน์โหลด{}ครั้งเดียว ({} MB) {offer} บนเครื่องนี้ ไม่ส่งรูปออกไปไหน", slot.name, s.download_size_mb);
    }
    Ok(s)
}

pub fn status(app: &App) -> Result<SubjectStatus> {
    let state = &app.ai.subject;
    let fill_unsupported = if state.model().is_none() {
        state.unsupported_reason.clone()
    } else {
        "build นี้ยังไม่มีตัวเติมฉากหลัง ตัวผอมจึงยังใช้ไม่ได้".to_string()
    };
    Ok(SubjectStatus {
        segment: slot_status(
            app,
            &state.segment,
            state.unsupported_reason.clone(),
            "พร้อมหาตัวคนในรูปบนเครื่องนี้",
            "แล้วแอปจะเลือกตัวคนในรูปให้เอง",
        )?,
        fill: slot_status(
            app,
            &state.fill,
            fill_unsupported,
            "พร้อมเติมฉากหลังบนเครื่องนี้",
            "ใช้กับตัวผอม: เติมฉากหลังตรงที่ตัวคนหดเข้าไป",
        )?,
    })
}

/// เริ่มงานดาวน์โหลดของโมเดลหนึ่งตัว (หรือคืนงานเดิมที่ยังทำอยู่)
fn start_install<T: ?Sized>(app: &App, slot: &Slot<T>, unsupported: &str) -> Result<String> {
    let (Some(model), Some(key)) = (slot.model.as_ref(), slot.dedup_key()) else {
        return Err(AppError::new("MODEL_NOT_READY", unsupported.to_string()));
    };
    let conn = app.db();
    let existing: Option<String> = conn
        .query_row(
            &format!(
                "SELECT id FROM jobs WHERE kind = 'model' AND dedup_key = ?1 AND state IN {ACTIVE_STATES} LIMIT 1"
            ),
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        conn.execute("UPDATE jobs SET state = 'queued' WHERE id = ?1 AND state IN ('paused', 'blocked')", [&id])?;
        return Ok(id);
    }
    let (id, now) = (util::new_id(), util::now());
    conn.execute(
        "INSERT INTO jobs(id, kind, state, stage, title, dedup_key, total, total_known, created_at, updated_at)
         VALUES (?1, 'model', 'queued', 'download', ?2, ?3, ?4, 1, ?5, ?5)",
        params![
            id,
            format!("ดาวน์โหลด{} ({})", slot.name, model.display_name),
            key,
            model.download_bytes.div_ceil(1_000_000) as i64,
            now
        ],
    )?;
    db::bump_revision(&conn)?;
    Ok(id)
}

/// เริ่มงานดาวน์โหลดตัวแยกตัวคน (หรือคืนงานเดิมที่ยังทำอยู่) · `fill` = ตัวเติมฉากหลังของตัวผอมด้วย
/// (เริ่มดาวน์โหลดตัวแยกตัวคนถ้ายังไม่มี แล้วคืนงานของตัวเติมฉากหลัง)
pub fn install(app: &App, fill: bool) -> Result<JobSnapshot> {
    let state = &app.ai.subject;
    let id = if fill {
        let fill_unsupported = "build นี้ยังไม่มีตัวเติมฉากหลัง ตัวผอมจึงยังใช้ไม่ได้";
        if state.fill.model.is_none() {
            return Err(AppError::new("MODEL_NOT_READY", fill_unsupported));
        }
        if !state.segment.installed() {
            start_install(app, &state.segment, &state.unsupported_reason)?;
        }
        start_install(app, &state.fill, fill_unsupported)?
    } else {
        start_install(app, &state.segment, &state.unsupported_reason)?
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    jobs::get(&app.db(), &id)
}

/// งานติดตั้ง: ดาวน์โหลด (ทำต่อจากเดิมได้) → ตรวจ checksum → ย้ายเข้าที่
pub(crate) fn run_install(ctx: &JobCtx, dedup_key: Option<&str>) -> Result<Outcome> {
    let state = &ctx.ai().subject;
    if dedup_key.is_some_and(|k| k.starts_with("model:fill:")) {
        install_slot(ctx, &state.fill, "install_subject_model", "build นี้ยังไม่มีตัวเติมฉากหลัง")
    } else {
        install_slot(ctx, &state.segment, "install_subject_model", &state.unsupported_reason)
    }
}

fn install_slot<T: ?Sized>(ctx: &JobCtx, slot: &Slot<T>, next: &'static str, unsupported: &str) -> Result<Outcome> {
    let model = slot.model.as_ref().ok_or_else(|| AppError::new("MODEL_NOT_READY", unsupported.to_string()))?;
    if slot.installed() {
        return Ok(Outcome::Done);
    }
    std::fs::create_dir_all(&slot.dir)?;
    let part = slot.dir.join("download.part");
    ctx.stage("download")?;
    if let Some(stopped) =
        crate::ai::download(ctx, &model.download_url, &part, model.download_bytes.div_ceil(1_000_000) as i64)?
    {
        return Ok(stopped);
    }
    ctx.stage("verify")?;
    if crate::ai::sha256_file(&part)? != model.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(AppError::new(
            "MODEL_NOT_READY",
            format!("ไฟล์{}ที่ดาวน์โหลดมาไม่ตรงกับรุ่นที่แอปรองรับ (checksum ไม่ตรง) กรุณาดาวน์โหลดใหม่", slot.name),
        )
        .retryable()
        .next(next));
    }
    std::fs::rename(&part, slot.dir.join(model.file))?;
    std::fs::write(slot.marker(), model.sha256)?;
    *lock(&slot.loaded) = None;
    *lock(&slot.load_error) = None;
    Ok(Outcome::Done)
}

// ───────────────────────── ภาพน้ำหนักของรูป ─────────────────────────

/// ที่เก็บภาพน้ำหนักของ revision (ผูกกับ revision: ไฟล์เปลี่ยน = คิดใหม่)
pub fn map_path(cache_dir: &Path, revision_id: &str) -> PathBuf {
    let shard = revision_id.get(revision_id.len().saturating_sub(2)..).unwrap_or("00");
    cache_dir.join("subject").join(shard).join(format!("{revision_id}.png"))
}

fn revision_of(app: &App, photo_id: &str) -> Result<(String, PathBuf)> {
    let conn = app.db();
    let revision: String = conn
        .query_row("SELECT current_revision_id FROM photos WHERE id = ?1", [photo_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("รูป"))?;
    Ok((revision, PathBuf::from(crate::catalog::photo_path(&conn, photo_id)?)))
}

/// ภาพน้ำหนักที่คิดไว้แล้วของ revision (ในหน่วยความจำ หรือใน cache) — None = ยังไม่เคยคิด
pub(crate) fn cached_map(state: &SubjectState, cache_dir: &Path, revision_id: &str) -> Option<SubjectMap> {
    if let Some(map) = state.recall(revision_id) {
        return Some(map);
    }
    let bytes = std::fs::read(map_path(cache_dir, revision_id)).ok()?;
    let map = SubjectMap::new(image::load_from_memory(&bytes).ok()?.into_luma8());
    state.remember(revision_id, &map);
    Some(map)
}

/// ภาพน้ำหนักของ revision ที่ไฟล์อยู่ที่ `source`: ที่คิดไว้แล้ว หรือคิดตอนนี้ (ราว 7 วินาที)
/// — ยังไม่ได้ติดตั้งตัวแยก = MODEL_NOT_READY
pub(crate) fn compute_map(
    state: &SubjectState,
    cache_dir: &Path,
    revision_id: &str,
    source: &Path,
) -> Result<SubjectMap> {
    if let Some(map) = cached_map(state, cache_dir, revision_id) {
        return Ok(map);
    }
    let segmenter = state.segmenter().ok_or_else(|| {
        let message = if state.model().is_none() {
            state.unsupported_reason.clone()
        } else {
            "รูปนี้มี Mask ตัวคน ต้องติดตั้งตัวแยกตัวคนก่อน".to_string()
        };
        AppError::new("MODEL_NOT_READY", message).next("install_subject_model")
    })?;
    let _turn = lock(&state.busy);
    // รูปเดียวกันอาจถูกคิดเสร็จระหว่างรอคิว
    if let Some(map) = cached_map(state, cache_dir, revision_id) {
        return Ok(map);
    }
    let bytes = media::read_file(source).map_err(|_| AppError::source_unavailable())?;
    let img = media::decode_upright(&bytes, MAP_EDGE)?;
    drop(bytes);
    let img = if img.width().max(img.height()) > MAP_EDGE { img.thumbnail(MAP_EDGE, MAP_EDGE) } else { img };
    let image = segmenter.segment(&img.into_rgb8())?;
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(image.clone())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(AppError::internal)?;
    // cache สร้างใหม่ได้: เขียนไม่สำเร็จก็ยังใช้ภาพนี้ต่อได้
    let _ = media::write_cache_file(&map_path(cache_dir, revision_id), &png);
    let map = SubjectMap::new(image);
    state.remember(revision_id, &map);
    Ok(map)
}

// ───────────────────────── ฉากหลังที่เติมไว้ (ตัวผอม — DECISIONS ข้อ 41) ─────────────────────────

/// ที่เก็บฉากหลังที่เติมแล้ว: ตาม revision และภาพตั้งต้นของการแต่ง (ไฟล์ของรูป หรือภาพจาก RAW คู่ — สีต้องเป็นของภาพที่แต่ง)
pub fn fill_path(cache_dir: &Path, revision_id: &str, from_raw: bool) -> PathBuf {
    map_path(cache_dir, revision_id)
        .with_file_name(format!("{revision_id}-fill{}.png", if from_raw { "-raw" } else { "" }))
}

fn fill_key(revision_id: &str, from_raw: bool) -> String {
    format!("{revision_id}:{from_raw}")
}

/// ฉากหลังที่เติมไว้แล้ว (ในหน่วยความจำ หรือใน cache) — None = ยังไม่เคยเติม
pub(crate) fn cached_fill(state: &SubjectState, cache_dir: &Path, revision_id: &str, from_raw: bool) -> Option<Fill> {
    let key = fill_key(revision_id, from_raw);
    if let Some(fill) = lock(&state.fills).iter().find(|(k, _)| *k == key).map(|(_, f)| f.clone()) {
        return Some(fill);
    }
    let bytes = std::fs::read(fill_path(cache_dir, revision_id, from_raw)).ok()?;
    let fill = Fill::new(image::load_from_memory(&bytes).ok()?.into_rgba8());
    remember_fill(state, key, &fill);
    Some(fill)
}

fn remember_fill(state: &SubjectState, key: String, fill: &Fill) {
    let mut fills = lock(&state.fills);
    fills.retain(|(k, _)| *k != key);
    fills.insert(0, (key, fill.clone()));
    fills.truncate(RECENT);
}

/// ฉากหลังที่เติมแล้วของ revision: ที่เติมไว้แล้ว หรือเติมตอนนี้ (ราว 2 วินาที) · `base` = ภาพตั้งต้นของการแต่ง
pub(crate) fn compute_fill(
    state: &SubjectState,
    cache_dir: &Path,
    revision_id: &str,
    from_raw: bool,
    map: &SubjectMap,
    base: impl FnOnce() -> Result<RgbImage>,
) -> Result<Fill> {
    if let Some(fill) = cached_fill(state, cache_dir, revision_id, from_raw) {
        return Ok(fill);
    }
    let inpainter = state.inpainter().ok_or_else(|| {
        let message = if state.fill.model.is_none() {
            "build นี้ยังไม่มีตัวเติมฉากหลัง ตัวผอมจึงยังใช้ไม่ได้".to_string()
        } else {
            "รูปนี้ใช้ตัวผอม ต้องติดตั้งตัวเติมฉากหลังก่อน".to_string()
        };
        AppError::new("MODEL_NOT_READY", message).next("install_subject_model")
    })?;
    let _turn = lock(&state.busy);
    if let Some(fill) = cached_fill(state, cache_dir, revision_id, from_raw) {
        return Ok(fill);
    }
    let fill = if map.bounds().is_none() {
        Fill::empty()
    } else {
        crate::body::fill(&base()?, map.image(), inpainter.as_ref())?
    };
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(fill.image().clone())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(AppError::internal)?;
    let _ = media::write_cache_file(&fill_path(cache_dir, revision_id, from_raw), &png);
    remember_fill(state, fill_key(revision_id, from_raw), &fill);
    Ok(fill)
}

/// ภาพตั้งต้นของการแต่ง ด้านยาวไม่เกิน [`MAP_EDGE`]: ไฟล์ของรูป (JPEG หรือโทนตั้งต้นของ RAW) หรือภาพจาก RAW คู่ในกรอบของ JPEG
/// — ภาพเดียวกับที่ตัวอย่างสดและภาพที่บันทึกใช้ ฉากหลังที่เติมจึงสีตรงกัน
pub(crate) fn base_image(
    cache_dir: &Path,
    source: &Path,
    raw_frame: Option<(&crate::pair::RawBase, &str)>,
    edge: u32,
) -> Result<RgbImage> {
    let bytes = match raw_frame {
        Some((base, revision_id)) => crate::assets::raw_frame_bytes(cache_dir, base, revision_id)?,
        None => media::read_file(source).map_err(|_| AppError::source_unavailable())?,
    };
    let img = media::decode_upright(&bytes, edge)?;
    let img = if img.width().max(img.height()) > edge { img.thumbnail(edge, edge) } else { img };
    Ok(img.into_rgb8())
}

/// ตัวโหลดภาพตั้งต้นของการแต่งของรูป (ด้านยาวตามที่ขอ) — ไฟล์ของรูป หรือภาพจาก RAW คู่
fn app_base<'a>(
    app: &'a App,
    photo_id: &'a str,
    source: &'a Path,
    from_raw: bool,
) -> impl Fn(u32) -> Result<RgbImage> + 'a {
    move |edge| {
        if from_raw {
            crate::pair::ensure_start(app, photo_id)?;
            let (base, rev) = crate::assets::raw_frame_base(&app.db(), photo_id)?;
            base_image(&app.cache_dir, source, Some((&base, &rev)), edge)
        } else {
            base_image(&app.cache_dir, source, None, edge)
        }
    }
}

// ───────────────────────── ลบคน/สิ่งของ (DECISIONS ข้อ 42) ─────────────────────────

fn removal_name(revision_id: &str, from_raw: bool, r: &crate::remove::Removal) -> String {
    use sha2::Digest;
    let rect = r.rect;
    let mut key = format!("{:.6},{:.6},{:.6},{:.6}", rect[0], rect[1], rect[2], rect[3]);
    if let Some([x, y]) = r.anchor {
        key.push_str(&format!("@{x:.6},{y:.6}"));
    }
    for [x, y, radius] in &r.brush {
        key.push_str(&format!("~{x:.5},{y:.5},{radius:.5}"));
    }
    let hash = util::hex(&sha2::Sha256::digest(key.as_bytes()));
    format!("{revision_id}-rm-{}{}", &hash[..16], if from_raw { "-raw" } else { "" })
}

/// ที่เก็บภาพที่เติมของกรอบหนึ่ง (`.png`) และส่วนที่ครอบ (`.json`)
pub fn removal_path(cache_dir: &Path, revision_id: &str, from_raw: bool, r: &crate::remove::Removal) -> PathBuf {
    map_path(cache_dir, revision_id).with_file_name(format!("{}.png", removal_name(revision_id, from_raw, r)))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PatchInfo {
    region: [f64; 4],
    found: bool,
}

/// ภาพที่เติมของกรอบที่คิดไว้แล้ว — None = ยังไม่เคยคิด
pub(crate) fn cached_removal(
    state: &SubjectState,
    cache_dir: &Path,
    revision_id: &str,
    from_raw: bool,
    r: &crate::remove::Removal,
) -> Option<(crate::remove::Patch, bool)> {
    let key = removal_name(revision_id, from_raw, r);
    if let Some((_, p, f)) = lock(&state.patches).iter().find(|(k, _, _)| *k == key) {
        return Some((p.clone(), *f));
    }
    let path = removal_path(cache_dir, revision_id, from_raw, r);
    let info: PatchInfo = serde_json::from_slice(&std::fs::read(path.with_extension("json")).ok()?).ok()?;
    let image = image::load_from_memory(&std::fs::read(&path).ok()?).ok()?.into_rgba8();
    let patch = crate::remove::Patch::new(info.region, image);
    remember_patch(state, key, &patch, info.found);
    Some((patch, info.found))
}

fn remember_patch(state: &SubjectState, key: String, patch: &crate::remove::Patch, found: bool) {
    let mut patches = lock(&state.patches);
    patches.retain(|(k, _, _)| *k != key);
    patches.insert(0, (key, patch.clone(), found));
    patches.truncate(4 * RECENT);
}

/// ภาพที่เติมของทุกกรอบ: ที่คิดไว้แล้ว หรือคิดตอนนี้ (ราว 3–8 วินาทีต่อกรอบ) · `base` = ภาพตั้งต้นด้านยาว [`crate::remove::REMOVE_EDGE`]
pub(crate) fn compute_removals(
    state: &SubjectState,
    cache_dir: &Path,
    revision_id: &str,
    from_raw: bool,
    removals: &[crate::remove::Removal],
    base: impl FnOnce() -> Result<RgbImage>,
) -> Result<Vec<(crate::remove::Patch, bool)>> {
    let cached = |r: &crate::remove::Removal| cached_removal(state, cache_dir, revision_id, from_raw, r);
    if let Some(all) = removals.iter().map(cached).collect::<Option<Vec<_>>>() {
        return Ok(all);
    }
    let (Some(segmenter), Some(inpainter)) = (state.segmenter(), state.inpainter()) else {
        let message = if state.model().is_none() || state.fill.model.is_none() {
            "build นี้ยังไม่มีตัวแยกตัวคนหรือตัวเติมฉากหลัง จึงยังลบคน/สิ่งของไม่ได้".to_string()
        } else {
            "ลบคน/สิ่งของต้องติดตั้งตัวแยกตัวคนและตัวเติมฉากหลังก่อน".to_string()
        };
        return Err(AppError::new("MODEL_NOT_READY", message).next("install_subject_model"));
    };
    let _turn = lock(&state.busy);
    let mut base = Some(base);
    let mut image: Option<RgbImage> = None;
    let mut out = Vec::with_capacity(removals.len());
    for r in removals {
        if let Some(done) = cached(r) {
            out.push(done);
            continue;
        }
        if image.is_none() {
            image = Some((base.take().expect("โหลดครั้งเดียว"))()?);
        }
        let image = image.as_ref().expect("โหลดแล้ว");
        let (patch, found) = crate::remove::compute(image, r, segmenter.as_ref(), inpainter.as_ref())?;
        let path = removal_path(cache_dir, revision_id, from_raw, r);
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(patch.image().clone())
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(AppError::internal)?;
        // cache สร้างใหม่ได้: เขียนไม่สำเร็จก็ยังใช้ภาพนี้ต่อได้ · เขียนส่วนที่ครอบก่อน (ไม่มีไฟล์ภาพ = ยังไม่เคยคิด)
        let info = serde_json::to_vec(&PatchInfo { region: patch.region(), found })?;
        if media::write_cache_file(&path.with_extension("json"), &info).is_ok() {
            let _ = media::write_cache_file(&path, &png);
        }
        remember_patch(state, removal_name(revision_id, from_raw, r), &patch, found);
        out.push((patch, found));
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    /// ต่อกรอบ: พบคน/สิ่งที่เด่นในกรอบ (ไม่พบ = ลบทั้งกรอบ)
    pub found: Vec<bool>,
}

/// คิดภาพที่เติมของกรอบที่จะลบไว้ก่อน (หน้าแต่งภาพเรียกตอนเพิ่มกรอบ) — จากภาพตั้งต้นของการแต่ง (`from_raw` = ภาพจาก RAW คู่)
pub fn prepare_removals(
    app: &App,
    photo_id: &str,
    from_raw: bool,
    removals: &[crate::remove::Removal],
) -> Result<Removed> {
    crate::remove::validate(removals)?;
    let (revision_id, source) = revision_of(app, photo_id)?;
    let base = app_base(app, photo_id, &source, from_raw);
    let done = compute_removals(&app.ai.subject, &app.cache_dir, &revision_id, from_raw, removals, || {
        base(crate::remove::REMOVE_EDGE)
    })?;
    Ok(Removed { found: done.into_iter().map(|(_, f)| f).collect() })
}

/// ฉากหลังที่เติมแล้วของรูป: ที่เติมไว้แล้ว หรือเติมตอนนี้ (คิดภาพน้ำหนักก่อนถ้ายังไม่มี)
pub fn ensure_fill(app: &App, photo_id: &str, from_raw: bool) -> Result<(SubjectMap, Fill)> {
    let map = ensure(app, photo_id)?;
    let (revision_id, source) = revision_of(app, photo_id)?;
    let base = app_base(app, photo_id, &source, from_raw);
    let fill = compute_fill(&app.ai.subject, &app.cache_dir, &revision_id, from_raw, &map, || base(MAP_EDGE))?;
    Ok((map, fill))
}

/// ภาพน้ำหนักที่คิดไว้แล้วของรูป (ไม่คิดใหม่) — None = ยังไม่เคยคิด
pub fn cached(app: &App, photo_id: &str) -> Result<Option<SubjectMap>> {
    let (revision_id, _) = revision_of(app, photo_id)?;
    Ok(cached_map(&app.ai.subject, &app.cache_dir, &revision_id))
}

/// ภาพน้ำหนักของรูป: ที่คิดไว้แล้ว หรือคิดตอนนี้
pub fn ensure(app: &App, photo_id: &str) -> Result<SubjectMap> {
    let (revision_id, source) = revision_of(app, photo_id)?;
    compute_map(&app.ai.subject, &app.cache_dir, &revision_id, &source)
}

/// แนบภาพน้ำหนักให้ Mask ตัวคนทุกอันในค่าที่แต่ง
pub fn attach(settings: &mut EditSettings, map: &SubjectMap) {
    for mask in &mut settings.masks {
        if let Area::Subject { map: slot } = &mut mask.area {
            *slot = Some(map.clone());
        }
    }
}

/// แนบสิ่งที่ตัวผอมใช้ (ภาพน้ำหนัก + ฉากหลังที่เติม) ให้รีทัชในค่าที่แต่ง
pub fn attach_body(settings: &mut EditSettings, map: &SubjectMap, fill: &Fill) {
    if let Some(r) = settings.retouch.as_mut() {
        r.plan = crate::body::Attached(Some(crate::body::BodyPlan::new(map.clone(), fill.clone(), &r.faces)));
    }
}

/// แนบทุกอย่างที่ค่าที่แต่งต้องใช้: ภาพที่เติมของส่วนที่ลบ (ลบคน/สิ่งของ) · ภาพน้ำหนัก (Mask ตัวคน ตัวผอม — ไม่นับคนที่ลบไปแล้ว) ·
/// ฉากหลังที่เติม (ตัวผอม) · `base` = ภาพตั้งต้นของการแต่งด้านยาวตามที่ขอ · `compute` = คิดส่วนที่ยังไม่มี (คิดไม่ได้ = Err)
/// ไม่คิด = ใช้เท่าที่มี · คืน false เมื่อยังขาดบางส่วน (ภาพที่ได้ยังไม่มีผลของส่วนนั้น — อย่าเก็บลง cache)
pub(crate) fn attach_all(
    state: &SubjectState,
    cache_dir: &Path,
    revision_id: &str,
    source: &Path,
    base: &dyn Fn(u32) -> Result<RgbImage>,
    settings: &mut EditSettings,
    compute: bool,
) -> Result<bool> {
    let from_raw = settings.from_raw;
    let mut complete = true;
    if !settings.removals.is_empty() {
        let patches: Vec<Option<crate::remove::Patch>> = if compute {
            compute_removals(state, cache_dir, revision_id, from_raw, &settings.removals, || {
                base(crate::remove::REMOVE_EDGE)
            })?
            .into_iter()
            .map(|(p, _)| Some(p))
            .collect()
        } else {
            let cached = |r| cached_removal(state, cache_dir, revision_id, from_raw, r).map(|(p, _)| p);
            settings.removals.iter().map(cached).collect()
        };
        for (r, p) in settings.removals.iter_mut().zip(patches) {
            complete &= p.is_some();
            r.patch = crate::remove::Attached(p);
        }
    }
    let body = settings.has_body();
    if settings.has_subject() || body {
        let map = if compute {
            Some(compute_map(state, cache_dir, revision_id, source)?)
        } else {
            cached_map(state, cache_dir, revision_id)
        };
        let Some(map) = map else { return Ok(false) };
        // คนที่ลบไปแล้วไม่อยู่ในภาพน้ำหนัก (ไม่งั้นเห็นเงาของคนที่ลบสว่าง/หดตามขึ้นมา)
        let people = if crate::remove::active(&settings.removals) {
            crate::remove::clear_map(&map, &settings.removals)
        } else {
            map.clone()
        };
        attach(settings, &people);
        if body {
            let fill = if compute {
                Some(compute_fill(state, cache_dir, revision_id, from_raw, &map, || base(MAP_EDGE))?)
            } else {
                cached_fill(state, cache_dir, revision_id, from_raw)
            };
            match fill {
                Some(fill) => attach_body(settings, &people, &fill),
                None => complete = false,
            }
        }
    }
    Ok(complete)
}

/// เตรียมค่าที่แต่งของรูปให้พร้อมสร้างภาพ ([`attach_all`]) · `compute` = คิดใหม่ได้ถ้ายังไม่เคยคิด ·
/// คืน false เมื่อยังขาดบางส่วน (ภาพที่ได้ยังไม่มีผลของส่วนนั้น — อย่าเก็บลง cache)
pub(crate) fn prepare(app: &App, photo_id: &str, settings: &mut EditSettings, compute: bool) -> bool {
    if !settings.has_subject() && !settings.has_body() && settings.removals.is_empty() {
        return true;
    }
    let Ok((revision_id, source)) = revision_of(app, photo_id) else { return false };
    let base = app_base(app, photo_id, &source, settings.from_raw);
    attach_all(&app.ai.subject, &app.cache_dir, &revision_id, &source, &base, settings, compute).unwrap_or(false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    /// พบตัวคนในรูป
    pub found: bool,
}

/// คิดภาพน้ำหนักของรูปไว้ก่อน (หน้าแต่งภาพเรียกตอนเพิ่ม Mask ตัวคน หรือเปิดรูปที่มี Mask นี้) · `fill` = เติมฉากหลังของตัวผอมด้วย
/// จากภาพตั้งต้นของการแต่ง (`from_raw` = ภาพจาก RAW คู่)
pub fn prepare_photo(app: &App, photo_id: &str, fill: bool, from_raw: bool) -> Result<Prepared> {
    let map = if fill { ensure_fill(app, photo_id, from_raw)?.0 } else { ensure(app, photo_id)? };
    Ok(Prepared { found: map.bounds().is_some() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_is_bilinear_and_zero_outside_the_photo() {
        // ครึ่งขวาเป็นตัวคน
        let map = SubjectMap::new(GrayImage::from_fn(4, 2, |x, _| image::Luma([if x >= 2 { 255 } else { 0 }])));
        assert_eq!(map.weight(0.9, 0.5), 1.0);
        assert_eq!(map.weight(0.1, 0.5), 0.0);
        assert!((map.weight(0.5, 0.5) - 0.5).abs() < 1e-9, "รอยต่อผสมครึ่งหนึ่ง");
        assert_eq!(map.weight(1.2, 0.5), 0.0);
        assert_eq!(map.bounds(), Some([0.5, 0.0, 1.0, 1.0]));
        assert_eq!(SubjectMap::new(GrayImage::new(3, 3)).bounds(), None, "ไม่พบตัวคน");
    }

    #[test]
    fn attaching_a_map_does_not_change_what_the_settings_are() {
        let json = r#"{"masks":[{"id":"s","area":{"kind":"subject"},"adjust":{"texture":-35}}]}"#;
        let plain: EditSettings = serde_json::from_str(json).unwrap();
        assert!(plain.has_subject());
        let mut with_map = plain.clone();
        attach(&mut with_map, &SubjectMap::new(GrayImage::new(2, 2)));
        assert!(matches!(&with_map.masks[0].area, Area::Subject { map: Some(_) }));
        assert_eq!(plain, with_map, "ภาพน้ำหนักไม่ใช่ส่วนของค่าที่แต่ง");
        assert_eq!(serde_json::to_string(&with_map).unwrap(), serde_json::to_string(&plain).unwrap(), "ไม่ถูกเก็บ");
    }
}
