//! AI ช่วยจัดองค์ประกอบ (DECISIONS ข้อ 38): ปรับเอียงจากเส้นตรงในภาพ และเสนอกรอบตัดตามหลักองค์ประกอบ (กฎสามส่วน ฯลฯ)
//!
//! คำนวณในเครื่องจากภาพที่ย่อแล้ว ไม่ใช้โมเดลภายนอก — ใช้ใบหน้าที่ตัววิเคราะห์หาไว้ (ถ้ามี) เป็นตัวแบบหลัก
//! ทิศทาง: องศาบวก = หมุนตามเข็มนาฬิกา (พิกัดภาพ y ชี้ลง) แบบเดียวกับ `EditSettings::straighten`

use image::{imageops, RgbImage};
use serde::Serialize;

use crate::edit::Rect;

/// ด้านยาวของภาพที่ใช้หาเส้นตรง
const TILT_EDGE: u32 = 1000;
/// หาเส้นที่เอียงไม่เกินเท่านี้จากแนวนอน/แนวตั้ง
const TILT_RANGE: f64 = 15.0;
const TILT_BIN: f64 = 0.1;
/// ความมั่นใจต่ำกว่านี้ = ไม่เสนอปรับเอียง
const TILT_MIN_CONFIDENCE: f64 = 0.12;
/// เส้นตรงที่ยาวต่อเนื่องอย่างน้อยเท่านี้ของความกว้างภาพ (เช่นขอบฟ้า) ใช้ตัดสินความเอียงได้เมื่อมีหลักฐานอื่นยืนยัน
const LONG_LINE: f64 = 0.3;
/// หลักฐานสองแบบต่างกันไม่เกินเท่านี้ (องศา) = ตรงกัน
const AGREE: f64 = 0.8;
/// เสนอปรับเอียงไม่เกินเท่านี้ — มากกว่านี้มักเป็นเส้นที่เอียงตามมุมมอง ไม่ใช่กล้องเอียง
const MAX_SUGGEST: f64 = 8.0;
/// ด้านยาวของแผนที่ความเด่นที่ใช้เลือกกรอบ
const SALIENCY_EDGE: u32 = 192;

/// ความเอียงของภาพ: เส้นแนวนอน/แนวตั้งในภาพเอียงไปตามเข็มนาฬิกากี่องศา (แก้ด้วย `straighten = -degrees`)
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tilt {
    pub degrees: f64,
    /// สัดส่วนของเส้นที่เอียงเท่ากันนี้ (0–1) — ยิ่งสูงยิ่งมีเส้นตรงยาว ๆ ยืนยันชัด
    pub confidence: f64,
}

fn luma(img: &RgbImage) -> (Vec<f32>, usize, usize) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let v = img.pixels().map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])).collect();
    (v, w, h)
}

fn downscale(img: &RgbImage, edge: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let scale = f64::from(edge) / f64::from(w.max(h).max(1));
    if scale >= 1.0 {
        return img.clone();
    }
    let (nw, nh) = (((f64::from(w) * scale).round() as u32).max(1), ((f64::from(h) * scale).round() as u32).max(1));
    imageops::resize(img, nw, nh, imageops::FilterType::Triangle)
}

/// กล่องเฉลี่ยรัศมี `r` แบบแยกแกน (ขอบภาพใช้เท่าที่มี)
fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0f32; src.len()];
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        let at = |o: usize, i: usize| if horizontal { o * w + i } else { i * w + o };
        for o in 0..outer {
            let mut sum = 0f32;
            let mut count = 0f32;
            for i in 0..=r.min(inner - 1) {
                sum += src[at(o, i)];
                count += 1.0;
            }
            for i in 0..inner {
                out[at(o, i)] = sum / count;
                if i + r + 1 < inner {
                    sum += src[at(o, i + r + 1)];
                    count += 1.0;
                }
                if i >= r {
                    sum -= src[at(o, i - r)];
                    count -= 1.0;
                }
            }
        }
        out
    };
    pass(&pass(src, true), false)
}

/// หลักฐานความเอียงแต่ละแบบ (สำหรับตัดสินและวัดผล)
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TiltCues {
    /// ยอดของฮิสโตแกรมทิศของขอบ (เส้นแนวนอน+แนวตั้ง) — (องศา, สัดส่วนของขอบที่เอียงเท่านี้)
    pub edges: Option<(f64, f64)>,
    /// เฉพาะเส้นแนวตั้งใกล้กลางภาพ (เสา ผนัง — ไม่เอียงตามมุมมองซ้าย/ขวา) — (องศา, สัดส่วน)
    pub vertical: Option<(f64, f64)>,
    /// เส้นเกือบแนวนอนที่ยาวต่อเนื่องที่สุด (ขอบฟ้า) — (องศา, ความยาวเทียบความกว้างภาพ)
    pub line: Option<(f64, f64)>,
}

