//! Mask ท้องฟ้า (AI — DECISIONS ข้อ 44) แบบ "Select Sky" ของ Lightroom
//!
//! ตัวแยกท้องฟ้าดูทั้งภาพที่ขนาดเล็ก (U²-Net 320×320) แล้วแอปทำต่อสามขั้น:
//! 1. **เก็บเฉพาะท้องฟ้าที่มั่นใจ:** แบ่งเป็นกลุ่มที่ติดกัน กลุ่มที่ค่ากลาง (median) ต่ำกว่า [`CONFIDENT`] ทิ้งทั้งกลุ่ม —
//!    ผนังขาวในร่มได้ค่ากลาง ๆ ทั้งผืน (ลองแล้ว 0.75–0.93) ส่วนท้องฟ้าจริงเกือบ 1.0
//! 2. **ขอบตามภาพจริง:** ขยายเป็นด้านยาว [`SKY_EDGE`] แล้วตรงขอบ (ค่าไม่ชัด) เทียบสีของพิกเซลกับสีเฉลี่ยของฟ้าที่แน่นอนและ
//!    ของส่วนที่ไม่ใช่ฟ้าแน่นอนรอบ ๆ — อยู่ใกล้สีไหนก็เป็นฝั่งนั้น ขอบจึงตามหลังคา ช่อฟ้า ตึก แทนขอบหยาบของภาพ 320
//!    (ลองแล้วคมกว่า guided filter ที่ยังเลยเส้นขอบฟ้าจริงไปครึ่งช่องของโมเดล)
//! 3. **ไม่นับตัวคน:** คูณด้วย (1 − ภาพน้ำหนักตัวคน) — เสื้อขาวที่สว่างติดกับฟ้าขาวถูกนับเป็นฟ้า (ลองแล้ว)
//!
//! ภาพน้ำหนักเป็นของรูป (เก็บ `<revision>-sky.png`) แนบตอนสร้างภาพเหมือน Mask ตัวคน

use crate::error::Result;
use crate::subject::{SubjectMap, SubjectModel};
use image::{imageops, GrayImage, RgbImage};
use std::path::Path;
use std::sync::Arc;

/// ด้านยาวของภาพน้ำหนักท้องฟ้า (ละเอียดกว่าตัวคน: ขอบใบไม้กับฟ้าเห็นชัดในภาพใหญ่)
pub const SKY_EDGE: u32 = 2048;
/// กลุ่มที่ค่ากลางต่ำกว่านี้ (0–255) ไม่ใช่ท้องฟ้า
const CONFIDENT: u8 = 242;
/// ค่าที่นับว่าเป็นส่วนหนึ่งของกลุ่ม
const PART_OF: u8 = 128;
/// ขยายกลุ่มที่เก็บออกไปเท่านี้พิกเซล (ของภาพจากโมเดล) ให้ขอบนุ่มรอบกลุ่มอยู่ครบ
const GROW: u32 = 3;
/// ขอบ: รัศมีที่ใช้หาสีของสองฝั่ง (สัดส่วนของด้านยาว) · ค่าที่นับว่าแน่นอน (ฟ้า/ไม่ใช่ฟ้า)
const EDGE_RADIUS: f64 = 1.0 / 64.0;
const SURE_SKY: f32 = 0.98;
const SURE_GROUND: f32 = 0.02;
/// สีของสองฝั่งต้องต่างกันอย่างน้อยเท่านี้ (ระยะใน RGB 0–1) จึงเชื่อการเทียบสี เต็มที่เมื่อต่างกัน + [`APART_FULL`]
const APART: f32 = 0.05;
const APART_FULL: f32 = 0.1;
/// สองฝั่งต้องมีพิกเซลในกรอบอย่างน้อยเท่านี้ (สัดส่วนของกรอบ) เต็มที่เมื่อเพิ่มอีก [`SEEN_FULL`]
const SEEN: f32 = 0.002;
const SEEN_FULL: f32 = 0.02;
/// น้ำหนักที่ถือว่า "มี" ตอนหากรอบ
const PRESENT: u8 = 8;

/// ตัวแยกท้องฟ้า
pub trait SkySegmenter: Send + Sync {
    /// ความเป็นท้องฟ้า 0–255 ของทั้งภาพ — ภาพผลขนาดใดก็ได้ (ปกติเท่าที่โมเดลรับ) แอปขยายให้เอง
    fn segment(&self, image: &RgbImage) -> Result<GrayImage>;
}

