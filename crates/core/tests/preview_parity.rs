//! ภาพตัวอย่างสดของหน้าแต่งภาพตรงกับภาพที่บันทึก/ส่งออกทุกพิกเซล (DECISIONS ข้อ 29–30)
//!
//! ตัวอย่างสดใช้ฟังก์ชันชุดเดียวกับไฟล์ที่บันทึกแล้ว แต่ทางเดินต่างกัน: canvas เป็น RGBA ส่วนไฟล์เป็น RGB · ตัวอย่างสุ่มส่วนที่เห็นจาก
//! ภาพต้นทางตามขนาด canvas · รัศมีของ Texture/Clarity/Dehaze และเรขาคณิตของ Mask ส่งต่อคนละทาง — เทสต์นี้สุ่มค่าการแต่ง
//! (ทุกแถบ HSL เส้นโค้ง Levels Color Grading Vignette Texture/Clarity/Dehaze หมุนทีละ 90° และ Mask ใบหน้า) กับภาพเล็ก ๆ
//! (รวมดำล้วน ขาวล้วน ไล่โทน) [`CASES`] ชุดด้วย seed ตายตัว แล้วเทียบ [`puri_core::preview::render_image`] ที่ขนาดภาพเต็ม
//! กับ [`puri_core::edit::render_with`] ต้องเท่ากันทุกบิต (หมุนทีละ 90° พิกเซลตรงกันพอดี ไม่มีการสุ่มภาพใหม่)
//!
//! ดูชุดที่ไม่ตรง: `PURI_PARITY_ONLY=<ids>` · ลองมากขึ้น: `PURI_PARITY_N=20000`

use image::{DynamicImage, Rgb, RgbImage};
use puri_core::adjust::{Adjust, Grading, Levels, Profile, Sharpen, Vignette, Wheel};
use puri_core::edit::{EditSettings, Geometry, Rect};
use puri_core::masks::{Area, Mask};
use puri_core::preview::{render_image, Request, SourceKind};
use puri_core::retouch::Retouch;

const CASES: u64 = 400;
const SEED: u64 = 20_261_121;

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
    if rng.chance(0.2) {
        a.sharpen = Sharpen {
            amount: rng.int(1, 150) as f64,
            radius: rng.int(5, 30) as f64 / 10.0,
            detail: rng.int(0, 100) as f64,
            masking: rng.int(0, 100) as f64,
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
            // ตัวคน (DECISIONS ข้อ 40): ภาพน้ำหนักวงรีนุ่ม ๆ ขนาดไม่เท่าภาพ (แนบไว้แล้วเหมือนตอนแอปสร้างภาพ)
            let area = if rng.chance(0.3) {
                let (mw, mh) = (rng.int(7, 40), rng.int(7, 40));
                let (cx, cy, r) = (rng.range(0.2, 0.8), rng.range(0.2, 0.8), rng.range(0.15, 0.5));
                let map = image::GrayImage::from_fn(mw, mh, |x, y| {
                    let d = ((x as f64 / mw as f64 - cx).powi(2) + (y as f64 / mh as f64 - cy).powi(2)).sqrt() / r;
                    image::Luma([((1.2 - d).clamp(0.0, 1.0) * 255.0) as u8])
                });
                Area::Subject { map: Some(puri_core::subject::SubjectMap::new(map)) }
            } else {
                Area::Faces { boxes, skin: rng.chance(0.6) }
            };
            Mask {
                id: format!("m{i}"),
                area,
                adjust: mask_adjust(rng),
                amount: if rng.chance(0.6) { 100.0 } else { rng.int(0, 100) as f64 },
                invert: rng.chance(0.25),
            }
        })
        .collect()
}

/// รีทัชใบหน้า: ใบหน้าเดียวกับ Mask (ภาพมีสีผิวตรงนั้น) หรือกรอบสุ่ม · ระดับสำเร็จรูปหรือค่าสุ่ม
fn retouch(rng: &mut Rng, masks: &[Mask]) -> Option<Retouch> {
    if !rng.chance(0.4) {
        return None;
    }
    let mut faces: Vec<[f64; 4]> = masks
        .iter()
        .flat_map(|m| match &m.area {
            Area::Faces { boxes, .. } => boxes.clone(),
            Area::Subject { .. } => vec![],
        })
        .collect();
    if faces.is_empty() || rng.chance(0.3) {
        faces = (0..rng.int(1, 2))
            .map(|_| [rng.range(-0.05, 0.7), rng.range(-0.05, 0.6), rng.range(0.15, 0.6), rng.range(0.2, 0.7)])
            .collect();
    }
    let amount = |rng: &mut Rng| match rng.int(0, 5) {
        0 | 1 => 0.0,
        2 => 25.0,
        3 => 50.0,
        4 => 80.0,
        _ => rng.int(1, 100) as f64,
    };
    Some(Retouch { faces, smooth: amount(rng), wrinkles: amount(rng), slim: amount(rng), thin: amount(rng) })
}

