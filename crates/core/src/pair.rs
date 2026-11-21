//! RAW + JPEG ที่เป็นภาพเดียวกัน (DECISIONS ข้อ 26) — แบบ Lightroom: รูปหนึ่งรูปในคลัง ไฟล์หลักคือ JPEG
//! (ภาพที่แสดง ใบหน้า ชื่อ อยู่บน JPEG) และมีไฟล์ RAW เป็นคู่ ใช้ล้างภาพใหม่ตอนแต่ง · ไฟล์ทั้งสองไม่ถูกแตะ
//!
//! จับคู่เมื่อ (ก) XMP ของ JPEG บอกว่าส่งออกจากไฟล์ RAW ชื่อนั้น (`crs:RawFileName` ของ Lightroom — RAW อยู่โฟลเดอร์ไหน
//! ก็ได้ในกิจกรรมเดียวกัน) หรือ (ข) ไฟล์ RAW ชื่อเดียวกันในโฟลเดอร์เดียวกัน (กล้องบันทึก RAW+JPEG) · อ่าน XMP เฉพาะ JPEG
//! ที่มี RAW ชื่อเดียวกันอยู่ในกิจกรรม (JPEG ที่เปลี่ยนชื่อตอนส่งออกจึงไม่จับคู่)
//!
//! กรอบของ JPEG บนภาพ RAW มาจาก `crs:CropTop/Left/Bottom/Right/CropAngle` — พิกัดบนภาพตามแนวที่เก็บในไฟล์ RAW
//! (ก่อนหมุนตาม Orientation) ของมุมบนซ้ายและล่างขวาของกรอบที่เอียงแล้ว (ตรวจกับไฟล์จริง 135 คู่: ขนาดตรงทุกไฟล์)
//! การแต่งของรูปคู่จึงล้างภาพจาก RAW แล้ววางลงกรอบเดียวกับ JPEG ก่อนตัด/หมุนตามที่แต่ง · การแก้เลนส์ของ Lightroom
//! (ความโค้งของภาพ) ทำตามไม่ได้ — ภาพจาก RAW ต่างจาก JPEG ได้ราว 0.3% ที่ขอบภาพของเลนส์ที่โค้งมาก

use crate::adjust::Adjust;
use crate::edit::{EditSettings, Geometry};
use crate::error::{AppError, Result};
use crate::raw::{self, Linear};
use image::{ImageBuffer, Rgb};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

const XMP_MAIN: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_EXTENSION: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
/// อ่านหัวไฟล์ไม่เกินนี้ (XMP ของ Lightroom ที่มี Mask หลายอันยาวราว 20–200 KB)
const MAX_HEADER_BYTES: u64 = 8 * 1024 * 1024;
/// สัดส่วนของกรอบที่คิดได้ต้องต่างจากของ JPEG ไม่เกินนี้ (ของจริงต่างไม่ถึง 0.02%)
const MAX_ASPECT_ERROR: f64 = 0.01;

// ───────────────────────── XMP ─────────────────────────

/// ข้อความ XMP ทั้งหมดของ JPEG (ส่วนหลัก + ส่วนขยายที่ Adobe แยกไว้เมื่อยาวเกิน 64 KB) — ไม่มี = None
pub fn jpeg_xmp(bytes: &[u8]) -> Option<String> {
    xmp_from(std::io::Cursor::new(bytes))
}

/// เหมือน [`jpeg_xmp`] แต่อ่านจากไฟล์เฉพาะส่วนหัว (ไม่อ่านข้อมูลภาพ)
pub fn read_jpeg_xmp(path: &Path) -> Option<String> {
    xmp_from(std::io::BufReader::new(std::fs::File::open(path).ok()?))
}

fn xmp_from<R: Read + Seek>(mut r: R) -> Option<String> {
    let mut two = [0u8; 2];
    r.read_exact(&mut two).ok()?;
    if two != [0xFF, 0xD8] {
        return None;
    }
    let mut main: Option<Vec<u8>> = None;
    let mut extension: Vec<(u32, Vec<u8>)> = vec![];
    let mut byte = [0u8; 1];
    loop {
        // marker: 0xFF (อาจมี 0xFF คั่นหลายตัว) ตามด้วยรหัส
        r.read_exact(&mut byte).ok()?;
        if byte[0] != 0xFF {
            break;
        }
        let mut marker = 0xFF;
        while marker == 0xFF {
            r.read_exact(&mut byte).ok()?;
            marker = byte[0];
        }
        if (0xD0..=0xD8).contains(&marker) || marker == 0x01 {
            continue;
        }
        // เริ่มข้อมูลภาพ: segment ข้อมูลกำกับอยู่ก่อนหน้านี้ทั้งหมด
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        r.read_exact(&mut two).ok()?;
        let len = u16::from_be_bytes(two) as usize;
        if len < 2 || r.stream_position().ok()? > MAX_HEADER_BYTES {
            break;
        }
        if marker == 0xE1 {
            let mut payload = vec![0u8; len - 2];
            r.read_exact(&mut payload).ok()?;
            if payload.starts_with(XMP_MAIN) {
                main = Some(payload[XMP_MAIN.len()..].to_vec());
            } else if payload.starts_with(XMP_EXTENSION) && payload.len() > XMP_EXTENSION.len() + 40 {
                // GUID 32 ไบต์ · ความยาวทั้งหมด 4 ไบต์ · ตำแหน่งของส่วนนี้ 4 ไบต์ · ข้อมูล
                let rest = &payload[XMP_EXTENSION.len()..];
                let offset = u32::from_be_bytes([rest[36], rest[37], rest[38], rest[39]]);
                extension.push((offset, rest[40..].to_vec()));
            }
        } else {
            r.seek_relative(len as i64 - 2).ok()?;
        }
    }
    let mut text = String::from_utf8_lossy(&main?).into_owned();
    extension.sort_by_key(|(offset, _)| *offset);
    for (_, part) in extension {
        text.push_str(&String::from_utf8_lossy(&part));
    }
    Some(text)
}

/// ค่าของ attribute `crs:Key="…"` ตัวแรก (ชื่อเต็ม ไม่ใช่ท้ายของชื่ออื่น) — ค่าหลักอยู่ก่อนส่วน Mask/Look เสมอ
fn attr<'a>(xmp: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("crs:{key}=\"");
    let mut from = 0;
    while let Some(i) = xmp[from..].find(&needle) {
        let at = from + i;
        if at == 0 || xmp.as_bytes()[at - 1].is_ascii_whitespace() {
            let start = at + needle.len();
            let end = xmp[start..].find('"')? + start;
            return Some(&xmp[start..end]);
        }
        from = at + needle.len();
    }
    None
}

