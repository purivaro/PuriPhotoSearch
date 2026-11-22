//! ลบคนหรือสิ่งของ (DECISIONS ข้อ 42): ผู้ใช้ลากกรอบรอบสิ่งที่ไม่ต้องการ (หรือเลือกจากใบหน้าที่แอปพบ) แอปหาตัวคน/สิ่งของในกรอบ
//! ด้วยตัวแยกตัวคน (ข้อ 40) แล้วเติมฉากหลังแทนด้วยตัวเติมฉากหลัง (ข้อ 41) — ไม่แตะต้นฉบับ
//!
//! - ค่าที่แต่งเก็บแค่กรอบ (สัดส่วนบนภาพเดิมก่อนตัด/หมุน) · ภาพที่เติมเป็นของรูป เก็บใน cache ตาม revision ภาพตั้งต้น และกรอบ
//!   แนบให้ค่าที่แต่งตอนสร้างภาพ ([`crate::subject::attach_all`])
//! - ทำก่อนทุกอย่างบนภาพเดิม (ก่อนตัวผอม หน้าเรียว ตัด/หมุน) — ตัวอย่างสด ภาพที่บันทึก และภาพจาก RAW คู่ ใช้ [`apply`] ตัวเดียวกัน
//! - ในกรอบไม่พบสิ่งที่เด่น = ลบทั้งกรอบ (วงรีในกรอบ)

use crate::body::Space;
use crate::error::{AppError, Result};
use crate::subject::{Inpainter, Segmenter};
use image::{imageops, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// ด้านยาวของภาพตั้งต้นที่ใช้หาสิ่งที่จะลบและเติมฉาก
pub const REMOVE_EDGE: u32 = 2048;
/// ลบได้มากสุดต่อรูป
pub const MAX_REMOVALS: usize = 20;
/// จุดของเส้นที่ระบายได้มากสุดต่อส่วน
pub const MAX_BRUSH_POINTS: usize = 2000;
/// ฉากรอบกรอบที่ส่งไปด้วย (สัดส่วนของด้านยาวของกรอบ) — ตัวเติมต้องเห็นฉากรอบ ๆ
const CONTEXT: f64 = 0.6;
/// หาสิ่งที่เด่นในกรอบที่ขยายออกเท่านี้ (คนที่ล้นกรอบที่ลากไว้เล็กน้อยยังถูกลบครบ)
const SEARCH: f64 = 0.15;
/// น้ำหนักของตัวแยกที่นับว่าเป็นสิ่งที่จะลบ (ขอบนุ่ม) และที่นับว่า "พบ"
const SOFT: u8 = 8;
const SOLID: u8 = 128;
/// ในกรอบต้องเป็นสิ่งที่เด่นอย่างน้อยสัดส่วนนี้ ไม่งั้นลบทั้งกรอบ
const FOUND: f64 = 0.04;
/// ขยายส่วนที่ลบออก (สัดส่วนของด้านยาวของกรอบ)
const GROW: f64 = 0.03;

/// ส่วนหนึ่งที่จะลบ (ค่าที่แต่ง)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    pub id: String,
    /// กรอบ [x, y, w, h] บนภาพเดิม (สัดส่วน ก่อนตัด/หมุน)
    pub rect: [f64; 4],
    /// จุดยึด (กลางใบหน้า — เลือกจากใบหน้าที่แอปพบ): ลบเฉพาะสิ่งที่ติดกับจุดนี้ ของอื่นในกรอบไม่ถูกลบ
    /// (กรอบของคนที่คิดจากใบหน้ามักกินถึงของบนโต๊ะด้านหน้า — ลองกับรูปจริงแล้ว) · ไม่มี = ทุกสิ่งที่เด่นในกรอบ
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[f64; 2]>,
    /// ระบายด้วยแปรง: จุดตามเส้นที่ลาก [x, y, รัศมี] (x, y สัดส่วนบนภาพเดิม · รัศมีเป็นสัดส่วนของด้านยาว) — ลบตรงที่ระบายทั้งหมด
    /// ไม่หาสิ่งที่เด่น (ของเล็ก ๆ สายไฟ ขยะบนพื้น) · ว่าง = ใช้กรอบ
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brush: Vec<[f64; 3]>,
    /// ภาพที่เติมแล้ว แนบตอนสร้างภาพ — ไม่ใช่ส่วนของค่าที่แต่ง ไม่ถูกเก็บ
    #[serde(skip)]
    pub patch: Attached,
}

