//! ข้อเสนอของ AI และคิวตรวจสอบใบหน้า (Blueprint S16, หมวด 10.3–10.4)
//!
//! ทุกอย่างในโมดูลนี้เป็น "ข้อเสนอ": ไม่มีฟังก์ชันใดสร้างคำยืนยัน คำยืนยันเกิดจากคำสั่งของผู้ใช้ใน `faces.rs` เท่านั้น
//! ข้อเสนอถูกนับเป็นรูปของบุคคลนั้นในหน้าบุคคลและการค้นหาได้เลย (เจ้าของผลิตภัณฑ์ตัดสิน 18 พ.ย. 2569: AI จัดให้ก่อน
//! ผู้ใช้แจ้งเฉพาะที่ผิด) แต่ยังแยกจากคำยืนยัน: ตัวอย่างที่ใช้เทียบมาจากคำยืนยันและใบหน้าตัวอย่างที่นำเข้าเท่านั้น
//! เกณฑ์ความคล้ายเป็นค่าตั้งต้นที่ยังไม่ได้ปรับเทียบกับชุดข้อมูลที่ติดป้ายแล้ว จึงแสดงเป็นระดับ ไม่แสดงเป็นเปอร์เซ็นต์

use crate::ai::{self, FaceAnalyzer};
use crate::catalog::PersonRef;
use crate::error::{AppError, Result};
use crate::people::{self, PersonSummary};
use crate::{db, util, App};
use rusqlite::{params, params_from_iter, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// cosine ต่ำสุดที่จะเสนอชื่อ และเกณฑ์ของป้าย "ใกล้เคียงมาก"
const SUGGEST_MIN: f32 = 0.45;
const CLOSE_MIN: f32 = 0.60;
const CLUSTER_MIN: f32 = 0.50;
/// คะแนนต่ำสุดของตัวตรวจใบหน้าที่จะนำใบนั้นมาเสนอชื่อหรือจัดกลุ่ม ใบที่ต่ำกว่านี้ (ท้ายทอย ภาพเบลอ สิ่งที่ไม่ใช่หน้า)
/// ยังมีกรอบในตัวดูภาพและตั้งชื่อเองได้ แต่ AI ไม่เดาให้ — เจ้าของผลิตภัณฑ์ตัดสิน 18 พ.ย. 2569 (DECISIONS ข้อ 9)
const MIN_DETECTION_SCORE: f64 = 0.6;
const REFS_PER_PERSON: usize = 12;
/// เพดานจำนวนใบหน้าไม่ทราบชื่อที่นำมาจัดกลุ่มต่อครั้ง (ใบหน้าคุณภาพดีกว่ามาก่อน) — การจับคู่กับบุคคลทำครบทุกใบ ไม่มีเพดาน
const MAX_REVIEW_FACES: usize = 12_000;
/// ระหว่างงานวิเคราะห์ ผลจับคู่ที่อายุไม่เกินนี้ยังใช้ต่อได้: identity revision ขยับทุกรูป ถ้าคิดใหม่ทุกคำขอหน้าจอจะค้าง
const MATCH_REFRESH: Duration = Duration::from_secs(20);
const MAX_CLUSTERS: usize = 200;
const MAX_GROUP_FACES: usize = 500;

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// ผลคูณจุดแบบสะสมแปดช่อง ให้คอมไพเลอร์แปลงเป็น SIMD ได้
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    for (ca, cb) in a.as_chunks::<8>().0.iter().zip(b.as_chunks::<8>().0) {
        for i in 0..8 {
            acc[i] += ca[i] * cb[i];
        }
    }
    acc.iter().sum()
}

fn level(score: f32) -> &'static str {
    if score >= CLOSE_MIN {
        "close"
    } else {
        "check"
    }
}

/// เวกเตอร์อ้างอิงของแต่ละบุคคล: ใบหน้าที่ยืนยันแล้วล่าสุด + ใบหน้าตัวอย่างที่นำเข้า (space เดียวกันเท่านั้น)
struct Refs {
    persons: Vec<(String, Vec<Vec<f32>>)>,
}

