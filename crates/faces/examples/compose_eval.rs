//! วัดตัวจัดองค์ประกอบ (`puri_core::compose`) กับที่ผู้ใช้ปรับเอียง/ครอปเองใน Lightroom (DECISIONS ข้อ 38)
//!
//! ```text
//! cargo run --release -p puri-faces --example compose_eval -- <โฟลเดอร์ RAW> <โฟลเดอร์ JPEG จาก Lightroom> <โฟลเดอร์โมเดลใบหน้า> [โฟลเดอร์ภาพ]
//! ```
//!
//! อ่านไฟล์อย่างเดียว · ความเอียง: เทียบกับ `CropAngle` (บวก = เส้นในภาพเอียงตามเข็มนาฬิกา) · ครอป: เฉพาะรูปที่ครอปโดยไม่เอียง
//! เทียบ IoU ของกรอบที่เสนอกับกรอบของผู้ใช้ (ทั้งภาพเป็นเกณฑ์ขั้นต่ำ) · ใส่ `[โฟลเดอร์ภาพ]` แล้วเขียนภาพที่วาดกรอบไว้ดูด้วยตา

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use puri_core::compose;
    use puri_core::edit::Rect;
    use puri_core::pair::{self, Lightroom};
    use std::path::PathBuf;

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("ใช้: compose_eval <โฟลเดอร์ RAW> <โฟลเดอร์ JPEG จาก Lightroom> <โฟลเดอร์โมเดลใบหน้า> [โฟลเดอร์ภาพ]");
        std::process::exit(2);
    }
    let (raw_dir, jpeg_dir, models) = (PathBuf::from(&args[0]), PathBuf::from(&args[1]), PathBuf::from(&args[2]));
    let out_dir = args.get(3).map(PathBuf::from);
    if let Some(d) = &out_dir {
        std::fs::create_dir_all(d).expect("สร้างโฟลเดอร์ภาพ");
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let mut engine =
        puri_faces::FaceEngine::load(&models, "det_10g.onnx", "w600k_r50.onnx", threads).expect("โหลดโมเดลใบหน้า");
    let mut jpegs: Vec<PathBuf> = std::fs::read_dir(&jpeg_dir)
        .expect("อ่านโฟลเดอร์ JPEG")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg")))
        .collect();
    jpegs.sort();

    let iou = |a: &Rect, b: &Rect| {
        let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
        let (x1, y1) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
        let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
        inter / (a.w * a.h + b.w * b.h - inter)
    };
    let (mut tilt_rows, mut false_tilts, mut level) = (Vec::new(), 0, 0);
    let (mut crop_rows, mut sum_full, mut sum_best, mut sum_any) = (Vec::new(), 0.0, 0.0, 0.0);
    for jpeg in &jpegs {
        let bytes = std::fs::read(jpeg).expect("อ่าน JPEG");
        let Some(xmp) = pair::jpeg_xmp(&bytes) else { continue };
        let Some(lr) = Lightroom::from_xmp(&xmp) else { continue };
        let Some(raw_name) = pair::raw_file_name(&xmp) else { continue };
        let Ok(raw) = std::fs::read(raw_dir.join(&raw_name)) else { continue };
        let (img, info) = puri_core::raw::base_image(&raw, 1500).expect("ล้าง RAW");
        if info.exif.orientation != 1 {
            continue; // กรอบของ Lightroom อยู่บนแนวที่เก็บในไฟล์ — วัดเฉพาะรูปที่ไม่ต้องหมุน
        }
        let name = jpeg.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let tilt = compose::detect_tilt(&img);
        if std::env::var_os("CUES").is_some() {
            let c = compose::tilt_cues(&img).unwrap_or_default();
            let r = |v: Option<(f64, f64)>| v.map(|(a, b)| format!("{a:+.1}°/{b:.2}")).unwrap_or_else(|| "-".into());
            println!(
                "  cues {}: LR {:+.2} · ขอบ {} · แนวตั้ง {} · เส้นยาว {} → {:?}",
                jpeg.file_stem().unwrap_or_default().to_string_lossy(),
                lr.frame.angle,
                r(c.edges),
                r(c.vertical),
                r(c.line),
                tilt.map(|t| t.degrees)
            );
        }
        let found = tilt.map_or(0.0, |t| t.degrees);
        if lr.frame.angle.abs() > 0.01 {
            tilt_rows.push(format!(
                "{name}: Lightroom {:+.2}° · หาได้ {:?}",
                lr.frame.angle,
                tilt.map(|t| (t.degrees, (t.confidence * 100.0).round()))
            ));
        } else {
            level += 1;
            if found.abs() >= 0.5 {
                false_tilts += 1;
                tilt_rows.push(format!(
                    "{name}: Lightroom 0 · หาได้ {found:+.1}° (มั่นใจ {:.0}%)",
                    tilt.map_or(0.0, |t| t.confidence * 100.0)
                ));
            }
        }
        let f = lr.frame;
        let cropped = f.left > 0.005 || f.top > 0.005 || f.right < 0.995 || f.bottom < 0.995;
        if cropped && f.angle.abs() < 0.01 {
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
            let user = Rect { x: f.left, y: f.top, w: f.right - f.left, h: f.bottom - f.top };
            let suggestions = compose::suggest_crops(&img, &faces, None);
            let full = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
            let best = suggestions.first().map_or(0.0, |s| iou(&s.crop, &user));
            let any = suggestions.iter().map(|s| iou(&s.crop, &user)).fold(0.0, f64::max);
            sum_full += iou(&full, &user);
            sum_best += best;
            sum_any += any;
            crop_rows.push(format!(
                "{name}: ผู้ใช้ {:.2},{:.2} {:.2}×{:.2} · ทั้งภาพ IoU {:.2} · ที่เสนออันแรก {:.2} ({}) · ดีสุดใน {} แบบ {:.2}",
                user.x,
                user.y,
                user.w,
                user.h,
                iou(&full, &user),
                best,
                suggestions.first().map_or("-", |s| s.ratio.as_str()),
                suggestions.len(),
                any
            ));
            if let Some(dir) = &out_dir {
                let mut canvas = img.clone();
                let mut draw = |r: &Rect, color: [u8; 3], thick: u32| {
                    let (x0, y0) = ((r.x * w) as u32, (r.y * h) as u32);
                    let (x1, y1) = (((r.x + r.w) * w) as u32 - 1, ((r.y + r.h) * h) as u32 - 1);
                    for t in 0..thick {
                        for x in x0..=x1 {
                            canvas.put_pixel(x, (y0 + t).min(y1), image::Rgb(color));
                            canvas.put_pixel(x, y1.saturating_sub(t).max(y0), image::Rgb(color));
                        }
                        for y in y0..=y1 {
                            canvas.put_pixel((x0 + t).min(x1), y, image::Rgb(color));
                            canvas.put_pixel(x1.saturating_sub(t).max(x0), y, image::Rgb(color));
                        }
                    }
                };
                draw(&user, [40, 220, 80], 6);
                for (k, s) in suggestions.iter().enumerate() {
                    draw(&s.crop, if k == 0 { [255, 60, 40] } else { [255, 200, 40] }, if k == 0 { 5 } else { 3 });
                }
                let small = image::imageops::thumbnail(&canvas, 900, 900);
                small.save(dir.join(format!("{name}.jpg"))).expect("เขียนภาพ");
            }
        }
    }
    println!("ความเอียง (รูปที่ผู้ใช้ปรับเอียง):");
    for r in tilt_rows.iter().filter(|r| r.contains("Lightroom +") || r.contains("Lightroom -")) {
        println!("  {r}");
    }
    println!("รูปที่ผู้ใช้ไม่ได้ปรับเอียง {level} รูป · แอปจะเสนอเอียง ≥ 0.5° {false_tilts} รูป:");
    for r in tilt_rows.iter().filter(|r| r.contains("Lightroom 0")) {
        println!("  {r}");
    }
    let n = crop_rows.len().max(1) as f64;
    println!(
        "ครอป ({} รูปที่ผู้ใช้ครอปโดยไม่เอียง) — IoU เฉลี่ย: ทั้งภาพ {:.2} · อันแรกที่เสนอ {:.2} · ดีสุดใน 3 แบบ {:.2}",
        crop_rows.len(),
        sum_full / n,
        sum_best / n,
        sum_any / n
    );
    for r in &crop_rows {
        println!("  {r}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {
    eprintln!("Mac ชิป Intel ไม่มี ONNX Runtime สำเร็จรูป");
}
