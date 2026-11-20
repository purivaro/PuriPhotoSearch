//! ให้ AI สร้างภาพใหม่จากรูปที่เลือก ตาม prompt ของผู้ใช้หรือแม่แบบ แล้วบันทึกกลับเข้าคลัง (DECISIONS ข้อ 23)
//!
//! - ใช้ Codex ของ ChatGPT (บัญชีที่ผู้ใช้ล็อกอินไว้) ซึ่งมีเครื่องมือสร้างภาพในตัว — ส่งสำเนาย่อ (ด้านยาว 2048 px) ของรูปที่แต่งแล้วไปทีละรูป
//! - ภาพที่ได้ (PNG ใน `~/.codex/generated_images/<รอบ>/`) แปลงเป็น JPEG แล้วเก็บเป็นระบบ:
//!   `<รูปภาพ>/Puri Photo Search/ภาพจาก AI/<ชื่อกิจกรรมต้นทาง>/<ชื่อไฟล์ต้นทาง>-ai-<n>.jpg` = หนึ่งกิจกรรมต่อกิจกรรมต้นทาง
//! - ที่มา (รูปต้นทาง prompt AI ที่ใช้ วันที่) ผูกกับไฟล์ด้วย SHA-256 ในตาราง `ai_images` · ไฟล์ต้นฉบับไม่ถูกแตะ

use super::detect::{self, Provider};
use super::{execute, scratch_dir, Event, Turn};
use crate::error::{AppError, Result};
use crate::export::SelectionSpec;
use crate::{util, App};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAX_PHOTOS: usize = 10;
const MAX_PROMPT: usize = 2000;
const INPUT_EDGE: u32 = 2048;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateArgs {
    pub selection: SelectionSpec,
    pub prompt: String,
    /// ชื่อแม่แบบที่เลือก (ถ้ามี) — เก็บไว้เป็นที่มา
    #[serde(default)]
    pub template: Option<String>,
}

/// ที่มาของภาพที่ AI สร้าง (แสดงในตัวดูภาพ)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOrigin {
    pub source_photo_id: Option<String>,
    pub source_filename: String,
    pub prompt: String,
    pub template: Option<String>,
    pub provider: String,
    pub created_at: String,
}

pub fn origin(conn: &rusqlite::Connection, sha256: &str) -> Result<Option<AiOrigin>> {
    Ok(conn
        .query_row(
            "SELECT a.source_photo_id, a.source_filename, a.prompt, a.template, a.provider, a.created_at FROM ai_images a WHERE a.sha256 = ?1",
            [sha256],
            |r| {
                Ok(AiOrigin {
                    source_photo_id: r.get(0)?,
                    source_filename: r.get(1)?,
                    prompt: r.get(2)?,
                    template: r.get(3)?,
                    provider: r.get(4)?,
                    created_at: r.get(5)?,
                })
            },
        )
        .optional()?
        // รูปต้นทางที่ไม่อยู่ในคลังแล้ว: ไม่ให้ลิงก์ไปเปิด
        .map(|mut o| {
            if let Some(id) = &o.source_photo_id {
                let exists = conn
                    .query_row("SELECT 1 FROM photos WHERE id = ?1 AND catalog_state = 'active'", [id], |_| Ok(()))
                    .optional()
                    .ok()
                    .flatten()
                    .is_some();
                if !exists {
                    o.source_photo_id = None;
                }
            }
            o
        }))
}

/// รูปต้นทางหนึ่งรูป
pub(crate) struct Source {
    pub photo_id: String,
    pub filename: String,
    pub event_title: String,
    pub event_date: Option<String>,
}

fn load_sources(app: &App, selection: &SelectionSpec) -> Result<Vec<Source>> {
    let conn = app.db();
    let rows = crate::export::load_items(&conn, selection)?;
    if rows.is_empty() {
        return Err(AppError::invalid("ยังไม่ได้เลือกรูป"));
    }
    if rows.len() > MAX_PHOTOS {
        return Err(AppError::invalid(format!("สร้างได้ครั้งละไม่เกิน {MAX_PHOTOS} รูป (เลือกไว้ {} รูป)", rows.len())));
    }
    rows.iter()
        .map(|r| {
            let (filename, event_title, event_date): (String, String, Option<String>) = conn.query_row(
                "SELECT l.filename, e.title, e.event_date FROM photo_locations l
                 JOIN event_photos ep ON ep.photo_id = l.photo_id JOIN events e ON e.id = ep.event_id WHERE l.photo_id = ?1",
                [&r.photo_id],
                |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
            )?;
            Ok(Source { photo_id: r.photo_id.clone(), filename, event_title, event_date })
        })
        .collect()
}

