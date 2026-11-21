//! AI ช่วยคัดภาพ (DECISIONS ข้อ 37): วัดความคมของแต่ละรูปและความเหมือนระหว่างรูป แล้วเลือกรูปที่ดีที่สุดของแต่ละช่วงที่ถ่ายต่อเนื่อง
//! และไม่เอารูปที่เบลอกว่ารูปอื่นในกิจกรรมมาก — ระดับการคัด 1 (เบามาก ได้รูปมาก) ถึง 5 (เข้มมาก ได้รูปน้อย)
//!
//! คำนวณในเครื่องทั้งหมดจากภาพที่ย่อด้านยาว [`MEASURE_EDGE`] ไม่ส่งรูปออกไปไหน · ไม่ใช้โมเดล AI (ใช้ใบหน้าที่ตัววิเคราะห์ใบหน้าหาไว้ถ้ามี)

use image::{imageops, GrayImage, RgbImage};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{AppError, Result};
use crate::jobs::{self, Control, JobCtx, JobSnapshot, Outcome, ACTIVE_STATES};
use crate::picks::Flag;
use crate::{db, util, App};

/// ด้านยาวของภาพที่ใช้วัด (ภาพทุกขนาดย่อมาเท่ากันก่อน ค่าความคมจึงเทียบกันได้)
pub const MEASURE_EDGE: u32 = 1500;
/// รุ่นของสูตรวัด — เปลี่ยนสูตรเมื่อไรเพิ่มเลข รูปที่วัดด้วยรุ่นเก่าจะถูกวัดใหม่
pub const VERSION: i64 = 2;
/// ระดับการคัดที่มี (1 = เบามาก … 5 = เข้มมาก) และค่าตั้งต้น — ระดับ 2 เก็บใกล้กับที่หลวงพี่คัดเองที่สุด
/// (110 รูป: เก็บ 59 หลวงพี่เลือก 61 — TEST_REPORT)
pub const LEVELS: u8 = 5;
pub const DEFAULT_LEVEL: u8 = 2;
/// ช่วงที่เบลอทั้งช่วง: ระดับไม่เกินนี้ยังเก็บรูปที่ดีที่สุดของช่วงไว้ (ไม่ให้ช่วงนั้นหายไปทั้งช่วง)
const KEEP_MOMENT_UNTIL: u8 = 3;

const TILE: usize = 64;
/// ช่องที่มีรายละเอียดพอจะวัดความคม: ส่วนเบี่ยงเบนของความสว่าง (0–1) อย่างน้อยเท่านี้
const TILE_MIN_STD: f32 = 0.03;
/// ใบหน้าที่เล็กกว่านี้ (พิกเซลบนภาพที่วัด) วัดความคมไม่น่าเชื่อ
const FACE_MIN_PX: f64 = 40.0;
const THUMB_LONG: u32 = 24;
/// เทียบความคมกับรูปที่ถ่ายก่อน/หลังข้างละกี่รูป
const NEIGHBOURS: usize = 6;
/// ใบหน้าเบลอเมื่อขอบกว้างกว่ารูปข้างเคียง (ความคมจากความกว้างของขอบต่ำกว่าค่ากลางเท่านี้) ด้วย
const EDGE_BLUR: f32 = 0.8;
const THUMB_SHORT: u32 = 16;

/// ผลวัดของรูปหนึ่งรูป (เก็บใน `photo_quality`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    /// ความคมของส่วนที่คมที่สุดของภาพ (ช่องที่คม 10% บน) — ภาพที่ตั้งใจให้ฉากหลังเบลอยังได้ค่าสูงจากตัวแบบ
    pub sharpness: f32,
    /// ความคมของใบหน้าที่ใหญ่พอ ที่ขนาดจริง (เฉลี่ยตามขนาด) — ใช้เลือกใบที่ดีที่สุดในช่วงที่ถ่ายต่อเนื่อง · None = ไม่มีใบหน้าที่วัดได้
    pub face_sharpness: Option<f32>,
    /// ความคมของใบหน้าเดียวกันจากความกว้างของขอบ — ใช้ตัดสินว่าเบลอ (หน้าใหญ่แสงนุ่มผิวเรียบไม่ถูกนับว่าเบลอ)
    pub face_detail: Option<f32>,
    /// จำนวนใบหน้าที่ใช้วัด
    pub faces: u32,
    /// ความสว่างเฉลี่ย 0–1
    pub brightness: f32,
    /// สัดส่วนพิกเซลที่ขาวหรือดำจนหมดรายละเอียด
    pub clipped: f32,
    /// ภาพย่อความสว่าง 24×16 (แนวตั้ง 16×24) สำหรับเทียบความเหมือน
    pub thumb: Vec<u8>,
    pub portrait: bool,
    /// สีโดยรวม 4×4×4 ช่อง (สัดส่วน × 255)
    pub colors: Vec<u8>,
}

