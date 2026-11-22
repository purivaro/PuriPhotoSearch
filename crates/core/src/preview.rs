//! ภาพตัวอย่างสดของหน้าแต่งภาพ (DECISIONS ข้อ 30): Rust วาดด้วยฟังก์ชันชุดเดียวกับภาพที่บันทึก แล้วส่งพิกเซลให้หน้าเว็บ
//!
//! ภาพต้นทางของรูปที่กำลังแต่ง (asset `original` หรือ `rawframe` แบบเดียวกับที่หน้าเว็บเคยถอดเอง ด้านยาวไม่เกิน
//! [`SOURCE_EDGE`]) ถอดครั้งเดียวแล้วเก็บไว้ · แต่ละเฟรมสุ่มส่วนที่เห็น (`view` ของกรอบหลังปรับเอียง) ตามขนาด canvas แล้วทำ
//! แสงสี → Texture/Clarity/Dehaze → ปรับเฉพาะส่วน ด้วย [`crate::adjust::apply_pixels`] และ [`crate::masks::apply_showing`]
//! ตัวเดียวกับไฟล์ที่ส่งออก · ทุกขั้นแบ่งแถวให้ทุกแกน
//!
//! หลอดดูดสี (สมดุลแสงขาวจากจุดสีเทา) และปรับอัตโนมัติคิดจากภาพต้นทางเดียวกันนี้

use crate::adjust::{Adjust, Profile};
use crate::edit::{EditSettings, Geometry, Rect};
use crate::error::{AppError, Result};
use crate::App;
use image::RgbImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// ด้านยาวสูงสุดของภาพต้นทาง (เท่าที่หน้าแต่งภาพเคยใช้)
pub const SOURCE_EDGE: u32 = 2400;
/// ขนาด canvas สูงสุดที่วาดให้
const MAX_CANVAS: u32 = 8192;
const FULL: Rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

/// ภาพต้นทางของหน้าแต่งภาพ: ไฟล์ของรูป หรือภาพจาก RAW คู่ในกรอบของ JPEG (DECISIONS ข้อ 26)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Original,
    Rawframe,
}

impl SourceKind {
    fn asset(self) -> &'static str {
        match self {
            SourceKind::Original => "original",
            SourceKind::Rawframe => "rawframe",
        }
    }
}

/// คำขอภาพหนึ่งเฟรม
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub photo_id: String,
    pub source: SourceKind,
    pub settings: EditSettings,
    /// ขนาด canvas (พิกเซล)
    pub width: u32,
    pub height: u32,
    /// ส่วนของกรอบหลังปรับเอียง (ยังไม่ตัด) ที่ canvas แสดง: แท็บตัดและหมุน = ทั้งกรอบ · แท็บอื่น = กรอบตัด
    pub view: Rect,
    /// canvas คือกรอบที่ตัดแล้วทั้งกรอบ (คิด Vignette) — แท็บตัดและหมุนไม่ใช่
    #[serde(default)]
    pub whole: bool,
    /// false = ภาพก่อนปรับ (กดค้างดูภาพก่อน)
    #[serde(default = "yes")]
    pub adjust: bool,
    /// แสดงบริเวณของ Mask นี้เป็นสีแดงทับ
    #[serde(default)]
    pub show: Option<String>,
}

fn yes() -> bool {
    true
}

