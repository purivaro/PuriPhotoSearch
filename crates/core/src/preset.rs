//! preset ของการแต่งภาพ (DECISIONS ข้อ 20): ค่าแสงและสีที่บันทึกไว้ใช้ซ้ำ และนำเข้าจาก preset `.xmp` ของ Lightroom
//!
//! การนำเข้าอ่านเฉพาะค่าที่ตัวแต่งภาพของเราทำได้ (แสงพื้นฐาน Texture Clarity Dehaze ความสด/ความอิ่ม HSL เส้นโค้งโทน Parametric curve
//! สมดุลแสงขาว ขาวดำ Color Grading Vignette ความคม และ Mask ตัวคน — DECISIONS ข้อ 40) ที่เหลือ (Profile, Mask แบบอื่น, ลดนอยส์ …)
//! บันทึกเป็นรายการ "นำเข้าไม่ได้" ให้ผู้ใช้เห็นตามจริง
//! ผลจึงใกล้เคียงกับ Lightroom ไม่ใช่เท่ากันทุกพิกเซล — สูตรของ Adobe ไม่เปิดเผย

use crate::adjust::{Adjust, Grading, Vignette, Wheel};
use crate::error::{AppError, Result};
use crate::masks::{Area, Mask};
use crate::{util, App};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

const MAX_NAME: usize = 80;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub adjust: Adjust,
    /// Mask ตัวคนของ preset (ใช้ preset = เพิ่ม Mask เหล่านี้ให้รูป แทนอันที่รหัสเดียวกัน)
    pub masks: Vec<Mask>,
    /// user | lightroom
    pub source: String,
    /// ส่วนของ preset ที่นำเข้าไม่ได้
    pub unsupported: Vec<String>,
}

pub fn list(conn: &Connection) -> Result<Vec<Preset>> {
    let rows: Vec<(String, String, String, String, String, String)> = conn
        .prepare(
            "SELECT id, name, adjust, source, unsupported, masks FROM edit_presets ORDER BY name COLLATE NOCASE, created_at",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    rows.into_iter()
        .map(|(id, name, adjust, source, unsupported, masks)| {
            Ok(Preset {
                id,
                name,
                adjust: serde_json::from_str(&adjust)?,
                masks: serde_json::from_str(&masks).unwrap_or_default(),
                source,
                unsupported: serde_json::from_str(&unsupported).unwrap_or_default(),
            })
        })
        .collect()
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("ตั้งชื่อ preset ก่อนบันทึก"));
    }
    Ok(name.chars().take(MAX_NAME).collect())
}

fn insert(
    app: &App,
    name: &str,
    adjust: &Adjust,
    masks: &[Mask],
    source: &str,
    unsupported: &[String],
) -> Result<Preset> {
    adjust.validate()?;
    // preset เก็บเฉพาะ Mask ตัวคน: Mask ใบหน้าผูกกับใบหน้าของรูปนั้น
    let masks: Vec<Mask> = masks
        .iter()
        .filter(|m| matches!(m.area, Area::Subject { .. }) && m.is_active())
        .map(|m| Mask { area: Area::Subject { map: None }, ..m.clone() })
        .collect();
    crate::masks::validate(&masks)?;
    if adjust.is_neutral() && masks.is_empty() {
        return Err(AppError::invalid("preset นี้ไม่ได้ปรับแสงหรือสีเลย"));
    }
    let preset = Preset {
        id: util::new_id(),
        name: clean_name(name)?,
        adjust: adjust.clone(),
        masks,
        source: source.into(),
        unsupported: unsupported.to_vec(),
    };
    app.db().execute(
        "INSERT INTO edit_presets(id, name, adjust, source, unsupported, masks, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            preset.id,
            preset.name,
            serde_json::to_string(&preset.adjust)?,
            preset.source,
            serde_json::to_string(&preset.unsupported)?,
            serde_json::to_string(&preset.masks)?,
            util::now()
        ],
    )?;
    Ok(preset)
}

