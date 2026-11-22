//! ตรวจใบหน้าและสร้างเวกเตอร์ใบหน้าในเครื่องด้วย ONNX Runtime — implementation ของ `puri_core::ai::FaceAnalyzer`
//!
//! ขั้นตอนเดียวกับ insightface: ย่อภาพให้ด้านยาว ≤ 1280 → SCRFD ที่ 640×640 (วางชิดมุมซ้ายบน) →
//! จัดหน้า 112×112 ด้วย similarity transform จากจุดสำคัญ 5 จุด → เวกเตอร์ 512 มิติ (ไม่ normalize)
//! แอปใช้ชุด `buffalo_l` (SCRFD-10GF + ResNet50@WebFace600K); ชุด `buffalo_sc` ของรุ่น Python ใช้เฉพาะใน
//! `examples/legacy_compat` เพื่อพิสูจน์ว่า pipeline นี้ให้ผลตรงกับรุ่นเดิม
//!
//! ไฟล์โมเดลไม่อยู่ใน repo และไม่อยู่ในตัวติดตั้ง: แอปดาวน์โหลดจากต้นทางเมื่อผู้ใช้กด แล้วตรวจ SHA-256
//! (เงื่อนไขการใช้ของโมเดล: docs/DECISIONS.md ข้อ 9)
#![cfg_attr(all(target_os = "macos", target_arch = "x86_64"), allow(dead_code, unused_imports))]

use image::{imageops::FilterType, DynamicImage, RgbImage};
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use ort::{environment::Environment, session::Session, value::Tensor};
use std::io::Cursor;
use std::path::Path;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

/// ค่าเดียวกับรุ่น Python (`app/services/face_service.py`)
pub const MAX_IMAGE_DIM: u32 = 1280;
// ย่อไฟล์ต้นฉบับในที่เดิม (puri_core::shrink) ต้องไม่ต่ำกว่าภาพที่ตัววิเคราะห์ใช้
// ไม่งั้นการวิเคราะห์ใหม่ภายหลังจะได้ผลแย่กว่าที่ได้จากต้นฉบับ — เพิ่มค่านี้เมื่อไรต้องเพิ่ม MIN_EDGE ด้วย
const _: () = assert!(puri_core::shrink::MIN_EDGE >= MAX_IMAGE_DIM);
const DET_SIZE: u32 = 640;
const DET_THRESHOLD: f32 = 0.5;
const NMS_THRESHOLD: f32 = 0.4;
const STRIDES: [u32; 3] = [8, 16, 32];
const ANCHORS_PER_CELL: usize = 2;
pub const EMBEDDING_DIM: usize = 512;
const ALIGNED: u32 = 112;
/// ตำแหน่งมาตรฐานของ ตาซ้าย ตาขวา จมูก มุมปากซ้าย มุมปากขวา บนภาพ 112×112 (ArcFace)
const ARCFACE_DST: [[f32; 2]; 5] =
    [[38.2946, 51.6963], [73.5318, 51.5014], [56.0252, 71.7366], [41.5493, 92.3655], [70.7299, 92.2041]];

#[derive(Debug, Clone)]
pub struct Detection {
    /// `[x1, y1, x2, y2]` เป็นพิกเซลบนภาพที่ส่งเข้า `detect`
    pub bbox: [f32; 4],
    pub landmarks: [[f32; 2]; 5],
    pub score: f32,
}

/// environment ของ ONNX Runtime ที่ทุกโมเดลใช้ร่วมกัน — ort ยอมให้สร้างได้ครั้งเดียวต่อโปรเซส (สร้างใหม่ไม่ได้แม้ตัวเดิมถูกปล่อยไปแล้ว)
/// ตัววิเคราะห์ใบหน้า ตัวแยกตัวคน และตัวเติมฉากหลังจึงต้องใช้ตัวเดียวกัน ไม่งั้นตัวที่โหลดทีหลังโหลดไม่ได้
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
pub fn environment() -> ort::Result<Environment> {
    static SHARED: std::sync::Mutex<Option<Environment>> = std::sync::Mutex::new(None);
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(env) = shared.as_ref() {
        return Ok(env.clone());
    }
    let env = ort::init().with_name("puri").build()?;
    *shared = Some(env.clone());
    Ok(env)
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
pub struct FaceEngine {
    detector: Session,
    embedder: Session,
    _env: Environment,
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
impl FaceEngine {
    pub fn load(model_dir: &Path, detector: &str, embedder: &str, threads: usize) -> Result<Self> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        coreml::remove_stale_temp_files(&std::env::temp_dir());
        let env = environment()?;
        let open = |file: &str| -> Result<Session> {
            let path = model_dir.join(file);
            if !path.is_file() {
                return Err(format!("ไม่พบไฟล์โมเดล {}", path.display()).into());
            }
            let mut builder = Session::builder(&env)?.with_intra_threads(threads.max(1)).map_err(|e| e.to_string())?;
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                builder = coreml::configure(builder, model_dir, &path)?;
            }
            Ok(builder.commit_from_file(path)?)
        };
        Ok(Self { detector: open(detector)?, embedder: open(embedder)?, _env: env })
    }

    /// SCRFD: คืนใบหน้าที่คะแนน ≥ 0.5 หลัง NMS เรียงคะแนนมากไปน้อย
    pub fn detect(&mut self, image: &RgbImage) -> Result<Vec<Detection>> {
        let (w, h) = image.dimensions();
        // ย่อให้พอดี 640×640 โดยคงสัดส่วน แล้ววางชิดมุมซ้ายบน ส่วนที่เหลือเป็นศูนย์ (เหมือน insightface)
        let (new_w, new_h) = if h > w {
            ((DET_SIZE as f32 * w as f32 / h as f32) as u32, DET_SIZE)
        } else {
            (DET_SIZE, (DET_SIZE as f32 * h as f32 / w as f32) as u32)
        };
        let (new_w, new_h) = (new_w.max(1), new_h.max(1));
        let det_scale = new_h as f32 / h as f32;
        let resized = image::imageops::resize(image, new_w, new_h, FilterType::Triangle);

        let plane = (DET_SIZE * DET_SIZE) as usize;
        let mut input = vec![-127.5 / 128.0; 3 * plane];
        for (x, y, px) in resized.enumerate_pixels() {
            let i = (y * DET_SIZE + x) as usize;
            for c in 0..3 {
                input[c * plane + i] = (px[c] as f32 - 127.5) / 128.0;
            }
        }
        let tensor = Tensor::from_array(([1usize, 3, DET_SIZE as usize, DET_SIZE as usize], input))?;
        let outputs = self.detector.run(ort::inputs![tensor])?;

        let mut found: Vec<Detection> = Vec::new();
        for (level, stride) in STRIDES.iter().enumerate() {
            // ลำดับ output ของโมเดล: คะแนน ×3 ระดับ, กรอบ ×3, จุดสำคัญ ×3
            let (_, scores) = outputs[level].try_extract_tensor::<f32>()?;
            let (_, boxes) = outputs[level + 3].try_extract_tensor::<f32>()?;
            let (_, points) = outputs[level + 6].try_extract_tensor::<f32>()?;
            let cells = (DET_SIZE / stride) as usize;
            let s = *stride as f32;
            for (i, &score) in scores.iter().enumerate() {
                if score < DET_THRESHOLD {
                    continue;
                }
                let cell = i / ANCHORS_PER_CELL;
                let (cx, cy) = ((cell % cells) as f32 * s, (cell / cells) as f32 * s);
                let b = &boxes[i * 4..i * 4 + 4];
                let p = &points[i * 10..i * 10 + 10];
                let mut landmarks = [[0.0; 2]; 5];
                for (k, lm) in landmarks.iter_mut().enumerate() {
                    *lm = [(cx + p[k * 2] * s) / det_scale, (cy + p[k * 2 + 1] * s) / det_scale];
                }
                found.push(Detection {
                    bbox: [
                        (cx - b[0] * s) / det_scale,
                        (cy - b[1] * s) / det_scale,
                        (cx + b[2] * s) / det_scale,
                        (cy + b[3] * s) / det_scale,
                    ],
                    landmarks,
                    score,
                });
            }
        }
        found.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut kept: Vec<Detection> = Vec::new();
        for d in found {
            if kept.iter().all(|k| iou(&k.bbox, &d.bbox) <= NMS_THRESHOLD) {
                kept.push(d);
            }
        }
        Ok(kept)
    }

    /// เวกเตอร์ 512 มิติของใบหน้าหนึ่งใบ (ค่าดิบ ไม่ normalize เหมือนที่รุ่น Python เก็บ)
    pub fn embed(&mut self, image: &RgbImage, landmarks: &[[f32; 2]; 5]) -> Result<Vec<f32>> {
        let aligned = align(image, landmarks);
        let plane = (ALIGNED * ALIGNED) as usize;
        let mut input = vec![0.0f32; 3 * plane];
        for (x, y, px) in aligned.enumerate_pixels() {
            let i = (y * ALIGNED + x) as usize;
            for c in 0..3 {
                input[c * plane + i] = (px[c] as f32 - 127.5) / 127.5;
            }
        }
        let tensor = Tensor::from_array(([1usize, 3, ALIGNED as usize, ALIGNED as usize], input))?;
        let outputs = self.embedder.run(ort::inputs![tensor])?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
        if data.len() != EMBEDDING_DIM {
            return Err(format!("โมเดลคืนเวกเตอร์ {} มิติ ไม่ใช่ {EMBEDDING_DIM}", data.len()).into());
        }
        Ok(data.to_vec())
    }
}

