//! เครื่องมือวัด: ภาพที่แอปล้างจาก RAW เทียบกับ JPEG ที่ส่งออกจาก Lightroom (ภาพเดียวกัน กรอบเดียวกัน)
//!
//! ```text
//! cargo run --release -p puri-core --example lightroom_compare -- \
//!     <โฟลเดอร์ RAW> <โฟลเดอร์ JPEG จาก Lightroom> <preset.xmp> <โฟลเดอร์ผล> [ด้านยาว=1200]
//! ```
//!
//! อ่านไฟล์ของผู้ใช้อย่างเดียว เขียนเฉพาะในโฟลเดอร์ผล · ต่อรูปวัดสามแบบ (กรอบตาม Lightroom ทุกแบบ จึงเทียบทีละพิกเซลได้):
//! - `preset` — ใช้ preset อย่างเดียว (สิ่งที่ผู้ใช้ทำในแอปได้: นำเข้า .xmp แล้วใช้กับรูป RAW) สมดุลแสงขาวตามกล้อง
//! - `same`   — ค่าเดียวกับใน JPEG ทุกค่า (preset + ค่าแสง/แสงขาวที่ปรับเฉพาะรูป) ไม่ปรับให้เข้ากับ JPEG
//! - `fitted` — ค่าเริ่มต้นของรูปคู่ RAW+JPEG ในแอป (ค่าเดียวกัน แล้วปรับค่าแสงและแสงขาวให้เท่ากับ JPEG — DECISIONS ข้อ 26)
//!
//! ตัวเลข: ผลต่างค่าเฉลี่ย L* a* b* · ความอิ่มสี (C*) · คอนทราสต์ (ส่วนเบี่ยงเบนของ L*) · ΔE76 รายพิกเซล (ค่าเฉลี่ยและ p90)

use std::path::{Path, PathBuf};

use image::{imageops, RgbImage};
use puri_core::edit::EditSettings;
use puri_core::pair::{self, Lightroom, RawBase};
use rayon::prelude::*;

const VARIANTS: [&str; 3] = ["preset", "same", "fitted"];

#[derive(Default, Clone, Copy)]
struct Stats {
    l: f64,
    a: f64,
    b: f64,
    chroma: f64,
    l_std: f64,
}

#[derive(Default, Clone, Copy)]
struct Diff {
    dl: f64,
    da: f64,
    db: f64,
    dc: f64,
    dstd: f64,
    de_mean: f64,
    de_p90: f64,
    /// ΔE เฉลี่ยบนภาพย่อ 1/8 — ไม่ไวต่อความคม รายละเอียด และการเลื่อนจากการแก้เลนส์ เหลือแต่สีและโทน
    de_small: f64,
}

fn srgb_to_linear() -> [f64; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let c = i as f64 / 255.0;
        *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    }
    t
}

fn lab(lut: &[f64; 256], p: [u8; 3]) -> [f64; 3] {
    let (r, g, b) = (lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]);
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn stats(labs: &[[f64; 3]]) -> Stats {
    let n = labs.len() as f64;
    let mut s = Stats::default();
    for p in labs {
        s.l += p[0];
        s.a += p[1];
        s.b += p[2];
        s.chroma += (p[1] * p[1] + p[2] * p[2]).sqrt();
    }
    (s.l, s.a, s.b, s.chroma) = (s.l / n, s.a / n, s.b / n, s.chroma / n);
    s.l_std = (labs.iter().map(|p| (p[0] - s.l).powi(2)).sum::<f64>() / n).sqrt();
    s
}

fn mean_de(lut: &[f64; 256], ours: &RgbImage, reference: &RgbImage) -> f64 {
    let n = (ours.width() * ours.height()) as f64;
    ours.pixels()
        .zip(reference.pixels())
        .map(|(x, y)| {
            let (x, y) = (lab(lut, x.0), lab(lut, y.0));
            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
        })
        .sum::<f64>()
        / n
}

fn compare(lut: &[f64; 256], ours: &RgbImage, reference: &RgbImage) -> Diff {
    let to_lab = |img: &RgbImage| img.pixels().map(|p| lab(lut, p.0)).collect::<Vec<_>>();
    let (o, r) = (to_lab(ours), to_lab(reference));
    let (sw, sh) = ((ours.width() / 8).max(1), (ours.height() / 8).max(1));
    let small = |img: &RgbImage| imageops::resize(img, sw, sh, imageops::FilterType::Triangle);
    let de_small = mean_de(lut, &small(ours), &small(reference));
    let (so, sr) = (stats(&o), stats(&r));
    let mut de: Vec<f64> = o
        .iter()
        .zip(&r)
        .map(|(x, y)| ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt())
        .collect();
    de.sort_by(|a, b| a.total_cmp(b));
    Diff {
        dl: so.l - sr.l,
        da: so.a - sr.a,
        db: so.b - sr.b,
        dc: so.chroma - sr.chroma,
        dstd: so.l_std - sr.l_std,
        de_mean: de.iter().sum::<f64>() / de.len() as f64,
        de_p90: de[de.len() * 9 / 10],
        de_small,
    }
}

