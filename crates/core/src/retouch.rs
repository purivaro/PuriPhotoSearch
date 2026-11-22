//! รีทัชใบหน้า (DECISIONS ข้อ 31): ผิวเนียน ลดริ้วรอย หน้าเรียว หน้าผอม — ทุกใบหน้าที่มีกรอบ ปรับได้หลายระดับ
//!
//! - **ผิวเนียน / ลดริ้วรอย:** แยกความถี่ของความสว่างบนผิวหน้า (frequency separation) ด้วยฐานแบบรักษาขอบสองขนาด
//!   ที่คิดจากความกว้างของ**แต่ละหน้า** (หน้าใกล้กล้องกับหน้าในรูปหมู่ได้ผลแบบเดียวกัน): รายละเอียดละเอียด (รูขุมขน ผิวไม่เรียบ)
//!   ลดตามผิวเนียน · รายละเอียดขนาดกลาง (ริ้วรอย ร่องเงา) ลดตามลดริ้วรอย · ขอบชัดอย่างตา คิ้ว ปาก ผม ไม่ถูกเบลอ (ฐานรักษาขอบ)
//!   และคงรายละเอียดไว้บางส่วนไม่ให้ผิวเหมือนพลาสติก · มีผลเฉพาะผิวของคนนั้น (วงรีของหน้า × สีผิว × ใกล้สีผิวของคนนั้น — แบบเดียวกับ
//!   Mask ใบหน้า "เฉพาะผิว")
//! - **หน้าเรียว / หน้าผอม:** ดันขอบหน้าเข้าหาเส้นกลางหน้าแบบนุ่ม เฉพาะแถบรอบขอบหน้า — กรามถึงคาง (เรียว) และแก้ม (ผอม) ·
//!   ตา จมูก ปากไม่ขยับ · คิดบนภาพเดิมก่อนตัด/หมุน: ตัวอย่างสดใส่ตอนสุ่มภาพ ไฟล์ที่บันทึกทำเป็นขั้นแรก ได้ภาพเดียวกัน
//!
//! กรอบใบหน้าเก็บเป็นสัดส่วนบนภาพเดิมในการแต่ง (แบบ Mask) — ลบ/แก้กรอบใบหน้าภายหลังไม่ทำให้การแต่งเสีย

use crate::edit::Geometry;
use crate::error::{AppError, Result};
use crate::masks::{Area, MAX_FACES};
use image::{DynamicImage, ImageBuffer, Pixel};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// ระดับสำเร็จรูปของแต่ละแถบ (เบา กลาง มาก)
pub const LEVELS: [f64; 3] = [25.0, 50.0, 80.0];

/// ฐานของผิวเนียน: รัศมี (สัดส่วนของความกว้างหน้า) และ ε (ความต่างของความสว่างที่ถือเป็นขอบ — ยกกำลังสอง)
const SMOOTH_RADIUS: f64 = 0.025;
const SMOOTH_EPS: f64 = 0.0016;
/// ฐานของลดริ้วรอย
const WRINKLE_RADIUS: f64 = 0.08;
const WRINKLE_EPS: f64 = 0.0064;
/// สัดส่วนที่ลดได้มากสุด (คงรายละเอียดที่เหลือไว้ ไม่ให้ผิวเรียบจนเหมือนพลาสติก)
const SMOOTH_MAX: f64 = 0.75;
const WRINKLE_MAX: f64 = 0.85;
/// หน้าเล็กกว่านี้ (พิกเซลของภาพที่แต่ง) ไม่ทำผิวเนียน — ไม่มีรายละเอียดให้ลด
const MIN_FACE_PX: f64 = 16.0;

