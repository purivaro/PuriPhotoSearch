//! แต่งภาพแบบไม่แตะไฟล์ (DECISIONS ข้อ 17) — แนวเดียวกับ Lightroom
//!
//! ค่าที่แต่งเก็บต่อรูปในตาราง `photo_edits` ไฟล์ต้นฉบับไม่ถูกแตะ ภาพที่แต่งสร้างเมื่อแสดง (เก็บใน cache) และตอนส่งออก
//! ลำดับ: หมุนตาม EXIF → หมุนทีละ 90° → ปรับเอียง (ตัดมุมที่ว่างออกเอง คงสัดส่วนเดิม) → ตัด
//!
//! ใบหน้าเก็บพิกัดบนภาพเดิมเสมอ การแต่งจึงไม่ทำให้ข้อมูลใบหน้า ชื่อ หรือคำยืนยันเปลี่ยน
//! ตอนแสดงแปลงกรอบไปยังภาพที่แต่งด้วย [`Geometry`] (สูตรเดียวกับ `editGeometry.ts` ฝั่ง UI)

use crate::error::{AppError, Result};
use crate::faces::BBox;
use crate::{catalog, db, media, util, App};
use image::{DynamicImage, ImageBuffer, Pixel};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// สี่เหลี่ยมเป็นสัดส่วน 0–1
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const FULL: Rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditSettings {
    /// หมุนตามเข็มนาฬิกาทีละ 90° (0, 90, 180, 270)
    pub rotate: u16,
    /// ปรับเอียง เป็นองศาตามเข็มนาฬิกา (−45 ถึง 45)
    pub straighten: f64,
    /// กรอบตัด เป็นสัดส่วนของภาพหลังหมุนและปรับเอียง — None = ไม่ตัด
    pub crop: Option<Rect>,
    /// แสงและสี (ทำหลังตัด/หมุน)
    pub adjust: crate::adjust::Adjust,
}

pub const MAX_STRAIGHTEN: f64 = 45.0;
/// กรอบตัดเล็กสุด (สัดส่วนของแต่ละด้าน)
const MIN_CROP: f64 = 0.02;
const EPS: f64 = 1e-6;

impl EditSettings {
    pub fn validate(&self) -> Result<()> {
        self.adjust.validate()?;
        if ![0, 90, 180, 270].contains(&self.rotate) {
            return Err(AppError::invalid("หมุนได้ทีละ 90 องศาเท่านั้น"));
        }
        if !self.straighten.is_finite() || self.straighten.abs() > MAX_STRAIGHTEN + EPS {
            return Err(AppError::invalid("ปรับเอียงได้ไม่เกิน 45 องศา"));
        }
        if let Some(c) = self.crop {
            let ok = [c.x, c.y, c.w, c.h].iter().all(|v| v.is_finite())
                && c.x >= -EPS
                && c.y >= -EPS
                && c.w >= MIN_CROP
                && c.h >= MIN_CROP
                && c.x + c.w <= 1.0 + EPS
                && c.y + c.h <= 1.0 + EPS;
            if !ok {
                return Err(AppError::invalid("กรอบตัดไม่ถูกต้อง"));
            }
        }
        Ok(())
    }

    /// ไม่ได้แต่งอะไรเลย (เก็บแบบนี้ = ลบการแต่ง)
    pub fn is_identity(&self) -> bool {
        self.is_geometry_identity() && self.adjust.is_neutral()
    }

    /// ไม่ได้ตัด หมุน หรือปรับเอียง (ขนาดและตำแหน่งของภาพเท่าเดิม)
    pub fn is_geometry_identity(&self) -> bool {
        self.rotate == 0 && self.straighten == 0.0 && self.crop.is_none_or(|c| c == FULL)
    }
}

/// การแปลงพิกัดระหว่างภาพเดิม (หลังหมุนตาม EXIF) กับภาพที่แต่ง ทุกพิกัดเป็นสัดส่วน 0–1
pub struct Geometry {
    /// ขนาดภาพหลังหมุนทีละ 90° (พิกเซล)
    w: f64,
    h: f64,
    quarter: u16,
    cos: f64,
    sin: f64,
    /// กรอบหลังปรับเอียง = ภาพหลังหมุน × scale (สี่เหลี่ยมใหญ่สุดที่อยู่ในภาพที่เอียงได้ทั้งหมด สัดส่วนเดิม)
    scale: f64,
    crop: Rect,
}

