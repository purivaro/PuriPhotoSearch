//! อ่านไฟล์ภาพ: ชนิดที่รองรับ, EXIF, การหมุน, ภาพย่อ และครอปใบหน้า (Blueprint 9.5)
//! โมดูลนี้อ่านไฟล์ต้นฉบับอย่างเดียว ไม่มีคำสั่งเขียนลงตำแหน่งต้นฉบับ

use crate::error::{AppError, Result};
use image::{DynamicImage, ImageReader};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;

/// ชนิดไฟล์ที่ build นี้ถอดรหัสได้จริง
pub const SUPPORTED_EXTS: &[&str] = &["jpg", "jpeg", "png"];
/// ชนิดไฟล์ภาพที่รู้จักแต่ยังไม่รองรับ — รายงานให้ผู้ใช้เห็น ไม่ทำให้หายเงียบ ๆ
const KNOWN_IMAGE_EXTS: &[&str] = &[
    "heic", "heif", "tif", "tiff", "webp", "bmp", "gif", "avif", "arw", "cr2", "cr3", "nef", "nrw", "orf", "raf",
    "rw2", "pef", "srw", "dng", "raw",
];

pub const THUMB_EDGE: u32 = 512;
/// เพดานขนาดไฟล์และจำนวนพิกเซล กันไฟล์ผิดปกติทำ RAM เต็ม (Blueprint 9.3)
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 250_000_000;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FileKind {
    Supported,
    UnsupportedImage,
    Other,
}

pub fn classify(path: &Path) -> (FileKind, String) {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let kind = if SUPPORTED_EXTS.contains(&ext.as_str()) {
        FileKind::Supported
    } else if KNOWN_IMAGE_EXTS.contains(&ext.as_str()) {
        FileKind::UnsupportedImage
    } else {
        FileKind::Other
    };
    (kind, ext)
}

pub fn mime_for(path: &Path) -> &'static str {
    match classify(path).1.as_str() {
        "png" => "image/png",
        _ => "image/jpeg",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    pub mtime_ns: i64,
}

pub fn stamp(path: &Path) -> std::io::Result<FileStamp> {
    let md = std::fs::metadata(path)?;
    let mtime_ns = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    Ok(FileStamp { size: md.len(), mtime_ns })
}

#[derive(Debug, Default, Clone)]
pub struct ExifInfo {
    pub orientation: u8,
    /// เวลาท้องถิ่นตาม EXIF `YYYY-MM-DDTHH:MM:SS`; ไม่เติม timezone เอง (Blueprint 2.3)
    pub taken_at_local: Option<String>,
    pub tz_offset: Option<String>,
}

pub fn read_exif(bytes: &[u8]) -> ExifInfo {
    let mut info = ExifInfo { orientation: 1, ..Default::default() };
    let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) else {
        return info;
    };
    if let Some(f) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        if let Some(v) = f.value.get_uint(0) {
            if (1..=8).contains(&v) {
                info.orientation = v as u8;
            }
        }
    }
    let ascii = |tag: exif::Tag| -> Option<String> {
        match &exif.get_field(tag, exif::In::PRIMARY)?.value {
            exif::Value::Ascii(v) => v.first().map(|b| String::from_utf8_lossy(b).trim().to_string()),
            _ => None,
        }
    };
    info.taken_at_local = ascii(exif::Tag::DateTimeOriginal).and_then(|s| parse_exif_datetime(&s));
    info.tz_offset = ascii(exif::Tag::OffsetTimeOriginal).filter(|s| s.len() == 6);
    info
}

/// `2026:11:18 10:20:30` → `2026-11-18T10:20:30`; ค่าที่ไม่ใช่วันเวลาจริง (เช่น 0000:00:00) ถือว่าไม่ทราบ
fn parse_exif_datetime(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<u32>().ok();
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    let date = format!("{y:04}-{mo:02}-{d:02}");
    if y < 1900 || !crate::util::is_valid_date(&date) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    Some(format!("{date}T{h:02}:{mi:02}:{se:02}"))
}

