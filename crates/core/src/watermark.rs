//! ลายน้ำของสำเนาที่ส่งออก (DECISIONS ข้อ 16)
//!
//! โลโก้ที่ผู้ใช้เพิ่มเก็บเป็น PNG ในตาราง `watermarks` ของคลัง (ชุดสำรองของคลังจึงมีครบ) และเลือกใช้ซ้ำได้
//! ลายน้ำใส่เฉพาะสำเนาที่ส่งออก — ไม่มีทางใส่ลงไฟล์ต้นฉบับ

use crate::error::{AppError, Result};
use crate::{media, util, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// ไฟล์โลโก้ใหญ่สุดที่รับ
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
/// โลโก้ที่ใหญ่กว่านี้ถูกย่อก่อนเก็บ: ลายน้ำไม่ต้องละเอียดกว่านี้ และไม่ให้คลังโตโดยไม่จำเป็น
const MAX_EDGE: u32 = 2048;

/// ตำแหน่งลายน้ำบนภาพ เก้าจุด
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    fn grid(self) -> (u8, u8) {
        use Anchor::*;
        match self {
            TopLeft => (0, 0),
            TopCenter => (1, 0),
            TopRight => (2, 0),
            CenterLeft => (0, 1),
            Center => (1, 1),
            CenterRight => (2, 1),
            BottomLeft => (0, 2),
            BottomCenter => (1, 2),
            BottomRight => (2, 2),
        }
    }
}

/// ลายน้ำที่เลือกสำหรับงานส่งออกหนึ่งงาน
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkSpec {
    pub id: String,
    pub anchor: Anchor,
    /// ด้านยาวของโลโก้ เป็นเปอร์เซ็นต์ของด้านสั้นของภาพ
    pub scale: u32,
    /// ความทึบ (100 = ทึบเต็มที่)
    pub opacity: u32,
}

impl WatermarkSpec {
    pub(crate) fn validate(&self, conn: &Connection) -> Result<()> {
        if !(5..=100).contains(&self.scale) {
            return Err(AppError::invalid("ขนาดลายน้ำต้องอยู่ระหว่าง 5% ถึง 100%"));
        }
        if !(5..=100).contains(&self.opacity) {
            return Err(AppError::invalid("ความทึบของลายน้ำต้องอยู่ระหว่าง 5% ถึง 100%"));
        }
        if !conn
            .query_row("SELECT EXISTS(SELECT 1 FROM watermarks WHERE id = ?1)", [&self.id], |r| r.get::<_, bool>(0))?
        {
            return Err(not_found());
        }
        Ok(())
    }

    /// โลโก้ที่พร้อมวาด — คืน None เมื่อโลโก้ถูกลบไปแล้ว
    pub(crate) fn load(&self, conn: &Connection) -> Result<Option<media::Watermark>> {
        let Some(bytes) = image_bytes(conn, &self.id)? else { return Ok(None) };
        Ok(Some(media::Watermark::new(&bytes, self.anchor.grid(), self.scale, self.opacity)?))
    }
}

fn not_found() -> AppError {
    AppError::new("NOT_FOUND", "ไม่พบลายน้ำที่เลือก อาจถูกลบไปแล้ว กรุณาเลือกลายน้ำอีกครั้ง")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// มีส่วนโปร่งใส (PNG พื้นใส) — ไม่มี = จะเห็นเป็นกรอบสี่เหลี่ยมทับรูป
    pub transparent: bool,
}

pub fn list(conn: &Connection) -> Result<Vec<WatermarkInfo>> {
    Ok(conn
        .prepare("SELECT id, name, width, height, transparent FROM watermarks ORDER BY created_at, id")?
        .query_map([], |r| {
            Ok(WatermarkInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                width: r.get(2)?,
                height: r.get(3)?,
                transparent: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// เพิ่มโลโก้จากไฟล์ที่ผู้ใช้เลือก (PNG หรือ JPEG) — เก็บเป็น PNG ในคลัง ไฟล์ต้นทางไม่ถูกแตะ
pub fn add(app: &App, path: &str) -> Result<WatermarkInfo> {
    let path = Path::new(path);
    let meta = std::fs::metadata(path).map_err(|_| AppError::new("SOURCE_UNAVAILABLE", "ไม่พบไฟล์ที่เลือก"))?;
    if !meta.is_file() {
        return Err(AppError::invalid("กรุณาเลือกไฟล์รูป (PNG หรือ JPEG)"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::invalid("ไฟล์ลายน้ำใหญ่เกิน 20 MB"));
    }
    let bytes = std::fs::read(path)?;
    let mut img = image::load_from_memory(&bytes)
        .map_err(|_| AppError::new("UNSUPPORTED_FORMAT", "เปิดไฟล์นี้เป็นรูปไม่ได้ ลายน้ำต้องเป็นไฟล์ PNG หรือ JPEG"))?;
    if img.width().max(img.height()) > MAX_EDGE {
        img = img.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8();
    let transparent = rgba.pixels().any(|p| p.0[3] < 250);
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(AppError::internal)?;
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "ลายน้ำ".into());
    let info = WatermarkInfo { id: util::new_id(), name, width: img.width(), height: img.height(), transparent };
    app.db().execute(
        "INSERT INTO watermarks(id, name, width, height, transparent, image, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![info.id, info.name, info.width, info.height, info.transparent, png, util::now()],
    )?;
    Ok(info)
}

/// ลบออกจากรายการ — งานส่งออกที่ยังไม่เสร็จและใช้ลายน้ำนี้จะหยุดรอพร้อมบอกเหตุผล
pub fn delete(app: &App, id: &str) -> Result<()> {
    if app.db().execute("DELETE FROM watermarks WHERE id = ?1", [id])? == 0 {
        return Err(not_found());
    }
    Ok(())
}

pub(crate) fn image_bytes(conn: &Connection, id: &str) -> Result<Option<Vec<u8>>> {
    Ok(conn.query_row("SELECT image FROM watermarks WHERE id = ?1", [id], |r| r.get(0)).optional()?)
}