pub type SkyFactory = Arc<dyn Fn(&Path) -> Result<Arc<dyn SkySegmenter>> + Send + Sync>;

pub struct SkyProvider {
    pub model: SubjectModel,
    pub factory: SkyFactory,
}

/// ภาพน้ำหนักท้องฟ้าของรูปหนึ่ง (แชร์ได้ ไม่คัดลอกภาพ)
#[derive(Clone)]
pub struct SkyMap {
    image: Arc<GrayImage>,
    bounds: Option<[f64; 4]>,
}

impl SkyMap {
    pub fn new(image: GrayImage) -> SkyMap {
        let (w, h) = image.dimensions();
        let mut b: Option<(u32, u32, u32, u32)> = None;
        for (x, y, p) in image.enumerate_pixels() {
            if p[0] >= PRESENT {
                b = Some(b.map_or((x, y, x, y), |(x0, y0, x1, y1)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y))));
            }
        }
        let bounds = b.map(|(x0, y0, x1, y1)| {
            [x0 as f64 / w as f64, y0 as f64 / h as f64, (x1 + 1) as f64 / w as f64, (y1 + 1) as f64 / h as f64]
        });
        SkyMap { image: Arc::new(image), bounds }
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

    /// กรอบของท้องฟ้า (สัดส่วนบนภาพเดิม) — None = ไม่พบท้องฟ้า
    pub fn bounds(&self) -> Option<[f64; 4]> {
        self.bounds
    }

    pub fn image(&self) -> &GrayImage {
        &self.image
    }
}

/// ภาพน้ำหนักเป็นของรูป ไม่ใช่ของค่าที่แต่ง
impl PartialEq for SkyMap {
    fn eq(&self, _: &SkyMap) -> bool {
        true
    }
}

impl std::fmt::Debug for SkyMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SkyMap({}×{})", self.image.width(), self.image.height())
    }
}

/// หาท้องฟ้าของภาพ `base` (ด้านยาวไม่เกิน [`SKY_EDGE`]) · `people` = ภาพน้ำหนักตัวคนของรูปเดียวกัน (ไม่นับเป็นท้องฟ้า)
pub fn compute(base: &RgbImage, people: &SubjectMap, segmenter: &dyn SkySegmenter) -> Result<SkyMap> {
    let (w, h) = base.dimensions();
    let sure = confident(&segmenter.segment(base)?);
    let coarse = imageops::resize(&sure, w, h, imageops::FilterType::Triangle);
    let input: Vec<f32> = coarse.as_raw().iter().map(|&v| f32::from(v) / 255.0).collect();
    let radius = ((w.max(h) as f64 * EDGE_RADIUS).round() as usize).max(1);
    let refined = by_colour(base, &input, radius);
    let image = GrayImage::from_fn(w, h, |x, y| {
        let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
        let sky = f64::from(refined[(y * w + x) as usize].clamp(0.0, 1.0)) * (1.0 - people.weight(u, v));
        image::Luma([(sky * 255.0).round() as u8])
    });
    Ok(SkyMap::new(image))
}