fn decode_error(e: impl std::fmt::Display) -> AppError {
    AppError::new("DECODE_FAILED", format!("เปิดไฟล์รูปนี้ไม่ได้ ไฟล์อาจเสียหาย ({e})"))
}

pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let md = std::fs::metadata(path)?;
    if md.len() > MAX_FILE_BYTES {
        return Err(AppError::new("DECODE_FAILED", "ไฟล์รูปใหญ่เกินกว่าที่แอปรองรับ"));
    }
    Ok(std::fs::read(path)?)
}

pub fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(decode_error)?;
    if reader.format().is_none() {
        return Err(AppError::new("UNSUPPORTED_FORMAT", "ชนิดไฟล์ภาพนี้ยังไม่รองรับในรุ่นนี้"));
    }
    let (w, h) = reader.into_dimensions().map_err(decode_error)?;
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(AppError::new("DECODE_FAILED", "รูปมีจำนวนพิกเซลมากเกินกว่าที่แอปรองรับ"));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(decode_error)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(2 * 1024 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(decode_error)
}

fn orient(img: &mut DynamicImage, orientation: u8) {
    if let Some(o) = image::metadata::Orientation::from_exif(orientation) {
        img.apply_orientation(o);
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode_image(&rgb).map_err(decode_error)?;
    Ok(out)
}

pub struct Processed {
    pub sha256: String,
    /// ขนาดภาพหลังหมุนตาม EXIF แล้ว
    pub width: u32,
    pub height: u32,
    pub exif: ExifInfo,
    pub thumbnail_jpeg: Vec<u8>,
}

/// ถอดรหัส JPEG แบบย่อระหว่างถอด (DCT scaling): ได้ภาพที่ด้านยาว ≥ `min_edge` โดยไม่ต้องถอดทุกพิกเซล
/// คืน (ภาพที่ย่อแล้ว, ขนาดเต็มก่อนหมุน); คืน None เมื่อไฟล์ไม่ใช่ JPEG แบบที่ทางลัดนี้รองรับ ให้ผู้เรียกใช้ทางปกติ
fn decode_jpeg_scaled(bytes: &[u8], min_edge: u32) -> Option<(DynamicImage, (u32, u32))> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    decoder.read_info().ok()?;
    let full = decoder.info()?;
    if (full.width as u64) * (full.height as u64) > MAX_PIXELS {
        return None;
    }
    // ตัวถอดรหัสเลือกสเกลเล็กสุดจาก 1/8, 1/4, 1/2, 1/1 ที่ด้านใดด้านหนึ่งถึงค่าที่ขอ — ขอค่าเดียวกันสองด้าน
    // จึงเท่ากับกำหนดด้านยาว
    let want = min_edge.min(u16::MAX as u32) as u16;
    decoder.scale(want.min(full.width), want.min(full.height)).ok()?;
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (info.width as u32, info.height as u32);
    let img = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, pixels)?),
        jpeg_decoder::PixelFormat::L8 => DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, pixels)?),
        _ => return None,
    };
    Some((img, (full.width as u32, full.height as u32)))
}

/// ขนาดเต็ม (ก่อนหมุน) ของไฟล์ JPEG จาก header; None เมื่อไม่ใช่ JPEG
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    decoder.read_info().ok()?;
    decoder.info().map(|i| (i.width as u32, i.height as u32))
}

/// ขั้น "เตรียมข้อมูลรูป": hash เนื้อหา, EXIF, ขนาด และภาพย่อที่หมุนถูกทิศ
pub fn process(bytes: &[u8]) -> Result<Processed> {
    let sha256 = crate::util::hex(&Sha256::digest(bytes));
    let exif = read_exif(bytes);
    let (img, (raw_w, raw_h)) = match decode_jpeg_scaled(bytes, THUMB_EDGE) {
        Some(scaled) => scaled,
        None => {
            let img = decode(bytes)?;
            let dims = (img.width(), img.height());
            (img, dims)
        }
    };
    let (width, height) = if exif.orientation >= 5 { (raw_h, raw_w) } else { (raw_w, raw_h) };
    // ย่อก่อนแล้วค่อยหมุน: ถูกกว่าการหมุนภาพเต็มหลายสิบล้านพิกเซล
    let mut thumb = img.thumbnail(THUMB_EDGE, THUMB_EDGE);
    drop(img);
    orient(&mut thumb, exif.orientation);
    Ok(Processed { sha256, width, height, exif, thumbnail_jpeg: encode_jpeg(&thumb, 82)? })
}