/// หาความเอียงจากเส้นตรงที่เกือบเป็นแนวนอนหรือแนวตั้ง (ขอบอาคาร เสา ป้าย ขอบฟ้า) — None = หลักฐานไม่ชัดหรือขัดกัน
/// (เสนอผิดแย่กว่าไม่เสนอ: เส้นในภาพจำนวนมากเอียงตามมุมมอง ไม่ใช่เพราะกล้องเอียง)
pub fn detect_tilt(img: &RgbImage) -> Option<Tilt> {
    let cues = tilt_cues(img)?;
    decide(&cues)
}

/// ตัดสินจากหลักฐาน — เสนอเมื่อหลักฐานตรงกันจริงเท่านั้น (ต่างกันไม่เกิน [`AGREE`]):
/// เส้นยาว (ขอบฟ้า) + หลักฐานอื่นอย่างน้อยหนึ่งแบบ · หรือทิศของขอบโดยรวมกับเส้นแนวตั้งที่มั่นใจทั้งคู่
/// (วัดกับรูปของหลวงพี่: กติกาที่หลวมกว่านี้เสนอผิดทิศหรือเกินไป 2–3° ในบางรูป)
fn decide(c: &TiltCues) -> Option<Tilt> {
    let small = |a: f64| a.abs() <= MAX_SUGGEST;
    let line = c.line.filter(|&(l, reach)| small(l) && reach >= LONG_LINE);
    let edges = c.edges.filter(|&(e, conf)| small(e) && conf >= 0.08);
    let vertical = c.vertical.filter(|&(v, conf)| small(v) && conf >= 0.1);
    if let Some((l, reach)) = line {
        // (มุม, น้ำหนัก) ของหลักฐานที่ตรงกับเส้นยาว
        let mut votes = vec![(l, reach)];
        votes.extend(edges.filter(|e| (e.0 - l).abs() <= AGREE).map(|(e, conf)| (e, conf * 3.0)));
        votes.extend(vertical.filter(|v| (v.0 - l).abs() <= AGREE).map(|(v, conf)| (v, conf * 3.0)));
        if votes.len() >= 2 {
            let weight: f64 = votes.iter().map(|v| v.1).sum();
            let d = votes.iter().map(|v| v.0 * v.1).sum::<f64>() / weight;
            return Some(Tilt { degrees: (d * 10.0).round() / 10.0, confidence: reach.min(1.0) });
        }
    }
    if let (Some((e, ec)), Some((v, vc))) = (edges, vertical) {
        if ec >= TILT_MIN_CONFIDENCE && vc >= 0.2 && (e - v).abs() <= 0.5 {
            let d = (e + v) / 2.0;
            return Some(Tilt { degrees: (d * 10.0).round() / 10.0, confidence: ec.min(vc) });
        }
        // ทั้งขอบโดยรวมและเส้นแนวตั้งบอกว่าตรงอยู่แล้ว = ภาพตรง (บอกผู้ใช้ได้ว่าไม่ต้องปรับ)
        if e.abs() <= 0.3 && v.abs() <= 0.3 {
            return Some(Tilt { degrees: 0.0, confidence: ec.min(vc) });
        }
    }
    None
}