/// CoreML บน Mac Apple Silicon: ONNX Runtime เปิดใช้เองและแปลงโมเดลเป็นของ CoreML ทุกครั้งที่โหลด (3–4 วินาที)
/// ไปวางใน `$TMPDIR` ครั้งละราว 365 MB ซึ่งถูกลบเฉพาะเมื่อโปรแกรมปิดแบบปกติ — แอปที่ปิดโดยไม่คืน session
/// เทสต์ และ dev build จึงทิ้งไว้ทุกครั้ง (เจอจริง 9.8 GB จนดิสก์เต็ม 21 พ.ย. 69)
///
/// แก้โดยบอก CoreML ให้เก็บโมเดลที่แปลงแล้วในโฟลเดอร์โมเดลของแอปและใช้ซ้ำ (โหลดเหลือราว 0.2 วินาที ผลเท่าเดิมทุกบิต)
/// และลบของเดิมใน `$TMPDIR` ที่โปรแกรมเจ้าของปิดไปแล้ว (ของแอปรุ่นก่อน เทสต์ และ dev build)
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod coreml {
    use ort::session::builder::{AutoDevicePolicy, SessionBuilder};
    use std::path::{Path, PathBuf};

    /// ตั้งให้ session ใช้ CoreML แบบเก็บโมเดลที่แปลงแล้วไว้ใช้ซ้ำ — เก็บไม่ได้ (โฟลเดอร์อ่านอย่างเดียว) ใช้ CPU แทน
    /// ดีกว่าปล่อยให้ ONNX Runtime แปลงลง `$TMPDIR` ทุกครั้ง
    pub fn configure(builder: SessionBuilder, model_dir: &Path, model: &Path) -> super::Result<SessionBuilder> {
        let configured = match cache_dir(model_dir, model) {
            Some(dir) => builder
                .with_execution_providers([ort::ep::CoreML::default().with_model_cache_dir(dir.display()).build()]),
            None => builder.with_auto_device(AutoDevicePolicy::PreferCPU),
        };
        Ok(configured.map_err(|e| e.to_string())?)
    }

    /// `coreml-cache/<ชื่อไฟล์>-<ขนาด>-<เวลาแก้ไข>` ในโฟลเดอร์โมเดล: ไฟล์โมเดลเปลี่ยนเมื่อไรได้โฟลเดอร์ใหม่ (ONNX Runtime
    /// ตรวจแค่โครงของโมเดล ไม่ตรวจค่าน้ำหนัก) และลบของรุ่นก่อนของไฟล์เดียวกันทิ้ง
    fn cache_dir(model_dir: &Path, model: &Path) -> Option<PathBuf> {
        let meta = std::fs::metadata(model).ok()?;
        let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        let name = model.file_name()?.to_string_lossy().into_owned();
        let root = model_dir.join("coreml-cache");
        let key = format!("{name}-{}-{modified}", meta.len());
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let other = entry.file_name().to_string_lossy().into_owned();
                if other != key && other.strip_prefix(&name).is_some_and(|rest| rest.starts_with('-')) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        let dir = root.join(key);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    /// ลบ `onnxruntime-<UUID>-<pid>-<เลข>.model.mlmodel` / `.mlmodelc` ใน `dir` ที่ PID ไม่ได้ทำงานแล้ว
    /// (ของโปรแกรมที่ยังทำงานอยู่ยังถูกใช้ — ไม่แตะ)
    pub fn remove_stale_temp_files(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let Some(pid) = owner_pid(&entry.file_name().to_string_lossy()) else { continue };
            if alive(pid) {
                continue;
            }
            let path = entry.path();
            let _ = match std::fs::symlink_metadata(&path) {
                Ok(m) if m.is_dir() => std::fs::remove_dir_all(&path),
                Ok(_) => std::fs::remove_file(&path),
                Err(_) => continue,
            };
        }
    }

    /// PID ในชื่อไฟล์ชั่วคราวของ CoreML · ชื่ออื่น = None
    fn owner_pid(name: &str) -> Option<i32> {
        let stem = name.strip_suffix(".model.mlmodel").or_else(|| name.strip_suffix(".model.mlmodelc"))?;
        let parts: Vec<&str> = stem.strip_prefix("onnxruntime-")?.split('-').collect();
        let hex = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
        let uuid = parts.len() == 7 && parts[..5].iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && hex(p));
        let pid: i32 = parts.get(5)?.parse().ok()?;
        (uuid && pid > 0 && hex(parts[6])).then_some(pid)
    }

    /// `kill(pid, 0)` ไม่ส่งสัญญาณจริง: สำเร็จ = ยังทำงาน · EPERM = ทำงานอยู่แต่เป็นของผู้ใช้อื่น
    fn alive(pid: i32) -> bool {
        let signalled = unsafe { libc::kill(pid, 0) } == 0;
        signalled || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_the_pid_only_from_coreml_temp_names() {
            let name = "onnxruntime-00F90307-CB02-48BC-BA7E-AAA899D6DCB4-8207-00001DFBBEEE368C";
            assert_eq!(owner_pid(&format!("{name}.model.mlmodel")), Some(8207));
            assert_eq!(owner_pid(&format!("{name}.model.mlmodelc")), Some(8207));
            assert_eq!(owner_pid(&format!("{name}.onnx")), None);
            assert_eq!(owner_pid("onnxruntime-notes-8207-1.model.mlmodel"), None);
            assert_eq!(owner_pid("photo-8207.model.mlmodel"), None);
        }

        #[test]
        fn removes_leftovers_of_finished_processes_only() {
            let dir = std::env::temp_dir().join(format!("puri-faces-coreml-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            // PID ของโปรแกรมที่จบไปแล้ว: process ลูกที่รอจนจบ
            let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
            let finished = child.id();
            child.wait().unwrap();
            let name = |pid: u32| format!("onnxruntime-00F90307-CB02-48BC-BA7E-AAA899D6DCB4-{pid}-00001DFBBEEE368C");
            let stale_file = dir.join(format!("{}.model.mlmodel", name(finished)));
            let stale_dir = dir.join(format!("{}.model.mlmodelc", name(finished)));
            let ours = dir.join(format!("{}.model.mlmodelc", name(std::process::id())));
            let other = dir.join("notes.txt");
            std::fs::write(&stale_file, b"x").unwrap();
            std::fs::create_dir_all(stale_dir.join("weights")).unwrap();
            std::fs::create_dir_all(&ours).unwrap();
            std::fs::write(&other, b"x").unwrap();
            remove_stale_temp_files(&dir);
            assert!(!stale_file.exists() && !stale_dir.exists(), "ของโปรแกรมที่จบแล้วถูกลบ");
            assert!(ours.exists() && other.exists(), "ของโปรแกรมที่ยังทำงาน และไฟล์อื่น ไม่ถูกแตะ");
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn keeps_one_cache_folder_per_model_file() {
            let dir = std::env::temp_dir().join(format!("puri-faces-cache-test-{}", std::process::id()));
            let model = dir.join("det.onnx");
            std::fs::create_dir_all(dir.join("coreml-cache/det.onnx-1-1")).unwrap();
            std::fs::create_dir_all(dir.join("coreml-cache/det.onnx.bak-1-1")).unwrap();
            std::fs::write(&model, b"model").unwrap();
            let cache = cache_dir(&dir, &model).unwrap();
            assert!(cache.is_dir() && cache.file_name().unwrap().to_string_lossy().starts_with("det.onnx-5-"));
            assert!(!dir.join("coreml-cache/det.onnx-1-1").exists(), "ของไฟล์โมเดลรุ่นก่อนถูกลบ");
            assert!(dir.join("coreml-cache/det.onnx.bak-1-1").exists(), "ของไฟล์อื่นไม่ถูกแตะ");
            assert_eq!(cache_dir(&dir, &model), Some(cache), "ไฟล์เดิม = โฟลเดอร์เดิม");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}

pub fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let (x1, y1, x2, y2) = (a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3]));
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area = |r: &[f32; 4]| (r[2] - r[0]).max(0.0) * (r[3] - r[1]).max(0.0);
    let union = area(a) + area(b) - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm(a) * norm(b) + 1e-10)
}