fn num(xmp: &str, key: &str) -> Option<f64> {
    attr(xmp, key).and_then(|v| v.trim().trim_start_matches('+').parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// ชื่อไฟล์ RAW ที่ JPEG นี้ส่งออกมา (เฉพาะชื่อ) — ไม่ใช่ไฟล์ RAW (เช่นแต่ง JPEG ใน Lightroom) = None
pub fn raw_file_name(xmp: &str) -> Option<String> {
    let name = attr(xmp, "RawFileName")?.trim();
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    (!name.is_empty() && raw::is_raw_path(Path::new(name))).then(|| name.to_string())
}

// ───────────────────────── กรอบและค่าที่แต่งใน Lightroom ─────────────────────────

/// กรอบของ JPEG บนภาพ RAW: มุมบนซ้ายและล่างขวาของกรอบที่เอียงแล้ว เป็นสัดส่วนของภาพตามแนวที่เก็บในไฟล์ RAW
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    /// องศาที่เอียง
    pub angle: f64,
}

impl Frame {
    /// ทั้งภาพ ไม่ตัดไม่เอียง (JPEG จากกล้อง)
    pub const FULL: Frame = Frame { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0, angle: 0.0 };

    /// กรอบตามค่าใน XMP — ไม่ได้ตัด (`HasCrop` ไม่ใช่ True) = ทั้งภาพ
    pub fn from_xmp(xmp: &str) -> Frame {
        if !attr(xmp, "HasCrop").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
            return Frame::FULL;
        }
        let get = |k: &str, d: f64| num(xmp, k).unwrap_or(d);
        let f = Frame {
            left: get("CropLeft", 0.0),
            top: get("CropTop", 0.0),
            right: get("CropRight", 1.0),
            bottom: get("CropBottom", 1.0),
            angle: get("CropAngle", 0.0),
        };
        let sane = f.right > f.left && f.bottom > f.top && f.angle.abs() <= 45.0;
        if sane {
            f
        } else {
            Frame::FULL
        }
    }

    /// ขนาดของกรอบ (พิกเซลของภาพตามแนวที่เก็บ `w`×`h`) ก่อนหมุนตาม Orientation
    fn crop_size(&self, w: f64, h: f64) -> (f64, f64) {
        let (dx, dy) = ((self.right - self.left) * w, (self.bottom - self.top) * h);
        let (sin, cos) = self.angle.to_radians().sin_cos();
        (dx * cos + dy * sin, -dx * sin + dy * cos)
    }

    /// ขนาดภาพ (ตั้งตรงแล้ว) ที่กรอบนี้ให้ บนภาพ RAW ที่ตั้งตรงแล้วขนาด `upright` และ Orientation ของไฟล์ RAW
    pub fn output_size(&self, upright: (u32, u32), orientation: u8) -> (f64, f64) {
        let (w, h) = stored_size(upright, orientation);
        let (cw, ch) = self.crop_size(w, h);
        if orientation >= 5 {
            (ch, cw)
        } else {
            (cw, ch)
        }
    }

    /// จุดบน JPEG (สัดส่วน 0–1) → จุดบนภาพ RAW ที่ตั้งตรงแล้ว (สัดส่วน 0–1)
    pub fn to_raw(&self, u: f64, v: f64, upright: (u32, u32), orientation: u8) -> (f64, f64) {
        let (w, h) = stored_size(upright, orientation);
        let (cw, ch) = self.crop_size(w, h);
        let (s, t) = unorient(orientation, u, v);
        let (s, t) = (s * cw, t * ch);
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let x = self.left * w + s * cos - t * sin;
        let y = self.top * h + s * sin + t * cos;
        orient(orientation, x / w, y / h)
    }
}

/// ขนาดภาพตามแนวที่เก็บในไฟล์ จากขนาดที่ตั้งตรงแล้ว
fn stored_size((w, h): (u32, u32), orientation: u8) -> (f64, f64) {
    if orientation >= 5 {
        (h as f64, w as f64)
    } else {
        (w as f64, h as f64)
    }
}

/// จุดบนภาพตามแนวที่เก็บ → บนภาพที่ตั้งตรงตาม Orientation (สัดส่วน)
fn orient(o: u8, x: f64, y: f64) -> (f64, f64) {
    match o {
        2 => (1.0 - x, y),
        3 => (1.0 - x, 1.0 - y),
        4 => (x, 1.0 - y),
        5 => (y, x),
        6 => (1.0 - y, x),
        7 => (1.0 - y, 1.0 - x),
        8 => (y, 1.0 - x),
        _ => (x, y),
    }
}

/// ย้อน [`orient`]
fn unorient(o: u8, x: f64, y: f64) -> (f64, f64) {
    match o {
        2 => (1.0 - x, y),
        3 => (1.0 - x, 1.0 - y),
        4 => (x, 1.0 - y),
        5 => (y, x),
        6 => (y, 1.0 - x),
        7 => (1.0 - y, 1.0 - x),
        8 => (1.0 - y, x),
        _ => (x, y),
    }
}

/// สิ่งที่ Lightroom บันทึกไว้ใน JPEG ที่ส่งออกจาก RAW
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lightroom {
    pub frame: Frame,
    /// ค่าแสงและสีที่ตัวแต่งภาพของเราทำตามได้ (สมดุลแสงขาวอยู่ใน `white_balance`)
    pub adjust: Adjust,
    /// สมดุลแสงขาวที่ตั้งเอง [อุณหภูมิสี K, Tint] — None = ตามกล้อง (As Shot)
    pub white_balance: Option<[f64; 2]>,
    /// ส่วนที่ทำตามไม่ได้ (Mask, Denoise, แก้เลนส์ …)
    pub unsupported: Vec<String>,
}

impl Lightroom {
    /// None = XMP นี้ไม่มีค่าของ Lightroom / Camera Raw
    pub fn from_xmp(xmp: &str) -> Option<Lightroom> {
        let parsed = crate::preset::parse_lightroom(xmp).ok()?;
        let mut adjust = parsed.adjust;
        // สมดุลแสงขาวของ RAW เป็น Kelvin: คิดเทียบกับค่าของกล้องตอนแต่ง (ต้องใช้ข้อมูลในไฟล์ RAW) ไม่ใช่โดยประมาณแบบ preset
        let custom = attr(xmp, "WhiteBalance").is_some_and(|wb| wb != "As Shot");
        let white_balance = match (custom, num(xmp, "Temperature")) {
            (true, Some(k)) => Some([k.clamp(2000.0, 50000.0), num(xmp, "Tint").unwrap_or(0.0).clamp(-150.0, 150.0)]),
            _ => None,
        };
        if num(xmp, "IncrementalTemperature").is_none() {
            (adjust.temperature, adjust.tint) = (0.0, 0.0);
        }
        let unsupported = parsed.unsupported.into_iter().filter(|u| !u.starts_with("อุณหภูมิสี")).collect();
        Some(Lightroom { frame: Frame::from_xmp(xmp), adjust, white_balance, unsupported })
    }
}