impl Geometry {
    /// `width`×`height` = ขนาดภาพเดิมหลังหมุนตาม EXIF
    pub fn new(width: u32, height: u32, s: &EditSettings) -> Geometry {
        let (w, h) = if s.rotate % 180 == 90 { (height, width) } else { (width, height) };
        let (w, h) = (w.max(1) as f64, h.max(1) as f64);
        let (sin, cos) = s.straighten.to_radians().sin_cos();
        let (sa, ca) = (sin.abs(), cos.abs());
        let scale = (w / (w * ca + h * sa)).min(h / (w * sa + h * ca));
        Geometry { w, h, quarter: s.rotate, cos, sin, scale, crop: s.crop.unwrap_or(FULL) }
    }

    /// ขนาดของภาพที่แต่ง (พิกเซล)
    pub fn output_size(&self) -> (u32, u32) {
        let px = |v: f64| (v.round() as u32).max(1);
        (px(self.crop.w * self.scale * self.w), px(self.crop.h * self.scale * self.h))
    }

    fn quarter_forward(&self, u: f64, v: f64) -> (f64, f64) {
        match self.quarter {
            90 => (1.0 - v, u),
            180 => (1.0 - u, 1.0 - v),
            270 => (v, 1.0 - u),
            _ => (u, v),
        }
    }

    fn quarter_inverse(&self, u: f64, v: f64) -> (f64, f64) {
        match self.quarter {
            90 => (v, 1.0 - u),
            180 => (1.0 - u, 1.0 - v),
            270 => (1.0 - v, u),
            _ => (u, v),
        }
    }

    /// จุดบนภาพเดิม → จุดบนภาพที่แต่ง
    pub fn forward(&self, u: f64, v: f64) -> (f64, f64) {
        let (u, v) = self.quarter_forward(u, v);
        let (px, py) = ((u - 0.5) * self.w, (v - 0.5) * self.h);
        let (rx, ry) = (px * self.cos - py * self.sin, px * self.sin + py * self.cos);
        let (fu, fv) = (rx / (self.scale * self.w) + 0.5, ry / (self.scale * self.h) + 0.5);
        ((fu - self.crop.x) / self.crop.w, (fv - self.crop.y) / self.crop.h)
    }

    /// จุดบนภาพที่แต่ง → จุดบนภาพหลังหมุนทีละ 90° (ก่อนปรับเอียง)
    fn to_rotated(&self, a: f64, b: f64) -> (f64, f64) {
        let (fu, fv) = (self.crop.x + a * self.crop.w, self.crop.y + b * self.crop.h);
        let (rx, ry) = ((fu - 0.5) * self.scale * self.w, (fv - 0.5) * self.scale * self.h);
        let (px, py) = (rx * self.cos + ry * self.sin, -rx * self.sin + ry * self.cos);
        (px / self.w + 0.5, py / self.h + 0.5)
    }

    /// จุดบนภาพที่แต่ง → จุดบนภาพเดิม
    pub fn inverse(&self, a: f64, b: f64) -> (f64, f64) {
        let (u, v) = self.to_rotated(a, b);
        self.quarter_inverse(u, v)
    }

    /// กรอบใบหน้าบนภาพเดิม → บนภาพที่แต่ง (กรอบตั้งตรงที่ครอบมุมทั้งสี่ ตัดส่วนที่ล้นออก)
    /// None = เหลือในภาพที่แต่งไม่ถึงครึ่ง (ถูกตัดออก)
    pub fn map_box(&self, b: BBox) -> Option<BBox> {
        let pts = corners(b).map(|(u, v)| self.forward(u, v));
        let (x0, y0, x1, y1) = bounds(&pts);
        let full = (x1 - x0) * (y1 - y0);
        let (cx0, cy0, cx1, cy1) = (x0.clamp(0.0, 1.0), y0.clamp(0.0, 1.0), x1.clamp(0.0, 1.0), y1.clamp(0.0, 1.0));
        let visible = (cx1 - cx0).max(0.0) * (cy1 - cy0).max(0.0);
        (full > 0.0 && visible >= full * 0.5).then_some(BBox { x: cx0, y: cy0, w: cx1 - cx0, h: cy1 - cy0 })
    }

