//! ไฟล์ RAW จากกล้อง (DECISIONS ข้อ 25): อ่านข้อมูลจากเซนเซอร์ด้วย crate `rawler` แล้ว "ล้างภาพ" เป็นแสงเชิงเส้น
//!
//! rawler ถอดรหัส ลบระดับดำ/หารระดับขาว (0–1 ต่อช่องของเซนเซอร์) ทำ demosaic และตัดขอบตามที่กล้องกำหนด ·
//! ส่วนที่เหลือทำเอง: สมดุลแสงขาวตามที่กล้องบันทึก → เมทริกซ์สีของกล้อง → sRGB เชิงเส้น (D65)
//! **ไม่ตัดค่าที่เกิน 1** (สีสดในที่สว่าง เช่น จีวรกลางแดด มีช่องแดงเกิน 1 หลังคูณสมดุลแสงขาว — เส้นโค้งโทนของ RAW
//! ใน `adjust` ม้วนค่าส่วนนี้ลงอย่างนุ่มนวล) และทำส่วนที่ช่องเขียวเต็มเซนเซอร์ให้เป็นสีกลาง (ไม่งั้นส่วนสว่างที่เต็มเซนเซอร์
//! จะออกชมพูเพราะช่องแดง/น้ำเงินถูกคูณขึ้นแต่เขียวไม่ขึ้นตาม) — rawler ทำส่วนนี้ด้วยการเฉลี่ยเข้าหาสีเทาทุกพิกเซลที่เกิน 1
//! ซึ่งทำให้สีสดในที่สว่างซีด จึงไม่ใช้
//!
//! ภาพย่อ/ตัวดูภาพ/ตัววิเคราะห์ใบหน้าใช้แบบครึ่งขนาด (รวมพิกเซล 2×2 ของ Bayer เป็นหนึ่งพิกเซล — เร็วและไม่ต้อง demosaic)
//! เมื่อขนาดที่ต้องการไม่เกินครึ่งหนึ่งของภาพเต็ม

use crate::error::{AppError, Result};
use crate::media::ExifInfo;
use image::{DynamicImage, ImageBuffer, Rgb};
use rawler::decoders::{RawDecodeParams, RawMetadata};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::xyz::{Illuminant, SRGB_TO_XYZ_D65};
use rawler::imgop::{chromatic_adaption::adapt_bradford, matrix};
use rawler::rawimage::RawPhotometricInterpretation;
use rawler::rawsource::RawSource;
use rawler::RawImage;
use std::path::Path;

/// นามสกุลไฟล์ RAW ที่รองรับ (rawler อ่านได้ทั้งหมด — รุ่นกล้องที่ไม่มีในฐานของ rawler จะเปิดไม่ได้และรายงานเป็นรายไฟล์)
pub const EXTS: &[&str] = &["arw", "cr2", "cr3", "nef", "nrw", "orf", "raf", "rw2", "pef", "srw", "dng"];

/// ค่าเชิงเส้นสูงสุดที่เก็บไว้เหนือระดับขาวของเซนเซอร์ (1 stop) — เกินนี้ถือว่าขาวสุดแล้ว
pub const HEADROOM: f32 = 2.0;

/// เพิ่มความอิ่มสีในแสงเชิงเส้นเป็นส่วนหนึ่งของโทนตั้งต้น: เส้นโค้งโทนต่อช่องอย่างเดียวยังจืดกว่าภาพ JPEG ของกล้อง 6% ·
/// 1.1 เท่าได้ความอิ่มสีเท่ากล้อง (วัดได้ 0.99 เท่า — Sony ILCE-7CM2 30 ไฟล์ + Panasonic DC-S5 49 ไฟล์)
const SATURATION: f32 = 1.1;