/// จุดบนส่วนที่เห็น (สัดส่วนของ canvas) สำหรับหลอดดูดสี
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointRequest {
    pub photo_id: String,
    pub source: SourceKind,
    pub settings: EditSettings,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

/// ภาพต้นทางที่ถอดไว้แล้ว + ภาพย่อทีละครึ่ง (ใช้เมื่อ canvas เล็กกว่าภาพมาก ไม่ให้ภาพแตกเป็นรอยหยัก)
pub(crate) struct Source {
    key: (String, SourceKind, String),
    profile: Profile,
    levels: Mutex<Vec<Arc<RgbImage>>>,
}

impl Source {
    fn level(&self, k: usize) -> Arc<RgbImage> {
        let mut levels = self.levels.lock().unwrap_or_else(|e| e.into_inner());
        while levels.len() <= k {
            let next = halve(levels.last().expect("ระดับแรกมีเสมอ"));
            levels.push(Arc::new(next));
        }
        levels[k].clone()
    }

    fn size(&self) -> (u32, u32) {
        self.level(0).dimensions()
    }
}

/// ย่อครึ่งด้วยค่าเฉลี่ย 2×2 (พิกเซลท้ายที่ไม่ครบคู่ใช้เท่าที่มี)
fn halve(img: &RgbImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let (nw, nh) = (w.div_ceil(2).max(1), h.div_ceil(2).max(1));
    let src = img.as_raw();
    let mut out = vec![0u8; (nw * nh * 3) as usize];
    out.par_chunks_mut((nw * 3) as usize).enumerate().for_each(|(y, line)| {
        let (y0, y1) = (2 * y as u32, (2 * y as u32 + 1).min(h - 1));
        for x in 0..nw {
            let (x0, x1) = (2 * x, (2 * x + 1).min(w - 1));
            for c in 0..3 {
                let at = |xx: u32, yy: u32| src[((yy * w + xx) * 3) as usize + c] as u32;
                line[(x * 3) as usize + c] = ((at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1) + 2) / 4) as u8;
            }
        }
    });
    RgbImage::from_raw(nw, nh, out).expect("ขนาดตรง")
}

/// ภาพต้นทางของรูป (ถอดครั้งแรกแล้วเก็บไว้ รูปที่กำลังแต่งรูปเดียว)
fn source(app: &App, photo_id: &str, kind: SourceKind) -> Result<Arc<Source>> {
    let (revision, path) = {
        let conn = app.db();
        let revision: String = conn
            .query_row("SELECT current_revision_id FROM photos WHERE id = ?1", [photo_id], |r| r.get(0))
            .map_err(|_| AppError::not_found("รูป"))?;
        (revision, crate::catalog::photo_path(&conn, photo_id)?)
    };
    let key = (photo_id.to_string(), kind, revision);
    if let Some(s) = app.editor_source.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|s| s.key == key) {
        return Ok(s.clone());
    }
    let asset = crate::assets::resolve(app, &format!("{}/{photo_id}", kind.asset()))?;
    let img = crate::media::decode_upright(&asset.bytes, SOURCE_EDGE)?.into_rgb8();
    let (w, h) = img.dimensions();
    // ขนาดเดียวกับที่หน้าเว็บเคยย่อ: ด้านยาวไม่เกิน SOURCE_EDGE
    let scale = (SOURCE_EDGE as f64 / w.max(h) as f64).min(1.0);
    let (nw, nh) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
    let img = if (nw, nh) == (w, h) {
        img
    } else {
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3)
    };
    // RAW (ภาพต้นทางเป็นโทนตั้งต้นของ RAW) คิดแสงสีด้วยสูตรของ RAW — DECISIONS ข้อ 25
    let profile = if kind == SourceKind::Rawframe || crate::raw::is_raw_path(std::path::Path::new(&path)) {
        Profile::Raw
    } else {
        Profile::Display
    };
    let source = Arc::new(Source { key, profile, levels: Mutex::new(vec![Arc::new(img)]) });
    *app.editor_source.lock().unwrap_or_else(|e| e.into_inner()) = Some(source.clone());
    Ok(source)
}

/// เรขาคณิตของส่วนที่เห็น: ภาพต้นทาง → หมุน/เอียงตามที่แต่ง → ส่วน `view`
fn view_geometry(src: (u32, u32), s: &EditSettings, view: Rect) -> Geometry {
    Geometry::new(
        src.0,
        src.1,
        &EditSettings { rotate: s.rotate, straighten: s.straighten, crop: Some(view), ..Default::default() },
    )
}