pub fn thumbnail_only(bytes: &[u8], edge: u32) -> Result<Vec<u8>> {
    let exif = read_exif(bytes);
    let mut thumb = decode(bytes)?.thumbnail(edge, edge);
    orient(&mut thumb, exif.orientation);
    encode_jpeg(&thumb, 80)
}

/// ด้านของภาพครอปใบหน้าที่เก็บใน cache
const FACE_CROP_EDGE: u32 = 256;
/// ครอปที่ได้อย่างน้อยเท่านี้จากการถอดแบบย่อครึ่งหนึ่งถือว่าคมพอ ไม่ต้องถอดรหัสรูปเต็มทุกพิกเซลเพื่อใบหน้าเล็ก
const FACE_CROP_ENOUGH: f64 = 96.0;

/// กรอบครอปของใบหน้า: สี่เหลี่ยมจัตุรัสรอบกรอบใบหน้าพร้อมขอบ 25% เพื่อให้เห็นบริบทของหน้า
/// คืน (ซ้าย, บน, ด้าน) เป็นพิกเซลของภาพขนาด `iw`×`ih`
fn crop_square(bbox: (f64, f64, f64, f64), iw: f64, ih: f64) -> (f64, f64, f64) {
    let (x, y, w, h) = bbox;
    let side = ((w * iw).max(h * ih) * 1.5).min(iw).min(ih).max(1.0);
    let (cx, cy) = ((x + w / 2.0) * iw, (y + h / 2.0) * ih);
    ((cx - side / 2.0).clamp(0.0, iw - side), (cy - side / 2.0).clamp(0.0, ih - side), side)
}

/// ครอปใบหน้าหลายใบจากการถอดรหัสรูปครั้งเดียว; `bboxes` เป็นสัดส่วนบนภาพที่หมุนตาม EXIF แล้ว
/// (transform เดียวกับตัวดูภาพ) `already_upright` = true เมื่อ `bytes` เป็นภาพย่อของแอปซึ่งหมุนไว้แล้ว
///
/// JPEG ถูกถอดแบบย่อระหว่างถอดเท่าที่ใบหน้าที่เล็กที่สุดต้องการ: ใบหน้าใหญ่ไม่ต้องถอดทุกพิกเซลของรูปหลายสิบล้านพิกเซล
pub fn face_crops(bytes: &[u8], bboxes: &[(f64, f64, f64, f64)], already_upright: bool) -> Result<Vec<Vec<u8>>> {
    if bboxes.is_empty() {
        return Ok(vec![]);
    }
    let orientation = if already_upright { 1 } else { read_exif(bytes).orientation };
    let scaled = jpeg_dimensions(bytes).and_then(|(raw_w, raw_h)| {
        let (fw, fh) = if orientation >= 5 { (raw_h, raw_w) } else { (raw_w, raw_h) };
        let smallest = bboxes.iter().map(|b| crop_square(*b, fw as f64, fh as f64).2).fold(f64::MAX, f64::min).max(1.0);
        // ย่อได้เท่าที่ครอปของใบเล็กสุดยังได้ FACE_CROP_EDGE; ถ้าต้องถอดเต็มแต่ครึ่งหนึ่งก็คมพอ ใช้ครึ่งหนึ่ง
        let wanted = (FACE_CROP_EDGE as f64 / smallest).min(1.0);
        let ratio = if wanted > 0.5 && smallest * 0.5 >= FACE_CROP_ENOUGH { 0.5 } else { wanted };
        let long_edge = (raw_w.max(raw_h) as f64 * ratio).ceil() as u32;
        decode_jpeg_scaled(bytes, long_edge.max(1))
    });
    let mut img = match scaled {
        Some((img, _)) => img,
        None => decode(bytes)?,
    };
    orient(&mut img, orientation);
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    bboxes
        .iter()
        .map(|bbox| {
            let (left, top, side) = crop_square(*bbox, iw, ih);
            let crop = img.crop_imm(left as u32, top as u32, side as u32, side as u32);
            encode_jpeg(&crop.thumbnail(FACE_CROP_EDGE, FACE_CROP_EDGE), 85)
        })
        .collect()
}