pub fn is_raw_ext(ext: &str) -> bool {
    EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

pub fn is_raw_path(path: &Path) -> bool {
    path.extension().is_some_and(|e| is_raw_ext(&e.to_string_lossy()))
}

/// bytes เป็นไฟล์ RAW หรือไม่ ดูจากหัวไฟล์ (TIFF ซึ่ง ARW NEF CR2 DNG PEF SRW ใช้, RW2, ORF, CR3, RAF) — ไม่ถอดรหัส
/// (TIFF ธรรมดาก็ผ่านข้อนี้ แต่แอปไม่รับไฟล์ .tif ตั้งแต่ชื่อไฟล์ — ถ้าหลุดมา ตัวถอดรหัสจะแจ้งว่าเปิดไม่ได้)
pub fn is_raw_bytes(bytes: &[u8]) -> bool {
    let head = |s: &[u8]| bytes.starts_with(s);
    head(b"II*\0")
        || head(b"MM\0*")
        || head(b"IIU\0")
        || head(b"IIRO")
        || head(b"IIRS")
        || head(b"MMOR")
        || head(b"FUJIFILMCCD-RAW")
        || bytes.get(4..12) == Some(b"ftypcrx ".as_slice())
}

fn raw_error(e: impl std::fmt::Display) -> AppError {
    AppError::new("DECODE_FAILED", format!("เปิดไฟล์ RAW นี้ไม่ได้ กล้องรุ่นนี้อาจยังไม่รองรับ หรือไฟล์เสียหาย ({e})"))
}

/// ข้อมูลของไฟล์ RAW ที่ไม่ต้องถอดรหัสภาพ
#[derive(Debug, Clone)]
pub struct RawInfo {
    /// ขนาดภาพเต็มหลังตัดขอบตามกล้อง และหมุนตาม Orientation แล้ว
    pub width: u32,
    pub height: u32,
    pub exif: ExifInfo,
    pub make: String,
    pub model: String,
    pub shot: Shot,
}

/// ข้อมูลการถ่ายที่ใส่ใน EXIF ของสำเนา JPEG ที่ส่งออก
#[derive(Debug, Clone, Default)]
pub struct Shot {
    /// (ตัวเศษ, ตัวส่วน)
    pub exposure_time: Option<(u32, u32)>,
    pub fnumber: Option<(u32, u32)>,
    pub focal_length: Option<(u32, u32)>,
    pub iso: Option<u16>,
    pub lens: Option<String>,
    /// รูปแบบ EXIF `YYYY:MM:DD HH:MM:SS`
    pub date_time_original: Option<String>,
    pub offset_time_original: Option<String>,
}

fn shot_of(md: &RawMetadata) -> Shot {
    let r = |v: &Option<rawler::formats::tiff::Rational>| v.as_ref().filter(|r| r.d > 0).map(|r| (r.n, r.d));
    Shot {
        exposure_time: r(&md.exif.exposure_time),
        fnumber: r(&md.exif.fnumber),
        focal_length: r(&md.exif.focal_length),
        iso: md.exif.iso_speed_ratings,
        lens: md
            .lens
            .as_ref()
            .map(|l| format!("{} {}", l.lens_make, l.lens_model).trim().to_string())
            .filter(|s| !s.is_empty()),
        date_time_original: md.exif.date_time_original.clone().filter(|s| s.len() >= 19),
        offset_time_original: md.exif.offset_time_original.clone().filter(|s| s.len() == 6),
    }
}

impl RawInfo {
    fn new(raw: &RawImage, md: RawMetadata) -> RawInfo {
        let exif = exif_of(&md, orientation_of(raw));
        let (w, h) = cropped_dims(raw);
        let (w, h) = if exif.orientation >= 5 { (h, w) } else { (w, h) };
        let shot = shot_of(&md);
        RawInfo { width: w as u32, height: h as u32, exif, make: md.make, model: md.model, shot }
    }

    /// segment APP1 "Exif" สำหรับ JPEG ที่สร้างจาก RAW: ยี่ห้อ/รุ่นกล้อง Orientation = 1 (ภาพตั้งตรงแล้ว) วันถ่าย
    /// ความเร็วชัตเตอร์ รูรับแสง ISO ทางยาวโฟกัส เลนส์ — ให้แอปรูปภาพอื่นเรียงวันที่ได้
    pub fn exif_app1(&self) -> Vec<u8> {
        let ascii = |s: &str| [s.as_bytes(), b"\0"].concat();
        let rational = |(n, d): (u32, u32)| [n.to_le_bytes(), d.to_le_bytes()].concat();
        let mut ifd0: Vec<(u16, u16, u32, Vec<u8>)> = vec![];
        let mut exif: Vec<(u16, u16, u32, Vec<u8>)> = vec![];
        for (tag, value) in [(0x010F_u16, &self.make), (0x0110, &self.model)] {
            if !value.is_empty() {
                let v = ascii(value);
                ifd0.push((tag, 2, v.len() as u32, v));
            }
        }
        ifd0.push((0x0112, 3, 1, 1u16.to_le_bytes().to_vec()));
        let s = &self.shot;
        if let Some(v) = s.exposure_time {
            exif.push((0x829A, 5, 1, rational(v)));
        }
        if let Some(v) = s.fnumber {
            exif.push((0x829D, 5, 1, rational(v)));
        }
        if let Some(v) = s.iso {
            exif.push((0x8827, 3, 1, v.to_le_bytes().to_vec()));
        }
        if let Some(v) = &s.date_time_original {
            let v = ascii(&v[..19]);
            exif.push((0x9003, 2, v.len() as u32, v));
        }
        if let Some(v) = &s.offset_time_original {
            let v = ascii(v);
            exif.push((0x9011, 2, v.len() as u32, v));
        }
        if let Some(v) = s.focal_length {
            exif.push((0x920A, 5, 1, rational(v)));
        }
        if let Some(v) = &s.lens {
            let v = ascii(v);
            exif.push((0xA434, 2, v.len() as u32, v));
        }
        // ตำแหน่ง: หัว TIFF 8 · IFD0 · Exif IFD · ข้อมูลที่ยาวเกิน 4 ไบต์
        let ifd_len = |n: usize| 2 + 12 * n + 4;
        let ifd0_at = 8usize;
        let exif_at = ifd0_at + ifd_len(ifd0.len() + 1);
        let mut data_at = exif_at + ifd_len(exif.len());
        let mut data: Vec<u8> = vec![];
        let mut write_ifd = |out: &mut Vec<u8>, entries: &[(u16, u16, u32, Vec<u8>)], data: &mut Vec<u8>| {
            out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            for (tag, typ, count, value) in entries {
                out.extend_from_slice(&tag.to_le_bytes());
                out.extend_from_slice(&typ.to_le_bytes());
                out.extend_from_slice(&count.to_le_bytes());
                if value.len() <= 4 {
                    let mut v = value.clone();
                    v.resize(4, 0);
                    out.extend_from_slice(&v);
                } else {
                    out.extend_from_slice(&(data_at as u32).to_le_bytes());
                    data.extend_from_slice(value);
                    data_at += value.len();
                    if value.len() % 2 == 1 {
                        data.push(0);
                        data_at += 1;
                    }
                }
            }
            out.extend_from_slice(&0u32.to_le_bytes());
        };
        ifd0.push((0x8769, 4, 1, (exif_at as u32).to_le_bytes().to_vec()));
        ifd0.sort_by_key(|e| e.0);
        exif.sort_by_key(|e| e.0);
        let mut tiff = b"II*\0".to_vec();
        tiff.extend_from_slice(&(ifd0_at as u32).to_le_bytes());
        write_ifd(&mut tiff, &ifd0, &mut data);
        write_ifd(&mut tiff, &exif, &mut data);
        tiff.extend_from_slice(&data);
        let payload = [b"Exif\0\0".as_slice(), &tiff].concat();
        let mut seg = vec![0xFF, 0xE1];
        seg.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        seg.extend_from_slice(&payload);
        seg
    }
}

/// ภาพที่ล้างแล้ว: sRGB เชิงเส้น ตั้งตรงแล้ว ค่า 0 ถึง [`HEADROOM`]
pub type Linear = ImageBuffer<Rgb<f32>, Vec<f32>>;

fn exif_of(md: &RawMetadata, fallback_orientation: u8) -> ExifInfo {
    let orientation =
        md.exif.orientation.filter(|o| (1..=8).contains(o)).map(|o| o as u8).unwrap_or(fallback_orientation);
    ExifInfo {
        orientation,
        taken_at_local: md.exif.date_time_original.as_deref().and_then(crate::media::parse_exif_datetime),
        tz_offset: md.exif.offset_time_original.clone().filter(|s| s.len() == 6),
    }
}

/// Orientation ของ rawler → เลข EXIF 1–8 (ใช้เมื่อ metadata ไม่มีค่านี้)
fn orientation_of(raw: &RawImage) -> u8 {
    use rawler::Orientation as O;
    match raw.orientation {
        O::Normal | O::Unknown => 1,
        O::HorizontalFlip => 2,
        O::Rotate180 => 3,
        O::VerticalFlip => 4,
        O::Transpose => 5,
        O::Rotate90 => 6,
        O::Transverse => 7,
        O::Rotate270 => 8,
    }
}

/// ขนาดหลังตัดขอบ (ก่อนหมุน)
fn cropped_dims(raw: &RawImage) -> (usize, usize) {
    match raw.crop_area.or(raw.active_area) {
        Some(r) => (r.d.w, r.d.h),
        None => (raw.width, raw.height),
    }
}

/// อ่านเฉพาะข้อมูลของไฟล์ (ขนาด วันถ่าย การหมุน) — ไม่ถอดรหัสข้อมูลภาพ
pub fn info(bytes: &[u8]) -> Result<RawInfo> {
    let src = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&src).map_err(raw_error)?;
    let params = RawDecodeParams::default();
    let raw = decoder.raw_image(&src, &params, true).map_err(raw_error)?;
    let md = decoder.raw_metadata(&src, &params).map_err(raw_error)?;
    Ok(RawInfo::new(&raw, md))
}