/// สุ่มส่วนที่เห็นของภาพต้นทางลง canvas `w`×`h` (RGBA) แบบ bilinear จากระดับย่อที่ใกล้ขนาด canvas
/// · หน้าเรียว/หน้าผอมของ `retouch` ใส่ตอนสุ่ม (ภาพที่บันทึกทำบนภาพเดิมก่อนตัด/หมุน — จุดต้นทางเดียวกัน)
fn sample(src: &Source, g: &Geometry, w: usize, h: usize, retouch: Option<&crate::retouch::Retouch>) -> Vec<u8> {
    let (sw, sh) = src.size();
    let aspect = sw as f64 / sh as f64;
    // พิกเซลของภาพต้นทางต่อหนึ่งพิกเซลของ canvas (วัดตามแนวนอนและแนวตั้งที่กลางภาพ)
    let at = |x: f64, y: f64| {
        let (u, v) = g.inverse(x / w as f64, y / h as f64);
        (u * sw as f64, v * sh as f64)
    };
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let (a, b, c) = (at(cx, cy), at(cx + 1.0, cy), at(cx, cy + 1.0));
    let step = ((b.0 - a.0).hypot(b.1 - a.1)).min((c.0 - a.0).hypot(c.1 - a.1));
    let level = if step > 1.0 { (step.log2().floor() as usize).min(8) } else { 0 };
    let img = src.level(level);
    let (lw, lh) = img.dimensions();
    let raw = img.as_raw();
    let mut out = vec![0u8; w * h * 4];
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, line)| {
        for x in 0..w {
            let (u, v) = g.inverse((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
            let (u, v) = retouch.map_or((u, v), |r| r.source_point(u, v, aspect));
            let fx = (u * lw as f64 - 0.5).clamp(0.0, (lw - 1) as f64);
            let fy = (v * lh as f64 - 0.5).clamp(0.0, (lh - 1) as f64);
            let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
            let (x1, y1) = ((x0 + 1).min(lw - 1), (y0 + 1).min(lh - 1));
            let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
            let px = |xx: u32, yy: u32, c: usize| raw[((yy * lw + xx) * 3) as usize + c] as f64;
            for c in 0..3 {
                let top = px(x0, y0, c) * (1.0 - tx) + px(x1, y0, c) * tx;
                let bottom = px(x0, y1, c) * (1.0 - tx) + px(x1, y1, c) * tx;
                line[x * 4 + c] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
            }
            line[x * 4 + 3] = 255;
        }
    });
    out
}

fn check_view(v: &Rect) -> Result<()> {
    let ok = [v.x, v.y, v.w, v.h].iter().all(|x| x.is_finite())
        && v.w > 0.0
        && v.h > 0.0
        && v.x >= -1e-6
        && v.y >= -1e-6
        && v.x + v.w <= 1.0 + 1e-6
        && v.y + v.h <= 1.0 + 1e-6;
    if ok {
        Ok(())
    } else {
        Err(AppError::invalid("ส่วนที่แสดงของภาพไม่ถูกต้อง"))
    }
}

/// ภาพตัวอย่างหนึ่งเฟรม: พิกเซล RGBA ขนาด `width`×`height`
pub fn render(app: &App, req: &Request) -> Result<Vec<u8>> {
    if req.width == 0 || req.height == 0 || req.width > MAX_CANVAS || req.height > MAX_CANVAS {
        return Err(AppError::invalid("ขนาดภาพตัวอย่างไม่ถูกต้อง"));
    }
    check_view(&req.view)?;
    req.settings.validate()?;
    let src = source(app, &req.photo_id, req.source)?;
    if req.settings.has_subject() {
        // Mask ตัวคน: ใช้ภาพน้ำหนักที่คิดไว้แล้ว (หน้าแต่งภาพสั่งคิดก่อนด้วย prepare_subject) — ยังไม่มี = ยังไม่เห็นผลของ Mask นั้น
        let mut req = req.clone();
        if let Some(map) = crate::subject::cached(app, &req.photo_id)? {
            crate::subject::attach(&mut req.settings, &map);
        }
        return Ok(render_from(&src, &req));
    }
    Ok(render_from(&src, req))
}

/// ภาพตัวอย่างจากภาพต้นทางในหน่วยความจำ — เหมือน [`render`] โดยไม่อ่านฐานข้อมูล (เทสต์ `tests/preview_parity.rs`)
#[doc(hidden)]
pub fn render_image(src: &RgbImage, profile: Profile, req: &Request) -> Vec<u8> {
    let source = Source {
        key: (String::new(), req.source, String::new()),
        profile,
        levels: Mutex::new(vec![Arc::new(src.clone())]),
    };
    render_from(&source, req)
}

