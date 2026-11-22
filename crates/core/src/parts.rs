//! ส่วนของคน (DECISIONS ข้อ 43): ผม ผิวหน้า ผิวตัว เสื้อผ้า — แบบ Mask "People" ของ Lightroom (Select People ›
//! Hair / Facial Skin / Body Skin / Clothes)
//!
//! - ตัวแยกส่วนของคน (MediaPipe Selfie Multiclass ของ Google · Apache-2.0) รับภาพ 256×256 — ดูทีละคนจากกรอบของแต่ละคนใน
//!   ภาพน้ำหนักตัวคน (ข้อ 40) ให้รายละเอียดพอ แล้วคูณด้วยภาพน้ำหนักตัวคน (ขอบของตัวคนคมกว่าของตัวแยกส่วน)
//! - ภาพน้ำหนักเป็นของรูป (แบบภาพน้ำหนักตัวคน): ค่าที่แต่งเก็บแค่ส่วนที่เลือก แนบภาพตอนสร้างภาพ
//!   ([`crate::subject::attach_all`]) · เก็บใน cache ตาม revision (`<revision>-parts.png` ช่อง R G B A = ผม ผิวตัว ผิวหน้า เสื้อผ้า)

use crate::error::{AppError, Result};
use crate::subject::{SubjectMap, SubjectModel};
use image::{imageops, GrayImage, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// ส่วนของคน — ลำดับช่องในภาพน้ำหนัก: ผม ผิวตัว ผิวหน้า เสื้อผ้า
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    Hair,
    FaceSkin,
    BodySkin,
    Clothes,
}

impl Part {
    fn channel(self) -> usize {
        match self {
            Part::Hair => 0,
            Part::BodySkin => 1,
            Part::FaceSkin => 2,
            Part::Clothes => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Part::Hair => "ผม",
            Part::FaceSkin => "ผิวหน้า",
            Part::BodySkin => "ผิวตัว",
            Part::Clothes => "เสื้อผ้า",
        }
    }
}

/// ตัวแยกส่วนของคน
pub trait PartSegmenter: Send + Sync {
    /// ความเป็น ผม ผิวตัว ผิวหน้า เสื้อผ้า ของแต่ละพิกเซล 0–255 — ภาพผลขนาดเท่าภาพที่ส่งเข้า
    fn segment(&self, image: &RgbImage) -> Result<[GrayImage; 4]>;
}

pub type PartFactory = Arc<dyn Fn(&Path) -> Result<Arc<dyn PartSegmenter>> + Send + Sync>;

pub struct PartsProvider {
    pub model: SubjectModel,
    pub factory: PartFactory,
}

/// น้ำหนักที่ถือว่า "มี" ตอนหากรอบของแต่ละส่วน
const PRESENT: u8 = 8;
/// ขยายกรอบของแต่ละคนก่อนส่งให้ตัวแยกส่วน (สัดส่วนของด้านยาวของกรอบ)
const AROUND: f64 = 0.1;

/// ภาพน้ำหนักของส่วนของคนของรูปหนึ่ง (แชร์ได้ ไม่คัดลอกภาพ)
#[derive(Clone)]
pub struct PartsMap {
    image: Arc<RgbaImage>,
    /// ต่อช่อง: กรอบ [x0, y0, x1, y1] (สัดส่วน) ของส่วนที่มีน้ำหนัก
    bounds: [Option<[f64; 4]>; 4],
}