fn load_refs(conn: &Connection, space: &str) -> Result<Refs> {
    // คู่ (บุคคล, ใบหน้าที่ยืนยัน) ก่อน แล้วค่อยอ่านเวกเตอร์เฉพาะใบที่ใช้: ถ้า join เวกเตอร์ในคำสั่งเดียว
    // SQLite จะไล่จากตารางใบหน้าทั้งคลัง (ครึ่งวินาทีที่ 76,000 ใบ) แทนที่จะเริ่มจากคำยืนยันที่มีไม่กี่แถว
    let mut wanted: Vec<(String, String)> = vec![];
    let mut per_person: HashMap<String, usize> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT v.person_id, v.face_id FROM v_confirmed v
         JOIN persons pe ON pe.id = v.person_id AND pe.catalog_state = 'active'
         ORDER BY v.person_id, v.face_id DESC",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (person, face) = row?;
        let n = per_person.entry(person.clone()).or_default();
        // เผื่อใบที่ไม่มีเวกเตอร์ใน space นี้ (เช่นกรอบที่วาดเองก่อนติดตั้งโมเดล)
        if *n < REFS_PER_PERSON * 2 {
            *n += 1;
            wanted.push((person, face));
        }
    }
    let mut vectors: HashMap<String, Vec<f32>> = HashMap::new();
    for chunk in wanted.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(", ");
        let mut stmt = conn.prepare(&format!(
            "SELECT face_id, vector FROM face_embeddings WHERE space = ? AND face_id IN ({marks})"
        ))?;
        let args = std::iter::once(space).chain(chunk.iter().map(|w| w.1.as_str()));
        for row in stmt.query_map(params_from_iter(args), |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))? {
            let (face, blob) = row?;
            vectors.insert(face, normalize(ai::from_blob(&blob)));
        }
    }
    let mut map: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
    for (person, face) in wanted {
        let list = map.entry(person).or_default();
        if list.len() < REFS_PER_PERSON {
            list.extend(vectors.remove(&face));
        }
    }

    let mut stmt = conn.prepare(
        "SELECT s.person_id, s.vector FROM person_reference_samples s
         JOIN persons pe ON pe.id = s.person_id AND pe.catalog_state = 'active'
         WHERE s.vector IS NOT NULL AND s.space = ?1",
    )?;
    for row in stmt.query_map([space], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))? {
        let (person, blob) = row?;
        let list = map.entry(person).or_default();
        if list.len() < REFS_PER_PERSON * 2 {
            list.push(normalize(ai::from_blob(&blob)));
        }
    }
    let mut persons: Vec<_> = map.into_iter().collect();
    persons.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Refs { persons })
}

impl Refs {
    /// บุคคลที่คล้ายที่สุด (ค่าสูงสุดจากทุกตัวอย่างของคนนั้น) โดยข้ามคนที่ผู้ใช้ปฏิเสธไว้สำหรับใบหน้านี้
    fn best(&self, vector: &[f32], rejected: Option<&HashSet<String>>) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (i, (person, samples)) in self.persons.iter().enumerate() {
            if rejected.is_some_and(|r| r.contains(person)) {
                continue;
            }
            let score = samples.iter().map(|s| dot(s, vector)).fold(f32::MIN, f32::max);
            if score >= SUGGEST_MIN && best.is_none_or(|b| score > b.1) {
                best = Some((i, score));
            }
        }
        best
    }
}

struct Unnamed {
    face_id: String,
    revision_id: String,
    vector: Arc<[f32]>,
}