/// หน้าเรียว/หน้าผอม: ดันตามแนวนอนเข้าหาเส้นกลางหน้า เฉพาะแถบรอบขอบหน้า — ระยะจากเส้นกลางเทียบกับครึ่งความกว้างของหน้า
/// ที่ระดับนั้น: ไม่ขยับเลยในส่วนใน (ตา จมูก ปาก) แรงสุดที่ขอบหน้า แล้วหมดนอกหน้า (ฉากหลังขยับแค่ใกล้ขอบหน้า)
const INNER: f64 = 0.45;
const OUTER: f64 = 1.7;
/// ขอบหน้าแคบลงจากแก้มถึงคาง (ครึ่งวงรีใต้ระดับแก้ม — กรอบของตัววิเคราะห์ใบหน้ากว้างเท่าแก้มและลงไปถึงใต้คาง):
/// จุดเริ่มแคบ และรัศมีตามแนวตั้ง (สัดส่วนของความสูงกรอบ) · ไม่แคบกว่าสัดส่วนนี้ของครึ่งความกว้าง
const CONTOUR_FROM: f64 = 0.45;
const CONTOUR_SPAN: f64 = 0.6;
const CONTOUR_MIN: f64 = 0.3;
/// แถบตามแนวตั้ง (เริ่ม, แรงสุด, หมด — สัดส่วนของความสูงกรอบ) และระยะดันสูงสุด (สัดส่วนของครึ่งความกว้างหน้า)
/// · เรียว = กรามถึงคาง · ผอม = แก้ม
const JAW_BAND: (f64, f64, f64) = (0.5, 0.85, 1.12);
const JAW_PUSH: f64 = 0.14;
const CHEEK_BAND: (f64, f64, f64) = (0.35, 0.6, 0.88);
const CHEEK_PUSH: f64 = 0.1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Retouch {
    /// กรอบ [x, y, w, h] ของแต่ละหน้าบนภาพเดิม
    pub faces: Vec<[f64; 4]>,
    /// ผิวเนียน 0–100
    pub smooth: f64,
    /// ลดริ้วรอย 0–100
    pub wrinkles: f64,
    /// หน้าเรียว (กรามไปทางคาง) 0–100
    pub slim: f64,
    /// หน้าผอม (แก้ม) 0–100
    pub thin: f64,
    /// ตัวผอม 0–100 (DECISIONS ข้อ 41 — ใช้ตัวแยกตัวคนและตัวเติมฉากหลัง ไม่ต้องมีกรอบใบหน้า)
    pub body: f64,
    /// ภาพน้ำหนักและฉากหลังที่เติมของรูป แนบตอนสร้างภาพ ([`crate::subject::attach_body`]) — ไม่ใช่ส่วนของค่าที่แต่ง ไม่ถูกเก็บ
    #[serde(skip)]
    pub plan: crate::body::Attached,
}

impl Retouch {
    /// มีผลกับภาพ
    pub fn is_active(&self) -> bool {
        (!self.faces.is_empty() && (self.smooth > 0.0 || self.wrinkles > 0.0 || self.slim > 0.0 || self.thin > 0.0))
            || self.body > 0.0
    }

    fn warps(&self) -> bool {
        !self.faces.is_empty() && (self.slim > 0.0 || self.thin > 0.0)
    }

    fn smooths(&self) -> bool {
        !self.faces.is_empty() && (self.smooth > 0.0 || self.wrinkles > 0.0)
    }

    pub fn validate(&self) -> Result<()> {
        let amount = |v: f64| v.is_finite() && (0.0..=100.0).contains(&v);
        let boxes = self.faces.len() <= MAX_FACES
            && self.faces.iter().all(|b| {
                b.iter().all(|v| v.is_finite())
                    && b[2] > 0.0
                    && b[3] > 0.0
                    && b[2] <= 2.0
                    && b[3] <= 2.0
                    && (-1.0..=2.0).contains(&b[0])
                    && (-1.0..=2.0).contains(&b[1])
            });
        if boxes && [self.smooth, self.wrinkles, self.slim, self.thin, self.body].into_iter().all(amount) {
            Ok(())
        } else {
            Err(AppError::invalid("ค่าของรีทัชใบหน้าไม่ถูกต้อง"))
        }
    }

    /// รีทัชเดียวกันสำหรับรูปอื่น (วางการแต่ง): ใช้ใบหน้าของรูปนั้น
    pub fn on_faces(&self, faces: &[[f64; 4]]) -> Retouch {
        Retouch { faces: faces.to_vec(), plan: Default::default(), ..self.clone() }
    }