/// เก็บเฉพาะกลุ่มที่มั่นใจว่าเป็นท้องฟ้า (ค่ากลางของกลุ่มอย่างน้อย [`CONFIDENT`]) พร้อมขอบนุ่มรอบกลุ่ม
fn confident(prob: &GrayImage) -> GrayImage {
    let (w, h) = prob.dimensions();
    let (wu, hu) = (w as usize, h as usize);
    let raw = prob.as_raw();
    let mut label = vec![0u32; wu * hu];
    let mut keep = vec![false; wu * hu];
    let mut next = 0u32;
    let mut stack = Vec::new();
    for start in 0..wu * hu {
        if raw[start] < PART_OF || label[start] != 0 {
            continue;
        }
        next += 1;
        label[start] = next;
        stack.push(start);
        let (mut members, mut hist) = (Vec::new(), [0u32; 256]);
        while let Some(i) = stack.pop() {
            members.push(i);
            hist[raw[i] as usize] += 1;
            let (x, y) = (i % wu, i / wu);
            let mut visit = |j: usize| {
                if raw[j] >= PART_OF && label[j] == 0 {
                    label[j] = next;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < wu {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - wu);
            }
            if y + 1 < hu {
                visit(i + wu);
            }
        }
        if median(&hist, members.len()) >= CONFIDENT {
            for i in members {
                keep[i] = true;
            }
        }
    }
    // ขยายกลุ่มที่เก็บ (ระยะแบบกระดานหมากรุก) แล้วใช้ค่าเดิมของโมเดลในบริเวณนั้น
    let mut near = keep.clone();
    for _ in 0..GROW {
        let prev = near.clone();
        for y in 0..hu {
            for x in 0..wu {
                let i = y * wu + x;
                near[i] = prev[i]
                    || (x > 0 && prev[i - 1])
                    || (x + 1 < wu && prev[i + 1])
                    || (y > 0 && prev[i - wu])
                    || (y + 1 < hu && prev[i + wu]);
            }
        }
    }
    GrayImage::from_fn(w, h, |x, y| {
        let i = (y * w + x) as usize;
        image::Luma([if near[i] { raw[i] } else { 0 }])
    })
}

fn median(hist: &[u32; 256], count: usize) -> u8 {
    let half = count.div_ceil(2) as u32;
    let mut seen = 0;
    for (v, &n) in hist.iter().enumerate() {
        seen += n;
        if seen >= half {
            return v as u8;
        }
    }
    255
}

/// ค่าเฉลี่ยในกรอบ (2r+1)² รอบแต่ละพิกเซล (ขอบภาพเฉลี่ยเท่าที่มี) — ผลรวมสะสมทีละแกน
fn box_mean(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut rows = vec![0f32; w * h];
    let mut acc = vec![0f64; w + 1];
    for y in 0..h {
        for x in 0..w {
            acc[x + 1] = acc[x] + f64::from(src[y * w + x]);
        }
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r + 1).min(w));
            rows[y * w + x] = ((acc[b] - acc[a]) / (b - a) as f64) as f32;
        }
    }
    let mut out = vec![0f32; w * h];
    let mut acc = vec![0f64; h + 1];
    for x in 0..w {
        for y in 0..h {
            acc[y + 1] = acc[y] + f64::from(rows[y * w + x]);
        }
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r + 1).min(h));
            out[y * w + x] = ((acc[b] - acc[a]) / (b - a) as f64) as f32;
        }
    }
    out
}