/// บันทึกค่าแสงและสีที่ปรับไว้เป็น preset (รวม Mask ตัวคนที่มีผล)
pub fn save(app: &App, name: &str, adjust: &Adjust, masks: &[Mask]) -> Result<Preset> {
    insert(app, name, adjust, masks, "user", &[])
}

pub fn rename(app: &App, id: &str, name: &str) -> Result<Vec<Preset>> {
    let name = clean_name(name)?;
    if app.db().execute("UPDATE edit_presets SET name = ?2 WHERE id = ?1", params![id, name])? == 0 {
        return Err(AppError::not_found("preset"));
    }
    list(&app.db())
}

/// ลบ preset — รูปที่ใช้ preset นี้ไปแล้วไม่เปลี่ยน (เก็บค่าที่แต่งไว้กับรูปเอง)
pub fn delete(app: &App, id: &str) -> Result<Vec<Preset>> {
    if app.db().execute("DELETE FROM edit_presets WHERE id = ?1", [id])? == 0 {
        return Err(AppError::not_found("preset"));
    }
    list(&app.db())
}

/// นำเข้า preset `.xmp` ของ Lightroom / Camera Raw — ไฟล์ต้นทางไม่ถูกแตะ
pub fn import_lightroom(app: &App, path: &str) -> Result<Preset> {
    let path = Path::new(path);
    let meta = std::fs::metadata(path).map_err(|_| AppError::new("SOURCE_UNAVAILABLE", "ไม่พบไฟล์ที่เลือก"))?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err(AppError::new("UNSUPPORTED_FORMAT", "ไฟล์นี้ไม่ใช่ preset .xmp ของ Lightroom"));
    }
    let text = std::fs::read_to_string(path).map_err(|_| AppError::new("UNSUPPORTED_FORMAT", "อ่านไฟล์ preset ไม่ได้"))?;
    let parsed = parse_lightroom(&text)?;
    let fallback = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Lightroom".into());
    insert(
        app,
        parsed.name.as_deref().unwrap_or(&fallback),
        &parsed.adjust,
        &parsed.masks,
        "lightroom",
        &parsed.unsupported,
    )
}

pub struct Parsed {
    pub name: Option<String>,
    pub adjust: Adjust,
    /// Mask ที่ทำตามได้ (ตัวคน)
    pub masks: Vec<crate::masks::Mask>,
    pub unsupported: Vec<String>,
}

/// ค่าของ `crs:Key="…"` ตัวแรกในไฟล์ (ค่าหลักของ preset อยู่ก่อนส่วน Mask/Look เสมอ)
fn attr<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("crs:{key}=\"");
    let mut from = 0;
    while let Some(i) = text[from..].find(&needle) {
        let at = from + i;
        // ต้องเป็นชื่อเต็ม (คั่นด้วยช่องว่าง) ไม่ใช่ท้ายของชื่ออื่น
        if at == 0 || text.as_bytes()[at - 1].is_ascii_whitespace() {
            let start = at + needle.len();
            let end = text[start..].find('"')? + start;
            return Some(&text[start..end]);
        }
        from = at + needle.len();
    }
    None
}

fn num(text: &str, key: &str) -> Option<f64> {
    attr(text, key).and_then(|v| v.trim().trim_start_matches('+').parse::<f64>().ok())
}

/// ข้อความใน `<rdf:li>` ตัวแรกของ element `<crs:Key>`
fn first_li(text: &str, key: &str) -> Option<String> {
    let block = element(text, key)?;
    let li = block.find("<rdf:li")?;
    let open = block[li..].find('>')? + li + 1;
    let close = block[open..].find("</rdf:li>")? + open;
    Some(block[open..close].trim().to_string())
}

fn element<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let open = format!("<crs:{key}>");
    let close = format!("</crs:{key}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(&text[start..end])
}