impl PartsMap {
    pub fn new(image: RgbaImage) -> PartsMap {
        let (w, h) = image.dimensions();
        let mut b = [(u32::MAX, u32::MAX, 0u32, 0u32); 4];
        for (x, y, p) in image.enumerate_pixels() {
            for c in 0..4 {
                if p[c] >= PRESENT {
                    let e = &mut b[c];
                    *e = (e.0.min(x), e.1.min(y), e.2.max(x + 1), e.3.max(y + 1));
                }
            }
        }
        let bounds = b.map(|(x0, y0, x1, y1)| {
            (x1 > x0 && y1 > y0)
                .then(|| [x0 as f64 / w as f64, y0 as f64 / h as f64, x1 as f64 / w as f64, y1 as f64 / h as f64])
        });
        PartsMap { image: Arc::new(image), bounds }
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// น้ำหนัก 0–1 ของส่วนที่เลือก (รวมกัน) ที่จุด (u, v) บนภาพเดิม (bilinear · นอกภาพ = 0)
    pub fn weight(&self, u: f64, v: f64, parts: &[Part]) -> f64 {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return 0.0;
        }
        let (w, h) = self.image.dimensions();
        let x = (u * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let y = (v * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let at = |xx: u32, yy: u32| {
            let p = self.image.get_pixel(xx, yy);
            parts.iter().map(|part| p[part.channel()] as f64).sum::<f64>().min(255.0)
        };
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        (top * (1.0 - fy) + bottom * fy) / 255.0
    }

    /// กรอบที่ครอบทุกส่วนที่เลือก (สัดส่วน) — None = ไม่มีส่วนนั้นในรูป
    pub fn bounds(&self, parts: &[Part]) -> Option<[f64; 4]> {
        parts
            .iter()
            .filter_map(|p| self.bounds[p.channel()])
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
    }

    /// พบส่วนนี้ในรูป
    pub fn has(&self, part: Part) -> bool {
        self.bounds[part.channel()].is_some()
    }
}

/// ภาพน้ำหนักเป็นของรูป ไม่ใช่ส่วนของค่าที่แต่ง
impl PartialEq for PartsMap {
    fn eq(&self, _: &PartsMap) -> bool {
        true
    }
}

impl std::fmt::Debug for PartsMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PartsMap({}×{})", self.image.width(), self.image.height())
    }
}

