//! ตัววิเคราะห์ใบหน้า: interface, สถานะโมเดลในเครื่อง และการติดตั้งโมเดล (Blueprint หมวด 10)
//!
//! core ไม่รู้จัก ONNX Runtime: เปลือกแอปส่ง [`FaceEngineProvider`] เข้ามา (ชุดโมเดล + ตัวสร้าง analyzer)
//! ไฟล์โมเดลไม่อยู่ในตัวติดตั้ง แอปดาวน์โหลดจากต้นทางเมื่อผู้ใช้กดครั้งเดียว แล้วตรวจ SHA-256 ก่อนใช้

use crate::error::{AppError, Result};
use crate::faces::BBox;
use crate::jobs::{self, Control, JobCtx, JobSnapshot, Outcome, ACTIVE_STATES};
use crate::{db, util, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct DetectedFace {
    /// สัดส่วน [0,1] บนภาพที่หมุนตาม EXIF แล้ว
    pub bbox: BBox,
    pub score: f32,
    pub embedding: Vec<f32>,
}

pub trait FaceAnalyzer: Send + Sync {
    /// ตรวจใบหน้าทั้งหมดในไฟล์รูปและสร้างเวกเตอร์ของแต่ละใบ
    fn analyze(&self, image_bytes: &[u8]) -> Result<Vec<DetectedFace>>;
    /// เวกเตอร์ของใบหน้าในภาพครอปเล็ก (ใบหน้าตัวอย่างที่นำเข้า); None เมื่อใช้ภาพนี้ไม่ได้
    fn embed_crop(&self, image_bytes: &[u8]) -> Result<Option<Vec<f32>>>;
}

pub struct ModelFile {
    pub name: &'static str,
    pub sha256: &'static str,
}

/// คำอธิบายชุดโมเดลหนึ่งชุด (Blueprint 10.2): ตัวตน ที่มา เงื่อนไขการใช้ และ embedding space
pub struct ModelSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    /// เวกเตอร์ต่าง space ห้ามนำมาเทียบกัน
    pub space: &'static str,
    pub files: &'static [ModelFile],
    pub download_url: String,
    pub download_bytes: u64,
    pub download_source: &'static str,
    pub license_note: &'static str,
}

pub type AnalyzerFactory = Arc<dyn Fn(&Path) -> Result<Arc<dyn FaceAnalyzer>> + Send + Sync>;

pub struct FaceEngineProvider {
    pub spec: ModelSpec,
    pub factory: AnalyzerFactory,
}

pub struct AiState {
    provider: Option<FaceEngineProvider>,
    unsupported_reason: String,
    pub model_dir: PathBuf,
    loaded: Mutex<Option<Arc<dyn FaceAnalyzer>>>,
    load_error: Mutex<Option<String>>,
}

impl AiState {
    pub fn new(provider: Option<FaceEngineProvider>, unsupported_reason: &str, models_root: &Path) -> Self {
        let model_dir = match &provider {
            Some(p) => models_root.join(p.spec.id),
            None => models_root.to_path_buf(),
        };
        Self {
            provider,
            unsupported_reason: unsupported_reason.to_string(),
            model_dir,
            loaded: Mutex::new(None),
            load_error: Mutex::new(None),
        }
    }

    pub fn spec(&self) -> Option<&ModelSpec> {
        self.provider.as_ref().map(|p| &p.spec)
    }