/// ครอปขนาดจริง (ของภาพที่ล้าง) รอบจุด (fx, fy) ของภาพ — ไว้ดูผิว ความคม และรายละเอียด
fn crop_at(img: &RgbImage, fx: f64, fy: f64, size: u32) -> RgbImage {
    let size = size.min(img.width()).min(img.height());
    let x = ((img.width() as f64 * fx) as u32).saturating_sub(size / 2).min(img.width() - size);
    let y = ((img.height() as f64 * fy) as u32).saturating_sub(size / 2).min(img.height() - size);
    imageops::crop_imm(img, x, y, size, size).to_image()
}

/// แผงเทียบ: Lightroom | preset | same | fitted สูงเท่ากัน
fn strip(panels: &[&RgbImage], height: u32) -> RgbImage {
    let scaled: Vec<RgbImage> = panels
        .iter()
        .map(|p| {
            let w = (p.width() as f64 * height as f64 / p.height() as f64).round() as u32;
            imageops::resize(*p, w.max(1), height, imageops::FilterType::Triangle)
        })
        .collect();
    let gap = 8;
    let width = scaled.iter().map(|p| p.width()).sum::<u32>() + gap * (scaled.len() as u32 - 1);
    let mut out = RgbImage::from_pixel(width, height, image::Rgb([24, 24, 27]));
    let mut x = 0;
    for p in &scaled {
        imageops::replace(&mut out, p, x as i64, 0);
        x += p.width() + gap;
    }
    out
}

struct Row {
    name: String,
    unsupported: Vec<String>,
    diffs: Vec<(String, Diff)>,
    settings: Vec<(String, EditSettings)>,
}