/// โฟลเดอร์รากของภาพที่ AI สร้าง — เทสต์ชี้ไปที่อื่นได้ด้วย `PURI_AI_OUTPUT`
pub fn output_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PURI_AI_OUTPUT") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join("Pictures").join("Puri Photo Search").join("ภาพจาก AI")
}

/// ชื่อโฟลเดอร์ที่ปลอดภัยทุกระบบ (ตัดอักขระที่ใช้ในชื่อไฟล์ไม่ได้)
fn safe_name(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let trimmed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = trimmed.trim_matches('.').trim();
    if trimmed.is_empty() {
        "ภาพ".into()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// เริ่มสร้างภาพในเบื้องหลัง — คืนรหัสรอบ (อ่านความคืบหน้าด้วย `ai_events`)
pub fn start(app: &App, args: GenerateArgs) -> Result<String> {
    let prompt = args.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(AppError::invalid("พิมพ์สิ่งที่อยากให้ AI สร้างก่อน"));
    }
    if prompt.chars().count() > MAX_PROMPT {
        return Err(AppError::invalid(format!("คำสั่งยาวเกิน {MAX_PROMPT} ตัวอักษร")));
    }
    let sources = load_sources(app, &args.selection)?;
    let me = app.me.upgrade().ok_or_else(|| AppError::invalid("แอปกำลังปิด"))?;
    let (id, turn) = super::register(app);
    let template = args.template.clone();
    std::thread::spawn(move || {
        let result = run(&me, &turn, &sources, &prompt, template.as_deref());
        let stopped = turn.cancel.load(Ordering::Relaxed);
        turn.push(Event::Finished {
            ok: result.is_ok() && !stopped,
            stopped,
            error: if stopped { None } else { result.err() },
            undoable: 0,
        });
        turn.done.store(true, Ordering::Relaxed);
    });
    Ok(id)
}

fn generation_prompt(prompt: &str) -> String {
    format!(
        "Use your image generation tool to create exactly one new image from the attached photo, following this request:\n\n{prompt}\n\n\
         Keep the people recognizable and respectful unless the request says otherwise. Do not add any text, letters, logos or watermarks unless the \
         request asks for them. Do not run shell commands and do not read or write files. When the image is done, reply with one short sentence."
    )
}

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".codex")
    })
}