    pub fn space(&self) -> Option<&'static str> {
        self.spec().map(|s| s.space)
    }

    fn marker(&self) -> PathBuf {
        self.model_dir.join("verified")
    }

    /// ไฟล์โมเดลครบและผ่านการตรวจ SHA-256 แล้ว
    pub fn installed(&self) -> bool {
        self.spec()
            .is_some_and(|s| self.marker().is_file() && s.files.iter().all(|f| self.model_dir.join(f.name).is_file()))
    }

    /// ตรวจ SHA-256 ของไฟล์โมเดลทุกไฟล์กับค่าที่กำหนดไว้ แล้วทำเครื่องหมายว่าใช้ได้
    pub fn verify_and_mark(&self) -> Result<()> {
        let spec = self.spec().ok_or_else(|| AppError::new("MODEL_NOT_READY", self.unsupported_reason.clone()))?;
        for file in spec.files {
            let path = self.model_dir.join(file.name);
            let mut reader = std::fs::File::open(&path).map_err(|_| {
                AppError::new("MODEL_NOT_READY", format!("ไม่พบไฟล์โมเดล {}", file.name)).next("install_model")
            })?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            if util::hex(&hasher.finalize()) != file.sha256 {
                return Err(AppError::new(
                    "MODEL_NOT_READY",
                    format!("ไฟล์โมเดล {} ไม่ตรงกับรุ่นที่แอปรองรับ (checksum ไม่ตรง) กรุณาดาวน์โหลดใหม่", file.name),
                )
                .next("install_model"));
            }
        }
        std::fs::write(self.marker(), spec.space)?;
        self.reset();
        Ok(())
    }

    pub fn reset(&self) {
        *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.load_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// analyzer ที่พร้อมใช้ (โหลดโมเดลครั้งแรกที่ถูกขอ); None เมื่อยังไม่ติดตั้งหรือโหลดไม่ได้
    pub fn analyzer(&self) -> Option<Arc<dyn FaceAnalyzer>> {
        let provider = self.provider.as_ref()?;
        if !self.installed() {
            return None;
        }
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if loaded.is_none() && self.load_error.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            match (provider.factory)(&self.model_dir) {
                Ok(a) => *loaded = Some(a),
                Err(e) => *self.load_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.message),
            }
        }
        loaded.clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceAnalysisStatus {
    pub available: bool,
    pub state: &'static str,
    pub reason: String,
    pub model_name: String,
    pub download_size_mb: u64,
    pub download_source: String,
    pub license_note: String,
    pub install_job_id: Option<String>,
    pub model_path: String,
}

pub fn status(app: &App) -> Result<FaceAnalysisStatus> {
    let ai = &app.ai;
    let mut s = FaceAnalysisStatus {
        available: false,
        state: "unsupported",
        reason: ai.unsupported_reason.clone(),
        model_name: String::new(),
        download_size_mb: 0,
        download_source: String::new(),
        license_note: String::new(),
        install_job_id: None,
        model_path: ai.model_dir.to_string_lossy().into_owned(),
    };
    let Some(spec) = ai.spec() else { return Ok(s) };
    s.model_name = spec.display_name.to_string();
    s.download_size_mb = spec.download_bytes.div_ceil(1_000_000);
    s.download_source = spec.download_source.to_string();
    s.license_note = spec.license_note.to_string();
    if ai.installed() {
        if ai.analyzer().is_some() {
            s.available = true;
            s.state = "ready";
            s.reason = "พร้อมวิเคราะห์ใบหน้าในเครื่องนี้".into();
        } else {
            let detail = ai.load_error.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default();
            s.reason = format!("โหลดตัววิเคราะห์ใบหน้าไม่สำเร็จ ({detail})");
        }
        return Ok(s);
    }
    s.install_job_id = app
        .db()
        .query_row(&format!("SELECT id FROM jobs WHERE kind = 'model' AND state IN {ACTIVE_STATES} LIMIT 1"), [], |r| {
            r.get(0)
        })
        .optional()?;
    if s.install_job_id.is_some() {
        s.state = "installing";
        s.reason = "กำลังดาวน์โหลดตัววิเคราะห์ใบหน้า เพิ่มและดูรูปได้ระหว่างรอ".into();
    } else {
        s.state = "not_installed";
        s.reason = "ดาวน์โหลดตัววิเคราะห์ใบหน้าครั้งเดียว แล้วแอปจะหาใบหน้าในรูปให้เอง".into();
    }
    Ok(s)
}

