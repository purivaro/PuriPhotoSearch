//! สร้างโฟลเดอร์รูปทดสอบสำหรับ e2e และการลองแอปด้วยมือ (ไม่ใช่ข้อมูลของแอป: เป็นไฟล์รูปจริงบนดิสก์ที่แอปนำเข้าเหมือนของผู้ใช้)
//!
//!   cargo run -p puri-core --example make_fixtures -- <โฟลเดอร์ปลายทาง>
//!
//! ได้โฟลเดอร์ `2569/` ที่มีสามงานลงวันที่ รูปวางตรงโฟลเดอร์แม่ รูปแนวตั้งที่หมุนด้วย EXIF ไฟล์เสีย ไฟล์ชนิดที่ยังไม่รองรับ และไฟล์ RAW

use image::{Rgb, RgbImage};
use std::path::Path;

/// ภาพไล่สีพร้อม "ใบหน้า" วงกลมตามตำแหน่งที่กำหนด — ให้มีเป้าสำหรับลากกรอบใบหน้าในเทสต์
fn picture(w: u32, h: u32, hue: u8, faces: &[(f32, f32)]) -> RgbImage {
    let mut img = RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        Rgb([(hue as f32 * (1.0 - fy * 0.5)) as u8, (120.0 + 100.0 * fx) as u8, (200.0 - 120.0 * fy) as u8])
    });
    let r = (w.min(h) as f32 * 0.11) as i32;
    for (cx, cy) in faces {
        let (cx, cy) = ((cx * w as f32) as i32, (cy * h as f32) as i32);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    let (x, y) = (cx + dx, cy + dy);
                    if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                        let eye = dy < -r / 6 && dy > -r / 2 && (dx.abs() - r / 3).abs() < r / 8;
                        img.put_pixel(x as u32, y as u32, if eye { Rgb([40, 30, 30]) } else { Rgb([236, 196, 160]) });
                    }
                }
            }
        }
    }
    img
}

fn jpeg(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88).encode_image(img).expect("encode");
    out
}

/// แทรก EXIF (Orientation + DateTimeOriginal) หลัง SOI
fn with_exif(jpeg: Vec<u8>, orientation: u16, datetime: &str) -> Vec<u8> {
    assert_eq!(datetime.len(), 19);
    let entry = |tag: u16, typ: u16, count: u32, value: u32| {
        [&tag.to_le_bytes()[..], &typ.to_le_bytes(), &count.to_le_bytes(), &value.to_le_bytes()].concat()
    };
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend(entry(0x0112, 3, 1, orientation as u32));
    tiff.extend(entry(0x8769, 4, 1, 38));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend(entry(0x9003, 2, 20, 56));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(datetime.as_bytes());
    tiff.push(0);
    let payload = [&b"Exif\0\0"[..], &tiff].concat();
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend(payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, bytes).expect("write");
}

fn main() {
    let dest = std::env::args().nth(1).expect("ระบุโฟลเดอร์ปลายทาง");
    let root = Path::new(&dest).join("2569");
    let shot = |hue: u8, faces: &[(f32, f32)], time: &str| with_exif(jpeg(&picture(1200, 800, hue, faces)), 1, time);

    let ai = root.join("691118 งานอบรม AI");
    write(&ai.join("กล้อง A/IMG_0248.jpg"), &shot(210, &[(0.3, 0.4), (0.62, 0.42)], "2026:11:18 09:10:00"));
    write(&ai.join("กล้อง A/IMG_0249.jpg"), &shot(180, &[(0.5, 0.4)], "2026:11:18 09:12:00"));
    write(&ai.join("กล้อง A/IMG_0250.jpg"), &shot(150, &[(0.35, 0.45), (0.7, 0.4)], "2026:11:18 09:20:00"));
    write(&ai.join("กล้อง B/DSC_0101.jpg"), &shot(120, &[(0.5, 0.5)], "2026:11:18 10:05:00"));
    // เซนเซอร์แนวนอน + EXIF orientation 6 → ผู้ใช้เห็นเป็นแนวตั้ง
    write(
        &ai.join("กล้อง B/DSC_0102_แนวตั้ง.jpg"),
        &with_exif(jpeg(&picture(1200, 800, 90, &[(0.5, 0.5)])), 6, "2026:11:18 10:06:00"),
    );
    write(&ai.join("กล้อง B/เสีย.jpg"), b"\xFF\xD8\xFF not a real jpeg");
    write(&ai.join("กล้อง B/DSC_0103.heic"), b"heic placeholder");
    // RAW (เปิดสวิตช์ "รวมไฟล์ RAW" จึงนำเข้า — e2e ไม่เปิด ไฟล์นี้จึงแค่ถูกนับ)
    write(&ai.join("กล้อง B/DSC_0104.arw"), b"raw placeholder");

    let welcome = root.join("691116 ต้อนรับคณะญาติโยม");
    write(&welcome.join("IMG_0101.jpg"), &shot(240, &[(0.4, 0.4), (0.6, 0.45)], "2026:11:16 08:00:00"));
    write(&welcome.join("IMG_0102.jpg"), &shot(200, &[(0.5, 0.4)], "2026:11:16 08:30:00"));
    write(&welcome.join("IMG_0248.jpg"), &shot(170, &[(0.3, 0.5)], "2026:11:16 09:00:00")); // ชื่อชนกับงานอบรม

    let lights = root.join("691114 กิจกรรมจุดประทีป");
    for i in 0..6u8 {
        write(
            &lights.join(format!("IMG_03{i:02}.jpg")),
            &shot(60 + i * 25, &[(0.25 + 0.1 * i as f32, 0.45)], &format!("2026:11:14 19:{:02}:00", i * 5)),
        );
    }

    let png = picture(600, 400, 30, &[]);
    std::fs::create_dir_all(&root).expect("mkdir");
    png.save_with_format(root.join("ป้ายงาน.png"), image::ImageFormat::Png).expect("png");
    println!("{}", root.display());
}
