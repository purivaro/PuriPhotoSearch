//! ตัวผอม (DECISIONS ข้อ 41): หดลำตัวของแต่ละคนเข้าหาเส้นกลางลำตัว โดยฉากหลังไม่โค้งตาม
//!
//! - **ตัวคน:** ภาพน้ำหนักของตัวแยกตัวคน (ข้อ 40) แบ่งเป็นรายคน (ส่วนที่ติดกัน) · แต่ละแถวของคนหนึ่งหาลำตัว (ช่วงที่ใกล้จุดกึ่งกลาง
//!   ของคนนั้นที่สุด) แล้วเกลี่ยเส้นกลางและความกว้างตามแนวตั้งให้เรียบ (ความกว้างใช้ค่าน้อยสุดของแถวใกล้ ๆ — แขนที่แนบตัวไม่ทำให้ลำตัวกว้าง)
//! - **หด:** ในลำตัวหดเข้าหาเส้นกลางตามสัดส่วน · แถบนอกลำตัว (แขนที่แนบตัว) ขยับน้อยลงจนหมด · แขนที่ยื่นไกลกว่าแถบไม่ขยับ ·
//!   หัวไม่ขยับ (ใต้คางจากกรอบใบหน้าถ้ามี หรือส่วนบนของคนนั้น) แล้วค่อย ๆ เริ่มหดที่คอ
//! - **ฉากหลังไม่ขยับ:** ขยับเฉพาะชั้นของตัวคน (สี + ภาพน้ำหนักที่จุดต้นทาง) แล้ววางบนฉากหลัง · ตรงที่ตัวคนหดออกไปใช้ฉากหลังที่
//!   ตัวเติมฉากหลัง (LaMa) เติมไว้แล้วของรูปนั้น ([`Fill`]) · ส่วนที่ไม่ขยับ = พิกเซลเดิมทุกพิกเซล
//!
//! คิดบนภาพเดิมก่อนตัด/หมุน (แบบหน้าเรียว ข้อ 31): ตัวอย่างสด ภาพที่บันทึก และภาพจาก RAW คู่ ใช้ [`BodyPlan::sample`] ตัวเดียวกัน

use crate::error::{AppError, Result};
use crate::subject::{Inpainter, SubjectMap};
use image::{imageops, DynamicImage, GrayImage, ImageBuffer, Pixel, RgbImage, RgbaImage};
use rayon::prelude::*;
use std::sync::Arc;