/// เวกเตอร์ (normalize แล้ว) ของใบหน้าที่อ่านจากคลังแล้ว เวกเตอร์ของใบหน้าหนึ่งไม่เปลี่ยน จึงเก็บข้ามรอบคำนวณได้
/// (การอ่าน 2 KB ต่อใบจาก SQLite ทุกรอบเป็นต้นทุนหลักเมื่อคลังมีใบหน้าหลายหมื่น; ใช้หน่วยความจำ ~2 MB ต่อพันใบ)
pub(crate) type VectorCache = HashMap<String, Arc<[f32]>>;

/// ใบหน้าที่ยังไม่มีชื่อ (active, อยู่บน revision ปัจจุบัน, ไม่มีคำยืนยัน) ที่มีเวกเตอร์ใน space นี้
/// และตัวตรวจมั่นใจพอ (กรอบที่ผู้ใช้วาดเองไม่มีคะแนน ถือว่ามั่นใจ) เรียงใบที่ตัวตรวจมั่นใจกว่าก่อน
fn load_unnamed(
    app: &App,
    conn: &Connection,
    space: &str,
    event_id: Option<&str>,
    revision_id: Option<&str>,
) -> Result<Vec<Unnamed>> {
    let ids: Vec<(String, String)> = conn
        .prepare(
            // ไม่ join ตารางเวกเตอร์ตรงนี้: แถวละ 2 KB ทำให้แค่ตรวจว่ามีเวกเตอร์ก็กินเกือบครึ่งวินาทีที่ 76,000 ใบ
            // ใบที่ไม่มีเวกเตอร์ใน space นี้ถูกคัดออกตอนอ่านเวกเตอร์ด้านล่างแทน
            "SELECT f.id, f.photo_revision_id FROM faces f
             JOIN photos p ON p.current_revision_id = f.photo_revision_id AND p.catalog_state = 'active'
             JOIN event_photos ep ON ep.photo_id = p.id
             JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
             WHERE f.status = 'active'
               AND NOT EXISTS (SELECT 1 FROM face_assignments a WHERE a.face_id = f.id AND a.revoked_at IS NULL)
               AND (?1 IS NULL OR ep.event_id = ?1) AND (?2 IS NULL OR f.photo_revision_id = ?2)
               AND COALESCE(f.score, 1) >= ?3
             ORDER BY COALESCE(f.score, 1) DESC, f.id",
        )?
        .query_map(params![event_id, revision_id, MIN_DETECTION_SCORE], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    let mut cache = app.face_vectors.lock().unwrap_or_else(|e| e.into_inner());
    let missing: Vec<&str> = ids.iter().map(|i| i.0.as_str()).filter(|id| !cache.contains_key(*id)).collect();
    for chunk in missing.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(", ");
        let mut stmt = conn.prepare(&format!(
            "SELECT face_id, vector FROM face_embeddings WHERE space = ? AND face_id IN ({marks})"
        ))?;
        let args = std::iter::once(space).chain(chunk.iter().copied());
        let rows = stmt.query_map(params_from_iter(args), |r| Ok((r.get(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        for row in rows {
            let (id, blob): (String, Vec<u8>) = row?;
            cache.insert(id, normalize(ai::from_blob(&blob)).into());
        }
    }
    if event_id.is_none() && revision_id.is_none() && cache.len() > ids.len() {
        // ชุดทั้งคลังครอบคลุมทุกขอบเขตย่อย: ใบที่ไม่อยู่ในชุดนี้แล้ว (ได้ชื่อแล้ว/ถูกลบ) ไม่ต้องเก็บ
        let live: HashSet<&str> = ids.iter().map(|i| i.0.as_str()).collect();
        cache.retain(|id, _| live.contains(id.as_str()));
    }
    Ok(ids
        .into_iter()
        .filter_map(|(face_id, revision_id)| {
            let vector = cache.get(&face_id)?.clone();
            Some(Unnamed { face_id, revision_id, vector })
        })
        .collect())
}

fn load_rejections(conn: &Connection) -> Result<HashMap<String, HashSet<String>>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT face_id, person_id FROM face_rejections")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (face, person) = row?;
        map.entry(face).or_default().insert(person);
    }
    Ok(map)
}

/// ข้อเสนอสำหรับใบหน้าที่ยังไม่มีชื่อของ revision หนึ่ง (ใช้ในตัวดูภาพ): face_id → (บุคคล, ระดับ)
pub fn suggestions_for_revision(
    app: &App,
    conn: &Connection,
    revision_id: &str,
) -> Result<HashMap<String, (String, &'static str)>> {
    let Some(space) = app.ai.space() else { return Ok(HashMap::new()) };
    let faces = load_unnamed(app, conn, space, None, Some(revision_id))?;
    if faces.is_empty() {
        return Ok(HashMap::new());
    }
    let refs = load_refs(conn, space)?;
    let rejections = load_rejections(conn)?;
    Ok(faces
        .iter()
        .filter_map(|f| {
            let (i, score) = refs.best(&f.vector, rejections.get(&f.face_id))?;
            Some((f.face_id.clone(), (refs.persons[i].0.clone(), level(score))))
        })
        .collect())
}

/// ผลจัดคิวที่คำนวณไว้ของ identity revision หนึ่ง; `get_review_faces` อ่านจากที่นี่
pub struct ReviewData {
    revision: i64,
    decisions: i64,
    event_id: Option<String>,
    unnamed: usize,
    suggested: Vec<(String, Vec<Suggested>)>,
    /// ใบหน้าที่ไม่ตรงกับใคร ชุดที่จะนำมาจัดกลุ่ม (ตัวตรวจมั่นใจที่สุดก่อน ไม่เกิน `MAX_REVIEW_FACES`)
    unknown: Vec<(String, Arc<[f32]>)>,
    /// ใบหน้าที่เข้าเกณฑ์แต่ยังไม่ถูกนำมาจัดกลุ่มรอบนี้ (เกินเพดาน)
    unexamined: usize,
    /// จัดกลุ่มเมื่อหน้าตรวจสอบขอครั้งแรก: รายชื่อบุคคลและการค้นหาใช้แค่ผลจับคู่ จึงไม่ต้องรอส่วนนี้
    grouped: OnceLock<Grouped>,
}

struct Grouped {
    clusters: Vec<Vec<String>>,
    singles: usize,
}

impl ReviewData {
    /// จัดกลุ่มใบหน้าที่ไม่ตรงกับใคร: กลุ่มชั่วคราวเพื่อช่วยตรวจ ไม่ใช่บุคคล
    fn grouped(&self) -> &Grouped {
        self.grouped.get_or_init(|| {
            let mut clusters: Vec<(Vec<f32>, Vec<String>)> = vec![];
            for (face_id, vector) in &self.unknown {
                let hit = clusters
                    .iter_mut()
                    .map(|c| (dot(&c.0, vector), c))
                    .filter(|(score, _)| *score >= CLUSTER_MIN)
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                match hit {
                    Some((_, cluster)) => {
                        let n = cluster.1.len() as f32;
                        let mean: Vec<f32> = cluster.0.iter().zip(vector.iter()).map(|(c, v)| c * n + v).collect();
                        cluster.0 = normalize(mean);
                        cluster.1.push(face_id.clone());
                    }
                    None => clusters.push((vector.to_vec(), vec![face_id.clone()])),
                }
            }
            let mut groups: Vec<Vec<String>> = clusters.into_iter().map(|c| c.1).collect();
            groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
            let clusters: Vec<Vec<String>> = groups.into_iter().filter(|g| g.len() >= 2).take(MAX_CLUSTERS).collect();
            let named = self.suggested.iter().map(|s| s.1.len()).sum::<usize>();
            let singles =
                self.unnamed.saturating_sub(named + clusters.iter().map(Vec::len).sum::<usize>() + self.unexamined);
            Grouped { clusters, singles }
        })
    }
}

/// ใบหน้าหนึ่งใบที่ AI จัดให้บุคคลหนึ่ง
struct Suggested {
    face_id: String,
    revision_id: String,
    level: &'static str,
}

/// ผลจับคู่ทั้งคลังใน `temp.ai_matches` ตรงกับข้อมูลรุ่นไหน
pub(crate) struct MatchState {
    identity: i64,
    decisions: i64,
    at: Instant,
}

fn compute(app: &App, conn: &Connection, event_id: Option<&str>) -> Result<ReviewData> {
    let revision = db::identity_revision(conn)?;
    let unnamed_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM faces f
         JOIN photos p ON p.current_revision_id = f.photo_revision_id AND p.catalog_state = 'active'
         JOIN event_photos ep ON ep.photo_id = p.id JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
         WHERE f.status = 'active' AND (?1 IS NULL OR ep.event_id = ?1)
           AND NOT EXISTS (SELECT 1 FROM face_assignments a WHERE a.face_id = f.id AND a.revoked_at IS NULL)",
        [event_id],
        |r| r.get(0),
    )?;
    let mut data = ReviewData {
        revision,
        decisions: db::decision_revision(conn)?,
        event_id: event_id.map(str::to_string),
        unnamed: unnamed_total as usize,
        suggested: vec![],
        unknown: vec![],
        unexamined: 0,
        grouped: OnceLock::new(),
    };
    let Some(space) = app.ai.space() else { return Ok(data) };
    let faces = load_unnamed(app, conn, space, event_id, None)?;
    let refs = load_refs(conn, space)?;
    let rejections = load_rejections(conn)?;

    // จับคู่กับบุคคลที่มีอยู่ (ขนานหลาย thread: งานนี้เป็นเลขล้วน ไม่แตะฐานข้อมูล)
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let chunk = faces.len().div_ceil(threads).max(1);
    let matches: Vec<Option<(usize, f32)>> = std::thread::scope(|s| {
        let handles: Vec<_> = faces
            .chunks(chunk)
            .map(|part| {
                let (refs, rejections) = (&refs, &rejections);
                s.spawn(move || {
                    part.iter().map(|f| refs.best(&f.vector, rejections.get(&f.face_id))).collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });

    let mut by_person: HashMap<usize, Vec<(&Unnamed, f32)>> = HashMap::new();
    let mut unknown: Vec<&Unnamed> = vec![];
    for (face, m) in faces.iter().zip(&matches) {
        match m {
            Some((i, score)) => by_person.entry(*i).or_default().push((face, *score)),
            None => unknown.push(face),
        }
    }
    let mut suggested: Vec<(String, Vec<(&Unnamed, f32)>)> =
        by_person.into_iter().map(|(i, list)| (refs.persons[i].0.clone(), list)).collect();
    for (_, list) in &mut suggested {
        list.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.face_id.cmp(&b.0.face_id)));
    }
    suggested.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    data.suggested = suggested
        .into_iter()
        .map(|(person, list)| {
            let list = list.into_iter().map(|(f, score)| Suggested {
                face_id: f.face_id.clone(),
                revision_id: f.revision_id.clone(),
                level: level(score),
            });
            (person, list.collect())
        })
        .collect();

    data.unexamined = unknown.len().saturating_sub(MAX_REVIEW_FACES);
    data.unknown = unknown.into_iter().take(MAX_REVIEW_FACES).map(|f| (f.face_id.clone(), f.vector.clone())).collect();
    Ok(data)
}

/// ผลจัดคิวล่าสุดของขอบเขตที่ขอ; คำนวณใหม่เมื่อข้อมูลตัวตนเปลี่ยนหรือเปลี่ยนขอบเขต
fn current(app: &App, event_id: Option<&str>) -> Result<Arc<ReviewData>> {
    current_with(app, &app.db(), event_id)
}

fn current_with(app: &App, conn: &Connection, event_id: Option<&str>) -> Result<Arc<ReviewData>> {
    let revision = db::identity_revision(conn)?;
    let mut cache = app.review_cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hit) = cache.as_ref().filter(|d| d.revision == revision && d.event_id.as_deref() == event_id) {
        return Ok(hit.clone());
    }
    let data = Arc::new(compute(app, conn, event_id)?);
    *cache = Some(data.clone());
    Ok(data)
}

/// ทำให้ `temp.ai_matches` (ใบหน้าที่ยังไม่มีคำยืนยัน → บุคคลที่ AI จัดให้ ทั้งคลัง) ตรงกับข้อมูลล่าสุด
/// เรียกก่อนคำสั่งที่นับรูปของ AI ร่วมกับรูปที่ยืนยันแล้ว (รายชื่อบุคคล, ค้นหารูป)
pub fn ensure_matches(app: &App) -> Result<()> {
    let conn = app.db();
    let (identity, decisions) = (db::identity_revision(&conn)?, db::decision_revision(&conn)?);
    let mut state = app.match_state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = state.as_ref() {
        // การตัดสินของผู้ใช้ต้องเห็นผลทันที ส่วนใบหน้าที่งานวิเคราะห์เพิ่งเพิ่มรอรอบถัดไปได้
        let analysing = || -> Result<bool> {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = 'catalog' AND state IN ('queued', 'running'))",
                [],
                |r| r.get(0),
            )?)
        };
        if s.identity == identity || (s.decisions == decisions && s.at.elapsed() < MATCH_REFRESH && analysing()?) {
            return Ok(());
        }
    }
    let data = current_with(app, &conn, None)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM temp.ai_matches", [])?;
    {
        let mut insert =
            tx.prepare("INSERT OR IGNORE INTO temp.ai_matches(face_id, person_id, revision_id) VALUES (?1, ?2, ?3)")?;
        for (person, faces) in &data.suggested {
            for face in faces {
                insert.execute(params![face.face_id, person, face.revision_id])?;
            }
        }
    }
    tx.commit()?;
    *state = Some(MatchState { identity, decisions, at: Instant::now() });
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedGroup {
    pub person: PersonSummary,
    pub face_count: usize,
    pub photo_count: usize,
    pub sample_face_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownCluster {
    pub cluster_id: String,
    pub face_count: usize,
    pub sample_face_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOverview {
    pub review_revision: i64,
    pub unnamed_face_count: usize,
    pub suggested: Vec<SuggestedGroup>,
    pub clusters: Vec<UnknownCluster>,
    pub single_face_count: usize,
    /// ใบหน้าที่ยังไม่ได้นำมาจัดกลุ่มรอบนี้ (จะเข้ามาเมื่อชุดแรกถูกตั้งชื่อหรือปัดออกไปแล้ว)
    pub unexamined_face_count: usize,
    pub photos_pending_analysis: i64,
}

pub fn overview(app: &App, event_id: Option<&str>) -> Result<ReviewOverview> {
    let data = current(app, event_id)?;
    let conn = app.db();
    let photos_pending_analysis = conn.query_row(
        "SELECT COUNT(*) FROM photos p JOIN photo_revisions r ON r.id = p.current_revision_id
         JOIN event_photos ep ON ep.photo_id = p.id JOIN events e ON e.id = ep.event_id AND e.catalog_state = 'active'
         WHERE p.catalog_state = 'active' AND r.face_status = 'not_analyzed' AND r.thumbnail_status <> 'failed'
           AND (?1 IS NULL OR ep.event_id = ?1)",
        [event_id],
        |r| r.get(0),
    )?;
    let suggested = data
        .suggested
        .iter()
        .map(|(person_id, faces)| {
            Ok(SuggestedGroup {
                person: people::summary(&conn, person_id)?,
                face_count: faces.len(),
                photo_count: faces.iter().map(|f| f.revision_id.as_str()).collect::<HashSet<_>>().len(),
                sample_face_ids: faces.iter().take(8).map(|f| f.face_id.clone()).collect(),
            })
        })
        .collect::<Result<_>>()?;
    Ok(ReviewOverview {
        review_revision: data.revision,
        unnamed_face_count: data.unnamed,
        suggested,
        clusters: data
            .grouped()
            .clusters
            .iter()
            .map(|c| UnknownCluster {
                cluster_id: c[0].clone(),
                face_count: c.len(),
                sample_face_ids: c.iter().take(8).cloned().collect(),
            })
            .collect(),
        single_face_count: data.grouped().singles,
        unexamined_face_count: data.unexamined,
        photos_pending_analysis,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFace {
    pub face_id: String,
    pub photo_id: String,
    pub filename: String,
    pub event_id: String,
    pub event_title: String,
    pub level: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGroupFaces {
    pub review_revision: i64,
    pub faces: Vec<ReviewFace>,
    pub total: usize,
}

pub fn group_faces(
    app: &App,
    kind: &str,
    id: &str,
    event_id: Option<&str>,
    review_revision: i64,
) -> Result<ReviewGroupFaces> {
    let stale = || AppError::new("PREVIEW_STALE", "รายการตรวจสอบเปลี่ยนไปแล้ว กำลังโหลดรายการล่าสุด").next("refresh_preview");
    // เสิร์ฟจากคิวที่ออกไปแล้วเท่านั้น (ไม่คิดใหม่): ใบหน้าที่เพิ่งวิเคราะห์เสร็จไม่ทำให้รายการที่ผู้ใช้ดูอยู่เก่า
    // มีแต่การตัดสินของผู้ใช้ที่ทำให้เก่า เพราะสมาชิกของกลุ่มเปลี่ยนจริง
    let data = app
        .review_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .filter(|d| d.revision == review_revision && d.event_id.as_deref() == event_id)
        .ok_or_else(stale)?;
    if db::decision_revision(&app.db())? != data.decisions {
        return Err(stale());
    }
    let members: Vec<(String, Option<&'static str>)> = match kind {
        "person" => data
            .suggested
            .iter()
            .find(|s| s.0 == id)
            .map(|s| s.1.iter().map(|f| (f.face_id.clone(), Some(f.level))).collect()),
        "cluster" => {
            data.grouped().clusters.iter().find(|c| c[0] == id).map(|c| c.iter().map(|f| (f.clone(), None)).collect())
        }
        _ => return Err(AppError::invalid("ชนิดกลุ่มไม่ถูกต้อง")),
    }
    .ok_or_else(stale)?;
    let conn = app.db();
    let mut stmt = conn.prepare(
        "SELECT p.id, l.filename, e.id, e.title FROM faces f
         JOIN photos p ON p.current_revision_id = f.photo_revision_id
         JOIN photo_locations l ON l.photo_id = p.id
         JOIN event_photos ep ON ep.photo_id = p.id JOIN events e ON e.id = ep.event_id
         WHERE f.id = ?1",
    )?;
    let mut faces = vec![];
    for (face_id, level) in members.iter().take(MAX_GROUP_FACES) {
        let row = stmt.query_row([face_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)));
        if let Ok((photo_id, filename, event_id, event_title)) = row {
            faces.push(ReviewFace {
                face_id: face_id.clone(),
                photo_id,
                filename,
                event_id,
                event_title,
                level: *level,
            });
        }
    }
    Ok(ReviewGroupFaces { review_revision: data.revision, faces, total: members.len() })
}

/// "ไม่ใช่บุคคลนี้" ต่อข้อเสนอ: จำไว้ไม่เสนอคู่นี้อีก ใบหน้า รูป และบุคคลไม่ถูกลบ
pub fn reject(app: &App, face_ids: &[String], person_id: &str) -> Result<i64> {
    if face_ids.is_empty() || face_ids.len() > 2000 {
        return Err(AppError::invalid("เลือกใบหน้าที่จะปฏิเสธข้อเสนอ (ไม่เกิน 2,000 ใบต่อครั้ง)"));
    }
    let revision = {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        people::summary(&tx, person_id)?;
        let now = util::now();
        for face_id in face_ids {
            tx.execute(
                "INSERT OR IGNORE INTO face_rejections(face_id, person_id, created_at)
                 SELECT id, ?2, ?3 FROM faces WHERE id = ?1",
                params![face_id, person_id, now],
            )?;
        }
        tx.execute(
            "INSERT INTO operations(id, kind, summary, created_at) VALUES (?1, 'reject_suggestions', ?2, ?3)",
            params![util::new_id(), serde_json::json!({ "faceIds": face_ids, "personId": person_id }).to_string(), now],
        )?;
        db::bump_decision(&tx)?;
        let revision = db::bump_revision(&tx)?;
        tx.commit()?;
        revision
    };
    app.notify(&["people", "photos"]);
    Ok(revision)
}

/// ย้อน "ไม่ใช่บุคคลนี้": ลบคำปฏิเสธของคู่ ใบหน้า–บุคคล ใบหน้านั้นกลับไปให้ AI จัดได้ตามเดิม (ไม่สร้างคำยืนยัน)
pub fn restore(app: &App, face_ids: &[String], person_id: &str) -> Result<i64> {
    if face_ids.is_empty() || face_ids.len() > 2000 {
        return Err(AppError::invalid("เลือกใบหน้าที่จะคืน (ไม่เกิน 2,000 ใบต่อครั้ง)"));
    }
    let revision = {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        people::summary(&tx, person_id)?;
        for face_id in face_ids {
            tx.execute(
                "DELETE FROM face_rejections WHERE face_id = ?1 AND person_id = ?2",
                params![face_id, person_id],
            )?;
        }
        tx.execute(
            "INSERT INTO operations(id, kind, summary, created_at) VALUES (?1, 'restore_suggestions', ?2, ?3)",
            params![
                util::new_id(),
                serde_json::json!({ "faceIds": face_ids, "personId": person_id }).to_string(),
                util::now()
            ],
        )?;
        db::bump_decision(&tx)?;
        let revision = db::bump_revision(&tx)?;
        tx.commit()?;
        revision
    };
    app.notify(&["people", "photos"]);
    Ok(revision)
}

/// สร้างเวกเตอร์ให้ใบหน้าตัวอย่างที่นำเข้าและยังไม่มีเวกเตอร์ใน space ปัจจุบัน; คืนจำนวนที่ทำสำเร็จ
/// เรียกจาก worker เมื่อโมเดลพร้อม ทำทีละชุดเล็กเพื่อไม่ขวางงานอื่น
pub(crate) fn embed_pending_samples(
    conn: &Connection,
    analyzer: &dyn FaceAnalyzer,
    space: &str,
    limit: usize,
) -> Result<usize> {
    let pending: Vec<(String, Vec<u8>)> = conn
        .prepare(
            "SELECT id, thumbnail FROM person_reference_samples
             WHERE (vector IS NULL OR space <> ?1) AND attempts < 2 ORDER BY id LIMIT ?2",
        )?
        .query_map(params![space, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut done = 0;
    for (id, thumbnail) in &pending {
        match analyzer.embed_crop(thumbnail) {
            Ok(Some(vector)) => {
                conn.execute(
                    "UPDATE person_reference_samples SET vector = ?2, space = ?3, attempts = attempts + 1 WHERE id = ?1",
                    params![id, ai::to_blob(&vector), space],
                )?;
                done += 1;
            }
            _ => {
                conn.execute("UPDATE person_reference_samples SET attempts = attempts + 1 WHERE id = ?1", [id])?;
            }
        }
    }
    if !pending.is_empty() {
        db::bump_identity(conn)?;
        db::bump_revision(conn)?;
    }
    Ok(done)
}

/// บุคคลของข้อเสนอ ในรูป PersonRef สำหรับ DTO ของใบหน้า
pub(crate) fn person_ref(conn: &Connection, person_id: &str) -> Result<PersonRef> {
    Ok(people::summary(conn, person_id)?.person)
}