    /// จุดบนภาพเดิม (สัดส่วน) ที่ภาพหลังหน้าเรียว/หน้าผอมนำสีมา — `aspect` = กว้าง/สูงของภาพเดิม
    /// (คิดระยะในหน่วยความสูงของภาพ ไม่ขึ้นกับความละเอียด) · ดันไม่เกินความชันของแถบ เนื้อภาพจึงไม่พับทับกัน
    pub fn source_point(&self, u: f64, v: f64, aspect: f64) -> (f64, f64) {
        if !self.warps() {
            return (u, v);
        }
        let mut x = u * aspect;
        for b in &self.faces {
            let (bx, bw) = (b[0] * aspect, b[2] * aspect);
            let ty = (v - b[1]) / b[3];
            // ครึ่งความกว้างของหน้าที่ระดับนี้
            let narrow = ((ty - CONTOUR_FROM).max(0.0) / CONTOUR_SPAN).min(1.0);
            let half = bw / 2.0 * (1.0 - narrow * narrow).sqrt().max(CONTOUR_MIN);
            let cx = bx + bw / 2.0;
            let t = (x - cx).abs() / half;
            let across = smoothstep(INNER, 1.0, t) * (1.0 - smoothstep(1.0, OUTER, t));
            if across <= 0.0 {
                continue;
            }
            let push = self.slim / 100.0 * band(ty, JAW_BAND) * JAW_PUSH
                + self.thin / 100.0 * band(ty, CHEEK_BAND) * CHEEK_PUSH;
            // ภาพผลนำสีมาจากด้านนอกกว่า = ขอบหน้าขยับเข้า
            x += (x - cx).signum() * push * across * half;
        }
        (x / aspect, v)
    }
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// น้ำหนักของแถบตามแนวตั้ง: ค่อย ๆ ขึ้นถึงจุดแรงสุดแล้วค่อย ๆ ลง
fn band(t: f64, (start, peak, end): (f64, f64, f64)) -> f64 {
    if t < peak {
        smoothstep(start, peak, t)
    } else {
        1.0 - smoothstep(peak, end, t)
    }
}

/// ตัวผอม แล้วหน้าเรียว/หน้าผอม บนภาพเดิม (ขั้นแรกของภาพที่บันทึก ก่อนตัด/หมุน) — ไม่มีผล = ภาพเดิม
/// · ภาพทศนิยมคือแสงเชิงเส้นของ RAW ([`crate::body::Space::RawLinear`])
pub(crate) fn warp_image(img: DynamicImage, retouch: Option<&Retouch>) -> DynamicImage {
    // ตัวผอม: รอบเดียวกับหน้าเรียว ลำดับเดียวกับตัวอย่างสด (จุดของหน้าเรียว → ตัวผอมที่จุดนั้น) ได้ภาพเดียวกันทุกพิกเซล
    if let Some(r) = retouch {
        if let Some(plan) = r.plan.0.as_ref().filter(|_| r.body > 0.0) {
            let aspect = img.width() as f64 / img.height() as f64;
            return crate::body::apply(img, r.body, plan, |u, v| r.source_point(u, v, aspect));
        }
    }
    let Some(r) = retouch.filter(|r| r.warps()) else { return img };
    match img {
        DynamicImage::ImageRgb8(src) => DynamicImage::ImageRgb8(warp(&src, r)),
        DynamicImage::ImageRgba8(src) => DynamicImage::ImageRgba8(warp(&src, r)),
        DynamicImage::ImageRgb32F(src) => DynamicImage::ImageRgb32F(warp(&src, r)),
        other => DynamicImage::ImageRgba8(warp(&other.to_rgba8(), r)),
    }
}

/// ทุกพิกเซลย้อนไปหาจุดต้นทางของ [`Retouch::source_point`] แล้วผสมสี่พิกเซลรอบ ๆ (bilinear) — แบบเดียวกับการสุ่มของตัวอย่างสด
fn warp<P>(src: &ImageBuffer<P, Vec<P::Subpixel>>, r: &Retouch) -> ImageBuffer<P, Vec<P::Subpixel>>
where
    P: Pixel + Send + Sync,
    P::Subpixel: crate::edit::Channel + Send + Sync,
{
    use crate::edit::Channel;
    let n = P::CHANNEL_COUNT as usize;
    let (w, h) = (src.width() as usize, src.height() as usize);
    let aspect = w as f64 / h as f64;
    let raw = src.as_raw();
    let mut out = raw.clone();
    out.par_chunks_mut(w * n).enumerate().for_each(|(y, line)| {
        for x in 0..w {
            let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
            let (su, sv) = r.source_point(u, v, aspect);
            if (su, sv) == (u, v) {
                continue;
            }
            let fx = (su * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
            let fy = (sv * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
            let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
            let at = |xx: usize, yy: usize, c: usize| raw[(yy * w + xx) * n + c].get();
            for c in 0..n {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
                let bottom = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
                line[x * n + c] = Channel::put(top * (1.0 - ty) + bottom * ty);
            }
        }
    });
    ImageBuffer::from_raw(w as u32, h as u32, out).expect("ขนาดเท่าเดิม")
}

/// ผิวเนียน/ลดริ้วรอยในที่ บนภาพที่แต่งแล้วทั้งกรอบ (w×h พิกเซลละ `stride` ไบต์ หลังแสงสี ก่อน Mask) · `g` แปลงพิกัดกลับไปภาพเดิม
pub fn smooth_faces(px: &mut [u8], w: usize, h: usize, stride: usize, g: &Geometry, retouch: Option<&Retouch>) {
    let Some(r) = retouch.filter(|r| r.smooths()) else { return };
    if w == 0 || h == 0 || px.len() < w * h * stride {
        return;
    }
    let long = g.frame_long(w as u32, h as u32);
    for b in &r.faces {
        // ความกว้างของหน้าบนภาพนี้ (พิกเซล) — รัศมีของฐานคิดจากค่านี้
        let cy = b[1] + b[3] / 2.0;
        let (p0, p1) = (g.forward(b[0], cy), g.forward(b[0] + b[2], cy));
        let face_px = ((p1.0 - p0.0) * w as f64).hypot((p1.1 - p0.1) * h as f64);
        if face_px < MIN_FACE_PX {
            continue;
        }
        let area = Area::Faces { boxes: vec![*b], skin: true };
        let Some((x0, y0, x1, y1)) = crate::masks::bounds(&area, g, w, h, long) else { continue };
        let (rw, rh) = (x1 - x0, y1 - y0);
        let tones = crate::masks::face_tones(px, w, h, stride, g, &area);
        let tone = tones.first().copied().flatten();
        let y: Vec<f32> = (0..rw * rh)
            .into_par_iter()
            .map(|k| {
                let i = ((y0 + k / rw) * w + x0 + k % rw) * stride;
                ((0.2126 * px[i] as f64 + 0.7152 * px[i + 1] as f64 + 0.0722 * px[i + 2] as f64) / 255.0) as f32
            })
            .collect();
        let fine = crate::local::edge_preserving(&y, rw, rh, (SMOOTH_RADIUS * face_px).round() as usize, SMOOTH_EPS);
        let mid = crate::local::edge_preserving(&y, rw, rh, (WRINKLE_RADIUS * face_px).round() as usize, WRINKLE_EPS);
        let keep_fine = 1.0 - SMOOTH_MAX * r.smooth / 100.0;
        let keep_mid = 1.0 - WRINKLE_MAX * r.wrinkles / 100.0;
        px[y0 * w * stride..y1 * w * stride].par_chunks_mut(w * stride).enumerate().for_each(|(row, line)| {
            for col in 0..rw {
                let k = row * rw + col;
                let i = (x0 + col) * stride;
                let rgb = [line[i], line[i + 1], line[i + 2]];
                let (u, v) = g.inverse(((x0 + col) as f64 + 0.5) / w as f64, ((y0 + row) as f64 + 0.5) / h as f64);
                let weight = crate::masks::face_weight(b, u, v)
                    * crate::masks::skin_weight(rgb)
                    * crate::masks::tone_match(tone, rgb);
                if weight <= 0.0 {
                    continue;
                }
                let (l, f, m) = (y[k] as f64, fine[k] as f64, mid[k] as f64);
                let new = m + (f - m) * keep_mid + (l - f) * keep_fine;
                let delta = (new - l) * 255.0 * weight;
                for c in 0..3 {
                    line[i + c] = (rgb[c] as f64 + delta).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::EditSettings;
    use image::RgbImage;

    fn face(level: f64) -> Retouch {
        Retouch { faces: vec![[0.3, 0.2, 0.4, 0.6]], smooth: level, wrinkles: level, ..Default::default() }
    }

    /// ภาพ 200×200: ฉากเทา + "หน้า" สีผิวมีลายละเอียด (รูขุมขน ±6) และร่องเข้ม (ริ้วรอย) กับตาสีเข้มขอบชัด
    fn portrait() -> RgbImage {
        RgbImage::from_fn(200, 200, |x, y| {
            let (dx, dy) = ((x as f64 - 100.0) / 70.0, (y as f64 - 105.0) / 90.0);
            if dx * dx + dy * dy > 1.0 {
                return image::Rgb([120, 120, 122]);
            }
            let eye = ((x as i32 - 78).pow(2) + (y as i32 - 85).pow(2) < 64)
                || ((x as i32 - 122).pow(2) + (y as i32 - 85).pow(2) < 64);
            if eye {
                return image::Rgb([30, 25, 25]);
            }
            let pore = ((x * 7 + y * 13) % 5) as i32 * 3 - 6;
            let wrinkle = if (y as i32 - 130).abs() < 2 && (60..140).contains(&x) { -18 } else { 0 };
            let v = |c: i32| (c + pore + wrinkle).clamp(0, 255) as u8;
            image::Rgb([v(205), v(150), v(125)])
        })
    }

    fn run(img: &RgbImage, r: &Retouch) -> RgbImage {
        let s = EditSettings { retouch: Some(r.clone()), ..Default::default() };
        let g = Geometry::new(img.width(), img.height(), &s);
        let mut px = img.as_raw().clone();
        smooth_faces(&mut px, img.width() as usize, img.height() as usize, 3, &g, Some(r));
        RgbImage::from_raw(img.width(), img.height(), px).unwrap()
    }

    fn spread(img: &RgbImage, x0: u32, x1: u32, y0: u32, y1: u32) -> f64 {
        let v: Vec<f64> = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .map(|(x, y)| img.get_pixel(x, y).0[1] as f64)
            .collect();
        let m = v.iter().sum::<f64>() / v.len() as f64;
        (v.iter().map(|a| (a - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    }

    #[test]
    fn smoothing_evens_the_skin_and_softens_wrinkles_but_keeps_eyes_and_background() {
        let img = portrait();
        let out = run(&img, &face(80.0));
        // ผิวบริเวณแก้ม: ลายละเอียดลดลงมาก
        assert!(spread(&out, 85, 115, 100, 120) < spread(&img, 85, 115, 100, 120) * 0.6);
        // ร่องริ้วรอยตื้นขึ้น (ห่างจากผิวรอบ ๆ น้อยลง)
        let depth = |im: &RgbImage| im.get_pixel(100, 122).0[1] as f64 - im.get_pixel(100, 130).0[1] as f64;
        assert!(depth(&out) < depth(&img) * 0.7, "{} → {}", depth(&img), depth(&out));
        // ตาเข้มเท่าเดิม (ขอบชัด ไม่ใช่สีผิว) · ฉากหลังไม่โดน
        assert!(out.get_pixel(78, 85).0[0].abs_diff(30) <= 2);
        assert_eq!(out.get_pixel(5, 5), img.get_pixel(5, 5));
        // ระดับมากกว่า = เนียนกว่า · 0 = ภาพเดิม
        assert!(spread(&run(&img, &face(25.0)), 85, 115, 100, 120) > spread(&out, 85, 115, 100, 120));
        assert_eq!(run(&img, &face(0.0)), img);
    }

    #[test]
    fn slimming_pulls_the_jaw_and_cheeks_in_and_leaves_the_rest() {
        let r = Retouch { faces: vec![[0.3, 0.2, 0.4, 0.6]], slim: 80.0, thin: 80.0, ..Default::default() };
        // จุดที่ขอบกรามซ้ายนำสีมาจากด้านนอก (ขอบหน้าจึงขยับเข้า) · กลางหน้าและฉากไกล ๆ อยู่ที่เดิม
        let (u, v) = (0.3 + 0.4 * 0.12, 0.2 + 0.6 * 0.78);
        let (su, _) = r.source_point(u, v, 1.0);
        assert!(su < u - 0.005, "{su} < {u}");
        let (ru, _) = r.source_point(0.3 + 0.4 * 0.88, v, 1.0);
        assert!(ru > 0.3 + 0.4 * 0.88 + 0.005);
        assert_eq!(r.source_point(0.05, 0.05, 1.0), (0.05, 0.05));
        // ภาพจริง: กรอบตรงกับ "หน้า" ที่วาด (x 30–170, y 15–195) — ความกว้างที่ระดับกรามและแก้มแคบลง ตาไม่ขยับ
        let img = portrait();
        let fit = Retouch { faces: vec![[0.15, 0.075, 0.7, 0.9]], ..r.clone() };
        let out = warp_image(DynamicImage::ImageRgb8(img.clone()), Some(&fit)).into_rgb8();
        let width = |im: &RgbImage, y: u32| (0..200).filter(|&x| im.get_pixel(x, y).0[0] > 160).count();
        for y in [110, 155] {
            assert!(width(&out, y) + 4 <= width(&img, y), "y {y}: {} → {}", width(&img, y), width(&out, y));
        }
        assert_eq!(out.get_pixel(78, 85), img.get_pixel(78, 85), "ตาอยู่ที่เดิม");
        assert_eq!(out.get_pixel(5, 5), img.get_pixel(5, 5));
        // ไม่ดัด = ภาพเดิมทุกพิกเซล
        let none = Retouch { slim: 0.0, thin: 0.0, ..r };
        assert_eq!(warp_image(DynamicImage::ImageRgb8(img.clone()), Some(&none)).into_rgb8(), img);
    }

    #[test]
    fn validation_rejects_bad_values() {
        assert!(face(50.0).validate().is_ok());
        assert!(Retouch { smooth: 101.0, ..face(0.0) }.validate().is_err());
        assert!(Retouch { faces: vec![[0.1, 0.1, f64::NAN, 0.2]], ..face(10.0) }.validate().is_err());
        assert!(!Retouch { faces: vec![], ..face(50.0) }.is_active());
    }
}
