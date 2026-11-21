//! หาค่าสีพื้นฐานของ RAW ที่ทำให้ภาพของแอปใกล้ JPEG ที่ส่งออกจาก Lightroom (DECISIONS ข้อ 35)
//!
//! ```text
//! cargo run --release -p puri-core --example lightroom_fit -- <โฟลเดอร์ RAW> <โฟลเดอร์ JPEG จาก Lightroom>
//! ```
//!
//! รูปที่ Lightroom ส่งออกพร้อมค่าที่แต่ง (XMP) — ภาพของเราใช้ค่าเดียวกับใน XMP แล้วหา "หน้าตาพื้นฐาน" ที่ใส่บนภาพเชิงเส้นก่อนการแต่ง:
//! หมุนสีและปรับความอิ่มตามช่วงสี (Oklab ทุก 30°) + คอนทราสต์ของโทน · วัดหลังหักค่าเฉลี่ยของแต่ละรูปออก เพราะแอปปรับค่าแสง/แสงขาว
//! ของรูปคู่ให้เท่ากับ JPEG อยู่แล้ว (ปรับจริงทุกครั้งที่ลองค่าช้าเกินไป) — ผลจริงทั้งระบบวัดต่อด้วย `lightroom_compare`
//! · ใช้รูปลำดับคู่หาค่า รูปลำดับคี่ตรวจ (ค่าที่ได้ต้องดีขึ้นกับรูปที่ไม่ได้ใช้หาด้วย) · อ่านไฟล์อย่างเดียว

use std::path::{Path, PathBuf};

use image::{imageops, RgbImage};
use puri_core::pair::{self, Lightroom, RawBase};
use puri_core::raw::Linear;
use rayon::prelude::*;

const ANCHORS: usize = 12;
const EDGE: u32 = 320;

#[derive(Clone)]
struct Params {
    hue: [f64; ANCHORS],
    sat: [f64; ANCHORS],
    contrast: f64,
}

impl Params {
    fn identity() -> Params {
        Params { hue: [0.0; ANCHORS], sat: [1.0; ANCHORS], contrast: 0.0 }
    }

    /// ค่าตามมุมสี (องศา) แบบเส้นตรงเป็นช่วงรอบวง
    fn at(&self, h: f64) -> (f64, f64) {
        let step = 360.0 / ANCHORS as f64;
        let x = h.rem_euclid(360.0) / step;
        let i = x.floor() as usize % ANCHORS;
        let j = (i + 1) % ANCHORS;
        let t = x - x.floor();
        (self.hue[i] * (1.0 - t) + self.hue[j] * t, self.sat[i] * (1.0 - t) + self.sat[j] * t)
    }
}

