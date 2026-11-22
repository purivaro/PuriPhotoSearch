//! ลองแยกส่วนของคน (ผม ผิวหน้า ผิวตัว เสื้อผ้า) กับรูปจริงด้วยโมเดลจริง — เครื่องมือของนักพัฒนา อ่านรูปอย่างเดียว เขียนเฉพาะในโฟลเดอร์ผล
//!
//! ```text
//! cargo run --release -p puri-faces --example parts_probe -- <โฟลเดอร์ BiRefNet> <โฟลเดอร์ตัวแยกส่วน> <โฟลเดอร์ผล> <รูป...>
//! ```

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use puri_core::{media, parts, subject};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (seg_dir, parts_dir, out_dir) = (Path::new(&args[0]), Path::new(&args[1]), PathBuf::from(&args[2]));
    std::fs::create_dir_all(&out_dir).expect("สร้างโฟลเดอร์ผล");
    let provider = puri_faces::subject::provider().expect("build นี้มีตัวแยกตัวคน");
    let segmenter = (provider.factory)(seg_dir).expect("โหลดตัวแยกตัวคน");
    let part_seg = (provider.parts.expect("มีตัวแยกส่วน").factory)(parts_dir).expect("โหลดตัวแยกส่วน");
    // สีต่อส่วน: ผม น้ำตาล · ผิวตัว ส้ม · ผิวหน้า ชมพู · เสื้อผ้า ฟ้า
    let colors = [[160.0, 82.0, 45.0], [255.0, 170.0, 90.0], [255.0, 100.0, 200.0], [60.0, 120.0, 255.0]];
    for f in &args[3..] {
        let bytes = std::fs::read(f).expect("อ่านรูป");
        let img = media::decode_upright(&bytes, subject::MAP_EDGE).expect("ถอดรหัส");
        let img = img.thumbnail(subject::MAP_EDGE, subject::MAP_EDGE).to_rgb8();
        let map = subject::SubjectMap::new(segmenter.segment(&img).expect("หาตัวคน"));
        let t = Instant::now();
        let p = parts::compute(&img, &map, part_seg.as_ref()).expect("แยกส่วน");
        let secs = t.elapsed().as_secs_f64();
        let mut over = img.clone();
        for (x, y, px) in over.enumerate_pixels_mut() {
            let q = p.image().get_pixel(x, y);
            for (c, color) in colors.iter().enumerate() {
                let a = q[c] as f64 / 255.0 * 0.6;
                for k in 0..3 {
                    px[k] = (px[k] as f64 * (1.0 - a) + color[k] * a).round() as u8;
                }
            }
        }
        let name = Path::new(f).file_stem().unwrap_or_default().to_string_lossy().into_owned();
        over.save(out_dir.join(format!("{name}-parts.jpg"))).expect("เขียนผล");
        println!("{name}: คน {} · แยกส่วน {secs:.2} วินาที", map.people().len());
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {}
