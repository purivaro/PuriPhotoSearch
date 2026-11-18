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
    // ขอขนาดที่ด้านสั้นไม่เล็กกว่า min_edge ตัวถอดรหัสจะเลือกสเกล 1/1, 1/2, 1/4 หรือ 1/8 ที่ยังใหญ่พอ
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

/// ครอปใบหน้าจากภาพ; `bbox` เป็นสัดส่วนบนภาพที่หมุนตาม EXIF แล้ว (transform เดียวกับตัวดูภาพ)
/// `already_upright` = true เมื่อ `bytes` เป็นภาพย่อของแอปซึ่งหมุนไว้แล้ว
pub fn face_crop(bytes: &[u8], bbox: (f64, f64, f64, f64), already_upright: bool) -> Result<Vec<u8>> {
    let mut img = decode(bytes)?;
    if !already_upright {
        orient(&mut img, read_exif(bytes).orientation);
    }
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let (x, y, w, h) = bbox;
    // ขยายกรอบเป็นสี่เหลี่ยมจัตุรัสพร้อมขอบ 25% เพื่อให้เห็นบริบทของหน้า
    let side = (w * iw).max(h * ih) * 1.5;
    let (cx, cy) = ((x + w / 2.0) * iw, (y + h / 2.0) * ih);
    let side = side.min(iw).min(ih).max(1.0);
    let left = (cx - side / 2.0).clamp(0.0, iw - side);
    let top = (cy - side / 2.0).clamp(0.0, ih - side);
    let crop = img.crop_imm(left as u32, top as u32, side as u32, side as u32).thumbnail(256, 256);
    encode_jpeg(&crop, 85)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_datetime_rules() {
        assert_eq!(parse_exif_datetime("2026:11:18 10:20:30").as_deref(), Some("2026-11-18T10:20:30"));
        assert_eq!(parse_exif_datetime("0000:00:00 00:00:00"), None);
        assert_eq!(parse_exif_datetime("2026:02:30 10:20:30"), None);
        assert_eq!(parse_exif_datetime("garbage"), None);
    }

    #[test]
    fn classify_extensions() {
        assert_eq!(classify(Path::new("a/IMG_1.JPG")).0, FileKind::Supported);
        assert_eq!(classify(Path::new("a/x.heic")).0, FileKind::UnsupportedImage);
        assert_eq!(classify(Path::new("a/x.mp4")).0, FileKind::Other);
        assert_eq!(classify(Path::new("a/noext")).0, FileKind::Other);
    }
}