/// หมุนทีละ 90° (ตัด/ปรับเอียงต้องสุ่มภาพใหม่ — ตัวอย่างกับไฟล์จึงต่างกันตามวิธีย่อภาพ ไม่ใช่สูตร)
fn geometry(rng: &mut Rng, s: &mut EditSettings) {
    if rng.chance(0.4) {
        s.rotate = [0, 90, 180, 270][rng.int(0, 3) as usize];
    }
}

/// ภาพต้นทาง — ฉากต่าง ๆ รวมกรณีขอบ · ฉากสุดท้ายมี "ใบหน้า" สีผิวตรงกรอบของ Mask
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
    if a.sharpen.is_active() {
        f.push("sharpen");
    }
    if s.rotate != 0 || s.straighten != 0.0 || s.crop.is_some() {
        f.push("geometry");
    }
    if s.masks.iter().any(Mask::is_active) {
        f.push("masks");
    }
    if s.retouch.as_ref().is_some_and(Retouch::is_active) {
        f.push("retouch");
    }
    f
}

#[test]
fn live_preview_matches_the_saved_image_pixel_for_pixel() {
    let n = std::env::var("PURI_PARITY_N").ok().and_then(|v| v.parse().ok()).unwrap_or(CASES);
    let only: Option<Vec<u64>> =
        std::env::var("PURI_PARITY_ONLY").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let mut failures = vec![];
    for id in only.unwrap_or_else(|| (0..n).collect()) {
        let mut rng = Rng(SEED.wrapping_mul(1_000_003).wrapping_add(id));
        let mut s = EditSettings { adjust: adjust(&mut rng), ..Default::default() };
        geometry(&mut rng, &mut s);
        s.masks = masks(&mut rng);
        // รีทัชใช้ลำดับสุ่มของตัวเอง (ชุดเดิมของค่าอื่นไม่เปลี่ยน)
        let mut rr = Rng(SEED ^ 0xA5A5_5A5A_u64.wrapping_add(id.wrapping_mul(7919)));
        s.retouch = retouch(&mut rr, &s.masks);
        let profile = if rng.chance(0.2) { Profile::Raw } else { Profile::Display };
        let (w, h) = (rng.int(16, 64), rng.int(16, 48));
        // ใบหน้าบนภาพต้นทาง (พิกเซล) ตรงกับกรอบของ Mask
        let faces: Vec<(f64, f64, f64, f64)> = s
            .masks
            .iter()
            .flat_map(|m| match &m.area {
                Area::Faces { boxes, .. } => boxes.clone(),
                Area::Subject { .. } => vec![],
            })
            .map(|b| {
                (
                    (b[0] + b[2] / 2.0) * w as f64,
                    (b[1] + b[3] * 0.56) * h as f64,
                    b[2] * 0.45 * w as f64,
                    b[3] * 0.5 * h as f64,
                )
            })
            .collect();
        let input = picture(&mut rng, w, h, &faces);
        s.validate().unwrap_or_else(|e| panic!("ชุด {id} สร้างค่าที่แอปไม่รับ: {e}"));
        let saved = puri_core::edit::render_with(DynamicImage::ImageRgb8(input.clone()), &s, profile).into_rgb8();
        let (ow, oh) = saved.dimensions();
        assert_eq!((ow, oh), Geometry::new(w, h, &s).output_size());
        let req = Request {
            photo_id: String::new(),
            source: SourceKind::Original,
            settings: s.clone(),
            width: ow,
            height: oh,
            view: Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
            whole: true,
            adjust: true,
            show: None,
        };
        let shown = render_image(&input, profile, &req);
        let mut over = 0;
        let mut first = None;
        for (i, (p, q)) in shown.as_chunks::<4>().0.iter().zip(saved.pixels()).enumerate() {
            if p[..3] != q.0 || p[3] != 255 {
                over += 1;
                first.get_or_insert((i as u32 % ow, i as u32 / ow, [p[0], p[1], p[2]], q.0));
            }
        }
        if over > 0 {
            failures.push(format!(
                "ชุด {id} ({ow}×{oh} {}): ต่าง {over} พิกเซล เช่น {first:?}",
                features(&s, profile).join(" ")
            ));
        }
    }
    assert!(failures.is_empty(), "ตัวอย่างสดต่างจากภาพที่บันทึก {} ชุด:\n{}", failures.len(), failures.join("\n"));
}
