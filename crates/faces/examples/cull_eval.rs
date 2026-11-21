//! วัดตัวคัดภาพ (`puri_core::cull`) กับการคัดจริงของผู้ใช้: รูป RAW/JPEG ทั้งหมดของงาน เทียบกับรูปที่ผู้ใช้เลือกส่งออก
//! (ชื่อเดียวกันในโฟลเดอร์ที่ส่งออก = ผู้ใช้เลือก) — DECISIONS ข้อ 37
//!
//! ```text
//! cargo run --release -p puri-faces --example cull_eval -- <โฟลเดอร์รูปทั้งหมด> <โฟลเดอร์รูปที่เลือก> <โฟลเดอร์โมเดลใบหน้า> <ไฟล์ผลวัด.json>
//! ```
//!
//! อ่านไฟล์อย่างเดียว · ผลวัดของแต่ละรูปเก็บใน `<ไฟล์ผลวัด.json>` (ครั้งต่อไปใช้ซ้ำ ไม่วัดใหม่ — ลบไฟล์เมื่อเปลี่ยนสูตรวัด)

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use puri_core::cull::{self, Decision, Reason, Shot};
    use puri_core::picks::Flag;
    use std::collections::{BTreeMap, HashSet};
    use std::path::{Path, PathBuf};

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Row {
        name: String,
        time: Option<i64>,
        quality: cull::Quality,
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("ใช้: cull_eval <โฟลเดอร์รูปทั้งหมด> <โฟลเดอร์รูปที่เลือก> <โฟลเดอร์โมเดลใบหน้า> <ไฟล์ผลวัด.json>");
        std::process::exit(2);
    }
    let (all_dir, chosen_dir, models, cache) =
        (PathBuf::from(&args[0]), PathBuf::from(&args[1]), PathBuf::from(&args[2]), PathBuf::from(&args[3]));
    let stem = |p: &Path| p.file_stem().unwrap_or_default().to_string_lossy().to_uppercase();
    let chosen: HashSet<String> = std::fs::read_dir(&chosen_dir)
        .expect("อ่านโฟลเดอร์รูปที่เลือก")
        .filter_map(|e| e.ok().map(|e| stem(&e.path())))
        .collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&all_dir)
        .expect("อ่านโฟลเดอร์รูป")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let ext = p.extension().unwrap_or_default().to_string_lossy().to_lowercase();
            puri_core::raw::is_raw_ext(&ext) || matches!(ext.as_str(), "jpg" | "jpeg")
        })
        .collect();
    files.sort();

    let rows: Vec<Row> = match std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        Some(rows) => rows,
        None => {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let mut engine = puri_faces::FaceEngine::load(&models, "det_10g.onnx", "w600k_r50.onnx", threads)
                .expect("โหลดโมเดลใบหน้า");
            let started = std::time::Instant::now();
            let mut rows = Vec::new();
            for (n, f) in files.iter().enumerate() {
                let bytes = std::fs::read(f).expect("อ่านรูป");
                let img = puri_core::media::decode_upright(&bytes, cull::MEASURE_EDGE).expect("ถอดรหัสรูป").to_rgb8();
                let (w, h) = (f64::from(img.width()), f64::from(img.height()));
                let faces: Vec<[f64; 4]> = engine
                    .detect(&img)
                    .expect("หาใบหน้า")
                    .iter()
                    .map(|d| {
                        [
                            d.bbox[0] as f64 / w,
                            d.bbox[1] as f64 / h,
                            (d.bbox[2] - d.bbox[0]) as f64 / w,
                            (d.bbox[3] - d.bbox[1]) as f64 / h,
                        ]
                    })
                    .collect();
                let taken = if puri_core::raw::is_raw_bytes(&bytes) {
                    puri_core::raw::info(&bytes).ok().and_then(|i| i.exif.taken_at_local)
                } else {
                    puri_core::media::read_exif(&bytes).taken_at_local
                };
                rows.push(Row {
                    name: stem(f),
                    time: taken.as_deref().and_then(cull::seconds),
                    quality: cull::measure(&img, &faces),
                });
                eprint!("\rวัดแล้ว {}/{} ({:.1} วินาที)", n + 1, files.len(), started.elapsed().as_secs_f64());
            }
            eprintln!();
            std::fs::write(&cache, serde_json::to_vec(&rows).unwrap()).expect("เขียนผลวัด");
            rows
        }
    };

    let shots: Vec<Shot> = rows
        .iter()
        .map(|r| Shot { photo_id: r.name.clone(), time: r.time, quality: Some(r.quality.clone()), user: None })
        .collect();
    let total_chosen = rows.iter().filter(|r| chosen.contains(&r.name)).count();
    println!(
        "รูปทั้งหมด {} · ผู้ใช้เลือก {} ({:.0}%)",
        rows.len(),
        total_chosen,
        100.0 * total_chosen as f64 / rows.len() as f64
    );
    println!("ระดับ | เก็บ | ตรงกับที่ผู้ใช้เลือก (recall) | ที่เก็บเป็นรูปที่ผู้ใช้เลือก (precision) | ผู้ใช้เลือกแต่ไม่เอา: ซ้ำ/เบลอ");
    for level in 1..=cull::LEVELS {
        let plan: Vec<Decision> = cull::plan(&shots, level);
        let kept: HashSet<&str> = plan.iter().filter(|d| d.flag == Flag::Pick).map(|d| d.photo_id.as_str()).collect();
        let hit = kept.iter().filter(|k| chosen.contains(**k)).count();
        let lost: Vec<&Decision> =
            plan.iter().filter(|d| d.flag == Flag::Reject && chosen.contains(&d.photo_id)).collect();
        let dup = lost.iter().filter(|d| matches!(d.reason, Reason::Duplicate { .. })).count();
        println!(
            "{level} | {} | {:.0}% | {:.0}% | {} / {}",
            kept.len(),
            100.0 * hit as f64 / total_chosen.max(1) as f64,
            100.0 * hit as f64 / kept.len().max(1) as f64,
            dup,
            lost.len() - dup
        );
        if std::env::var_os("DETAIL").is_some() {
            let by: BTreeMap<&str, &Decision> = plan.iter().map(|d| (d.photo_id.as_str(), d)).collect();
            for r in &rows {
                let d = by[r.name.as_str()];
                let mark = if chosen.contains(&r.name) { "*" } else { " " };
                let q = &r.quality;
                println!(
                    "  {}{mark} {:?} {:?} t={:?} s={:.1} f={:?} faces={}",
                    r.name,
                    d.flag,
                    d.reason,
                    r.time.map(|t| t % 100_000),
                    q.sharpness,
                    q.face_sharpness.map(|f| (f * 10.0).round() / 10.0),
                    q.faces
                );
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {
    eprintln!("Mac ชิป Intel ไม่มี ONNX Runtime สำเร็จรูป");
}