/// similarity transform (หมุน + ย่อขยาย + เลื่อน ไม่กลับด้าน) แบบ least squares ที่พาจุดสำคัญไปยังตำแหน่งมาตรฐาน
/// คืนเมทริกซ์ 2×3 `[[a, -b, tx], [b, a, ty]]`
fn similarity(src: &[[f32; 2]; 5], dst: &[[f32; 2]; 5]) -> [[f32; 3]; 2] {
    let mean = |p: &[[f32; 2]; 5]| {
        let (sx, sy) = p.iter().fold((0.0, 0.0), |(x, y), q| (x + q[0], y + q[1]));
        [sx / 5.0, sy / 5.0]
    };
    let (ms, md) = (mean(src), mean(dst));
    let (mut dot, mut cross, mut var) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..5 {
        let (px, py) = (src[i][0] - ms[0], src[i][1] - ms[1]);
        let (qx, qy) = (dst[i][0] - md[0], dst[i][1] - md[1]);
        dot += px * qx + py * qy;
        cross += px * qy - py * qx;
        var += px * px + py * py;
    }
    let (a, b) = (dot / var.max(1e-12), cross / var.max(1e-12));
    [[a, -b, md[0] - (a * ms[0] - b * ms[1])], [b, a, md[1] - (b * ms[0] + a * ms[1])]]
}

/// ครอปและจัดหน้าเป็น 112×112 ด้วย bilinear; นอกขอบภาพเป็นสีดำ (เหมือน `cv2.warpAffine(..., borderValue=0)`)
fn align(image: &RgbImage, landmarks: &[[f32; 2]; 5]) -> RgbImage {
    let m = similarity(landmarks, &ARCFACE_DST);
    // ผกผันของ [[a, -b], [b, a]] คือ 1/(a²+b²) · [[a, b], [-b, a]]
    let (a, b) = (m[0][0], m[1][0]);
    let det = (a * a + b * b).max(1e-12);
    let (w, h) = (image.width() as i64, image.height() as i64);
    let sample = |x: i64, y: i64, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= w || y >= h {
            0.0
        } else {
            image.get_pixel(x as u32, y as u32)[c] as f32
        }
    };
    RgbImage::from_fn(ALIGNED, ALIGNED, |dx, dy| {
        let (ux, uy) = (dx as f32 - m[0][2], dy as f32 - m[1][2]);
        let (sx, sy) = ((a * ux + b * uy) / det, (-b * ux + a * uy) / det);
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let mut px = [0u8; 3];
        for (c, out) in px.iter_mut().enumerate() {
            let top = sample(x0, y0, c) * (1.0 - fx) + sample(x0 + 1, y0, c) * fx;
            let bottom = sample(x0, y0 + 1, c) * (1.0 - fx) + sample(x0 + 1, y0 + 1, c) * fx;
            *out = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
        }
        image::Rgb(px)
    })
}