pub fn face_crop(bytes: &[u8], bbox: (f64, f64, f64, f64), already_upright: bool) -> Result<Vec<u8>> {
    Ok(face_crops(bytes, &[bbox], already_upright)?.remove(0))
}

/// เขียนไฟล์ cache แบบ atomic (เขียนไฟล์ชั่วคราวแล้ว rename) — ใช้กับโฟลเดอร์ cache ของแอปเท่านั้น
pub fn write_cache_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", crate::util::new_id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ───────────────────────── ย่อรูปตอนส่งออก ─────────────────────────

/// คุณภาพ JPEG ของสำเนาที่ย่อ: สูงพอที่ดูด้วยตาแยกจากต้นฉบับไม่ออก
const EXPORT_QUALITY: u8 = 90;
/// คุณภาพสูงสุดเมื่อไล่ใช้งบขนาดไฟล์ที่เหลือให้คุ้ม
const EXPORT_QUALITY_MAX: u8 = 95;
/// ด้านยาวเล็กสุดที่ยอมย่อลงไปเพื่อให้ได้ขนาดไฟล์ตามที่ขอ ต่ำกว่านี้รูปใช้งานไม่ได้แล้ว
const MIN_FIT_EDGE: u32 = 200;
/// ไบต์ต่อพิกเซลโดยเฉลี่ยของรูปถ่ายเมื่อบันทึกเป็น JPEG คุณภาพ 90 (วัดจากรูปกิจกรรมจริง 12 รูป: 0.19–0.25)
pub const BYTES_PER_PIXEL_GUESS: f64 = 0.22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// ด้านยาวไม่เกินกี่พิกเซล (ไม่ขยายรูปที่เล็กกว่า)
    LongEdge(u32),
    /// ขนาดไฟล์ไม่เกินกี่ไบต์
    MaxBytes(u64),
}