// ───────────────────────── สมดุลแสงขาว ─────────────────────────

type M3 = [[f64; 3]; 3];

fn mul(m: &M3, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

const BRADFORD: M3 = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
const BRADFORD_INV: M3 =
    [[0.986_993, -0.147_054, 0.159_963], [0.432_305, 0.518_360, 0.049_291], [-0.008_529, 0.040_043, 0.968_487]];
const XYZ_TO_SRGB: M3 = [[3.2406, -1.5372, -0.4986], [-0.9689, 1.8758, 0.0415], [0.0557, -0.2040, 1.0570]];
const D65: [f64; 2] = [0.312_7, 0.329_0];

/// เส้นของแสงจากวัตถุดำที่อุณหภูมิ `k` บนแผนภาพ CIE 1960 uv (สูตรของ Krystek 1985 — คลาดไม่เกิน 8×10⁻⁵ ช่วง 1000–15000 K)
fn planck_uv(k: f64) -> [f64; 2] {
    let u = (0.860_117_757 + 1.541_182_54e-4 * k + 1.286_412_12e-7 * k * k)
        / (1.0 + 8.424_202_35e-4 * k + 7.081_451_63e-7 * k * k);
    let v = (0.317_398_726 + 4.228_062_45e-5 * k + 4.204_816_91e-8 * k * k)
        / (1.0 - 2.897_418_16e-5 * k + 1.614_560_53e-7 * k * k);
    [u, v]
}

/// จุดขาว xy ของแสงที่อุณหภูมิสี `kelvin` และ Tint ตามแบบของ Lightroom: Tint 1 หน่วย = เลื่อนตั้งฉากกับเส้นของแสง
/// 1/3000 บน uv · Tint บวก (ภาพอมม่วงแดง) = แสงที่ถูกแก้อมเขียว
pub fn kelvin_xy(kelvin: f64, tint: f64) -> [f64; 2] {
    let k = kelvin.clamp(1000.0, 15000.0);
    let [u, v] = planck_uv(k);
    let (a, b) = (planck_uv(k - 1.0), planck_uv(k + 1.0));
    let (du, dv) = (b[0] - a[0], b[1] - a[1]);
    let len = (du * du + dv * dv).sqrt().max(1e-12);
    // ตั้งฉากกับเส้นของแสง ด้านที่ v มากขึ้น (เขียว)
    let (nu, nv) = if du > 0.0 { (-dv / len, du / len) } else { (dv / len, -du / len) };
    let (u, v) = (u + nu * tint / 3000.0, v + nv * tint / 3000.0);
    let d = 2.0 * u - 8.0 * v + 4.0;
    [3.0 * u / d, 2.0 * v / d]
}

fn white_xyz([x, y]: [f64; 2]) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// ค่าอุณหภูมิสีและโทนสีของเรา (−100 ถึง 100) ที่ทำให้ภาพจาก RAW ได้สมดุลแสงขาวตามที่ตั้งใน Lightroom
/// — `as_shot` จุดขาวที่กล้องบันทึก (จากไฟล์ RAW) · `target` [K, Tint] ของ Lightroom
///
/// ภาพที่แก้แสงด้วยจุดขาวของกล้องทำให้วัตถุสีเทาเป็นกลาง · แก้ด้วยจุดขาวของ Lightroom แทน วัตถุสีเทาจะได้สีตาม
/// Bradford(จุดขาวของ Lightroom → D65) ของแสงตามกล้อง — แถบของเราคูณช่องแดง/น้ำเงิน/เขียวในแสงเชิงเส้น จึงคิดย้อนจากสีนั้น
pub fn white_balance_sliders(as_shot: [f64; 2], target: [f64; 2]) -> (f64, f64) {
    let src = mul(&BRADFORD, white_xyz(kelvin_xy(target[0], target[1])));
    let dst = mul(&BRADFORD, white_xyz(D65));
    let cone = mul(&BRADFORD, white_xyz(as_shot));
    let adapted = mul(&BRADFORD_INV, std::array::from_fn(|i| cone[i] * dst[i] / src[i]));
    let [r, g, b] = mul(&XYZ_TO_SRGB, adapted);
    if r <= 0.0 || g <= 0.0 || b <= 0.0 {
        return (0.0, 0.0);
    }
    // แถบของเรา: แดง ×(1 + 0.25t) น้ำเงิน ×(1 − 0.25t) เขียว ×(1 − 0.2s) — t, s เป็นสัดส่วนของ 100
    let q = r / b;
    let temperature = 4.0 * (q - 1.0) / (q + 1.0) * 100.0;
    let tint = (1.0 - 2.0 * g / (r + b)) / 0.2 * 100.0;
    (temperature.clamp(-100.0, 100.0), tint.clamp(-100.0, 100.0))
}

// ───────────────────────── จับคู่ไฟล์ ─────────────────────────

/// JPEG หนึ่งไฟล์จับคู่กับ RAW ใด
#[derive(Debug, Clone)]
pub struct Match {
    /// ลำดับใน `raws`
    pub raw: usize,
    /// ค่าของ Lightroom เมื่อจับคู่จาก XMP · None = จับคู่จากชื่อในโฟลเดอร์เดียวกัน (JPEG จากกล้อง)
    pub lightroom: Option<Lightroom>,
}

fn is_jpeg(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
}

fn stem_key(path: &Path) -> Option<String> {
    Some(path.file_stem()?.to_string_lossy().to_lowercase())
}

/// จับคู่ JPEG กับ RAW ในขอบเขตเดียวกัน (กฎอยู่หัวไฟล์) · RAW หนึ่งไฟล์เป็นคู่ของ JPEG ได้หลายไฟล์ (ส่งออกหลายแบบ)
pub fn match_files(jpegs: &[PathBuf], raws: &[PathBuf]) -> Vec<Option<Match>> {
    let mut by_stem: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, path) in raws.iter().enumerate() {
        if let Some(stem) = stem_key(path) {
            by_stem.entry(stem).or_default().push(i);
        }
    }
    jpegs
        .iter()
        .map(|jpeg| {
            if !is_jpeg(jpeg) {
                return None;
            }
            let candidates = by_stem.get(&stem_key(jpeg)?)?;
            let xmp = read_jpeg_xmp(jpeg);
            let same_dir = |i: &usize| raws[*i].parent() == jpeg.parent();
            match xmp.as_deref().and_then(raw_file_name) {
                // XMP บอกชื่อไฟล์: ต้องเป็นไฟล์ชื่อนั้น — อยู่โฟลเดอร์เดียวกันก่อน แล้วจึงโฟลเดอร์ที่ใกล้ที่สุด
                Some(name) => {
                    let named: Vec<usize> = candidates
                        .iter()
                        .copied()
                        .filter(|i| {
                            raws[*i].file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(&name))
                        })
                        .collect();
                    let raw = match named.iter().find(|i| same_dir(i)) {
                        Some(i) => *i,
                        None => nearest(jpeg, raws, &named)?,
                    };
                    Some(Match { raw, lightroom: xmp.as_deref().and_then(Lightroom::from_xmp) })
                }
                None => candidates.iter().copied().find(same_dir).map(|raw| Match { raw, lightroom: None }),
            }
        })
        .collect()
}