fn run(raw: &Path, jpeg: &Path, preset: &EditSettings, out: &Path, max_edge: u32) -> Result<Row, String> {
    let name = jpeg.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let jpeg_bytes = std::fs::read(jpeg).map_err(|e| e.to_string())?;
    let raw_bytes = std::fs::read(raw).map_err(|e| e.to_string())?;
    let xmp = pair::jpeg_xmp(&jpeg_bytes).ok_or("JPEG ไม่มี XMP ของ Lightroom")?;
    let lr = Lightroom::from_xmp(&xmp).ok_or("XMP ไม่มีค่าของ Lightroom")?;
    let reference = image::load_from_memory(&jpeg_bytes).map_err(|e| e.to_string())?.to_rgb8();
    let size = reference.dimensions();

    let same = pair::start_settings(&raw_bytes, Some(&lr), size, None)?;
    let fitted = pair::start_settings(&raw_bytes, Some(&lr), size, Some(&jpeg_bytes))?;
    let variants = [preset.clone(), same, fitted];

    let lut = srgb_to_linear();
    let mut renders = Vec::new();
    let mut diffs = Vec::new();
    let mut reference_small = None;
    for (label, settings) in VARIANTS.iter().zip(&variants) {
        let base = RawBase {
            raw_path: PathBuf::new(),
            frame: lr.frame,
            jpeg: size,
            start: settings.clone(),
            lens: lr.lens,
            unsupported: vec![],
        };
        let ours = pair::render(&raw_bytes, &base, settings, max_edge).map_err(|e| e.message)?;
        let target = reference_small.get_or_insert_with(|| {
            imageops::resize(&reference, ours.width(), ours.height(), imageops::FilterType::Triangle)
        });
        diffs.push((label.to_string(), compare(&lut, &ours, target)));
        renders.push(ours);
    }
    let target = reference_small.expect("มีภาพอย่างน้อยหนึ่งแบบ");
    let panels: Vec<&RgbImage> = std::iter::once(&target).chain(renders.iter()).collect();
    strip(&panels, 420).save(out.join(format!("{name}.jpg"))).map_err(|e| e.to_string())?;
    let crops: Vec<RgbImage> = panels.iter().map(|p| crop_at(p, 0.5, 0.4, 360)).collect();
    strip(&crops.iter().collect::<Vec<_>>(), 360)
        .save(out.join(format!("{name}-crop.jpg")))
        .map_err(|e| e.to_string())?;
    Ok(Row {
        name,
        unsupported: lr.unsupported,
        diffs,
        settings: VARIANTS.iter().map(|v| v.to_string()).zip(variants).collect(),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("ใช้: lightroom_compare <โฟลเดอร์ RAW> <โฟลเดอร์ JPEG จาก Lightroom> <preset.xmp> <โฟลเดอร์ผล> [ด้านยาว]");
        std::process::exit(2);
    }
    let (raw_dir, jpeg_dir, preset_path, out) =
        (Path::new(&args[0]), Path::new(&args[1]), &args[2], Path::new(&args[3]));
    let max_edge: u32 = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(1200);
    // NO_LOOK=1 วัดแบบไม่มีหน้าตาพื้นฐานของ RAW (DECISIONS ข้อ 35) — ไว้เทียบว่าหน้าตาช่วยไหม
    if std::env::var_os("NO_LOOK").is_some() {
        puri_core::raw::BASE_LOOK.store(false, std::sync::atomic::Ordering::Relaxed);
        println!("ไม่ใส่หน้าตาพื้นฐาน (NO_LOOK)");
    }
    std::fs::create_dir_all(out).expect("สร้างโฟลเดอร์ผลไม่ได้");

    let text = std::fs::read_to_string(preset_path).expect("อ่าน preset ไม่ได้");
    let parsed = puri_core::preset::parse_lightroom(&text).expect("อ่าน preset ไม่ได้");
    println!("preset: ส่วนที่ทำตามไม่ได้ = {:?}", parsed.unsupported);
    let preset = EditSettings { adjust: parsed.adjust, from_raw: true, ..Default::default() };

    let mut jpegs: Vec<PathBuf> = std::fs::read_dir(jpeg_dir)
        .expect("อ่านโฟลเดอร์ JPEG ไม่ได้")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("jpeg")))
        .collect();
    jpegs.sort();
    let rows: Vec<(String, Result<Row, String>)> = jpegs
        .par_iter()
        .map(|jpeg| {
            let label = jpeg.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let raw = std::fs::read(jpeg)
                .ok()
                .and_then(|b| pair::jpeg_xmp(&b))
                .and_then(|x| pair::raw_file_name(&x))
                .map(|n| raw_dir.join(n))
                .filter(|p| p.exists());
            let result = match raw {
                Some(raw) => run(&raw, jpeg, &preset, out, max_edge),
                None => Err("ไม่พบไฟล์ RAW คู่".into()),
            };
            (label, result)
        })
        .collect();

    println!(
        "\nรูป | แบบ | ΔL* | Δa* | Δb* | ΔC* (อิ่มสี) | Δσ(L*) (คอนทราสต์) | ΔE เฉลี่ย | ΔE p90 | ΔE ภาพย่อ 1/8 | ค่าแสงที่ใช้ | อุณหภูมิ/โทนสี"
    );
    let mut sums = vec![Diff::default(); VARIANTS.len()];
    let mut abs_l = vec![0.0; VARIANTS.len()];
    let mut n = 0;
    let mut unsupported = std::collections::BTreeSet::new();
    for (label, row) in &rows {
        match row {
            Err(e) => println!("{label} | ข้าม: {e}"),
            Ok(row) => {
                n += 1;
                unsupported.extend(row.unsupported.iter().cloned());
                for (i, ((variant, d), (_, s))) in row.diffs.iter().zip(&row.settings).enumerate() {
                    println!(
                        "{} | {} | {:+.1} | {:+.1} | {:+.1} | {:+.1} | {:+.1} | {:.1} | {:.1} | {:.1} | {:+.2} | {:+.0}/{:+.0}",
                        row.name,
                        variant,
                        d.dl,
                        d.da,
                        d.db,
                        d.dc,
                        d.dstd,
                        d.de_mean,
                        d.de_p90,
                        d.de_small,
                        s.adjust.exposure,
                        s.adjust.temperature,
                        s.adjust.tint
                    );
                    let t = &mut sums[i];
                    t.de_small += d.de_small;
                    (t.dl, t.da, t.db, t.dc, t.dstd, t.de_mean, t.de_p90) = (
                        t.dl + d.dl,
                        t.da + d.da,
                        t.db + d.db,
                        t.dc + d.dc,
                        t.dstd + d.dstd,
                        t.de_mean + d.de_mean,
                        t.de_p90 + d.de_p90,
                    );
                    abs_l[i] += d.dl.abs();
                }
            }
        }
    }
    println!("\nเฉลี่ย {n} รูป (ค่าบวก = ภาพของเรามากกว่า Lightroom)");
    for (i, v) in VARIANTS.iter().enumerate() {
        let t = sums[i];
        let k = n.max(1) as f64;
        println!(
            "{v}: ΔL* {:+.1} (|ΔL*| {:.1}) · Δa* {:+.1} · Δb* {:+.1} · ΔC* {:+.1} · Δσ(L*) {:+.1} · ΔE เฉลี่ย {:.1} · ΔE p90 {:.1} · ΔE ภาพย่อ {:.1}",
            t.dl / k,
            abs_l[i] / k,
            t.da / k,
            t.db / k,
            t.dc / k,
            t.dstd / k,
            t.de_mean / k,
            t.de_p90 / k,
            t.de_small / k
        );
    }
    println!("ส่วนของ Lightroom ที่ทำตามไม่ได้ (จาก XMP ใน JPEG): {unsupported:?}");
}
