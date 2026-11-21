//! เพิ่มโฟลเดอร์กิจกรรม: preview แบบไม่แตะคลัง แล้ว commit ใน transaction เดียว (Blueprint 2, 9.4)

use crate::error::{AppError, Result};
use crate::media::{self, FileKind};
use crate::util::{self, guess_from_folder};
use crate::{db, jobs, App};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

const PREVIEW_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDraft {
    pub draft_id: String,
    pub kind: &'static str,
    pub folder_name: String,
    pub display_path: String,
    pub suggested_title: String,
    pub suggested_date: Option<String>,
    pub date_source: &'static str,
    pub photo_count: usize,
    pub new_count: usize,
    pub existing_count: usize,
    pub unsupported_count: usize,
    /// ไฟล์ RAW ในแถวนี้ (ไม่รวมใน `photo_count`) และที่ยังไม่อยู่ในคลัง
    pub raw_count: usize,
    pub raw_new_count: usize,
    pub readable: bool,
    pub unreadable_reason: Option<String>,
    pub existing_event_id: Option<String>,
    pub existing_event_title: Option<String>,
    pub default_selected: bool,
    pub has_sample: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub preview_id: String,
    pub root_path: String,
    pub root_name: String,
    pub suggested_mode: &'static str,
    pub single: EventDraft,
    pub children: Vec<EventDraft>,
    pub root_loose: Option<EventDraft>,
    pub unsupported_by_ext: BTreeMap<String, usize>,
    pub expires_at: String,
}

pub struct DraftData {
    pub draft: EventDraft,
    pub folder: PathBuf,
    pub recursive: bool,
    pub sample: Option<PathBuf>,
}

