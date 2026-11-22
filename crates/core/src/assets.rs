//! เสิร์ฟภาพให้ UI ด้วย ID ทึบ (Blueprint 7.3): ไม่มี path จากฝั่ง UI และไม่เปิดสิทธิ์อ่านทั้งดิสก์
//!   thumb/{photoId} · photo/{photoId} · original/{photoId} · rawframe/{photoId} · face/{faceId}
//!   · draft/{previewId}/{draftId} · watermark/{watermarkId}
//! รูปที่แต่งแล้ว: thumb/photo เป็นภาพที่แต่ง ส่วน original เป็นไฟล์ต้นฉบับเสมอ (ใช้ในหน้าแต่งภาพ)
//! · rawframe = ภาพจากไฟล์ RAW คู่ในกรอบของ JPEG (ต้นทางของหน้าแต่งภาพเมื่อแต่งจาก RAW — DECISIONS ข้อ 26)

use crate::error::{AppError, Result};
use crate::{media, App};
use rusqlite::OptionalExtension;
use sha2::Digest;
use std::path::{Path, PathBuf};

pub struct Asset {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

fn jpeg(bytes: Vec<u8>) -> Asset {
    Asset { bytes, mime: "image/jpeg" }
}

pub fn resolve(app: &App, request_path: &str) -> Result<Asset> {
    let path = request_path.split('?').next().unwrap_or("").trim_matches('/');
    let mut parts = path.split('/');
    let (kind, id, extra) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next());
    if id.is_empty() || parts.next().is_some() {
        return Err(AppError::invalid("คำขอรูปไม่ถูกต้อง"));
    }
    match (kind, extra) {
        ("thumb", None) => thumb(app, id),
        ("photo", None) => {
            // อ่านค่าที่แต่งให้เสร็จก่อน (ปล่อยฐานข้อมูล) เพราะ edited() ต้องใช้ฐานข้อมูลอีก
            let stored = crate::edit::load(&app.db(), id)?;
            match stored {
                Some(stored) => edited(app, id, &stored, "view"),
                None => original(app, id),
            }
        }
        ("original", None) => original(app, id),
        ("rawframe", None) => raw_frame(app, id),
        ("face", None) => face(app, id),
        ("watermark", None) => Ok(Asset {
            bytes: crate::watermark::image_bytes(&app.db(), id)?.ok_or_else(|| AppError::not_found("ลายน้ำ"))?,
            mime: "image/png",
        }),
        ("sample", None) => {
            let bytes: Vec<u8> = app
                .db()
                .query_row("SELECT thumbnail FROM person_reference_samples WHERE id = ?1", [id], |r| r.get(0))
                .optional()?
                .ok_or_else(|| AppError::not_found("ใบหน้าตัวอย่าง"))?;
            Ok(jpeg(bytes))
        }
        ("draft", Some(draft_id)) => {
            let session = app
                .previews
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(id)
                .cloned()
                .ok_or_else(|| AppError::not_found("ผลตรวจโฟลเดอร์"))?;
            Ok(jpeg(session.sample_jpeg(draft_id)?.as_ref().clone()))
        }
        _ => Err(AppError::invalid("คำขอรูปไม่ถูกต้อง")),
    }
}

fn original(app: &App, photo_id: &str) -> Result<Asset> {
    let path = PathBuf::from(crate::catalog::photo_path(&app.db(), photo_id)?);
    if crate::raw::is_raw_path(&path) {
        return raw_view(app, photo_id, &path);
    }
    Ok(Asset { bytes: std::fs::read(&path)?, mime: media::mime_for(&path) })
}

/// "ต้นฉบับ" ของรูป RAW สำหรับ UI: โทนตั้งต้น JPEG ด้านยาว [`crate::edit::VIEW_EDGE`] (WebView เปิดไฟล์ RAW ไม่ได้)
/// — ตัวดูภาพใช้แสดง และหน้าแต่งภาพใช้เป็นต้นทางของตัวอย่างสด (DECISIONS ข้อ 25) · เก็บใน cache ตาม revision
fn raw_view(app: &App, photo_id: &str, path: &Path) -> Result<Asset> {
    let revision_id: String =
        app.db().query_row("SELECT current_revision_id FROM photos WHERE id = ?1", [photo_id], |r| r.get(0))?;
    let cached = app.cache_dir.join("raw").join(format!("{revision_id}-view.jpg"));
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(jpeg(bytes));
    }
    let original = media::read_file(path).map_err(|_| AppError::source_unavailable())?;
    let (img, _) = crate::raw::base_image(&original, crate::edit::VIEW_EDGE)?;
    let bytes = media::encode_jpeg(&image::DynamicImage::ImageRgb8(img), 92)?;
    let _ = media::write_cache_file(&cached, &bytes);
    Ok(jpeg(bytes))
}