/// RAW ที่อยู่ใกล้ JPEG ที่สุด (โฟลเดอร์ร่วมกันลึกที่สุด) — เสมอกันหลายไฟล์ = ไม่แน่ใจ ไม่จับคู่
fn nearest(jpeg: &Path, raws: &[PathBuf], candidates: &[usize]) -> Option<usize> {
    let shared = |i: usize| jpeg.components().zip(raws[i].components()).take_while(|(a, b)| a == b).count();
    let best = candidates.iter().map(|i| shared(*i)).max()?;
    let mut top = candidates.iter().copied().filter(|i| shared(*i) == best);
    let first = top.next()?;
    top.next().is_none().then_some(first)
}

// ───────────────────────── ในคลัง ─────────────────────────

/// คู่ RAW ของรูปหนึ่งในคลัง
#[derive(Debug, Clone)]
pub struct Pair {
    pub raw_path: PathBuf,
    pub filename: String,
    pub lightroom: Option<Lightroom>,
    pub available: bool,
    start: Option<Start>,
}

/// รุ่นของวิธีคิดค่าเริ่มต้น — เปลี่ยนวิธีคิดเมื่อไร (เช่นทำตามค่าของ Lightroom ได้เพิ่ม) เพิ่มเลขนี้ ค่าที่เก็บไว้จะถูกคิดใหม่
/// · 3 = อ่านค่าของ Lightroom ใหม่จาก JPEG และทำตาม Texture/Clarity/Dehaze ได้ (DECISIONS ข้อ 27)
const START_VERSION: u32 = 3;

/// ค่าเริ่มต้นเมื่อแต่งจาก RAW (คิดจากไฟล์ RAW ครั้งแรกที่ต้องใช้ แล้วเก็บไว้)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Start {
    /// None = ใช้ RAW แต่งไม่ได้ (`problem` บอกเหตุ)
    settings: Option<EditSettings>,
    problem: Option<String>,
    #[serde(default)]
    version: u32,
}

fn json<T: serde::de::DeserializeOwned>(v: Option<String>) -> Option<T> {
    v.and_then(|j| serde_json::from_str(&j).ok())
}

