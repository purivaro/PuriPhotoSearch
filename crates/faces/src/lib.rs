//! ตรวจใบหน้าและสร้างเวกเตอร์ใบหน้าในเครื่องด้วย ONNX Runtime (งานทดลอง Phase 0 — ยังไม่ถูกเรียกจากแอป)
//!
//! เป้าหมาย: พิสูจน์ว่า Rust ให้ผลใน embedding space เดียวกับรุ่น Python (insightface 0.7.3 + `buffalo_sc`)
//! จึงเลียนขั้นตอนของรุ่นเดิมทีละขั้น: ย่อภาพให้ด้านยาว ≤ 1280 → SCRFD ที่ 640×640 (วางชิดมุมซ้ายบน) →
//! จัดหน้า 112×112 ด้วย similarity transform จากจุดสำคัญ 5 จุด → เวกเตอร์ 512 มิติ (ไม่ normalize)
//!
//! ไฟล์โมเดลไม่อยู่ใน repo และไม่ถูกแจกกับแอป: ผู้เรียกชี้โฟลเดอร์ที่มี `det_500m.onnx` และ `w600k_mbf.onnx` เอง
//! (สัญญาอนุญาตของโมเดล: docs/DECISIONS.md ข้อ 9)

use image::{imageops::FilterType, DynamicImage, RgbImage};
use ort::{environment::Environment, session::Session, value::Tensor};
use std::io::Cursor;
use std::path::Path;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

/// ค่าเดียวกับรุ่น Python (`app/services/face_service.py`)
pub const MAX_IMAGE_DIM: u32 = 1280;
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

pub struct FaceEngine {
    detector: Session,
    embedder: Session,
    _env: Environment,
}

impl FaceEngine {
    pub fn load(model_dir: &Path, threads: usize) -> Result<Self> {
        let env = ort::init().with_name("puri-faces").build()?;
        let open = |file: &str| -> Result<Session> {
            let path = model_dir.join(file);
            if !path.is_file() {
                return Err(format!("ไม่พบไฟล์โมเดล {}", path.display()).into());
            }
            Ok(Session::builder(&env)?
                .with_intra_threads(threads.max(1))
                .map_err(|e| e.to_string())?
                .commit_from_file(path)?)
        };
        Ok(Self { detector: open("det_500m.onnx")?, embedder: open("w600k_mbf.onnx")?, _env: env })
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
pub fn prepare(bytes: &[u8]) -> Result<Prepared> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