/// ภาพที่เติมแล้วของกรอบหนึ่ง: ครอบส่วน `region` (สัดส่วนบนภาพเดิม) · alpha = น้ำหนักที่ใช้ภาพนี้แทนภาพเดิม
#[derive(Clone)]
pub struct Patch {
    region: [f64; 4],
    image: Arc<RgbaImage>,
}

impl Patch {
    pub fn new(region: [f64; 4], image: RgbaImage) -> Patch {
        Patch { region, image: Arc::new(image) }
    }

    pub fn region(&self) -> [f64; 4] {
        self.region
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// สีที่เติมไว้ที่จุด (u, v) บนภาพเดิม (0–255 ของภาพตั้งต้น) และน้ำหนัก 0–1 — None = นอกส่วนนี้หรือไม่ได้เติมตรงนั้น
    pub fn at(&self, u: f64, v: f64) -> Option<([f64; 3], f64)> {
        let [rx, ry, rw, rh] = self.region;
        let (x, y) = ((u - rx) / rw, (v - ry) / rh);
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        let (w, h) = self.image.dimensions();
        let fx = (x * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let fy = (y * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let (mut rgb, mut weight) = ([0.0f64; 3], 0.0f64);
        for (xx, yy, k) in
            [(x0, y0, (1.0 - tx) * (1.0 - ty)), (x1, y0, tx * (1.0 - ty)), (x0, y1, (1.0 - tx) * ty), (x1, y1, tx * ty)]
        {
            let p = self.image.get_pixel(xx, yy).0;
            let a = k * p[3] as f64 / 255.0;
            for c in 0..3 {
                rgb[c] += a * p[c] as f64;
            }
            weight += a;
        }
        (weight > 1e-6).then(|| (rgb.map(|c| c / weight), weight))
    }
}

/// ภาพที่เติมที่แนบให้ค่าที่แต่ง — ไม่ใช่ส่วนของค่าที่แต่ง: ค่าที่แต่งสองชุดเท่ากันไม่ว่าจะแนบหรือไม่
#[derive(Clone, Default)]
pub struct Attached(pub Option<Patch>);

impl PartialEq for Attached {
    fn eq(&self, _: &Attached) -> bool {
        true
    }
}

impl std::fmt::Debug for Attached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Patch({})", if self.0.is_some() { "แนบแล้ว" } else { "ยังไม่แนบ" })
    }
}

pub fn validate(removals: &[Removal]) -> Result<()> {
    let ok = removals.len() <= MAX_REMOVALS
        && removals.iter().all(|r| {
            let [x, y, w, h] = r.rect;
            !r.id.is_empty()
                && r.id.len() <= 64
                && r.rect.iter().all(|v| v.is_finite())
                && w >= 0.005
                && h >= 0.005
                && w <= 1.05
                && h <= 1.05
                && (-0.05..=1.05).contains(&x)
                && (-0.05..=1.05).contains(&y)
                && r.anchor.is_none_or(|[ax, ay]| (0.0..=1.0).contains(&ax) && (0.0..=1.0).contains(&ay))
                && r.brush.len() <= MAX_BRUSH_POINTS
                && r.brush.iter().all(|&[bx, by, br]| {
                    (-0.1..=1.1).contains(&bx) && (-0.1..=1.1).contains(&by) && br > 0.0 && br <= 0.3
                })
        });
    if ok {
        Ok(())
    } else {
        Err(AppError::invalid("ส่วนที่จะลบไม่ถูกต้อง"))
    }
}

/// มีภาพที่เติมแนบไว้อย่างน้อยหนึ่งส่วน (มีผลกับภาพ)
pub fn active(removals: &[Removal]) -> bool {
    removals.iter().any(|r| r.patch.0.is_some())
}

/// สีที่จุด (u, v) บนภาพเดิมหลังลบ: `here` = สีเดิมที่จุดนั้น (หน่วยของภาพที่กำลังสร้าง — `space`)
pub fn apply<const N: usize>(
    removals: &[Removal],
    u: f64,
    v: f64,
    mut here: [f64; N],
    space: Space,
    alpha_max: f64,
) -> [f64; N] {
    for r in removals {
        if let Some((rgb, a)) = r.patch.0.as_ref().and_then(|p| p.at(u, v)) {
            let fill: [f64; N] = space.convert(rgb, alpha_max);
            here = std::array::from_fn(|c| here[c] + (fill[c] - here[c]) * a);
        }
    }
    here
}

/// ภาพที่เติมของส่วนที่จะลบ จากภาพตั้งต้น `base` — คืนภาพที่เติม และพบสิ่งที่เด่นในกรอบไหม (ไม่พบ = ลบทั้งกรอบ · ระบาย = true)
/// · กรอบ: หาสิ่งที่เด่นในกรอบ (`anchor` = เฉพาะสิ่งที่ติดกับจุดนี้) · แปรง: ตรงที่ระบายทั้งหมด
pub fn compute(
    base: &RgbImage,
    r: &Removal,
    segmenter: &dyn Segmenter,
    inpainter: &dyn Inpainter,
) -> Result<(Patch, bool)> {
    let (w, h) = (base.width() as f64, base.height() as f64);
    let (anchor, long_side) = (r.anchor, w.max(h));
    // แปรง: กรอบคือส่วนที่ระบาย (รวมรัศมี)
    let rect = if r.brush.is_empty() {
        r.rect
    } else {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &[bx, by, br] in &r.brush {
            let (rx, ry) = (br * long_side / w, br * long_side / h);
            (x0, y0, x1, y1) = (x0.min(bx - rx), y0.min(by - ry), x1.max(bx + rx), y1.max(by + ry));
        }
        [x0, y0, x1 - x0, y1 - y0]
    };
    let px = |v: f64, size: f64| (v * size).round().clamp(0.0, size) as u32;
    // กรอบเป็นพิกเซล (อย่างน้อย 4×4)
    let (bx0, by0) = (px(rect[0], w), px(rect[1], h));
    let (bx1, by1) =
        (px(rect[0] + rect[2], w).max(bx0 + 4).min(w as u32), px(rect[1] + rect[3], h).max(by0 + 4).min(h as u32));
    if bx1 <= bx0 || by1 <= by0 {
        return Err(AppError::invalid("ส่วนที่จะลบอยู่นอกภาพ"));
    }
    let long = (bx1 - bx0).max(by1 - by0) as f64;
    let around = |m: f64| {
        let m = m.round() as u32;
        (bx0.saturating_sub(m), by0.saturating_sub(m), (bx1 + m).min(w as u32), (by1 + m).min(h as u32))
    };
    // ส่วนที่ส่งให้ตัวเติม (กรอบ + ฉากรอบ ๆ) และส่วนที่หาสิ่งที่เด่น
    let (rx0, ry0, rx1, ry1) = around((CONTEXT * long).max(16.0));
    let (sx0, sy0, sx1, sy1) = around(SEARCH * long);
    let region = imageops::crop_imm(base, rx0, ry0, rx1 - rx0, ry1 - ry0).to_image();
    let (rw, rh) = region.dimensions();
    if !r.brush.is_empty() {
        // ตรงที่ระบาย: ระยะถึงเส้นที่ลากไม่เกินรัศมี (พิกเซลของภาพตั้งต้น)
        let mut target = vec![false; (rw * rh) as usize];
        let pts: Vec<(f64, f64, f64)> = r.brush.iter().map(|&[bx, by, br]| (bx * w, by * h, br * long_side)).collect();
        let segments = pts.windows(2).map(|p| (p[0], p[1])).chain(pts.first().map(|&p| (p, p)));
        let mut widest = 0.0f64;
        for ((ax, ay, ar), (bx, by, br)) in segments {
            let rad = ar.max(br);
            widest = widest.max(rad);
            let (minx, maxx) = ((ax.min(bx) - rad).floor().max(rx0 as f64), (ax.max(bx) + rad).ceil().min(rx1 as f64));
            let (miny, maxy) = ((ay.min(by) - rad).floor().max(ry0 as f64), (ay.max(by) + rad).ceil().min(ry1 as f64));
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let mut y = miny;
            while y < maxy {
                let mut x = minx;
                while x < maxx {
                    let (px, py) = (x + 0.5, y + 0.5);
                    let t = if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                    let (cx, cy) = (ax + t * dx, ay + t * dy);
                    if (px - cx).powi(2) + (py - cy).powi(2) <= rad * rad {
                        target[((y as u32 - ry0) * rw + (x as u32 - rx0)) as usize] = true;
                    }
                    x += 1.0;
                }
                y += 1.0;
            }
        }
        let grow_px = ((0.25 * widest).round() as usize).max(2);
        let filled = crate::body::fill_holes(&region, &target, grow_px, inpainter)?
            .ok_or_else(|| AppError::invalid("ไม่มีส่วนที่ระบายอยู่ในภาพ"))?;
        let feathered = feather(&filled, (grow_px / 2).max(1));
        let region = [rx0 as f64 / w, ry0 as f64 / h, (rx1 - rx0) as f64 / w, (ry1 - ry0) as f64 / h];
        return Ok((Patch::new(region, feathered), true));
    }
    let search = imageops::crop_imm(base, sx0, sy0, sx1 - sx0, sy1 - sy0).to_image();
    let mut found_map = segmenter.segment(&search)?;
    if let Some([ax, ay]) = anchor {
        let at = ((ax * w).round() as i64 - sx0 as i64, (ay * h).round() as i64 - sy0 as i64);
        keep_touching(&mut found_map, at);
    }
    let inside_rect = |x: u32, y: u32| (bx0..bx1).contains(&x) && (by0..by1).contains(&y);
    let solid =
        found_map.enumerate_pixels().filter(|(x, y, p)| p[0] >= SOLID && inside_rect(sx0 + x, sy0 + y)).count() as f64;
    let found = solid / ((bx1 - bx0) * (by1 - by0)) as f64 >= FOUND;
    let mut target = vec![false; (rw * rh) as usize];
    if found {
        for (x, y, p) in found_map.enumerate_pixels() {
            if p[0] >= SOFT {
                target[((sy0 + y - ry0) * rw + (sx0 + x - rx0)) as usize] = true;
            }
        }
    } else {
        // ไม่พบสิ่งที่เด่น: ทั้งกรอบ (วงรีในกรอบ — มุมกรอบมักเป็นฉากหลัง)
        let (cx, cy) = ((bx0 + bx1) as f64 / 2.0, (by0 + by1) as f64 / 2.0);
        let (ax, ay) = ((bx1 - bx0) as f64 / 2.0, (by1 - by0) as f64 / 2.0);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let (dx, dy) = ((x as f64 + 0.5 - cx) / ax, (y as f64 + 0.5 - cy) / ay);
                if dx * dx + dy * dy <= 1.0 {
                    target[((y - ry0) * rw + (x - rx0)) as usize] = true;
                }
            }
        }
    }
    let grow_px = ((GROW * long).round() as usize).max(3);
    let filled = crate::body::fill_holes(&region, &target, grow_px, inpainter)?
        .ok_or_else(|| AppError::invalid("ไม่มีส่วนให้ลบในกรอบนี้"))?;
    // ขอบของส่วนที่เติมค่อย ๆ จางเข้าไปในภาพจริง (เฉพาะด้านใน — นอกส่วนที่เติมไม่มีสี)
    let feathered = feather(&filled, (grow_px / 2).max(1));
    let region = [rx0 as f64 / w, ry0 as f64 / h, (rx1 - rx0) as f64 / w, (ry1 - ry0) as f64 / h];
    Ok((Patch::new(region, feathered), found))
}