/// หดลำตัวสูงสุด (สัดส่วนของครึ่งความกว้างลำตัว) ที่ตัวผอม 100
pub const MAX_SLIM: f64 = 0.12;
/// นับเป็นตัวคน (ภาพน้ำหนัก 0–255) ตอนแบ่งคนและหาลำตัว
const SOLID: u8 = 128;
/// ขอบนุ่มที่ยังนับเป็นส่วนของคนตอนหาขอบซ้าย/ขวาของแถว
const SOFT: u8 = 12;
/// ส่วนที่ต้องเติมฉากหลัง (ภาพน้ำหนักตั้งแต่ค่านี้ แล้วขยายออกอีก [`HOLE_GROW`] ของด้านยาว)
const HOLE: u8 = 8;
/// ขยายช่องที่เติมออกจากตัวคน: สีของขอบตัวคนที่เหลือไว้จะถูกตัวเติมลากเข้าไปเป็นเงาของคนเดิม (ลองแล้ว)
const HOLE_GROW: f64 = 0.01;
/// ระยะเผื่อรอบตัวคนที่ส่งให้ตัวเติมดูฉากหลัง (สัดส่วนของด้านยาวของกรอบตัวคน)
const CONTEXT: f64 = 0.15;
/// คนที่เล็กกว่าสัดส่วนนี้ของภาพไม่หด
const MIN_AREA: f64 = 0.005;
/// หัวเมื่อไม่มีกรอบใบหน้า: เริ่มหด / หดเต็ม (สัดส่วนจากบนสุดของคนนั้น)
const HEAD_FROM: f64 = 0.2;
const HEAD_SPAN: f64 = 0.12;
/// มีกรอบใบหน้า: เริ่มหดที่ขอบล่างของกรอบ (ใต้คาง) แล้วหดเต็มเมื่อลงไปอีกสัดส่วนนี้ของความสูงหน้า (คอ)
const NECK_SPAN: f64 = 0.6;
/// ขอบนุ่มของตัวคนเดิมที่น้ำหนักตั้งแต่นี้ใช้ฉากที่เติมเต็มที่ (ต่ำกว่านี้ผสมกับพิกเซลเดิมตามสัดส่วน)
const EDGE_SEEN: f64 = 0.2;
/// นอกขอบตัวคนเดิม (พิกเซลของภาพน้ำหนัก 0, 1, 2, …): สัดส่วนที่ยังใช้ฉากที่เติม — ค่อย ๆ จางไปหาฉากจริง
const FEATHER: [f64; 5] = [1.0, 1.0, 0.7, 0.4, 0.15];
/// แถบนอกลำตัวที่ยังหดตาม (สัดส่วนของครึ่งความกว้างลำตัว)
const BAND: f64 = 0.6;
/// เกลี่ยตามแนวตั้ง (สัดส่วนของความสูงภาพ): ช่วงหาความกว้างน้อยสุด และความกว้างของการเกลี่ย
const MIN_WINDOW: f64 = 0.02;
const SMOOTH: f64 = 0.03;

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    if b <= a {
        return if x >= b { 1.0 } else { 0.0 };
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ───────────────────────── คนในภาพน้ำหนัก ─────────────────────────

/// คนหนึ่งคนในภาพน้ำหนัก: ต่อแถว (จาก `top`) เส้นกลางลำตัว ครึ่งความกว้างลำตัว ขอบซ้าย/ขวาของคน (พิกเซลของภาพน้ำหนัก แบบต่อเนื่อง)
#[derive(Debug, Clone)]
pub struct Person {
    top: usize,
    centre: Vec<f64>,
    half: Vec<f64>,
    left: Vec<f64>,
    right: Vec<f64>,
    x0: f64,
    x1: f64,
}

impl Person {
    fn rows(&self) -> usize {
        self.centre.len()
    }
}

/// แบ่งภาพน้ำหนักเป็นรายคน แล้วหาลำตัวของแต่ละแถว
pub fn people(map: &GrayImage) -> Vec<Person> {
    let (w, h) = (map.width() as usize, map.height() as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let px = map.as_raw();
    let mut label = vec![0u32; w * h];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    let mut next = 0u32;
    for start in 0..w * h {
        if px[start] < SOLID || label[start] != 0 {
            continue;
        }
        next += 1;
        label[start] = next;
        stack.push(start);
        // ต่อแถว: x น้อยสุด มากสุด ผลรวม x จำนวน
        let mut rows: std::collections::BTreeMap<usize, (usize, usize, f64, f64)> = Default::default();
        let mut area = 0usize;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            area += 1;
            let r = rows.entry(y).or_insert((x, x, 0.0, 0.0));
            (r.0, r.1, r.2, r.3) = (r.0.min(x), r.1.max(x), r.2 + x as f64 + 0.5, r.3 + 1.0);
            let mut visit = |j: usize| {
                if px[j] >= SOLID && label[j] == 0 {
                    label[j] = next;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        if (area as f64) < MIN_AREA * (w * h) as f64 {
            continue;
        }
        found.push(person(px, &label, next, w, h, &rows));
    }
    found
}

fn person(
    px: &[u8],
    label: &[u32],
    me: u32,
    w: usize,
    h: usize,
    rows: &std::collections::BTreeMap<usize, (usize, usize, f64, f64)>,
) -> Person {
    let top = *rows.keys().next().expect("มีอย่างน้อยหนึ่งแถว");
    let bottom = *rows.keys().next_back().expect("มีอย่างน้อยหนึ่งแถว");
    let n = bottom - top + 1;
    let (mut centre, mut half, mut left, mut right) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for (y, &(min, max, sum, count)) in rows {
        let i = y - top;
        let line = &label[y * w..(y + 1) * w];
        // ลำตัว: ช่วงต่อเนื่องของคนนี้ที่ใกล้จุดกึ่งกลางของคนนี้ในแถวนี้ที่สุด
        let mass = sum / count;
        let (mut best, mut best_d) = ((min as f64, max as f64 + 1.0), f64::MAX);
        let mut x = min;
        while x <= max {
            if line[x] != me {
                x += 1;
                continue;
            }
            let st = x;
            while x <= max && line[x] == me {
                x += 1;
            }
            let (s, e) = (st as f64, x as f64);
            let d = if (s..e).contains(&mass) { 0.0 } else { (mass - s).abs().min((mass - e).abs()) };
            if d < best_d {
                (best, best_d) = ((s, e), d);
            }
        }
        centre[i] = (best.0 + best.1) / 2.0;
        half[i] = (best.1 - best.0) / 2.0;
        // ขอบของคนในแถวนี้ รวมขอบนุ่ม (ผม ขอบผ้า)
        let row = &px[y * w..(y + 1) * w];
        let (mut l, mut r) = (min, max + 1);
        while l > 0 && row[l - 1] > SOFT {
            l -= 1;
        }
        while r < w && row[r] > SOFT {
            r += 1;
        }
        (left[i], right[i]) = (l as f64, r as f64);
    }
    // ลำตัวของแถว = ของแถวที่แคบที่สุดในช่วงใกล้ ๆ (แขนที่ยื่นออกไปแล้วติดกับตัวในบางแถว ไม่ทำให้ลำตัวกว้างหรือเส้นกลางเบี้ยว)
    // แล้วเกลี่ยทั้งสองค่าให้เรียบตามแนวตั้ง
    let win = ((MIN_WINDOW * h as f64) as usize).max(2);
    let narrowest: Vec<usize> = (0..n)
        .map(|i| {
            (i.saturating_sub(win)..(i + win + 1).min(n))
                .min_by(|&a, &b| {
                    half[a].total_cmp(&half[b]).then((a as i64 - i as i64).abs().cmp(&(b as i64 - i as i64).abs()))
                })
                .unwrap_or(i)
        })
        .collect();
    let sigma = (SMOOTH * h as f64).max(3.0);
    let centre = smooth(&narrowest.iter().map(|&j| centre[j]).collect::<Vec<_>>(), sigma);
    let half = smooth(&narrowest.iter().map(|&j| half[j]).collect::<Vec<_>>(), sigma);
    let x0 = left.iter().copied().fold(f64::MAX, f64::min);
    let x1 = right.iter().copied().fold(f64::MIN, f64::max);
    Person { top, centre, half, left, right, x0, x1 }
}

/// เกลี่ยแบบ Gaussian (ปลายสองข้างใช้ค่าที่ขอบ)
fn smooth(v: &[f64], sigma: f64) -> Vec<f64> {
    let reach = (3.0 * sigma).ceil() as i64;
    let kernel: Vec<f64> = (-reach..=reach).map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp()).collect();
    let total: f64 = kernel.iter().sum();
    let n = v.len() as i64;
    (0..n)
        .map(|i| {
            kernel.iter().enumerate().map(|(k, w)| w * v[(i + k as i64 - reach).clamp(0, n - 1) as usize]).sum::<f64>()
                / total
        })
        .collect()
}

// ───────────────────────── ฉากหลังที่เติมไว้ ─────────────────────────

/// ฉากหลังของรูปที่ตัวเติมเติมส่วนที่ตัวคนบังไว้แล้ว: ภาพขนาดเท่าภาพน้ำหนัก · alpha = ส่วนที่เติม (นอกนั้นว่าง)
#[derive(Clone)]
pub struct Fill {
    image: Arc<RgbaImage>,
}

impl Fill {
    pub fn new(image: RgbaImage) -> Fill {
        Fill { image: Arc::new(image) }
    }

    /// ไม่มีส่วนที่เติม (ไม่พบตัวคน)
    pub fn empty() -> Fill {
        Fill::new(RgbaImage::new(1, 1))
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// สีของฉากหลังที่เติมไว้ที่จุด (u, v) บนภาพเดิม (0–255 ของภาพตั้งต้น) และน้ำหนัก 0–1 — None = ไม่มีส่วนที่เติมตรงนั้น
    pub fn at(&self, u: f64, v: f64) -> Option<([f64; 3], f64)> {
        let (w, h) = self.image.dimensions();
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let x = (u * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let y = (v * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let (mut rgb, mut weight) = ([0.0f64; 3], 0.0f64);
        for (xx, yy, k) in
            [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x1, y0, fx * (1.0 - fy)), (x0, y1, (1.0 - fx) * fy), (x1, y1, fx * fy)]
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

/// ขยายส่วนที่เป็น true ออกไป `r` พิกเซลทุกทิศ (สี่เหลี่ยม · แยกแนวนอน/แนวตั้ง)
fn grow(on: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let pass = |src: &[bool], len: usize, lines: usize, at: &dyn Fn(usize, usize) -> usize| {
        let mut out = vec![false; src.len()];
        for line in 0..lines {
            let mut count = vec![0usize; len + 1];
            for i in 0..len {
                count[i + 1] = count[i] + src[at(line, i)] as usize;
            }
            for i in 0..len {
                let (a, b) = (i.saturating_sub(r), (i + r + 1).min(len));
                out[at(line, i)] = count[b] > count[a];
            }
        }
        out
    };
    let rows = pass(on, w, h, &|y, x| y * w + x);
    pass(&rows, h, w, &|x, y| y * w + x)
}

/// เติมฉากหลังตรงที่ตัวคนบัง: `base` = ภาพตั้งต้นของการแต่ง (ย่อให้เท่าภาพน้ำหนัก) · ส่งเฉพาะกรอบรอบตัวคน (+ ฉากรอบ ๆ)
/// ให้ตัวเติม แล้วขยายผลกลับมาวางเฉพาะในช่อง
pub fn fill(base: &RgbImage, map: &GrayImage, inpainter: &dyn Inpainter) -> Result<Fill> {
    let (w, h) = map.dimensions();
    let (wu, hu) = (w as usize, h as usize);
    let hole: Vec<bool> = map.as_raw().iter().map(|&v| v >= HOLE).collect();
    let grow_px = ((HOLE_GROW * w.max(h) as f64).round() as usize).max(2);
    let hole = grow(&hole, wu, hu, grow_px);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, _) in hole.iter().enumerate().filter(|(_, &on)| on) {
        let (x, y) = (i % wu, i / wu);
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
    }
    if x1 <= x0 || y1 <= y0 {
        return Ok(Fill::empty());
    }
    // กรอบที่ส่งให้ตัวเติม: รอบตัวคน + ฉากรอบ ๆ ทำให้ใกล้จัตุรัส (ตัวเติมรับภาพจัตุรัส ภาพไม่บิดมาก)
    let margin = (CONTEXT * (x1 - x0).max(y1 - y0) as f64) as usize;
    let (mut x0, mut y0, mut x1, mut y1) =
        (x0.saturating_sub(margin), y0.saturating_sub(margin), (x1 + margin).min(wu), (y1 + margin).min(hu));
    let side = (x1 - x0).max(y1 - y0);
    let widen = |a: &mut usize, b: &mut usize, limit: usize| {
        let extra = side.min(limit).saturating_sub(*b - *a);
        let before = (extra / 2).min(*a);
        *a -= before;
        *b = (*b + extra - before).min(limit);
        *a = (*b).saturating_sub(side.min(limit)).min(*a);
    };
    widen(&mut x0, &mut x1, wu);
    widen(&mut y0, &mut y1, hu);
    let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let base = if base.dimensions() == (w, h) {
        base.clone()
    } else {
        imageops::resize(base, w, h, imageops::FilterType::Triangle)
    };
    let s = inpainter.side();
    let crop = imageops::crop_imm(&base, x0 as u32, y0 as u32, cw, ch).to_image();
    let image = imageops::resize(&crop, s, s, imageops::FilterType::CatmullRom);
    let holes = GrayImage::from_fn(cw, ch, |x, y| {
        image::Luma([if hole[(y0 + y as usize) * wu + x0 + x as usize] { 255 } else { 0 }])
    });
    // ย่อแล้วยังนับเป็นช่องถ้ามีส่วนของช่องอยู่บ้าง (ช่องไม่หดตอนย่อ)
    let holes = imageops::resize(&holes, s, s, imageops::FilterType::Triangle);
    let holes = GrayImage::from_fn(s, s, |x, y| image::Luma([if holes.get_pixel(x, y)[0] > 0 { 255 } else { 0 }]));
    let filled = inpainter.inpaint(&image, &holes)?;
    if filled.dimensions() != (s, s) {
        return Err(AppError::internal("ตัวเติมฉากหลังคืนภาพผิดขนาด"));
    }
    let filled = imageops::resize(&filled, cw, ch, imageops::FilterType::CatmullRom);
    // สีที่เติมกับฉากจริงต่างกันเล็กน้อย (ตัวเติมทำงานที่ภาพย่อ) จนเห็นเป็นเส้นตรงขอบตัวคนเดิม: วัดความต่างตรงวงรอบตัวคน
    // ที่เห็นฉากจริง (ส่วนที่ขยายออกไป) แล้วเกลี่ยเข้าไปปรับสีของส่วนที่เติมทั้งหมด — ตรงขอบจึงต่อกับฉากจริงพอดี
    // (ไม่วัดตรงที่ชิดตัวคน: พิกเซลจริงตรงนั้นยังมีแสงจากขอบตัวคนซึมอยู่ — วัดแล้วฉากที่เติมสว่างเป็นเส้นตามรูปร่างเดิม ลองแล้ว)
    let (cwu, chu) = (cw as usize, ch as usize);
    let person: Vec<bool> = map.as_raw().iter().map(|&v| v >= HOLE).collect();
    let near = grow(&person, wu, hu, (grow_px / 3).max(2));
    let mut residual = vec![[0f32; 3]; cwu * chu];
    let mut known = vec![0f32; cwu * chu];
    for (x, y, p) in filled.enumerate_pixels() {
        let (gx, gy) = (x0 + x as usize, y0 + y as usize);
        let g = gy * wu + gx;
        if hole[g] && !near[g] {
            let real = base.get_pixel(gx as u32, gy as u32);
            let i = y as usize * cwu + x as usize;
            residual[i] = std::array::from_fn(|c| real[c] as f32 - p[c] as f32);
            known[i] = 1.0;
        }
    }
    let residual = pull_push(&residual, &known, cwu, chu);
    let mut out = RgbaImage::new(w, h);
    for (x, y, p) in filled.enumerate_pixels() {
        let (gx, gy) = (x0 as u32 + x, y0 as u32 + y);
        if hole[gy as usize * wu + gx as usize] {
            let r = residual[y as usize * cwu + x as usize];
            let c = |k: usize| (p[k] as f32 + r[k]).round().clamp(0.0, 255.0) as u8;
            out.put_pixel(gx, gy, image::Rgba([c(0), c(1), c(2), 255]));
        }
    }
    Ok(Fill::new(out))
}

/// เกลี่ยค่าที่รู้ (`known` = น้ำหนัก 0–1) ไปทั่วภาพแบบนุ่ม (pull-push: เฉลี่ยลงภาพย่อทีละครึ่ง แล้วเติมส่วนที่ไม่รู้จากภาพย่อ)
fn pull_push(values: &[[f32; 3]], known: &[f32], w: usize, h: usize) -> Vec<[f32; 3]> {
    if w <= 1 && h <= 1 {
        return values.to_vec();
    }
    let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
    let (mut cv, mut ck) = (vec![[0f32; 3]; nw * nh], vec![0f32; nw * nh]);
    for y in 0..h {
        for x in 0..w {
            let (i, j) = (y * w + x, (y / 2) * nw + x / 2);
            for c in 0..3 {
                cv[j][c] += values[i][c] * known[i];
            }
            ck[j] += known[i];
        }
    }
    for (v, k) in cv.iter_mut().zip(ck.iter_mut()) {
        if *k > 0.0 {
            *v = v.map(|c| c / *k);
        }
        *k = k.min(1.0);
    }
    let coarse = pull_push(&cv, &ck, nw, nh);
    // ภาพย่อขยายกลับแบบ bilinear (ไม่เป็นบล็อก)
    let at = |x: f32, y: f32| -> [f32; 3] {
        let (fx, fy) = (x.clamp(0.0, (nw - 1) as f32), y.clamp(0.0, (nh - 1) as f32));
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(nw - 1), (y0 + 1).min(nh - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let p = |xx: usize, yy: usize| coarse[yy * nw + xx];
        std::array::from_fn(|c| {
            (p(x0, y0)[c] * (1.0 - tx) + p(x1, y0)[c] * tx) * (1.0 - ty)
                + (p(x0, y1)[c] * (1.0 - tx) + p(x1, y1)[c] * tx) * ty
        })
    };
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let c = at((x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5);
            std::array::from_fn(|k| c[k] + (values[i][k] - c[k]) * known[i])
        })
        .collect()
}

// ───────────────────────── หดลำตัว ─────────────────────────

/// สีของฉากหลังที่เติม (0–255 ของภาพตั้งต้น 8 บิต) ในหน่วยของภาพที่กำลังสร้าง
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Space {
    /// ภาพ 8 บิต (JPEG หรือโทนตั้งต้นของ RAW — ภาพเดียวกับที่ใช้เติม)
    Bytes,
    /// แสงเชิงเส้นของ RAW (ภาพตั้งต้น 8 บิตคือโทนตั้งต้นของ RAW — แปลงกลับด้วยเส้นโค้งเดียวกัน)
    RawLinear,
}

impl Space {
    fn convert<const N: usize>(self, rgb: [f64; 3], alpha: f64) -> [f64; N] {
        std::array::from_fn(|c| match (c, self) {
            (3, _) => alpha,
            (_, Space::Bytes) => rgb[c.min(2)],
            (_, Space::RawLinear) => crate::adjust::decode_for(crate::adjust::Profile::Raw, rgb[c.min(2)] / 255.0),
        })
    }
}

/// สิ่งที่ตัวผอมใช้กับรูปหนึ่ง: ภาพน้ำหนัก ฉากหลังที่เติมไว้ และจุดเริ่มหดของแต่ละคน (แนบให้ค่าที่แต่งตอนสร้างภาพ)
#[derive(Clone)]
pub struct BodyPlan {
    map: SubjectMap,
    fill: Fill,
    /// ต่อคน (ลำดับเดียวกับ [`SubjectMap::people`]): แถวที่เริ่มหด และแถวที่หดเต็ม (พิกเซลของภาพน้ำหนัก)
    necks: Vec<(f64, f64)>,
}

impl BodyPlan {
    /// `faces` = กรอบใบหน้า [x, y, w, h] (สัดส่วนบนภาพเดิม) ใช้หาคอของแต่ละคน
    pub fn new(map: SubjectMap, fill: Fill, faces: &[[f64; 4]]) -> BodyPlan {
        let (w, h) = (map.image().width() as f64, map.image().height() as f64);
        let necks = map
            .people()
            .iter()
            .map(|p| {
                let (top, n) = (p.top as f64, p.rows() as f64);
                let mut neck = None::<(f64, f64)>;
                for f in faces {
                    let (cx, cy) = ((f[0] + f[2] / 2.0) * w, (f[1] + f[3] / 2.0) * h);
                    // ใบหน้าของคนนี้: อยู่ในความกว้างของคนนี้ และในครึ่งบนของคนนี้
                    if (p.x0..p.x1).contains(&cx) && (top..top + 0.5 * n).contains(&cy) {
                        let chin = (f[1] + f[3]) * h;
                        let span = NECK_SPAN * f[3] * h;
                        if neck.is_none_or(|(c, _)| chin > c) {
                            neck = Some((chin, chin + span));
                        }
                    }
                }
                neck.unwrap_or((top + HEAD_FROM * n, top + (HEAD_FROM + HEAD_SPAN) * n))
            })
            .collect();
        BodyPlan { map, fill, necks }
    }

    /// จุดต้นทาง (u บนภาพเดิม) ของชั้นตัวคนที่จุด (u, v) — None = ไม่ขยับ · `k` = สัดส่วนที่หด
    fn source_u(&self, k: f64, u: f64, v: f64) -> Option<f64> {
        let map = self.map.image();
        let (w, h) = (map.width() as f64, map.height() as f64);
        let (x, y) = (u * w, v * h - 0.5);
        for (p, &(from, full)) in self.map.people().iter().zip(&self.necks) {
            let n = p.rows();
            let ry = y - p.top as f64;
            if ry < -0.5 || ry > n as f64 - 0.5 {
                continue;
            }
            let i0 = (ry.floor().max(0.0) as usize).min(n - 1);
            let i1 = (i0 + 1).min(n - 1);
            let f = (ry - i0 as f64).clamp(0.0, 1.0);
            // รวมแถบนอกขอบตัวคนที่ฉากที่เติมค่อย ๆ จางไปหาฉากจริง
            let reach = (FEATHER.len() - 1) as f64 + 1.0;
            if x < p.left[i0].min(p.left[i1]) - reach || x > p.right[i0].max(p.right[i1]) + reach {
                continue;
            }
            let kk = k * smoothstep(from, full, y);
            if kk <= 0.0 {
                return None;
            }
            let at = |a: &[f64]| a[i0] * (1.0 - f) + a[i1] * f;
            let (cx, half) = (at(&p.centre), at(&p.half).max(1.0));
            let o = x - cx;
            return Some((cx + o.signum() * unshift(o.abs(), kk, half)) / w);
        }
        None
    }

    /// สีที่จุด (u, v) บนภาพเดิมหลังตัวผอม `amount` (0–100) · `fetch` = สีของภาพเดิมที่จุดใดก็ได้ (สัดส่วน) ·
    /// `space` = หน่วยสีของภาพนี้ (แปลงสีของฉากหลังที่เติม) — ช่องที่ 4 (ถ้ามี) เป็นความทึบ
    pub fn sample<const N: usize>(
        &self,
        amount: f64,
        u: f64,
        v: f64,
        space: Space,
        alpha_max: f64,
        fetch: impl Fn(f64, f64) -> [f64; N],
    ) -> [f64; N] {
        let here = fetch(u, v);
        let k = amount.clamp(0.0, 100.0) / 100.0 * MAX_SLIM;
        let Some(su) = (k > 0.0).then(|| self.source_u(k, u, v)).flatten() else { return here };
        // ขยับไม่ถึงครึ่งพิกเซลของภาพน้ำหนัก: ผสมกับภาพเดิม (ใกล้เส้นกลางลำตัว ไม่มีรอยต่อ)
        let moved = ((su - u).abs() * self.map.image().width() as f64 * 2.0).min(1.0);
        if moved <= 0.0 {
            return here;
        }
        let person = fetch(su, v);
        let a = self.map.weight(su, v);
        // ฉากหลังที่จุดนี้: ส่วนที่ตัวคนเคยบัง ใช้ฉากที่เติมไว้ · ส่วนที่เห็นฉากจริงอยู่แล้วใช้พิกเซลเดิม — ขอบนุ่มของตัวคนเดิม
        // และพิกเซลถัดออกไปอีกหนึ่งพิกเซลของภาพน้ำหนัก (สีของตัวคนยังซึมอยู่แม้น้ำหนักเกือบศูนย์) นับเป็นส่วนที่บัง
        // ไม่งั้นเห็นเส้นจาง ๆ ตามรูปร่างเดิม · ไกลออกไปอีก [`FEATHER`] ค่อย ๆ จางไปหาฉากจริง (ฉากที่เติมละเอียดน้อยกว่า
        // ตัดทันทีแล้วเห็นรอยต่อเป็นขั้นในไฟล์ขนาดเต็ม) — ลองกับรูปจริงแล้วทั้งสองอย่าง
        let behind = match self.fill.at(u, v) {
            Some((rgb, m)) => {
                let step = 1.0 / self.map.image().width() as f64;
                let seen = FEATHER
                    .iter()
                    .enumerate()
                    .flat_map(|(k, f)| [-1.0, 1.0].map(|side| self.map.weight(u + side * k as f64 * step, v) * f))
                    .fold(0.0, f64::max);
                let t = m * (seen / EDGE_SEEN).min(1.0);
                let plate: [f64; N] = space.convert(rgb, alpha_max);
                std::array::from_fn(|c| here[c] + (plate[c] - here[c]) * t)
            }
            None => here,
        };
        std::array::from_fn(|c| {
            let composed = person[c] * a + behind[c] * (1.0 - a);
            here[c] + (composed - here[c]) * moved
        })
    }
}

/// ระยะจากเส้นกลางของจุดต้นทาง ที่หลังหดแล้วอยู่ห่างเส้นกลาง `d` — ในลำตัว (`half`) หดตามสัดส่วน `k` ·
/// แถบนอกลำตัวขยับน้อยลงจนหมด · หดแล้วลำดับของพิกเซลไม่สลับกัน (ระยะหลังหดเพิ่มขึ้นตามระยะเดิมเสมอ)
fn unshift(d: f64, k: f64, half: f64) -> f64 {
    let band = BAND * half;
    if d <= half * (1.0 - k) {
        return d / (1.0 - k);
    }
    if d >= half + band {
        return d;
    }
    let after = |a: f64| a - k * half * (1.0 - smoothstep(half, half + band, a));
    let (mut lo, mut hi) = (half, half + band);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if after(mid) < d {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// แผนที่แนบให้ค่าที่แต่งตอนสร้างภาพ — ไม่ใช่ส่วนของค่าที่แต่ง: ค่าที่แต่งสองชุดเท่ากันไม่ว่าจะแนบหรือไม่ และไม่ถูกเก็บ
#[derive(Clone, Default)]
pub struct Attached(pub Option<BodyPlan>);

impl PartialEq for Attached {
    fn eq(&self, _: &Attached) -> bool {
        true
    }
}

impl std::fmt::Debug for Attached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BodyPlan({})", if self.0.is_some() { "แนบแล้ว" } else { "ยังไม่แนบ" })
    }
}

/// ตัวผอมบนภาพเดิม (ก่อนตัด/หมุน) — ภาพ 8 บิต หรือแสงเชิงเส้นของ RAW (ทศนิยม) · `point` = จุดบนภาพเดิมที่แต่ละพิกเซลนำสีมา
/// ก่อนตัวผอม (หน้าเรียว/หน้าผอม — ทำรอบเดียวกัน)
pub fn apply(
    img: DynamicImage,
    amount: f64,
    plan: &BodyPlan,
    point: impl Fn(f64, f64) -> (f64, f64) + Sync,
) -> DynamicImage {
    match img {
        DynamicImage::ImageRgb8(src) => {
            DynamicImage::ImageRgb8(warp::<_, 3>(&src, amount, plan, Space::Bytes, 255.0, &point))
        }
        DynamicImage::ImageRgba8(src) => {
            DynamicImage::ImageRgba8(warp::<_, 4>(&src, amount, plan, Space::Bytes, 255.0, &point))
        }
        DynamicImage::ImageRgb32F(src) => {
            DynamicImage::ImageRgb32F(warp::<_, 3>(&src, amount, plan, Space::RawLinear, 1.0, &point))
        }
        other => DynamicImage::ImageRgba8(warp::<_, 4>(&other.to_rgba8(), amount, plan, Space::Bytes, 255.0, &point)),
    }
}

fn warp<P, const N: usize>(
    src: &ImageBuffer<P, Vec<P::Subpixel>>,
    amount: f64,
    plan: &BodyPlan,
    space: Space,
    alpha_max: f64,
    point: &(impl Fn(f64, f64) -> (f64, f64) + Sync),
) -> ImageBuffer<P, Vec<P::Subpixel>>
where
    P: Pixel + Send + Sync,
    P::Subpixel: crate::edit::Channel + Send + Sync,
{
    use crate::edit::Channel;
    let (w, h) = (src.width() as usize, src.height() as usize);
    let raw = src.as_raw();
    let fetch = |u: f64, v: f64| -> [f64; N] {
        let fx = (u * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let fy = (v * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let at = |xx: usize, yy: usize, c: usize| raw[(yy * w + xx) * N + c].get();
        std::array::from_fn(|c| {
            let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
            let bottom = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
            top * (1.0 - ty) + bottom * ty
        })
    };
    let mut out = raw.clone();
    out.par_chunks_mut(w * N).enumerate().for_each(|(y, line)| {
        let v = (y as f64 + 0.5) / h as f64;
        for x in 0..w {
            let (u, v) = point((x as f64 + 0.5) / w as f64, v);
            let value = plan.sample::<N>(amount, u, v, space, alpha_max, &fetch);
            for c in 0..N {
                line[x * N + c] = Channel::put(value[c]);
            }
        }
    });
    ImageBuffer::from_raw(w as u32, h as u32, out).expect("ขนาดเท่าเดิม")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb};

    /// คนยืนตรงกลางภาพ 200×300: หัววงกลม (y 20–60) ลำตัวกว้าง 60 (x 70–130, y 60–290) แขนยื่นออกไปทางขวาที่ y 150–158
    fn figure() -> GrayImage {
        GrayImage::from_fn(200, 300, |x, y| {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            let head = (fx - 100.0).powi(2) + (fy - 40.0).powi(2) <= 20.0f64.powi(2);
            let body = (70.0..130.0).contains(&fx) && (60.0..290.0).contains(&fy);
            let arm = (130.0..185.0).contains(&fx) && (150.0..158.0).contains(&fy);
            Luma([if head || body || arm { 255 } else { 0 }])
        })
    }

    /// ฉากหลังเป็นแถบสีตามแนวตั้ง · ตัวคนสีแดง
    fn scene(map: &GrayImage) -> RgbImage {
        RgbImage::from_fn(map.width(), map.height(), |x, y| {
            if map.get_pixel(x, y)[0] > 127 {
                Rgb([220, 30, 30])
            } else {
                Rgb([40, (x * 255 / 200) as u8, 200])
            }
        })
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

    fn plan(faces: &[[f64; 4]]) -> (BodyPlan, RgbImage) {
        let map = figure();
        let img = scene(&map);
        let fill = fill(&img, &map, &Flat).unwrap();
        (BodyPlan::new(SubjectMap::new(map), fill, faces), img)
    }

    fn red(p: &Rgb<u8>) -> bool {
        p[0] > 150 && p[1] < 100
    }

    fn red_span(img: &RgbImage, y: u32) -> (u32, u32) {
        let xs: Vec<u32> = (0..img.width()).filter(|&x| red(img.get_pixel(x, y))).collect();
        (xs[0], xs[xs.len() - 1] + 1)
    }

    #[test]
    fn people_are_found_with_their_torso_not_their_arms() {
        let people = people(&figure());
        assert_eq!(people.len(), 1);
        let p = &people[0];
        let at = |y: usize| y - p.top;
        // ลำตัว 70–130: เส้นกลาง 100 ครึ่งความกว้าง 30 แม้แถวที่มีแขน
        for y in [100, 154, 250] {
            assert!((p.centre[at(y)] - 100.0).abs() < 2.0, "เส้นกลางแถว {y}: {}", p.centre[at(y)]);
            assert!((p.half[at(y)] - 30.0).abs() < 2.0, "ครึ่งความกว้างแถว {y}: {}", p.half[at(y)]);
        }
        assert_eq!(p.right[at(154)], 185.0, "แถวที่มีแขน: ขอบขวาคือปลายแขน");
        assert!(super::people(&GrayImage::new(50, 50)).is_empty());
    }

    #[test]
    fn unshift_inverts_the_squeeze_and_keeps_pixels_in_order() {
        let (k, half) = (0.1, 30.0);
        let squeeze =
            |a: f64| if a <= half { a * (1.0 - k) } else { a - k * half * (1.0 - smoothstep(half, half * 1.6, a)) };
        let mut last = -1.0;
        for i in 0..=600 {
            let d = i as f64 * 0.1;
            let a = unshift(d, k, half);
            assert!((squeeze(a) - d).abs() < 1e-6, "{d} → {a}");
            assert!(a > last, "เรียงตามเดิม");
            last = a;
        }
        assert_eq!(unshift(50.0, k, half), 50.0, "นอกแถบไม่ขยับ");
    }

    #[test]
    fn the_torso_narrows_the_head_and_the_background_stay() {
        let (plan, img) = plan(&[]);
        let out = apply(DynamicImage::ImageRgb8(img.clone()), 100.0, &plan, |u, v| (u, v)).into_rgb8();
        // ลำตัวแคบลงราว 12% ทั้งสองข้าง (เส้นกลางเดิม)
        let (l, r) = red_span(&out, 250);
        assert!((r - l) <= 54 && (r - l) >= 50, "ลำตัว 60 → {}", r - l);
        assert!(((l + r) as i32 - 200).abs() <= 2, "เส้นกลางเดิม {l}..{r}");
        // หัวไม่ขยับเลย
        for y in 20..60 {
            for x in 70..130 {
                assert_eq!(out.get_pixel(x, y), img.get_pixel(x, y), "หัว ({x},{y})");
            }
        }
        // ตรงที่ตัวคนหดออกเป็นฉากที่เติม (ไม่ใช่สีแดง และไม่ใช่สีของแถบที่ยืดมา) · ฉากหลังที่ไกลออกไปเท่าเดิมทุกพิกเซล
        // (ตัวเติมจำลองเติมสีเดียว แต่สีถูกปรับให้ต่อกับฉากจริงรอบ ๆ — ฉากทดสอบไล่สีเขียวตามแนวนอน)
        let gap = out.get_pixel(l - 2, 250);
        let real_g = ((l - 2) * 255 / 200) as u8;
        assert!(!red(gap) && gap[1].abs_diff(real_g) <= 12 && gap[2] >= 190, "ช่องที่หดออก {gap:?} ฉากจริง {real_g}");
        for (x, y) in [(10, 250), (60, 250), (140, 250), (190, 100), (60, 10)] {
            assert_eq!(out.get_pixel(x, y), img.get_pixel(x, y), "ฉากหลัง ({x},{y})");
        }
        // แขนที่ยื่นออกไปไกล: ปลายแขนไม่ขยับ
        assert_eq!(red_span(&out, 154).1, 185);
        // ไม่ใส่ = ภาพเดิม
        let same = apply(DynamicImage::ImageRgb8(img.clone()), 0.0, &plan, |u, v| (u, v)).into_rgb8();
        assert_eq!(same, img);
    }

    #[test]
    fn raw_linear_images_get_the_fill_in_linear_light() {
        // แสงเชิงเส้นของ RAW: ตรงที่หดออกได้สีที่เติม (8 บิตของโทนตั้งต้น) แปลงกลับเป็นแสงเชิงเส้น
        let (plan, img) = plan(&[]);
        let lin = image::Rgb32FImage::from_fn(img.width(), img.height(), |x, y| {
            let p = img.get_pixel(x, y);
            image::Rgb(p.0.map(|c| crate::adjust::decode_for(crate::adjust::Profile::Raw, c as f64 / 255.0) as f32))
        });
        let as_bytes = apply(DynamicImage::ImageRgb8(img.clone()), 100.0, &plan, |u, v| (u, v)).into_rgb8();
        let as_linear = apply(DynamicImage::ImageRgb32F(lin), 100.0, &plan, |u, v| (u, v)).into_rgb32f();
        let (l, _) = red_span(&as_bytes, 250);
        let (b, f) = (as_bytes.get_pixel(l - 2, 250), as_linear.get_pixel(l - 2, 250));
        for c in 0..3 {
            let want = crate::adjust::decode_for(crate::adjust::Profile::Raw, b[c] as f64 / 255.0);
            assert!((f[c] as f64 - want).abs() < 0.01, "ช่อง {c}: {} {want}", f[c]);
        }
    }

    #[test]
    fn a_face_box_protects_the_head_down_to_the_chin() {
        // กรอบใบหน้าลงไปถึง y 100 (สัดส่วน 1/3): ลำตัวใต้หัวที่ y 80 ยังไม่หด · ไม่มีกรอบ = หดแล้ว (เริ่มที่ 20% ของความสูงคน)
        let face = [80.0 / 200.0, 20.0 / 300.0, 40.0 / 200.0, 80.0 / 300.0];
        let (with_face, img) = plan(&[face]);
        let (plain, _) = plan(&[]);
        let width = |p: &BodyPlan, y: u32| {
            let out = apply(DynamicImage::ImageRgb8(img.clone()), 100.0, p, |u, v| (u, v)).into_rgb8();
            let (l, r) = red_span(&out, y);
            r - l
        };
        assert_eq!(width(&with_face, 95), 60, "เหนือคาง");
        assert!(width(&plain, 95) < 58, "ไม่มีกรอบใบหน้า");
        assert!(width(&with_face, 250) < 55, "ลำตัวด้านล่างหดเหมือนกัน");
    }

    #[test]
    fn pull_push_keeps_what_is_known_and_spreads_it_smoothly() {
        // รู้ค่าแค่สองข้าง (ซ้าย 0 ขวา 100): ข้างที่รู้คงเดิม ตรงกลางอยู่ระหว่างสองค่า
        let (w, h) = (32, 8);
        let known: Vec<f32> = (0..w * h).map(|i| if i % w < 2 || i % w >= w - 2 { 1.0 } else { 0.0 }).collect();
        let values: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                [if i % w < 2 {
                    0.0
                } else if i % w >= w - 2 {
                    100.0
                } else {
                    999.0
                }; 3]
            })
            .collect();
        let out = pull_push(&values, &known, w, h);
        assert_eq!(out[0], [0.0; 3]);
        assert_eq!(out[w - 1], [100.0; 3]);
        let mid = out[4 * w + w / 2][0];
        assert!((20.0..80.0).contains(&mid), "ตรงกลาง {mid}");
        let row: Vec<f32> = (0..w).map(|x| out[4 * w + x][0]).collect();
        assert!(row.windows(2).all(|p| p[1] >= p[0] - 1e-3), "ไล่ขึ้นเรียบ {row:?}");
    }

    #[test]
    fn the_fill_covers_the_person_and_a_little_more() {
        let map = figure();
        let fill = fill(&scene(&map), &map, &Flat).unwrap();
        let (centre, _) = fill.at(0.5, 0.5).expect("กลางตัวคนมีฉากที่เติม");
        assert!(centre[0] < 80.0 && centre[2] > 180.0, "ไม่ใช่สีแดงของตัวคน {centre:?}");
        // วงรอบตัวคนที่เห็นฉากจริง (นอกสุดของช่อง): สีที่เติมเท่าฉากจริง · ชิดตัวคนสีค่อย ๆ เปลี่ยนไปหาสีที่ตัวเติมเติม
        let (ring, _) = fill.at(67.5 / 200.0, 0.5).unwrap();
        assert!((ring[1] - 85.0).abs() <= 1.0, "วงรอบตัวคน {ring:?}");
        let (inside, _) = fill.at(69.5 / 200.0, 0.5).unwrap();
        assert!((inside[1] - 85.0).abs() <= 12.0, "ชิดตัวคน {inside:?}");
        assert!(fill.at(0.05, 0.5).is_none(), "ฉากหลังไกล ๆ ไม่เติม");
        // ขยายออกจากขอบตัวคน 1% ของด้านยาว (3 พิกเซล)
        assert!(fill.at(68.5 / 200.0, 0.5).is_some() && fill.at(64.5 / 200.0, 0.5).is_none());
        assert!(super::fill(&scene(&map), &GrayImage::new(200, 300), &Flat).unwrap().at(0.5, 0.5).is_none());
    }
}