/// จุดของเส้นโค้ง `<crs:ToneCurvePV2012…>` ("x, y" ต่อบรรทัด) — เส้นตรงหรืออ่านไม่ได้ = ว่าง
fn curve(text: &str, key: &str) -> Vec<[f64; 2]> {
    let Some(block) = element(text, key) else { return vec![] };
    let mut points: Vec<[f64; 2]> = block
        .split("<rdf:li>")
        .skip(1)
        .filter_map(|part| {
            let value = part.split("</rdf:li>").next()?;
            let (x, y) = value.split_once(',')?;
            Some([x.trim().parse::<f64>().ok()?.clamp(0.0, 255.0), y.trim().parse::<f64>().ok()?.clamp(0.0, 255.0)])
        })
        .collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    points.dedup_by(|a, b| a[0] == b[0]);
    if points.len() < 2 || points.iter().all(|p| p[0] == p[1]) {
        return vec![];
    }
    points.truncate(32);
    points
}

fn signed(v: f64) -> String {
    if v > 0.0 {
        format!("+{v}")
    } else {
        format!("{v}")
    }
}

/// แปลง preset `.xmp` ของ Lightroom เป็นค่าแสงและสีของเรา
pub fn parse_lightroom(text: &str) -> Result<Parsed> {
    if !text.contains("crs:") {
        return Err(AppError::new("UNSUPPORTED_FORMAT", "ไฟล์นี้ไม่ใช่ preset .xmp ของ Lightroom"));
    }
    let mut a = Adjust::default();
    let mut unsupported = Vec::new();
    let clamp = |v: f64| v.clamp(-100.0, 100.0);
    a.exposure = num(text, "Exposure2012").unwrap_or(0.0).clamp(-3.0, 3.0);
    for (field, key) in [
        (&mut a.contrast, "Contrast2012"),
        (&mut a.highlights, "Highlights2012"),
        (&mut a.shadows, "Shadows2012"),
        (&mut a.whites, "Whites2012"),
        (&mut a.blacks, "Blacks2012"),
        (&mut a.vibrance, "Vibrance"),
        (&mut a.saturation, "Saturation"),
        (&mut a.texture, "Texture"),
        (&mut a.clarity, "Clarity2012"),
        (&mut a.dehaze, "Dehaze"),
    ] {
        *field = clamp(num(text, key).unwrap_or(0.0));
    }
    // สมดุลแสงขาว: preset ของ JPEG เก็บแบบเพิ่ม/ลด · แบบ Kelvin (ของ RAW) แปลงโดยประมาณ · "As Shot" = ไม่ปรับ
    if let Some(t) = num(text, "IncrementalTemperature") {
        a.temperature = clamp(t);
        a.tint = clamp(num(text, "IncrementalTint").unwrap_or(0.0) * 0.67);
    } else if attr(text, "WhiteBalance").is_some_and(|wb| wb != "As Shot") {
        if let Some(k) = num(text, "Temperature") {
            a.temperature = clamp((k - 5500.0) / 30.0);
            a.tint = clamp(num(text, "Tint").unwrap_or(0.0) * 0.67);
            unsupported.push(format!("อุณหภูมิสี {k:.0}K (แปลงโดยประมาณ)"));
        }
    }
    const COLORS: [&str; 8] = ["Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta"];
    for (i, color) in COLORS.iter().enumerate() {
        a.hue[i] = clamp(num(text, &format!("HueAdjustment{color}")).unwrap_or(0.0));
        a.sat[i] = clamp(num(text, &format!("SaturationAdjustment{color}")).unwrap_or(0.0));
        a.lum[i] = clamp(num(text, &format!("LuminanceAdjustment{color}")).unwrap_or(0.0));
    }
    a.parametric = ["ParametricShadows", "ParametricDarks", "ParametricLights", "ParametricHighlights"]
        .map(|k| clamp(num(text, k).unwrap_or(0.0)));
    let splits = ["ParametricShadowSplit", "ParametricMidtoneSplit", "ParametricHighlightSplit"].map(|k| num(text, k));
    if let [Some(s0), Some(s1), Some(s2)] = splits {
        if (s0, s1, s2) != (25.0, 50.0, 75.0) && s0 < s1 && s1 < s2 {
            a.parametric_split = Some([s0, s1, s2].map(|v| v.clamp(0.0, 100.0)));
        }
    }
    a.curve = curve(text, "ToneCurvePV2012");
    a.curve_red = curve(text, "ToneCurvePV2012Red");
    a.curve_green = curve(text, "ToneCurvePV2012Green");
    a.curve_blue = curve(text, "ToneCurvePV2012Blue");
    if attr(text, "ConvertToGrayscale").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        a.saturation = -100.0;
    }
    // Color Grading (Lightroom 10 ขึ้นไป ใช้ชื่อเดิมของ Split Toning สำหรับสีของเงา/ส่วนสว่างและ Balance)
    let wheel = |hue: &str, sat: &str, lum: Option<&str>| Wheel {
        hue: num(text, hue).unwrap_or(0.0).rem_euclid(360.0),
        sat: num(text, sat).unwrap_or(0.0).clamp(0.0, 100.0),
        lum: lum.and_then(|k| num(text, k)).unwrap_or(0.0).clamp(-100.0, 100.0),
    };
    a.grading = Grading {
        shadows: wheel("SplitToningShadowHue", "SplitToningShadowSaturation", Some("ColorGradeShadowLum")),
        midtones: wheel("ColorGradeMidtoneHue", "ColorGradeMidtoneSat", Some("ColorGradeMidtoneLum")),
        highlights: wheel("SplitToningHighlightHue", "SplitToningHighlightSaturation", Some("ColorGradeHighlightLum")),
        global: wheel("ColorGradeGlobalHue", "ColorGradeGlobalSat", Some("ColorGradeGlobalLum")),
        blending: num(text, "ColorGradeBlending").unwrap_or(50.0).clamp(0.0, 100.0),
        balance: clamp(num(text, "SplitToningBalance").unwrap_or(0.0)),
    };
    // ความคม (DECISIONS ข้อ 36): ค่าเดียวกับ Lightroom
    if let Some(amount) = num(text, "Sharpness").filter(|v| *v > 0.0) {
        a.sharpen = crate::adjust::Sharpen {
            amount: amount.clamp(0.0, 150.0),
            radius: num(text, "SharpenRadius").unwrap_or(1.0).clamp(0.5, 3.0),
            detail: num(text, "SharpenDetail").unwrap_or(25.0).clamp(0.0, 100.0),
            masking: num(text, "SharpenEdgeMasking").unwrap_or(0.0).clamp(0.0, 100.0),
        };
    }
    if let Some(amount) = num(text, "PostCropVignetteAmount").filter(|v| *v != 0.0) {
        a.vignette = Vignette {
            amount: clamp(amount),
            midpoint: num(text, "PostCropVignetteMidpoint").unwrap_or(50.0).clamp(0.0, 100.0),
            feather: num(text, "PostCropVignetteFeather").unwrap_or(50.0).clamp(0.0, 100.0),
            roundness: clamp(num(text, "PostCropVignetteRoundness").unwrap_or(0.0)),
        };
    }

    // ส่วนที่ทำตามไม่ได้: บอกเฉพาะที่ preset ตั้งค่าไว้จริง
    if let Some(look) = element(text, "Look").and_then(|block| attr(block, "Name")) {
        if !matches!(look, "Adobe Color" | "Adobe Standard") {
            unsupported.push(format!("Profile “{look}”"));
        }
    }
    // (ชื่อใน xmp, ชื่อที่แสดง, ค่าติดลบได้)
    for (key, label, bipolar) in [("GrainAmount", "Grain", false), ("LuminanceSmoothing", "ลดนอยส์", false)]
    {
        if let Some(v) = num(text, key).filter(|v| *v != 0.0) {
            unsupported.push(format!("{label} {}", if bipolar { signed(v) } else { format!("{v}") }));
        }
    }
    if attr(text, "LensProfileEnable") == Some("1") {
        unsupported.push("แก้เลนส์ (Lens Corrections)".into());
    }
    // ฟิลเตอร์ AI ของ Lightroom (Denoise / Raw Details / Super Resolution)
    if text.contains("<crs:Filters>") {
        unsupported.push(if text.contains("Filter/Title/Denoise") { "Denoise (AI)" } else { "Enhance (AI)" }.into());
    }
    // Mask: ตัวคน (Select Subject) ทำได้ (DECISIONS ข้อ 40) ที่เหลือบอกชื่อไว้
    let (masks, skipped) = lightroom_masks(text);
    unsupported.extend(skipped);

    Ok(Parsed { name: first_li(text, "Name"), adjust: a, masks, unsupported })
}