/// ล้างภาพ RAW ให้ด้านยาวอย่างน้อย `min_edge` (หรือเต็มขนาดถ้าภาพเล็กกว่า) — คืนภาพเชิงเส้นที่ตั้งตรงแล้ว และข้อมูลของไฟล์
pub fn develop(bytes: &[u8], min_edge: u32) -> Result<(Linear, RawInfo)> {
    let src = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&src).map_err(raw_error)?;
    let params = RawDecodeParams::default();
    let raw = decoder.raw_image(&src, &params, false).map_err(raw_error)?;
    let md = decoder.raw_metadata(&src, &params).map_err(raw_error)?;
    let info = RawInfo::new(&raw, md);
    let (cw, ch) = cropped_dims(&raw);
    let half = (min_edge as usize) * 2 <= cw.max(ch);
    let to_rgb = CameraToRgb::new(&raw);
    let linear_raw = matches!(raw.photometric, RawPhotometricInterpretation::LinearRaw) && raw.cpp == 3;
    let (camera, w, h) = match (half, is_bayer_rgb(&raw), linear_raw) {
        (true, true, _) => superpixel(raw)?,
        (true, _, true) => binned_linear(raw)?,
        _ => full(&raw)?,
    };
    let mut out: Vec<f32> = Vec::with_capacity(camera.len() * 3);
    for c in &camera {
        out.extend_from_slice(&to_rgb.apply(*c));
    }
    let mut img = DynamicImage::ImageRgb32F(
        Linear::from_raw(w as u32, h as u32, out).ok_or_else(|| AppError::internal("ขนาดภาพ RAW ไม่ตรงกับข้อมูล"))?,
    );
    if let Some(o) = image::metadata::Orientation::from_exif(info.exif.orientation) {
        img.apply_orientation(o);
    }
    Ok((img.into_rgb32f(), info))
}