/// เริ่มงานดาวน์โหลดโมเดล (หรือคืนงานเดิมที่ยังทำอยู่)
pub fn install(app: &App) -> Result<JobSnapshot> {
    let spec = app.ai.spec().ok_or_else(|| AppError::new("MODEL_NOT_READY", app.ai.unsupported_reason.clone()))?;
    let id = {
        let conn = app.db();
        let existing: Option<String> = conn
            .query_row(
                &format!("SELECT id FROM jobs WHERE kind = 'model' AND state IN {ACTIVE_STATES} LIMIT 1"),
                [],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            Some(id) => {
                // งานที่พัก/ติดอยู่ให้เดินต่อเมื่อผู้ใช้กดติดตั้งอีกครั้ง
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
                    params![id, format!("ดาวน์โหลดตัววิเคราะห์ใบหน้า ({})", spec.display_name), format!("model:{}", spec.id), spec.download_bytes.div_ceil(1_000_000) as i64, now],
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

/// งานติดตั้งโมเดล: ดาวน์โหลด (ทำต่อจากเดิมได้) → แตกเฉพาะไฟล์ที่ใช้ → ตรวจ checksum → วิเคราะห์รูปในคลังต่อ
/// ใช้ `curl` และ `unzip` ของระบบ (มีใน macOS ทุกเครื่อง) ส่ง argument ตรง ๆ ไม่ผ่าน shell
pub(crate) fn run_install(ctx: &JobCtx) -> Result<Outcome> {
    let ai = ctx.ai();
    let spec = ai.spec().ok_or_else(|| AppError::new("MODEL_NOT_READY", "build นี้ไม่มีตัววิเคราะห์ใบหน้า"))?;
    std::fs::create_dir_all(&ai.model_dir)?;
    let archive = ai.model_dir.join("download.part");
    let total_mb = spec.download_bytes.div_ceil(1_000_000) as i64;
    let size_mb = |p: &Path| std::fs::metadata(p).map(|m| (m.len() / 1_000_000) as i64).unwrap_or(0);

    if !ai.installed() {
        ctx.stage("download")?;
        let mut child = Command::new("curl")
            .args([
                "--location",
                "--fail",
                "--silent",
                "--show-error",
                "--continue-at",
                "-",
                "--retry",
                "3",
                "--output",
            ])
            .arg(&archive)
            .arg(&spec.download_url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::internal(format!("เรียกตัวดาวน์โหลดของระบบไม่ได้: {e}")))?;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(AppError::internal)? {
                break status;
            }
            let c = ctx.control()?;
            if c != Control::Continue {
                let _ = child.kill();
                let _ = child.wait();
                // ไฟล์ที่ได้มาแล้วเก็บไว้ให้ดาวน์โหลดต่อ ยกเว้นผู้ใช้สั่งยกเลิก
                if c == Control::Cancel {
                    let _ = std::fs::remove_file(&archive);
                }
                return Ok(JobCtx::outcome_for(c));
            }
            ctx.progress(total_mb, size_mb(&archive).min(total_mb), 0, 0, true)?;
            std::thread::sleep(Duration::from_millis(400));
        };
        if !status.success() {
            let mut detail = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut detail);
            }
            return Ok(Outcome::Blocked(format!(
                "ดาวน์โหลดไม่สำเร็จ ตรวจการเชื่อมต่ออินเทอร์เน็ตแล้วสั่งทำต่อ ({})",
                detail.trim().chars().take(160).collect::<String>()
            )));
        }
        ctx.progress(total_mb, total_mb, 0, 0, true)?;

        ctx.stage("verify")?;
        let mut unzip = Command::new("unzip");
        unzip.args(["-o", "-j", "-q"]).arg(&archive);
        for file in spec.files {
            unzip.arg(file.name);
        }
        let out = unzip.arg("-d").arg(&ai.model_dir).output().map_err(AppError::internal)?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&archive);
            return Err(AppError::new("MODEL_NOT_READY", "ไฟล์ที่ดาวน์โหลดมาเสียหาย กรุณาลองดาวน์โหลดใหม่")
                .retryable()
                .next("install_model"));
        }
        if let Err(e) = ai.verify_and_mark() {
            let _ = std::fs::remove_file(&archive);
            return Err(e);
        }
        let _ = std::fs::remove_file(&archive);
    }
    // โมเดลพร้อมแล้ว: วิเคราะห์รูปที่มีอยู่ในคลังต่อเลย ผู้ใช้ไม่ต้องกดอะไรอีก
    enqueue_analysis(ctx.conn, None)?;
    Ok(Outcome::Done)
}

