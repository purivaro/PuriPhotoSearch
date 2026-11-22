//! ลองลบคน/สิ่งของกับรูปจริงด้วยโมเดลจริง (ตัวแยกตัวคน + ตัวเติมฉากหลัง) — เครื่องมือของนักพัฒนา อ่านรูปอย่างเดียว เขียนเฉพาะในโฟลเดอร์ผล
//!
//! ```text
//! cargo run --release -p puri-faces --example remove_probe -- <โฟลเดอร์ BiRefNet> <โฟลเดอร์ LaMa> <โฟลเดอร์ผล> <รูป> <x,y,w,h> [x,y,w,h ...]
//! ```

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use image::{imageops, DynamicImage};
    use puri_core::{body, media, remove};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (seg_dir, fill_dir, out_dir, photo) =
        (Path::new(&args[0]), Path::new(&args[1]), PathBuf::from(&args[2]), &args[3]);
    let rects: Vec<[f64; 4]> = args[4..]
        .iter()
        .map(|r| {
            let v: Vec<f64> = r.split(',').map(|x| x.trim().parse().expect("ตัวเลข")).collect();
            [v[0], v[1], v[2], v[3]]
        })
        .collect();
    std::fs::create_dir_all(&out_dir).expect("สร้างโฟลเดอร์ผล");
    let provider = puri_faces::subject::provider().expect("build นี้มีตัวแยกตัวคน");
    let segmenter = (provider.factory)(seg_dir).expect("โหลดตัวแยกตัวคน");
    let inpainter = (provider.fill.expect("มีตัวเติมฉากหลัง").factory)(fill_dir).expect("โหลดตัวเติมฉากหลัง");
    let bytes = std::fs::read(photo).expect("อ่านรูป");
    let base = media::decode_upright(&bytes, remove::REMOVE_EDGE).expect("ถอดรหัส");
    let base = base.thumbnail(remove::REMOVE_EDGE, remove::REMOVE_EDGE).to_rgb8();
    let mut removals = Vec::new();
    for (i, rect) in rects.iter().enumerate() {
        let t = Instant::now();
        let anchor = std::env::var("ANCHOR").ok().map(|a| {
            let v: Vec<f64> = a.split(',').map(|x| x.trim().parse().expect("ตัวเลข")).collect();
            [v[0], v[1]]
        });
        let (patch, found) = remove::compute(&base, *rect, anchor, segmenter.as_ref(), inpainter.as_ref()).expect("ลบ");
        println!("กรอบ {i}: {:.1} วิ · พบสิ่งที่เด่น {found}", t.elapsed().as_secs_f64());
        removals.push(remove::Removal { id: i.to_string(), rect: *rect, anchor, patch: remove::Attached(Some(patch)) });
    }
    let edge: u32 = std::env::var("EDGE").ok().and_then(|e| e.parse().ok()).unwrap_or(2048);
    let big = media::decode_upright(&bytes, edge).expect("ถอดรหัส");
    let big = if big.width().max(big.height()) > edge { big.thumbnail(edge, edge) } else { big }.to_rgb8();
    let out = body::apply(DynamicImage::ImageRgb8(big.clone()), None, &removals, |u, v| (u, v)).into_rgb8();
    let name = Path::new(photo).file_stem().unwrap_or_default().to_string_lossy().into_owned();
    out.save(out_dir.join(format!("{name}-removed.jpg"))).expect("เขียนผล");
    let (w, h) = big.dimensions();
    let mut both = image::RgbImage::new(w * 2 + 16, h);
    imageops::replace(&mut both, &big, 0, 0);
    imageops::replace(&mut both, &out, (w + 16) as i64, 0);
    let scale = 2400.0 / both.width() as f64;
    imageops::thumbnail(&both, 2400, (both.height() as f64 * scale) as u32)
        .save(out_dir.join(format!("{name}-compare.jpg")))
        .expect("เขียนผล");
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {}