    /// กรอบที่วาดบนภาพที่แต่ง → กรอบบนภาพเดิม (ใช้เก็บใบหน้าที่ผู้ใช้วาดเอง)
    pub fn unmap_box(&self, b: BBox) -> BBox {
        let pts = corners(b).map(|(a, c)| self.inverse(a, c));
        let (x0, y0, x1, y1) = bounds(&pts);
        let (x0, y0, x1, y1) = (x0.clamp(0.0, 1.0), y0.clamp(0.0, 1.0), x1.clamp(0.0, 1.0), y1.clamp(0.0, 1.0));
        BBox { x: x0, y: y0, w: (x1 - x0).max(0.0), h: (y1 - y0).max(0.0) }
    }
}

fn corners(b: BBox) -> [(f64, f64); 4] {
    [(b.x, b.y), (b.x + b.w, b.y), (b.x, b.y + b.h), (b.x + b.w, b.y + b.h)]
}

fn bounds(pts: &[(f64, f64); 4]) -> (f64, f64, f64, f64) {
    pts.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(x0, y0, x1, y1), &(x, y)| {
        (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
    })
}

/// ภาพที่แต่งจากภาพเดิมที่หมุนตาม EXIF แล้ว (ขนาดใดก็ได้ — พิกัดเป็นสัดส่วน)
pub fn render(img: DynamicImage, s: &EditSettings) -> DynamicImage {
    render_with(img, s, crate::adjust::Profile::Display)
}

/// เหมือน [`render`] ตามชนิดของภาพต้นทาง (RAW: `img` เป็นภาพ 8 บิตของโทนตั้งต้น)
pub fn render_with(img: DynamicImage, s: &EditSettings, profile: crate::adjust::Profile) -> DynamicImage {
    crate::adjust::apply_profile(render_geometry(img, s), &s.adjust, profile)
}

/// ตัด หมุน ปรับเอียง (ภาพ 8 บิต หรือภาพ RAW แบบทศนิยม)
pub(crate) fn render_geometry(img: DynamicImage, s: &EditSettings) -> DynamicImage {
    if s.is_geometry_identity() {
        return img;
    }
    let g = Geometry::new(img.width(), img.height(), s);
    let img = match s.rotate {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img,
    };
    let (ow, oh) = g.output_size();
    if s.straighten == 0.0 {
        // ไม่เอียง: ตัดตามพิกเซลตรง ๆ ไม่ต้องสุ่มสีใหม่
        let (w, h) = (img.width(), img.height());
        let x = ((g.crop.x * w as f64).round() as u32).min(w - 1);
        let y = ((g.crop.y * h as f64).round() as u32).min(h - 1);
        return img.crop_imm(x, y, ow.min(w - x), oh.min(h - y));
    }
    match img {
        DynamicImage::ImageRgb8(src) => DynamicImage::ImageRgb8(resample(&src, &g, ow, oh)),
        DynamicImage::ImageLuma8(src) => DynamicImage::ImageLuma8(resample(&src, &g, ow, oh)),
        DynamicImage::ImageRgb32F(src) => DynamicImage::ImageRgb32F(resample(&src, &g, ow, oh)),
        other => DynamicImage::ImageRgba8(resample(&other.to_rgba8(), &g, ow, oh)),
    }
}

/// ค่าของหนึ่งช่องสีที่ resample ได้: 8 บิต (ปัดเป็นจำนวนเต็ม) หรือทศนิยม (ภาพ RAW เชิงเส้น)
trait Channel: image::Primitive {
    fn get(self) -> f64;
    fn put(v: f64) -> Self;
}

impl Channel for u8 {
    fn get(self) -> f64 {
        self as f64
    }
    fn put(v: f64) -> u8 {
        v.round() as u8
    }
}

impl Channel for f32 {
    fn get(self) -> f64 {
        self as f64
    }
    fn put(v: f64) -> f32 {
        v as f32
    }
}