fn is_bayer_rgb(raw: &RawImage) -> bool {
    matches!(&raw.photometric, RawPhotometricInterpretation::Cfa(c)
        if c.cfa.is_rgb() && c.sensor == rawler::imgop::sensor::SensorType::Bayer && raw.cpp == 1)
}

/// demosaic เต็มขนาดด้วย rawler (PPG สำหรับ Bayer) — ค่าในปริภูมิสีของกล้อง ยังไม่ปรับสมดุลแสงขาว
fn full(raw: &RawImage) -> Result<(Vec<[f32; 3]>, usize, usize)> {
    let steps = [
        ProcessingStep::Rescale,
        ProcessingStep::Demosaic,
        ProcessingStep::FujiRotate,
        ProcessingStep::CropActiveArea,
        ProcessingStep::CropDefault,
    ];
    match RawDevelop::new_with(&steps).develop_intermediate(raw).map_err(raw_error)? {
        Intermediate::ThreeColor(px) => Ok((px.data, px.width, px.height)),
        Intermediate::Monochrome(px) => Ok((px.data.iter().map(|v| [*v; 3]).collect(), px.width, px.height)),
        Intermediate::FourColor(_) => Err(raw_error("เซนเซอร์แบบ 4 สียังไม่รองรับ")),
    }
}