/// ส่วนที่ไม่ต้องใช้ฐานข้อมูล (เทสต์เทียบกับ [`crate::edit::render_with`] ได้ตรง ๆ)
fn render_from(src: &Source, req: &Request) -> Vec<u8> {
    let (w, h) = (req.width as usize, req.height as usize);
    let g = view_geometry(src.size(), &req.settings, req.view);
    // "ก่อนปรับ" = ภาพเดิมไม่มีการแต่งใด ๆ รวมรีทัชใบหน้า
    let retouch = req.settings.retouch.as_ref().filter(|_| req.adjust);
    let mut px = sample(src, &g, w, h, retouch);
    if req.adjust {
        // รัศมีของ Texture/Clarity/Dehaze และขอบเผื่อของ Mask คิดจากทั้งกรอบ (ส่วนที่เห็นเป็นแค่บางส่วน)
        let long = g.frame_long(req.width, req.height);
        let mut a = req.settings.adjust.clone();
        if !req.whole {
            a.vignette = Default::default();
        }
        crate::adjust::apply_pixels(&mut px, w, h, 4, &a, src.profile, long);
        crate::retouch::smooth_faces(&mut px, w, h, 4, &g, retouch);
        crate::masks::apply_showing(&mut px, w, h, 4, &g, &req.settings.masks, req.show.as_deref());
    }
    px
}

/// ส่วนที่ตัดแล้วแบบไม่ปรับแสงสี ด้านยาว `edge` พิกเซล (หลอดดูดสีและปรับอัตโนมัติ)
fn plain(src: &Source, s: &EditSettings, edge: u32) -> (Vec<u8>, usize, usize) {
    let (sw, sh) = src.size();
    let (rw, rh) = if s.rotate % 180 == 90 { (sh, sw) } else { (sw, sh) };
    let crop = s.crop.unwrap_or(FULL);
    let aspect = (rw as f64 / rh as f64) * crop.w / crop.h;
    let (w, h) = if aspect >= 1.0 {
        (edge, ((edge as f64 / aspect).round() as u32).max(1))
    } else {
        (((edge as f64 * aspect).round() as u32).max(1), edge)
    };
    let g = view_geometry((sw, sh), s, crop);
    (sample(src, &g, w as usize, h as usize, s.retouch.as_ref()), w as usize, h as usize)
}

/// ปัดแบบ `Math.round` ของ JavaScript (ครึ่งปัดขึ้น) ให้ค่าที่เสนอเท่าเดิมกับก่อนย้ายมา Rust
fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// หลอดดูดสี: อุณหภูมิสีและโทนสีที่ทำให้จุด (x, y) ของส่วนที่ตัดแล้วเป็นสีเทา (เฉลี่ย 5×5 พิกเซลบนภาพด้านยาว 800)
pub fn white_balance(app: &App, req: &PointRequest) -> Result<serde_json::Value> {
    req.settings.validate()?;
    let src = source(app, &req.photo_id, req.source)?;
    let (px, w, h) = plain(&src, &req.settings, 800);
    let x0 = (js_round(req.x * w as f64) as i64 - 2).clamp(0, (w as i64 - 5).max(0)) as usize;
    let y0 = (js_round(req.y * h as f64) as i64 - 2).clamp(0, (h as i64 - 5).max(0)) as usize;
    let mut sum = [0f64; 3];
    let mut n = 0f64;
    for y in y0..(y0 + 5).min(h) {
        for x in x0..(x0 + 5).min(w) {
            for (c, s) in sum.iter_mut().enumerate() {
                *s += px[(y * w + x) * 4 + c] as f64;
            }
            n += 1.0;
        }
    }
    let rgb = sum.map(|v| v / n);
    let (temperature, tint) = crate::adjust::neutral_white_balance(rgb, src.profile);
    Ok(serde_json::json!({ "temperature": js_round(temperature), "tint": js_round(tint) }))
}

/// ปรับอัตโนมัติจากส่วนที่ตัดแล้ว (ภาพด้านยาว 320): ค่าแสง จุดขาว จุดดำ — แบบระมัดระวัง "ช่วยให้ดีขึ้น" ไม่ใช่เปลี่ยนอารมณ์ภาพ
pub fn auto(app: &App, req: &PointRequest) -> Result<serde_json::Value> {
    req.settings.validate()?;
    let src = source(app, &req.photo_id, req.source)?;
    let (px, _, _) = plain(&src, &req.settings, 320);
    let (exposure, whites, blacks) = auto_light(&px, src.profile);
    Ok(serde_json::json!({ "exposure": exposure, "whites": whites, "blacks": blacks }))
}

