//! เครื่องมือของแอปที่ AI เรียกได้ระหว่างคุย (DECISIONS ข้อ 22)
//!
//! อ่านคลัง (ค้นรูป กิจกรรม บุคคล preset) · ดูรูป (ส่งภาพย่อให้ AI) · แต่งรูป/ใช้ preset/ย้อนการแต่ง (ไม่แตะไฟล์ต้นฉบับ
//! และทุกการแก้ของรอบนี้ย้อนได้ด้วยปุ่มเดียว) · แสดงรูปให้ผู้ใช้เห็นในแชต · **ไม่มี**เครื่องมือลบ ย้าย ย่อไฟล์ หรือส่งออก

use super::mcp::{ToolHost, ToolOutput, ToolSpec};
use super::{Event, Turn};
use crate::catalog::{self, DateFilter, SearchParams};
use crate::edit::{self, EditSettings};
use crate::{people, App};
use serde_json::{json, Value};
use std::sync::Arc;

const MAX_LIST: i64 = 100;
const MAX_EDIT_PHOTOS: usize = 300;
const MAX_SHOW: usize = 60;

pub struct PhotoTools {
    pub app: Arc<App>,
    pub turn: Arc<Turn>,
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

impl ToolHost for PhotoTools {
    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "search_photos",
                description: "Search the photo library. Returns one line per photo: id | filename | event | date taken | people | edited. \
                    Combine filters freely; people are matched by name (a photo must contain every person given).",
                input_schema: schema(
                    json!({
                        "text": { "type": "string", "description": "Words in the filename, event title or person names" },
                        "people": { "type": "array", "items": { "type": "string" }, "description": "Person names" },
                        "event": { "type": "string", "description": "Event title (or part of it) or event id" },
                        "from": { "type": "string", "description": "Taken on or after, YYYY-MM-DD" },
                        "to": { "type": "string", "description": "Taken on or before, YYYY-MM-DD" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST },
                    }),
                    &[],
                ),
                read_only: true,
            },
            ToolSpec {
                name: "list_events",
                description: "List events (one folder of photos each): id | title | date | photo count. Newest first.",
                input_schema: schema(json!({ "text": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST } }), &[]),
                read_only: true,
            },
            ToolSpec {
                name: "list_people",
                description: "List the people the library knows: id | name | confirmed photo count.",
                input_schema: schema(json!({ "text": { "type": "string" } }), &[]),
                read_only: true,
            },
            ToolSpec {
                name: "get_photo",
                description: "Details of one photo: file, size, date, event, people, and its current edit settings (JSON) if it has been edited.",
                input_schema: schema(json!({ "photo_id": { "type": "string" } }), &["photo_id"]),
                read_only: true,
            },
            ToolSpec {
                name: "view_photo",
                description: "Look at a photo. Returns a JPEG of the photo as it currently looks (with its edits), or the untouched original. \
                    Use it before editing and again after, to judge the result.",
                input_schema: schema(
                    json!({
                        "photo_id": { "type": "string" },
                        "version": { "type": "string", "enum": ["edited", "original"] },
                        "size": { "type": "integer", "minimum": 256, "maximum": 1280, "description": "Long side in pixels (default 768)" },
                    }),
                    &["photo_id"],
                ),
                read_only: true,
            },
            ToolSpec {
                name: "edit_photo",
                description: "Edit one photo non-destructively (the original file is never changed; the user can undo). `adjust` is merged into the \
                    photo's current settings: only the fields you give change (objects merge, arrays are replaced). Set `reset` true to start from \
                    the original first. See the field list in edit_photos.",
                input_schema: schema(
                    json!({
                        "photo_id": { "type": "string" },
                        "adjust": { "type": "object" },
                        "rotate": { "type": "integer", "enum": [0, 90, 180, 270] },
                        "straighten": { "type": "number", "minimum": -45, "maximum": 45 },
                        "crop": { "type": ["object", "null"] },
                        "reset": { "type": "boolean" },
                    }),
                    &["photo_id"],
                ),
                read_only: false,
            },
            ToolSpec {
                name: "edit_photos",
                description: ADJUST_HELP_EDIT_PHOTOS,
                input_schema: schema(
                    json!({
                        "photo_ids": { "type": "array", "items": { "type": "string" }, "maxItems": MAX_EDIT_PHOTOS },
                        "adjust": { "type": "object" },
                        "reset": { "type": "boolean" },
                    }),
                    &["photo_ids", "adjust"],
                ),
                read_only: false,
            },
            ToolSpec {
                name: "revert_photos",
                description: "Remove all edits from photos so they show the original again.",
                input_schema: schema(json!({ "photo_ids": { "type": "array", "items": { "type": "string" }, "maxItems": MAX_EDIT_PHOTOS } }), &["photo_ids"]),
                read_only: false,
            },
            ToolSpec {
                name: "list_presets",
                description: "List the user's light-and-color presets: id | name | source | what it changes.",
                input_schema: schema(json!({}), &[]),
                read_only: true,
            },
            ToolSpec {
                name: "apply_preset",
                description: "Apply a preset (by id or exact name) to photos: replaces their light and color settings, keeps their crop and rotation.",
                input_schema: schema(
                    json!({ "preset": { "type": "string" }, "photo_ids": { "type": "array", "items": { "type": "string" }, "maxItems": MAX_EDIT_PHOTOS } }),
                    &["preset", "photo_ids"],
                ),
                read_only: false,
            },
            ToolSpec {
                name: "save_preset",
                description: "Save the light and color settings of an edited photo as a new preset the user can reuse.",
                input_schema: schema(json!({ "name": { "type": "string" }, "photo_id": { "type": "string" } }), &["name", "photo_id"]),
                read_only: false,
            },
            ToolSpec {
                name: "show_photos",
                description: "Show photos to the user as thumbnails in the chat (they can click to open them). Use it to present search results or what you edited.",
                input_schema: schema(
                    json!({ "photo_ids": { "type": "array", "items": { "type": "string" }, "maxItems": MAX_SHOW }, "title": { "type": "string" } }),
                    &["photo_ids"],
                ),
                read_only: true,
            },
        ]
    }

    fn call(&self, name: &str, args: Value) -> ToolOutput {
        let id = self.turn.next_tool_id();
        self.turn.push(Event::Tool { id, name: name.to_string(), summary: summarize(name, &args) });
        let out = match name {
            "search_photos" => self.search(&args),
            "list_events" => self.list_events(&args),
            "list_people" => self.list_people(&args),
            "get_photo" => self.get_photo(&args),
            "view_photo" => self.view_photo(&args),
            "edit_photo" => self.edit_photo(&args),
            "edit_photos" => self.edit_photos(&args),
            "revert_photos" => self.revert_photos(&args),
            "list_presets" => self.list_presets(),
            "apply_preset" => self.apply_preset(&args),
            "save_preset" => self.save_preset(&args),
            "show_photos" => self.show_photos(&args),
            _ => Err("Unknown tool".into()),
        };
        let out = out.unwrap_or_else(ToolOutput::error);
        let result = out
            .content
            .iter()
            .map(|c| match c.get("type").and_then(Value::as_str) {
                Some("image") => "[ภาพ]".to_string(),
                _ => c.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.turn.push(Event::ToolDone { id, ok: !out.is_error, result: cut(&result, 4000) });
        out
    }
}

/// edit_photos ใช้คำอธิบายฟิลด์ทั้งหมด (ยาว) ในที่เดียว
const ADJUST_HELP_EDIT_PHOTOS: &str = concat!(
    "Edit many photos at once with the same change (non-destructive; the user can undo). `adjust` is merged into EACH photo's own current \
settings (only the given fields change), unless `reset` is true. ",
    "Fields of `adjust` (all optional, 0 or empty = unchanged look): exposure (EV, -3..3); contrast, highlights, shadows, whites, blacks, \
temperature (+warm/-cool), tint (+magenta/-green), vibrance, saturation (-100..100); hue, sat, lum: arrays of 8 values -100..100 for [red, orange, \
yellow, green, aqua, blue, purple, magenta] (skin tones are mostly orange); parametric: [shadows, darks, lights, highlights] -100..100; curve, \
curveRed, curveGreen, curveBlue: point curves [[in,out],...] 0..255 sorted by in, at most 16 points; levels, levelsRed, levelsGreen, levelsBlue: \
{inBlack 0..253, gamma 0.1..9.99 (1 = none, >1 brighter mids), inWhite, outBlack, outWhite 0..255}; grading: {shadows, midtones, highlights, \
global: {hue 0..360, sat 0..100, lum -100..100}, blending 0..100 (default 50), balance -100..100}; vignette: {amount -100..100 (negative darkens \
edges), midpoint 0..100, feather 0..100, roundness -100..100}."
);

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

fn summarize(name: &str, args: &Value) -> String {
    let ids = |k: &str| args.get(k).and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match name {
        "search_photos" => cut(&args.to_string(), 200),
        "edit_photos" | "revert_photos" | "show_photos" => format!("{} รูป", ids("photo_ids")),
        "apply_preset" => format!("“{}” กับ {} รูป", s("preset"), ids("photo_ids")),
        "edit_photo" => cut(&args.get("adjust").map(|a| a.to_string()).unwrap_or_default(), 200),
        _ => cut(&args.to_string(), 200),
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

fn ids_arg(args: &Value, key: &str) -> Result<Vec<String>, String> {
    let ids: Vec<String> = args
        .get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(format!("`{key}` must be a non-empty list of photo ids"));
    }
    if ids.len() > MAX_EDIT_PHOTOS {
        return Err(format!("At most {MAX_EDIT_PHOTOS} photos per call"));
    }
    Ok(ids)
}

/// รวม object: ฟิลด์ใน patch แทนของเดิม · object ซ้อนรวมต่อ · array และค่าอื่นแทนทั้งก้อน
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => merge(existing, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn line_of(p: &catalog::PhotoResult) -> String {
    let people = p.confirmed_people.iter().map(|x| x.display_name.as_str()).collect::<Vec<_>>().join(", ");
    format!(
        "{} | {} | {} | {} | {} | {}",
        p.id,
        p.filename,
        p.event_title,
        p.taken_at.as_deref().unwrap_or("-"),
        if people.is_empty() { "-" } else { &people },
        if p.edit_id.is_some() { "edited" } else { "-" }
    )
}

impl PhotoTools {
    fn search(&self, args: &Value) -> Result<ToolOutput, String> {
        let conn = self.app.db();
        let mut person_ids = vec![];
        if let Some(names) = args.get("people").and_then(Value::as_array) {
            for name in names.iter().filter_map(Value::as_str) {
                let found =
                    people::list(&conn, &people::ListArgs { text: Some(name.to_string()), ..Default::default() })
                        .map_err(|e| e.message)?;
                let exact = found.iter().find(|p| p.person.display_name == name).or(found.first());
                match exact {
                    Some(p) => person_ids.push(p.person.id.clone()),
                    None => {
                        return Ok(ToolOutput::text(format!("No person named “{name}”. Use list_people to see names.")))
                    }
                }
            }
        }
        let mut event_ids = vec![];
        if let Some(event) = str_arg(args, "event") {
            let events = catalog::list_events(&conn, None).map_err(|e| e.message)?;
            let matched: Vec<String> =
                events.iter().filter(|e| e.id == event || e.title.contains(event)).map(|e| e.id.clone()).collect();
            if matched.is_empty() {
                return Ok(ToolOutput::text(format!("No event matches “{event}”. Use list_events.")));
            }
            event_ids = matched;
        }
        let (from, to) = (str_arg(args, "from").map(str::to_string), str_arg(args, "to").map(str::to_string));
        let params = SearchParams {
            text: str_arg(args, "text").map(str::to_string),
            person_ids,
            person_mode: Some("all".into()),
            event_ids,
            date: (from.is_some() || to.is_some()).then(|| DateFilter {
                field: "photo_taken".into(),
                from,
                to,
                include_unknown: false,
            }),
            sort: Some("taken_desc".into()),
            limit: Some(args.get("limit").and_then(Value::as_i64).unwrap_or(30).clamp(1, MAX_LIST)),
            ..Default::default()
        };
        let page = catalog::query_photos(&conn, &params).map_err(|e| e.message)?;
        let mut text = format!(
            "{} photos match; showing {}.\nid | filename | event | taken | people | edited\n",
            page.total,
            page.items.len()
        );
        for p in &page.items {
            text.push_str(&line_of(p));
            text.push('\n');
        }
        Ok(ToolOutput::text(text))
    }

    fn list_events(&self, args: &Value) -> Result<ToolOutput, String> {
        let events = catalog::list_events(&self.app.db(), str_arg(args, "text")).map_err(|e| e.message)?;
        let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(50).clamp(1, MAX_LIST) as usize;
        let mut text = format!("{} events.\nid | title | date | photos\n", events.len());
        for e in events.iter().take(limit) {
            text.push_str(&format!(
                "{} | {} | {} | {}\n",
                e.id,
                e.title,
                e.event_date.as_deref().unwrap_or("-"),
                e.photo_count
            ));
        }
        Ok(ToolOutput::text(text))
    }

    fn list_people(&self, args: &Value) -> Result<ToolOutput, String> {
        let list = people::list(
            &self.app.db(),
            &people::ListArgs { text: str_arg(args, "text").map(str::to_string), ..Default::default() },
        )
        .map_err(|e| e.message)?;
        let mut text = format!("{} people.\nid | name | confirmed photos\n", list.len());
        for p in &list {
            text.push_str(&format!("{} | {} | {}\n", p.person.id, p.person.display_name, p.confirmed_photo_count));
        }
        Ok(ToolOutput::text(text))
    }

    fn get_photo(&self, args: &Value) -> Result<ToolOutput, String> {
        let id = str_arg(args, "photo_id").ok_or("photo_id is required")?;
        let d = catalog::get_photo_details(&self.app, id).map_err(|e| e.message)?;
        let edit = d
            .edit
            .as_ref()
            .map(|e| serde_json::to_string(&e.settings).unwrap_or_default())
            .unwrap_or_else(|| "none (original)".into());
        Ok(ToolOutput::text(format!(
            "{}\nfile: {}\nsize: {}×{} px\ntaken: {}\nevent: {}\npeople: {}\ncurrent edit: {}",
            d.photo.id,
            d.photo.filename,
            d.photo.width,
            d.photo.height,
            d.photo.taken_at.as_deref().unwrap_or("-"),
            d.photo.event_title,
            d.photo.confirmed_people.iter().map(|p| p.display_name.as_str()).collect::<Vec<_>>().join(", "),
            edit
        )))
    }

    fn view_photo(&self, args: &Value) -> Result<ToolOutput, String> {
        let id = str_arg(args, "photo_id").ok_or("photo_id is required")?;
        let original = str_arg(args, "version") == Some("original");
        let size = args.get("size").and_then(Value::as_u64).unwrap_or(768).clamp(256, 1280) as u32;
        let asset = crate::api::asset(&self.app, &format!("{}/{id}", if original { "original" } else { "photo" }))
            .map_err(|e| e.message)?;
        let img = crate::media::decode_upright(&asset.bytes, size).map_err(|e| e.message)?;
        let small = img.thumbnail(size, size);
        let jpeg = crate::media::encode_jpeg(&small, 82).map_err(|e| e.message)?;
        Ok(ToolOutput {
            content: vec![
                json!({ "type": "image", "data": base64(&jpeg), "mimeType": "image/jpeg" }),
                json!({ "type": "text", "text": format!("{} ({}), {}×{} px shown", id, if original { "original" } else { "as edited" }, small.width(), small.height()) }),
            ],
            is_error: false,
        })
    }

    /// แต่งรูปหนึ่งรูป (จำค่าเดิมไว้ให้ย้อนได้) — คืนค่าที่บันทึก
    fn apply_edit(&self, photo_id: &str, patch: &Value, reset: bool) -> Result<EditSettings, String> {
        let current = { edit::load(&self.app.db(), photo_id).map_err(|e| e.message)? }.map(|s| s.settings);
        let mut value =
            serde_json::to_value(if reset { EditSettings::default() } else { current.clone().unwrap_or_default() })
                .map_err(|e| e.to_string())?;
        merge(&mut value, patch);
        let next: EditSettings = serde_json::from_value(value).map_err(|e| format!("Invalid settings: {e}"))?;
        if current.as_ref() == Some(&next) || (current.is_none() && next.is_identity()) {
            return Ok(next);
        }
        self.turn.remember(photo_id, current);
        edit::save(&self.app, photo_id, next.clone()).map_err(|e| e.message)?;
        Ok(next)
    }

    fn edit_photo(&self, args: &Value) -> Result<ToolOutput, String> {
        let id = str_arg(args, "photo_id").ok_or("photo_id is required")?;
        let mut patch = json!({});
        if let Some(a) = args.get("adjust") {
            patch["adjust"] = a.clone();
        }
        for key in ["rotate", "straighten", "crop"] {
            if let Some(v) = args.get(key) {
                patch[key] = v.clone();
            }
        }
        let reset = args.get("reset").and_then(Value::as_bool).unwrap_or(false);
        let saved = self.apply_edit(id, &patch, reset)?;
        self.turn.push(Event::Changed { photo_ids: vec![id.to_string()] });
        Ok(ToolOutput::text(format!(
            "Saved. Current settings of {id}: {}\nCall view_photo to check the result.",
            serde_json::to_string(&saved).unwrap_or_default()
        )))
    }

    fn edit_photos(&self, args: &Value) -> Result<ToolOutput, String> {
        let ids = ids_arg(args, "photo_ids")?;
        let patch = json!({ "adjust": args.get("adjust").cloned().unwrap_or_else(|| json!({})) });
        let reset = args.get("reset").and_then(Value::as_bool).unwrap_or(false);
        let mut done = vec![];
        let mut failed = vec![];
        for id in &ids {
            match self.apply_edit(id, &patch, reset) {
                Ok(_) => done.push(id.clone()),
                Err(e) => failed.push(format!("{id}: {e}")),
            }
        }
        self.turn.push(Event::Changed { photo_ids: done.clone() });
        Ok(ToolOutput::text(format!(
            "Edited {} photos.{}",
            done.len(),
            if failed.is_empty() { String::new() } else { format!(" Failed: {}", failed.join("; ")) }
        )))
    }

    fn revert_photos(&self, args: &Value) -> Result<ToolOutput, String> {
        let ids = ids_arg(args, "photo_ids")?;
        let mut done = vec![];
        for id in &ids {
            let current = { edit::load(&self.app.db(), id).map_err(|e| e.message)? }.map(|s| s.settings);
            if current.is_none() {
                continue;
            }
            self.turn.remember(id, current);
            edit::save(&self.app, id, EditSettings::default()).map_err(|e| e.message)?;
            done.push(id.clone());
        }
        self.turn.push(Event::Changed { photo_ids: done.clone() });
        Ok(ToolOutput::text(format!("Reverted {} photos to the original.", done.len())))
    }

    fn list_presets(&self) -> Result<ToolOutput, String> {
        let list = crate::preset::list(&self.app.db()).map_err(|e| e.message)?;
        let mut text = format!("{} presets.\nid | name | source | settings\n", list.len());
        for p in &list {
            text.push_str(&format!(
                "{} | {} | {} | {}\n",
                p.id,
                p.name,
                p.source,
                serde_json::to_string(&p.adjust).unwrap_or_default()
            ));
        }
        Ok(ToolOutput::text(text))
    }

    fn apply_preset(&self, args: &Value) -> Result<ToolOutput, String> {
        let key = str_arg(args, "preset").ok_or("preset is required")?;
        let ids = ids_arg(args, "photo_ids")?;
        let list = crate::preset::list(&self.app.db()).map_err(|e| e.message)?;
        let preset = list
            .iter()
            .find(|p| p.id == key || p.name == key)
            .ok_or_else(|| format!("No preset “{key}”. Use list_presets."))?;
        let adjust = serde_json::to_value(&preset.adjust).map_err(|e| e.to_string())?;
        let mut done = vec![];
        for id in &ids {
            // แทนแสงและสีทั้งชุด (ไม่ merge) แต่คงการตัด/หมุนของแต่ละรูป
            let current =
                { edit::load(&self.app.db(), id).map_err(|e| e.message)? }.map(|s| s.settings).unwrap_or_default();
            let mut base = serde_json::to_value(&current).map_err(|e| e.to_string())?;
            base["adjust"] = adjust.clone();
            let next: EditSettings = serde_json::from_value(base).map_err(|e| e.to_string())?;
            if self.apply_edit(id, &serde_json::to_value(&next).map_err(|e| e.to_string())?, true).is_ok() {
                done.push(id.clone());
            }
        }
        self.turn.push(Event::Changed { photo_ids: done.clone() });
        Ok(ToolOutput::text(format!("Applied “{}” to {} photos.", preset.name, done.len())))
    }

    fn save_preset(&self, args: &Value) -> Result<ToolOutput, String> {
        let name = str_arg(args, "name").ok_or("name is required")?;
        let id = str_arg(args, "photo_id").ok_or("photo_id is required")?;
        let settings = { edit::load(&self.app.db(), id).map_err(|e| e.message)? }
            .ok_or("That photo has no edits to save")?
            .settings;
        let preset = crate::preset::save(&self.app, name, &settings.adjust).map_err(|e| e.message)?;
        self.turn.push(Event::Changed { photo_ids: vec![] });
        Ok(ToolOutput::text(format!("Saved preset “{}” ({}).", preset.name, preset.id)))
    }

    fn show_photos(&self, args: &Value) -> Result<ToolOutput, String> {
        let mut ids = ids_arg(args, "photo_ids")?;
        ids.truncate(MAX_SHOW);
        let title = str_arg(args, "title").unwrap_or("").to_string();
        self.turn.push(Event::Photos { title, photo_ids: ids.clone() });
        Ok(ToolOutput::text(format!("Shown {} photos to the user.", ids.len())))
    }
}

/// base64 มาตรฐาน (มี padding) สำหรับภาพที่ส่งให้ AI
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n =
            (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn merge_replaces_arrays_and_merges_objects() {
        let mut base = json!({ "adjust": { "exposure": 0.5, "lum": [1, 2], "grading": { "blending": 50, "balance": 0 } }, "rotate": 0 });
        merge(&mut base, &json!({ "adjust": { "lum": [9], "grading": { "balance": 20 } }, "rotate": 90 }));
        assert_eq!(
            base,
            json!({ "adjust": { "exposure": 0.5, "lum": [9], "grading": { "blending": 50, "balance": 20 } }, "rotate": 90 })
        );
    }
}