/// วัดรูปที่ตั้งตรงแล้ว · `faces` = กรอบใบหน้า [x, y, w, h] เป็นสัดส่วนบนภาพนี้
pub fn measure(img: &RgbImage, faces: &[[f64; 4]]) -> Quality {
    let (w0, h0) = img.dimensions();
    let scale = f64::from(MEASURE_EDGE) / f64::from(w0.max(h0).max(1));
    let small;
    let img = if scale < 1.0 {
        let (w, h) = (((f64::from(w0) * scale).round() as u32).max(1), ((f64::from(h0) * scale).round() as u32).max(1));
        small = imageops::resize(img, w, h, imageops::FilterType::Triangle);
        &small
    } else {
        img
    };
    let (w, h) = (img.width() as usize, img.height() as usize);
    let luma: Vec<f32> = img
        .pixels()
        .map(|p| (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])) / 255.0)
        .collect();
    let smooth = blur3(&luma, w, h);
    let lap = laplacian(&smooth, w, h);

    let mut tiles: Vec<f32> = Vec::new();
    let mut fallback: Vec<f32> = Vec::new();
    for ty in (0..h.saturating_sub(TILE / 2)).step_by(TILE) {
        for tx in (0..w.saturating_sub(TILE / 2)).step_by(TILE) {
            let (x1, y1) = ((tx + TILE).min(w), (ty + TILE).min(h));
            let (score, std) = region_sharpness(&smooth, &lap, w, tx, ty, x1, y1);
            fallback.push(score);
            if std >= TILE_MIN_STD {
                tiles.push(score);
            }
        }
    }
    let pool = if tiles.len() >= 4 { tiles } else { fallback };
    let sharpness = percentile(pool, 0.9);

    let mut face_sum = 0.0f64;
    let mut detail_sum = 0.0f64;
    let mut face_weight = 0.0f64;
    let mut used = 0u32;
    let mut sized: Vec<&[f64; 4]> = faces.iter().filter(|b| b[3] * h as f64 >= FACE_MIN_PX).collect();
    sized.sort_by(|a, b| (b[2] * b[3]).total_cmp(&(a[2] * a[3])));
    for b in sized.into_iter().take(5) {
        let clamp = |v: f64, n: usize| (v * n as f64).round().clamp(0.0, n as f64) as usize;
        let (x0, y0, x1, y1) = (clamp(b[0], w), clamp(b[1], h), clamp(b[0] + b[2], w), clamp(b[1] + b[3], h));
        if x1 <= x0 + 8 || y1 <= y0 + 8 {
            continue;
        }
        let native = region_sharpness(&smooth, &lap, w, x0, y0, x1, y1).0;
        let detail = edge_sharpness(&smooth, w, (x0, y0, x1, y1)).unwrap_or(native);
        let area = ((x1 - x0) * (y1 - y0)) as f64;
        face_sum += f64::from(native) * area;
        detail_sum += f64::from(detail) * area;
        face_weight += area;
        used += 1;
    }
    let face_sharpness = (face_weight > 0.0).then(|| (face_sum / face_weight) as f32);
    let face_detail = (face_weight > 0.0).then(|| (detail_sum / face_weight) as f32);

    let brightness = luma.iter().sum::<f32>() / luma.len().max(1) as f32;
    let clipped = img.pixels().filter(|p| p.0.iter().all(|&c| c >= 250) || p.0.iter().all(|&c| c <= 5)).count() as f32
        / (w * h).max(1) as f32;

    let portrait = h > w;
    let (tw, th) = if portrait { (THUMB_SHORT, THUMB_LONG) } else { (THUMB_LONG, THUMB_SHORT) };
    let gray = GrayImage::from_fn(w as u32, h as u32, |x, y| {
        image::Luma([(luma[y as usize * w + x as usize] * 255.0).round().clamp(0.0, 255.0) as u8])
    });
    let thumb = imageops::resize(&gray, tw, th, imageops::FilterType::Triangle).into_raw();

    let tiny = imageops::resize(img, 64, 64, imageops::FilterType::Triangle);
    let mut hist = [0u32; 64];
    for p in tiny.pixels() {
        let bin = |c: u8| usize::from(c >> 6);
        hist[bin(p[0]) * 16 + bin(p[1]) * 4 + bin(p[2])] += 1;
    }
    let total = hist.iter().sum::<u32>().max(1) as f32;
    let colors = hist.iter().map(|&n| (n as f32 / total * 255.0).round() as u8).collect();

    Quality { sharpness, face_sharpness, face_detail, faces: used, brightness, clipped, thumb, portrait, colors }
}

/// ความคมของใบหน้าจากความกว้างของขอบ (ตา คิ้ว ปาก ไรผม): ขอบที่คมแคบเสมอไม่ว่าผิวจะเรียบแค่ไหน ขอบที่หลุดโฟกัสกว้าง
/// (แบบ Marziliano) — คืน 10 ÷ ความกว้างเฉลี่ย (พิกเซล) ยิ่งมากยิ่งคม · None = ขอบไม่พอวัด
fn edge_sharpness(l: &[f32], w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> Option<f32> {
    if x1 < x0 + 8 || y1 < y0 + 8 {
        return None;
    }
    let mut widths: Vec<f32> = Vec::new();
    // ตามแนวนอน (ขอบแนวตั้ง) แล้วตามแนวตั้ง (ขอบแนวนอน)
    for horizontal in [true, false] {
        let (outer, inner) = if horizontal { ((y0..y1), (x0, x1)) } else { ((x0..x1), (y0, y1)) };
        let at = |o: usize, i: usize| if horizontal { l[o * w + i] } else { l[i * w + o] };
        let mut grads: Vec<f32> = Vec::new();
        for o in outer.clone() {
            for i in inner.0 + 1..inner.1 - 1 {
                grads.push((at(o, i + 1) - at(o, i - 1)).abs());
            }
        }
        grads.sort_by(f32::total_cmp);
        let threshold = grads.get(grads.len() * 9 / 10).copied().unwrap_or(0.0).max(0.04);
        for o in outer {
            for i in inner.0 + 2..inner.1 - 2 {
                let g = at(o, i + 1) - at(o, i - 1);
                let (prev, next) = (at(o, i) - at(o, i - 2), at(o, i + 2) - at(o, i));
                if g.abs() < threshold || g.abs() < prev.abs() || g.abs() < next.abs() {
                    continue;
                }
                // เดินออกสองข้างจนความสว่างหยุดเปลี่ยนไปทางเดียวกับขอบ
                let rising = g > 0.0;
                let mut left = i;
                while left > inner.0
                    && (if rising { at(o, left - 1) < at(o, left) } else { at(o, left - 1) > at(o, left) })
                {
                    left -= 1;
                }
                let mut right = i;
                while right + 1 < inner.1
                    && (if rising { at(o, right + 1) > at(o, right) } else { at(o, right + 1) < at(o, right) })
                {
                    right += 1;
                }
                widths.push((right - left) as f32);
            }
        }
    }
    if widths.len() < 12 {
        return None;
    }
    widths.sort_by(f32::total_cmp);
    // ขอบที่แคบที่สุดหนึ่งในสี่ (ขนตา มุมตา ริมฝีปาก) — หน้าที่คมมีขอบแคบเสมอ หน้าที่หลุดโฟกัสแม้ขอบที่แคบที่สุดก็กว้าง
    Some(10.0 / widths[widths.len() / 4].max(1.0))
}

/// เบลอเล็กน้อย 3×3 (1-2-1) ลดผลของ noise ก่อนหาขอบ
fn blur3(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut tmp = vec![0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            let l = src[y * w + x.saturating_sub(1)];
            let r = src[y * w + (x + 1).min(w - 1)];
            tmp[y * w + x] = (l + 2.0 * src[y * w + x] + r) / 4.0;
        }
    }
    let mut out = vec![0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            let u = tmp[y.saturating_sub(1) * w + x];
            let d = tmp[(y + 1).min(h - 1) * w + x];
            out[y * w + x] = (u + 2.0 * tmp[y * w + x] + d) / 4.0;
        }
    }
    out
}

