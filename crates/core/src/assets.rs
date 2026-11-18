//! เสิร์ฟภาพให้ UI ด้วย ID ทึบ (Blueprint 7.3): ไม่มี path จากฝั่ง UI และไม่เปิดสิทธิ์อ่านทั้งดิสก์
//!   thumb/{photoId} · photo/{photoId} · face/{faceId} · draft/{previewId}/{draftId}

use crate::error::{AppError, Result};
use crate::{media, App};
use rusqlite::OptionalExtension;
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
            let path = PathBuf::from(crate::catalog::photo_path(&app.db(), id)?);
            Ok(Asset { bytes: std::fs::read(&path)?, mime: media::mime_for(&path) })
        }
        ("face", None) => face(app, id),
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

fn thumb(app: &App, photo_id: &str) -> Result<Asset> {
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