/// ทุกพิกเซลของภาพที่แต่งย้อนไปหาจุดบนภาพหลังหมุน แล้วผสมสี่พิกเซลรอบ ๆ (bilinear)
fn resample<P: Pixel>(
    src: &ImageBuffer<P, Vec<P::Subpixel>>,
    g: &Geometry,
    ow: u32,
    oh: u32,
) -> ImageBuffer<P, Vec<P::Subpixel>>
where
    P::Subpixel: Channel,
{
    let n = P::CHANNEL_COUNT as usize;
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let raw = src.as_raw();
    let mut out = vec![<P::Subpixel as Channel>::put(0.0); ow as usize * oh as usize * n];
    for (j, row) in out.chunks_exact_mut(ow as usize * n).enumerate() {
        let b = (j as f64 + 0.5) / oh as f64;
        for (i, px) in row.chunks_exact_mut(n).enumerate() {
            let (u, v) = g.to_rotated((i as f64 + 0.5) / ow as f64, b);
            let (x, y) =
                ((u * sw as f64 - 0.5).clamp(0.0, (sw - 1) as f64), (v * sh as f64 - 0.5).clamp(0.0, (sh - 1) as f64));
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1));
            let (fx, fy) = (x - x0 as f64, y - y0 as f64);
            let at = |xx: usize, yy: usize, c: usize| raw[(yy * sw + xx) * n + c].get();
            for (c, value) in px.iter_mut().enumerate() {
                let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
                let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
                *value = Channel::put(top * (1.0 - fy) + bottom * fy);
            }
        }
    }
    ImageBuffer::from_raw(ow, oh, out).expect("ขนาด buffer ตรงกับภาพ")
}

// ───────────────────────── เก็บในคลัง / cache ─────────────────────────

/// ด้านยาวของภาพที่แต่งที่ตัวดูภาพใช้ (พอสำหรับจอทั่วไป ภาพเต็มความละเอียดได้ตอนส่งออก)
pub const VIEW_EDGE: u32 = 2560;

pub(crate) struct Stored {
    pub edit_id: String,
    pub settings: EditSettings,
}