/// ครึ่งขนาด: รวมพิกเซล 2×2 ของ Bayer (แดง เขียวสองตัว น้ำเงิน) เป็นหนึ่งพิกเซล ในกรอบที่กล้องกำหนด
fn superpixel(mut raw: RawImage) -> Result<(Vec<[f32; 3]>, usize, usize)> {
    raw.apply_scaling().map_err(raw_error)?;
    let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else {
        return Err(raw_error("ไม่ใช่เซนเซอร์แบบ Bayer"));
    };
    let cfa = config.cfa.clone();
    let data = raw.data.as_f32();
    let stride = raw.width;
    let area = raw.crop_area.or(raw.active_area);
    let (x0, y0, w, h) = match area {
        Some(r) => (r.p.x, r.p.y, r.d.w, r.d.h),
        None => (0, 0, raw.width, raw.height),
    };
    // เริ่มที่พิกัดคู่ ให้แต่ละกลุ่ม 2×2 มีครบทุกสีตามลาย CFA
    let (x0, y0) = (x0 & !1, y0 & !1);
    let (w2, h2) = ((w / 2).min((raw.width - x0) / 2), (h / 2).min((raw.height - y0) / 2));
    let mut out = Vec::with_capacity(w2 * h2);
    for j in 0..h2 {
        for i in 0..w2 {
            let (mut sum, mut n) = ([0.0f32; 3], [0u8; 3]);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (y, x) = (y0 + 2 * j + dy, x0 + 2 * i + dx);
                let c = cfa.color_at(y, x).min(2);
                sum[c] += data[y * stride + x];
                n[c] += 1;
            }
            out.push(std::array::from_fn(|c| if n[c] > 0 { sum[c] / n[c] as f32 } else { 0.0 }));
        }
    }
    Ok((out, w2, h2))
}

/// สมดุลแสงขาวตามกล้อง + เมทริกซ์สีของกล้อง → sRGB เชิงเส้น พร้อมจัดการส่วนที่เต็มเซนเซอร์
struct CameraToRgb {
    wb: [f32; 3],
    cam2rgb: [[f32; 3]; 3],
}

impl CameraToRgb {
    fn new(raw: &RawImage) -> CameraToRgb {
        let coeffs = raw.wb_coeffs;
        let wb = if coeffs[0].is_finite() && coeffs[1].is_finite() && coeffs[2].is_finite() && coeffs[1] > 0.0 {
            [coeffs[0] / coeffs[1], 1.0, coeffs[2] / coeffs[1]]
        } else {
            [1.0; 3]
        };
        let xyz2cam = Self::xyz_to_camera(raw);
        let rgb2cam = matrix::normalize(matrix::multiply(&xyz2cam, &SRGB_TO_XYZ_D65));
        CameraToRgb { wb, cam2rgb: matrix::pseudo_inverse(rgb2cam) }
    }

    /// เมทริกซ์ XYZ → กล้อง ของแสง D65 (แปลงจากแสงอื่นด้วย Bradford ถ้ากล้องมีแต่แสงอื่น) — ไม่มีเลยใช้เมทริกซ์เอกลักษณ์
    fn xyz_to_camera(raw: &RawImage) -> [[f32; 3]; 3] {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let order = [
            Illuminant::D65,
            Illuminant::D55,
            Illuminant::D50,
            Illuminant::D75,
            Illuminant::Daylight,
            Illuminant::A,
            Illuminant::Flash,
        ];
        let Some((illu, flat)) = raw.color_matrix_find_first(order) else { return identity };
        let Some(m) = matrix::transform_1d::<3, 3>(flat.get(..9).unwrap_or(&[])) else { return identity };
        if illu == Illuminant::D65 {
            m
        } else {
            adapt_bradford(&illu, &Illuminant::D65, &m)
        }
    }

    fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let u = [c[0] * self.wb[0], c[1] * self.wb[1], c[2] * self.wb[2]];
        // ช่องเขียว (ตัวคูณต่ำสุด) เต็มเซนเซอร์: สีจริงไม่รู้แล้ว — ค่อย ๆ เปลี่ยนเป็นสีกลางที่ความสว่างของช่องที่สว่างที่สุด
        let t = ((c[1] - 0.95) / (0.995 - 0.95)).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let u = if t > 0.0 {
            let l = u[0].max(u[1]).max(u[2]);
            u.map(|v| v + (l - v) * t)
        } else {
            u
        };
        let m = &self.cam2rgb;
        let rgb = [
            m[0][0] * u[0] + m[0][1] * u[1] + m[0][2] * u[2],
            m[1][0] * u[0] + m[1][1] * u[1] + m[1][2] * u[2],
            m[2][0] * u[0] + m[2][1] * u[1] + m[2][2] * u[2],
        ];
        let y = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        rgb.map(|v| (y + (v - y) * SATURATION).clamp(0.0, HEADROOM))
    }
}