fn to_oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn from_oklab([ll, a, b]: [f64; 3]) -> [f64; 3] {
    let l = (ll + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (ll - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (ll - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
}

/// หน้าตาพื้นฐานบนภาพเชิงเส้น: หมุนสีและปรับความอิ่มตามมุมสี (ความสว่างเท่าเดิม)
fn look(img: &Linear, p: &Params) -> Linear {
    let mut out = img.clone();
    for px in out.pixels_mut() {
        let [l, a, b] = to_oklab(px.0.map(f64::from));
        let c = (a * a + b * b).sqrt();
        if c < 1e-6 {
            continue;
        }
        let (dh, s) = p.at(b.atan2(a).to_degrees());
        let h = b.atan2(a) + dh.to_radians();
        let rgb = from_oklab([l, c * s * h.cos(), c * s * h.sin()]);
        px.0 = rgb.map(|v| v.clamp(0.0, f64::from(puri_core::raw::HEADROOM)) as f32);
    }
    out
}

/// คอนทราสต์ของโทนบนภาพที่แสดง (สูตรเดียวกับแถบคอนทราสต์ ค่าบวก)
fn tone(img: &mut RgbImage, k: f64) {
    if k == 0.0 {
        return;
    }
    for px in img.pixels_mut() {
        for v in px.0.iter_mut() {
            let x = f64::from(*v) / 255.0;
            let y = x + k * (x * x * (3.0 - 2.0 * x) - x);
            *v = (y.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

struct Photo {
    name: String,
    /// ภาพเชิงเส้นในกรอบของ JPEG แบบไม่มีหน้าตาพื้นฐาน
    linear: Linear,
    /// ค่าของ Lightroom ก่อนปรับค่าแสง/แสงขาวให้เท่ากับ JPEG
    adjust: puri_core::adjust::Adjust,
    /// L* a* b* ของ JPEG บนภาพย่อครึ่งหนึ่ง (ใช้วัด)
    target: Vec<[f64; 3]>,
}

fn srgb_lab(lut: &[f64; 256], p: [u8; 3]) -> [f64; 3] {
    let (r, g, b) = (lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]);
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn lut() -> [f64; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let c = i as f64 / 255.0;
        *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    }
    t
}

/// ΔE76 เฉลี่ยบนภาพย่อครึ่งหนึ่ง (ลดผลของรายละเอียด/ความคมที่เราทำตามไม่ได้) หลังหักค่าเฉลี่ย L* a* b* ของแต่ละภาพออก
fn error(photo: &Photo, p: &Params, lut: &[f64; 256]) -> f64 {
    let lin = look(&photo.linear, p);
    // แสงสีอย่างเดียว (ไม่คิด Texture/Clarity — มีผลกับรายละเอียด ไม่ใช่สี และช้ามาก)
    let mut out = puri_core::adjust::apply_linear(&lin, &photo.adjust);
    tone(&mut out, p.contrast);
    let (w, h) = (out.width() / 2, out.height() / 2);
    let small = imageops::resize(&out, w, h, imageops::FilterType::Triangle);
    let ours: Vec<[f64; 3]> = small.pixels().map(|o| srgb_lab(lut, o.0)).collect();
    let n = ours.len() as f64;
    let mean = |v: &[[f64; 3]]| {
        let mut s = [0.0; 3];
        for p in v {
            for c in 0..3 {
                s[c] += p[c];
            }
        }
        s.map(|x| x / n)
    };
    let (mo, mt) = (mean(&ours), mean(&photo.target));
    ours.iter()
        .zip(&photo.target)
        .map(|(o, t)| {
            let d = [(o[0] - mo[0]) - (t[0] - mt[0]), (o[1] - mo[1]) - (t[1] - mt[1]), (o[2] - mo[2]) - (t[2] - mt[2])];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .sum::<f64>()
        / n
}

fn mean_error(photos: &[&Photo], p: &Params, lut: &[f64; 256]) -> f64 {
    photos.par_iter().map(|ph| error(ph, p, lut)).sum::<f64>() / photos.len() as f64
}

fn load(raw_dir: &Path, jpeg: &Path) -> Result<Photo, String> {
    let bytes = std::fs::read(jpeg).map_err(|e| e.to_string())?;
    let xmp = pair::jpeg_xmp(&bytes).ok_or("ไม่มี XMP")?;
    let lr = Lightroom::from_xmp(&xmp).ok_or("ไม่มีค่าของ Lightroom")?;
    let raw =
        std::fs::read(raw_dir.join(pair::raw_file_name(&xmp).ok_or("ไม่มีชื่อไฟล์ RAW")?)).map_err(|e| e.to_string())?;
    let reference = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgb8();
    let size = reference.dimensions();
    // ค่าของ Lightroom ล้วน (แสงขาวแปลงเทียบกล้องแล้ว) — ส่วนต่างของค่าเฉลี่ยทั้งภาพหักออกตอนวัด (ดู `error`)
    let same = pair::start_settings(&raw, Some(&lr), size, None)?;
    let base = RawBase {
        raw_path: PathBuf::new(),
        frame: lr.frame,
        jpeg: size,
        start: same.clone(),
        lens: lr.lens,
        unsupported: vec![],
    };
    let linear = pair::framed_linear(&raw, &base, &same, EDGE).map_err(|e| e.message)?;
    let lut = lut();
    let (w, h) = (linear.width() / 2, linear.height() / 2);
    let target = imageops::resize(&reference, w, h, imageops::FilterType::Triangle)
        .pixels()
        .map(|p| srgb_lab(&lut, p.0))
        .collect();
    Ok(Photo {
        name: jpeg.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
        linear,
        adjust: same.adjust,
        target,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (raw_dir, jpeg_dir) = (Path::new(&args[0]), Path::new(&args[1]));
    let mut jpegs: Vec<PathBuf> = std::fs::read_dir(jpeg_dir)
        .expect("อ่านโฟลเดอร์ไม่ได้")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jpg")))
        .collect();
    jpegs.sort();
    // หาค่าจากภาพที่ยังไม่มีหน้าตาพื้นฐาน
    puri_core::raw::BASE_LOOK.store(false, std::sync::atomic::Ordering::Relaxed);
    let photos: Vec<Photo> =
        jpegs.par_iter().filter_map(|j| load(raw_dir, j).map_err(|e| eprintln!("{}: {e}", j.display())).ok()).collect();
    let train: Vec<&Photo> = photos.iter().step_by(2).collect();
    let test: Vec<&Photo> = photos.iter().skip(1).step_by(2).collect();
    let lut = lut();
    println!("รูป {} (หาค่า {} · ตรวจ {})", photos.len(), train.len(), test.len());

    let mut p = Params::identity();
    let regular = |p: &Params| {
        // กันค่าที่เกินจริงจากรูปชุดเดียว: หมุนสีเกิน ±15° หรือความอิ่มเกิน ±30% ถูกลงโทษ
        p.hue.iter().map(|h| (h / 15.0).powi(2)).sum::<f64>() * 0.02
            + p.sat.iter().map(|s| ((s - 1.0) / 0.3).powi(2)).sum::<f64>() * 0.02
    };
    let objective = |p: &Params| mean_error(&train, p, &lut) + regular(p);
    let mut best = objective(&p);
    println!("ก่อนหา: ชุดหาค่า ΔE {:.3} · ชุดตรวจ ΔE {:.3}", mean_error(&train, &p, &lut), mean_error(&test, &p, &lut));
    let mut steps = (4.0, 0.08, 0.06);
    for round in 0..4 {
        let mut improved = true;
        while improved {
            improved = false;
            for k in 0..(ANCHORS * 2 + 1) {
                for dir in [-1.0, 1.0] {
                    let mut q = p.clone();
                    match k {
                        k if k < ANCHORS => q.hue[k] += dir * steps.0,
                        k if k < ANCHORS * 2 => q.sat[k - ANCHORS] = (q.sat[k - ANCHORS] + dir * steps.1).max(0.3),
                        _ => q.contrast = (q.contrast + dir * steps.2).clamp(-0.5, 0.8),
                    }
                    let e = objective(&q);
                    if e < best - 1e-5 {
                        (p, best, improved) = (q, e, true);
                    }
                }
            }
        }
        println!(
            "รอบ {round}: ชุดหาค่า ΔE {:.3} · ชุดตรวจ ΔE {:.3}",
            mean_error(&train, &p, &lut),
            mean_error(&test, &p, &lut)
        );
        steps = (steps.0 / 2.0, steps.1 / 2.0, steps.2 / 2.0);
    }
    println!("\nมุมสี (Oklab องศา) | หมุนสี | ความอิ่ม");
    for i in 0..ANCHORS {
        println!("{:>3}° | {:+.1}° | ×{:.3}", i * 360 / ANCHORS, p.hue[i], p.sat[i]);
    }
    println!("คอนทราสต์ของโทน {:+.3}", p.contrast);
    let per: Vec<(String, f64, f64)> = photos
        .par_iter()
        .map(|ph| (ph.name.clone(), error(ph, &Params::identity(), &lut), error(ph, &p, &lut)))
        .collect();
    let worse = per.iter().filter(|(_, a, b)| b > a).count();
    println!("รูปที่แย่ลง {worse} จาก {}", per.len());
    println!("HUE = {:?}\nSAT = {:?}\nCONTRAST = {}", p.hue, p.sat, p.contrast);
}
