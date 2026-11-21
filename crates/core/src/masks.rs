//! ปรับเฉพาะส่วน (DECISIONS ข้อ 28): บริเวณ (Mask) + ค่าแสงสีของบริเวณนั้น แบบ Lightroom
//!
//! ทำหลังการปรับทั้งภาพ (รวม Texture/Clarity/Dehaze) บนภาพผล 8 บิต: ปรับส่วนที่ครอบบริเวณนั้นด้วยค่าของ Mask แล้วผสมกับภาพเดิม
//! ตามน้ำหนักของ Mask (0–1 ต่อพิกเซล) · พิกัดของบริเวณเป็นสัดส่วนบนภาพเดิม (ก่อนตัด/หมุน) — ตัด/หมุนใหม่ภายหลัง Mask ยังอยู่ที่เดิม
//! บนภาพ · ตัวอย่างสดของหน้าแต่งภาพใช้ฟังก์ชันนี้เอง ([`crate::preview`] — ค่าอ้างอิงตายตัว [`MASK_EXPECTED`])
//!
//! - **ใบหน้า:** วงรีนุ่ม ๆ รอบกรอบใบหน้าที่แอปตรวจไว้ (ใหญ่กว่ากรอบให้คลุมหน้าผากและคาง) · "เฉพาะผิว" = คูณความเป็นสีผิวจากช่วง Cb/Cr
//!   ของ YCbCr (สีจีวรส้มสดอยู่นอกช่วง) จึงทำหน้าขาวใส/ผิวเนียนได้โดยจีวรและฉากหลังไม่โดน

use crate::adjust::{Adjust, Profile};
use crate::edit::Geometry;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};

pub const MAX_MASKS: usize = 10;
pub(crate) const MAX_FACES: usize = 200;
/// วงรีของใบหน้า: รัศมีเทียบกับกรอบใบหน้า — ครึ่งบนสั้น (ไม่ล้นขึ้นไปบนฉากหลังเหนือศีรษะ) ครึ่งล่างยาว (คลุมคางและคอ)
/// และเลื่อนศูนย์กลางลงเล็กน้อย (สัดส่วนของความสูงกรอบ)
const FACE_RX: f64 = 0.62;
const FACE_UP: f64 = 0.62;
const FACE_DOWN: f64 = 0.85;
const FACE_DROP: f64 = 0.06;
/// ขอบนุ่มของวงรี: เต็มที่ถึงระยะ 0.75 ของรัศมี หมดที่ 1.15
const FACE_SOFT: (f64, f64) = (0.75, 1.15);
/// ช่วงสีผิวบน Cb/Cr: (ต่ำสุด, สูงสุด) และขอบนุ่มด้านล่าง/บน — ขอบล่างของ Cr ไม่ถึง 128 (สีเทา/ขาวเป็นกลาง ผนัง ผมขาว ไม่ใช่ผิว)
const SKIN_CB: (f64, f64, f64, f64) = (77.0, 127.0, 8.0, 8.0);
const SKIN_CR: (f64, f64, f64, f64) = (135.0, 173.0, 5.0, 8.0);
/// ขอบเผื่อรอบบริเวณ (สัดส่วนของด้านยาว) ให้ Texture/Clarity ของ Mask เห็นพิกเซลรอบข้างพอ
const MARGIN: f64 = 0.03;
/// สีผิวของแต่ละคน: วัดจากกลางใบหน้า (ระยะไม่เกิน 0.45 ของรัศมีวงรี) ต้องมีพิกเซลสีผิวอย่างน้อยเท่านี้
const TONE_INNER: f64 = 0.45;
const TONE_MIN_SAMPLES: usize = 12;
/// ห่างจากสีผิวของคนนั้น (Cb/Cr) ไม่เกินระยะแรก = เต็มที่ · เกินระยะหลัง = ไม่ใช่ผิวของหน้านี้ (ฉากหลังสีทอง/ส้มใกล้สีผิว)
const TONE_NEAR: f64 = 10.0;
const TONE_FAR: f64 = 20.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mask {
    /// รหัสที่ UI ใช้อ้างถึง
    pub id: String,
    pub area: Area,
    /// ค่าแสงสีของบริเวณนี้ — ทำต่อจากภาพที่ปรับทั้งภาพแล้ว
    pub adjust: Adjust,
    /// ความแรง 0–100
    #[serde(default = "full")]
    pub amount: f64,
    /// กลับด้าน: ปรับทุกส่วนยกเว้นบริเวณนี้
    #[serde(default)]
    pub invert: bool,
}