pub struct Prepared {
    /// ภาพที่หมุนตาม EXIF แล้วและด้านยาวไม่เกิน [`MAX_IMAGE_DIM`]
    pub image: RgbImage,
    /// ขนาดเต็มของภาพหลังหมุน
    pub full_width: u32,
    pub full_height: u32,
}

impl Prepared {
    /// ตัวคูณจากพิกเซลของ `image` กลับเป็นพิกเซลของภาพเต็ม
    pub fn scale_to_full(&self) -> f32 {
        self.full_width as f32 / self.image.width() as f32
    }
}

fn exif_orientation(bytes: &[u8]) -> u8 {
    exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .and_then(|e| e.get_field(exif::Tag::Orientation, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0)))
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1) as u8
}

/// ถอดรหัสรูปสำหรับวิเคราะห์ใบหน้า: JPEG ถอดแบบย่อระหว่างถอด แล้วย่อให้ด้านยาวเท่ากับที่รุ่น Python ใช้
/// · RAW: โทนตั้งต้นที่แอปล้างจากข้อมูลเซนเซอร์ (ตั้งตรงแล้ว) ภาพเดียวกับภาพย่อและตัวดูภาพ
pub fn prepare(bytes: &[u8]) -> Result<Prepared> {
    if puri_core::raw::is_raw_bytes(bytes) {
        let info = puri_core::raw::info(bytes).map_err(|e| e.message)?;
        let img = puri_core::media::decode_upright(bytes, MAX_IMAGE_DIM).map_err(|e| e.message)?;
        let img = if img.width().max(img.height()) > MAX_IMAGE_DIM {
            img.resize(MAX_IMAGE_DIM, MAX_IMAGE_DIM, FilterType::Triangle)
        } else {
            img
        };
        return Ok(Prepared { image: img.to_rgb8(), full_width: info.width, full_height: info.height });
    }
    let orientation = exif_orientation(bytes);
    let mut decoded: Option<(DynamicImage, u32, u32)> = None;
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
        if decoder.read_info().is_ok() {
            if let Some(full) = decoder.info() {
                let long = full.width.max(full.height) as f32;
                let ratio = (MAX_IMAGE_DIM as f32 / long).min(1.0);
                let want = ((full.width as f32 * ratio).ceil() as u16, (full.height as f32 * ratio).ceil() as u16);
                if decoder.scale(want.0, want.1).is_ok() {
                    if let (Ok(pixels), Some(info)) = (decoder.decode(), decoder.info()) {
                        if info.pixel_format == jpeg_decoder::PixelFormat::RGB24 {
                            if let Some(img) = RgbImage::from_raw(info.width as u32, info.height as u32, pixels) {
                                decoded = Some((DynamicImage::ImageRgb8(img), full.width as u32, full.height as u32));
                            }
                        }
                    }
                }
            }
        }
    }
    let (mut img, raw_w, raw_h) = match decoded {
        Some(d) => d,
        None => {
            let img = image::load_from_memory(bytes)?;
            let (w, h) = (img.width(), img.height());
            (img, w, h)
        }
    };
    let long = raw_w.max(raw_h);
    if long > MAX_IMAGE_DIM {
        let scale = MAX_IMAGE_DIM as f32 / long as f32;
        let (w, h) = ((raw_w as f32 * scale) as u32, (raw_h as f32 * scale) as u32);
        img = img.resize_exact(w.max(1), h.max(1), FilterType::Triangle);
    }
    if let Some(o) = image::metadata::Orientation::from_exif(orientation) {
        img.apply_orientation(o);
    }
    let (full_width, full_height) = if orientation >= 5 { (raw_h, raw_w) } else { (raw_w, raw_h) };
    Ok(Prepared { image: img.to_rgb8(), full_width, full_height })
}

// ───────────────────────── ตัววิเคราะห์ของแอป (ชุด buffalo_l) ─────────────────────────

pub mod app {
    use puri_core::ai::{FaceEngineProvider, ModelFile, ModelSpec};

    /// เหตุผลที่แสดงผู้ใช้เมื่อ build นี้ไม่มีตัววิเคราะห์ใบหน้า
    pub const UNAVAILABLE: &str = "Mac รุ่นชิป Intel ยังใช้การวิเคราะห์ใบหน้าอัตโนมัติไม่ได้ ระบุใบหน้าเองได้จากตัวดูภาพ";

    pub fn spec() -> ModelSpec {
        ModelSpec {
            id: "buffalo_l",
            display_name: "InsightFace buffalo_l",
            space: "insightface/buffalo_l/w600k_r50",
            files: &[
                ModelFile {
                    name: "det_10g.onnx",
                    sha256: "5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91",
                },
                ModelFile {
                    name: "w600k_r50.onnx",
                    sha256: "4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43",
                },
            ],
            // นักพัฒนาชี้ไปไฟล์ในเครื่องได้ (file://…) เพื่อไม่ต้องดาวน์โหลดซ้ำระหว่างทดสอบ
            download_url: std::env::var("PURI_MODEL_URL").unwrap_or_else(|_| {
                "https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip".into()
            }),
            download_bytes: 288_621_354,
            download_source: "github.com/deepinsight/insightface (release v0.7)",
            license_note: "โมเดลของ InsightFace อนุญาตให้ใช้เฉพาะงานที่ไม่ใช่เชิงพาณิชย์ (non-commercial research)",
        }
    }

    /// ตัววิเคราะห์ของ build นี้; None บนแพลตฟอร์มที่ไม่มี ONNX Runtime สำเร็จรูป
    pub fn provider() -> Option<FaceEngineProvider> {
        #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
        {
            Some(FaceEngineProvider { spec: spec(), factory: std::sync::Arc::new(super::analyzer::load) })
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            None
        }
    }
}

// ───────────────────────── ตัวแยกตัวคน (Mask ตัวคน — DECISIONS ข้อ 40) ─────────────────────────

pub mod subject {
    use puri_core::subject::{FillProvider, SubjectModel, SubjectProvider};

    /// BiRefNet รุ่นเล็ก (Swin-T) สำหรับวัตถุทั่วไป — ไฟล์ .onnx ไฟล์เดียวจาก release ของผู้พัฒนาโมเดล
    pub fn model() -> SubjectModel {
        SubjectModel {
            id: "birefnet-lite",
            display_name: "BiRefNet (รุ่นเล็ก)",
            file: "BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx",
            sha256: "5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333",
            // นักพัฒนาชี้ไปไฟล์ในเครื่องได้ (file://…) เพื่อไม่ต้องดาวน์โหลดซ้ำระหว่างทดสอบ
            download_url: std::env::var("PURI_SUBJECT_MODEL_URL").unwrap_or_else(|_| {
                "https://github.com/ZhengPeng7/BiRefNet/releases/download/v1/BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx"
                    .into()
            }),
            download_bytes: 224_005_088,
            download_source: "github.com/ZhengPeng7/BiRefNet (release v1)",
            license_note: "BiRefNet เผยแพร่ด้วยสัญญาอนุญาต MIT ใช้ได้ทั้งงานทั่วไปและเชิงพาณิชย์",
        }
    }