/// สำหรับเทสต์: ไฟล์ DNG แบบ Bayer (RGGB, 12 บิต) จากภาพในแสงเชิงเส้นที่รู้ค่า — เมทริกซ์สีของ "กล้อง" เท่ากับ sRGB
/// และสมดุลแสงขาวที่บันทึกไว้ (R×2, B×1.5) ล้างแล้วจึงควรได้ภาพเดิมกลับมา (คูณความอิ่มสีของโทนตั้งต้น) ·
/// `orientation` เลข EXIF · `taken` รูปแบบ EXIF `YYYY:MM:DD HH:MM:SS`
#[doc(hidden)]
pub fn test_dng(scene: &Linear, orientation: u16, taken: &str) -> Vec<u8> {
    use rawler::cfa::{PlaneColor, CFA};
    use rawler::dng::{writer::DngWriter, CropMode, DngCompression, DngPhotometricConversion, DNG_VERSION_V1_4};
    use rawler::formats::tiff::{Rational, SRational};
    use rawler::pixarray::PixU16;
    use rawler::rawimage::{BlackLevel, CFAConfig, WhiteLevel};
    const WHITE: f32 = 4095.0;
    const WB: [f32; 3] = [2.0, 1.0, 1.5];
    let (w, h) = (scene.width() as usize & !1, scene.height() as usize & !1);
    let cfa = CFA::new("RGGB");
    let mut data = vec![0u16; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = cfa.color_at(y, x).min(2);
            let v = scene.get_pixel(x as u32, y as u32).0[c] / WB[c];
            data[y * w + x] = (v.clamp(0.0, 1.0) * WHITE).round() as u16;
        }
    }
    let mut cam = rawler::decoders::Camera::new();
    for s in [&mut cam.make, &mut cam.clean_make] {
        *s = "Puri".into();
    }
    for s in [&mut cam.model, &mut cam.clean_model] {
        *s = "Test RAW".into();
    }
    cam.cfa = cfa.clone();
    let config = CFAConfig { cfa, colors: PlaneColor::default(), sensor: rawler::imgop::sensor::SensorType::Bayer };
    let raw = RawImage::new(
        cam,
        PixU16::new_with(data, w, h),
        1,
        [WB[0], WB[1], WB[2], f32::NAN],
        RawPhotometricInterpretation::Cfa(config),
        Some(BlackLevel::new(&[0u16], 1, 1, 1)),
        Some(WhiteLevel::new(vec![WHITE as u32])),
        false,
    );
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut dng = DngWriter::new(&mut out, DNG_VERSION_V1_4).expect("DNG writer");
        dng.load_base_tags(&raw).expect("base tags");
        let xyz_to_srgb: [f32; 9] = [3.2406, -1.5372, -0.4986, -0.9689, 1.8758, 0.0415, 0.0557, -0.2040, 1.0570];
        let matrix: Vec<SRational> = xyz_to_srgb.iter().map(|v| SRational::new((v * 10000.0) as i32, 10000)).collect();
        dng.color_matrix(1, Illuminant::D65, &matrix);
        let neutral: Vec<Rational> = WB.iter().map(|v| Rational::new((10000.0 / v) as u32, 10000)).collect();
        dng.as_shot_neutral(&neutral);
        let md = RawMetadata {
            exif: rawler::exif::Exif {
                orientation: Some(orientation),
                date_time_original: Some(taken.into()),
                ..Default::default()
            },
            model: "Test RAW".into(),
            make: "Puri".into(),
            lens: None,
            unique_image_id: None,
            rating: None,
        };
        dng.load_metadata(&md).expect("metadata");
        dng.subframe_on_root(0)
            .raw_image(&raw, CropMode::None, DngCompression::Uncompressed, DngPhotometricConversion::Original, 1)
            .expect("raw image");
        dng.close().expect("close");
    }
    out.into_inner()
}