pub(crate) fn load(conn: &Connection, photo_id: &str) -> Result<Option<Stored>> {
    let row: Option<(String, String)> = conn
        .query_row("SELECT edit_id, settings FROM photo_edits WHERE photo_id = ?1", [photo_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    row.map(|(edit_id, json)| Ok(Stored { edit_id, settings: serde_json::from_str(&json)? })).transpose()
}

/// การแปลงพิกัดของรูปที่ revision นี้เป็นของ — None เมื่อไม่ได้ตัด/หมุน หรือยังไม่รู้ขนาดภาพ
pub(crate) fn geometry_for_revision(conn: &Connection, revision_id: &str) -> Result<Option<Geometry>> {
    let row: Option<(u32, u32, Option<String>)> = conn
        .query_row(
            "SELECT r.width, r.height, pe.settings FROM photo_revisions r
             LEFT JOIN photo_edits pe ON pe.photo_id = r.photo_id WHERE r.id = ?1",
            [revision_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(match row {
        Some((w, h, Some(json))) if w > 0 && h > 0 => {
            let settings: EditSettings = serde_json::from_str(&json)?;
            // แต่งแค่แสงสี: ตำแหน่งเท่าเดิม ไม่ต้องแปลงกรอบ
            (!settings.is_geometry_identity()).then(|| Geometry::new(w, h, &settings))
        }
        _ => None,
    })
}

pub(crate) fn cache_path(cache_dir: &Path, photo_id: &str, edit_id: &str, kind: &str) -> PathBuf {
    cache_dir.join("edits").join(format!("{photo_id}-{edit_id}-{kind}.jpg"))
}

/// ภาพที่แต่งย่อให้ด้านยาวไม่เกิน `max_edge` เป็น JPEG — ถอดรหัสต้นฉบับเท่าที่ต้องใช้
/// (`bytes` เป็นต้นฉบับ หรือภาพย่อของแอปซึ่งหมุนตั้งตรงแล้ว ก็ได้ — ภาพย่อของรูป RAW ใช้ `profile` = Raw)
pub(crate) fn preview_jpeg(
    bytes: &[u8],
    s: &EditSettings,
    max_edge: u32,
    quality: u8,
    profile: crate::adjust::Profile,
) -> Result<Vec<u8>> {
    media::encode_jpeg(&render_small(bytes, s, max_edge, profile)?, quality)
}

/// ภาพที่แต่งย่อให้ด้านยาวไม่เกิน `max_edge` — ถอดรหัสต้นฉบับเท่าที่ต้องใช้ (ถอดเผื่อส่วนที่ถูกตัดออก)
/// · ไฟล์ RAW ล้างจากข้อมูลเซนเซอร์ (`profile` ใช้กับภาพ 8 บิตเท่านั้น)
pub(crate) fn render_small(
    bytes: &[u8],
    s: &EditSettings,
    max_edge: u32,
    profile: crate::adjust::Profile,
) -> Result<DynamicImage> {
    if crate::raw::is_raw_bytes(bytes) {
        return Ok(DynamicImage::ImageRgb8(crate::raw::render(bytes, s, max_edge)?));
    }
    let need = match media::upright_dimensions(bytes) {
        Some((w, h)) if w > 0 && h > 0 => {
            let (ow, oh) = Geometry::new(w, h, s).output_size();
            let ratio = ow.max(oh) as f64 / w.max(h) as f64;
            (max_edge as f64 / ratio).ceil() as u32
        }
        _ => u32::MAX,
    };
    let img = render_with(media::decode_upright(bytes, need)?, s, profile);
    Ok(if img.width().max(img.height()) > max_edge { img.thumbnail(max_edge, max_edge) } else { img })
}

/// ส่วนของการแต่งที่จะวางให้รูปอื่น (แบบ "Paste Settings" ของ Lightroom)
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditParts {
    /// ตัด หมุน ปรับเอียง (แต่ละรูปมักตัดต่างกัน — UI ไม่ติ๊กไว้ก่อน)
    pub geometry: bool,
    /// ค่าแสง คอนทราสต์ ส่วนสว่าง เงา จุดขาว จุดดำ และเส้นโค้งโทน
    pub light: bool,
    /// อุณหภูมิสี โทนสี ความสดของสี ความอิ่มสี และ HSL
    pub color: bool,
}

impl EditParts {
    /// การแต่งเดิมของรูป แทนเฉพาะส่วนที่เลือกด้วยค่าจาก `from`
    pub fn merge(self, mut base: EditSettings, from: &EditSettings) -> EditSettings {
        if self.geometry {
            (base.rotate, base.straighten, base.crop) = (from.rotate, from.straighten, from.crop);
        }
        let (a, f) = (&mut base.adjust, &from.adjust);
        if self.light {
            (a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks) =
                (f.exposure, f.contrast, f.highlights, f.shadows, f.whites, f.blacks);
            (a.parametric, a.parametric_split) = (f.parametric, f.parametric_split);
            (a.curve, a.curve_red, a.curve_green, a.curve_blue) =
                (f.curve.clone(), f.curve_red.clone(), f.curve_green.clone(), f.curve_blue.clone());
            (a.levels, a.levels_red, a.levels_green, a.levels_blue) =
                (f.levels, f.levels_red, f.levels_green, f.levels_blue);
            a.vignette = f.vignette;
        }
        if self.color {
            (a.temperature, a.tint, a.vibrance, a.saturation) = (f.temperature, f.tint, f.vibrance, f.saturation);
            (a.hue, a.sat, a.lum) = (f.hue, f.sat, f.lum);
            a.grading = f.grading;
        }
        base
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    /// รูปที่การแต่งเปลี่ยน
    pub updated: usize,
    /// รูปที่ได้ค่าเดิมอยู่แล้ว
    pub unchanged: usize,
    /// รูปที่ยังเตรียมข้อมูลไม่เสร็จ (ไม่รู้ขนาดภาพ) จึงข้าม
    pub not_ready: usize,
}

/// วางการแต่งให้หลายรูปพร้อมกัน — เก็บค่าอย่างเดียว ภาพที่แต่งสร้างเมื่อถูกเปิดดู ไฟล์ต้นฉบับไม่ถูกแตะ
pub fn apply_many(
    app: &App,
    selection: &crate::export::SelectionSpec,
    settings: &EditSettings,
    parts: EditParts,
) -> Result<ApplyResult> {
    settings.validate()?;
    if !(parts.geometry || parts.light || parts.color) {
        return Err(AppError::invalid("เลือกอย่างน้อยหนึ่งส่วนที่จะวาง"));
    }
    let mut result = ApplyResult { updated: 0, unchanged: 0, not_ready: 0 };
    let mut stale = Vec::new();
    {
        let conn = app.db();
        let rows = crate::export::load_items(&conn, selection)?;
        if rows.is_empty() {
            return Err(AppError::invalid("ยังไม่ได้เลือกรูป"));
        }
        let tx = conn.unchecked_transaction()?;
        for row in rows {
            if row.width == 0 || row.height == 0 {
                result.not_ready += 1;
                continue;
            }
            let old = load(&tx, &row.photo_id)?;
            let base = old.as_ref().map(|o| o.settings.clone()).unwrap_or_default();
            let next = parts.merge(base.clone(), settings);
            if next == base {
                result.unchanged += 1;
                continue;
            }
            if next.is_identity() {
                tx.execute("DELETE FROM photo_edits WHERE photo_id = ?1", [&row.photo_id])?;
            } else {
                tx.execute(
                    "INSERT INTO photo_edits(photo_id, edit_id, settings, updated_at) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(photo_id) DO UPDATE SET edit_id = excluded.edit_id, settings = excluded.settings,
                       updated_at = excluded.updated_at",
                    params![row.photo_id, util::new_id(), serde_json::to_string(&next)?, util::now()],
                )?;
            }
            if let Some(old) = old {
                stale.push((row.photo_id, old.edit_id));
            }
            result.updated += 1;
        }
        db::bump_revision(&tx)?;
        tx.commit()?;
    }
    for (photo_id, edit_id) in stale {
        for kind in ["thumb", "view"] {
            let _ = std::fs::remove_file(cache_path(&app.cache_dir, &photo_id, &edit_id, kind));
        }
    }
    app.notify(&["photos"]);
    Ok(result)
}

/// บันทึกการแต่งของรูปหนึ่ง (ไม่ได้แต่งอะไร = ลบการแต่ง) — ไฟล์ต้นฉบับไม่ถูกแตะ
pub fn save(app: &App, photo_id: &str, settings: EditSettings) -> Result<catalog::PhotoDetails> {
    settings.validate()?;
    {
        let conn = app.db();
        let (w, h): (i64, i64) = conn
            .query_row(
                "SELECT r.width, r.height FROM photos p JOIN photo_revisions r ON r.id = p.current_revision_id
                 WHERE p.id = ?1 AND p.catalog_state = 'active'",
                [photo_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("รูป"))?;
        if w <= 0 || h <= 0 {
            return Err(AppError::invalid("รูปนี้ยังเตรียมข้อมูลไม่เสร็จ รอสักครู่แล้วลองใหม่"));
        }
        let old = load(&conn, photo_id)?;
        let tx = conn.unchecked_transaction()?;
        if settings.is_identity() {
            tx.execute("DELETE FROM photo_edits WHERE photo_id = ?1", [photo_id])?;
        } else {
            tx.execute(
                "INSERT INTO photo_edits(photo_id, edit_id, settings, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(photo_id) DO UPDATE SET edit_id = excluded.edit_id, settings = excluded.settings,
                   updated_at = excluded.updated_at",
                params![photo_id, util::new_id(), serde_json::to_string(&settings)?, util::now()],
            )?;
        }
        db::bump_revision(&tx)?;
        tx.commit()?;
        // ภาพที่แต่งครั้งก่อนไม่ถูกใช้อีก
        if let Some(old) = old {
            for kind in ["thumb", "view"] {
                let _ = std::fs::remove_file(cache_path(&app.cache_dir, photo_id, &old.edit_id, kind));
            }
        }
    }
    app.notify(&["photos"]);
    catalog::get_photo_details(app, photo_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(rotate: u16, straighten: f64, crop: Option<Rect>) -> EditSettings {
        EditSettings { rotate, straighten, crop, ..Default::default() }
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn quarter_turns_move_corners_like_the_picture_turns() {
        // หมุนตามเข็ม 90°: มุมซ้ายบนไปอยู่ขวาบน ภาพแนวนอนกลายเป็นแนวตั้ง
        let g = Geometry::new(400, 300, &settings(90, 0.0, None));
        assert_eq!(g.output_size(), (300, 400));
        assert!(close(g.forward(0.0, 0.0), (1.0, 0.0)));
        assert!(close(g.forward(1.0, 0.0), (1.0, 1.0)));
        let g = Geometry::new(400, 300, &settings(270, 0.0, None));
        assert!(close(g.forward(0.0, 0.0), (0.0, 1.0)));
        let g = Geometry::new(400, 300, &settings(180, 0.0, None));
        assert!(close(g.forward(0.25, 0.1), (0.75, 0.9)));
    }

    #[test]
    fn forward_and_inverse_undo_each_other_in_every_combination() {
        let crop = Some(Rect { x: 0.1, y: 0.2, w: 0.6, h: 0.5 });
        for rotate in [0, 90, 180, 270] {
            for straighten in [0.0, 7.5, -30.0] {
                let g = Geometry::new(640, 480, &settings(rotate, straighten, crop));
                for p in [(0.3, 0.4), (0.5, 0.5), (0.62, 0.71)] {
                    let back = g.inverse(g.forward(p.0, p.1).0, g.forward(p.0, p.1).1);
                    assert!(close(back, p), "rotate {rotate} straighten {straighten}: {p:?} → {back:?}");
                }
            }
        }
    }

    #[test]
    fn straightening_keeps_the_aspect_and_never_shows_empty_corners() {
        let g = Geometry::new(1000, 500, &settings(0, 10.0, None));
        let (w, h) = g.output_size();
        assert!((w as f64 / h as f64 - 2.0).abs() < 0.01, "สัดส่วนเดิม: {w}×{h}");
        assert!(w < 1000);
        // มุมของภาพที่แต่งต้องย้อนไปตกในภาพเดิมทั้งหมด
        for (a, b) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (u, v) = g.inverse(a, b);
            assert!((-1e-9..=1.0 + 1e-9).contains(&u) && (-1e-9..=1.0 + 1e-9).contains(&v), "{a},{b} → {u},{v}");
        }
    }

    #[test]
    fn faces_follow_the_crop_and_disappear_when_cut_out() {
        let crop = Some(Rect { x: 0.5, y: 0.0, w: 0.5, h: 1.0 });
        let g = Geometry::new(1000, 500, &settings(0, 0.0, crop));
        let right = BBox { x: 0.6, y: 0.2, w: 0.1, h: 0.2 };
        let mapped = g.map_box(right).unwrap();
        assert!(close((mapped.x, mapped.y), (0.2, 0.2)) && (mapped.w - 0.2).abs() < 1e-9);
        assert_eq!(g.map_box(BBox { x: 0.1, y: 0.2, w: 0.1, h: 0.2 }), None, "ใบหน้าครึ่งซ้ายถูกตัดออก");
        // วาดกรอบบนภาพที่แต่ง แล้วเก็บกลับเป็นพิกัดบนภาพเดิม
        let back = g.unmap_box(mapped);
        assert!((back.x - right.x).abs() < 1e-9 && (back.w - right.w).abs() < 1e-9);
    }

    #[test]
    fn rendering_matches_the_geometry() {
        // ภาพ 40×20: ครึ่งซ้ายแดง ครึ่งขวาน้ำเงิน · หมุน 90° แล้วตัดครึ่งบน → เหลือสีแดงล้วน
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(40, 20, |x, _| {
            image::Rgb(if x < 20 { [255, 0, 0] } else { [0, 0, 255] })
        }));
        let s = settings(90, 0.0, Some(Rect { x: 0.0, y: 0.0, w: 1.0, h: 0.5 }));
        let out = render(img.clone(), &s).to_rgb8();
        assert_eq!(out.dimensions(), (20, 20));
        assert!(out.pixels().all(|p| p.0 == [255, 0, 0]));
        // ปรับเอียง: ขนาดตาม Geometry และกลางภาพยังเป็นจุดกลางเดิม
        let s = settings(0, 5.0, None);
        let out = render(img, &s).to_rgb8();
        assert_eq!(out.dimensions(), Geometry::new(40, 20, &s).output_size());
        assert_eq!(out.get_pixel(2, out.height() / 2).0, [255, 0, 0]);
        assert_eq!(out.get_pixel(out.width() - 3, out.height() / 2).0, [0, 0, 255]);
    }
}
