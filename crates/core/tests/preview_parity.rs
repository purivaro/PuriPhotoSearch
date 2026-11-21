//! ภาพตัวอย่างสดของหน้าแต่งภาพ (TypeScript) ตรงกับภาพที่บันทึก/ส่งออก (Rust) ทีละพิกเซล
//!
//! สุ่มค่าการแต่ง (ทุกแถบ HSL เส้นโค้ง Levels Color Grading Vignette Texture/Clarity/Dehaze ตัด/หมุน/เอียง และปรับเฉพาะส่วน)
//! กับภาพเล็ก ๆ (รวมดำล้วน ขาวล้วน ไล่โทน) [`CASES`] ชุดด้วย seed ตายตัว · Rust คิดผลด้วยขั้นเดียวกับภาพที่บันทึก
//! (`adjust::apply_profile` แล้ว `masks::apply` — ส่วนท้ายของ `edit::render_with`) แล้วให้ vitest คิดชุดเดียวกันด้วยโค้ดของ
//! ตัวอย่างสด (`src/features/photos/parity.test.ts` → `renderPixels`) และเทียบทีละพิกเซล ต่างได้ไม่เกิน [`TOLERANCE`] ระดับ
//!
//! ต้องมี node และ `pnpm install` แล้ว (CI ติดตั้งก่อนรันเทสต์ Rust) · ชุดที่เคยไม่ตรงถูกเก็บเป็นค่าอ้างอิงตายตัวของแต่ละสูตร

use image::{DynamicImage, Rgb, RgbImage};
use puri_core::adjust::{apply_profile_in_frame, Adjust, Grading, Levels, Profile, Vignette, Wheel};
use puri_core::edit::{EditSettings, Geometry, Rect};
use puri_core::masks::{Area, Mask};
use serde_json::json;
use std::path::Path;
use std::process::Command;

const CASES: u64 = 400;
const SEED: u64 = 20_261_121;
/// ต่างได้ไม่เกินเท่านี้ทุกช่องสีของทุกพิกเซล (การปัดทศนิยมที่ต่างกันเล็กน้อยระหว่าง f32/f64 และ Math ของ JavaScript)
const TOLERANCE: u8 = 1;

/// SplitMix64 — สุ่มแบบทำซ้ำได้ ไม่ต้องพึ่ง crate
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn int(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() % u64::from(hi - lo + 1)) as u32
    }

    fn byte(&mut self) -> u8 {
        (self.next() >> 56) as u8
    }

    /// ค่าของแถบ: บ่อยครั้งเป็น 0 บางครั้งชนขอบ ที่เหลือสุ่ม · ปัดตามช่วงที่แถบเลื่อนได้ (บางครั้งเป็นทศนิยมแบบค่าจาก Lightroom)
    fn slider(&mut self, max: f64, step: f64) -> f64 {
        let r = self.unit();
        let v = if r < 0.4 {
            0.0
        } else if r < 0.5 {
            max
        } else if r < 0.6 {
            -max
        } else {
            self.range(-max, max)
        };
        if self.chance(0.85) {
            (v / step).round() * step
        } else {
            (v * 1000.0).round() / 1000.0
        }
    }
}

fn levels(rng: &mut Rng) -> Levels {
    let in_black = rng.int(0, 120) as f64;
    let in_white = rng.int(in_black as u32 + 2, 255) as f64;
    let out_black = rng.int(0, 120) as f64;
    let out_white = rng.int(out_black as u32 + 1, 255) as f64;
    // แกมมา 0.1–9.99 แบบลอการิทึม (ส่วนใหญ่อยู่แถว 0.5–2)
    let gamma = (10f64.powf(rng.range(-1.0, 0.999)) * 100.0).round() / 100.0;
    Levels { in_black, in_white, gamma: gamma.clamp(0.1, 9.99), out_black, out_white }
}