/// ภาพ RAW แบบเชิงเส้นอยู่แล้ว (เช่น RAW ขนาด M/S ของ Sony): ครึ่งขนาดด้วยค่าเฉลี่ย 2×2 ในกรอบที่กล้องกำหนด
fn binned_linear(mut raw: RawImage) -> Result<(Vec<[f32; 3]>, usize, usize)> {
    raw.apply_scaling().map_err(raw_error)?;
    let data = raw.data.as_f32();
    let stride = raw.width;
    let (x0, y0, w, h) = match raw.crop_area.or(raw.active_area) {
        Some(r) => (r.p.x, r.p.y, r.d.w, r.d.h),
        None => (0, 0, raw.width, raw.height),
    };
    let (w2, h2) = ((w / 2).min((raw.width - x0) / 2), (h / 2).min((raw.height - y0) / 2));
    let mut out = Vec::with_capacity(w2 * h2);
    for j in 0..h2 {
        for i in 0..w2 {
            let mut sum = [0.0f32; 3];
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let at = ((y0 + 2 * j + dy) * stride + x0 + 2 * i + dx) * 3;
                for (c, s) in sum.iter_mut().enumerate() {
                    *s += data[at + c];
                }
            }
            out.push(sum.map(|s| s / 4.0));
        }
    }
    Ok((out, w2, h2))
}

/// ย่อภาพเชิงเส้นให้ด้านยาวไม่เกิน `max_edge` (ย่อในแสงเชิงเส้น — ถูกต้องทางแสงกว่าย่อค่า sRGB)
pub fn shrink(img: Linear, max_edge: u32) -> Linear {
    if img.width().max(img.height()) <= max_edge {
        return img;
    }
    let s = max_edge as f64 / img.width().max(img.height()) as f64;
    let (w, h) = (((img.width() as f64 * s).round() as u32).max(1), ((img.height() as f64 * s).round() as u32).max(1));
    image::imageops::resize(&img, w, h, image::imageops::FilterType::CatmullRom)
}

/// โทนตั้งต้นของ RAW (ไม่แต่ง) 8 บิต ด้านยาวไม่เกิน `max_edge` — ภาพย่อ ตัวดูภาพ ตัววิเคราะห์ใบหน้า
/// และภาพต้นทางของตัวอย่างสดในหน้าแต่งภาพ (ซึ่งถอดกลับเป็นแสงเชิงเส้นด้วย `adjust::raw_tone_inverse`)
pub fn base_image(bytes: &[u8], max_edge: u32) -> Result<(image::RgbImage, RawInfo)> {
    let (lin, info) = develop(bytes, max_edge)?;
    let lin = shrink(lin, max_edge);
    Ok((crate::adjust::apply_linear(&lin, &crate::adjust::Adjust::default()), info))
}