/// หลักฐานความเอียงของภาพ — None = ภาพเล็กหรือเรียบเกินไป
#[doc(hidden)]
pub fn tilt_cues(img: &RgbImage) -> Option<TiltCues> {
    let small = downscale(img, TILT_EDGE);
    let (l, w, h) = luma(&small);
    if w < 16 || h < 16 {
        return None;
    }
    // Sobel
    let mut gx = vec![0f32; w * h];
    let mut gy = vec![0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let p = |dx: isize, dy: isize| l[(y as isize + dy) as usize * w + (x as isize + dx) as usize];
            gx[y * w + x] = (p(1, -1) + 2.0 * p(1, 0) + p(1, 1)) - (p(-1, -1) + 2.0 * p(-1, 0) + p(-1, 1));
            gy[y * w + x] = (p(-1, 1) + 2.0 * p(0, 1) + p(1, 1)) - (p(-1, -1) + 2.0 * p(0, -1) + p(1, -1));
        }
    }
    let mag: Vec<f32> = gx.iter().zip(&gy).map(|(a, b)| (a * a + b * b).sqrt()).collect();
    let mut sorted: Vec<f32> = mag.iter().copied().filter(|m| *m > 0.0).collect();
    if sorted.len() < 100 {
        return None;
    }
    sorted.sort_by(f32::total_cmp);
    let threshold = sorted[(sorted.len() as f64 * 0.6) as usize].max(24.0);
    // structure tensor เฉลี่ยรอบ ๆ: ทิศของขอบที่สม่ำเสมอ (เส้นตรง) ได้ความสอดคล้องสูง
    let jxx = box_blur(&gx.iter().map(|v| v * v).collect::<Vec<_>>(), w, h, 2);
    let jyy = box_blur(&gy.iter().map(|v| v * v).collect::<Vec<_>>(), w, h, 2);
    let jxy = box_blur(&gx.iter().zip(&gy).map(|(a, b)| a * b).collect::<Vec<_>>(), w, h, 2);
    // จุดบนเส้น (x, y, ความเอียงโดยประมาณ, น้ำหนัก, เป็นเส้นแนวตั้ง)
    let mut points: Vec<(f64, f64, f64, f64, bool)> = Vec::new();
    for i in 0..w * h {
        if mag[i] < threshold {
            continue;
        }
        let (a, b, c) = (f64::from(jxx[i]), f64::from(jyy[i]), f64::from(jxy[i]));
        let tr = a + b;
        if tr <= 0.0 {
            continue;
        }
        let coherence = ((a - b).powi(2) + 4.0 * c * c).sqrt() / tr;
        if coherence < 0.6 {
            continue;
        }
        // ทิศของ gradient → ทิศของเส้น (ตั้งฉาก) ในช่วง (−90°, 90°]
        let gradient = 0.5 * (2.0 * c).atan2(a - b);
        let mut line = (gradient + std::f64::consts::FRAC_PI_2).to_degrees();
        while line > 90.0 {
            line -= 180.0;
        }
        while line <= -90.0 {
            line += 180.0;
        }
        let (dev, vertical) = if line.abs() <= TILT_RANGE {
            (line, false)
        } else if line.abs() >= 90.0 - TILT_RANGE {
            (if line > 0.0 { line - 90.0 } else { line + 90.0 }, true)
        } else {
            continue;
        };
        points.push(((i % w) as f64, (i / w) as f64, dev, f64::from(mag[i]) * coherence, vertical));
    }
    let edges = peak(points.iter().map(|p| (p.2, if p.4 { p.3 * 0.7 } else { p.3 })))
        .map(|(d, conf)| (refine(&points, d), conf));
    // เส้นแนวตั้งใกล้กลางภาพ (ซ้าย/ขวาสุดเอียงตามมุมมองเมื่อถ่ายเงย/ก้ม)
    let uprights: Vec<(f64, f64, f64, f64, bool)> = points
        .iter()
        .filter(|p| p.4)
        .map(|&(x, y, d, wgt, v)| (x, y, d, wgt * (1.0 - (x / w as f64 - 0.5).abs() * 2.0).max(0.0), v))
        .collect();
    let vertical = peak(uprights.iter().map(|p| (p.2, p.3))).map(|(d, conf)| (refine(&uprights, d), conf));
    Some(TiltCues { edges, vertical, line: longest_line(&points, w, h) })
}

/// ยอดของฮิสโตแกรมความเอียง (เกลี่ย σ 0.3°) และสัดส่วนน้ำหนักภายใน ±0.5° ของยอด — (องศา, สัดส่วน)
fn peak(values: impl Iterator<Item = (f64, f64)>) -> Option<(f64, f64)> {
    let bins = (2.0 * TILT_RANGE / TILT_BIN).round() as usize + 1;
    let mut hist = vec![0f64; bins];
    let mut total = 0f64;
    for (dev, w) in values {
        let k = ((dev + TILT_RANGE) / TILT_BIN).round();
        if (0.0..bins as f64).contains(&k) && w > 0.0 {
            hist[k as usize] += w;
            total += w;
        }
    }
    if total <= 0.0 {
        return None;
    }
    let sigma = 0.3 / TILT_BIN;
    let reach = (3.0 * sigma).ceil() as isize;
    let smooth: Vec<f64> = (0..bins as isize)
        .map(|k| {
            (-reach..=reach)
                .filter_map(|d| {
                    let j = k + d;
                    (0..bins as isize)
                        .contains(&j)
                        .then(|| hist[j as usize] * (-(d * d) as f64 / (2.0 * sigma * sigma)).exp())
                })
                .sum()
        })
        .collect();
    let top = (0..bins).max_by(|&a, &b| smooth[a].total_cmp(&smooth[b]))?;
    let near = (0.5 / TILT_BIN).round() as usize;
    let (lo, hi) = (top.saturating_sub(near), (top + near).min(bins - 1));
    let mass: f64 = hist[lo..=hi].iter().sum();
    if mass <= 0.0 {
        return None;
    }
    let mean =
        hist[lo..=hi].iter().enumerate().map(|(i, v)| v * ((lo + i) as f64 * TILT_BIN - TILT_RANGE)).sum::<f64>()
            / mass;
    Some((mean, mass / total))
}

