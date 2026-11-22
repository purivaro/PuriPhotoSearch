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
        SubjectMap { image: Arc::new(image), bounds }
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

pub struct SubjectProvider {
    pub model: SubjectModel,
    pub factory: SegmenterFactory,
}

pub struct SubjectState {
    provider: Option<SubjectProvider>,
    unsupported_reason: String,
    pub model_dir: PathBuf,
    loaded: Mutex<Option<Arc<dyn Segmenter>>>,
    load_error: Mutex<Option<String>>,
    /// คิดทีละรูป
    busy: Mutex<()>,
    recent: Mutex<Vec<(String, SubjectMap)>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl SubjectState {
    pub fn new(provider: Option<SubjectProvider>, unsupported_reason: &str, models_root: &Path) -> SubjectState {
        let model_dir = match &provider {
            Some(p) => models_root.join(p.model.id),
            None => models_root.to_path_buf(),
        };
        SubjectState {
            provider,
            unsupported_reason: unsupported_reason.to_string(),
            model_dir,
            loaded: Mutex::new(None),
            load_error: Mutex::new(None),
            busy: Mutex::new(()),
            recent: Mutex::new(Vec::new()),
        }
    }

    pub fn model(&self) -> Option<&SubjectModel> {
        self.provider.as_ref().map(|p| &p.model)
    }

    fn marker(&self) -> PathBuf {
        self.model_dir.join("verified")
    }

    /// ไฟล์โมเดลอยู่ครบและผ่านการตรวจ SHA-256 แล้ว
    pub fn installed(&self) -> bool {
        self.model().is_some_and(|m| self.marker().is_file() && self.model_dir.join(m.file).is_file())
    }

    /// ตัวแยกที่พร้อมใช้ (โหลดโมเดลครั้งแรกที่ถูกขอ) — None เมื่อยังไม่ติดตั้งหรือโหลดไม่ได้
    pub fn segmenter(&self) -> Option<Arc<dyn Segmenter>> {
        let provider = self.provider.as_ref()?;
        if !self.installed() {
            return None;
        }
        let mut loaded = lock(&self.loaded);
        if loaded.is_none() && lock(&self.load_error).is_none() {
            match (provider.factory)(&self.model_dir) {
                Ok(s) => *loaded = Some(s),
                Err(e) => *lock(&self.load_error) = Some(e.message),
            }
        }
        loaded.clone()
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectStatus {
    /// unsupported | not_installed | installing | ready | failed
    pub state: &'static str,
    pub reason: String,
    pub model_name: String,
    pub download_size_mb: u64,
    pub download_source: String,
    pub license_note: String,
    pub install_job_id: Option<String>,
}

fn dedup_key(model: &SubjectModel) -> String {
    format!("model:subject:{}", model.id)
}

/// งานติดตั้งของตัวแยกตัวคน (คนละงานกับตัววิเคราะห์ใบหน้า — ชนิดงาน `model` เหมือนกัน แยกด้วย dedup_key)
pub(crate) fn is_install_job(dedup_key: Option<&str>) -> bool {
    dedup_key.is_some_and(|k| k.starts_with("model:subject:"))
}

pub fn status(app: &App) -> Result<SubjectStatus> {
    let state = &app.ai.subject;
    let mut s = SubjectStatus {
        state: "unsupported",
        reason: state.unsupported_reason.clone(),
        model_name: String::new(),
        download_size_mb: 0,
        download_source: String::new(),
        license_note: String::new(),
        install_job_id: None,
    };
    let Some(model) = state.model() else { return Ok(s) };
    s.model_name = model.display_name.to_string();
    s.download_size_mb = model.download_bytes.div_ceil(1_000_000);
    s.download_source = model.download_source.to_string();
    s.license_note = model.license_note.to_string();
    if state.installed() {
        if state.segmenter().is_some() {
            s.state = "ready";
            s.reason = "พร้อมหาตัวคนในรูปบนเครื่องนี้".into();
        } else {
            s.state = "failed";
            let detail = lock(&state.load_error).clone().unwrap_or_default();
            s.reason = format!("โหลดตัวแยกตัวคนไม่สำเร็จ ({detail})");
        }
        return Ok(s);
    }
    s.install_job_id = app
        .db()
        .query_row(
            &format!(
                "SELECT id FROM jobs WHERE kind = 'model' AND dedup_key = ?1 AND state IN {ACTIVE_STATES} LIMIT 1"
            ),
            [dedup_key(model)],
            |r| r.get(0),
        )
        .optional()?;
    if s.install_job_id.is_some() {
        s.state = "installing";
        s.reason = "กำลังดาวน์โหลดตัวแยกตัวคน แต่งภาพส่วนอื่นได้ระหว่างรอ".into();
    } else {
        s.state = "not_installed";
        s.reason = format!(
            "ดาวน์โหลดตัวแยกตัวคนครั้งเดียว ({} MB) แล้วแอปจะเลือกตัวคนในรูปให้เอง บนเครื่องนี้ ไม่ส่งรูปออกไปไหน",
            s.download_size_mb
        );
    }
    Ok(s)
}

/// เริ่มงานดาวน์โหลดตัวแยกตัวคน (หรือคืนงานเดิมที่ยังทำอยู่)
pub fn install(app: &App) -> Result<JobSnapshot> {
    let state = &app.ai.subject;
    let model = state.model().ok_or_else(|| AppError::new("MODEL_NOT_READY", state.unsupported_reason.clone()))?;
    let key = dedup_key(model);
    let id = {
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
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE jobs SET state = 'queued' WHERE id = ?1 AND state IN ('paused', 'blocked')",
                    [&id],
                )?;
                id
            }
            None => {
                let (id, now) = (util::new_id(), util::now());
                conn.execute(
                    "INSERT INTO jobs(id, kind, state, stage, title, dedup_key, total, total_known, created_at, updated_at)
                     VALUES (?1, 'model', 'queued', 'download', ?2, ?3, ?4, 1, ?5, ?5)",
                    params![
                        id,
                        format!("ดาวน์โหลดตัวแยกตัวคน ({})", model.display_name),
                        key,
                        model.download_bytes.div_ceil(1_000_000) as i64,
                        now
                    ],
                )?;
                db::bump_revision(&conn)?;
                id
            }
        }
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    jobs::get(&app.db(), &id)
}

/// งานติดตั้ง: ดาวน์โหลด (ทำต่อจากเดิมได้) → ตรวจ checksum → ย้ายเข้าที่
pub(crate) fn run_install(ctx: &JobCtx) -> Result<Outcome> {
    let state = &ctx.ai().subject;
    let model = state.model().ok_or_else(|| AppError::new("MODEL_NOT_READY", state.unsupported_reason.clone()))?;
    if state.installed() {
        return Ok(Outcome::Done);
    }
    std::fs::create_dir_all(&state.model_dir)?;
    let part = state.model_dir.join("download.part");
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
            "ไฟล์ตัวแยกตัวคนที่ดาวน์โหลดมาไม่ตรงกับรุ่นที่แอปรองรับ (checksum ไม่ตรง) กรุณาดาวน์โหลดใหม่",
        )
        .retryable()
        .next("install_subject_model"));
    }
    std::fs::rename(&part, state.model_dir.join(model.file))?;
    std::fs::write(state.marker(), model.sha256)?;
    *lock(&state.loaded) = None;
    *lock(&state.load_error) = None;
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

/// เตรียมค่าที่แต่งของรูปให้พร้อมสร้างภาพ: แนบภาพน้ำหนักถ้ามี Mask ตัวคน · `compute` = คิดใหม่ได้ถ้ายังไม่เคยคิด
/// คืน false เมื่อมี Mask ตัวคนแต่ยังไม่มีภาพน้ำหนัก (ภาพที่ได้ยังไม่มีผลของ Mask นั้น — อย่าเก็บลง cache)
pub(crate) fn prepare(app: &App, photo_id: &str, settings: &mut EditSettings, compute: bool) -> bool {
    if !settings.has_subject() {
        return true;
    }
    let map = if compute { ensure(app, photo_id).ok() } else { cached(app, photo_id).ok().flatten() };
    match map {
        Some(map) => {
            attach(settings, &map);
            true
        }
        None => false,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    /// พบตัวคนในรูป
    pub found: bool,
}

/// คิดภาพน้ำหนักของรูปไว้ก่อน (หน้าแต่งภาพเรียกตอนเพิ่ม Mask ตัวคน หรือเปิดรูปที่มี Mask นี้)
pub fn prepare_photo(app: &App, photo_id: &str) -> Result<Prepared> {
    let map = ensure(app, photo_id)?;
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
