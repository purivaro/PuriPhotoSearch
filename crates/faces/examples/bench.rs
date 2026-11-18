//! วัดเวลาวิเคราะห์ใบหน้าของชุดโมเดลกับรูปจริง (อ่านอย่างเดียว พิมพ์เฉพาะตัวเลข)
//!   cargo run --release -p puri-faces --example bench -- <โฟลเดอร์โมเดล> <โฟลเดอร์รูป> [จำนวนรูป] [threads]
use puri_faces::{prepare, FaceEngine};
use std::time::{Duration, Instant};

fn main() -> puri_faces::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (models, photos) = (std::path::PathBuf::from(&args[1]), std::path::PathBuf::from(&args[2]));
    let limit: usize = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(40);
    let threads: usize = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(5);
    let mut files: Vec<_> = std::fs::read_dir(&photos)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jpg")))
        .collect();
    files.sort();
    files.truncate(limit);
    let start = Instant::now();
    let mut engine = FaceEngine::load(&models, "det_10g.onnx", "w600k_r50.onnx", threads)?;
    let load = start.elapsed();
    let (mut t_prep, mut t_det, mut t_emb, mut faces) = (Duration::ZERO, Duration::ZERO, Duration::ZERO, 0usize);
    for (i, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path)?;
        let t = Instant::now();
        let prepared = prepare(&bytes)?;
        let prep = t.elapsed();
        let t = Instant::now();
        let found = engine.detect(&prepared.image)?;
        let det = t.elapsed();
        let t = Instant::now();
        for d in &found {
            engine.embed(&prepared.image, &d.landmarks)?;
        }
        if i > 0 {
            // รูปแรกรวมเวลาอุ่นเครื่องของ runtime จึงไม่นับ
            t_prep += prep;
            t_det += det;
            t_emb += t.elapsed();
            faces += found.len();
        }
    }
    let n = (files.len() - 1).max(1) as f32;
    println!(
        "threads {threads} · โหลดโมเดล {:.0} ms · ต่อรูป: ถอดรหัส+ย่อ {:.0} ms · ตรวจใบหน้า {:.0} ms · เวกเตอร์ {:.0} ms ({:.1} ใบหน้า/รูป, {:.1} ms/ใบหน้า)",
        load.as_secs_f32() * 1000.0,
        t_prep.as_secs_f32() * 1000.0 / n,
        t_det.as_secs_f32() * 1000.0 / n,
        t_emb.as_secs_f32() * 1000.0 / n,
        faces as f32 / n,
        t_emb.as_secs_f32() * 1000.0 / faces.max(1) as f32
    );
    Ok(())
}