fn curve(rng: &mut Rng) -> Vec<[f64; 2]> {
    let n = rng.int(2, 6);
    let mut xs: Vec<f64> = (0..n).map(|_| rng.int(0, 255) as f64).collect();
    if rng.chance(0.5) {
        xs[0] = 0.0;
        xs[n as usize - 1] = 255.0;
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs.into_iter().map(|x| [x, rng.int(0, 255) as f64]).collect()
}

fn wheel(rng: &mut Rng) -> Wheel {
    if !rng.chance(0.5) {
        return Wheel::default();
    }
    Wheel { hue: rng.int(0, 360) as f64, sat: rng.int(0, 100) as f64, lum: rng.slider(100.0, 1.0) }
}

/// ค่าแสงสีของทั้งภาพ — แต่ละกลุ่มถูกเปิดแบบสุ่ม
fn adjust(rng: &mut Rng) -> Adjust {
    let mut a = Adjust::default();
    if rng.chance(0.7) {
        a.exposure = rng.slider(3.0, 0.05);
        (a.contrast, a.highlights, a.shadows) =
            (rng.slider(100.0, 1.0), rng.slider(100.0, 1.0), rng.slider(100.0, 1.0));
        (a.whites, a.blacks) = (rng.slider(100.0, 1.0), rng.slider(100.0, 1.0));
    }
    if rng.chance(0.6) {
        (a.temperature, a.tint) = (rng.slider(100.0, 1.0), rng.slider(100.0, 1.0));
        (a.vibrance, a.saturation) = (rng.slider(100.0, 1.0), rng.slider(100.0, 1.0));
    }
    if rng.chance(0.35) {
        (a.texture, a.clarity, a.dehaze) = (rng.slider(100.0, 1.0), rng.slider(100.0, 1.0), rng.slider(100.0, 1.0));
    }
    if rng.chance(0.3) {
        for i in 0..8 {
            (a.hue[i], a.sat[i], a.lum[i]) = (rng.slider(100.0, 1.0), rng.slider(100.0, 1.0), rng.slider(100.0, 1.0));
        }
    }
    if rng.chance(0.2) {
        a.parametric = std::array::from_fn(|_| rng.slider(100.0, 1.0));
        if rng.chance(0.5) {
            let s0 = rng.int(5, 40) as f64;
            let s1 = rng.int(s0 as u32 + 1, 70) as f64;
            a.parametric_split = Some([s0, s1, rng.int(s1 as u32 + 1, 95) as f64]);
        }
    }
    if rng.chance(0.25) {
        a.curve = curve(rng);
    }
    for c in [&mut a.curve_red, &mut a.curve_green, &mut a.curve_blue] {
        if rng.chance(0.12) {
            *c = curve(rng);
        }
    }
    if rng.chance(0.2) {
        a.levels = levels(rng);
    }
    for l in [&mut a.levels_red, &mut a.levels_green, &mut a.levels_blue] {
        if rng.chance(0.1) {
            *l = levels(rng);
        }
    }
    if rng.chance(0.2) {
        a.grading = Grading {
            shadows: wheel(rng),
            midtones: wheel(rng),
            highlights: wheel(rng),
            global: wheel(rng),
            blending: rng.int(0, 100) as f64,
            balance: rng.slider(100.0, 1.0),
        };
    }
    if rng.chance(0.2) {
        a.vignette = Vignette {
            amount: rng.slider(100.0, 1.0),
            midpoint: rng.int(0, 100) as f64,
            feather: rng.int(0, 100) as f64,
            roundness: rng.slider(100.0, 1.0),
        };
    }
    a
}

/// ค่าของบริเวณที่ปรับเฉพาะส่วน: แถบที่แผงปรับเฉพาะส่วนมี (บางครั้งทุกอย่างแบบที่ AI/ค่าที่เก็บไว้ส่งมาได้)
fn mask_adjust(rng: &mut Rng) -> Adjust {
    if rng.chance(0.2) {
        return adjust(rng);
    }
    // ลำดับการสุ่มตามลำดับช่อง (ชุดที่ได้ต้องคงเดิม)
    Adjust {
        exposure: rng.slider(3.0, 0.05),
        contrast: rng.slider(100.0, 1.0),
        highlights: rng.slider(100.0, 1.0),
        shadows: rng.slider(100.0, 1.0),
        texture: rng.slider(100.0, 1.0),
        clarity: rng.slider(100.0, 1.0),
        dehaze: rng.slider(100.0, 1.0),
        temperature: rng.slider(100.0, 1.0),
        tint: rng.slider(100.0, 1.0),
        saturation: rng.slider(100.0, 1.0),
        ..Default::default()
    }
}

fn masks(rng: &mut Rng) -> Vec<Mask> {
    if !rng.chance(0.35) {
        return vec![];
    }
    (0..rng.int(1, 2))
        .map(|i| {
            let boxes = (0..rng.int(1, 3))
                .map(|_| [rng.range(-0.1, 0.9), rng.range(-0.1, 0.9), rng.range(0.03, 0.45), rng.range(0.03, 0.5)])
                .collect();
            Mask {
                id: format!("m{i}"),
                area: Area::Faces { boxes, skin: rng.chance(0.6) },
                adjust: mask_adjust(rng),
                amount: if rng.chance(0.6) { 100.0 } else { rng.int(0, 100) as f64 },
                invert: rng.chance(0.25),
            }
        })
        .collect()
}

fn geometry(rng: &mut Rng, s: &mut EditSettings) {
    if !rng.chance(0.3) {
        return;
    }
    s.rotate = [0, 90, 180, 270][rng.int(0, 3) as usize];
    if rng.chance(0.3) {
        s.straighten = (rng.range(-20.0, 20.0) * 10.0).round() / 10.0;
    }
    if rng.chance(0.4) {
        let (w, h) = (rng.range(0.15, 1.0), rng.range(0.15, 1.0));
        s.crop = Some(Rect { x: rng.range(0.0, 1.0 - w), y: rng.range(0.0, 1.0 - h), w, h });
    }
}

/// ภาพของกรอบที่แต่ง (หลังตัด/หมุน) — ฉากต่าง ๆ รวมกรณีขอบ · ฉากสุดท้ายมี "ใบหน้า" สีผิวตรงกรอบของ Mask
fn picture(rng: &mut Rng, w: u32, h: u32, faces: &[(f64, f64, f64, f64)]) -> RgbImage {
    let base = [rng.byte(), rng.byte(), rng.byte()];
    let kind = rng.int(0, 7);
    let mut img = match kind {
        0 => RgbImage::from_pixel(w, h, Rgb([0, 0, 0])),
        1 => RgbImage::from_pixel(w, h, Rgb([255, 255, 255])),
        2 => RgbImage::from_fn(w, h, |x, _| {
            let v = (x * 255 / (w - 1)) as u8;
            Rgb([v, v, v])
        }),
        3 => RgbImage::from_fn(w, h, |x, y| {
            let (r, g) = ((x * 255 / (w - 1)) as u8, (y * 255 / (h - 1)) as u8);
            Rgb([r, g, 255 - r / 2 - g / 2])
        }),
        4 => RgbImage::from_pixel(w, h, Rgb(base)),
        5 => {
            let mut img = RgbImage::new(w, h);
            for p in img.pixels_mut() {
                *p = Rgb([rng.byte(), rng.byte(), rng.byte()]);
            }
            img
        }
        6 => {
            // พื้นผิวละเอียดบนสีพื้น (แบบผิว/ผ้า) — ให้ Texture/Clarity มีรายละเอียดให้ทำงาน
            let mut img = RgbImage::new(w, h);
            for (x, y, p) in img.enumerate_pixels_mut() {
                let wave = ((x as f64 * 0.7).sin() * 18.0 + (y as f64 * 0.45).cos() * 12.0) as i32;
                let n = (rng.next() % 17) as i32 - 8;
                *p = Rgb(base.map(|c| (c as i32 + wave + n).clamp(0, 255) as u8));
            }
            img
        }
        _ => RgbImage::from_fn(w, h, |x, y| {
            if x < w / 4 && y > h / 2 {
                Rgb([235, 140, 20])
            } else {
                Rgb([120 + (x % 7) as u8, 118, 116 + (y % 5) as u8])
            }
        }),
    };
    if kind == 7 || rng.chance(0.3) {
        for &(cx, cy, rx, ry) in faces {
            let tone = [rng.int(150, 235) as u8, rng.int(100, 175) as u8, rng.int(80, 150) as u8];
            for (x, y, p) in img.enumerate_pixels_mut() {
                let (dx, dy) = ((x as f64 + 0.5 - cx) / rx.max(1.0), (y as f64 + 0.5 - cy) / ry.max(1.0));
                if dx * dx + dy * dy <= 1.0 {
                    *p = Rgb(tone);
                }
            }
        }
    }
    img
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// ผลของ Rust: ขั้นเดียวกับภาพที่บันทึกหลังตัด/หมุน (`edit::render_with` = render_geometry → apply_profile_in_frame → with_masks)
fn render(input: &RgbImage, s: &EditSettings, profile: Profile, source: (u32, u32)) -> RgbImage {
    let (w, h) = input.dimensions();
    let g = Geometry::new(source.0, source.1, s);
    let adjusted =
        apply_profile_in_frame(DynamicImage::ImageRgb8(input.clone()), &s.adjust, profile, g.frame_long(w, h));
    let mut px = adjusted.into_rgb8().into_raw();
    puri_core::masks::apply(&mut px, w as usize, h as usize, 3, &g, &s.masks);
    RgbImage::from_raw(w, h, px).unwrap()
}

/// ชื่อสั้น ๆ ของสิ่งที่เปิดในชุดนี้ (ให้รายงานชุดที่ไม่ตรงอ่านง่าย)
fn features(s: &EditSettings, profile: Profile) -> Vec<&'static str> {
    let a = &s.adjust;
    let mut f = vec![];
    if profile == Profile::Raw {
        f.push("raw");
    }
    let on = |v: &[f64]| v.iter().any(|x| *x != 0.0);
    if on(&[a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks]) {
        f.push("light");
    }
    if on(&[a.temperature, a.tint]) {
        f.push("wb");
    }
    if on(&[a.vibrance, a.saturation]) {
        f.push("sat");
    }
    if on(&[a.texture, a.clarity, a.dehaze]) {
        f.push("local");
    }
    if on(&a.hue) || on(&a.sat) || on(&a.lum) {
        f.push("hsl");
    }
    if on(&a.parametric) {
        f.push("parametric");
    }
    if [&a.curve, &a.curve_red, &a.curve_green, &a.curve_blue].iter().any(|c| !c.is_empty()) {
        f.push("curves");
    }
    if [a.levels, a.levels_red, a.levels_green, a.levels_blue].iter().any(|l| !l.is_identity()) {
        f.push("levels");
    }
    if a.grading.is_active() {
        f.push("grading");
    }
    if a.vignette.amount != 0.0 {
        f.push("vignette");
    }
    if s.rotate != 0 || s.straighten != 0.0 || s.crop.is_some() {
        f.push("geometry");
    }
    if s.masks.iter().any(Mask::is_active) {
        f.push("masks");
    }
    f
}

#[test]
fn live_preview_matches_the_saved_image_pixel_for_pixel() {
    let mut cases = vec![];
    // ลองชุดมากขึ้นได้: PURI_PARITY_N=<จำนวน>
    let n = std::env::var("PURI_PARITY_N").ok().and_then(|v| v.parse().ok()).unwrap_or(CASES);
    // ดูเฉพาะบางชุด: PURI_PARITY_ONLY=5995,11569 (ใช้กับ PURI_PARITY_DUMP)
    let only: Option<Vec<u64>> =
        std::env::var("PURI_PARITY_ONLY").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let ids: Vec<u64> = only.unwrap_or_else(|| (0..n).collect());
    for id in ids {
        let mut rng = Rng(SEED.wrapping_mul(1_000_003).wrapping_add(id));
        let mut s = EditSettings { adjust: adjust(&mut rng), ..Default::default() };
        geometry(&mut rng, &mut s);
        s.masks = masks(&mut rng);
        let profile = if rng.chance(0.2) { Profile::Raw } else { Profile::Display };
        // ขนาดภาพเดิม (สัดส่วนมีผลกับการแปลงพิกัดของ Mask) และกรอบที่แต่งย่อให้อยู่ใน 16–64 × 16–48 ตามสัดส่วนจริง
        let source = (rng.int(300, 6000), rng.int(300, 6000));
        let g = Geometry::new(source.0, source.1, &s);
        let (fw, fh) = g.output_size();
        let k = (rng.int(32, 64) as f64 / fw as f64).min(rng.int(24, 48) as f64 / fh as f64);
        let (w, h) = (((fw as f64 * k).round() as u32).clamp(16, 64), ((fh as f64 * k).round() as u32).clamp(16, 48));
        // ใบหน้าบนกรอบที่แต่ง (พิกเซล) ตรงกับกรอบของ Mask บนภาพเดิม
        let faces: Vec<(f64, f64, f64, f64)> = s
            .masks
            .iter()
            .flat_map(|m| match &m.area {
                Area::Faces { boxes, .. } => boxes.clone(),
            })
            .map(|b| {
                let (cx, cy) = g.forward(b[0] + b[2] / 2.0, b[1] + b[3] * 0.56);
                (cx * w as f64, cy * h as f64, b[2] * 0.45 * w as f64, b[3] * 0.5 * h as f64)
            })
            .collect();
        let input = picture(&mut rng, w, h, &faces);
        s.validate().unwrap_or_else(|e| panic!("ชุด {id} สร้างค่าที่แอปไม่รับ: {e}"));
        let output = render(&input, &s, profile, source);
        cases.push(json!({
            "id": id,
            "w": w,
            "h": h,
            "sourceW": source.0,
            "sourceH": source.1,
            "profile": if profile == Profile::Raw { "raw" } else { "display" },
            "features": features(&s, profile),
            "settings": s,
            "input": hex(input.as_raw()),
            "output": hex(output.as_raw()),
        }));
    }

    let dir = tempfile::tempdir().unwrap();
    let (file, report) = (dir.path().join("cases.json"), dir.path().join("report.json"));
    let tolerance = std::env::var("PURI_PARITY_TOLERANCE").ok().and_then(|v| v.parse().ok()).unwrap_or(TOLERANCE);
    std::fs::write(&file, serde_json::to_vec(&json!({ "tolerance": tolerance, "cases": cases })).unwrap()).unwrap();
    // ดูชุดที่ไม่ตรงทีละชุด: PURI_PARITY_DUMP=<ไฟล์> เก็บชุดทั้งหมดไว้
    if let Some(dump) = std::env::var_os("PURI_PARITY_DUMP") {
        std::fs::copy(&file, dump).unwrap();
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let vitest = root.join("node_modules/vitest/vitest.mjs");
    assert!(vitest.is_file(), "ต้อง `pnpm install` ก่อน: เทสต์นี้รันโค้ดตัวอย่างสดของหน้าแต่งภาพด้วย vitest");
    let out = Command::new("node")
        .arg(&vitest)
        .args(["run", "src/features/photos/parity.test.ts"])
        .current_dir(&root)
        .env("PURI_PARITY_CASES", &file)
        .env("PURI_PARITY_REPORT", &report)
        .output()
        .expect("เรียก node ไม่ได้ (ต้องมี Node.js)");
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let failures = std::fs::read_to_string(&report).unwrap_or_default();
        panic!("ตัวอย่างสดต่างจากภาพที่บันทึกเกิน {TOLERANCE} ระดับ:\n{failures}\n\n{text}");
    }
    assert!(text.contains("1 passed"), "vitest ไม่ได้รันเทสต์เทียบผล:\n{text}");
}