fn full() -> f64 {
    100.0
}

/// บริเวณ — พิกัดเป็นสัดส่วนบนภาพเดิม (หลังหมุนตาม EXIF ก่อนตัด/หมุนของการแต่ง)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Area {
    /// ใบหน้า: กรอบ [x, y, w, h] ของแต่ละหน้า · `skin` = เฉพาะสีผิวในบริเวณนั้น
    #[serde(rename_all = "camelCase")]
    Faces { boxes: Vec<[f64; 4]>, skin: bool },
}

impl Mask {
    /// มีผลกับภาพ
    pub fn is_active(&self) -> bool {
        self.amount > 0.0 && !self.adjust.is_neutral()
    }

    /// Mask เดียวกันสำหรับรูปอื่น (วางการแต่ง): บริเวณใบหน้าใช้ใบหน้าที่แอปพบในรูปนั้น `faces` แทนของรูปต้นทาง
    pub fn on_faces(&self, faces: &[[f64; 4]]) -> Mask {
        let area = match &self.area {
            Area::Faces { skin, .. } => Area::Faces { boxes: faces.to_vec(), skin: *skin },
        };
        Mask { area, ..self.clone() }
    }

    pub fn validate(&self) -> Result<()> {
        self.adjust.validate()?;
        let ok_id = !self.id.trim().is_empty() && self.id.chars().count() <= 64;
        let ok_amount = self.amount.is_finite() && (0.0..=100.0).contains(&self.amount);
        let ok_area = match &self.area {
            Area::Faces { boxes, .. } => {
                boxes.len() <= MAX_FACES
                    && boxes.iter().all(|b| {
                        b.iter().all(|v| v.is_finite())
                            && b[2] > 0.0
                            && b[3] > 0.0
                            && b[2] <= 2.0
                            && b[3] <= 2.0
                            && (-1.0..=2.0).contains(&b[0])
                            && (-1.0..=2.0).contains(&b[1])
                    })
            }
        };
        if ok_id && ok_amount && ok_area {
            Ok(())
        } else {
            Err(AppError::invalid("ค่าของการปรับเฉพาะส่วนไม่ถูกต้อง"))
        }
    }
}