/// ภาพน้ำหนักของส่วนของคน: ดูทีละคนจากกรอบของคนนั้นในภาพน้ำหนักตัวคน (ไม่มีคน = ทั้งภาพ) แล้วคูณด้วยภาพน้ำหนักตัวคน ·
/// `base` = ภาพเดียวกับที่ใช้หาตัวคน (ย่อให้เท่าภาพน้ำหนักตัวคน)
pub fn compute(base: &RgbImage, map: &SubjectMap, segmenter: &dyn PartSegmenter) -> Result<PartsMap> {
    let (w, h) = map.image().dimensions();
    let base = if base.dimensions() == (w, h) {
        base.clone()
    } else {
        imageops::resize(base, w, h, imageops::FilterType::Triangle)
    };
    let mut boxes: Vec<(u32, u32, u32, u32)> = map
        .people()
        .iter()
        .map(|p| {
            let (x0, y0, x1, y1) = p.bounds();
            let m = AROUND * (x1 - x0).max(y1 - y0);
            let px = |v: f64, size: u32| v.round().clamp(0.0, size as f64) as u32;
            (px(x0 - m, w), px(y0 - m, h), px(x1 + m, w), px(y1 + m, h))
        })
        .filter(|(x0, y0, x1, y1)| x1 > x0 && y1 > y0)
        .collect();
    if boxes.is_empty() {
        boxes.push((0, 0, w, h));
    }
    let mut out = RgbaImage::new(w, h);
    for (x0, y0, x1, y1) in boxes {
        let crop = imageops::crop_imm(&base, x0, y0, x1 - x0, y1 - y0).to_image();
        let maps = segmenter.segment(&crop)?;
        if maps.iter().any(|m| m.dimensions() != crop.dimensions()) {
            return Err(AppError::internal("ตัวแยกส่วนของคนคืนภาพผิดขนาด"));
        }
        for (x, y, p) in out.enumerate_pixels_mut() {
            if !(x0..x1).contains(&x) || !(y0..y1).contains(&y) {
                continue;
            }
            for (c, m) in maps.iter().enumerate() {
                p[c] = p[c].max(m.get_pixel(x - x0, y - y0)[0]);
            }
        }
    }
    // ขอบนอกของตัวคนตามภาพน้ำหนักตัวคน (ตัวแยกส่วนทำงานที่ 256 พิกเซล ขอบหยาบกว่า)
    let people = map.image();
    for (x, y, p) in out.enumerate_pixels_mut() {
        let k = people.get_pixel(x, y)[0] as u32;
        for c in 0..4 {
            p[c] = ((p[c] as u32 * k + 127) / 255) as u8;
        }
    }
    Ok(PartsMap::new(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb};

    /// ตัวแยกส่วนจำลอง: สีน้ำตาล = ผม · สีผิว = ผิวหน้า · สีน้ำเงิน = เสื้อผ้า
    pub(crate) struct ByColor;
    impl PartSegmenter for ByColor {
        fn segment(&self, image: &RgbImage) -> Result<[GrayImage; 4]> {
            let pick = |want: fn(&Rgb<u8>) -> bool| {
                GrayImage::from_fn(image.width(), image.height(), |x, y| {
                    Luma([if want(image.get_pixel(x, y)) { 255 } else { 0 }])
                })
            };
            Ok([
                pick(|p| p[0] < 90 && p[1] < 60 && p[2] < 50),
                GrayImage::new(image.width(), image.height()),
                pick(|p| p[0] > 200 && p[1] > 150 && p[2] > 120 && p[2] < 180),
                pick(|p| p[2] > 180 && p[0] < 80),
            ])
        }
    }

    /// คนกลางภาพ 100×100 (x 30–70): ผม y 10–25 · หน้า y 25–45 · เสื้อ y 45–95 บนพื้นเทา · ภาพน้ำหนักตัวคน = กรอบนั้น
    fn scene() -> (RgbImage, SubjectMap) {
        let img = RgbImage::from_fn(100, 100, |x, y| match (x, y) {
            (30..70, 10..25) => Rgb([60, 35, 25]),
            (30..70, 25..45) => Rgb([230, 180, 150]),
            (30..70, 45..95) => Rgb([40, 70, 210]),
            _ => Rgb([120, 120, 120]),
        });
        let map = GrayImage::from_fn(100, 100, |x, y| {
            Luma([if (30..70).contains(&x) && (10..95).contains(&y) { 255 } else { 0 }])
        });
        (img, SubjectMap::new(map))
    }

    #[test]
    fn parts_are_found_on_the_person_and_combine() {
        let (img, map) = scene();
        let parts = compute(&img, &map, &ByColor).unwrap();
        let at = |x: f64, y: f64, p: &[Part]| parts.weight((x + 0.5) / 100.0, (y + 0.5) / 100.0, p);
        assert_eq!(at(50.0, 15.0, &[Part::Hair]), 1.0);
        assert_eq!(at(50.0, 15.0, &[Part::Clothes]), 0.0);
        assert_eq!(at(50.0, 35.0, &[Part::FaceSkin]), 1.0);
        assert_eq!(at(50.0, 70.0, &[Part::Clothes]), 1.0);
        assert_eq!(at(50.0, 70.0, &[Part::Hair, Part::Clothes]), 1.0, "รวมกันได้");
        assert_eq!(at(10.0, 50.0, &[Part::Hair, Part::FaceSkin, Part::BodySkin, Part::Clothes]), 0.0, "ฉากหลัง");
        assert!(!parts.has(Part::BodySkin));
        let b = parts.bounds(&[Part::Hair]).unwrap();
        assert!((b[1] - 0.10).abs() < 1e-9 && (b[3] - 0.25).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn outside_the_person_map_there_are_no_parts() {
        // ภาพน้ำหนักตัวคนครอบแค่ครึ่งซ้ายของคน: ครึ่งขวาไม่มีส่วนใดเลย
        let (img, _) = scene();
        let half = SubjectMap::new(GrayImage::from_fn(100, 100, |x, y| {
            Luma([if (30..50).contains(&x) && (10..95).contains(&y) { 255 } else { 0 }])
        }));
        let parts = compute(&img, &half, &ByColor).unwrap();
        assert_eq!(parts.weight(40.5 / 100.0, 0.7, &[Part::Clothes]), 1.0);
        assert_eq!(parts.weight(60.5 / 100.0, 0.7, &[Part::Clothes]), 0.0);
    }
}