/// เหลือเฉพาะส่วนที่ติดกัน (ภาพน้ำหนักตั้งแต่ [`SOFT`]) ที่มีจุด `at` (พิกเซล) หรือใกล้จุดนั้นที่สุด — ส่วนอื่นเป็นศูนย์
fn keep_touching(map: &mut image::GrayImage, at: (i64, i64)) {
    let (w, h) = (map.width() as usize, map.height() as usize);
    let on: Vec<bool> = map.as_raw().iter().map(|&v| v >= SOFT).collect();
    // จุดเริ่ม: จุดยึด หรือพิกเซลที่เป็นส่วนของสิ่งที่เด่นที่ใกล้จุดยึดที่สุด
    let start = on
        .iter()
        .enumerate()
        .filter(|(_, &o)| o)
        .min_by_key(|(i, _)| {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            (x - at.0).pow(2) + (y - at.1).pow(2)
        })
        .map(|(i, _)| i);
    let mut keep = vec![false; w * h];
    if let Some(start) = start {
        let mut stack = vec![start];
        keep[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let next = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for j in next.into_iter().flatten() {
                if on[j] && !keep[j] {
                    keep[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    for (p, k) in map.iter_mut().zip(keep) {
        if !k {
            *p = 0;
        }
    }
}

/// ลด alpha ตรงขอบด้านในของส่วนที่เติม: เฉลี่ยในช่อง (2r+1)² แล้วไม่เกินของเดิม
fn feather(img: &RgbaImage, r: usize) -> RgbaImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let alpha: Vec<f32> = img.pixels().map(|p| p[3] as f32).collect();
    let blur = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let (len, lines) = if horizontal { (w, h) } else { (h, w) };
        let at = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
        let mut out = vec![0f32; src.len()];
        for line in 0..lines {
            let mut sum = vec![0f32; len + 1];
            for i in 0..len {
                sum[i + 1] = sum[i] + src[at(line, i)];
            }
            for i in 0..len {
                let (a, b) = (i.saturating_sub(r), (i + r + 1).min(len));
                out[at(line, i)] = (sum[b] - sum[a]) / (2 * r + 1) as f32;
            }
        }
        out
    };
    let soft = blur(&blur(&alpha, true), false);
    let mut out = img.clone();
    for (i, p) in out.pixels_mut().enumerate() {
        p[3] = soft[i].min(alpha[i]).round() as u8;
    }
    out
}

/// ภาพน้ำหนักตัวคนที่ไม่นับส่วนที่ลบแล้ว (Mask ตัวคนและตัวผอมไม่ทำกับคนที่ลบไป — ไม่งั้นเห็นเงาของคนที่ลบสว่าง/หดตามขึ้นมา)
pub fn clear_map(map: &crate::subject::SubjectMap, removals: &[Removal]) -> crate::subject::SubjectMap {
    let img = map.image();
    let (w, h) = img.dimensions();
    let cleared = image::GrayImage::from_fn(w, h, |x, y| {
        let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
        let keep = removals
            .iter()
            .filter_map(|r| r.patch.0.as_ref().and_then(|p| p.at(u, v)).map(|(_, a)| 1.0 - a))
            .product::<f64>();
        image::Luma([(img.get_pixel(x, y)[0] as f64 * keep).round() as u8])
    });
    crate::subject::SubjectMap::new(cleared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma, Rgb};

    /// ฉากไล่สีตามแนวนอน + คนสีแดงสองคน (x 40–70 และ 130–160, y 40–180) บนภาพ 200×200
    fn scene() -> RgbImage {
        RgbImage::from_fn(200, 200, |x, y| {
            let person = ((40..70).contains(&x) || (130..160).contains(&x)) && (40..180).contains(&y);
            if person {
                Rgb([220, 30, 30])
            } else {
                Rgb([40, (x * 255 / 200) as u8, 200])
            }
        })
    }

    /// ตัวแยกจำลอง: ส่วนที่เป็นสีแดง
    struct Red;
    impl Segmenter for Red {
        fn segment(&self, image: &RgbImage) -> Result<GrayImage> {
            Ok(GrayImage::from_fn(image.width(), image.height(), |x, y| {
                let p = image.get_pixel(x, y);
                Luma([if p[0] > 150 && p[1] < 100 { 255 } else { 0 }])
            }))
        }
    }

    /// ตัวเติมจำลอง: ช่องเป็นสีฟ้าล้วน
    struct Flat;
    impl Inpainter for Flat {
        fn side(&self) -> u32 {
            64
        }
        fn inpaint(&self, image: &RgbImage, holes: &GrayImage) -> Result<RgbImage> {
            Ok(RgbImage::from_fn(64, 64, |x, y| {
                if holes.get_pixel(x, y)[0] > 127 {
                    Rgb([40, 120, 200])
                } else {
                    *image.get_pixel(x, y)
                }
            }))
        }
    }

    fn spec(rect: [f64; 4], anchor: Option<[f64; 2]>) -> Removal {
        Removal { id: "r".into(), rect, anchor, brush: vec![], patch: Attached::default() }
    }

    fn removal(rect: [f64; 4], patch: Patch) -> Removal {
        Removal { patch: Attached(Some(patch)), ..spec(rect, None) }
    }

    fn render(img: &RgbImage, removals: &[Removal]) -> RgbImage {
        RgbImage::from_fn(img.width(), img.height(), |x, y| {
            let (u, v) = ((x as f64 + 0.5) / img.width() as f64, (y as f64 + 0.5) / img.height() as f64);
            let here = img.get_pixel(x, y).0.map(f64::from);
            Rgb(apply(removals, u, v, here, Space::Bytes, 255.0).map(|c| c.round() as u8))
        })
    }

    #[test]
    fn the_person_in_the_box_goes_and_everything_else_stays() {
        let img = scene();
        // กรอบรอบคนขวา (ล้นตัวคนเล็กน้อย)
        let rect = [125.0 / 200.0, 35.0 / 200.0, 40.0 / 200.0, 150.0 / 200.0];
        let (patch, found) = compute(&img, &spec(rect, None), &Red, &Flat).unwrap();
        assert!(found, "พบคนในกรอบ");
        let out = render(&img, &[removal(rect, patch)]);
        let red = |x: u32, y: u32| out.get_pixel(x, y)[0] > 150;
        assert!((130..160).all(|x| !red(x, 100)), "คนขวาหายไป");
        assert!((40..70).all(|x| red(x, 100)), "คนซ้ายอยู่ครบ");
        for (x, y) in [(10, 100), (100, 100), (190, 10), (100, 195)] {
            assert_eq!(out.get_pixel(x, y), img.get_pixel(x, y), "ฉากที่ไม่เกี่ยว ({x},{y})");
        }
        // ตรงที่ลบ: ฉากที่ต่อกับฉากจริงรอบ ๆ (ตัวเติมจำลองเติมสีเดียวแต่ถูกปรับสีตามวงรอบ)
        let p = out.get_pixel(145, 100);
        assert!(p[2] > 180 && p[1].abs_diff((145 * 255 / 200) as u8) < 30, "ตรงที่ลบ {p:?}");
    }

    #[test]
    fn an_anchor_keeps_only_what_touches_it() {
        // กรอบกว้างครอบทั้งสองคน แต่ยึดที่คนขวา: คนซ้ายไม่ถูกลบ
        let img = scene();
        let rect = [30.0 / 200.0, 30.0 / 200.0, 140.0 / 200.0, 160.0 / 200.0];
        let (patch, found) = compute(&img, &spec(rect, Some([145.0 / 200.0, 60.0 / 200.0])), &Red, &Flat).unwrap();
        assert!(found);
        let out = render(&img, &[removal(rect, patch)]);
        let red = |x: u32| out.get_pixel(x, 100)[0] > 150;
        assert!((130..160).all(|x| !red(x)), "คนขวาหายไป");
        assert!((40..70).all(red), "คนซ้ายอยู่ครบ");
    }

    #[test]
    fn a_brush_removes_exactly_what_was_painted() {
        // ระบายเส้นตั้งผ่านคนซ้ายครึ่งตัวบน (รัศมี 20 พิกเซล = 0.1 ของด้านยาว): ส่วนนั้นหาย ส่วนล่างของคนซ้ายและคนขวาอยู่ครบ
        let img = scene();
        let brush = vec![[55.0 / 200.0, 50.0 / 200.0, 0.1], [55.0 / 200.0, 100.0 / 200.0, 0.1]];
        let r = Removal { brush, ..spec([0.0, 0.0, 0.01, 0.01], None) };
        assert!(validate(std::slice::from_ref(&r)).is_ok());
        let (patch, found) = compute(&img, &r, &Red, &Flat).unwrap();
        assert!(found);
        let out = render(&img, &[Removal { patch: Attached(Some(patch)), ..r }]);
        let red = |x: u32, y: u32| out.get_pixel(x, y)[0] > 150;
        assert!((40..70).all(|x| !red(x, 75)), "ตรงที่ระบายหายไป");
        assert!((40..70).all(|x| red(x, 170)), "ส่วนที่ไม่ได้ระบายอยู่ครบ");
        assert!((130..160).all(|x| red(x, 75)), "คนขวาอยู่ครบ");
        let bad = Removal { brush: vec![[0.5, 0.5, 0.0]], ..spec([0.1, 0.1, 0.1, 0.1], None) };
        assert!(validate(&[bad]).is_err(), "รัศมีศูนย์");
    }

    #[test]
    fn a_box_without_anything_standing_out_is_removed_whole() {
        let img = scene();
        // กรอบบนฉากหลังล้วน: ลบวงรีในกรอบ
        let rect = [80.0 / 200.0, 60.0 / 200.0, 30.0 / 200.0, 30.0 / 200.0];
        let (patch, found) = compute(&img, &spec(rect, None), &Red, &Flat).unwrap();
        assert!(!found);
        let r = removal(rect, patch);
        assert!(r.patch.0.as_ref().unwrap().at(95.0 / 200.0, 75.0 / 200.0).is_some_and(|(_, a)| a > 0.99), "กลางกรอบ");
        assert!(r.patch.0.as_ref().unwrap().at(10.0 / 200.0, 10.0 / 200.0).is_none(), "นอกส่วนที่เติม");
    }

    #[test]
    fn removed_people_leave_the_subject_map() {
        let img = scene();
        let rect = [125.0 / 200.0, 35.0 / 200.0, 40.0 / 200.0, 150.0 / 200.0];
        let (patch, _) = compute(&img, &spec(rect, None), &Red, &Flat).unwrap();
        let map = crate::subject::SubjectMap::new(Red.segment(&img).unwrap());
        let cleared = clear_map(&map, &[removal(rect, patch)]);
        assert_eq!(cleared.weight(145.0 / 200.0, 0.5), 0.0, "คนที่ลบไม่อยู่ในภาพน้ำหนัก");
        assert_eq!(cleared.weight(55.0 / 200.0, 0.5), 1.0, "คนที่เหลืออยู่ครบ");
        assert_eq!(cleared.people().len(), 1);
    }

    #[test]
    fn validation_and_equality() {
        let ok = Removal {
            id: "a".into(),
            rect: [0.1, 0.1, 0.2, 0.3],
            anchor: None,
            brush: vec![],
            patch: Attached::default(),
        };
        assert!(validate(std::slice::from_ref(&ok)).is_ok());
        assert!(validate(&[Removal { rect: [0.1, 0.1, 0.0, 0.3], ..ok.clone() }]).is_err());
        assert!(validate(&vec![ok.clone(); MAX_REMOVALS + 1]).is_err());
        let attached =
            Removal { patch: Attached(Some(Patch::new([0.0, 0.0, 1.0, 1.0], RgbaImage::new(1, 1)))), ..ok.clone() };
        assert_eq!(ok, attached, "ภาพที่เติมไม่ใช่ส่วนของค่าที่แต่ง");
        assert_eq!(
            serde_json::to_value(&attached).unwrap(),
            serde_json::json!({ "id": "a", "rect": [0.1, 0.1, 0.2, 0.3] })
        );
    }
}
