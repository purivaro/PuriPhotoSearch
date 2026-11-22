//! ลองหาท้องฟ้ากับรูปจริงด้วยโมเดลจริง — เครื่องมือของนักพัฒนา อ่านรูปอย่างเดียว เขียนเฉพาะในโฟลเดอร์ผล
//!
//! ```text
//! cargo run --release -p puri-faces --example sky_probe -- <โฟลเดอร์ BiRefNet> <โฟลเดอร์ตัวแยกท้องฟ้า> <โฟลเดอร์ผล> <รูป...>
//! ```

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use puri_core::{media, sky, subject};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (seg_dir, sky_dir, out_dir) = (Path::new(&args[0]), Path::new(&args[1]), PathBuf::from(&args[2]));
    std::fs::create_dir_all(&out_dir).expect("สร้างโฟลเดอร์ผล");
    let provider = puri_faces::subject::provider().expect("build นี้มีตัวแยกตัวคน");
    let segmenter = (provider.factory)(seg_dir).expect("โหลดตัวแยกตัวคน");
    let sky_seg = (provider.sky.expect("มีตัวแยกท้องฟ้า").factory)(sky_dir).expect("โหลดตัวแยกท้องฟ้า");
    for f in &args[3..] {
        let bytes = std::fs::read(f).expect("อ่านรูป");
        let img = media::decode_upright(&bytes, sky::SKY_EDGE).expect("ถอดรหัส");
        let img = img.thumbnail(sky::SKY_EDGE, sky::SKY_EDGE).to_rgb8();
        let small = image::imageops::thumbnail(&img, subject::MAP_EDGE, subject::MAP_EDGE);
        let t = Instant::now();
        let people = subject::SubjectMap::new(segmenter.segment(&small).expect("หาตัวคน"));
        let people_secs = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let map = sky::compute(&img, &people, sky_seg.as_ref()).expect("หาท้องฟ้า");
        let secs = t.elapsed().as_secs_f64();
        let mut over = img.clone();
        for (x, y, px) in over.enumerate_pixels_mut() {
            let a = map.image().get_pixel(x, y)[0] as f64 / 255.0 * 0.55;
            for (k, c) in [255.0, 0.0, 0.0].iter().enumerate() {
                px[k] = (px[k] as f64 * (1.0 - a) + c * a).round() as u8;
            }
        }
        let name = Path::new(f).file_stem().unwrap_or_default().to_string_lossy().into_owned();
        over.save(out_dir.join(format!("{name}-sky.jpg"))).expect("เขียนผล");
        println!(
            "{name}: ท้องฟ้า {} · หาตัวคน {people_secs:.2} วินาที · หาท้องฟ้า {secs:.2} วินาที",
            if map.bounds().is_some() { "พบ" } else { "ไม่พบ" }
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {}