/// รูป RAW ตามที่แต่ง ด้านยาวไม่เกิน `max_edge` (`u32::MAX` = เต็มขนาด) — ล้างเท่าที่ต้องใช้ ถอดเผื่อส่วนที่ถูกตัดออก
pub fn render(bytes: &[u8], s: &crate::edit::EditSettings, max_edge: u32) -> Result<image::RgbImage> {
    let need = match info(bytes) {
        Ok(i) if max_edge != u32::MAX => {
            let (ow, oh) = crate::edit::Geometry::new(i.width, i.height, s).output_size();
            let ratio = ow.max(oh) as f64 / i.width.max(i.height).max(1) as f64;
            (max_edge as f64 / ratio.max(1e-6)).ceil().min(u32::MAX as f64) as u32
        }
        _ => u32::MAX,
    };
    let (lin, _) = develop(bytes, need)?;
    let img = crate::edit::render_geometry(DynamicImage::ImageRgb32F(lin), s).into_rgb32f();
    let img = if max_edge == u32::MAX { img } else { shrink(img, max_edge) };
    Ok(crate::adjust::apply_linear(&img, &s.adjust))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// ฉากทดสอบในแสงเชิงเส้น: ครึ่งบนสีเทาไล่ความสว่าง · ครึ่งล่างสีส้ม น้ำเงิน และเทากลาง (18%)
    fn scene() -> Linear {
        Linear::from_fn(96, 64, |x, y| {
            if y < 32 {
                let v = 0.02 + 0.6 * x as f32 / 95.0;
                Rgb([v, v, v])
            } else if x < 32 {
                Rgb([0.40, 0.10, 0.05])
            } else if x < 64 {
                Rgb([0.05, 0.20, 0.40])
            } else {
                Rgb([0.18, 0.18, 0.18])
            }
        })
    }

    #[test]
    fn a_synthetic_dng_develops_back_to_its_scene() {
        let dng = test_dng(&scene(), 1, "2026:11:21 10:20:30");
        assert!(is_raw_bytes(&dng) && !is_raw_bytes(b"\xFF\xD8\xFF\xE0 jpeg"));
        let i = info(&dng).unwrap();
        assert_eq!((i.width, i.height, i.make.as_str()), (96, 64, "Puri"));
        assert_eq!(i.exif.taken_at_local.as_deref(), Some("2026-11-21T10:20:30"));
        // เต็มขนาด (demosaic): สีเทากลางเป็นกลางและสว่างเท่าเดิม · สีส้มยังเป็นส้ม (ความอิ่มสีของโทนตั้งต้นเพิ่มขึ้นเล็กน้อย)
        let (full, _) = develop(&dng, u32::MAX).unwrap();
        assert_eq!(full.dimensions(), (96, 64));
        let gray = full.get_pixel(80, 48).0;
        assert!(gray.iter().all(|c| (c - 0.18).abs() < 0.01), "{gray:?}");
        let orange = full.get_pixel(16, 48).0;
        assert!((orange[0] - 0.424).abs() < 0.02 && orange[1] < 0.11 && orange[2] < 0.05, "{orange:?}");
        // ครึ่งขนาด (รวม 2×2) เมื่อขอภาพเล็ก: ค่าเท่ากัน
        let (half, _) = develop(&dng, 24).unwrap();
        assert_eq!(half.dimensions(), (48, 32));
        let gray = half.get_pixel(40, 24).0;
        assert!(gray.iter().all(|c| (c - 0.18).abs() < 0.01), "{gray:?}");
    }

    #[test]
    fn orientation_from_the_file_turns_the_developed_image() {
        let dng = test_dng(&scene(), 6, "2026:11:21 10:20:30");
        let i = info(&dng).unwrap();
        assert_eq!((i.width, i.height, i.exif.orientation), (64, 96, 6));
        let (full, _) = develop(&dng, u32::MAX).unwrap();
        assert_eq!(full.dimensions(), (64, 96));
        // หมุน 90° ตามเข็ม: แถบสีเทาไล่ความสว่าง (เดิมด้านบน) อยู่ด้านขวา
        assert!(full.get_pixel(48, 90).0[1] > full.get_pixel(48, 4).0[1]);
    }

    /// ส่วนที่เต็มเซนเซอร์: ช่องแดง/น้ำเงินถูกคูณสมดุลแสงขาวขึ้นแต่เขียวไม่ขึ้นตาม — ไม่จัดการจะออกชมพู
    #[test]
    fn clipped_highlights_turn_white_not_pink() {
        let bright = Linear::from_fn(32, 32, |_, _| Rgb([3.0, 3.0, 3.0]));
        let (full, _) = develop(&test_dng(&bright, 1, "2026:11:21 10:20:30"), u32::MAX).unwrap();
        let p = full.get_pixel(16, 16).0;
        assert!((p[0] - p[1]).abs() < 0.02 && (p[2] - p[1]).abs() < 0.02 && p[1] > 1.0, "{p:?}");
        // ฐานโทนตั้งต้น: ขาวสุด
        let (base, _) = base_image(&test_dng(&bright, 1, "2026:11:21 10:20:30"), 32).unwrap();
        assert!(base.get_pixel(16, 16).0.iter().all(|v| *v >= 250), "{:?}", base.get_pixel(16, 16));
    }

    #[test]
    fn exported_jpeg_keeps_the_date_and_camera_and_stands_upright() {
        let dng = test_dng(&scene(), 6, "2026:11:21 10:20:30");
        let out = crate::media::export_jpeg(&dng, crate::media::Fit::Original, None, None).unwrap();
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (64, 96), "ล้างแล้วตั้งตรง");
        let exif = crate::media::read_exif(&out);
        assert_eq!(exif.orientation, 1, "ไม่ให้แอปอื่นหมุนซ้ำ");
        assert_eq!(exif.taken_at_local.as_deref(), Some("2026-11-21T10:20:30"));
        let fields = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
        let make = fields.get_field(exif::Tag::Make, exif::In::PRIMARY).unwrap();
        assert_eq!(make.display_value().to_string(), "\"Puri\"");
    }
}