/// สำเนา JPEG ที่ย่อแล้วจาก bytes ของต้นฉบับ: หมุนให้ตั้งตรงแล้ว และเก็บ EXIF (วันถ่าย กล้อง ฯลฯ)
/// กับโปรไฟล์สีของต้นฉบับไว้ เพื่อให้แอปรูปภาพอื่นเรียงวันที่และแสดงสีถูก
///
/// ขนาดไฟล์ (แนวทางเดียวกับ PuriSlim): ย่อขนาดภาพโดยคงคุณภาพสูงไว้ แล้วจึงไล่เพิ่มคุณภาพให้เต็มงบ
/// — ให้รูปคมกว่าการกดคุณภาพ JPEG ลงอย่างเดียว
pub fn export_jpeg(bytes: &[u8], fit: Fit) -> Result<Vec<u8>> {
    let orientation = read_exif(bytes).orientation;
    let mut meta = metadata_segments(bytes);
    match fit {
        Fit::LongEdge(edge) => {
            let base = decode_for_export(bytes, edge)?;
            let img = render(&base, edge, orientation);
            Ok(splice_metadata(&encode_jpeg(&img, EXPORT_QUALITY)?, &meta))
        }
        Fit::MaxBytes(max) => {
            // EXIF ที่ใหญ่ผิดปกติ (เช่น maker note ยาว) กินงบเกินควรเมื่อขอไฟล์เล็กมาก: ยอมตัดข้อมูลแทนการย่อภาพจนเสีย
            let mut meta_len: u64 = meta.iter().map(|s| s.len() as u64).sum();
            if meta_len > max / 4 {
                meta.clear();
                meta_len = 0;
            }
            let budget = (max - meta_len) as usize;
            let (full_w, full_h) = match jpeg_dimensions(bytes) {
                Some(dims) => dims,
                None => ImageReader::new(Cursor::new(bytes))
                    .with_guessed_format()
                    .map_err(decode_error)?
                    .into_dimensions()
                    .map_err(decode_error)?,
            };
            let full_long = full_w.max(full_h);
            // เดาจำนวนพิกเซลแรกจากอัตราไบต์ต่อพิกเซลของ JPEG คุณภาพ 90 กับรูปถ่ายทั่วไป
            // (เดาจากขนาดไฟล์ต้นฉบับไม่ได้ เพราะกล้องแต่ละรุ่นบีบอัดต่างกันหลายเท่า)
            let full_px = full_w as f64 * full_h as f64;
            let ratio = (budget as f64 / BYTES_PER_PIXEL_GUESS / full_px.max(1.0)).sqrt().min(1.0);
            let mut target = ((full_long as f64 * ratio) as u32).clamp(MIN_FIT_EDGE.min(full_long), full_long);
            // ถอดเผื่อไว้ครึ่งเท่าของค่าที่เดา เผื่อขยายขึ้นได้เมื่อเดาต่ำไป (ไม่ถอดเต็มทุกพิกเซลโดยไม่จำเป็น)
            let base = decode_for_export(bytes, (target.saturating_mul(3) / 2).min(full_long))?;
            let base_long = base.width().max(base.height());
            target = target.min(base_long);

            // หาด้านยาวที่ใหญ่ที่สุดที่ยังไม่เกินงบ: เดาจากสัดส่วนขนาดไฟล์ ถ้าค่าที่เดาหลุดช่วง
            // [ด้านที่พอดี, ด้านที่เกินงบ] ให้แบ่งครึ่งช่วงแทน
            let mut best: Option<(u32, DynamicImage, Vec<u8>)> = None;
            let mut too_big = base_long + 1;
            for _ in 0..8 {
                let img = render(&base, target, orientation);
                let out = encode_jpeg(&img, EXPORT_QUALITY)?;
                let len = out.len();
                if len <= budget {
                    best = Some((target, img, out));
                } else {
                    too_big = target;
                }
                let fits_at = best.as_ref().map_or(0, |b| b.0);
                // ใช้งบเกือบเต็มแล้ว หรือช่วงที่เหลือแคบกว่า 5%: พอแล้ว
                if (len <= budget && len * 100 >= budget * 85)
                    || (fits_at > 0 && too_big.saturating_sub(fits_at) <= fits_at / 20)
                {
                    break;
                }
                let guess = (target as f64 * (budget as f64 / len as f64).sqrt() * 0.97) as u32;
                let next = if guess > fits_at && guess < too_big { guess } else { (fits_at + too_big) / 2 };
                let next = next.min(base_long);
                if next <= fits_at || next >= too_big {
                    break;
                }
                if best.is_none() && next < MIN_FIT_EDGE {
                    return Err(AppError::invalid("ขนาดไฟล์ที่ตั้งไว้เล็กเกินไปสำหรับรูปนี้ ลองตั้งให้ใหญ่ขึ้น"));
                }
                target = next;
            }
            let (_, img, mut out) =
                best.ok_or_else(|| AppError::invalid("ย่อรูปนี้ให้ได้ขนาดไฟล์ตามที่ตั้งไว้ไม่สำเร็จ ลองตั้งให้ใหญ่ขึ้น"))?;
            // ได้ขนาดภาพแล้ว: หาคุณภาพสูงสุดที่ยังไม่เกินงบ
            let (mut lo, mut hi) = (EXPORT_QUALITY + 1, EXPORT_QUALITY_MAX);
            while lo <= hi {
                let mid = (lo + hi) / 2;
                let candidate = encode_jpeg(&img, mid)?;
                if candidate.len() <= budget {
                    out = candidate;
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            }
            Ok(splice_metadata(&out, &meta))
        }
    }
}

/// ถอดรหัสให้ได้ภาพที่ด้านยาวอย่างน้อย `edge` (JPEG ใช้การย่อระหว่างถอด ไม่ต้องถอดทุกพิกเซล)
fn decode_for_export(bytes: &[u8], edge: u32) -> Result<DynamicImage> {
    match decode_jpeg_scaled(bytes, edge) {
        Some((img, _)) => Ok(img),
        None => decode(bytes),
    }
}

/// ย่อให้ด้านยาวไม่เกิน `edge` (ไม่ขยาย) หมุนให้ตั้งตรง และปูพื้นขาวใต้ส่วนโปร่งใส (JPEG ไม่มีความโปร่งใส)
fn render(base: &DynamicImage, edge: u32, orientation: u8) -> DynamicImage {
    let mut img = if base.width().max(base.height()) > edge {
        base.resize(edge, edge, image::imageops::FilterType::Lanczos3)
    } else {
        base.clone()
    };
    orient(&mut img, orientation);
    if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let flat = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            let mix = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
            image::Rgb([mix(r), mix(g), mix(b)])
        });
        img = DynamicImage::ImageRgb8(flat);
    }
    img
}