/// สิ่งที่ผู้ใช้เห็นใน preview; commit รับได้เฉพาะ draft ที่อยู่ในนี้ (Blueprint 9.4)
pub struct PreviewSession {
    pub id: String,
    pub root: PathBuf,
    created: Instant,
    pub drafts: Vec<DraftData>,
    samples: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

impl PreviewSession {
    /// ภาพย่อของรูปตัวอย่างในแถว preview (สร้างเมื่อถูกขอ และจำไว้ตลอดอายุ session)
    pub fn sample_jpeg(&self, draft_id: &str) -> Result<Arc<Vec<u8>>> {
        if let Some(hit) = self.samples.lock().unwrap_or_else(|e| e.into_inner()).get(draft_id) {
            return Ok(hit.clone());
        }
        let path = self
            .drafts
            .iter()
            .find(|d| d.draft.draft_id == draft_id)
            .and_then(|d| d.sample.clone())
            .ok_or_else(|| AppError::not_found("รูปตัวอย่าง"))?;
        let jpeg = Arc::new(media::thumbnail_only(&media::read_file(&path)?, 360)?);
        self.samples.lock().unwrap_or_else(|e| e.into_inner()).insert(draft_id.to_string(), jpeg.clone());
        Ok(jpeg)
    }
}

pub fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

#[derive(Default)]
struct Group {
    photos: usize,
    unsupported: usize,
    existing: usize,
    /// ไฟล์ RAW (นับแยก — นำเข้าเฉพาะเมื่อผู้ใช้เลือกรวม RAW) และที่อยู่ในคลังแล้ว
    raw: usize,
    raw_existing: usize,
    sample: Option<PathBuf>,
    raw_sample: Option<PathBuf>,
    unreadable: Option<String>,
}

/// หากิจกรรมที่ครอบคลุมโฟลเดอร์นี้อยู่แล้ว: binding ตรงตัว หรือ binding แบบรวมโฟลเดอร์ย่อยของโฟลเดอร์แม่
/// (เพิ่ม parent แล้วเพิ่ม child → เปิดกิจกรรมเดิม ไม่สร้างกิจกรรมซ้ำ — Blueprint 2.5)
pub(crate) fn find_covering_event(
    conn: &Connection,
    folder: &Path,
    recursive: bool,
) -> Result<Option<(String, String)>> {
    let lookup = |key: &str, rec: bool| -> Result<Option<(String, String)>> {
        Ok(conn
            .query_row(
                "SELECT e.id, e.title FROM event_source_bindings b JOIN events e ON e.id = b.event_id
                 WHERE b.folder_key = ?1 AND b.recursive = ?2 AND b.active = 1 AND e.catalog_state = 'active'",
                params![key, rec],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    };
    if let Some(hit) = lookup(&util::path_key(folder), recursive)? {
        return Ok(Some(hit));
    }
    let start = if recursive { folder.parent() } else { Some(folder) };
    let mut cur = start;
    while let Some(dir) = cur {
        if let Some(hit) = lookup(&util::path_key(dir), true)? {
            return Ok(Some(hit));
        }
        cur = dir.parent();
    }
    Ok(None)
}

pub fn preview(app: &App, path: &str) -> Result<ImportPreview> {
    let root = util::canonical(path)?;
    if !root.is_dir() {
        return Err(AppError::invalid("กรุณาเลือกโฟลเดอร์ ไม่ใช่ไฟล์รูป"));
    }
    std::fs::read_dir(&root)?; // สิทธิ์อ่านโฟลเดอร์หลัก
    let root_key = util::path_key(&root);
    let root_name = util::file_name(&root);

    // รายการที่อยู่ในคลังแล้วใต้โฟลเดอร์นี้ — โหลดครั้งเดียวก่อนเดินไฟล์ จะได้ไม่ถือฐานข้อมูลค้างระหว่างสแกน
    let known: HashSet<String> = {
        let conn = app.db();
        let prefix = format!("{root_key}{}", std::path::MAIN_SEPARATOR);
        let mut stmt = conn.prepare("SELECT path_key FROM photo_locations WHERE substr(path_key, 1, ?1) = ?2")?;
        let rows = stmt.query_map(params![prefix.chars().count() as i64, prefix], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    let mut loose = Group::default();
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut unsupported_by_ext: BTreeMap<String, usize> = BTreeMap::new();

    let walker = WalkDir::new(&root).min_depth(1).sort_by_file_name().into_iter();
    for entry in walker.filter_entry(|e| !is_hidden(e.file_name())) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // อ่านโฟลเดอร์ย่อยไม่ได้: แสดงที่แถวนั้น แล้วทำรายการที่อ่านได้ต่อ
                if let Some(child) =
                    err.path().and_then(|p| p.strip_prefix(&root).ok()).and_then(|p| p.components().next())
                {
                    let name = child.as_os_str().to_string_lossy().into_owned();
                    groups.entry(name).or_default().unreadable = Some("แอปไม่มีสิทธิ์อ่านโฟลเดอร์นี้".into());
                }
                continue;
            }
        };
        let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if entry.file_type().is_dir() {
            if entry.depth() == 1 {
                groups.entry(top).or_default();
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue; // ไม่ตาม symlink/alias (Blueprint 9.5)
        }
        let (kind, ext) = media::classify(entry.path());
        let group = if entry.depth() == 1 { &mut loose } else { groups.entry(top).or_default() };
        match kind {
            FileKind::Supported => {
                group.photos += 1;
                if known.contains(&util::path_key(entry.path())) {
                    group.existing += 1;
                }
                if group.sample.is_none() {
                    group.sample = Some(entry.path().to_path_buf());
                }
            }
            FileKind::Raw => {
                group.raw += 1;
                if known.contains(&util::path_key(entry.path())) {
                    group.raw_existing += 1;
                }
                if group.raw_sample.is_none() {
                    group.raw_sample = Some(entry.path().to_path_buf());
                }
            }
            FileKind::UnsupportedImage => {
                group.unsupported += 1;
                *unsupported_by_ext.entry(ext).or_default() += 1;
            }
            FileKind::Other => {}
        }
    }

    let conn = app.db();
    let make = |kind: &'static str,
                name: &str,
                folder: &Path,
                recursive: bool,
                g: &Group,
                parent: Option<&str>|
     -> Result<DraftData> {
        let guess = if kind == "root_loose" {
            util::FolderGuess { title: format!("รูปใน {name}"), date: None }
        } else {
            guess_from_folder(name, parent)
        };
        let existing = find_covering_event(&conn, folder, recursive)?;
        let new_count = g.photos - g.existing;
        let raw_new = g.raw - g.raw_existing;
        let readable = g.unreadable.is_none();
        Ok(DraftData {
            draft: EventDraft {
                draft_id: util::new_id(),
                kind,
                folder_name: name.to_string(),
                display_path: folder.to_string_lossy().into_owned(),
                suggested_title: guess.title,
                date_source: if guess.date.is_some() { "folder_name" } else { "none" },
                suggested_date: guess.date,
                photo_count: g.photos,
                new_count,
                existing_count: g.existing,
                unsupported_count: g.unsupported,
                raw_count: g.raw,
                raw_new_count: raw_new,
                readable,
                unreadable_reason: g.unreadable.clone(),
                default_selected: readable && (new_count > 0 || (g.photos == 0 && raw_new > 0)),
                existing_event_id: existing.as_ref().map(|e| e.0.clone()),
                existing_event_title: existing.map(|e| e.1),
                has_sample: g.sample.is_some() || g.raw_sample.is_some(),
            },
            folder: folder.to_path_buf(),
            recursive,
            sample: g.sample.clone().or_else(|| g.raw_sample.clone()),
        })
    };

    let total = Group {
        photos: loose.photos + groups.values().map(|g| g.photos).sum::<usize>(),
        unsupported: loose.unsupported + groups.values().map(|g| g.unsupported).sum::<usize>(),
        existing: loose.existing + groups.values().map(|g| g.existing).sum::<usize>(),
        raw: loose.raw + groups.values().map(|g| g.raw).sum::<usize>(),
        raw_existing: loose.raw_existing + groups.values().map(|g| g.raw_existing).sum::<usize>(),
        sample: loose.sample.clone().or_else(|| groups.values().find_map(|g| g.sample.clone())),
        raw_sample: loose.raw_sample.clone().or_else(|| groups.values().find_map(|g| g.raw_sample.clone())),
        unreadable: None,
    };
    let parent_name = root.parent().map(util::file_name);
    let single = make("whole", &root_name, &root, true, &total, parent_name.as_deref())?;

    let mut children = Vec::new();
    for (name, g) in &groups {
        children.push(make("folder", name, &root.join(name), true, g, Some(&root_name))?);
    }
    let root_loose = if loose.photos > 0 || loose.raw > 0 || (loose.unsupported > 0 && !children.is_empty()) {
        Some(make("root_loose", &root_name, &root, false, &loose, None)?)
    } else {
        None
    };
    drop(conn);

    // เสนอ "หลายงาน" เมื่อโฟลเดอร์ย่อยดูเป็นงานแยกกัน: มีอย่างน้อยสองโฟลเดอร์ที่มีรูป และ
    // (มีโฟลเดอร์ที่ชื่อบอกวันที่ หรือ โฟลเดอร์ที่เลือกเองไม่บอกวันที่และไม่มีรูปวางตรง ๆ)
    let has_photos = |d: &EventDraft| d.photo_count > 0 || d.raw_count > 0;
    let with_photos = children.iter().filter(|c| has_photos(&c.draft)).count();
    let any_dated = children.iter().any(|c| has_photos(&c.draft) && c.draft.suggested_date.is_some());
    let suggested_mode = if with_photos >= 2
        && (any_dated || (single.draft.suggested_date.is_none() && loose.photos == 0 && loose.raw == 0))
    {
        "child_events"
    } else {
        "single_event"
    };

    let preview = ImportPreview {
        preview_id: util::new_id(),
        root_path: root.to_string_lossy().into_owned(),
        root_name,
        suggested_mode,
        single: single.draft.clone(),
        children: children.iter().map(|c| c.draft.clone()).collect(),
        root_loose: root_loose.as_ref().map(|d| d.draft.clone()),
        unsupported_by_ext,
        expires_at: (time::OffsetDateTime::now_utc() + PREVIEW_TTL)
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    };
    let mut drafts = vec![single];
    drafts.extend(children);
    drafts.extend(root_loose);
    let session = Arc::new(PreviewSession {
        id: preview.preview_id.clone(),
        root,
        created: Instant::now(),
        drafts,
        samples: Mutex::new(HashMap::new()),
    });
    let mut previews = app.previews.lock().unwrap_or_else(|e| e.into_inner());
    previews.retain(|_, s| s.created.elapsed() < PREVIEW_TTL);
    previews.insert(session.id.clone(), session);
    Ok(preview)
}

pub fn cancel_preview(app: &App, preview_id: &str) {
    app.previews.lock().unwrap_or_else(|e| e.into_inner()).remove(preview_id);
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftChoice {
    pub draft_id: String,
    pub title: String,
    pub event_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitArgs {
    pub preview_id: String,
    pub request_id: String,
    pub mode: String,
    pub drafts: Vec<DraftChoice>,
    #[serde(default = "yes")]
    pub analyze_faces: bool,
    #[serde(default = "yes")]
    pub watch: bool,
    /// นำไฟล์ RAW เข้าด้วย (จำไว้กับกิจกรรม ใช้ทุกครั้งที่ตรวจรูปใหม่)
    #[serde(default)]
    pub include_raw: bool,
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub event_ids: Vec<String>,
    pub job_ids: Vec<String>,
    pub created_events: usize,
    pub reused_events: usize,
}

pub fn commit(app: &App, args: CommitArgs) -> Result<CommitResult> {
    // กดซ้ำ/retry ด้วย request ID เดิม → ตอบผลเดิม ไม่สร้างกิจกรรมหรืองานเพิ่ม
    if let Some(prev) = replay::<CommitResult>(&app.db(), &args.request_id)? {
        return Ok(prev);
    }
    let session = app
        .previews
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&args.preview_id)
        .filter(|s| s.created.elapsed() < PREVIEW_TTL)
        .cloned()
        .ok_or_else(|| {
            AppError::new("PREVIEW_STALE", "รายการที่ตรวจไว้หมดอายุแล้ว กรุณาตรวจโฟลเดอร์อีกครั้งก่อนเพิ่ม").next("refresh_preview")
        })?;
    if !matches!(args.mode.as_str(), "single_event" | "child_events") {
        return Err(AppError::invalid("โหมดการเพิ่มไม่ถูกต้อง"));
    }
    if args.drafts.is_empty() {
        return Err(AppError::invalid("เลือกอย่างน้อยหนึ่งกิจกรรมก่อนเพิ่ม"));
    }
    if !session.root.is_dir() {
        return Err(AppError::source_unavailable());
    }

    let mut chosen: Vec<(&DraftData, String, Option<String>)> = Vec::new();
    let mut seen = HashSet::new();
    for c in &args.drafts {
        let data = session.drafts.iter().find(|d| d.draft.draft_id == c.draft_id).ok_or_else(|| {
            AppError::new("PREVIEW_STALE", "รายการนี้ไม่อยู่ในผลตรวจล่าสุด กรุณาตรวจโฟลเดอร์อีกครั้ง").next("refresh_preview")
        })?;
        let mode_ok = match args.mode.as_str() {
            "single_event" => data.draft.kind == "whole",
            _ => data.draft.kind != "whole",
        };
        if !mode_ok || !seen.insert(&c.draft_id) {
            return Err(AppError::invalid("รายการที่เลือกไม่ตรงกับโหมดการเพิ่ม"));
        }
        if !data.draft.readable {
            return Err(AppError::new(
                "ACCESS_DENIED",
                format!("อ่านโฟลเดอร์ “{}” ไม่ได้ จึงยังเพิ่มไม่ได้", data.draft.folder_name),
            )
            .next("grant_access"));
        }
        let title = c.title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::invalid("ชื่อกิจกรรมต้องไม่ว่าง"));
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return Err(AppError::invalid(format!("ชื่อกิจกรรมยาวได้ไม่เกิน {MAX_TITLE_CHARS} ตัวอักษร")));
        }
        if let Some(d) = &c.event_date {
            if !util::is_valid_date(d) {
                return Err(AppError::invalid("วันที่กิจกรรมไม่ถูกต้อง"));
            }
        }
        chosen.push((data, title, c.event_date.clone()));
    }

    let now = util::now();
    let mut result = CommitResult { event_ids: vec![], job_ids: vec![], created_events: 0, reused_events: 0 };
    {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        let root_key = util::path_key(&session.root);
        tx.execute(
            "INSERT INTO sources(id, root_path, path_key, import_mode, watch_enabled, analyze_faces, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path_key) DO UPDATE SET import_mode = excluded.import_mode,
               watch_enabled = excluded.watch_enabled, analyze_faces = excluded.analyze_faces",
            params![util::new_id(), root_key, root_key, args.mode, args.watch, args.analyze_faces, now],
        )?;
        let source_id: String =
            tx.query_row("SELECT id FROM sources WHERE path_key = ?1", [&root_key], |r| r.get(0))?;

        for (data, title, date) in &chosen {
            let (event_id, title) = match find_covering_event(&tx, &data.folder, data.recursive)? {
                Some((id, existing_title)) => {
                    result.reused_events += 1;
                    // เพิ่มโฟลเดอร์เดิมอีกครั้งพร้อมเลือกรวม RAW = กิจกรรมเดิมรวม RAW ตั้งแต่นี้ (ไม่มีทางกลับ — ไม่ลบรูปที่นำเข้าแล้ว)
                    if args.include_raw {
                        tx.execute(
                            "UPDATE event_source_bindings SET include_raw = 1 WHERE event_id = ?1 AND active = 1",
                            [&id],
                        )?;
                    }
                    (id, existing_title)
                }
                None => {
                    let id = util::new_id();
                    let date_source = match date {
                        Some(d) if data.draft.suggested_date.as_deref() == Some(d.as_str()) => "folder_name",
                        Some(_) => "user",
                        None => "none",
                    };
                    tx.execute(
                        "INSERT INTO events(id, title, title_key, event_date, date_source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![id, title, util::text_key(title), date, date_source, now],
                    )?;
                    tx.execute(
                        "INSERT INTO event_source_bindings(event_id, source_id, folder_path, folder_key, recursive, include_raw)
                         VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
                        params![id, source_id, util::path_key(&data.folder), data.recursive, args.include_raw],
                    )?;
                    result.created_events += 1;
                    (id, title.clone())
                }
            };
            // กิจกรรมกับงานเริ่มต้น commit พร้อมกัน: ไม่มีกิจกรรมที่ UI บอกว่ากำลังทำแต่ไม่มีงานจริง
            let job_id = jobs::enqueue_catalog(&tx, &event_id, &title)?;
            if !result.event_ids.contains(&event_id) {
                result.event_ids.push(event_id);
                result.job_ids.push(job_id);
            }
        }
        remember(&tx, &args.request_id, "commit_import", &result)?;
        db::bump_revision(&tx)?;
        tx.commit()?;
    }
    cancel_preview(app, &args.preview_id);
    app.worker.wake();
    app.notify(&["events", "jobs"]);
    Ok(result)
}

/// ผลของคำสั่งที่เคยทำสำเร็จด้วย request ID นี้ (ถ้ามี)
pub(crate) fn replay<T: serde::de::DeserializeOwned>(conn: &Connection, request_id: &str) -> Result<Option<T>> {
    if request_id.trim().is_empty() {
        return Err(AppError::invalid("คำสั่งนี้ต้องมีรหัสคำขอ"));
    }
    let stored: Option<String> =
        conn.query_row("SELECT result FROM requests WHERE request_id = ?1", [request_id], |r| r.get(0)).optional()?;
    Ok(match stored {
        Some(json) => Some(serde_json::from_str(&json).map_err(AppError::internal)?),
        None => None,
    })
}

pub(crate) fn remember<T: Serialize>(conn: &Connection, request_id: &str, kind: &str, result: &T) -> Result<()> {
    conn.execute(
        "INSERT INTO requests(request_id, kind, result, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![request_id, kind, serde_json::to_string(result).map_err(AppError::internal)?, util::now()],
    )?;
    Ok(())
}