/// ต้นทางของหน้าแต่งภาพเมื่อแต่งจาก RAW คู่: ภาพจาก RAW ในกรอบของ JPEG โทนตั้งต้น ด้านยาว [`crate::edit::VIEW_EDGE`]
/// (ตัวอย่างสดถอดกลับเป็นแสงเชิงเส้นแบบเดียวกับรูป RAW) · เก็บใน cache ตาม revision ไฟล์ RAW และกรอบ
fn raw_frame(app: &App, photo_id: &str) -> Result<Asset> {
    crate::pair::ensure_start(app, photo_id)?;
    let (base, revision_id) = raw_frame_base(&app.db(), photo_id)?;
    Ok(jpeg(raw_frame_bytes(&app.cache_dir, &base, &revision_id)?))
}

/// ไฟล์ RAW คู่และกรอบของรูป (อ่านฐานข้อมูลอย่างเดียว — ถอดไฟล์ทีหลังโดยไม่ถือฐานข้อมูล)
pub(crate) fn raw_frame_base(
    conn: &rusqlite::Connection,
    photo_id: &str,
) -> Result<(Box<crate::pair::RawBase>, String)> {
    let base = match crate::pair::raw_edit(conn, photo_id)? {
        crate::pair::RawEdit::Ready(base) => base,
        crate::pair::RawEdit::Unusable(why) => return Err(AppError::new("SOURCE_UNAVAILABLE", why)),
        crate::pair::RawEdit::None => return Err(AppError::not_found("ไฟล์ RAW คู่")),
    };
    let revision_id: String =
        conn.query_row("SELECT current_revision_id FROM photos WHERE id = ?1", [photo_id], |r| r.get(0))?;
    Ok((base, revision_id))
}

/// JPEG ของภาพจาก RAW คู่ในกรอบของ JPEG โทนตั้งต้น (ต้นทางของหน้าแต่งภาพ และภาพตั้งต้นของตัวผอม)
pub(crate) fn raw_frame_bytes(cache_dir: &Path, base: &crate::pair::RawBase, revision_id: &str) -> Result<Vec<u8>> {
    let stamp = media::stamp(&base.raw_path).map_err(|_| AppError::source_unavailable())?;
    let frame = serde_json::to_vec(&base.frame)?;
    let key = &crate::util::hex(&sha2::Sha256::digest(&frame))[..12];
    let cached = cache_dir.join("raw").join(format!("{revision_id}-{}-{}-{key}-frame.jpg", stamp.size, stamp.mtime_ns));
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(bytes);
    }
    let raw = media::read_file(&base.raw_path).map_err(|_| AppError::source_unavailable())?;
    let plain = crate::edit::EditSettings { from_raw: true, ..Default::default() };
    let img = crate::pair::render(&raw, base, &plain, crate::edit::VIEW_EDGE)?;
    let bytes = media::encode_jpeg(&image::DynamicImage::ImageRgb8(img), 92)?;
    let _ = media::write_cache_file(&cached, &bytes);
    Ok(bytes)
}

/// ภาพที่แต่งแล้ว (เก็บใน cache ตามรหัสการแต่ง): ภาพย่อ หรือภาพสำหรับตัวดูภาพ
fn edited(app: &App, photo_id: &str, stored: &crate::edit::Stored, kind: &str) -> Result<Asset> {
    let cached = crate::edit::cache_path(&app.cache_dir, photo_id, &stored.edit_id, kind);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(jpeg(bytes));
    }
    let (edge, quality) = if kind == "thumb" { (media::THUMB_EDGE, 82) } else { (crate::edit::VIEW_EDGE, 90) };
    // Mask ตัวคน (DECISIONS ข้อ 40): ภาพสำหรับตัวดูภาพคิดภาพน้ำหนักเลยถ้ายังไม่มี (ราว 7 วินาที ครั้งเดียวต่อรูป)
    // ภาพย่อใช้เฉพาะที่คิดไว้แล้ว (ตารางรูปไม่ต้องรอ) — ยังไม่มีภาพน้ำหนัก = ภาพที่ได้ยังไม่มีผลของ Mask นั้น ไม่เก็บลง cache
    let mut settings = stored.settings.clone();
    let complete = crate::subject::prepare(app, photo_id, &mut settings, kind != "thumb");
    let store = |bytes: &[u8]| {
        if complete {
            let _ = media::write_cache_file(&cached, bytes);
        }
    };
    let stored = crate::edit::Stored { edit_id: stored.edit_id.clone(), settings };
    let stored = &stored;
    if stored.settings.from_raw {
        crate::pair::ensure_start(app, photo_id)?;
        let raw = { crate::pair::raw_edit(&app.db(), photo_id)? };
        if let crate::pair::RawEdit::Ready(base) = raw {
            if let Ok(raw_bytes) = media::read_file(&base.raw_path) {
                let img = crate::pair::render(&raw_bytes, &base, &stored.settings, edge)?;
                let bytes = media::encode_jpeg(&image::DynamicImage::ImageRgb8(img), quality)?;
                store(&bytes);
                return Ok(jpeg(bytes));
            }
        }
        // ไฟล์ RAW คู่ไม่พร้อม: แสดง JPEG ตามเดิม (ค่าที่แต่งเป็นของ RAW ใช้กับ JPEG ไม่ได้) — ไม่เก็บลง cache
        return if kind == "thumb" { plain_thumb(app, photo_id) } else { original(app, photo_id) };
    }
    let source = PathBuf::from(crate::catalog::photo_path(&app.db(), photo_id)?);
    let profile = profile_of(&source);
    let bytes = match media::read_file(&source) {
        Ok(original) => crate::edit::preview_jpeg(&original, &stored.settings, edge, quality, profile)?,
        // ต้นฉบับไม่พร้อม (ไดรฟ์ไม่ได้ต่อ): แต่งจากภาพย่อของแอปแทน ภาพเล็กลงแต่ตัด/หมุนถูก — ไม่เก็บลง cache
        Err(_) => {
            let thumb = plain_thumb(app, photo_id)?;
            return Ok(jpeg(crate::edit::preview_jpeg(&thumb.bytes, &stored.settings, edge, quality, profile)?));
        }
    };
    store(&bytes);
    Ok(jpeg(bytes))
}