pub fn validate(masks: &[Mask]) -> Result<()> {
    if masks.len() > MAX_MASKS {
        return Err(AppError::invalid(format!("ปรับเฉพาะส่วนได้ไม่เกิน {MAX_MASKS} บริเวณต่อรูป")));
    }
    masks.iter().try_for_each(Mask::validate)
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn chroma([r, g, b]: [u8; 3]) -> [f64; 2] {
    let (r, g, b) = (r as f64, g as f64, b as f64);
    [128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b, 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b]
}

/// ความเป็นสีผิว 0–1 จาก Cb/Cr ของ YCbCr
pub(crate) fn skin_weight(rgb: [u8; 3]) -> f64 {
    let [cb, cr] = chroma(rgb);
    let inside = |v: f64, (lo, hi, below, above): (f64, f64, f64, f64)| {
        smoothstep(lo - below, lo, v) * (1.0 - smoothstep(hi, hi + above, v))
    };
    inside(cb, SKIN_CB) * inside(cr, SKIN_CR)
}

/// วงรีรอบใบหน้าบนภาพเดิม
struct Oval {
    c: [f64; 2],
    rx: f64,
    up: f64,
    down: f64,
}

impl Oval {
    fn of(b: &[f64; 4]) -> Oval {
        Oval {
            c: [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0 + b[3] * FACE_DROP],
            rx: b[2] * FACE_RX,
            up: b[3] * FACE_UP,
            down: b[3] * FACE_DOWN,
        }
    }

    /// ระยะจากศูนย์กลางเทียบกับรัศมี (1 = ขอบวงรี)
    fn distance(&self, u: f64, v: f64) -> f64 {
        let ry = if v < self.c[1] { self.up } else { self.down };
        (((u - self.c[0]) / self.rx).powi(2) + ((v - self.c[1]) / ry).powi(2)).sqrt()
    }

    /// มุมทั้งสี่ของกรอบที่ครอบวงรีที่ระยะ `k`
    fn corners(&self, k: f64) -> [(f64, f64); 4] {
        let (x0, x1) = (self.c[0] - self.rx * k, self.c[0] + self.rx * k);
        let (y0, y1) = (self.c[1] - self.up * k, self.c[1] + self.down * k);
        [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
    }
}

pub(crate) fn face_weight(b: &[f64; 4], u: f64, v: f64) -> f64 {
    1.0 - smoothstep(FACE_SOFT.0, FACE_SOFT.1, Oval::of(b).distance(u, v))
}

/// น้ำหนักของบริเวณที่จุด (u, v) บนภาพเดิม สำหรับพิกเซลสี `rgb` (ก่อนกลับด้าน/ความแรง) · `tones` = สีผิวของแต่ละหน้า ([`face_tones`])
pub fn area_weight(area: &Area, tones: &[Option<[f64; 2]>], u: f64, v: f64, rgb: [u8; 3]) -> f64 {
    match area {
        Area::Faces { boxes, skin } => {
            let mut w = 0f64;
            for (i, b) in boxes.iter().enumerate() {
                let mut e = face_weight(b, u, v);
                if e <= w {
                    continue;
                }
                if *skin {
                    e *= skin_weight(rgb) * tone_match(tones.get(i).copied().flatten(), rgb);
                }
                w = w.max(e);
            }
            w
        }
    }
}

/// ใกล้สีผิวของคนนั้นแค่ไหน (1 = ใกล้ · 0 = สีอื่นที่บังเอิญอยู่ในช่วงสีผิว) — ไม่รู้สีผิว = 1
pub(crate) fn tone_match(tone: Option<[f64; 2]>, rgb: [u8; 3]) -> f64 {
    let Some([tb, tr]) = tone else { return 1.0 };
    let [cb, cr] = chroma(rgb);
    1.0 - smoothstep(TONE_NEAR, TONE_FAR, ((cb - tb).powi(2) + (cr - tr).powi(2)).sqrt())
}

/// สีผิวของแต่ละหน้า: Cb/Cr เฉลี่ยของพิกเซลสีผิวกลางใบหน้า บนภาพที่กำลังปรับ — หาไม่พอ = None
pub fn face_tones(px: &[u8], w: usize, h: usize, stride: usize, g: &Geometry, area: &Area) -> Vec<Option<[f64; 2]>> {
    let Area::Faces { boxes, skin } = area;
    if !skin {
        return vec![None; boxes.len()];
    }
    boxes
        .iter()
        .map(|b| {
            let oval = Oval::of(b);
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (u, v) in oval.corners(TONE_INNER) {
                let (a, b) = g.forward(u, v);
                (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(a), y1.max(b));
            }
            let span = |a: f64, b: f64, n: usize| {
                (
                    (a * n as f64).floor().clamp(0.0, n as f64) as usize,
                    (b * n as f64).ceil().clamp(0.0, n as f64) as usize,
                )
            };
            let ((ax, bx), (ay, by)) = (span(x0, x1, w), span(y0, y1, h));
            let (mut sum, mut n) = ([0f64; 2], 0usize);
            for y in ay..by {
                for x in ax..bx {
                    let (u, v) = g.inverse((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
                    if oval.distance(u, v) > TONE_INNER {
                        continue;
                    }
                    let i = (y * w + x) * stride;
                    let rgb = [px[i], px[i + 1], px[i + 2]];
                    if skin_weight(rgb) > 0.5 {
                        let [cb, cr] = chroma(rgb);
                        sum = [sum[0] + cb, sum[1] + cr];
                        n += 1;
                    }
                }
            }
            (n >= TONE_MIN_SAMPLES).then(|| [sum[0] / n as f64, sum[1] / n as f64])
        })
        .collect()
}

/// กรอบบนภาพที่แต่ง (พิกเซล x0, y0, x1, y1) ที่ครอบส่วนที่มีน้ำหนัก + ขอบเผื่อ · None = ไม่มีส่วนใดอยู่ในภาพ
pub(crate) fn bounds(area: &Area, g: &Geometry, w: usize, h: usize, long: f64) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    match area {
        Area::Faces { boxes, .. } => {
            for b in boxes {
                for (u, v) in Oval::of(b).corners(FACE_SOFT.1) {
                    let (a, b) = g.forward(u, v);
                    (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(a), y1.max(b));
                }
            }
        }
    }
    let margin = MARGIN * long;
    let px = |v: f64, n: usize, extra: f64| (v * n as f64 + extra).clamp(0.0, n as f64);
    let (ax, ay) = (px(x0, w, -margin).floor() as usize, px(y0, h, -margin).floor() as usize);
    let (bx, by) = (px(x1, w, margin).ceil() as usize, px(y1, h, margin).ceil() as usize);
    (bx > ax && by > ay).then_some((ax, ay, bx, by))
}

/// ปรับเฉพาะส่วนในที่ · `px` ภาพที่แต่งแล้วทั้งกรอบ ขนาด w×h พิกเซลละ `stride` ไบต์ (3 หรือ 4) · `g` แปลงพิกัดของภาพนี้กลับไปภาพเดิม
pub fn apply(px: &mut [u8], w: usize, h: usize, stride: usize, g: &Geometry, masks: &[Mask]) {
    apply_showing(px, w, h, stride, g, masks, None);
}

/// เหมือน [`apply`] · `show` = แสดงบริเวณของ Mask นี้เป็นสีแดงทับแทนการปรับ (หน้าแต่งภาพตอนกดค้างหรือเพิ่งเพิ่ม — แม้ยังไม่ได้ปรับค่า)
/// · แต่ละแถวคิดคู่ขนานกัน (ผลเท่ากับแกนเดียวทุกบิต)
pub fn apply_showing(
    px: &mut [u8],
    w: usize,
    h: usize,
    stride: usize,
    g: &Geometry,
    masks: &[Mask],
    show: Option<&str>,
) {
    use rayon::prelude::*;
    if w == 0 || h == 0 || px.len() < w * h * stride {
        return;
    }
    // ด้านยาวของกรอบก่อนตัด (รัศมีของ Texture/Clarity และขอบเผื่อ)
    let long = g.frame_long(w as u32, h as u32);
    for mask in masks {
        let showing = show == Some(mask.id.as_str());
        if !showing && !mask.is_active() {
            continue;
        }
        let region = if mask.invert { Some((0, 0, w, h)) } else { bounds(&mask.area, g, w, h, long) };
        let Some((x0, y0, x1, y1)) = region else { continue };
        let (rw, rh) = (x1 - x0, y1 - y0);
        // ส่วนที่ครอบบริเวณ → ปรับด้วยค่าของ Mask (Texture/Clarity คิดรัศมีจากด้านยาวของทั้งกรอบ)
        let adjusted = (!showing).then(|| {
            let mut crop = vec![0u8; rw * rh * 3];
            crop.par_chunks_mut(rw * 3).enumerate().for_each(|(y, line)| {
                for x in 0..rw {
                    let i = ((y0 + y) * w + x0 + x) * stride;
                    line[x * 3..x * 3 + 3].copy_from_slice(&px[i..i + 3]);
                }
            });
            // Vignette เป็นของกรอบภาพทั้งภาพ ไม่ใช่ของบริเวณ — ไม่ใช้ในบริเวณ
            let per_pixel = Adjust {
                texture: 0.0,
                clarity: 0.0,
                dehaze: 0.0,
                vignette: crate::adjust::Vignette::default(),
                ..mask.adjust.clone()
            };
            crate::adjust::apply_pixels(&mut crop, rw, rh, 3, &per_pixel, Profile::Display, long);
            if crate::local::is_active(&mask.adjust) {
                crate::local::apply(&mut crop, rw, rh, 3, long, &mask.adjust);
            }
            crop
        });
        let amount = mask.amount / 100.0;
        let tones = face_tones(px, w, h, stride, g, &mask.area);
        px[y0 * w * stride..y1 * w * stride].par_chunks_mut(w * stride).enumerate().for_each(|(y, line)| {
            for x in 0..rw {
                let i = (x0 + x) * stride;
                let rgb = [line[i], line[i + 1], line[i + 2]];
                let (u, v) = g.inverse(((x0 + x) as f64 + 0.5) / w as f64, ((y0 + y) as f64 + 0.5) / h as f64);
                let mut t = area_weight(&mask.area, &tones, u, v, rgb);
                if mask.invert {
                    t = 1.0 - t;
                }
                let Some(adjusted) = &adjusted else {
                    // บริเวณที่เลือก: แดงโปร่งตามน้ำหนัก
                    let k = t * 0.55;
                    for (c, target) in [230.0, 40.0, 60.0].into_iter().enumerate() {
                        line[i + c] = (rgb[c] as f64 + (target - rgb[c] as f64) * k).round() as u8;
                    }
                    continue;
                };
                let t = t * amount;
                if t <= 0.0 {
                    continue;
                }
                let a = &adjusted[(y * rw + x) * 3..(y * rw + x) * 3 + 3];
                for c in 0..3 {
                    line[i + c] = (rgb[c] as f64 + (a[c] as f64 - rgb[c] as f64) * t).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
}

/// ตำแหน่งที่เทียบใน [`MASK_EXPECTED`] (ภาพ [`crate::local::test_image`] 48×32)
#[doc(hidden)]
pub const MASK_SAMPLES: [(usize, usize); 5] = [(5, 5), (20, 14), (24, 16), (30, 20), (44, 28)];

/// ค่าอ้างอิงตายตัว: Mask ใบหน้า [0.3, 0.2, 0.35, 0.5] ค่าแสง +0.6 Texture −40 (เฉพาะผิว / ทั้งบริเวณ)
/// บนภาพที่ไม่ได้ตัด/หมุน → พิกเซลที่ [`MASK_SAMPLES`]
#[doc(hidden)]
pub const MASK_EXPECTED: [(bool, [[u8; 3]; 5]); 2] = [
    (true, [[70, 192, 60], [124, 60, 144], [94, 78, 142], [160, 75, 160], [166, 60, 234]]),
    (false, [[70, 192, 60], [151, 75, 175], [116, 97, 174], [190, 91, 190], [166, 60, 234]]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::EditSettings;
    use image::RgbImage;

    fn face_mask(boxes: Vec<[f64; 4]>, skin: bool, adjust: Adjust) -> Mask {
        Mask { id: "m1".into(), area: Area::Faces { boxes, skin }, adjust, amount: 100.0, invert: false }
    }

    fn brighter() -> Adjust {
        Adjust { exposure: 1.0, ..Default::default() }
    }

    #[test]
    fn skin_is_skin_and_saffron_robes_are_not() {
        assert!(skin_weight([210, 160, 130]) > 0.95, "ผิวคนไทยใต้แสงอุ่น");
        assert!(skin_weight([175, 120, 95]) > 0.95, "ผิวเข้มขึ้น");
        assert!(skin_weight([240, 150, 30]) < 0.01, "จีวรส้มสด");
        assert!(skin_weight([128, 128, 128]) < 0.01, "สีเทา");
        assert!(skin_weight([90, 140, 220]) < 0.01, "ฟ้า");
        assert!(skin_weight([220, 220, 225]) < 0.01, "ผมขาว/ผนังขาว");
    }

    #[test]
    fn a_face_mask_changes_only_the_face_and_with_skin_only_the_skin() {
        // ครึ่งซ้ายสีผิว ครึ่งขวาสีจีวร · กรอบใบหน้ากลางภาพคร่อมทั้งสองสี
        let (w, h) = (80usize, 60usize);
        let img = RgbImage::from_fn(w as u32, h as u32, |x, _| {
            image::Rgb(if x < 40 { [200, 150, 120] } else { [230, 140, 30] })
        });
        let s = EditSettings::default();
        let g = Geometry::new(w as u32, h as u32, &s);
        let at = |px: &RgbImage, x: u32, y: u32| px.get_pixel(x, y).0;
        // ทั้งบริเวณ: ในวงรีสว่างขึ้นทั้งสองสี · นอกวงรี (มุมภาพ) เท่าเดิม
        let mut all = img.clone();
        apply(&mut all, w, h, 3, &g, &[face_mask(vec![[0.35, 0.3, 0.3, 0.4]], false, brighter())]);
        assert!(
            at(&all, 36, 30)[1] > 175 && at(&all, 44, 30)[1] > 160,
            "{:?} {:?}",
            at(&all, 36, 30),
            at(&all, 44, 30)
        );
        assert_eq!(at(&all, 2, 2), [200, 150, 120], "นอกบริเวณไม่เปลี่ยน");
        // เฉพาะผิว: สีผิวสว่างขึ้น สีจีวรในวงรีเดียวกันไม่เปลี่ยน
        let mut skin = img.clone();
        apply(&mut skin, w, h, 3, &g, &[face_mask(vec![[0.35, 0.3, 0.3, 0.4]], true, brighter())]);
        assert!(at(&skin, 36, 30)[1] > 175, "ผิว: {:?}", at(&skin, 36, 30));
        assert_eq!(at(&skin, 44, 30), [230, 140, 30], "จีวรไม่โดน");
        // กลับด้าน: นอกบริเวณสว่างขึ้น ใจกลางเท่าเดิม
        let mut inv = img.clone();
        let mut m = face_mask(vec![[0.35, 0.3, 0.3, 0.4]], false, brighter());
        m.invert = true;
        apply(&mut inv, w, h, 3, &g, &[m]);
        assert!(at(&inv, 2, 2)[1] > 175 && at(&inv, 38, 30) == [200, 150, 120], "{:?}", at(&inv, 2, 2));
        // ความแรง 0 หรือไม่ได้ปรับอะไร = ไม่มีผล
        let mut none = img.clone();
        apply(
            &mut none,
            w,
            h,
            3,
            &g,
            &[Mask { amount: 0.0, ..face_mask(vec![[0.35, 0.3, 0.3, 0.4]], false, brighter()) }],
        );
        assert_eq!(none, img);
    }

    #[test]
    fn a_golden_backdrop_next_to_the_face_is_not_taken_as_skin() {
        // หน้าสีผิว (Cb 104.6 Cr 155.4) อยู่บนฉากหลังสีทองที่บังเอิญอยู่ในช่วงสีผิวทั่วไป (Cb 84.6 Cr 158.7)
        let (w, h) = (80usize, 80usize);
        let img = RgbImage::from_fn(w as u32, h as u32, |x, y| {
            let d = ((x as f64 - 40.0).powi(2) + (y as f64 - 40.0).powi(2)).sqrt();
            image::Rgb(if d < 11.0 { [200, 150, 120] } else { [220, 170, 100] })
        });
        assert!(skin_weight([220, 170, 100]) > 0.95, "สีทองนี้อยู่ในช่วงสีผิวทั่วไปจริง");
        let g = Geometry::new(w as u32, h as u32, &EditSettings::default());
        let mut px = img.clone();
        apply(&mut px, w, h, 3, &g, &[face_mask(vec![[0.36, 0.34, 0.28, 0.3]], true, brighter())]);
        assert!(px.get_pixel(40, 40).0[1] > 175, "หน้าสว่างขึ้น: {:?}", px.get_pixel(40, 40));
        assert_eq!(px.get_pixel(40, 56).0, [220, 170, 100], "ฉากหลังสีทองในวงรีไม่โดน");
    }

    #[test]
    fn the_mask_stays_on_the_same_spot_of_the_photo_after_turning() {
        // บริเวณอยู่มุมซ้ายบนของภาพเดิม · หมุน 90° ตามเข็ม → อยู่มุมขวาบนของภาพที่แต่ง
        let (w, h) = (60usize, 40usize);
        let s = EditSettings { rotate: 90, ..Default::default() };
        let g = Geometry::new(w as u32, h as u32, &s);
        let mut out = RgbImage::from_pixel(h as u32, w as u32, image::Rgb([200, 150, 120]));
        apply(&mut out, h, w, 3, &g, &[face_mask(vec![[0.05, 0.05, 0.2, 0.3]], false, brighter())]);
        assert!(out.get_pixel(33, 9).0[1] > 175, "มุมขวาบน: {:?}", out.get_pixel(33, 9));
        assert_eq!(out.get_pixel(5, 50).0, [200, 150, 120], "มุมซ้ายล่างไม่เปลี่ยน");
    }

    #[test]
    fn validation_rejects_bad_masks() {
        let ok = face_mask(vec![[0.1, 0.1, 0.2, 0.2]], true, brighter());
        assert!(validate(std::slice::from_ref(&ok)).is_ok());
        assert!(validate(&[Mask { amount: 120.0, ..ok.clone() }]).is_err());
        assert!(validate(&[face_mask(vec![[0.1, 0.1, -0.2, 0.2]], true, brighter())]).is_err());
        assert!(validate(&[face_mask(vec![[f64::NAN, 0.1, 0.2, 0.2]], true, brighter())]).is_err());
        assert!(validate(&vec![ok; MAX_MASKS + 1]).is_err());
    }

    #[test]
    fn matches_the_shared_reference_values() {
        let g = Geometry::new(48, 32, &EditSettings::default());
        for (skin, expected) in MASK_EXPECTED {
            let mut px = crate::local::test_image(48, 32);
            let adjust = Adjust { exposure: 0.6, texture: -40.0, ..Default::default() };
            apply(&mut px, 48, 32, 3, &g, &[face_mask(vec![[0.3, 0.2, 0.35, 0.5]], skin, adjust)]);
            let got =
                MASK_SAMPLES.map(|(x, y)| [px[(y * 48 + x) * 3], px[(y * 48 + x) * 3 + 1], px[(y * 48 + x) * 3 + 2]]);
            assert_eq!(got, expected, "skin {skin}");
        }
    }

    #[test]
    fn a_mask_ignores_vignette_which_belongs_to_the_whole_frame() {
        // ค่าที่ส่งมาทาง AI หรือค่าที่เก็บไว้มี Vignette ในบริเวณได้: ไม่มีผล เหมือนตัวอย่างสด (เทสต์สุ่มจับได้ว่าเคยต่างกัน)
        let g = Geometry::new(48, 32, &EditSettings::default());
        let run = |adjust: Adjust| {
            let mut px = crate::local::test_image(48, 32);
            apply(&mut px, 48, 32, 3, &g, &[face_mask(vec![[0.3, 0.2, 0.35, 0.5]], false, adjust)]);
            px
        };
        let vignette = crate::adjust::Vignette { amount: -100.0, ..Default::default() };
        assert_eq!(run(Adjust { vignette, ..brighter() }), run(brighter()));
    }

    #[test]
    fn on_a_cropped_photo_the_area_uses_the_whole_frame_like_the_live_preview() {
        // ตัดครึ่งซ้าย: ภาพที่แต่ง 48×32 มาจากกรอบ 96×32 — Texture ของบริเวณคิดรัศมีจากด้านยาว 96 (ไม่ใช่ 48)
        let s = EditSettings { crop: Some(crate::edit::Rect { x: 0.0, y: 0.0, w: 0.5, h: 1.0 }), ..Default::default() };
        let g = Geometry::new(96, 32, &s);
        let adjust = Adjust { texture: 80.0, clarity: 60.0, ..Default::default() };
        let mut px = crate::local::test_image(48, 32);
        apply(
            &mut px,
            48,
            32,
            3,
            &g,
            &[Mask { invert: true, ..face_mask(vec![[0.9, 0.9, 0.05, 0.05]], false, adjust.clone()) }],
        );
        let mut want = crate::local::test_image(48, 32);
        crate::local::apply(&mut want, 48, 32, 3, 96.0, &adjust);
        assert_eq!(px, want);
    }
}