/// เข้าคิววิเคราะห์ใบหน้าของกิจกรรมที่ยังมีรูปค้าง (ทั้งคลัง หรือกิจกรรมเดียว); คืน job IDs
pub fn enqueue_analysis(conn: &Connection, event_id: Option<&str>) -> Result<Vec<String>> {
    let events: Vec<(String, String)> = conn
        .prepare(
            "SELECT e.id, e.title FROM events e WHERE e.catalog_state = 'active' AND (?1 IS NULL OR e.id = ?1) AND EXISTS (
               SELECT 1 FROM event_photos ep JOIN photos p ON p.id = ep.photo_id
               JOIN photo_revisions r ON r.id = p.current_revision_id
               WHERE ep.event_id = e.id AND r.face_status = 'not_analyzed')
             ORDER BY e.event_date IS NULL, e.event_date DESC, e.id",
        )?
        .query_map([event_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    events.iter().map(|(id, title)| jobs::enqueue_catalog(conn, id, title)).collect()
}

pub fn analyze(app: &App, event_id: Option<&str>) -> Result<Vec<String>> {
    if app.ai.analyzer().is_none() {
        return Err(AppError::new("MODEL_NOT_READY", "ยังไม่ได้ติดตั้งตัววิเคราะห์ใบหน้า").next("install_model"));
    }
    let ids = {
        let conn = app.db();
        let ids = enqueue_analysis(&conn, event_id)?;
        db::bump_revision(&conn)?;
        ids
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    Ok(ids)
}

pub fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn from_blob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

fn iou(a: &BBox, b: &BBox) -> f64 {
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    let (x2, y2) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// บันทึกผลวิเคราะห์ของ revision หนึ่ง: ใบหน้าที่ตรงกับกรอบที่ผู้ใช้วาดไว้แล้วได้เวกเตอร์เพิ่ม (ไม่สร้างกรอบซ้ำ
/// และไม่แตะคำยืนยัน) ที่เหลือเป็นใบหน้าใหม่แบบ origin = detector
pub(crate) fn save_analysis(conn: &Connection, revision_id: &str, space: &str, faces: &[DetectedFace]) -> Result<()> {
    let existing: Vec<(String, BBox)> = conn
        .prepare("SELECT id, bbox_x, bbox_y, bbox_w, bbox_h FROM faces WHERE photo_revision_id = ?1 AND status <> 'superseded'")?
        .query_map([revision_id], |r| Ok((r.get(0)?, BBox { x: r.get(1)?, y: r.get(2)?, w: r.get(3)?, h: r.get(4)? })))?
        .collect::<rusqlite::Result<_>>()?;
    let now = util::now();
    for face in faces {
        let b = face.bbox;
        if !(b.w > 0.0 && b.h > 0.0) {
            continue;
        }
        let face_id = match existing.iter().find(|(_, e)| iou(e, &b) >= 0.5) {
            Some((id, _)) => id.clone(),
            None => {
                let id = util::new_id();
                conn.execute(
                    "INSERT INTO faces(id, photo_revision_id, origin, bbox_x, bbox_y, bbox_w, bbox_h, score, created_at)
                     VALUES (?1, ?2, 'detector', ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![id, revision_id, b.x, b.y, b.w, b.h, face.score, now],
                )?;
                id
            }
        };
        conn.execute(
            "INSERT INTO face_embeddings(face_id, space, vector) VALUES (?1, ?2, ?3)
             ON CONFLICT(face_id) DO UPDATE SET space = excluded.space, vector = excluded.vector",
            params![face_id, space, to_blob(&face.embedding)],
        )?;
    }
    conn.execute("UPDATE photo_revisions SET face_status = 'done' WHERE id = ?1", [revision_id])?;
    db::bump_identity(conn)?;
    Ok(())
}