/// ปรับมุมให้ละเอียด: หมุนลองทีละ 0.05° รอบมุมคร่าว ๆ (±1°) แล้วเลือกมุมที่จุดบนเส้นเรียงเป็นแถว/คอลัมน์ชัดที่สุด
/// (ความแปรปรวนของ projection profile สูงสุด แบบที่ใช้ตั้งเอกสารที่สแกนเอียงให้ตรง) — ทิศของขอบรายจุดเพี้ยนเข้าหาแนวตั้ง/นอนเล็กน้อย
/// เพราะรอยหยักของพิกเซล ส่วนตำแหน่งของจุดไม่เพี้ยน
fn refine(points: &[(f64, f64, f64, f64, bool)], coarse: f64) -> f64 {
    let near: Vec<&(f64, f64, f64, f64, bool)> = points.iter().filter(|p| (p.2 - coarse).abs() <= 1.5).collect();
    if near.len() < 50 {
        return coarse;
    }
    let profile = |a: f64| {
        let (s, c) = a.to_radians().sin_cos();
        let mut rows: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        let mut cols: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        for &&(x, y, _, w, vertical) in &near {
            if vertical {
                *cols.entry((x * c + y * s).floor() as i64).or_default() += w;
            } else {
                *rows.entry((y * c - x * s).floor() as i64).or_default() += w;
            }
        }
        rows.values().chain(cols.values()).map(|v| v * v).sum::<f64>()
    };
    let mut best = (coarse, f64::MIN);
    for k in -20..=20 {
        let a = coarse + f64::from(k) * 0.05;
        let score = profile(a);
        if score > best.1 {
            best = (a, score);
        }
    }
    best.0
}

/// เส้นตรงเกือบแนวนอนที่ยาวที่สุด (Hough เฉพาะมุม ±1.5° รอบทิศของขอบแต่ละจุด) — (มุม, ความยาวเทียบความกว้างภาพ)
/// นับเฉพาะส่วนที่ต่อเนื่อง: จุดบนเส้นเดียวกันต้องห่างกันไม่เกิน 3% ของความกว้างภาพ
fn longest_line(points: &[(f64, f64, f64, f64, bool)], w: usize, h: usize) -> Option<(f64, f64)> {
    let horizontal: Vec<&(f64, f64, f64, f64, bool)> = points.iter().filter(|p| !p.4).collect();
    if horizontal.len() < 50 {
        return None;
    }
    let steps = (2.0 * TILT_RANGE / TILT_BIN).round() as usize + 1;
    let offset = (w as f64 * TILT_RANGE.to_radians().sin()).ceil() as i64 + 2;
    let rows = h as i64 + 2 * offset;
    let mut acc = vec![0u32; steps * rows as usize];
    let trig: Vec<(f64, f64)> = (0..steps).map(|k| (k as f64 * TILT_BIN - TILT_RANGE).to_radians().sin_cos()).collect();
    for &&(x, y, dev, _, _) in &horizontal {
        let k0 = ((dev - 1.5 + TILT_RANGE) / TILT_BIN).floor().max(0.0) as usize;
        let k1 = (((dev + 1.5 + TILT_RANGE) / TILT_BIN).ceil() as usize).min(steps - 1);
        for (k, &(s, c)) in trig.iter().enumerate().take(k1 + 1).skip(k0) {
            let rho = (y * c - x * s).round() as i64 + offset;
            if (0..rows).contains(&rho) {
                acc[k * rows as usize + rho as usize] += 1;
            }
        }
    }
    // ช่องที่มีจุดมากที่สุดสิบกว่าช่องแรก แล้ววัดความยาวส่วนที่ต่อเนื่องจริงของแต่ละช่อง
    let mut cells: Vec<(u32, usize)> =
        acc.iter().copied().enumerate().filter(|(_, v)| *v > 0).map(|(i, v)| (v, i)).collect();
    cells.sort_unstable_by_key(|c| std::cmp::Reverse(c.0));
    let gap = (w as f64 * 0.03).max(4.0);
    let mut best: Option<(f64, f64)> = None;
    for &(_, cell) in cells.iter().take(12) {
        let (k, rho) = (cell / rows as usize, (cell % rows as usize) as i64 - offset);
        let (s, c) = trig[k];
        let mut xs: Vec<f64> =
            horizontal.iter().filter(|p| ((p.1 * c - p.0 * s).round() as i64 - rho).abs() <= 1).map(|p| p.0).collect();
        xs.sort_by(f64::total_cmp);
        let (mut longest, mut start) = (0.0f64, xs.first().copied().unwrap_or(0.0));
        for pair in xs.windows(2) {
            if pair[1] - pair[0] > gap {
                start = pair[1];
            }
            longest = longest.max(pair[1] - start);
        }
        let reach = longest / w as f64;
        if best.is_none_or(|b| reach > b.1) {
            best = Some((k as f64 * TILT_BIN - TILT_RANGE, reach));
        }
    }
    best
}