/// AI ช่วยจัดองค์ประกอบ (DECISIONS ข้อ 38): ความเอียงจากภาพต้นทาง + กรอบตัดที่เสนอบนภาพที่หมุน/ปรับเอียงตาม `settings`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionRequest {
    pub photo_id: String,
    pub source: SourceKind,
    pub settings: EditSettings,
    /// สัดส่วนที่ล็อกไว้ (กว้าง/สูง ของภาพผล) — ไม่ส่ง = เสนอสัดส่วนเดิมก่อน แล้วสัดส่วนอื่น
    #[serde(default)]
    pub ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Composition {
    /// ค่าปรับเอียงที่เสนอ (องศา ตามเข็มนาฬิกา แบบ `EditSettings::straighten`) — None = หลักฐานไม่ชัด ไม่เสนอ
    pub straighten: Option<f64>,
    pub tilt: Option<crate::compose::Tilt>,
    pub crops: Vec<crate::compose::CropSuggestion>,
}

pub fn composition(app: &App, req: &CompositionRequest) -> Result<Composition> {
    req.settings.validate()?;
    let src = source(app, &req.photo_id, req.source)?;
    let tilt = crate::compose::detect_tilt(&src.level(0));
    // ภาพเล็กพอสำหรับเลือกกรอบ (แผนที่ความเด่นใช้ด้านยาว 192)
    let mut k = 0;
    while src.level(k + 1).width().max(src.level(k + 1).height()) >= 480 {
        k += 1;
    }
    let small = src.level(k);
    let frame_settings = EditSettings { crop: None, ..req.settings.clone() };
    let frame =
        crate::edit::render_geometry(image::DynamicImage::ImageRgb8((*small).clone()), &frame_settings).into_rgb8();
    let g = Geometry::new(small.width(), small.height(), &frame_settings);
    let boxes: Vec<crate::faces::BBox> = {
        let conn = app.db();
        let mut stmt = conn.prepare(
            "SELECT f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h FROM faces f JOIN photos p ON p.current_revision_id = f.photo_revision_id
             WHERE p.id = ?1 AND f.status = 'active'",
        )?;
        let rows = stmt.query_map([&req.photo_id], |r| {
            Ok(crate::faces::BBox { x: r.get(0)?, y: r.get(1)?, w: r.get(2)?, h: r.get(3)? })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let faces: Vec<[f64; 4]> = boxes.into_iter().filter_map(|b| g.map_box(b)).map(|b| [b.x, b.y, b.w, b.h]).collect();
    let crops = crate::compose::suggest_crops(&frame, &faces, req.ratio.filter(|r| r.is_finite() && *r > 0.0));
    let straighten = tilt.map(|t| (-t.degrees).clamp(-crate::edit::MAX_STRAIGHTEN, crate::edit::MAX_STRAIGHTEN));
    Ok(Composition { straighten, tilt, crops })
}

/// ค่าแสง — แตะเฉพาะภาพที่ค่ากลางความสว่างมืดกว่า 30% หรือสว่างกว่า 62% และไม่เกิน ±1 EV · จุดดำ/จุดขาว — ยืดช่วงให้เต็ม
/// (0.5% และ 99.5% หลังปรับค่าแสง) ไม่เกิน ±60 · นับทุกพิกเซลที่สี่ของภาพ RGBA
fn auto_light(rgba: &[u8], profile: Profile) -> (f64, f64, f64) {
    let mut hist = [0f64; 256];
    let mut count = 0f64;
    for p in rgba.as_chunks::<4>().0.iter().step_by(4) {
        let luma = 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
        hist[js_round(luma) as usize] += 1.0;
        count += 1.0;
    }
    if count == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let percentile = |h: &[f64; 256], p: f64| {
        let mut seen = 0.0;
        for (v, n) in h.iter().enumerate() {
            seen += n;
            if seen >= count * p {
                return v as f64 / 255.0;
            }
        }
        1.0
    };
    let round = |v: f64, step: f64| js_round(v / step) * step;
    let median = percentile(&hist, 0.5).clamp(0.02, 0.98);
    let target = if median < 0.3 {
        0.36
    } else if median > 0.62 {
        0.56
    } else {
        median
    };
    // ค่าแสงทำในแสงเชิงเส้น: RAW ถอดกลับด้วยเส้นโค้งโทนตั้งต้น
    let decode = |v: f64| crate::adjust::decode_for(profile, v);
    let exposure = round((decode(target) / decode(median)).log2(), 0.05).clamp(-1.0, 1.0);
    // ฮิสโตแกรมหลังปรับค่าแสง แล้วจึงหาจุดดำ/จุดขาว
    let table = Adjust { exposure, ..Default::default() }.curves_for(profile)[0];
    let mut shifted = [0f64; 256];
    for (v, n) in hist.iter().enumerate() {
        shifted[js_round(table[v] * 255.0) as usize] += n;
    }
    let (low, high) = (percentile(&shifted, 0.005), percentile(&shifted, 0.995));
    (exposure, round((1.0 - high) * 1000.0, 1.0).min(60.0), round(-low * 1000.0, 1.0).max(-60.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn test_source(w: u32, h: u32, profile: Profile) -> Source {
        let img = RgbImage::from_raw(w, h, crate::local::test_image(w as usize, h as usize)).unwrap();
        Source { key: ("p".into(), SourceKind::Original, "r".into()), profile, levels: Mutex::new(vec![Arc::new(img)]) }
    }

    fn request(settings: EditSettings, width: u32, height: u32, view: Rect) -> Request {
        Request {
            photo_id: "p".into(),
            source: SourceKind::Original,
            settings,
            width,
            height,
            view,
            whole: true,
            adjust: true,
            show: None,
        }
    }

    #[test]
    fn at_full_size_the_preview_is_the_saved_image_pixel_for_pixel() {
        // ภาพต้นทางเต็มขนาด ไม่ตัด/หมุน และหมุนทีละ 90° (พิกเซลตรงกันพอดี ไม่ต้องสุ่มใหม่): ภาพตัวอย่าง = edit::render_with ทุกบิต
        let src = test_source(48, 32, Profile::Display);
        let img = DynamicImage::ImageRgb8((*src.level(0)).clone());
        let adjust = Adjust {
            exposure: 0.4,
            contrast: 20.0,
            texture: -40.0,
            clarity: 30.0,
            dehaze: 15.0,
            saturation: 10.0,
            ..Default::default()
        };
        let mask = crate::masks::Mask {
            id: "m".into(),
            area: crate::masks::Area::Faces { boxes: vec![[0.3, 0.2, 0.35, 0.5]], skin: false },
            adjust: Adjust { exposure: 0.6, texture: -40.0, ..Default::default() },
            amount: 100.0,
            invert: false,
        };
        for rotate in [0, 90, 180, 270] {
            let s = EditSettings { rotate, adjust: adjust.clone(), masks: vec![mask.clone()], ..Default::default() };
            let saved = crate::edit::render_with(img.clone(), &s, Profile::Display).into_rgba8();
            let shown = render_from(&src, &request(s.clone(), saved.width(), saved.height(), FULL));
            assert_eq!(shown, saved.into_raw(), "หมุน {rotate}");
        }
    }

    #[test]
    fn a_cropped_view_shows_exactly_the_cropped_part() {
        // ตัดครึ่งซ้ายพอดีพิกเซล: canvas 24×32 ของส่วนที่ตัด = ภาพเต็มที่ตัดแล้ว (ก่อนปรับ)
        let src = test_source(48, 32, Profile::Display);
        let crop = Rect { x: 0.0, y: 0.0, w: 0.5, h: 1.0 };
        let s = EditSettings { crop: Some(crop), ..Default::default() };
        let mut req = request(s, 24, 32, crop);
        req.adjust = false;
        let shown = render_from(&src, &req);
        let full = crate::local::test_image(48, 32);
        for y in 0..32 {
            for x in 0..24 {
                assert_eq!(
                    &shown[(y * 24 + x) * 4..(y * 24 + x) * 4 + 3],
                    &full[(y * 48 + x) * 3..(y * 48 + x) * 3 + 3]
                );
            }
        }
    }

    #[test]
    fn crop_mode_has_no_vignette_and_before_shows_the_plain_image() {
        let src = test_source(48, 32, Profile::Display);
        let a =
            Adjust { vignette: crate::adjust::Vignette { amount: -80.0, ..Default::default() }, ..Default::default() };
        let s = EditSettings { adjust: a, ..Default::default() };
        let plain_px = render_from(&src, &Request { adjust: false, ..request(s.clone(), 48, 32, FULL) });
        let crop_mode = render_from(&src, &Request { whole: false, ..request(s.clone(), 48, 32, FULL) });
        assert_eq!(crop_mode, plain_px, "แท็บตัดและหมุนไม่ใส่ Vignette");
        let framed = render_from(&src, &request(s, 48, 32, FULL));
        assert!(framed[0] < plain_px[0], "แท็บแสงและสีใส่ Vignette (มุมมืดลง)");
    }

    #[test]
    fn showing_an_area_tints_it_red_even_before_any_adjustment() {
        let src = test_source(48, 32, Profile::Display);
        let mask = crate::masks::Mask {
            id: "m".into(),
            area: crate::masks::Area::Faces { boxes: vec![[0.3, 0.2, 0.35, 0.5]], skin: false },
            adjust: Adjust::default(),
            amount: 100.0,
            invert: false,
        };
        let s = EditSettings { masks: vec![mask], ..Default::default() };
        let plain_px = render_from(&src, &request(s.clone(), 48, 32, FULL));
        let shown = render_from(&src, &Request { show: Some("m".into()), ..request(s, 48, 32, FULL) });
        let i = (16 * 48 + 22) * 4;
        assert!(shown[i] > plain_px[i] && shown[i + 1] < plain_px[i + 1], "กลางบริเวณเป็นสีแดงขึ้น");
        assert_eq!(&shown[..4], &plain_px[..4], "นอกบริเวณเท่าเดิม");
    }

    #[test]
    fn a_small_canvas_uses_a_smoothly_reduced_image() {
        // canvas เล็กกว่าภาพ 8 เท่า: ภาพลายตารางละเอียดต้องออกมาเป็นสีเฉลี่ย ไม่ใช่จุดใดจุดหนึ่งของลาย
        let img = RgbImage::from_fn(256, 256, |x, y| {
            if (x + y) % 2 == 0 {
                image::Rgb([255, 255, 255])
            } else {
                image::Rgb([0, 0, 0])
            }
        });
        let src = Source {
            key: ("p".into(), SourceKind::Original, "r".into()),
            profile: Profile::Display,
            levels: Mutex::new(vec![Arc::new(img)]),
        };
        let shown = render_from(&src, &Request { adjust: false, ..request(EditSettings::default(), 32, 32, FULL) });
        assert!(shown.as_chunks::<4>().0.iter().all(|p| (120..=136).contains(&p[0])), "{:?}", &shown[..8]);
    }

    #[test]
    fn auto_and_white_balance_give_the_values_the_editor_gave_before() {
        // ค่าเดียวกับ autoAdjust / neutralize ของหน้าเว็บก่อนย้ายมา Rust (คิดด้วย adjust.ts เดิมบนภาพเดียวกัน)
        let n = 80 * 60;
        let img = |f: &dyn Fn(f64) -> f64| -> Vec<u8> {
            (0..n)
                .flat_map(|i| {
                    let v = (255.0 * f(((i * 37) % n) as f64 / (n - 1) as f64)).floor() as u8;
                    [v, v, v, 255]
                })
                .collect()
        };
        type Case<'a> = (&'a dyn Fn(f64) -> f64, Profile, (f64, f64, f64));
        let cases: [Case; 4] = [
            (&|t| t * t, Profile::Display, (1.0, 0.0, 0.0)),
            (&|t| t * t, Profile::Raw, (0.75, 0.0, 0.0)),
            (&|t: f64| t.sqrt(), Profile::Display, (-0.75, 60.0, -39.0)),
            (&|t| 0.1 + 0.8 * t, Profile::Display, (0.0, 60.0, -60.0)),
        ];
        for (f, profile, want) in cases {
            assert_eq!(auto_light(&img(f), profile), want, "{profile:?}");
        }
        let wb = |rgb: [f64; 3], profile| {
            let (t, ti) = crate::adjust::neutral_white_balance(rgb, profile);
            (js_round(t), js_round(ti))
        };
        assert_eq!(wb([150.0, 160.0, 200.0], Profile::Display), (100.0, -42.0));
        assert_eq!(wb([180.0, 200.0, 170.0], Profile::Display), (-25.0, 100.0));
        assert_eq!(wb([200.0, 180.0, 150.0], Profile::Display), (-100.0, 25.0));
        assert_eq!(wb([120.0, 130.0, 160.0], Profile::Display), (100.0, -26.0));
        assert_eq!(wb([90.0, 100.0, 95.0], Profile::Display), (23.0, 76.0));
        assert_eq!(wb([150.0, 160.0, 200.0], Profile::Raw), (100.0, -48.0));
        assert_eq!(wb([200.0, 180.0, 150.0], Profile::Raw), (-100.0, -3.0));
    }
}