/// ค่าของ Mask ใน XMP (−1 ถึง 1) → ค่าของเรา: (ชื่อใน xmp, ตัวคูณ)
const LOCAL_SLIDERS: [(&str, f64); 12] = [
    // Lightroom เก็บค่าแสงของ Mask เป็นสัดส่วนของช่วง ±4 stop (ค่าอื่น ×100 เหมือน Texture/Clarity ที่ตรวจกับ preset จริงแล้ว)
    ("LocalExposure2012", 4.0),
    ("LocalContrast2012", 100.0),
    ("LocalHighlights2012", 100.0),
    ("LocalShadows2012", 100.0),
    ("LocalWhites2012", 100.0),
    ("LocalBlacks2012", 100.0),
    ("LocalTexture", 100.0),
    ("LocalClarity2012", 100.0),
    ("LocalDehaze", 100.0),
    ("LocalSaturation", 100.0),
    ("LocalTemperature", 100.0),
    ("LocalTint", 100.0),
];

/// ค่าของ Mask ที่ยังทำตามไม่ได้ (บอกผู้ใช้เมื่อตั้งไว้)
const LOCAL_UNSUPPORTED: [(&str, &str); 7] = [
    ("LocalSharpness", "ความคม"),
    ("LocalLuminanceNoise", "ลดนอยส์"),
    ("LocalMoire", "Moiré"),
    ("LocalDefringe", "Defringe"),
    ("LocalHue", "Hue"),
    ("LocalGrain", "Grain"),
    ("LocalToningSaturation", "ทาสี (Color)"),
];