/// ภาพล่าสุดที่ Codex สร้างในรอบนี้
fn generated_image(thread_id: &str) -> Option<PathBuf> {
    let dir = codex_home().join("generated_images").join(thread_id);
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| {
                x.eq_ignore_ascii_case("png")
                    || x.eq_ignore_ascii_case("jpg")
                    || x.eq_ignore_ascii_case("jpeg")
                    || x.eq_ignore_ascii_case("webp")
            })
        })
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn run(
    app: &Arc<App>,
    turn: &Arc<Turn>,
    sources: &[Source],
    prompt: &str,
    template: Option<&str>,
) -> std::result::Result<(), String> {
    let provider = Provider::Codex;
    let path = detect::locate(provider)
        .ok_or("ไม่พบ Codex ของ ChatGPT ในเครื่องนี้ — ติดตั้งแอป ChatGPT หรือ Codex CLI แล้วล็อกอินก่อน")?;
    let work = scratch_dir(app).map_err(|e| e.message)?.join(format!("generate-{}", util::new_id()));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let mut made = 0;
    let mut failed = vec![];
    // โฟลเดอร์ปลายทาง → (ชื่อกิจกรรม, วันที่, ไฟล์ที่สร้างในรอบนี้)
    let mut folders: Vec<(PathBuf, String, Option<String>, Vec<PathBuf>)> = vec![];
    for (n, source) in sources.iter().enumerate() {
        if turn.cancel.load(Ordering::Relaxed) {
            break;
        }
        let step = turn.next_tool_id();
        turn.push(Event::Tool {
            id: step,
            name: "generate_image".into(),
            summary: format!("{} ({}/{})", source.filename, n + 1, sources.len()),
        });
        let outcome = (|| -> std::result::Result<PathBuf, String> {
            let input = work.join(format!("{n}.jpg"));
            prepare_input(app, &source.photo_id, &input).map_err(|e| e.message)?;
            let argv: Vec<String> = [
                "exec",
                "--json",
                "--sandbox",
                "read-only",
                "--skip-git-repo-check",
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules",
            ]
            .iter()
            .map(|s| s.to_string())
            .chain([
                format!("--image={}", input.to_string_lossy()),
                "-C".into(),
                work.to_string_lossy().into_owned(),
                "-".into(),
            ])
            .collect();
            let parser = execute(provider, &path, &argv, generation_prompt(prompt), &work, turn, None, false)?;
            let image = parser.thread_id.as_deref().and_then(generated_image).ok_or_else(|| {
                let said = parser.text.trim();
                if said.is_empty() {
                    "ChatGPT ไม่ได้สร้างภาพ".to_string()
                } else {
                    format!("ChatGPT ไม่ได้สร้างภาพ: {said}")
                }
            })?;
            let bytes = std::fs::read(&image).map_err(|e| e.to_string())?;
            let folder = output_root().join(safe_name(&source.event_title));
            save_output(app, source, &bytes, &folder, prompt, template, provider.binary(), None).map_err(|e| e.message)
        })();
        match outcome {
            Ok(file) => {
                made += 1;
                let folder = file.parent().map(Path::to_path_buf).unwrap_or_default();
                turn.push(Event::ToolDone { id: step, ok: true, result: file.to_string_lossy().into_owned() });
                match folders.iter_mut().find(|(f, ..)| *f == folder) {
                    Some((.., files)) => files.push(file),
                    None => folders.push((
                        folder,
                        format!("{} · ภาพจาก AI", source.event_title),
                        source.event_date.clone(),
                        vec![file],
                    )),
                }
            }
            Err(e) => {
                failed.push(format!("{}: {e}", source.filename));
                turn.push(Event::ToolDone { id: step, ok: false, result: e });
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    // เข้าคลัง: กิจกรรมละหนึ่งโฟลเดอร์ แล้วแสดงภาพที่ได้
    let mut photo_ids = vec![];
    for (folder, title, date, files) in &folders {
        match register_folder(app, folder, title, date.as_deref(), files) {
            Ok(ids) => photo_ids.extend(ids),
            Err(e) => failed.push(format!("เพิ่มเข้าคลังไม่สำเร็จ ({}): {}", folder.to_string_lossy(), e.message)),
        }
    }
    if !photo_ids.is_empty() {
        turn.push(Event::Photos {
            title: format!("ภาพที่สร้าง {made} ภาพ"), photo_ids: photo_ids.clone()
        });
        turn.push(Event::Changed { photo_ids });
    }
    turn.push(Event::Answer {
        text: if failed.is_empty() {
            format!("สร้างภาพเสร็จ {made} ภาพ เก็บไว้ที่ “{}” และเพิ่มเข้าคลังแล้ว", output_root().to_string_lossy())
        } else {
            format!(
                "สร้างได้ {made} จาก {} ภาพ\n\n{}",
                sources.len(),
                failed.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")
            )
        },
    });
    if made == 0 && !failed.is_empty() {
        return Err(failed.join(" · "));
    }
    Ok(())
}

/// สำเนาย่อของรูป (ตามที่แต่งไว้) สำหรับส่งให้ AI
fn prepare_input(app: &App, photo_id: &str, out: &Path) -> Result<()> {
    let asset = crate::api::asset(app, &format!("photo/{photo_id}"))?;
    let img = crate::media::decode_upright(&asset.bytes, INPUT_EDGE)?;
    let img = if img.width().max(img.height()) > INPUT_EDGE { img.thumbnail(INPUT_EDGE, INPUT_EDGE) } else { img };
    std::fs::write(out, crate::media::encode_jpeg(&img, 90)?)?;
    Ok(())
}

/// เก็บภาพที่ AI สร้างเป็น JPEG ในโฟลเดอร์ของกิจกรรมต้นทาง และบันทึกที่มา — คืน path ของไฟล์
#[allow(clippy::too_many_arguments)]
pub(crate) fn save_output(
    app: &App,
    source: &Source,
    image: &[u8],
    folder: &Path,
    prompt: &str,
    template: Option<&str>,
    provider: &str,
    model: Option<&str>,
) -> Result<PathBuf> {
    let img = image::load_from_memory(image)
        .map_err(|e| AppError::new("UNSUPPORTED_FORMAT", format!("อ่านภาพที่ AI สร้างไม่ได้: {e}")))?;
    let jpeg = crate::media::encode_jpeg(&img, 92)?;
    std::fs::create_dir_all(folder)?;
    let stem = Path::new(&source.filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ภาพ".into());
    let file = (1..)
        .map(|n| folder.join(format!("{}-ai-{n}.jpg", safe_name(&stem))))
        .find(|p| !p.exists())
        .ok_or_else(|| AppError::invalid("ตั้งชื่อไฟล์ไม่ได้"))?;
    // เขียนไฟล์ชั่วคราวแล้วเปลี่ยนชื่อ: ไม่มีไฟล์ครึ่ง ๆ ให้ตัวสแกนเห็น
    let tmp = folder.join(format!(".{}.part", util::new_id()));
    std::fs::write(&tmp, &jpeg)?;
    std::fs::rename(&tmp, &file)?;
    app.db().execute(
        "INSERT OR REPLACE INTO ai_images(sha256, source_photo_id, source_filename, prompt, template, provider, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![util::hex(&Sha256::digest(&jpeg)), source.photo_id, source.filename, prompt, template, provider, model, util::now()],
    )?;
    Ok(file)
}

/// สำหรับเทสต์: บันทึกภาพ (เหมือนที่ AI สร้าง) ของรูปต้นทางลงโฟลเดอร์ที่กำหนด พร้อมที่มา
#[doc(hidden)]
pub fn test_save(app: &App, photo_id: &str, image: &[u8], folder: &Path, prompt: &str) -> Result<PathBuf> {
    let source = load_sources(app, &SelectionSpec::Explicit { photo_ids: vec![photo_id.to_string()] })?.remove(0);
    save_output(app, &source, image, folder, prompt, Some("สีน้ำ"), "codex", None)
}

/// ให้โฟลเดอร์ของภาพที่สร้างเป็นกิจกรรมในคลัง (กิจกรรมเดิมถ้ามีแล้ว = สแกนใหม่) แล้วรอให้ไฟล์ของรอบนี้เข้าคลัง — คืนรหัสรูปของไฟล์เหล่านั้น
fn register_folder(
    app: &App,
    folder: &Path,
    title: &str,
    date: Option<&str>,
    files: &[PathBuf],
) -> Result<Vec<String>> {
    let existing = { crate::import::find_covering_event(&app.db(), folder, false)? };
    match existing {
        Some((event_id, _)) => {
            crate::jobs::rescan_event(app, &event_id)?;
        }
        None => {
            let preview = crate::import::preview(app, &folder.to_string_lossy())?;
            crate::import::commit(
                app,
                crate::import::CommitArgs {
                    preview_id: preview.preview_id,
                    request_id: util::new_id(),
                    mode: "single_event".into(),
                    drafts: vec![crate::import::DraftChoice {
                        draft_id: preview.single.draft_id,
                        title: title.into(),
                        event_date: date.map(str::to_string).or(preview.single.suggested_date),
                    }],
                    analyze_faces: true,
                    watch: true,
                },
            )?;
        }
    }
    // รอตัวสแกนเพิ่มไฟล์ใหม่เข้าคลัง (worker ทำงานเบื้องหลัง) — ไม่เกิน 2 นาที
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let ids: Vec<String> = {
            let conn = app.db();
            files
                .iter()
                .filter_map(|f| {
                    conn.query_row(
                        "SELECT photo_id FROM photo_locations WHERE path_key = ?1",
                        [util::path_key(f)],
                        |r| r.get(0),
                    )
                    .optional()
                    .ok()
                    .flatten()
                })
                .collect()
        };
        if ids.len() >= files.len() || Instant::now() > deadline {
            return Ok(ids);
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_are_safe_everywhere() {
        assert_eq!(safe_name("งาน/บุญ: กฐิน*2569?"), "งาน บุญ กฐิน 2569");
        assert_eq!(safe_name("  ...  "), "ภาพ");
        assert!(generation_prompt("สีน้ำ").contains("สีน้ำ"));
    }
}
