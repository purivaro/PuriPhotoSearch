//! เทียบผลของ Rust กับฐานข้อมูลของรุ่น Python: ใบหน้าเดียวกันได้เวกเตอร์ใน space เดียวกันหรือไม่
//!
//!   cargo run --release -p puri-faces --example legacy_compat -- [จำนวนรูปสูงสุด]
//!
//! อ่านฐานเดิมแบบอ่านอย่างเดียว (`immutable=1`) และอ่านไฟล์รูปที่ยังอยู่ในตำแหน่งเดิม ไม่เขียนอะไรลงดิสก์
//! พิมพ์เฉพาะตัวเลขรวม ไม่พิมพ์ชื่อบุคคลหรือชื่อไฟล์

use puri_faces::{cosine, iou, prepare, FaceEngine};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct LegacyFace {
    bbox: [f32; 4],
    embedding: Vec<f32>,
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn encode(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn main() -> puri_faces::Result<()> {
    let limit: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(200);
    let home = PathBuf::from(std::env::var("HOME")?);
    let db_path = home.join("Library/Application Support/PuriPhotoSearch/photosearch.db");
    let model_dir = home.join(".insightface/models/buffalo_sc");
    let conn = Connection::open_with_flags(
        format!("file:{}?mode=ro&immutable=1", encode(&db_path.to_string_lossy())),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;

    // รูปที่ไฟล์ยังอยู่ กระจายทั่วฐาน (ไม่เอาเฉพาะโฟลเดอร์แรก)
    let all: Vec<(i64, String)> = conn
        .prepare("SELECT id, file_path FROM photos WHERE face_count > 0 ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let existing: Vec<&(i64, String)> = all.iter().filter(|(_, p)| Path::new(p).is_file()).collect();
    let step = (existing.len() / limit.max(1)).max(1);
    let sample: Vec<&(i64, String)> = existing.iter().step_by(step).take(limit).copied().collect();
    println!("รูปที่มีใบหน้าในฐานเดิม {} · ไฟล์ยังอยู่ {} · ใช้ทดสอบ {}", all.len(), existing.len(), sample.len());

    let mut engine = FaceEngine::load(&model_dir, 4)?;
    let mut faces_stmt =
        conn.prepare("SELECT bbox_x1, bbox_y1, bbox_x2, bbox_y2, embedding FROM faces WHERE photo_id = ?1")?;

    let (mut same, mut other, mut ious): (Vec<f32>, Vec<f32>, Vec<f32>) = (vec![], vec![], vec![]);
    let (mut legacy_total, mut rust_total, mut matched, mut unreadable) = (0usize, 0usize, 0usize, 0usize);
    let (mut t_prepare, mut t_detect, mut t_embed) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let mut portrait_photos = 0usize;
    // รูปแนวตั้งแยกนับ: ถ้าพิกัดเดิมไม่ได้อยู่บนภาพที่หมุนตาม EXIF แล้ว กรอบจะจับคู่ไม่ได้
    let (mut portrait_legacy, mut portrait_matched) = (0usize, 0usize);

    for (photo_id, path) in &sample {
        let legacy: Vec<LegacyFace> = faces_stmt
            .query_map([photo_id], |r| {
                let blob: Vec<u8> = r.get(4)?;
                let embedding = blob.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
                Ok(LegacyFace { bbox: [r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?], embedding })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let Ok(bytes) = std::fs::read(path) else {
            unreadable += 1;
            continue;
        };
        let start = Instant::now();
        let Ok(prepared) = prepare(&bytes) else {
            unreadable += 1;
            continue;
        };
        t_prepare += start.elapsed();
        let portrait = prepared.full_height > prepared.full_width;
        if portrait {
            portrait_photos += 1;
            portrait_legacy += legacy.len();
        }
        let start = Instant::now();
        let detections = engine.detect(&prepared.image)?;
        t_detect += start.elapsed();
        let start = Instant::now();
        let embeddings: Vec<Vec<f32>> = detections
            .iter()
            .map(|d| engine.embed(&prepared.image, &d.landmarks))
            .collect::<puri_faces::Result<_>>()?;
        t_embed += start.elapsed();

        legacy_total += legacy.len();
        rust_total += detections.len();
        let scale = prepared.scale_to_full();
        let boxes: Vec<[f32; 4]> = detections.iter().map(|d| d.bbox.map(|v| v * scale)).collect();
        for face in &legacy {
            // จับคู่ด้วยตำแหน่งกรอบ (พิกัดเดิมเป็นพิกเซลของภาพเต็ม)
            let best =
                boxes.iter().enumerate().map(|(i, b)| (i, iou(&face.bbox, b))).max_by(|a, b| a.1.total_cmp(&b.1));
            let Some((i, overlap)) = best.filter(|(_, o)| *o >= 0.5) else { continue };
            matched += 1;
            if portrait {
                portrait_matched += 1;
            }
            ious.push(overlap);
            same.push(cosine(&face.embedding, &embeddings[i]));
            // เส้นฐาน: เวกเตอร์เดิมของใบหน้านี้ เทียบกับใบหน้า "อื่น" ในรูปเดียวกันที่ Rust สร้าง
            other.extend(
                embeddings.iter().enumerate().filter(|(j, _)| *j != i).take(3).map(|(_, e)| cosine(&face.embedding, e)),
            );
        }
    }

    same.sort_by(f32::total_cmp);
    other.sort_by(f32::total_cmp);
    ious.sort_by(f32::total_cmp);
    let n = (sample.len() - unreadable).max(1) as f32;
    let frac = |v: &[f32], t: f32| 100.0 * v.iter().filter(|x| **x >= t).count() as f32 / v.len().max(1) as f32;
    println!("รูปที่อ่านได้ {} (แนวตั้ง {portrait_photos}) · อ่านไม่ได้ {unreadable}", sample.len() - unreadable);
    println!(
        "ใบหน้า: ฐานเดิม {legacy_total} · Rust ตรวจพบ {rust_total} · จับคู่ตำแหน่งได้ {matched} ({:.1}% ของฐานเดิม)",
        100.0 * matched as f32 / legacy_total.max(1) as f32
    );
    println!(
        "  เฉพาะรูปแนวตั้ง: ฐานเดิม {portrait_legacy} · จับคู่ได้ {portrait_matched} ({:.1}%)",
        100.0 * portrait_matched as f32 / portrait_legacy.max(1) as f32
    );
    println!("IoU ของกรอบที่จับคู่: มัธยฐาน {:.3} · p5 {:.3}", percentile(&ious, 0.5), percentile(&ious, 0.05));
    println!(
        "cosine ใบหน้าเดียวกัน (เดิม ↔ Rust): ต่ำสุด {:.3} · p1 {:.3} · p5 {:.3} · มัธยฐาน {:.3} · เฉลี่ย {:.3}",
        same.first().copied().unwrap_or(f32::NAN),
        percentile(&same, 0.01),
        percentile(&same, 0.05),
        percentile(&same, 0.5),
        same.iter().sum::<f32>() / same.len().max(1) as f32
    );
    println!(
        "  ≥ 0.95: {:.1}% · ≥ 0.90: {:.1}% · ≥ 0.80: {:.1}% · ≥ 0.45 (เกณฑ์ของรุ่นเดิม): {:.1}%",
        frac(&same, 0.95),
        frac(&same, 0.9),
        frac(&same, 0.8),
        frac(&same, 0.45)
    );
    println!(
        "cosine กับใบหน้าอื่นในรูปเดียวกัน ({} คู่): มัธยฐาน {:.3} · p95 {:.3} · p99 {:.3} · สูงสุด {:.3} · ≥ 0.45: {:.1}%",
        other.len(),
        percentile(&other, 0.5),
        percentile(&other, 0.95),
        percentile(&other, 0.99),
        other.last().copied().unwrap_or(f32::NAN),
        frac(&other, 0.45)
    );
    println!(
        "เวลาเฉลี่ยต่อรูป: ถอดรหัส+ย่อ {:.0} ms · ตรวจใบหน้า {:.0} ms · สร้างเวกเตอร์ {:.1} ms/ใบหน้า ({:.1} ใบหน้า/รูป)",
        t_prepare.as_secs_f32() * 1000.0 / n,
        t_detect.as_secs_f32() * 1000.0 / n,
        t_embed.as_secs_f32() * 1000.0 / rust_total.max(1) as f32,
        rust_total as f32 / n
    );
    Ok(())
}