pub fn load(conn: &Connection, photo_id: &str) -> Result<Option<Pair>> {
    Ok(conn
        .query_row(
            "SELECT os_path, filename, lightroom, start, available FROM photo_raw_pairs WHERE photo_id = ?1",
            [photo_id],
            |r| {
                Ok(Pair {
                    raw_path: PathBuf::from(r.get::<_, String>(0)?),
                    filename: r.get(1)?,
                    lightroom: json(r.get(2)?),
                    // คิดด้วยวิธีรุ่นก่อน = ยังไม่ได้คิด
                    start: json::<Start>(r.get(3)?).filter(|s| s.version == START_VERSION),
                    available: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// บันทึกคู่ที่พบตอนตรวจรายการ (ค่าที่เคยคิดไว้ล้างทิ้งเมื่อไฟล์คู่หรือค่าของ Lightroom เปลี่ยน)
pub(crate) fn upsert(
    conn: &Connection,
    photo_id: &str,
    raw: &Path,
    lightroom: Option<&Lightroom>,
    scan_stamp: &str,
) -> Result<()> {
    let key = crate::util::path_key(raw);
    let lightroom = lightroom.map(serde_json::to_string).transpose()?;
    conn.execute(
        "INSERT INTO photo_raw_pairs(photo_id, os_path, path_key, filename, lightroom, start, available, verified_at)
         VALUES (?1, ?2, ?2, ?3, ?4, NULL, 1, ?5)
         ON CONFLICT(photo_id) DO UPDATE SET
           start = CASE WHEN path_key = excluded.path_key AND lightroom IS excluded.lightroom THEN start ELSE NULL END,
           os_path = excluded.os_path, path_key = excluded.path_key, filename = excluded.filename,
           lightroom = excluded.lightroom, available = 1, verified_at = excluded.verified_at",
        params![photo_id, key, crate::util::file_name(raw), lightroom, scan_stamp],
    )?;
    Ok(())
}

/// การแต่งของรูปคู่ที่ใช้ RAW ได้: ไฟล์ RAW กรอบ ขนาดของ JPEG และค่าเริ่มต้น
#[derive(Debug, Clone)]
pub struct RawBase {
    pub raw_path: PathBuf,
    pub frame: Frame,
    /// ขนาดของ JPEG (ตั้งตรงแล้ว) — พิกัดของการแต่งเป็นสัดส่วนบนภาพนี้
    pub jpeg: (u32, u32),
    /// ค่าเริ่มต้นเมื่อยังไม่ได้แต่ง (ค่าจาก Lightroom)
    pub start: EditSettings,
    pub unsupported: Vec<String>,
}

/// รูปนี้แต่งจาก RAW ได้ไหม
#[derive(Debug, Clone)]
pub enum RawEdit {
    /// ไม่มีคู่ RAW
    None,
    Ready(Box<RawBase>),
    /// มีคู่แต่ใช้ไม่ได้ตอนนี้ (ไฟล์ไม่พร้อม หรือกรอบไม่ตรงกับ JPEG) — เหตุผลสำหรับผู้ใช้
    Unusable(String),
}

/// คิดค่าเริ่มต้นของรูปคู่ไว้ก่อน โดยอ่านไฟล์ระหว่างที่ไม่ถือฐานข้อมูล (ราว 0.3 วินาทีต่อรูป ครั้งเดียว) — เรียกก่อน
/// [`raw_edit`] จากคำสั่งของ UI · ไม่มีคู่ / คิดไว้แล้ว / ไฟล์ไม่พร้อม = ไม่ทำอะไร ([`raw_edit`] รายงานเหตุเอง)
pub fn ensure_start(app: &crate::App, photo_id: &str) -> Result<()> {
    let (pair, dims, jpeg_path) = {
        let conn = app.db();
        let Some(pair) = load(&conn, photo_id)? else { return Ok(()) };
        if pair.start.is_some() || !pair.available {
            return Ok(());
        }
        let dims: (u32, u32) = conn.query_row(
            "SELECT r.width, r.height FROM photos p JOIN photo_revisions r ON r.id = p.current_revision_id WHERE p.id = ?1",
            [photo_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        (pair, dims, crate::catalog::photo_path(&conn, photo_id).ok())
    };
    if dims.0 == 0 || dims.1 == 0 {
        return Ok(());
    }
    let Ok(raw) = crate::media::read_file(&pair.raw_path) else { return Ok(()) };
    let jpeg = jpeg_path.and_then(|p| crate::media::read_file(Path::new(&p)).ok());
    let lightroom = fresh_lightroom(pair.lightroom.as_ref(), jpeg.as_deref());
    let start = compute_start(&raw, lightroom.as_ref(), dims, jpeg.as_deref());
    // คู่เปลี่ยนระหว่างคิด (ตรวจรูปใหม่พร้อมกัน): ไม่เก็บ
    app.db().execute(
        "UPDATE photo_raw_pairs SET start = ?2, lightroom = ?4 WHERE photo_id = ?1 AND path_key = ?3",
        params![
            photo_id,
            serde_json::to_string(&start)?,
            crate::util::path_key(&pair.raw_path),
            lightroom.map(|l| serde_json::to_string(&l)).transpose()?
        ],
    )?;
    Ok(())
}

/// ค่าของ Lightroom อ่านใหม่จาก XMP ของ JPEG (ตัวอ่านรุ่นใหม่ทำตามได้มากขึ้น เช่น Texture) — อ่านไม่ได้ใช้ค่าที่เก็บไว้ ·
/// คู่ที่จับจากชื่อ (JPEG จากกล้อง) ไม่มีค่าของ Lightroom
fn fresh_lightroom(stored: Option<&Lightroom>, jpeg: Option<&[u8]>) -> Option<Lightroom> {
    let stored = stored?;
    Some(jpeg.and_then(jpeg_xmp).and_then(|x| Lightroom::from_xmp(&x)).unwrap_or_else(|| stored.clone()))
}

/// คิดค่าเริ่มต้นจากไฟล์ RAW ครั้งแรกที่ต้องใช้ (อ่านไฟล์ RAW) แล้วเก็บไว้ · ไฟล์ RAW หายไปภายหลัง = ใช้ไม่ได้
pub fn raw_edit(conn: &Connection, photo_id: &str) -> Result<RawEdit> {
    let Some(mut pair) = load(conn, photo_id)? else { return Ok(RawEdit::None) };
    if !pair.available {
        return Ok(RawEdit::Unusable(format!("ไม่พบไฟล์ RAW คู่ “{}” ในโฟลเดอร์ของกิจกรรม", pair.filename)));
    }
    let (w, h): (u32, u32) = conn.query_row(
        "SELECT r.width, r.height FROM photos p JOIN photo_revisions r ON r.id = p.current_revision_id WHERE p.id = ?1",
        [photo_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if w == 0 || h == 0 {
        return Ok(RawEdit::Unusable("รูปนี้ยังเตรียมข้อมูลไม่เสร็จ".into()));
    }
    let start = match pair.start.clone() {
        Some(start) => start,
        None => {
            let Ok(bytes) = crate::media::read_file(&pair.raw_path) else {
                return Ok(RawEdit::Unusable(format!("เปิดไฟล์ RAW คู่ “{}” ไม่ได้ (ไดรฟ์อาจไม่ได้ต่ออยู่)", pair.filename)));
            };
            let jpeg = crate::catalog::photo_path(conn, photo_id)
                .ok()
                .and_then(|p| crate::media::read_file(Path::new(&p)).ok());
            let lightroom = fresh_lightroom(pair.lightroom.as_ref(), jpeg.as_deref());
            let start = compute_start(&bytes, lightroom.as_ref(), (w, h), jpeg.as_deref());
            conn.execute(
                "UPDATE photo_raw_pairs SET start = ?2, lightroom = ?3 WHERE photo_id = ?1",
                params![
                    photo_id,
                    serde_json::to_string(&start)?,
                    lightroom.as_ref().map(serde_json::to_string).transpose()?
                ],
            )?;
            pair.lightroom = lightroom;
            start
        }
    };
    Ok(match start.settings {
        Some(settings) => RawEdit::Ready(Box::new(RawBase {
            raw_path: pair.raw_path,
            frame: pair.lightroom.as_ref().map_or(Frame::FULL, |l| l.frame),
            jpeg: (w, h),
            start: settings,
            unsupported: pair.lightroom.map(|l| l.unsupported).unwrap_or_default(),
        })),
        None => RawEdit::Unusable(start.problem.unwrap_or_else(|| "ใช้ไฟล์ RAW คู่แต่งรูปนี้ไม่ได้".into())),
    })
}

/// ค่าเริ่มต้น: ค่าจาก Lightroom (สมดุลแสงขาวคิดเทียบกับค่าของกล้อง) แล้วปรับค่าแสงและสมดุลแสงขาวให้ภาพจาก RAW
/// สว่างและสีโดยรวมเท่ากับ JPEG — `jpeg_bytes` = ไฟล์ JPEG ของรูป (ไม่มี = ไม่ปรับ)
fn compute_start(
    raw_bytes: &[u8],
    lightroom: Option<&Lightroom>,
    jpeg: (u32, u32),
    jpeg_bytes: Option<&[u8]>,
) -> Start {
    let problem = |p: String| Start { settings: None, problem: Some(p), version: START_VERSION };
    let info = match raw::info(raw_bytes) {
        Ok(info) => info,
        Err(e) => return problem(e.message),
    };
    let frame = lightroom.map_or(Frame::FULL, |l| l.frame);
    if let Err(e) = check_frame(&frame, &info, jpeg) {
        return problem(e.message);
    }
    let mut adjust = lightroom.map(|l| l.adjust.clone()).unwrap_or_default();
    if let (Some(target), Some(white)) = (lightroom.and_then(|l| l.white_balance), info.white_xy) {
        (adjust.temperature, adjust.tint) = white_balance_sliders(white, target);
    }
    if let Some(bytes) = jpeg_bytes.filter(|b| is_srgb(b)) {
        if let Ok(target) = crate::media::decode_upright(bytes, FIT_EDGE) {
            let base =
                RawBase { raw_path: PathBuf::new(), frame, jpeg, start: EditSettings::default(), unsupported: vec![] };
            if let Ok(framed) = develop_framed(raw_bytes, &base, &EditSettings::default(), FIT_EDGE) {
                let target = image::imageops::resize(
                    &target.to_rgb8(),
                    framed.width(),
                    framed.height(),
                    image::imageops::FilterType::Triangle,
                );
                match_jpeg(&framed, &target, &mut adjust);
            }
        }
    }
    // ค่าที่อยู่นอกช่วงของเรา (เช่นค่าแสงเกิน ±3) ถูกหนีบไว้แล้วตอนอ่าน — ตรวจซ้ำเผื่อไว้
    let settings = EditSettings { adjust, from_raw: true, ..Default::default() };
    match settings.validate() {
        Ok(()) => Start { settings: Some(settings), problem: None, version: START_VERSION },
        Err(e) => problem(e.message),
    }
}

/// ค่าเริ่มต้นเมื่อแต่งจาก RAW ของรูปคู่ (สำหรับเทสต์และเครื่องมือวัด) — Err = ใช้ RAW ไม่ได้ พร้อมเหตุผล
#[doc(hidden)]
pub fn start_settings(
    raw_bytes: &[u8],
    lightroom: Option<&Lightroom>,
    jpeg: (u32, u32),
    jpeg_bytes: Option<&[u8]>,
) -> std::result::Result<EditSettings, String> {
    let start = compute_start(raw_bytes, lightroom, jpeg, jpeg_bytes);
    start.settings.ok_or_else(|| start.problem.unwrap_or_default())
}

/// สัดส่วนของกรอบบน RAW ต้องตรงกับ JPEG — ไม่ตรง = หมุนใน Lightroom หรือ JPEG ของกล้องที่ตั้งสัดส่วนอื่น ใช้ RAW ไม่ได้
fn check_frame(frame: &Frame, info: &raw::RawInfo, jpeg: (u32, u32)) -> Result<()> {
    let (fw, fh) = frame.output_size((info.width, info.height), info.exif.orientation);
    let error = (fw / fh) / (jpeg.0 as f64 / jpeg.1 as f64) - 1.0;
    // NaN (ขนาดเป็นศูนย์) ถือว่าไม่ตรง
    if error.is_nan() || error.abs() > MAX_ASPECT_ERROR {
        return Err(AppError::new(
            "RAW_PAIR_MISMATCH",
            "สัดส่วนของไฟล์ RAW คู่ไม่ตรงกับ JPEG (อาจหมุนภาพใน Lightroom หรือกล้องตั้งสัดส่วนอื่น) จึงแต่งจาก JPEG แทน",
        ));
    }
    Ok(())
}

// ───────────────────────── ภาพ ─────────────────────────

/// ภาพจาก RAW ในกรอบของ JPEG ตามที่แต่ง ด้านยาวไม่เกิน `max_edge` (`u32::MAX` = เต็มขนาด) — ล้างเท่าที่ต้องใช้
pub fn render(raw_bytes: &[u8], base: &RawBase, s: &EditSettings, max_edge: u32) -> Result<image::RgbImage> {
    let framed = develop_framed(raw_bytes, base, s, max_edge)?;
    let g = Geometry::new(base.jpeg.0, base.jpeg.1, s);
    let out = crate::adjust::apply_linear_in_frame(&framed, &s.adjust, g.frame_long(framed.width(), framed.height()));
    Ok(crate::edit::with_masks(image::DynamicImage::ImageRgb8(out), &g, s).into_rgb8())
}

/// ภาพจาก RAW ในกรอบของ JPEG หลังตัด/หมุนตามที่แต่ง (ยังเป็นแสงเชิงเส้น ยังไม่ปรับแสงสี)
///
/// ขนาดเต็ม = ขนาดของ JPEG ที่แต่งแล้ว (เท่ากันทุกพิกเซลเมื่อ JPEG ส่งออกเต็มขนาด) หรือความละเอียดของ RAW
/// ถ้า JPEG ถูกย่อตอนส่งออก · `max_edge` จำกัดด้านยาว
fn develop_framed(raw_bytes: &[u8], base: &RawBase, s: &EditSettings, max_edge: u32) -> Result<Linear> {
    let info = raw::info(raw_bytes)?;
    check_frame(&base.frame, &info, base.jpeg)?;
    let (o, upright) = (info.exif.orientation, (info.width, info.height));
    let (fw, _) = base.frame.output_size(upright, o);
    let g = Geometry::new(base.jpeg.0, base.jpeg.1, s);
    let (ow, oh) = g.output_size();
    // พิกเซลของ RAW ต่อพิกเซลของ JPEG (ต่างกันไม่ถึง 1% = เท่ากัน — เศษจากการปัดขนาดตอนส่งออก)
    let per = fw / base.jpeg.0.max(1) as f64;
    let per = if (per - 1.0).abs() < 0.01 { 1.0 } else { per };
    // ขนาดผลเป็นพิกเซลของ JPEG ที่คูณ `z`
    let z = (max_edge as f64 / ow.max(oh).max(1) as f64).min(per.max(1.0));
    let size = |k: f64| (((ow as f64 * k).round() as u32).max(1), ((oh as f64 * k).round() as u32).max(1));
    let (tw, th) = size(z);
    // ล้างเท่าที่ต้องใช้: ต้องการความละเอียด z/per ของ RAW เต็ม
    let raw_long = info.width.max(info.height) as f64;
    let need = if z >= per { u32::MAX } else { (raw_long * z / per).ceil() as u32 };
    let (lin, _) = raw::develop(raw_bytes, need)?;
    // วางลงกรอบที่ความละเอียดของภาพที่ล้าง แล้วจึงย่อ (ย่อในแสงเชิงเส้น ไม่เกิด aliasing ของการสุ่มจุด)
    let developed = per * lin.width() as f64 / info.width as f64;
    let (sw, sh) = if developed > z * 1.05 { size(developed) } else { (tw, th) };
    let framed: Linear = sample(&lin, sw, sh, |a, b| {
        let (u, v) = g.inverse(a, b);
        base.frame.to_raw(u, v, upright, o)
    });
    Ok(if (sw, sh) == (tw, th) {
        framed
    } else {
        image::imageops::resize(&framed, tw, th, image::imageops::FilterType::CatmullRom)
    })
}

/// ด้านยาวของภาพที่ใช้เทียบกับ JPEG
const FIT_EDGE: u32 = 320;

/// ค่าเฉลี่ย L* a* b* ของภาพ 8 บิต (sRGB) เฉพาะพิกเซลที่ `keep`
fn lab_mean(img: &image::RgbImage, keep: &[bool]) -> [f64; 3] {
    let mut sum = [0.0f64; 3];
    let mut n = 0.0f64;
    for (lab, k) in crate::assistant::look::labs(img).into_iter().zip(keep) {
        if *k {
            for (s, v) in sum.iter_mut().zip(lab) {
                *s += v;
            }
            n += 1.0;
        }
    }
    sum.map(|s| s / n.max(1.0))
}

/// ปรับค่าแสง แล้วอุณหภูมิสี/โทนสี ให้ค่าเฉลี่ย L* a* b* ของภาพจาก RAW เท่ากับของ JPEG (ภาพเดียวกัน กรอบเดียวกัน)
/// — สูตรโทนของเรากับของ Adobe ต่างกัน, Mask ของ Lightroom ทำตามไม่ได้ และสมดุลแสงขาวของกล้องอ่านได้โดยประมาณ
/// (บางไฟล์อ่านไม่ได้เลย) จึงใช้ JPEG เป็นตัวตั้ง · ไม่นับส่วนที่ขาวหรือดำจนหมดรายละเอียดใน JPEG
fn match_jpeg(framed: &Linear, jpeg: &image::RgbImage, adjust: &mut Adjust) {
    let keep: Vec<bool> = jpeg
        .pixels()
        .map(|p| {
            let (hi, lo) = (p.0.iter().max().copied().unwrap_or(0), p.0.iter().min().copied().unwrap_or(0));
            hi < 250 && lo > 5
        })
        .collect();
    if keep.iter().filter(|k| **k).count() < keep.len() / 10 {
        return;
    }
    let target = lab_mean(jpeg, &keep);
    let measure = |a: &Adjust| lab_mean(&crate::adjust::apply_linear(framed, a), &keep);
    let fit_exposure = |a: &mut Adjust| {
        // ความสว่างเพิ่มตามค่าแสงเสมอ: secant 5 รอบ
        let (mut e0, mut l0) = (a.exposure, measure(a)[0]);
        let mut e1 = (e0 + 0.3).min(3.0);
        if e1 == e0 {
            e1 = e0 - 0.3;
        }
        for _ in 0..5 {
            a.exposure = e1;
            let l1 = measure(a)[0];
            if (l1 - target[0]).abs() < 0.2 || (l1 - l0).abs() < 1e-6 {
                break;
            }
            let next = (e1 + (target[0] - l1) * (e1 - e0) / (l1 - l0)).clamp(-3.0, 3.0);
            (e0, l0, e1) = (e1, l1, next);
        }
        a.exposure = (a.exposure * 100.0).round() / 100.0;
    };
    let fit_white = |a: &mut Adjust| {
        // a* b* เปลี่ยนเกือบเป็นเส้นตรงตามอุณหภูมิสี/โทนสี: Newton 2 รอบ อนุพันธ์จากการลองขยับ 10
        for _ in 0..2 {
            let now = measure(a);
            let (da, db) = (target[1] - now[1], target[2] - now[2]);
            if da.abs() < 0.3 && db.abs() < 0.3 {
                break;
            }
            let probe = |dt: f64, ds: f64| {
                let mut b = a.clone();
                b.temperature = (b.temperature + dt).clamp(-100.0, 100.0);
                b.tint = (b.tint + ds).clamp(-100.0, 100.0);
                let m = measure(&b);
                [(m[1] - now[1]) / 10.0, (m[2] - now[2]) / 10.0]
            };
            let (jt, js) = (probe(10.0, 0.0), probe(0.0, 10.0));
            let det = jt[0] * js[1] - js[0] * jt[1];
            if det.abs() < 1e-9 {
                break;
            }
            let dt = (da * js[1] - js[0] * db) / det;
            let ds = (jt[0] * db - da * jt[1]) / det;
            a.temperature = (a.temperature + dt).clamp(-100.0, 100.0).round();
            a.tint = (a.tint + ds).clamp(-100.0, 100.0).round();
        }
    };
    fit_exposure(adjust);
    fit_white(adjust);
    fit_exposure(adjust);
}

/// JPEG ที่ไม่มีโปรไฟล์สี หรือเป็น sRGB (ภาพจาก RAW ของเราเป็น sRGB จึงเทียบกับ JPEG แบบอื่น เช่น Adobe RGB ไม่ได้)
fn is_srgb(jpeg: &[u8]) -> bool {
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF {
        let marker = jpeg[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        let Some(seg) = jpeg.get(at + 4..at + 2 + len) else { break };
        if marker == 0xE2 && seg.starts_with(b"ICC_PROFILE\0") {
            return seg.windows(4).any(|w| w == b"sRGB");
        }
        at += 2 + len;
    }
    true
}

/// ทุกพิกเซลของภาพผลย้อนไปหาจุดบน `src` ด้วย `map` (สัดส่วน → สัดส่วน) แล้วผสมสี่พิกเซลรอบ ๆ (bilinear)
fn sample(src: &Linear, w: u32, h: u32, map: impl Fn(f64, f64) -> (f64, f64)) -> Linear {
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let (max_x, max_y) = (src.width() - 1, src.height() - 1);
    ImageBuffer::from_fn(w, h, |i, j| {
        let (u, v) = map((i as f64 + 0.5) / w as f64, (j as f64 + 0.5) / h as f64);
        let (x, y) = ((u * sw - 0.5).clamp(0.0, max_x as f64), (v * sh - 0.5).clamp(0.0, max_y as f64));
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(max_x), (y0 + 1).min(max_y));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let (a, b, c, d) =
            (src.get_pixel(x0, y0).0, src.get_pixel(x1, y0).0, src.get_pixel(x0, y1).0, src.get_pixel(x1, y1).0);
        Rgb(std::array::from_fn(|k| {
            (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_round_trips() {
        for o in 1..=8 {
            let (x, y) = orient(o, 0.2, 0.7);
            let back = unorient(o, x, y);
            assert!((back.0 - 0.2).abs() < 1e-12 && (back.1 - 0.7).abs() < 1e-12, "{o}");
        }
    }

    #[test]
    fn a_tilted_crop_maps_its_corners_onto_the_raw() {
        // ภาพ 600×400 ตามแนวที่เก็บ กรอบเอียง 2° · มุมบนซ้ายของ JPEG = มุมบนซ้ายของกรอบที่ Lightroom บอก
        let f = Frame { left: 0.1, top: 0.05, right: 0.9, bottom: 0.95, angle: 2.0 };
        let (x, y) = f.to_raw(0.0, 0.0, (600, 400), 1);
        assert!((x - 0.1).abs() < 1e-9 && (y - 0.05).abs() < 1e-9);
        let (x, y) = f.to_raw(1.0, 1.0, (600, 400), 1);
        assert!((x - 0.9).abs() < 1e-9 && (y - 0.95).abs() < 1e-9, "มุมล่างขวา {x},{y}");
        // ขนาดของกรอบ: เส้นทแยงหมุนกลับ 2°
        let (w, h) = f.output_size((600, 400), 1);
        let (dx, dy) = (480.0, 360.0);
        let (s, c) = 2f64.to_radians().sin_cos();
        assert!((w - (dx * c + dy * s)).abs() < 1e-9 && (h - (-dx * s + dy * c)).abs() < 1e-9);
        // RAW แนวตั้ง (Orientation 6 = ภาพที่เก็บหมุน 90°): JPEG แนวตั้ง มุมบนซ้ายของ JPEG = มุมล่างซ้ายของภาพที่เก็บ
        let full = Frame::FULL;
        assert_eq!(full.output_size((400, 600), 6), (400.0, 600.0));
        let (x, y) = full.to_raw(0.0, 0.0, (400, 600), 6);
        assert!(x.abs() < 1e-9 && y.abs() < 1e-9, "จุดบนภาพที่ตั้งตรงแล้วของ RAW ตรงกับ JPEG");
    }

    const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description
   crs:RawFileName="MOT03115.ARW" crs:WhiteBalance="Custom" crs:Temperature="4550" crs:Tint="+7"
   crs:Exposure2012="+0.25" crs:Highlights2012="-30" crs:Texture="-10" crs:LensProfileEnable="1"
   crs:HasCrop="True" crs:CropTop="0" crs:CropLeft="0.013625" crs:CropBottom="0.998252" crs:CropRight="0.967597"
   crs:CropAngle="1.21"><crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li><rdf:Description crs:CorrectionName="Hair"/>
   </rdf:li></rdf:Seq></crs:MaskGroupBasedCorrections></rdf:Description></rdf:RDF></x:xmpmeta>"#;

    /// JPEG เล็กที่มี EXIF แล้วตามด้วย XMP (แบบที่ Lightroom เขียน)
    pub(crate) fn jpeg_with_xmp(xmp: &str) -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut out)
            .encode(&[128u8; 8 * 8 * 3], 8, 8, image::ExtendedColorType::Rgb8)
            .unwrap();
        let payload = [XMP_MAIN, xmp.as_bytes()].concat();
        let mut seg = vec![0xFF, 0xE1];
        seg.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        seg.extend_from_slice(&payload);
        let exif = [0xFF, 0xE1, 0x00, 0x08, b'E', b'x', b'i', b'f', 0, 0];
        [&out[..2], &exif[..], &seg[..], &out[2..]].concat()
    }

    #[test]
    fn reads_what_lightroom_wrote_in_the_jpeg() {
        let jpeg = jpeg_with_xmp(XMP);
        let xmp = jpeg_xmp(&jpeg).unwrap();
        assert_eq!(raw_file_name(&xmp).as_deref(), Some("MOT03115.ARW"));
        let lr = Lightroom::from_xmp(&xmp).unwrap();
        assert_eq!(lr.frame, Frame { left: 0.013625, top: 0.0, right: 0.967597, bottom: 0.998252, angle: 1.21 });
        assert_eq!(lr.white_balance, Some([4550.0, 7.0]));
        assert_eq!((lr.adjust.exposure, lr.adjust.highlights, lr.adjust.temperature), (0.25, -30.0, 0.0));
        assert_eq!(lr.adjust.texture, -10.0, "Texture ทำตามได้แล้ว (ข้อ 27)");
        assert_eq!(lr.unsupported, ["แก้เลนส์ (Lens Corrections)", "Mask “Hair”"]);
        // ไม่ได้ตัด = ทั้งภาพ · JPEG ที่ไม่มี XMP
        assert_eq!(Frame::from_xmp(r#"crs:HasCrop="False" crs:CropLeft="0.2""#), Frame::FULL);
        assert_eq!(jpeg_xmp(&jpeg_with_xmp("")[..0]), None);
        assert!(raw_file_name(r#"crs:RawFileName="MOT03115.jpg""#).is_none(), "แต่ง JPEG ใน Lightroom ไม่ใช่ RAW");
    }

    #[test]
    fn white_balance_follows_lightroom_relative_to_the_camera() {
        // แสงหลอดไส้ (A = วัตถุดำ 2856 K)
        let a = kelvin_xy(2856.0, 0.0);
        assert!((a[0] - 0.4476).abs() < 0.001 && (a[1] - 0.4074).abs() < 0.001, "{a:?}");
        let shot = kelvin_xy(4000.0, 0.0);
        let (t, s) = white_balance_sliders(shot, [4000.0, 0.0]);
        assert!(t.abs() < 0.5 && s.abs() < 0.5, "เท่ากล้อง = ไม่ปรับ ({t}, {s})");
        let (warmer, _) = white_balance_sliders(shot, [4600.0, 0.0]);
        let (cooler, _) = white_balance_sliders(shot, [3500.0, 0.0]);
        assert!(warmer > 5.0 && cooler < -5.0, "Kelvin สูงกว่ากล้อง = อุ่นขึ้น ({warmer}, {cooler})");
        let (_, magenta) = white_balance_sliders(shot, [4000.0, 20.0]);
        assert!(magenta > 5.0, "Tint บวก = อมม่วงแดง ({magenta})");
    }

    #[test]
    fn pairs_by_xmp_anywhere_or_by_name_in_the_same_folder() {
        let dir = std::env::temp_dir().join(format!("puri-pair-{}", std::process::id()));
        let raw_dir = dir.join("sony_raw");
        std::fs::create_dir_all(&raw_dir).unwrap();
        let exported = dir.join("MOT03115.jpg");
        std::fs::write(&exported, jpeg_with_xmp(XMP)).unwrap();
        let camera = dir.join("DSC00001.JPG");
        std::fs::write(&camera, jpeg_with_xmp("<x:xmpmeta/>")).unwrap();
        let alone = dir.join("DSC00002.jpg");
        std::fs::write(&alone, jpeg_with_xmp("<x:xmpmeta/>")).unwrap();
        let raws = vec![raw_dir.join("MOT03115.ARW"), dir.join("DSC00001.ARW"), raw_dir.join("DSC00002.ARW")];
        let found = match_files(&[exported, camera, alone], &raws);
        assert_eq!(found[0].as_ref().map(|m| m.raw), Some(0), "Lightroom บอกชื่อไว้ อยู่โฟลเดอร์ย่อยก็จับคู่");
        assert!(found[0].as_ref().unwrap().lightroom.is_some());
        assert_eq!(found[1].as_ref().map(|m| m.raw), Some(1), "ชื่อเดียวกันในโฟลเดอร์เดียวกัน");
        assert!(found[1].as_ref().unwrap().lightroom.is_none());
        assert!(found[2].is_none(), "ชื่อเดียวกันแต่ต่างโฟลเดอร์และไม่มี XMP = ไม่จับคู่");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