/// ชนิดของภาพต้นทางจากนามสกุลไฟล์ (ภาพ 8 บิตของรูป RAW เช่นภาพย่อ ต้องแต่งด้วยสูตรของ RAW)
pub(crate) fn profile_of(path: &Path) -> crate::adjust::Profile {
    if crate::raw::is_raw_path(path) {
        crate::adjust::Profile::Raw
    } else {
        crate::adjust::Profile::Display
    }
}

/// ไฟล์ต้นฉบับ หรือภาพย่อเดิมของแอป (หมุนตั้งตรงแล้ว ยังไม่แต่ง) เมื่อต้นฉบับไม่พร้อม — สำหรับคำนวณจากภาพก่อนแต่ง
pub(crate) fn source_or_plain_thumb(app: &App, photo_id: &str) -> Result<Vec<u8>> {
    let source = PathBuf::from(crate::catalog::photo_path(&app.db(), photo_id)?);
    match media::read_file(&source) {
        Ok(bytes) => Ok(bytes),
        Err(_) => Ok(plain_thumb(app, photo_id)?.bytes),
    }
}

fn thumb(app: &App, photo_id: &str) -> Result<Asset> {
    let stored = crate::edit::load(&app.db(), photo_id)?;
    if let Some(stored) = stored {
        // ภาพย่อของรูปที่แต่ง: ถ้ายังไม่เคยสร้าง อ่านต้นฉบับ (หรือภาพย่อเดิมเมื่อต้นฉบับไม่พร้อม)
        return edited(app, photo_id, &stored, "thumb");
    }
    plain_thumb(app, photo_id)
}

fn plain_thumb(app: &App, photo_id: &str) -> Result<Asset> {
    let (revision_id, source): (String, String) = app
        .db()
        .query_row(
            "SELECT p.current_revision_id, l.os_path FROM photos p JOIN photo_locations l ON l.photo_id = p.id WHERE p.id = ?1",
            [photo_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("รูป"))?;
    let cached = app.thumb_path(&revision_id);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(jpeg(bytes));
    }
    // cache ถูกล้าง: สร้างใหม่ได้เมื่อต้นทางพร้อม (ภาพย่อเป็นข้อมูลที่สร้างใหม่ได้เสมอ)
    let bytes = media::thumbnail_only(&media::read_file(Path::new(&source))?, media::THUMB_EDGE)?;
    let _ = media::write_cache_file(&cached, &bytes);
    Ok(jpeg(bytes))
}

fn face(app: &App, face_id: &str) -> Result<Asset> {
    let cached = app.face_crop_path(face_id);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(jpeg(bytes));
    }
    let (bbox, revision_id, source): ((f64, f64, f64, f64), String, String) = app
        .db()
        .query_row(
            "SELECT f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.photo_revision_id, l.os_path
             FROM faces f JOIN photo_revisions r ON r.id = f.photo_revision_id JOIN photo_locations l ON l.photo_id = r.photo_id
             WHERE f.id = ?1",
            [face_id],
            |r| Ok(((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?), r.get(4)?, r.get(5)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("ใบหน้า"))?;
    if let Ok(original) = media::read_file(Path::new(&source)) {
        let bytes = media::face_crop(&original, bbox, false)?;
        let _ = media::write_cache_file(&cached, &bytes);
        return Ok(jpeg(bytes));
    }
    // ต้นทางไม่พร้อม: ครอปจากภาพย่อไปก่อน และไม่เก็บลง cache เพื่อให้ได้ภาพคมเมื่อต้นทางกลับมา
    let thumb = std::fs::read(app.thumb_path(&revision_id)).map_err(|_| AppError::source_unavailable())?;
    Ok(jpeg(media::face_crop(&thumb, bbox, true)?))
}