/// Laplacian 4 ทิศ
fn laplacian(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; src.len()];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            out[i] = src[i - 1] + src[i + 1] + src[i - w] + src[i + w] - 4.0 * src[i];
        }
    }
    out
}

/// ความคมของบริเวณ: พลังงานของ Laplacian ต่อความต่างของความสว่างในบริเวณนั้น (ไม่ขึ้นกับว่าบริเวณสว่าง/มืดหรือคอนทราสต์มากแค่ไหน
/// ขึ้นกับว่าขอบคมแค่ไหน) — คืน (ความคม, ส่วนเบี่ยงเบนของความสว่าง)
fn region_sharpness(luma: &[f32], lap: &[f32], w: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> (f32, f32) {
    let (mut s, mut s2, mut e, mut n) = (0f64, 0f64, 0f64, 0f64);
    for y in y0..y1 {
        for x in x0..x1 {
            let (v, l) = (f64::from(luma[y * w + x]), f64::from(lap[y * w + x]));
            s += v;
            s2 += v * v;
            e += l * l;
            n += 1.0;
        }
    }
    if n < 1.0 {
        return (0.0, 0.0);
    }
    let var = (s2 / n - (s / n).powi(2)).max(0.0);
    ((e / n / (var + 1e-4) * 1000.0) as f32, var.sqrt() as f32)
}

fn percentile(mut v: Vec<f32>, p: f64) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// ความเหมือนของสองรูป 0–1 (1 = เหมือนกันทุกอย่าง) — โครงภาพ (ภาพย่อความสว่าง) 70% สีโดยรวม 30% · แนวภาพต่างกัน = 0
pub fn similarity(a: &Quality, b: &Quality) -> f32 {
    if a.portrait != b.portrait || a.thumb.len() != b.thumb.len() || a.thumb.is_empty() {
        return 0.0;
    }
    let norm = |t: &[u8]| {
        let n = t.len() as f32;
        let mean = t.iter().map(|&v| f32::from(v)).sum::<f32>() / n;
        let std = (t.iter().map(|&v| (f32::from(v) - mean).powi(2)).sum::<f32>() / n).sqrt().max(1.0);
        t.iter().map(|&v| (f32::from(v) - mean) / std).collect::<Vec<f32>>()
    };
    let (na, nb) = (norm(&a.thumb), norm(&b.thumb));
    let ncc = na.iter().zip(&nb).map(|(x, y)| x * y).sum::<f32>() / na.len() as f32;
    let hist = a.colors.iter().zip(&b.colors).map(|(&x, &y)| f32::from(x.min(y))).sum::<f32>() / 255.0;
    0.7 * ncc.max(0.0) + 0.3 * hist.clamp(0.0, 1.0)
}

/// ค่าของแต่ละระดับการคัด
#[derive(Debug, Clone, Copy)]
struct Level {
    /// ห่างกันไม่เกินกี่วินาทีจึงนับว่าถ่ายต่อเนื่อง
    gap: i64,
    /// เหมือนกันอย่างน้อยเท่านี้จึงนับว่าซ้ำ
    similar: f32,
    /// ความคมน้อยกว่าค่ากลางของกิจกรรมเท่านี้ (สัดส่วน) = เบลอ
    blur: f32,
}

/// ระดับเข้มขึ้นรวมรูปเป็นช่วงเดียวกันได้กว้างขึ้นเป็นหลัก — เกณฑ์เบลอขยับน้อย (รูปที่แค่รายละเอียดน้อยกว่ารูปอื่นไม่ใช่รูปเบลอ:
/// ลองเกณฑ์ 70% ของค่ากลางแล้วรูปพิธีที่คมดีถูกเรียกว่าเบลอ)
const LEVEL_VALUES: [Level; LEVELS as usize] = [
    Level { gap: 2, similar: 0.95, blur: 0.25 },
    Level { gap: 4, similar: 0.90, blur: 0.30 },
    Level { gap: 8, similar: 0.85, blur: 0.35 },
    Level { gap: 20, similar: 0.78, blur: 0.40 },
    Level { gap: 60, similar: 0.70, blur: 0.45 },
];

/// รูปหนึ่งรูปที่จะคัด
#[derive(Debug, Clone)]
pub struct Shot {
    pub photo_id: String,
    /// เวลาถ่าย (วินาที) — None = ไม่รู้ ใช้ความเหมือนอย่างเดียวกับรูปที่อยู่ติดกัน
    pub time: Option<i64>,
    /// None = ยังวัดไม่ได้ (ไม่แตะ)
    pub quality: Option<Quality>,
    /// การคัดที่ผู้ใช้ทำเอง — AI ไม่แตะรูปนี้ แต่รูปที่ผู้ใช้เลือกไว้เป็นตัวแทนของช่วงนั้น
    pub user: Option<Flag>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reason {
    /// รูปที่ดีที่สุดของช่วงนั้น
    Best,
    /// เบลอเมื่อเทียบกับรูปอื่นในกิจกรรม
    Blur,
    /// ซ้ำกับรูปที่เลือกไว้ของช่วงเดียวกัน
    #[serde(rename_all = "camelCase")]
    Duplicate { of: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub photo_id: String,
    pub flag: Flag,
    pub reason: Reason,
}

/// ความคมของตัวแบบ: ใบหน้าถ้ามี ไม่มีใช้ส่วนที่คมที่สุดของภาพ — (มาจากใบหน้า, ค่า)
fn subject(q: &Quality) -> (bool, f32) {
    match q.face_sharpness {
        Some(f) => (true, f),
        None => (false, q.sharpness),
    }
}

/// ความกว้างของขอบบนใบหน้า (ตัวยืนยันว่าเบลอจริง) — ไม่มีใบหน้า = None
fn edges(q: &Quality) -> Option<(bool, f32)> {
    q.face_detail.map(|d| (true, d))
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

/// ผลการคัดที่ระดับ `level` (1–5) สำหรับรูปที่ผู้ใช้ยังไม่ได้คัดเอง
pub fn plan(shots: &[Shot], level: u8) -> Vec<Decision> {
    let lv = LEVEL_VALUES[usize::from(level.clamp(1, LEVELS) - 1)];
    let mut order: Vec<usize> = (0..shots.len()).collect();
    order.sort_by_key(|&i| (shots[i].time.unwrap_or(i64::MAX), i));

    // ความคมเทียบค่ากลางของรูปที่ถ่ายใกล้กัน (ฉาก แสง กล้อง และขนาดหน้าใกล้กัน — เทียบทั้งกิจกรรมแล้วรูปต่างฉาก/ต่างกล้อง
    // ที่คมดีถูกนับว่าเบลอ) แยกรูปที่วัดจากใบหน้ากับรูปที่ไม่มีใบหน้า (ค่าคนละแบบ) · รูปข้างเคียงไม่พอใช้ค่ากลางของทั้งกิจกรรม
    let mut position = vec![0usize; shots.len()];
    for (p, &i) in order.iter().enumerate() {
        position[i] = p;
    }
    let relative = |values: &[Option<(bool, f32)>]| -> Vec<f32> {
        let event_mid = |face: bool| median(values.iter().flatten().filter(|m| m.0 == face).map(|m| m.1).collect());
        let (mid_face, mid_plain) = (event_mid(true), event_mid(false));
        (0..shots.len())
            .map(|i| {
                let Some((face, s)) = values[i] else { return 1.0 };
                let p = position[i];
                let near: Vec<f32> = order[p.saturating_sub(NEIGHBOURS)..(p + NEIGHBOURS + 1).min(order.len())]
                    .iter()
                    .filter_map(|&j| values[j].filter(|m| m.0 == face).map(|m| m.1))
                    .collect();
                let reference = if near.len() >= 4 {
                    median(near)
                } else if face {
                    mid_face
                } else {
                    mid_plain
                };
                match reference {
                    Some(m) if m > 0.0 => s / m,
                    _ => 1.0,
                }
            })
            .collect()
    };
    let rel_all = relative(&shots.iter().map(|s| s.quality.as_ref().map(subject)).collect::<Vec<_>>());
    // ใบหน้า: ต้องให้ความกว้างของขอบยืนยันด้วย — หน้าใหญ่แสงนุ่มผิวเรียบได้ค่าแรกต่ำแต่ขอบตา/ปากยังคม ไม่ใช่รูปเบลอ
    let edge_values: Vec<Option<(bool, f32)>> = shots.iter().map(|s| s.quality.as_ref().and_then(edges)).collect();
    let edge_all = relative(&edge_values);
    let blurry =
        |i: usize, threshold: f32| rel_all[i] < threshold && (edge_values[i].is_none() || edge_all[i] < EDGE_BLUR);

    // ช่วงที่ถ่ายต่อเนื่อง: ห่างกันไม่เกิน gap วินาที และเหมือนทั้งรูปก่อนหน้าและรูปแรกของช่วง
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for &i in &order {
        let Some(q) = &shots[i].quality else { continue };
        let joins = groups.last().is_some_and(|g| {
            let (first, last) = (g[0], g[g.len() - 1]);
            let close = match (shots[last].time, shots[i].time) {
                (Some(a), Some(b)) => (b - a).abs() <= lv.gap,
                _ => true,
            };
            let sim = |j: usize| shots[j].quality.as_ref().map_or(0.0, |o| similarity(o, q));
            close && sim(last) >= lv.similar && sim(first) >= lv.similar - 0.05
        });
        match groups.last_mut() {
            Some(g) if joins => g.push(i),
            _ => groups.push(vec![i]),
        }
    }

    let mut out = Vec::new();
    for g in groups {
        let qs: Vec<&Quality> = g.iter().map(|&i| shots[i].quality.as_ref().expect("วัดแล้ว")).collect();
        let sharp_rank: Vec<f32> = g.iter().map(|&i| rel_all[i]).collect();
        // คะแนนเทียบกันภายในช่วง: คมกว่า · เห็นหน้าครบกว่า · สว่างจ้าน้อยกว่า · ใบหลังของช่วงได้เปรียบเล็กน้อย
        // (ช่างภาพมักถ่ายซ้ำเมื่อใบแรกยังไม่ได้ — ใบหลังตัวแบบนิ่งแล้ว)
        let top_rank = sharp_rank.iter().copied().fold(f32::MIN, f32::max).max(1e-6);
        let top_faces = qs.iter().map(|q| q.faces).max().unwrap_or(0).max(1) as f32;
        let least_clipped = qs.iter().map(|q| q.clipped).fold(f32::MAX, f32::min);
        let last = (g.len() - 1).max(1) as f32;
        let score = |k: usize| {
            let q = qs[k];
            0.6 * sharp_rank[k] / top_rank + 0.25 * q.faces as f32 / top_faces - 2.0 * (q.clipped - least_clipped)
                + 0.04 * k as f32 / last
        };
        let user_pick = g.iter().position(|&i| shots[i].user == Some(Flag::Pick));
        let candidates: Vec<usize> = (0..g.len()).filter(|&k| shots[g[k]].user != Some(Flag::Reject)).collect();
        let sharp: Vec<usize> = candidates.iter().copied().filter(|&k| !blurry(g[k], lv.blur)).collect();
        let keeper = match user_pick {
            Some(k) => Some(k),
            None => {
                let best = |pool: &[usize]| pool.iter().copied().max_by(|&a, &b| score(a).total_cmp(&score(b)));
                // ทั้งช่วงเบลอ: ระดับเบา–กลางยังเก็บรูปที่ดีที่สุดไว้ (ไม่ให้ช่วงนั้นหายไปทั้งช่วง) ถ้าไม่เบลอจนเกินไป
                best(&sharp).or_else(|| {
                    let b = best(&candidates)?;
                    (level <= KEEP_MOMENT_UNTIL && !blurry(g[b], lv.blur * 0.5)).then_some(b)
                })
            }
        };
        for (k, &i) in g.iter().enumerate() {
            if shots[i].user.is_some() {
                continue;
            }
            // ช่วงนั้นมีรูปที่เก็บไว้แล้ว = ซ้ำ (บอกว่ามีรูปที่ดีกว่าของช่วงเดียวกัน) · ไม่มี = เบลอ
            let (flag, reason) = match keeper {
                Some(b) if b == k => (Flag::Pick, Reason::Best),
                Some(b) => (Flag::Reject, Reason::Duplicate { of: shots[g[b]].photo_id.clone() }),
                None => (Flag::Reject, Reason::Blur),
            };
            out.push(Decision { photo_id: shots[i].photo_id.clone(), flag, reason });
        }
    }
    out
}

// ───────────────────────── ในคลัง: งานวัด · ดูผล · ใช้ผล · ล้างผลของ AI ─────────────────────────

/// สถานะการคัดของกิจกรรม
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CullStatus {
    /// รูปของกิจกรรม (ไฟล์อยู่ครบ วัดได้)
    pub total: i64,
    pub measured: i64,
    /// ผู้ใช้คัดเองแล้ว (AI ไม่แตะ) / AI คัดไว้
    pub user_flagged: i64,
    pub ai_flagged: i64,
    /// งานวัดที่ยังทำอยู่
    pub job: Option<JobSnapshot>,
}

const EVENT_PHOTOS: &str = "FROM event_photos ep
  JOIN photos p ON p.id = ep.photo_id AND p.catalog_state = 'active'
  JOIN photo_revisions r ON r.id = p.current_revision_id
  JOIN photo_locations l ON l.photo_id = p.id
  LEFT JOIN photo_quality q ON q.revision_id = r.id AND q.version = ?2
  LEFT JOIN photo_picks pk ON pk.photo_id = p.id
  WHERE ep.event_id = ?1";

fn active_job(conn: &Connection, event_id: &str) -> Result<Option<JobSnapshot>> {
    let id: Option<String> = conn
        .query_row(
            &format!("SELECT id FROM jobs WHERE dedup_key = ?1 AND state IN {ACTIVE_STATES}"),
            [format!("cull:{event_id}")],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| jobs::get(conn, &id)).transpose()
}

pub fn status(app: &App, event_id: &str) -> Result<CullStatus> {
    let conn = app.db();
    let (total, measured, user_flagged, ai_flagged): (i64, i64, i64, i64) = conn.query_row(
        &format!(
            "SELECT COUNT(*), COUNT(q.revision_id), COUNT(CASE WHEN pk.source = 'user' THEN 1 END),
                    COUNT(CASE WHEN pk.source = 'ai' THEN 1 END)
             {EVENT_PHOTOS} AND l.available = 1"
        ),
        params![event_id, VERSION],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(CullStatus { total, measured, user_flagged, ai_flagged, job: active_job(&conn, event_id)? })
}

/// เริ่มงานวัดรูปที่ยังไม่ได้วัดของกิจกรรม (หรือคืนงานเดิมที่ยังทำอยู่) — None = วัดครบแล้ว
pub fn analyze(app: &App, event_id: &str) -> Result<Option<JobSnapshot>> {
    let id = {
        let conn = app.db();
        let title: String = conn
            .query_row("SELECT title FROM events WHERE id = ?1 AND catalog_state = 'active'", [event_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::not_found("กิจกรรม"))?;
        if let Some(job) = active_job(&conn, event_id)? {
            // งานที่พัก/ติดอยู่ให้เดินต่อเมื่อผู้ใช้สั่งอีกครั้ง
            conn.execute(
                "UPDATE jobs SET state = 'queued' WHERE id = ?1 AND state IN ('paused', 'blocked')",
                [&job.id],
            )?;
            job.id
        } else {
            let missing: i64 = conn.query_row(
                &format!("SELECT COUNT(*) {EVENT_PHOTOS} AND l.available = 1 AND q.revision_id IS NULL"),
                params![event_id, VERSION],
                |r| r.get(0),
            )?;
            if missing == 0 {
                return Ok(None);
            }
            let (id, now) = (util::new_id(), util::now());
            conn.execute(
                "INSERT INTO jobs(id, kind, state, stage, title, event_id, dedup_key, total, total_known, created_at, updated_at)
                 VALUES (?1, 'cull', 'queued', 'measure', ?2, ?3, ?4, ?5, 1, ?6, ?6)",
                params![id, format!("ดูความคมและหาภาพซ้ำ: {title}"), event_id, format!("cull:{event_id}"), missing, now],
            )?;
            db::bump_revision(&conn)?;
            id
        }
    };
    app.worker.wake();
    app.notify(&["jobs"]);
    Ok(Some(jobs::get(&app.db(), &id)?))
}

struct Item {
    photo_id: String,
    revision_id: String,
    path: PathBuf,
    filename: String,
    size: i64,
    mtime_ns: i64,
}

/// งานวัด: ถอดรหัสรูปขนาดย่อ (RAW ล้างแบบเร็ว) + กรอบใบหน้าที่มีอยู่ → ผลวัด · ทำต่อจากเดิมได้ (วัดเฉพาะรูปที่ยังไม่มีผล)
pub(crate) fn run(ctx: &JobCtx) -> Result<Outcome> {
    let conn = ctx.conn;
    let event_id = ctx.job.event_id.clone().ok_or_else(|| AppError::internal("งานคัดภาพไม่ได้ระบุกิจกรรม"))?;
    ctx.stage("measure")?;
    let items: Vec<Item> = conn
        .prepare(&format!(
            "SELECT p.id, r.id, l.os_path, l.filename, r.size, r.mtime_ns {EVENT_PHOTOS}
               AND l.available = 1 AND q.revision_id IS NULL ORDER BY l.filename_key"
        ))?
        .query_map(params![event_id, VERSION], |r| {
            Ok(Item {
                photo_id: r.get(0)?,
                revision_id: r.get(1)?,
                path: PathBuf::from(r.get::<_, String>(2)?),
                filename: r.get(3)?,
                size: r.get(4)?,
                mtime_ns: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let (mut done, mut failed) = (ctx.job.done, ctx.job.failed);
    let total = done + failed + items.len() as i64;
    ctx.progress(total, done, failed, 0, true)?;
    let threads = std::thread::available_parallelism().map(|n| n.get() / 2).unwrap_or(2).clamp(1, 6);
    for chunk in items.chunks(threads) {
        let c = ctx.control()?;
        if c != Control::Continue {
            ctx.progress(total, done, failed, 0, true)?;
            return Ok(JobCtx::outcome_for(c));
        }
        let faces: Vec<Vec<[f64; 4]>> =
            chunk.iter().map(|it| face_boxes(conn, &it.revision_id)).collect::<Result<_>>()?;
        let results: Vec<Result<Quality>> = std::thread::scope(|s| {
            let handles: Vec<_> =
                chunk.iter().zip(&faces).map(|(it, f)| s.spawn(move || measure_file(&it.path, f))).collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err(AppError::internal("วัดรูปนี้ไม่สำเร็จ (ตัวประมวลผลภาพหยุดทำงาน)"))))
                .collect()
        });
        for (it, result) in chunk.iter().zip(results) {
            match result {
                Ok(q) => {
                    // เขียนเฉพาะเมื่อ revision ยังเป็นเนื้อหาเดียวกับที่อ่าน
                    let current: bool = conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM photo_revisions WHERE id = ?1 AND size = ?2 AND mtime_ns = ?3)",
                        params![it.revision_id, it.size, it.mtime_ns],
                        |r| r.get(0),
                    )?;
                    if current {
                        save(conn, &it.revision_id, &q)?;
                    }
                    done += 1;
                }
                Err(e) => {
                    ctx.error(Some(&it.photo_id), &it.filename, &e)?;
                    failed += 1;
                }
            }
        }
        ctx.progress(total, done, failed, 0, true)?;
    }
    Ok(Outcome::Done)
}

fn face_boxes(conn: &Connection, revision_id: &str) -> Result<Vec<[f64; 4]>> {
    Ok(conn
        .prepare("SELECT bbox_x, bbox_y, bbox_w, bbox_h FROM faces WHERE photo_revision_id = ?1 AND status = 'active'")?
        .query_map([revision_id], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))?
        .collect::<rusqlite::Result<_>>()?)
}

fn measure_file(path: &std::path::Path, faces: &[[f64; 4]]) -> Result<Quality> {
    let bytes = crate::media::read_file(path)?;
    let img = crate::media::decode_upright(&bytes, MEASURE_EDGE)?.to_rgb8();
    Ok(measure(&img, faces))
}

fn save(conn: &Connection, revision_id: &str, q: &Quality) -> Result<()> {
    conn.execute(
        "INSERT INTO photo_quality(revision_id, version, sharpness, face_sharpness, face_detail, faces, brightness, clipped,
                                   portrait, thumb, colors, measured_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(revision_id) DO UPDATE SET version = excluded.version, sharpness = excluded.sharpness,
           face_sharpness = excluded.face_sharpness, face_detail = excluded.face_detail, faces = excluded.faces,
           brightness = excluded.brightness,
           clipped = excluded.clipped, portrait = excluded.portrait, thumb = excluded.thumb, colors = excluded.colors,
           measured_at = excluded.measured_at",
        params![
            revision_id,
            VERSION,
            q.sharpness,
            q.face_sharpness,
            q.face_detail,
            q.faces,
            q.brightness,
            q.clipped,
            q.portrait,
            q.thumb,
            q.colors,
            util::now()
        ],
    )?;
    Ok(())
}

/// รูปของกิจกรรมพร้อมผลวัดและการคัดของผู้ใช้ เรียงตามเวลาถ่ายแล้วชื่อไฟล์
fn load_shots(conn: &Connection, event_id: &str) -> Result<Vec<Shot>> {
    let rows = conn
        .prepare(&format!(
            "SELECT p.id, r.taken_at_local, q.sharpness, q.face_sharpness, q.faces, q.brightness, q.clipped, q.portrait,
                    q.thumb, q.colors, CASE WHEN pk.source = 'user' THEN pk.flag END, q.face_detail
             {EVENT_PHOTOS} AND l.available = 1 ORDER BY r.taken_at_local IS NULL, r.taken_at_local, l.filename_key"
        ))?
        .query_map(params![event_id, VERSION], |r| {
            let quality = match r.get::<_, Option<f32>>(2)? {
                Some(sharpness) => Some(Quality {
                    sharpness,
                    face_sharpness: r.get(3)?,
                    face_detail: r.get(11)?,
                    faces: r.get(4)?,
                    brightness: r.get(5)?,
                    clipped: r.get(6)?,
                    portrait: r.get(7)?,
                    thumb: r.get(8)?,
                    colors: r.get(9)?,
                }),
                None => None,
            };
            let user = match r.get::<_, Option<String>>(10)?.as_deref() {
                Some("pick") => Some(Flag::Pick),
                Some("reject") => Some(Flag::Reject),
                _ => None,
            };
            Ok(Shot {
                photo_id: r.get(0)?,
                time: r.get::<_, Option<String>>(1)?.as_deref().and_then(seconds),
                quality,
                user,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// `YYYY-MM-DDTHH:MM:SS` → วินาที (ไม่มี timezone — ใช้เทียบระยะห่างระหว่างรูปของกิจกรรมเดียวกันเท่านั้น)
pub fn seconds(s: &str) -> Option<i64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // วันจากปฏิทินเกรกอเรียน (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe) * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// จำนวนของแต่ละระดับ (ให้แถบเลื่อนบอกได้ทันทีว่าได้กี่รูป)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelCount {
    pub level: u8,
    pub keep: usize,
    pub duplicates: usize,
    pub blurry: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CullPreview {
    pub level: u8,
    pub levels: Vec<LevelCount>,
    /// ผลของระดับที่ขอ (เฉพาะรูปที่ผู้ใช้ยังไม่ได้คัดเอง)
    pub decisions: Vec<Decision>,
    /// รูปที่ยังไม่ได้วัด (ไม่ถูกคัด) / ผู้ใช้คัดเองแล้ว (AI ไม่แตะ)
    pub unmeasured: usize,
    pub user_flagged: usize,
}

fn count(level: u8, plan: &[Decision]) -> LevelCount {
    LevelCount {
        level,
        keep: plan.iter().filter(|d| d.flag == Flag::Pick).count(),
        duplicates: plan.iter().filter(|d| matches!(d.reason, Reason::Duplicate { .. })).count(),
        blurry: plan.iter().filter(|d| d.reason == Reason::Blur).count(),
    }
}

pub fn preview(app: &App, event_id: &str, level: u8) -> Result<CullPreview> {
    let level = level.clamp(1, LEVELS);
    let shots = load_shots(&app.db(), event_id)?;
    let levels = (1..=LEVELS).map(|l| count(l, &plan(&shots, l))).collect();
    Ok(CullPreview {
        level,
        levels,
        decisions: plan(&shots, level),
        unmeasured: shots.iter().filter(|s| s.quality.is_none()).count(),
        user_flagged: shots.iter().filter(|s| s.user.is_some()).count(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CullApplied {
    pub picked: usize,
    pub rejected: usize,
    /// รูปที่การคัดเปลี่ยนจากเดิม
    pub updated: usize,
}

/// ใช้ผลการคัดที่ระดับ `level`: ตั้ง/แก้การคัดของ AI · ไม่แตะรูปที่ผู้ใช้คัดเอง (ตรวจซ้ำใน SQL ด้วย)
pub fn apply(app: &App, event_id: &str, level: u8) -> Result<CullApplied> {
    let level = level.clamp(1, LEVELS);
    let mut out = CullApplied { picked: 0, rejected: 0, updated: 0 };
    {
        let conn = app.db();
        let decisions = plan(&load_shots(&conn, event_id)?, level);
        let tx = conn.unchecked_transaction()?;
        let now = util::now();
        for d in &decisions {
            let (reason, of) = match &d.reason {
                Reason::Best => ("best", None),
                Reason::Blur => ("blur", None),
                Reason::Duplicate { of } => ("duplicate", Some(of.as_str())),
            };
            out.updated += tx.execute(
                "INSERT INTO photo_picks(photo_id, flag, updated_at, source, reason, reason_photo_id) VALUES (?1, ?2, ?3, 'ai', ?4, ?5)
                 ON CONFLICT(photo_id) DO UPDATE SET flag = excluded.flag, updated_at = excluded.updated_at,
                   reason = excluded.reason, reason_photo_id = excluded.reason_photo_id
                 WHERE photo_picks.source = 'ai' AND (photo_picks.flag <> excluded.flag OR photo_picks.reason IS NOT excluded.reason
                   OR photo_picks.reason_photo_id IS NOT excluded.reason_photo_id)",
                params![d.photo_id, if d.flag == Flag::Pick { "pick" } else { "reject" }, now, reason, of],
            )?;
            match d.flag {
                Flag::Pick => out.picked += 1,
                Flag::Reject => out.rejected += 1,
            }
        }
        if out.updated > 0 {
            db::bump_revision(&tx)?;
        }
        tx.commit()?;
    }
    if out.updated > 0 {
        app.notify(&["photos"]);
    }
    Ok(out)
}

/// ล้างการคัดที่ AI ทำในกิจกรรมนี้ (การคัดของผู้ใช้อยู่ครบ) — คืนจำนวนรูปที่ล้าง
pub fn clear(app: &App, event_id: &str) -> Result<usize> {
    let n = {
        let conn = app.db();
        let n = conn.execute(
            "DELETE FROM photo_picks WHERE source = 'ai' AND photo_id IN (SELECT photo_id FROM event_photos WHERE event_id = ?1)",
            [event_id],
        )?;
        if n > 0 {
            db::bump_revision(&conn)?;
        }
        n
    };
    if n > 0 {
        app.notify(&["photos"]);
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// ภาพทดสอบ: ลายตารางหมากรุก + วงกลม (ขอบคม) · `blur` = รัศมีเบลอ (พิกเซล)
    fn scene(shift: u32, blur: f32) -> RgbImage {
        let img = RgbImage::from_fn(600, 400, |x, y| {
            let check = ((x + shift) / 40 + y / 40).is_multiple_of(2);
            let (dx, dy) = (x as f32 - 300.0 - shift as f32, y as f32 - 200.0);
            let disc = dx * dx + dy * dy < 90.0 * 90.0;
            let v = if disc {
                220
            } else if check {
                160
            } else {
                60
            };
            Rgb([v, (v as f32 * 0.8) as u8, (v as f32 * 0.6) as u8])
        });
        if blur > 0.0 {
            imageops::blur(&img, blur)
        } else {
            img
        }
    }

    #[test]
    fn blur_lowers_the_sharpness_and_faces_are_measured_on_their_own() {
        let sharp = measure(&scene(0, 0.0), &[]);
        let soft = measure(&scene(0, 3.0), &[]);
        assert!(sharp.sharpness > soft.sharpness * 3.0, "{} vs {}", sharp.sharpness, soft.sharpness);
        assert_eq!(sharp.face_sharpness, None);
        let face = measure(&scene(0, 0.0), &[[0.35, 0.25, 0.3, 0.5]]);
        assert_eq!(face.faces, 1);
        assert!(face.face_sharpness.is_some_and(|f| f > 0.0));
        // ใบหน้าเล็กเกินวัดไม่ได้
        assert_eq!(measure(&scene(0, 0.0), &[[0.5, 0.5, 0.01, 0.02]]).faces, 0);
    }

    #[test]
    fn similar_frames_are_close_and_other_scenes_are_not() {
        let a = measure(&scene(0, 0.0), &[]);
        let b = measure(&scene(4, 0.0), &[]);
        let other = measure(&RgbImage::from_fn(600, 400, |x, _| Rgb([(x % 255) as u8, 30, 200])), &[]);
        assert!(similarity(&a, &b) > 0.9, "{}", similarity(&a, &b));
        assert!(similarity(&a, &other) < 0.6, "{}", similarity(&a, &other));
        let tall = measure(&imageops::rotate90(&scene(0, 0.0)), &[]);
        assert_eq!(similarity(&a, &tall), 0.0, "แนวภาพต่างกันไม่ซ้ำกัน");
    }

    fn shot(id: &str, time: i64, img: &RgbImage, user: Option<Flag>) -> Shot {
        Shot { photo_id: id.into(), time: Some(time), quality: Some(measure(img, &[])), user }
    }

    #[test]
    fn a_burst_keeps_its_sharpest_frame_and_blurry_photos_are_dropped() {
        let shots = vec![
            shot("a1", 0, &scene(0, 1.5), None),
            shot("a2", 1, &scene(2, 0.0), None),
            shot("a3", 2, &scene(4, 0.8), None),
            // ฉากอื่น เวลาห่างไป
            shot("b", 300, &RgbImage::from_fn(600, 400, |x, y| Rgb([((x * y) % 251) as u8, 90, 40])), None),
            // ฉากที่สาม เบลอมาก
            shot(
                "c",
                900,
                &imageops::blur(
                    &RgbImage::from_fn(600, 400, |x, y| Rgb([((x / 30 + y / 30) % 2 * 200) as u8, 40, 90])),
                    6.0,
                ),
                None,
            ),
            shot(
                "c2",
                960,
                &RgbImage::from_fn(600, 400, |x, y| Rgb([((x / 30 + y / 30) % 2 * 200) as u8, 40, 90])),
                None,
            ),
        ];
        let plan = plan(&shots, 3);
        let get = |id: &str| plan.iter().find(|d| d.photo_id == id).cloned().unwrap();
        assert_eq!((get("a2").flag, get("a2").reason), (Flag::Pick, Reason::Best));
        assert_eq!(get("a1").reason, Reason::Duplicate { of: "a2".into() });
        assert_eq!(get("a3").flag, Flag::Reject);
        assert_eq!(get("b").flag, Flag::Pick, "ฉากเดียวในช่วงนั้นเก็บไว้");
        assert_eq!(get("c2").flag, Flag::Pick);
        assert_eq!(get("c").flag, Flag::Reject);
    }

    #[test]
    fn user_flags_are_never_touched_and_a_user_pick_leads_its_burst() {
        let shots = vec![
            shot("a1", 0, &scene(0, 1.5), Some(Flag::Pick)),
            shot("a2", 1, &scene(2, 0.0), None),
            shot("a3", 2, &scene(4, 0.0), Some(Flag::Reject)),
        ];
        let plan = plan(&shots, 3);
        assert!(plan.iter().all(|d| d.photo_id == "a2"), "{plan:?}");
        assert_eq!(plan[0].reason, Reason::Duplicate { of: "a1".into() });
    }

    #[test]
    fn stricter_levels_keep_fewer_photos() {
        let mut shots = Vec::new();
        for i in 0..12u32 {
            let blur = if i % 4 == 3 { 1.6 } else { 0.0 };
            shots.push(shot(&format!("p{i}"), i64::from(i) * 6, &scene(i * 3, blur), None));
        }
        let kept: Vec<usize> =
            (1..=LEVELS).map(|l| plan(&shots, l).iter().filter(|d| d.flag == Flag::Pick).count()).collect();
        assert!(kept.windows(2).all(|w| w[0] >= w[1]), "{kept:?}");
        assert!(kept[0] > kept[4], "{kept:?}");
    }
}