/// ขอบตามสี: ตรงที่ไม่แน่ใจ (ระหว่าง [`SURE_GROUND`] กับ [`SURE_SKY`]) ฉายสีของพิกเซลลงบนเส้นจากสีเฉลี่ยของส่วนที่ไม่ใช่ฟ้า
/// ไปสีเฉลี่ยของฟ้าในกรอบรอบ ๆ — สีสองฝั่งใกล้กันหรือไม่เห็นทั้งสองฝั่ง = ใช้ค่าเดิมของโมเดล
fn by_colour(base: &RgbImage, p: &[f32], r: usize) -> Vec<f32> {
    let (w, h) = (base.width() as usize, base.height() as usize);
    let sky: Vec<f32> = p.iter().map(|&v| if v >= SURE_SKY { 1.0 } else { 0.0 }).collect();
    let ground: Vec<f32> = p.iter().map(|&v| if v <= SURE_GROUND { 1.0 } else { 0.0 }).collect();
    let (seen_sky, seen_ground) = (box_mean(&sky, w, h, r), box_mean(&ground, w, h, r));
    let raw = base.as_raw();
    let side = |c: usize, of: &[f32]| {
        let v: Vec<f32> = (0..w * h).map(|i| f32::from(raw[i * 3 + c]) / 255.0 * of[i]).collect();
        box_mean(&v, w, h, r)
    };
    let sky_rgb: [Vec<f32>; 3] = std::array::from_fn(|c| side(c, &sky));
    let ground_rgb: [Vec<f32>; 3] = std::array::from_fn(|c| side(c, &ground));
    (0..w * h)
        .map(|i| {
            if p[i] >= SURE_SKY || p[i] <= SURE_GROUND {
                return p[i];
            }
            let (ns, ng) = (seen_sky[i].max(1e-6), seen_ground[i].max(1e-6));
            let (mut along, mut apart2) = (0f32, 0f32);
            for c in 0..3 {
                let (s, g) = (sky_rgb[c][i] / ns, ground_rgb[c][i] / ng);
                let px = f32::from(raw[i * 3 + c]) / 255.0;
                along += (px - g) * (s - g);
                apart2 += (s - g) * (s - g);
            }
            let alpha = (along / apart2.max(1e-6)).clamp(0.0, 1.0);
            let trust = ((seen_sky[i].min(seen_ground[i]) - SEEN) / SEEN_FULL).clamp(0.0, 1.0)
                * ((apart2.sqrt() - APART) / APART_FULL).clamp(0.0, 1.0);
            trust * alpha + (1.0 - trust) * p[i]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb};

    /// ตัวแยกจำลอง: ส่งค่าที่กำหนดไว้กลับ (ไม่ดูภาพ)
    struct Fixed(GrayImage);
    impl SkySegmenter for Fixed {
        fn segment(&self, _: &RgbImage) -> Result<GrayImage> {
            Ok(self.0.clone())
        }
    }

    fn no_people(w: u32, h: u32) -> SubjectMap {
        SubjectMap::new(GrayImage::new(w, h))
    }

    #[test]
    fn a_confident_sky_follows_the_real_horizon_not_the_coarse_one() {
        // ภาพ 1024×512: ฟ้าสว่างถึงแถว 200 · ตัวแยกทำงานที่ภาพเล็ก (ช่องละ 16 พิกเซล)
        // บอกฟ้าถึงแถว 208 — ขอบหยาบเกินเส้นขอบฟ้าจริงไปครึ่งช่อง แบบที่โมเดล 320 พิกเซลทำกับภาพ 2048
        let base = RgbImage::from_fn(1024, 512, |_, y| if y < 200 { Rgb([200, 220, 250]) } else { Rgb([60, 70, 40]) });
        let prob = GrayImage::from_fn(64, 32, |_, y| Luma([if y < 13 { 255 } else { 0 }]));
        let sky = compute(&base, &no_people(1024, 512), &Fixed(prob)).unwrap();
        let at = |row: f64| sky.weight(0.5, (row + 0.5) / 512.0);
        assert!(at(100.0) > 0.95, "ฟ้า {}", at(100.0));
        assert!(at(196.0) > 0.8, "ฟ้าชิดเส้นขอบฟ้า {}", at(196.0));
        assert!(at(204.0) < 0.1, "ใต้เส้นขอบฟ้าจริงไม่ใช่ฟ้า {}", at(204.0));
        assert!(at(400.0) < 0.02);
        let [x0, y0, x1, _] = sky.bounds().unwrap();
        assert_eq!((x0, y0, x1), (0.0, 0.0, 1.0));
    }

    #[test]
    fn an_unsure_region_like_a_white_wall_is_not_sky() {
        // ทั้งภาพได้ค่ากลาง ๆ (0.8) แบบผนังขาว: ไม่ใช่ท้องฟ้าเลย
        let base = RgbImage::from_pixel(100, 60, Rgb([240, 240, 240]));
        let prob = GrayImage::from_pixel(20, 12, Luma([204]));
        let sky = compute(&base, &no_people(100, 60), &Fixed(prob)).unwrap();
        assert_eq!(sky.bounds(), None);
        // กลุ่มมั่นใจกับกลุ่มไม่มั่นใจในภาพเดียวกัน: เก็บเฉพาะกลุ่มที่มั่นใจ
        let prob = GrayImage::from_fn(20, 12, |x, _| {
            Luma([if x < 8 {
                255
            } else if x >= 12 {
                200
            } else {
                0
            }])
        });
        let sure = confident(&prob);
        assert_eq!(sure.get_pixel(2, 6)[0], 255);
        assert_eq!(sure.get_pixel(18, 6)[0], 0, "กลุ่มที่ไม่มั่นใจถูกทิ้ง");
    }

    #[test]
    fn people_against_the_sky_are_not_sky() {
        // ฟ้าขาวทั้งภาพ ตัวคนอยู่ครึ่งขวา (เสื้อขาวสว่างเท่าฟ้า)
        let base = RgbImage::from_pixel(100, 60, Rgb([245, 245, 245]));
        let prob = GrayImage::from_pixel(20, 12, Luma([255]));
        let people = SubjectMap::new(GrayImage::from_fn(50, 30, |x, _| Luma([if x >= 25 { 255 } else { 0 }])));
        let sky = compute(&base, &people, &Fixed(prob)).unwrap();
        assert!(sky.weight(0.2, 0.5) > 0.95);
        assert!(sky.weight(0.8, 0.5) < 0.02, "ตัวคน {}", sky.weight(0.8, 0.5));
    }

    #[test]
    fn box_mean_averages_what_is_inside_the_photo() {
        let src: Vec<f32> = (0..5).map(|i| i as f32).collect();
        let m = box_mean(&src, 5, 1, 1);
        assert_eq!(m, vec![0.5, 1.0, 2.0, 3.0, 3.5]);
    }
}