/// Mask ของ Lightroom (`<crs:MaskGroupBasedCorrections>`) ที่ทำตามได้ + รายการที่ทำตามไม่ได้
/// — ทำได้เฉพาะ Mask ที่เป็น "Select Subject" อย่างเดียว (Mask/Image ชนิดย่อย 1) · Mask ที่ปิดไว้ใน Lightroom ข้าม
pub(crate) fn lightroom_masks(text: &str) -> (Vec<crate::masks::Mask>, Vec<String>) {
    let (mut masks, mut skipped) = (Vec::new(), Vec::new());
    let Some(block) = element(text, "MaskGroupBasedCorrections") else { return (masks, skipped) };
    // ลำดับใน XMP: Description ของการปรับ ตามด้วย Description ของบริเวณที่ประกอบกันเป็นการปรับนั้น
    let mut corrections: Vec<(&str, Vec<&str>)> = Vec::new();
    // ค่าอยู่ใน attribute ของ `<rdf:Description …>` หรือของ `<rdf:li …/>` เอง (Lightroom Classic เขียนบริเวณแบบหลัง)
    for part in block.split('<').skip(1) {
        let tag = &part[..part.find('>').unwrap_or(part.len())];
        if !(tag.starts_with("rdf:Description") || tag.starts_with("rdf:li")) || !tag.contains("crs:") {
            continue;
        }
        if attr(tag, "What").is_some_and(|w| w.starts_with("Mask/")) {
            if let Some((_, areas)) = corrections.last_mut() {
                areas.push(tag);
            }
        } else {
            corrections.push((tag, Vec::new()));
        }
    }
    for (i, (head, areas)) in corrections.into_iter().enumerate() {
        if attr(head, "CorrectionActive") == Some("false") {
            continue;
        }
        let name = attr(head, "CorrectionName").unwrap_or("Mask").to_string();
        let subject = match areas.as_slice() {
            [area] => {
                attr(area, "What") == Some("Mask/Image")
                    && attr(area, "MaskSubType") == Some("1")
                    && attr(area, "MaskActive") != Some("false")
                    && num(area, "MaskValue").is_none_or(|v| v == 1.0)
                    && num(area, "MaskBlendMode").is_none_or(|v| v == 0.0)
            }
            _ => false,
        };
        if !subject {
            skipped.push(format!("Mask “{name}”"));
            continue;
        }
        let mut adjust = Adjust::default();
        for (key, scale) in LOCAL_SLIDERS {
            let value = num(head, key).unwrap_or(0.0) * scale;
            let field = match key {
                "LocalExposure2012" => {
                    adjust.exposure = value.clamp(-3.0, 3.0);
                    continue;
                }
                "LocalContrast2012" => &mut adjust.contrast,
                "LocalHighlights2012" => &mut adjust.highlights,
                "LocalShadows2012" => &mut adjust.shadows,
                "LocalWhites2012" => &mut adjust.whites,
                "LocalBlacks2012" => &mut adjust.blacks,
                "LocalTexture" => &mut adjust.texture,
                "LocalClarity2012" => &mut adjust.clarity,
                "LocalDehaze" => &mut adjust.dehaze,
                "LocalSaturation" => &mut adjust.saturation,
                "LocalTemperature" => &mut adjust.temperature,
                _ => &mut adjust.tint,
            };
            *field = value.clamp(-100.0, 100.0);
        }
        let missing: Vec<&str> = LOCAL_UNSUPPORTED
            .iter()
            .filter(|(key, _)| num(head, key).is_some_and(|v| v != 0.0))
            .map(|(_, l)| *l)
            .collect();
        if !missing.is_empty() {
            skipped.push(format!("Mask “{name}”: {}", missing.join(", ")));
        }
        let id = attr(head, "CorrectionSyncID").map_or_else(|| format!("lr-{i}"), |sync| format!("lr-{sync}"));
        masks.push(crate::masks::Mask {
            id: id.chars().take(64).collect(),
            area: crate::masks::Area::Subject { map: None },
            adjust,
            amount: (num(head, "CorrectionAmount").unwrap_or(1.0) * 100.0).clamp(0.0, 100.0),
            invert: attr(areas[0], "MaskInverted") == Some("true"),
        });
    }
    masks.truncate(crate::masks::MAX_MASKS);
    (masks, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ย่อจากโครงของ preset จริงที่ส่งออกจาก Lightroom Classic (ค่าแบบ "more white skin")
    const LIGHTROOM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:PresetType="Normal"
   crs:WhiteBalance="As Shot"
   crs:Exposure2012="+0.25"
   crs:Contrast2012="-8"
   crs:Highlights2012="-12"
   crs:Shadows2012="+11"
   crs:Whites2012="+21"
   crs:Blacks2012="+23"
   crs:Texture="-21"
   crs:Clarity2012="-7"
   crs:Dehaze="0"
   crs:Vibrance="+14"
   crs:Saturation="+13"
   crs:ParametricShadows="-5"
   crs:ParametricDarks="+4"
   crs:ParametricLights="+12"
   crs:ParametricHighlights="0"
   crs:ParametricShadowSplit="25"
   crs:ParametricMidtoneSplit="50"
   crs:ParametricHighlightSplit="75"
   crs:Sharpness="0"
   crs:LuminanceSmoothing="11"
   crs:HueAdjustmentOrange="+3"
   crs:SaturationAdjustmentRed="+32"
   crs:SaturationAdjustmentGreen="-33"
   crs:LuminanceAdjustmentOrange="+42"
   crs:SplitToningShadowHue="220"
   crs:SplitToningShadowSaturation="12"
   crs:ColorGradeMidtoneHue="40"
   crs:ColorGradeMidtoneSat="8"
   crs:ColorGradeBlending="70"
   crs:SplitToningBalance="-15"
   crs:PostCropVignetteAmount="-18"
   crs:PostCropVignetteMidpoint="35"
   crs:LocalExposure2012="0">
   <crs:Name>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">more white skin 85</rdf:li>
    </rdf:Alt>
   </crs:Name>
   <crs:Look>
    <rdf:Description crs:Name="Adobe Portrait" crs:Amount="1"/>
   </crs:Look>
   <crs:ToneCurvePV2012>
    <rdf:Seq>
     <rdf:li>2, 11</rdf:li>
     <rdf:li>60, 60</rdf:li>
     <rdf:li>138, 145</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012>
   <crs:ToneCurvePV2012Red>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012Red>
   <crs:MaskGroupBasedCorrections>
    <rdf:Seq><rdf:li><rdf:Description crs:What="Correction" crs:CorrectionName="Smoothen Face" crs:LocalExposure2012="0"/></rdf:li></rdf:Seq>
   </crs:MaskGroupBasedCorrections>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn reads_what_we_can_and_lists_what_we_cannot() {
        let p = parse_lightroom(LIGHTROOM).unwrap();
        let a = &p.adjust;
        assert_eq!(p.name.as_deref(), Some("more white skin 85"));
        assert_eq!(
            (a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks),
            (0.25, -8.0, -12.0, 11.0, 21.0, 23.0)
        );
        assert_eq!((a.vibrance, a.saturation, a.temperature), (14.0, 13.0, 0.0), "As Shot = ไม่ปรับสมดุลแสงขาว");
        assert_eq!((a.texture, a.clarity, a.dehaze), (-21.0, -7.0, 0.0));
        assert_eq!((a.hue[1], a.sat[0], a.sat[3], a.lum[1]), (3.0, 32.0, -33.0, 42.0));
        assert_eq!(a.parametric, [-5.0, 4.0, 12.0, 0.0]);
        assert_eq!(a.parametric_split, None, "จุดแบ่งค่าเริ่มต้น");
        assert_eq!(a.curve, vec![[2.0, 11.0], [60.0, 60.0], [138.0, 145.0], [255.0, 255.0]]);
        assert!(a.curve_red.is_empty(), "เส้นตรง = ไม่ปรับ");
        assert_eq!((a.grading.shadows.hue, a.grading.shadows.sat, a.grading.midtones.sat), (220.0, 12.0, 8.0));
        assert_eq!((a.grading.blending, a.grading.balance), (70.0, -15.0));
        assert_eq!((a.vignette.amount, a.vignette.midpoint, a.vignette.feather), (-18.0, 35.0, 50.0));
        assert!(a.validate().is_ok());
        assert_eq!(p.unsupported, ["Profile “Adobe Portrait”", "ลดนอยส์ 11", "Mask “Smoothen Face”"]);
    }

    #[test]
    fn sharpening_is_imported_with_lightroom_defaults_for_missing_values() {
        let p = parse_lightroom(r#"<x crs:Sharpness="40" crs:SharpenRadius="+1.0" crs:SharpenDetail="25"/>"#).unwrap();
        assert_eq!(p.adjust.sharpen, crate::adjust::Sharpen { amount: 40.0, radius: 1.0, detail: 25.0, masking: 0.0 });
        assert!(p.unsupported.is_empty(), "{:?}", p.unsupported);
        // ค่าเกินช่วงถูกหนีบ · Sharpness 0 = ไม่ทำ
        let big =
            parse_lightroom(r#"<x crs:Sharpness="200" crs:SharpenRadius="9" crs:SharpenEdgeMasking="70"/>"#).unwrap();
        assert_eq!(
            (big.adjust.sharpen.amount, big.adjust.sharpen.radius, big.adjust.sharpen.masking),
            (150.0, 3.0, 70.0)
        );
        assert!(!parse_lightroom(r#"<x crs:Sharpness="0"/>"#).unwrap().adjust.sharpen.is_active());
    }

    /// โครงเดียวกับ Mask ที่ Lightroom Classic เขียนลง preset: การปรับหนึ่งชุด + บริเวณของมัน
    fn correction(name: &str, locals: &str, areas: &str) -> String {
        format!(
            r#"<rdf:li><rdf:Description crs:What="Correction" crs:CorrectionAmount="0.9" crs:CorrectionActive="true"
       crs:CorrectionName="{name}" crs:CorrectionSyncID="SYNC{name}" {locals}>
      <crs:CorrectionMasks><rdf:Seq>{areas}</rdf:Seq></crs:CorrectionMasks></rdf:Description></rdf:li>"#
        )
    }

    /// บริเวณแบบที่ Lightroom Classic เขียนจริง: attribute อยู่บน `<rdf:li …/>` เอง
    fn area(what: &str, extra: &str) -> String {
        format!(
            r#"<rdf:li
         crs:What="{what}" crs:MaskActive="true" crs:MaskBlendMode="0" crs:MaskInverted="false" crs:MaskValue="1" {extra}/>"#
        )
    }

    /// แบบห่อด้วย `<rdf:Description>` ก็อ่านได้
    fn wrapped_area(what: &str, extra: &str) -> String {
        format!(r#"<rdf:li><rdf:Description crs:What="{what}" crs:MaskActive="true" {extra}/></rdf:li>"#)
    }

    #[test]
    fn subject_masks_are_imported_and_other_masks_are_listed() {
        let subject = area("Mask/Image", r#"crs:MaskName="Subject 1" crs:MaskSubType="1""#);
        let sky = wrapped_area("Mask/Image", r#"crs:MaskName="Sky 1" crs:MaskSubType="2""#);
        let brush = area("Mask/Paint", "");
        let xmp = format!(
            r#"<x crs:Exposure2012="+0.30"><crs:MaskGroupBasedCorrections><rdf:Seq>{}{}{}{}{}</rdf:Seq></crs:MaskGroupBasedCorrections></x>"#,
            correction(
                "Mask 1",
                r#"crs:LocalTexture="-0.35" crs:LocalClarity2012="-0.15" crs:LocalSharpness="0""#,
                &subject
            ),
            correction("Sky", r#"crs:LocalDehaze="0.2""#, &sky),
            correction("Brush", r#"crs:LocalExposure2012="0.25""#, &brush),
            correction("Subject and sky", r#"crs:LocalTexture="-0.2""#, &format!("{subject}{sky}")),
            correction("Lit subject", r#"crs:LocalExposure2012="0.125" crs:LocalLuminanceNoise="0.3""#, &subject),
        );
        let p = parse_lightroom(&xmp).unwrap();
        assert_eq!(p.adjust.exposure, 0.3, "ค่าหลักไม่ปนกับค่าของ Mask");
        assert_eq!(p.masks.len(), 2, "{:?}", p.masks);
        let m = &p.masks[0];
        assert!(matches!(m.area, crate::masks::Area::Subject { map: None }));
        assert_eq!((m.id.as_str(), m.amount, m.invert), ("lr-SYNCMask 1", 90.0, false));
        assert!((m.adjust.texture + 35.0).abs() < 1e-9 && (m.adjust.clarity + 15.0).abs() < 1e-9);
        assert!(m.adjust.validate().is_ok());
        assert!((p.masks[1].adjust.exposure - 0.5).abs() < 1e-9, "ค่าแสงของ Mask เป็นสัดส่วนของ ±4 stop");
        assert_eq!(
            p.unsupported,
            ["Mask “Sky”", "Mask “Brush”", "Mask “Subject and sky”", "Mask “Lit subject”: ลดนอยส์"]
        );
    }

    #[test]
    fn masks_switched_off_in_lightroom_are_skipped() {
        let subject = area("Mask/Image", r#"crs:MaskSubType="1""#);
        let off = correction("Off", r#"crs:LocalTexture="-0.5""#, &subject)
            .replace(r#"crs:CorrectionActive="true""#, r#"crs:CorrectionActive="false""#);
        let p = parse_lightroom(&format!("<x crs:Exposure2012=\"0\"><crs:MaskGroupBasedCorrections><rdf:Seq>{off}</rdf:Seq></crs:MaskGroupBasedCorrections></x>")).unwrap();
        assert!(p.masks.is_empty() && p.unsupported.is_empty(), "{:?} {:?}", p.masks, p.unsupported);
    }

    #[test]
    fn rejects_files_that_are_not_presets() {
        assert_eq!(parse_lightroom("<x:xmpmeta></x:xmpmeta>").err().unwrap().code, "UNSUPPORTED_FORMAT");
        // preset ขาวดำ
        let bw = parse_lightroom(r#"<x crs:ConvertToGrayscale="True" crs:Exposure2012="+9.00"/>"#).unwrap();
        assert_eq!((bw.adjust.saturation, bw.adjust.exposure), (-100.0, 3.0), "เกินช่วงถูกหนีบ");
    }
}