/// กรอบตัดที่เสนอ (สัดส่วนบนภาพหลังหมุนและปรับเอียง = พิกัดเดียวกับ `EditSettings::crop`)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CropSuggestion {
    pub crop: Rect,
    /// สัดส่วนของกรอบ: "original" (เท่าภาพ) หรือ "1:1" "4:5" "3:2" "16:9" …
    pub ratio: String,
    pub score: f64,
}

/// แผนที่สองชั้นบนภาพย่อ: ความเด่นทั่วไป (สีที่ต่างจากสีเฉลี่ยของภาพ — frequency-tuned saliency) และตัวแบบ
/// (ใบหน้า + ลำตัวใต้ใบหน้าโดยประมาณ) — ไม่มีใบหน้า: ตัวแบบ = ส่วนที่เด่นที่สุดของภาพ
struct Maps {
    general: Vec<f64>,
    subject: Vec<f64>,
    w: usize,
    h: usize,
}

fn saliency(frame: &RgbImage, faces: &[[f64; 4]]) -> Maps {
    let small = downscale(frame, SALIENCY_EDGE);
    let (w, h) = (small.width() as usize, small.height() as usize);
    let lab: Vec<[f64; 3]> = imageops::blur(&small, 1.5).pixels().map(|p| srgb_to_lab(p.0)).collect();
    let n = lab.len().max(1) as f64;
    let mean = lab.iter().fold([0.0; 3], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
    let mut general: Vec<f64> = lab
        .iter()
        .map(|p| ((p[0] - mean[0]).powi(2) + (p[1] - mean[1]).powi(2) + (p[2] - mean[2]).powi(2)).sqrt())
        .collect();
    let top = general.iter().copied().fold(0.0, f64::max).max(1e-6);
    for v in &mut general {
        *v = (*v / top).powi(2);
    }
    let mut subject = vec![0f64; w * h];
    let mut add = |x0: f64, y0: f64, x1: f64, y1: f64, value: f64| {
        let (ax, ay) = ((x0 * w as f64).floor().max(0.0) as usize, (y0 * h as f64).floor().max(0.0) as usize);
        let (bx, by) = ((x1 * w as f64).ceil().min(w as f64) as usize, (y1 * h as f64).ceil().min(h as f64) as usize);
        for y in ay..by {
            for x in ax..bx {
                subject[y * w + x] = f64::max(subject[y * w + x], value);
            }
        }
    };
    for f in faces {
        let (cx, fw, fh) = (f[0] + f[2] / 2.0, f[2], f[3]);
        add(cx - 1.1 * fw, f[1] + fh, cx + 1.1 * fw, f[1] + 3.6 * fh, 0.5);
        add(f[0] - 0.1 * fw, f[1] - 0.15 * fh, f[0] + 1.1 * fw, f[1] + 1.1 * fh, 1.0);
    }
    if faces.is_empty() {
        // ส่วนที่เด่นที่สุดราว 15% ของภาพเป็นตัวแบบ
        let mut sorted = general.clone();
        sorted.sort_by(f64::total_cmp);
        let cut = sorted[(sorted.len() as f64 * 0.85) as usize];
        for (s, g) in subject.iter_mut().zip(&general) {
            if *g >= cut {
                *s = *g;
            }
        }
    }
    Maps { general, subject, w, h }
}

fn srgb_to_lab(p: [u8; 3]) -> [f64; 3] {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(p[0]), lin(p[1]), lin(p[2]));
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// ผลรวมในสี่เหลี่ยมจาก integral image
struct Integral {
    sum: Vec<f64>,
    sx: Vec<f64>,
    sy: Vec<f64>,
    w: usize,
}

impl Integral {
    fn new(s: &[f64], w: usize, h: usize) -> Integral {
        let stride = w + 1;
        let mut sum = vec![0f64; stride * (h + 1)];
        let mut sx = sum.clone();
        let mut sy = sum.clone();
        for y in 0..h {
            let (mut row, mut row_x, mut row_y) = (0f64, 0f64, 0f64);
            for x in 0..w {
                let v = s[y * w + x];
                row += v;
                row_x += v * (x as f64 + 0.5);
                row_y += v * (y as f64 + 0.5);
                let i = (y + 1) * stride + x + 1;
                sum[i] = sum[i - stride] + row;
                sx[i] = sx[i - stride] + row_x;
                sy[i] = sy[i - stride] + row_y;
            }
        }
        Integral { sum, sx, sy, w }
    }

    fn rect(t: &[f64], stride: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
        t[y1 * stride + x1] - t[y0 * stride + x1] - t[y1 * stride + x0] + t[y0 * stride + x0]
    }

    /// (ผลรวม, จุดศูนย์ถ่วง x, y) ของช่วง [x0, x1) × [y0, y1)
    fn mass(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> (f64, f64, f64) {
        let stride = self.w + 1;
        let m = Self::rect(&self.sum, stride, x0, y0, x1, y1);
        if m <= 1e-9 {
            return (0.0, (x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0);
        }
        (m, Self::rect(&self.sx, stride, x0, y0, x1, y1) / m, Self::rect(&self.sy, stride, x0, y0, x1, y1) / m)
    }
}

/// สัดส่วนกรอบที่ลองเมื่อผู้ใช้ไม่ได้ล็อกสัดส่วน (กว้าง/สูง) — แนวเดียวกับภาพ + จัตุรัส
fn ratios_for(frame_ratio: f64) -> Vec<(String, f64)> {
    let mut out = vec![("original".to_string(), frame_ratio), ("1:1".to_string(), 1.0)];
    if frame_ratio >= 1.0 {
        out.extend([("3:2".to_string(), 1.5), ("4:3".to_string(), 4.0 / 3.0), ("16:9".to_string(), 16.0 / 9.0)]);
    } else {
        out.extend([("4:5".to_string(), 0.8), ("2:3".to_string(), 2.0 / 3.0)]);
    }
    out.dedup_by(|a, b| (a.1 - b.1).abs() < 0.01);
    out
}

/// เสนอกรอบตัด 1–3 แบบบนภาพที่หมุน/ปรับเอียงแล้ว (`frame`) · `faces` = กรอบใบหน้าบน `frame` (สัดส่วน)
/// · `ratio` = สัดส่วนที่ผู้ใช้ล็อกไว้ (กว้าง/สูง ของภาพผล) — None = สัดส่วนเดิมก่อน แล้วสัดส่วนอื่นเป็นทางเลือก
/// แบบแรกคือแบบที่หลวงพี่ครอปเองใน Lightroom: คงสัดส่วน ตัดเหลือราว 90% ให้กระชับรอบตัวคน ตัดด้านที่รก (ข้อ 38)
pub fn suggest_crops(frame: &RgbImage, faces: &[[f64; 4]], ratio: Option<f64>) -> Vec<CropSuggestion> {
    let maps = saliency(frame, faces);
    let (w, h) = (maps.w, maps.h);
    if w < 8 || h < 8 {
        return Vec::new();
    }
    let subject = Integral::new(&maps.subject, w, h);
    let general = Integral::new(&maps.general, w, h);
    let frame_ratio = f64::from(frame.width()) / f64::from(frame.height().max(1));
    let candidates = match ratio {
        Some(r) => vec![("locked".to_string(), r)],
        None => ratios_for(frame_ratio),
    };
    // ใบหน้าที่ต้องอยู่ในกรอบทั้งใบ: ใหญ่อย่างน้อยหนึ่งในสี่ของใบใหญ่สุด
    let largest = faces.iter().map(|f| f[2] * f[3]).fold(0.0, f64::max);
    let key_faces: Vec<[f64; 4]> = faces.iter().copied().filter(|f| f[2] * f[3] >= largest * 0.25).collect();
    let group = key_faces.len() >= 3;
    let step = ((w.max(h) as f64) * 0.015).round().max(1.0) as usize;
    let ctx = Context { subject: &subject, general: &general, faces: &key_faces, group, w, h };

    let mut best: Vec<CropSuggestion> = Vec::new();
    for (label, r) in candidates {
        // กรอบใหญ่สุดของสัดส่วนนี้ (พิกเซลของแผนที่) — r คือสัดส่วนของภาพผล
        let (mut cw, mut ch) = (w as f64, w as f64 / r);
        if ch > h as f64 {
            ch = h as f64;
            cw = h as f64 * r;
        }
        let tighten = label == "original" || label == "locked";
        let mut top: Option<CropSuggestion> = None;
        for k in 0..12 {
            let scale = 1.0 - 0.025 * k as f64;
            let (bw, bh) = ((cw * scale).round() as usize, (ch * scale).round() as usize);
            if bw < 4 || bh < 4 || bw > w || bh > h {
                continue;
            }
            let mut y0 = 0;
            while y0 + bh <= h {
                let mut x0 = 0;
                while x0 + bw <= w {
                    if let Some(score) = ctx.score((x0, y0, bw, bh), tighten) {
                        if top.as_ref().is_none_or(|t| score > t.score) {
                            let crop = Rect {
                                x: x0 as f64 / w as f64,
                                y: y0 as f64 / h as f64,
                                w: bw as f64 / w as f64,
                                h: bh as f64 / h as f64,
                            };
                            top = Some(CropSuggestion { crop, ratio: label.clone(), score });
                        }
                    }
                    if x0 + bw == w {
                        break;
                    }
                    x0 = (x0 + step).min(w - bw);
                }
                if y0 + bh == h {
                    break;
                }
                y0 = (y0 + step).min(h - bh);
            }
        }
        best.extend(top);
    }
    // สัดส่วนเดิมมาก่อนเสมอ (แบบที่หลวงพี่ใช้) ที่เหลือเรียงตามคะแนน
    best.sort_by(|a, b| (b.ratio == "original").cmp(&(a.ratio == "original")).then(b.score.total_cmp(&a.score)));
    let mut out: Vec<CropSuggestion> = Vec::new();
    for c in best {
        if out.iter().all(|o| iou(&o.crop, &c.crop) < 0.85) {
            out.push(c);
        }
        if out.len() == 3 {
            break;
        }
    }
    out
}

struct Context<'a> {
    subject: &'a Integral,
    general: &'a Integral,
    faces: &'a [[f64; 4]],
    group: bool,
    w: usize,
    h: usize,
}

impl Context<'_> {
    /// คะแนนของกรอบ (พิกเซลของแผนที่) — None = ตัดผ่านใบหน้าสำคัญ · `tighten` = แบบคงสัดส่วน (กระชับรอบตัวแบบ ตัดของรก)
    fn score(&self, (x0, y0, bw, bh): (usize, usize, usize, usize), tighten: bool) -> Option<f64> {
        let (w, h) = (self.w as f64, self.h as f64);
        let rect = Rect { x: x0 as f64 / w, y: y0 as f64 / h, w: bw as f64 / w, h: bh as f64 / h };
        for f in self.faces {
            let margin = 0.12 * f[3];
            let inside = f[0] - margin >= rect.x - 1e-9
                && f[1] - margin >= rect.y - 1e-9
                && f[0] + f[2] + margin <= rect.x + rect.w + 1e-9
                && f[1] + f[3] <= rect.y + rect.h + 1e-9;
            if !inside {
                return None;
            }
        }
        let (x1, y1) = (x0 + bw, y0 + bh);
        let (subject_all, _, _) = self.subject.mass(0, 0, self.w, self.h);
        let (subject_in, cx, cy) = self.subject.mass(x0, y0, x1, y1);
        let kept = if subject_all > 0.0 { subject_in / subject_all } else { 1.0 };
        // ความเด่นที่ไม่ใช่ตัวแบบที่ยังอยู่ในกรอบ (ของรกตามขอบ) — ตัดออกได้ดี
        let (general_all, _, _) = self.general.mass(0, 0, self.w, self.h);
        let (general_in, _, _) = self.general.mass(x0, y0, x1, y1);
        let clutter = if general_all > 0.0 { (general_in - subject_in.min(general_in)) / general_all } else { 0.0 };
        // จุดศูนย์ถ่วงของตัวแบบในกรอบ (สัดส่วนของกรอบ)
        let (u, v) = ((cx - x0 as f64) / bw as f64, (cy - y0 as f64) / bh as f64);
        let near = |a: f64, b: f64, s: f64| (-((a - b) * (a - b)) / (2.0 * s * s)).exp();
        let thirds = [1.0 / 3.0, 2.0 / 3.0].iter().map(|&a| near(u, a, 0.08)).fold(0.0, f64::max);
        let centred = near(u, 0.5, 0.08);
        let balance = if self.group { centred.max(thirds * 0.8) } else { thirds.max(centred * 0.85) };
        // พื้นที่เหนือศีรษะ: หน้าบนสุดควรเริ่มที่ราว 6–28% ของความสูงกรอบ
        let head = match self.faces.iter().map(|f| f[1]).reduce(f64::min) {
            Some(top) => {
                let t = (top - rect.y) / rect.h;
                if (0.06..=0.28).contains(&t) {
                    1.0
                } else {
                    (1.0 - (t - 0.17).abs() * 3.0).max(0.0)
                }
            }
            None => 0.5,
        };
        let _ = v;
        let scale = (rect.w * rect.h).sqrt();
        if tighten {
            // แบบที่หลวงพี่ครอปเอง: ตัดเหลือราว 90% (86–94% ในรูปที่ครอปจริง)
            let size = near(scale, 0.9, 0.05);
            Some(3.0 * kept - 1.2 * clutter + 0.6 * head + 0.5 * balance + 0.8 * size)
        } else {
            let content = if general_all > 0.0 { (general_in + subject_in) / (general_all + subject_all) } else { 1.0 };
            let size = if scale >= 0.75 { 1.0 } else { scale / 0.75 };
            Some(2.0 * content + 2.0 * kept + 1.0 * balance + 0.6 * head + 0.8 * size)
        }
    }
}

fn iou(a: &Rect, b: &Rect) -> f64 {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    inter / (a.w * a.h + b.w * b.h - inter).max(1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// ฉากที่มีเส้นแนวนอนและแนวตั้งชัด ๆ (หน้าต่างอาคาร) เอียงไป `deg` องศาตามเข็มนาฬิกา
    fn building(deg: f64) -> RgbImage {
        let base = RgbImage::from_fn(900, 600, |x, y| {
            let window = (x % 90 > 20 && x % 90 < 70) && (y % 80 > 20 && y % 80 < 60);
            if window {
                Rgb([40, 60, 90])
            } else {
                Rgb([200, 190, 170])
            }
        });
        if deg == 0.0 {
            return base;
        }
        // หมุนภาพรอบกลางด้วย bilinear (พิกัด y ชี้ลง: บวก = ตามเข็มนาฬิกา)
        let (s, c) = deg.to_radians().sin_cos();
        RgbImage::from_fn(900, 600, |x, y| {
            let (dx, dy) = (x as f64 - 450.0, y as f64 - 300.0);
            let (sx, sy) = (dx * c + dy * s + 450.0, -dx * s + dy * c + 300.0);
            let (ix, iy) = (sx.clamp(0.0, 898.0), sy.clamp(0.0, 598.0));
            let (x0, y0) = (ix.floor() as u32, iy.floor() as u32);
            let (fx, fy) = (ix - x0 as f64, iy - y0 as f64);
            let p = |xx: u32, yy: u32| base.get_pixel(xx, yy).0;
            let mut out = [0u8; 3];
            for (k, o) in out.iter_mut().enumerate() {
                let top = f64::from(p(x0, y0)[k]) * (1.0 - fx) + f64::from(p(x0 + 1, y0)[k]) * fx;
                let bottom = f64::from(p(x0, y0 + 1)[k]) * (1.0 - fx) + f64::from(p(x0 + 1, y0 + 1)[k]) * fx;
                *o = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
            Rgb(out)
        })
    }

    #[test]
    fn tilt_is_found_from_straight_lines_in_both_directions() {
        for deg in [2.5, -3.2, 6.0] {
            let tilt = detect_tilt(&building(deg)).expect("มีเส้นตรงชัด");
            assert!((tilt.degrees - deg).abs() <= 0.3, "เอียง {deg}° หาได้ {tilt:?}");
        }
        let level = detect_tilt(&building(0.0)).expect("มีเส้นตรงชัด");
        assert!(level.degrees.abs() <= 0.2, "{level:?}");
        // ภาพเรียบไม่มีเส้น = ไม่เสนอ
        assert_eq!(detect_tilt(&RgbImage::from_pixel(400, 300, Rgb([120, 120, 120]))), None);
    }

    /// ฉากพื้นเรียบ มีคนหนึ่งคน (ใบหน้า + ลำตัวสีเด่น) อยู่ตรงกลางภาพ
    fn portrait_scene(cx: f64) -> (RgbImage, Vec<[f64; 4]>) {
        let (w, h) = (900u32, 600u32);
        let face = [cx - 0.04, 0.22, 0.08, 0.12];
        let img = RgbImage::from_fn(w, h, |x, y| {
            let (u, v) = (x as f64 / w as f64, y as f64 / h as f64);
            let in_face = u >= face[0] && u <= face[0] + face[2] && v >= face[1] && v <= face[1] + face[3];
            let in_body = (u - cx).abs() < 0.08 && v > 0.34 && v < 0.8;
            if in_face {
                Rgb([220, 170, 140])
            } else if in_body {
                Rgb([230, 120, 30])
            } else {
                Rgb([90, 110, 100])
            }
        });
        (img, vec![face])
    }

    #[test]
    fn crops_keep_faces_whole_and_put_a_single_subject_on_a_third() {
        let (img, faces) = portrait_scene(0.5);
        let crops = suggest_crops(&img, &faces, None);
        assert!(!crops.is_empty() && crops.len() <= 3);
        for c in &crops {
            let f = faces[0];
            assert!(
                f[0] >= c.crop.x && f[0] + f[2] <= c.crop.x + c.crop.w && f[1] >= c.crop.y,
                "ใบหน้าอยู่ในกรอบทั้งใบ {c:?}"
            );
            assert!(c.crop.w * c.crop.h >= 0.25, "ไม่ตัดจนเล็กเกินไป {c:?}");
        }
        // สัดส่วนที่ล็อกไว้: ทุกกรอบเป็นสัดส่วนนั้น (กว้าง/สูง ของภาพผล)
        let square = suggest_crops(&img, &faces, Some(1.0));
        for c in &square {
            let px = (c.crop.w * 900.0) / (c.crop.h * 600.0);
            assert!((px - 1.0).abs() < 0.03, "{c:?}");
        }
        // คนอยู่ชิดซ้ายของภาพ: กรอบที่ดีที่สุดเลื่อนให้คนไปอยู่แนวสามส่วน ไม่ใช่ชิดขอบ
        let (img, faces) = portrait_scene(0.2);
        let best = &suggest_crops(&img, &faces, Some(1.5))[0];
        let u = (0.2 - best.crop.x) / best.crop.w;
        assert!((u - 1.0 / 3.0).abs() < 0.12, "ตำแหน่งคนในกรอบ {u:.2} ({best:?})");
    }
}