/// segment EXIF (APP1 "Exif") และโปรไฟล์สี (APP2 "ICC_PROFILE") จากหัวไฟล์ JPEG ต้นฉบับ
/// EXIF ถูกแก้ Orientation เป็น 1 เพราะภาพที่เขียนใหม่หมุนตั้งตรงแล้ว (ไม่งั้นแอปอื่นจะหมุนซ้ำ)
fn metadata_segments(src: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if !src.starts_with(&[0xFF, 0xD8]) {
        return out;
    }
    let (mut i, mut has_exif) = (2usize, false);
    while i + 4 <= src.len() && src[i] == 0xFF {
        let marker = src[i + 1];
        if marker == 0xFF {
            i += 1; // fill byte
            continue;
        }
        // SOS / EOI: หมดส่วนหัวแล้ว
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([src[i + 2], src[i + 3]]) as usize;
        if len < 2 || i + 2 + len > src.len() {
            break;
        }
        let segment = &src[i..i + 2 + len];
        let payload = &segment[4..];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") && !has_exif {
            let mut seg = segment.to_vec();
            set_orientation_upright(&mut seg[10..]);
            out.push(seg);
            has_exif = true;
        } else if marker == 0xE2 && payload.starts_with(b"ICC_PROFILE\0") {
            out.push(segment.to_vec());
        }
        i += 2 + len;
    }
    out
}

/// แก้ค่า Orientation (tag 0x0112) ใน IFD0 ของข้อมูล TIFF/EXIF เป็น 1 ถ้ามี
fn set_orientation_upright(tiff: &mut [u8]) {
    let le = match tiff.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return,
    };
    let u16_at = |b: &[u8], o: usize| -> Option<u16> {
        let v = [*b.get(o)?, *b.get(o + 1)?];
        Some(if le { u16::from_le_bytes(v) } else { u16::from_be_bytes(v) })
    };
    let Some(ifd) = tiff.get(4..8).map(|v| {
        let v = [v[0], v[1], v[2], v[3]];
        (if le { u32::from_le_bytes(v) } else { u32::from_be_bytes(v) }) as usize
    }) else {
        return;
    };
    let Some(count) = u16_at(tiff, ifd) else { return };
    for k in 0..count as usize {
        let entry = ifd + 2 + 12 * k;
        if entry + 12 > tiff.len() {
            return;
        }
        // tag 0x0112 ชนิด SHORT (3) จำนวน 1: ค่าอยู่ใน 2 ไบต์แรกของช่องค่า
        if u16_at(tiff, entry) == Some(0x0112) && u16_at(tiff, entry + 2) == Some(3) {
            let one = if le { 1u16.to_le_bytes() } else { 1u16.to_be_bytes() };
            tiff[entry + 8..entry + 10].copy_from_slice(&one);
            return;
        }
    }
}