    /// ตัวเติมฉากหลังของตัวผอม (DECISIONS ข้อ 41): LaMa (big-lama) แปลงเป็น ONNX ขนาด 512×512 — ไฟล์เดียวจาก Hugging Face
    /// (ตรึงรุ่นด้วย commit ของ repo)
    pub fn fill_model() -> SubjectModel {
        SubjectModel {
            id: "lama-fp32",
            display_name: "LaMa",
            file: "lama_fp32.onnx",
            sha256: "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6",
            download_url: std::env::var("PURI_FILL_MODEL_URL").unwrap_or_else(|_| {
                "https://huggingface.co/Carve/LaMa-ONNX/resolve/c3c0c9e468934d62e79c329e35d82dd09ff8c444/lama_fp32.onnx".into()
            }),
            download_bytes: 208_044_816,
            download_source: "huggingface.co/Carve/LaMa-ONNX (จาก github.com/advimman/lama)",
            license_note: "LaMa เผยแพร่ด้วยสัญญาอนุญาต Apache-2.0 ใช้ได้ทั้งงานทั่วไปและเชิงพาณิชย์",
        }
    }

    /// ตัวแยกส่วนของคน (Mask ส่วนของคน — DECISIONS ข้อ 43): MediaPipe Selfie Multiclass 256×256 ของ Google (Apache-2.0)
    /// แบบ ONNX ที่แปลงจาก TFLite ต้นฉบับ (ตรวจแล้วว่าผลเท่ากับ TFLite ของ Google ทุกพิกเซล — ตรึงรุ่นด้วย commit ของ repo)
    pub fn parts_model() -> SubjectModel {
        SubjectModel {
            id: "selfie-multiclass",
            display_name: "MediaPipe Selfie Multiclass",
            file: "selfie_multiclass_256x256.onnx",
            sha256: "35ec1ecd9ee7f85073c99c00020b7f6751b69506eeacf683bc8665f6117f85b0",
            download_url: std::env::var("PURI_PARTS_MODEL_URL").unwrap_or_else(|_| {
                "https://huggingface.co/senty-au/selfie_multiclass_256x256-ONNX/resolve/6db8421a7150ac20558f2c24675078eb3a1a04d0/onnx/model.onnx".into()
            }),
            download_bytes: 16_454_560,
            download_source: "huggingface.co/senty-au/selfie_multiclass_256x256-ONNX (โมเดลของ Google MediaPipe)",
            license_note: "โมเดลของ Google เผยแพร่ด้วยสัญญาอนุญาต Apache-2.0 ใช้ได้ทั้งงานทั่วไปและเชิงพาณิชย์",
        }
    }

    /// ตัวแยกท้องฟ้า (Mask ท้องฟ้า — DECISIONS ข้อ 44): U²-Net ที่ฝึกกับท้องฟ้าของ xiongzhu666 (MIT) — ไฟล์ .onnx ไฟล์เดียวจาก
    /// Hugging Face (ตรึงรุ่นด้วย commit ของ repo · SHA-256 ตรงกับไฟล์ต้นทางที่ chili-raw อ้าง)
    pub fn sky_model() -> SubjectModel {
        SubjectModel {
            id: "skyseg",
            display_name: "Sky Segmentation (U²-Net)",
            file: "skyseg.onnx",
            sha256: "ab9c34c64c3d821220a2886a4a06da4642ffa14d5b30e8d5339056a089aa1d39",
            download_url: std::env::var("PURI_SKY_MODEL_URL").unwrap_or_else(|_| {
                "https://huggingface.co/JianyuanWang/skyseg/resolve/3ba8c6df1d9ba9ff26f637c7ba9568ac11a9aa7f/skyseg.onnx".into()
            }),
            download_bytes: 175_997_079,
            download_source: "huggingface.co/JianyuanWang/skyseg (จาก github.com/xiongzhu666/Sky-Segmentation-and-Post-processing)",
            license_note: "ตัวแยกท้องฟ้าเผยแพร่ด้วยสัญญาอนุญาต MIT ใช้ได้ทั้งงานทั่วไปและเชิงพาณิชย์",
        }
    }

