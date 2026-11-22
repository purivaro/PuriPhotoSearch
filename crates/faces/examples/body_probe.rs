//! ลองตัวผอมกับรูปจริงด้วยโมเดลจริง (ตัวแยกตัวคน + ตัวเติมฉากหลัง) — เครื่องมือของนักพัฒนา อ่านรูปอย่างเดียว เขียนเฉพาะในโฟลเดอร์ผล
//!
//! ```text
//! cargo run --release -p puri-faces --example body_probe -- <โฟลเดอร์ BiRefNet> <โฟลเดอร์ LaMa> <โฟลเดอร์ผล> <ระดับ 0–100> <รูป...>
//! ```

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use image::{imageops, DynamicImage};
    use puri_core::{body, media, subject};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (seg_dir, fill_dir, out_dir) = (Path::new(&args[0]), Path::new(&args[1]), PathBuf::from(&args[2]));
    let amount: f64 = args[3].parse().expect("ระดับ");
    std::fs::create_dir_all(&out_dir).expect("สร้างโฟลเดอร์ผล");
    let provider = puri_faces::subject::provider().expect("build นี้มีตัวแยกตัวคน");
    let segmenter = (provider.factory)(seg_dir).expect("โหลดตัวแยกตัวคน");
    let fill = provider.fill.expect("มีตัวเติมฉากหลัง");
    let inpainter = (fill.factory)(fill_dir).expect("โหลดตัวเติมฉากหลัง");
    for f in &args[4..] {
        let bytes = std::fs::read(f).expect("อ่านรูป");
        let small = media::decode_upright(&bytes, subject::MAP_EDGE).expect("ถอดรหัส");
        let small = small.thumbnail(subject::MAP_EDGE, subject::MAP_EDGE).to_rgb8();
        let t = Instant::now();
        let map = segmenter.segment(&small).expect("หาตัวคน");
        let t_map = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let filled = body::fill(&small, &map, inpainter.as_ref()).expect("เติมฉากหลัง");
        let t_fill = t.elapsed().as_secs_f64();
        let map = subject::SubjectMap::new(map);
        let people = map.people().len();
        // FACES="x,y,w,h;x,y,w,h" (สัดส่วน) = กรอบใบหน้า ใช้หาคอและแยกคนที่ยืนชิดกัน
        let faces: Vec<[f64; 4]> = std::env::var("FACES")
            .unwrap_or_default()
            .split(';')
            .filter_map(|f| {
                let v: Vec<f64> = f.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
            })
            .collect();
        let plan = body::BodyPlan::new(map, filled.clone(), &faces);
        // ขนาดของภาพผล (ด้านยาว) — EDGE=0 = เต็มขนาด
        let edge: u32 = std::env::var("EDGE").ok().and_then(|e| e.parse().ok()).unwrap_or(2048);
        let edge = if edge == 0 { u32::MAX } else { edge };
        let big = media::decode_upright(&bytes, edge).expect("ถอดรหัส");
        let big = if big.width().max(big.height()) > edge { big.thumbnail(edge, edge) } else { big }.to_rgb8();
        let t = Instant::now();
        let out = body::apply(DynamicImage::ImageRgb8(big.clone()), amount, &plan, |u, v| (u, v)).into_rgb8();
        let t_apply = t.elapsed().as_secs_f64();
        let name = Path::new(f).file_stem().unwrap_or_default().to_string_lossy().into_owned();
        out.save(out_dir.join(format!("{name}-body{amount}.jpg"))).expect("เขียนผล");
        filled.image().save(out_dir.join(format!("{name}-fill.png"))).expect("เขียนผล");
        let (w, h) = big.dimensions();
        let mut both = image::RgbImage::new(w * 2 + 16, h);
        imageops::replace(&mut both, &big, 0, 0);
        imageops::replace(&mut both, &out, (w + 16) as i64, 0);
        let scale = 2400.0 / both.width() as f64;
        imageops::thumbnail(&both, 2400, (both.height() as f64 * scale) as u32)
            .save(out_dir.join(format!("{name}-compare.jpg")))
            .expect("เขียนผล");
        println!(
            "{name}: คน {people} · หาตัวคน {t_map:.1} วิ · เติมฉาก {t_fill:.1} วิ · หดตัว ({}×{}) {t_apply:.2} วิ",
            big.width(),
            big.height()
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {}