/// วาง segment ของต้นฉบับต่อจาก SOI ของไฟล์ที่เข้ารหัสใหม่ (ตัด JFIF APP0 ของตัวเข้ารหัสออก
/// เพราะมาตรฐาน EXIF ให้ APP1 ตามหลัง SOI ทันที)
fn splice_metadata(encoded: &[u8], segments: &[Vec<u8>]) -> Vec<u8> {
    if segments.is_empty() || !encoded.starts_with(&[0xFF, 0xD8]) {
        return encoded.to_vec();
    }
    let mut rest = 2;
    if encoded.len() > 6 && encoded[2] == 0xFF && encoded[3] == 0xE0 {
        rest += 2 + u16::from_be_bytes([encoded[4], encoded[5]]) as usize;
    }
    let mut out = Vec::with_capacity(encoded.len() + segments.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&encoded[..2]);
    for s in segments {
        out.extend_from_slice(s);
    }
    out.extend_from_slice(&encoded[rest.min(encoded.len())..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JPEG ขนาด w×h ที่มี EXIF (Orientation + DateTimeOriginal) และ ICC segment ปลอม
    fn camera_jpeg(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        // ลายละเอียดสุ่มเทียม ให้ขนาดไฟล์ใกล้รูปจริงกว่าสีเรียบ
        let mut seed = 12345u32;
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let n = (seed >> 24) as u8 / 4;
            image::Rgb([(x * 255 / w) as u8 ^ n, (y * 255 / h) as u8, n.wrapping_mul(3)])
        });
        let plain = encode_jpeg(&DynamicImage::ImageRgb8(img), 96).unwrap();
        let mut tiff: Vec<u8> = b"II*\0".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        let entry = |tag: u16, typ: u16, count: u32, value: u32| {
            [tag.to_le_bytes().as_slice(), &typ.to_le_bytes(), &count.to_le_bytes(), &value.to_le_bytes()].concat()
        };
        tiff.extend(entry(0x0112, 3, 1, orientation as u32));
        tiff.extend(entry(0x8769, 4, 1, 38));
        tiff.extend_from_slice(&0u32.to_le_bytes());
        // Exif IFD: DateTimeOriginal
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend(entry(0x9003, 2, 20, 56));
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(b"2026:11:18 10:20:30\0");
        let app1 = [b"Exif\0\0".as_slice(), &tiff].concat();
        let icc = [b"ICC_PROFILE\0\x01\x01".as_slice(), &[7u8; 64]].concat();
        let mut out = plain[..2].to_vec();
        for (marker, payload) in [(0xE1u8, &app1), (0xE2, &icc)] {
            out.extend_from_slice(&[0xFF, marker]);
            out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            out.extend_from_slice(payload);
        }
        out.extend_from_slice(&plain[2..]);
        out
    }

    #[test]
    fn export_by_long_edge_keeps_date_and_colour_profile_and_stands_upright() {
        // ต้นฉบับ 1200×800 ที่กล้องบอกให้หมุน 90° (orientation 6): ภาพจริงเป็นแนวตั้ง 800×1200
        let src = camera_jpeg(1200, 800, 6);
        let out = export_jpeg(&src, Fit::LongEdge(600)).unwrap();
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (400, 600), "ด้านยาวตามที่ขอ และหมุนตั้งตรงแล้ว");
        let exif = read_exif(&out);
        assert_eq!(exif.orientation, 1, "ไม่ให้แอปอื่นหมุนซ้ำ");
        assert_eq!(exif.taken_at_local.as_deref(), Some("2026-11-18T10:20:30"), "เก็บวันถ่ายไว้");
        let segments = metadata_segments(&out);
        assert!(segments.iter().any(|s| s[4..].starts_with(b"ICC_PROFILE\0")), "เก็บโปรไฟล์สีไว้");
        assert!(out.len() < src.len());

        // รูปที่เล็กกว่าค่าที่ขอไม่ถูกขยาย
        let small = export_jpeg(&src, Fit::LongEdge(5000)).unwrap();
        let img = image::load_from_memory(&small).unwrap();
        assert_eq!((img.width(), img.height()), (800, 1200));
    }

    #[test]
    fn export_by_file_size_fits_the_budget_and_uses_it() {
        let src = camera_jpeg(2400, 1600, 1);
        for max in [150_000u64, 400_000] {
            let out = export_jpeg(&src, Fit::MaxBytes(max)).unwrap();
            assert!(out.len() as u64 <= max, "{} > {max}", out.len());
            assert!(out.len() as u64 > max / 2, "ใช้งบให้คุ้ม ไม่ย่อจนเล็กเกินจำเป็น: {} จาก {max}", out.len());
            assert_eq!(read_exif(&out).taken_at_local.as_deref(), Some("2026-11-18T10:20:30"));
        }
        let tiny = export_jpeg(&src, Fit::MaxBytes(2_000));
        assert_eq!(tiny.unwrap_err().code, "INVALID_INPUT", "ตั้งเล็กจนรูปใช้ไม่ได้: แจ้งผู้ใช้ ไม่สร้างไฟล์จิ๋ว");
    }

    #[test]
    fn exif_datetime_rules() {
        assert_eq!(parse_exif_datetime("2026:11:18 10:20:30").as_deref(), Some("2026-11-18T10:20:30"));
        assert_eq!(parse_exif_datetime("0000:00:00 00:00:00"), None);
        assert_eq!(parse_exif_datetime("2026:02:30 10:20:30"), None);
        assert_eq!(parse_exif_datetime("garbage"), None);
    }

    /// ครอปจากการถอดแบบย่อต้องได้บริเวณเดียวกับการถอดเต็ม (ใบหน้าใหญ่ = ย่อ 1/4, ใบหน้าเล็ก = ถอดเต็ม)
    #[test]
    fn face_crops_pick_the_same_region_at_any_decode_scale() {
        // ครึ่งซ้ายแดง ครึ่งขวาน้ำเงิน มีจุดเขียว 40×40 ที่ (1500, 300)
        let img = image::RgbImage::from_fn(2048, 1536, |x, y| {
            if (1500..1540).contains(&x) && (300..340).contains(&y) {
                image::Rgb([0, 255, 0])
            } else if x < 1024 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        let bytes = encode_jpeg(&DynamicImage::ImageRgb8(img), 90).unwrap();
        let big = (0.05, 0.2, 0.3, 0.5); // ใบหน้าใหญ่ในครึ่งซ้าย
        let dot = (1500.0 / 2048.0, 300.0 / 1536.0, 40.0 / 2048.0, 40.0 / 1536.0);
        let center = |jpeg: &[u8]| {
            let c = image::load_from_memory(jpeg).unwrap().to_rgb8();
            assert_eq!((c.width(), c.height()), (FACE_CROP_EDGE, FACE_CROP_EDGE));
            let p = c.get_pixel(c.width() / 2, c.height() / 2).0;
            (p[0] > 200, p[1] > 200, p[2] > 200)
        };
        assert_eq!(center(&face_crops(&bytes, &[big], false).unwrap()[0]), (true, false, false));
        let both = face_crops(&bytes, &[big, dot], false).unwrap();
        assert_eq!((center(&both[0]), center(&both[1])), ((true, false, false), (false, true, false)));
        let widths = [300, 512, 800, 1536].map(|edge| decode_jpeg_scaled(&bytes, edge).unwrap().0.width());
        assert_eq!(widths, [512, 512, 1024, 2048], "ทางลัดย่อระหว่างถอดถูกใช้จริง");
    }

    #[test]
    fn classify_extensions() {
        assert_eq!(classify(Path::new("a/IMG_1.JPG")).0, FileKind::Supported);
        assert_eq!(classify(Path::new("a/x.heic")).0, FileKind::UnsupportedImage);
        assert_eq!(classify(Path::new("a/x.mp4")).0, FileKind::Other);
        assert_eq!(classify(Path::new("a/noext")).0, FileKind::Other);
    }
}