    /// ตัวแยกตัวคน (+ ตัวเติมฉากหลัง + ตัวแยกส่วนของคน + ตัวแยกท้องฟ้า) ของ build นี้; None บนแพลตฟอร์มที่ไม่มี ONNX Runtime สำเร็จรูป
    pub fn provider() -> Option<SubjectProvider> {
        #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
        {
            Some(SubjectProvider {
                model: model(),
                factory: std::sync::Arc::new(super::segmenter::load),
                fill: Some(FillProvider { model: fill_model(), factory: std::sync::Arc::new(super::inpainter::load) }),
                parts: Some(puri_core::parts::PartsProvider {
                    model: parts_model(),
                    factory: std::sync::Arc::new(super::part_segmenter::load),
                }),
                sky: Some(puri_core::sky::SkyProvider {
                    model: sky_model(),
                    factory: std::sync::Arc::new(super::sky_segmenter::load),
                }),
            })
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            None
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod segmenter {
    use image::{imageops, GrayImage, RgbImage};
    use ort::{environment::Environment, session::Session, value::Tensor};
    use puri_core::subject::Segmenter;
    use puri_core::AppError;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// ขนาดภาพที่โมเดลรับ (ตายตัว)
    const SIDE: u32 = 1024;
    /// ค่าปรับสีแบบ ImageNet ที่โมเดลฝึกมา
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];

    struct BiRefNet {
        session: Mutex<Session>,
        _env: Environment,
    }

    fn failed(e: impl std::fmt::Display) -> AppError {
        AppError::new("DECODE_FAILED", format!("หาตัวคนในรูปนี้ไม่สำเร็จ ({e})"))
    }

    pub fn load(model_dir: &Path) -> Result<Arc<dyn Segmenter>, AppError> {
        let path = model_dir.join(super::subject::model().file);
        if !path.is_file() {
            return Err(AppError::new("MODEL_NOT_READY", "ไม่พบไฟล์ตัวแยกตัวคน"));
        }
        let env = crate::environment().map_err(failed)?;
        // CPU เท่านั้น: ONNX Runtime บน Mac เปิด CoreML ให้เองถ้าไม่สั่ง PreferCPU — ลองโมเดลนี้บน CoreML (GPU)
        // แล้วเครื่องค้างจนรีบูต (22 พ.ย. 69) แบบ Neural Engine ก็แปลงโมเดลนานเกินครึ่งชั่วโมง · เหลือสองแกนให้หน้าจอลื่น
        let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(2)).unwrap_or(2).max(1);
        let mut builder = Session::builder(&env)
            .map_err(failed)?
            .with_intra_threads(threads)
            .map_err(failed)?
            .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
            .map_err(failed)?;
        let session = builder
            .commit_from_file(&path)
            .map_err(|e| AppError::new("MODEL_NOT_READY", format!("โหลดตัวแยกตัวคนไม่ได้ ({e})")))?;
        Ok(Arc::new(BiRefNet { session: Mutex::new(session), _env: env }))
    }

    impl Segmenter for BiRefNet {
        fn segment(&self, image: &RgbImage) -> Result<GrayImage, AppError> {
            let (w, h) = image.dimensions();
            if w == 0 || h == 0 {
                return Err(failed("ภาพว่าง"));
            }
            let small = imageops::resize(image, SIDE, SIDE, imageops::FilterType::Triangle);
            let plane = (SIDE * SIDE) as usize;
            let mut input = vec![0f32; 3 * plane];
            for (x, y, p) in small.enumerate_pixels() {
                let i = (y * SIDE + x) as usize;
                for c in 0..3 {
                    input[c * plane + i] = (f32::from(p[c]) / 255.0 - MEAN[c]) / STD[c];
                }
            }
            let tensor = Tensor::from_array(([1usize, 3, SIDE as usize, SIDE as usize], input)).map_err(failed)?;
            let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
            let outputs = session.run(ort::inputs![tensor]).map_err(failed)?;
            // ผลสุดท้ายของโมเดล (ค่าก่อน sigmoid) ขนาด 1×1×1024×1024
            let (_, data) = outputs[outputs.len() - 1].try_extract_tensor::<f32>().map_err(failed)?;
            if data.len() != plane {
                return Err(failed(format!("โมเดลคืนผล {} ค่า ไม่ใช่ {plane}", data.len())));
            }
            let mask = GrayImage::from_fn(SIDE, SIDE, |x, y| {
                let v = data[(y * SIDE + x) as usize];
                image::Luma([((1.0 / (1.0 + (-v).exp())) * 255.0).round() as u8])
            });
            Ok(imageops::resize(&mask, w, h, imageops::FilterType::Triangle))
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod part_segmenter {
    use image::{imageops, GrayImage, RgbImage};
    use ort::{environment::Environment, session::Session, value::Tensor};
    use puri_core::parts::PartSegmenter;
    use puri_core::AppError;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// ขนาดภาพที่โมเดลรับ (ตายตัว)
    const SIDE: u32 = 256;
    /// ช่องผลของโมเดล: 0 ฉากหลัง 1 ผม 2 ผิวตัว 3 ผิวหน้า 4 เสื้อผ้า 5 อื่น ๆ (เครื่องประดับ)
    const CLASSES: usize = 6;

    struct Selfie {
        session: Mutex<Session>,
        _env: Environment,
    }

    fn failed(e: impl std::fmt::Display) -> AppError {
        AppError::new("DECODE_FAILED", format!("แยกส่วนของคนในรูปนี้ไม่สำเร็จ ({e})"))
    }

    pub fn load(model_dir: &Path) -> Result<Arc<dyn PartSegmenter>, AppError> {
        let path = model_dir.join(super::subject::parts_model().file);
        if !path.is_file() {
            return Err(AppError::new("MODEL_NOT_READY", "ไม่พบไฟล์ตัวแยกส่วนของคน"));
        }
        let env = crate::environment().map_err(failed)?;
        // CPU (โมเดลเล็ก เร็วพออยู่แล้ว · ไม่ใช้ CoreML — ข้อ 40)
        let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(2)).unwrap_or(2).max(1);
        let mut builder = Session::builder(&env)
            .map_err(failed)?
            .with_intra_threads(threads)
            .map_err(failed)?
            .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
            .map_err(failed)?;
        let session = builder
            .commit_from_file(&path)
            .map_err(|e| AppError::new("MODEL_NOT_READY", format!("โหลดตัวแยกส่วนของคนไม่ได้ ({e})")))?;
        Ok(Arc::new(Selfie { session: Mutex::new(session), _env: env }))
    }

    impl PartSegmenter for Selfie {
        fn segment(&self, image: &RgbImage) -> Result<[GrayImage; 4], AppError> {
            let (w, h) = image.dimensions();
            if w == 0 || h == 0 {
                return Err(failed("ภาพว่าง"));
            }
            // ยืดเป็น 256×256 (ไม่เติมขอบ) ค่า 0–1 แบบ NHWC
            let small = imageops::resize(image, SIDE, SIDE, imageops::FilterType::Triangle);
            let input: Vec<f32> = small.as_raw().iter().map(|&v| f32::from(v) / 255.0).collect();
            let side = SIDE as usize;
            let tensor = Tensor::from_array(([1usize, side, side, 3], input)).map_err(failed)?;
            let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
            let outputs = session.run(ort::inputs![tensor]).map_err(failed)?;
            // ค่าก่อน softmax 1×256×256×6
            let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(failed)?;
            if data.len() != side * side * CLASSES {
                return Err(failed(format!("โมเดลคืนผล {} ค่า", data.len())));
            }
            let mut maps: [GrayImage; 4] = std::array::from_fn(|_| GrayImage::new(SIDE, SIDE));
            for i in 0..side * side {
                let logits = &data[i * CLASSES..(i + 1) * CLASSES];
                let top = logits.iter().copied().fold(f32::MIN, f32::max);
                let e: Vec<f32> = logits.iter().map(|v| (v - top).exp()).collect();
                let sum: f32 = e.iter().sum();
                let (x, y) = ((i % side) as u32, (i / side) as u32);
                for (k, map) in maps.iter_mut().enumerate() {
                    map.put_pixel(x, y, image::Luma([(e[k + 1] / sum * 255.0).round() as u8]));
                }
            }
            Ok(maps.map(|m| imageops::resize(&m, w, h, imageops::FilterType::Triangle)))
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod sky_segmenter {
    use image::{imageops, GrayImage, RgbImage};
    use ort::{environment::Environment, session::Session, value::Tensor};
    use puri_core::sky::SkySegmenter;
    use puri_core::AppError;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// ขนาดภาพที่โมเดลรับ (ตายตัว)
    const SIDE: u32 = 320;
    /// ค่าปรับสีแบบ ImageNet ที่โมเดลฝึกมา
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];

    struct SkySeg {
        session: Mutex<Session>,
        _env: Environment,
    }

    fn failed(e: impl std::fmt::Display) -> AppError {
        AppError::new("DECODE_FAILED", format!("หาท้องฟ้าในรูปนี้ไม่สำเร็จ ({e})"))
    }

    pub fn load(model_dir: &Path) -> Result<Arc<dyn SkySegmenter>, AppError> {
        let path = model_dir.join(super::subject::sky_model().file);
        if !path.is_file() {
            return Err(AppError::new("MODEL_NOT_READY", "ไม่พบไฟล์ตัวแยกท้องฟ้า"));
        }
        let env = crate::environment().map_err(failed)?;
        // CPU เท่านั้น (ไม่ใช้ CoreML กับโมเดลใหญ่ — ข้อ 40)
        let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(2)).unwrap_or(2).max(1);
        let mut builder = Session::builder(&env)
            .map_err(failed)?
            .with_intra_threads(threads)
            .map_err(failed)?
            .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
            .map_err(failed)?;
        let session = builder
            .commit_from_file(&path)
            .map_err(|e| AppError::new("MODEL_NOT_READY", format!("โหลดตัวแยกท้องฟ้าไม่ได้ ({e})")))?;
        Ok(Arc::new(SkySeg { session: Mutex::new(session), _env: env }))
    }

    impl SkySegmenter for SkySeg {
        fn segment(&self, image: &RgbImage) -> Result<GrayImage, AppError> {
            if image.width() == 0 || image.height() == 0 {
                return Err(failed("ภาพว่าง"));
            }
            // ยืดเป็น 320×320 (ไม่เติมขอบ) แบบ NCHW ปรับสีแบบ ImageNet
            let small = imageops::resize(image, SIDE, SIDE, imageops::FilterType::Triangle);
            let plane = (SIDE * SIDE) as usize;
            let mut input = vec![0f32; 3 * plane];
            for (x, y, p) in small.enumerate_pixels() {
                let i = (y * SIDE + x) as usize;
                for c in 0..3 {
                    input[c * plane + i] = (f32::from(p[c]) / 255.0 - MEAN[c]) / STD[c];
                }
            }
            let tensor = Tensor::from_array(([1usize, 3, SIDE as usize, SIDE as usize], input)).map_err(failed)?;
            let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
            let outputs = session.run(ort::inputs![tensor]).map_err(failed)?;
            // ผลหลักของ U²-Net (ผ่าน sigmoid แล้ว 0–1) ขนาด 1×1×320×320 — ไม่ยืดค่าต่ำสุด–สูงสุด (รูปที่ไม่มีฟ้าต้องได้ว่าง)
            let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(failed)?;
            if data.len() != plane {
                return Err(failed(format!("โมเดลคืนผล {} ค่า ไม่ใช่ {plane}", data.len())));
            }
            Ok(GrayImage::from_fn(SIDE, SIDE, |x, y| {
                image::Luma([(data[(y * SIDE + x) as usize].clamp(0.0, 1.0) * 255.0).round() as u8])
            }))
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod inpainter {
    use image::{GrayImage, RgbImage};
    use ort::{environment::Environment, session::Session, value::Tensor};
    use puri_core::subject::Inpainter;
    use puri_core::AppError;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// ขนาดภาพที่โมเดลรับ (ตายตัว)
    const SIDE: u32 = 512;

    struct Lama {
        session: Mutex<Session>,
        _env: Environment,
    }

    fn failed(e: impl std::fmt::Display) -> AppError {
        AppError::new("DECODE_FAILED", format!("เติมฉากหลังของรูปนี้ไม่สำเร็จ ({e})"))
    }

    pub fn load(model_dir: &Path) -> Result<Arc<dyn Inpainter>, AppError> {
        let path = model_dir.join(super::subject::fill_model().file);
        if !path.is_file() {
            return Err(AppError::new("MODEL_NOT_READY", "ไม่พบไฟล์ตัวเติมฉากหลัง"));
        }
        let env = crate::environment().map_err(failed)?;
        // CPU เท่านั้น (โมเดลใหญ่บน CoreML ทำเครื่องค้างจนรีบูต — ข้อ 40) · เหลือสองแกนให้หน้าจอลื่น
        let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(2)).unwrap_or(2).max(1);
        let mut builder = Session::builder(&env)
            .map_err(failed)?
            .with_intra_threads(threads)
            .map_err(failed)?
            .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
            .map_err(failed)?;
        let session = builder
            .commit_from_file(&path)
            .map_err(|e| AppError::new("MODEL_NOT_READY", format!("โหลดตัวเติมฉากหลังไม่ได้ ({e})")))?;
        Ok(Arc::new(Lama { session: Mutex::new(session), _env: env }))
    }

    impl Inpainter for Lama {
        fn side(&self) -> u32 {
            SIDE
        }

        fn inpaint(&self, image: &RgbImage, holes: &GrayImage) -> Result<RgbImage, AppError> {
            if image.dimensions() != (SIDE, SIDE) || holes.dimensions() != (SIDE, SIDE) {
                return Err(failed("ภาพผิดขนาด"));
            }
            let plane = (SIDE * SIDE) as usize;
            let mut pixels = vec![0f32; 3 * plane];
            let mut mask = vec![0f32; plane];
            for (x, y, p) in image.enumerate_pixels() {
                let i = (y * SIDE + x) as usize;
                let hole = holes.get_pixel(x, y)[0] > 127;
                // ช่องต้องเป็นศูนย์: โมเดลไม่ลบให้เอง (ไม่ลบ = เห็นเงาของสิ่งเดิมในช่อง)
                for c in 0..3 {
                    pixels[c * plane + i] = if hole { 0.0 } else { f32::from(p[c]) / 255.0 };
                }
                mask[i] = if hole { 1.0 } else { 0.0 };
            }
            let side = SIDE as usize;
            let image_t = Tensor::from_array(([1usize, 3, side, side], pixels)).map_err(failed)?;
            let mask_t = Tensor::from_array(([1usize, 1, side, side], mask)).map_err(failed)?;
            let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
            let outputs = session.run(ort::inputs!["image" => image_t, "mask" => mask_t]).map_err(failed)?;
            // ผล 1×3×512×512 ค่า 0–255 (นอกช่องเท่าภาพที่ส่งเข้า)
            let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(failed)?;
            if data.len() != 3 * plane {
                return Err(failed(format!("โมเดลคืนผล {} ค่า ไม่ใช่ {}", data.len(), 3 * plane)));
            }
            Ok(RgbImage::from_fn(SIDE, SIDE, |x, y| {
                let i = (y * SIDE + x) as usize;
                image::Rgb([0, 1, 2].map(|c| data[c * plane + i].round().clamp(0.0, 255.0) as u8))
            }))
        }
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod analyzer {
    use super::{prepare, FaceEngine, ALIGNED, ARCFACE_DST};
    use image::{imageops::FilterType, RgbImage};
    use puri_core::ai::{DetectedFace, FaceAnalyzer};
    use puri_core::faces::BBox;
    use puri_core::AppError;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// ONNX Runtime ไม่ให้ใช้ session เดียวจากหลาย thread พร้อมกัน: เก็บ engine เดียวหลัง mutex
    /// (งานถอดรหัสรูปยังขนานกันได้ ส่วน inference ใช้หลาย thread ภายในอยู่แล้ว)
    struct OrtAnalyzer(Mutex<FaceEngine>);

    fn failed(e: impl std::fmt::Display) -> AppError {
        AppError::new("DECODE_FAILED", format!("วิเคราะห์ใบหน้าของรูปนี้ไม่สำเร็จ ({e})"))
    }

    pub fn load(model_dir: &Path) -> Result<Arc<dyn FaceAnalyzer>, AppError> {
        let threads = std::thread::available_parallelism().map(|n| n.get() / 2).unwrap_or(2).clamp(1, 6);
        let engine = FaceEngine::load(model_dir, "det_10g.onnx", "w600k_r50.onnx", threads)
            .map_err(|e| AppError::new("MODEL_NOT_READY", e.to_string()))?;
        Ok(Arc::new(OrtAnalyzer(Mutex::new(engine))))
    }

    impl FaceAnalyzer for OrtAnalyzer {
        fn analyze(&self, image_bytes: &[u8]) -> Result<Vec<DetectedFace>, AppError> {
            let prepared = prepare(image_bytes).map_err(failed)?;
            let (w, h) = (prepared.image.width() as f64, prepared.image.height() as f64);
            let mut engine = self.0.lock().unwrap_or_else(|e| e.into_inner());
            let detections = engine.detect(&prepared.image).map_err(failed)?;
            let mut faces = Vec::with_capacity(detections.len());
            for d in detections {
                let embedding = engine.embed(&prepared.image, &d.landmarks).map_err(failed)?;
                // พิกัดเป็นสัดส่วนบนภาพที่หมุนแล้ว หนีบให้อยู่ในภาพ (ตัวตรวจอาจให้กรอบล้นขอบเล็กน้อย)
                let (x1, y1) = ((d.bbox[0] as f64 / w).clamp(0.0, 1.0), (d.bbox[1] as f64 / h).clamp(0.0, 1.0));
                let (x2, y2) = ((d.bbox[2] as f64 / w).clamp(0.0, 1.0), (d.bbox[3] as f64 / h).clamp(0.0, 1.0));
                if x2 - x1 <= 0.0 || y2 - y1 <= 0.0 {
                    continue;
                }
                faces.push(DetectedFace {
                    bbox: BBox { x: x1, y: y1, w: x2 - x1, h: y2 - y1 },
                    score: d.score,
                    embedding,
                });
            }
            Ok(faces)
        }

        fn embed_crop(&self, image_bytes: &[u8]) -> Result<Option<Vec<f32>>, AppError> {
            let crop = image::load_from_memory(image_bytes).map_err(failed)?.to_rgb8();
            if crop.width() < 24 || crop.height() < 24 {
                return Ok(None);
            }
            // ภาพครอปของรุ่นเดิมแน่นติดใบหน้า: วางกลางผืนดำที่ใหญ่กว่าเพื่อให้ตัวตรวจเห็นทั้งหน้า
            let side = crop.width().max(crop.height()) * 2;
            let scale = (640.0 / side as f32).max(1.0);
            let (cw, ch) = ((crop.width() as f32 * scale) as u32, (crop.height() as f32 * scale) as u32);
            let resized = image::imageops::resize(&crop, cw, ch, FilterType::Triangle);
            let side = (side as f32 * scale) as u32;
            let mut canvas = RgbImage::new(side, side);
            image::imageops::replace(&mut canvas, &resized, ((side - cw) / 2) as i64, ((side - ch) / 2) as i64);
            let mut engine = self.0.lock().unwrap_or_else(|e| e.into_inner());
            let found = engine.detect(&canvas).map_err(failed)?;
            let area = |d: &super::Detection| (d.bbox[2] - d.bbox[0]) * (d.bbox[3] - d.bbox[1]);
            if let Some(best) = found.iter().max_by(|a, b| area(a).total_cmp(&area(b))) {
                return Ok(Some(engine.embed(&canvas, &best.landmarks).map_err(failed)?));
            }
            // ตรวจไม่พบ: ถือว่าครอปคือใบหน้าที่จัดกลางแล้ว (คุณภาพต่ำกว่า แต่ยังช่วยเสนอชื่อได้)
            let s = [crop.width() as f32 / ALIGNED as f32, crop.height() as f32 / ALIGNED as f32];
            let landmarks = ARCFACE_DST.map(|p| [p[0] * s[0], p[1] * s[1]]);
            Ok(Some(engine.embed(&crop, &landmarks).map_err(failed)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    #[test]
    fn every_model_shares_one_onnx_environment() {
        // ort สร้าง environment ได้ครั้งเดียวต่อโปรเซส: ขอซ้ำต้องได้ตัวเดิม ไม่ใช่ข้อผิดพลาด
        let first = environment().expect("ครั้งแรก");
        drop(first);
        environment().expect("ครั้งที่สอง หลังตัวแรกถูกปล่อย");
        assert!(ort::init().build().is_err(), "ort ไม่ยอมให้สร้างตัวที่สอง — เหตุผลที่ต้องใช้ร่วมกัน");
    }

    #[test]
    fn similarity_recovers_a_known_transform() {
        // จุดต้นทาง = จุดมาตรฐานที่ถูกย่อ 2 เท่า หมุน 90° แล้วเลื่อน → transform ต้องพากลับไปตรงจุดมาตรฐาน
        let mut src = [[0.0f32; 2]; 5];
        for (s, d) in src.iter_mut().zip(ARCFACE_DST.iter()) {
            *s = [-d[1] * 2.0 + 300.0, d[0] * 2.0 + 50.0];
        }
        let m = similarity(&src, &ARCFACE_DST);
        for (s, d) in src.iter().zip(ARCFACE_DST.iter()) {
            let x = m[0][0] * s[0] + m[0][1] * s[1] + m[0][2];
            let y = m[1][0] * s[0] + m[1][1] * s[1] + m[1][2];
            assert!((x - d[0]).abs() < 1e-3 && (y - d[1]).abs() < 1e-3, "({x}, {y}) != {d:?}");
        }
    }

    #[test]
    fn iou_and_cosine_basics() {
        assert_eq!(iou(&[0.0, 0.0, 10.0, 10.0], &[0.0, 0.0, 10.0, 10.0]), 1.0);
        assert_eq!(iou(&[0.0, 0.0, 10.0, 10.0], &[20.0, 20.0, 30.0, 30.0]), 0.0);
        assert!((iou(&[0.0, 0.0, 10.0, 10.0], &[5.0, 0.0, 15.0, 10.0]) - 1.0 / 3.0).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 3.0]).abs() < 1e-6);
    }
}
