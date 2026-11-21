//! วัด "ลุค" ของภาพเป็นตัวเลข และเสนอค่าการแต่งให้รูปหนึ่งดูเหมือนภาพตัวอย่าง (DECISIONS ข้อ 24)
//!
//! วัดในปริภูมิสี CIE Lab (D65) จากภาพย่อ: ความสว่าง (L* ที่เปอร์เซ็นไทล์ 2/25/50/75/98) ความอิ่มสี (chroma)
//! และ **โทนสีจากส่วนที่เกือบเป็นสีเทา** ทั้งภาพ / เงา / ส่วนสว่าง (a* เขียว↔ม่วงแดง, b* น้ำเงิน↔เหลือง) — วัดจากส่วนสีเทาเท่านั้น
//! เพราะสีของสิ่งของในภาพ (จีวรสีส้ม ต้นไม้) ไม่ใช่โทนของภาพ: ถ้าเฉลี่ยทั้งภาพ จะไปแก้สมดุลแสงขาวเพื่อชดเชยสีของเนื้อหา
//!
//! ค่าที่เสนอได้จากการค้นหาทีละค่าด้วยสูตรแต่งภาพตัวจริง (`adjust::apply`) **บนรูปที่ยังไม่ได้ปรับค่าเหล่านี้** และเป็นค่าจริง
//! (ไม่ใช่ส่วนเพิ่ม) — เรียกซ้ำหลังแต่งตามแล้วได้คำตอบเดิม ไม่สะสมแรงขึ้นทุกรอบ · ค่าอื่นของรูป (HSL เส้นโค้ง ฯลฯ) คงไว้และคิดรวมด้วย
//! · ส่วนสีเทาของรูปที่จะแต่งเลือกครั้งเดียวก่อนค้นหา (สีเทาจริงเท่านั้น) แล้ววัดส่วนเดิมทุกครั้ง ผลจึงเปลี่ยนตามค่าที่ลองอย่างราบรื่น
//! · ช่วงที่ค้นจำกัดไว้ — เป็นจุดเริ่ม: ฉากต่างกันจึงต้องดูด้วยตาประกอบ

use crate::adjust::{self, Adjust, Profile, Wheel};
use image::{DynamicImage, RgbImage};
use serde_json::{json, Value};
use std::fmt::Write as _;

/// ตัวเลขที่บอกลุคของภาพ
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// L* ที่เปอร์เซ็นไทล์ 2, 25, 50, 75, 98 (0–100)
    pub l: [f64; 5],
    /// โทนสีของส่วนที่เกือบเป็นสีเทาในช่วงความสว่างกลาง (L* 30–75) = สมดุลแสงขาวของภาพ — None เมื่อมีไม่ถึง 3% ของภาพ
    /// (ไม่รวมเงา/ส่วนสว่าง ซึ่งอาจถูกทำโทนแยก เช่น เงาฟ้า ส่วนสว่างส้ม)
    pub cast: Option<(f64, f64)>,
    /// ความอิ่มสีเฉลี่ย (ไม่นับส่วนที่ดำ/ขาวจนสุด)
    pub chroma: f64,
    /// โทนสีของส่วนสีเทาในเงา (L* < 35) และส่วนสว่าง (L* > 70) — None เมื่อมีไม่ถึง 1.5% ของภาพ
    pub shadows: Option<(f64, f64)>,
    pub highlights: Option<(f64, f64)>,
}

/// ภาพตัวอย่าง: ส่วนที่ chroma ต่ำกว่านี้ถือว่า "เกือบเป็นสีเทา" (ภาพที่ทำโทนสีแรง ๆ ส่วนสีเทาก็ยังต่ำกว่านี้)
const NEUTRAL_CHROMA: f64 = 18.0;
/// รูปที่จะแต่ง (ยังไม่ได้ทำโทน): สีเทาจริง (ผนังขาว เสื้อขาว ปูน) มี chroma ต่ำกว่า 10 **และ** ต่ำกว่า 0.12 × L* ·
/// สีอ่อนของสิ่งของ — ผนังเหลืองอ่อน ดอกไม้ชมพู ผิว — สูงกว่า ถ้านับรวมจะไปแก้สมดุลแสงขาวเพื่อลบสีของสิ่งของ
/// (เจอจริง: ผนังเหลือง + ดอกไม้ชมพู ได้ tint −65) · ใช้สัดส่วนต่อ L* ด้วยเพราะส่วนที่อยู่ในเงาของสีอ่อน chroma ลดตามความสว่าง
/// (ผนังเหลืองอ่อนในเงา L* 52 มี chroma ราว 9.8 — ผ่านเกณฑ์ 10 อย่างเดียว แต่สัดส่วนยังเท่าส่วนที่โดนแสง)
const TARGET_NEUTRAL_CHROMA: f64 = 10.0;
const TARGET_NEUTRAL_RATIO: f64 = 0.12;
/// ส่วนสีเทาช่วงกลางต้องมีอย่างน้อยเท่านี้ของภาพจึงอ่านสมดุลแสงขาว · เงา/ส่วนสว่างอย่างน้อยเท่านี้
const MIN_MIDS: f64 = 0.03;
const MIN_ZONE: f64 = 0.015;

impl Look {
    fn mid_contrast(&self) -> f64 {
        self.l[3] - self.l[1]
    }
}

fn srgb_to_linear() -> [f64; 256] {
    std::array::from_fn(|i| {
        let c = i as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
}

fn lab(lin: &[f64; 256], [r, g, b]: [u8; 3]) -> [f64; 3] {
    let (r, g, b) = (lin[r as usize], lin[g as usize], lin[b as usize]);
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn labs(img: &RgbImage) -> Vec<[f64; 3]> {
    let lin = srgb_to_linear();
    img.pixels().map(|p| lab(&lin, p.0)).collect()
}

/// ตำแหน่งพิกเซลที่ใช้วัดโทนสี แยกช่วงกลาง / เงา / ส่วนสว่าง
#[derive(Debug, Clone, Default)]
struct Grays {
    mids: Vec<usize>,
    darks: Vec<usize>,
    lights: Vec<usize>,
}

/// พิกเซลที่ chroma ต่ำกว่า `max_chroma` และต่ำกว่า `max_ratio` × L*
fn select_grays(labs: &[[f64; 3]], max_chroma: f64, max_ratio: f64) -> Grays {
    let mut g = Grays::default();
    for (i, &[l, a, b]) in labs.iter().enumerate() {
        let c = a.hypot(b);
        if c >= max_chroma || c >= max_ratio * l || l <= 3.0 || l >= 97.0 {
            continue;
        }
        if (30.0..=75.0).contains(&l) {
            g.mids.push(i);
        }
        if l < 35.0 {
            g.darks.push(i);
        } else if l > 70.0 {
            g.lights.push(i);
        }
    }
    g
}

/// ลุคของภาพ โดยวัดโทนสีจากพิกเซลตามตำแหน่งใน `grays`
fn look_of(labs: &[[f64; 3]], grays: &Grays) -> Look {
    let total = labs.len().max(1) as f64;
    // ฮิสโตแกรม L* ละเอียด 0.25
    let mut hist = vec![0u32; 401];
    let (mut sc, mut n) = (0.0, 0.0_f64);
    for &[l, a, b] in labs {
        hist[(l.clamp(0.0, 100.0) * 4.0).round() as usize] += 1;
        if l > 3.0 && l < 98.0 {
            sc += a.hypot(b);
            n += 1.0;
        }
    }
    let percentile = |p: f64| {
        let want = total * p;
        let mut seen = 0.0;
        for (i, c) in hist.iter().enumerate() {
            seen += *c as f64;
            if seen >= want {
                return i as f64 / 4.0;
            }
        }
        100.0
    };
    // มัธยฐาน ทนต่อพิกเซลสีอ่อนของสิ่งของที่หลุดเข้ามา
    let tone = |idx: &[usize], min: f64| {
        (idx.len() as f64 / total >= min).then(|| median_ab(idx.iter().map(|&i| (labs[i][1], labs[i][2])).collect()))
    };
    Look {
        l: [0.02, 0.25, 0.5, 0.75, 0.98].map(percentile),
        cast: tone(&grays.mids, MIN_MIDS),
        chroma: sc / n.max(1.0),
        shadows: tone(&grays.darks, MIN_ZONE),
        highlights: tone(&grays.lights, MIN_ZONE),
    }
}

/// วัดลุคของภาพ (ควรย่อก่อน — ราว 256 px ก็พอ)
pub fn measure(img: &RgbImage) -> Look {
    let labs = labs(img);
    look_of(&labs, &select_grays(&labs, NEUTRAL_CHROMA, f64::INFINITY))
}

fn median_ab(mut values: Vec<(f64, f64)>) -> (f64, f64) {
    let mid = values.len() / 2;
    values.sort_by(|x, y| x.0.total_cmp(&y.0));
    let a = values[mid].0;
    values.sort_by(|x, y| x.1.total_cmp(&y.1));
    (a, values[mid].1)
}

/// ระยะห่างของสองลุค (ยิ่งน้อยยิ่งใกล้) — ความสว่างกลาง คอนทราสต์ ความอิ่มสี และโทนสีที่วัดได้ทั้งสองภาพ
pub fn distance(a: &Look, b: &Look) -> f64 {
    let mut d =
        (a.l[2] - b.l[2]).abs() + 0.5 * (a.mid_contrast() - b.mid_contrast()).abs() + (a.chroma - b.chroma).abs();
    if let (Some(x), Some(y)) = (a.cast, b.cast) {
        d += (x.0 - y.0).hypot(x.1 - y.1);
    }
    for (x, y) in [(a.shadows, b.shadows), (a.highlights, b.highlights)] {
        if let (Some(x), Some(y)) = (x, y) {
            d += 0.5 * (x.0 - y.0).hypot(x.1 - y.1);
        }
    }
    d
}

/// คำอธิบายลุคแบบสั้น (ส่งให้ AI)
pub fn describe(look: &Look) -> String {
    let tone = |a: f64, b: f64| {
        let mut words = vec![];
        if b > 3.0 {
            words.push("warm/yellow");
        } else if b < -3.0 {
            words.push("cool/blue");
        }
        if a > 3.0 {
            words.push("magenta");
        } else if a < -3.0 {
            words.push("green");
        }
        if words.is_empty() {
            "neutral".to_string()
        } else {
            words.join("+")
        }
    };
    let mut s = format!(
        "brightness L* p2 {:.0} · p25 {:.0} · median {:.0} · p75 {:.0} · p98 {:.0} (mid-contrast {:.0}); chroma {:.1}",
        look.l[0],
        look.l[1],
        look.l[2],
        look.l[3],
        look.l[4],
        look.mid_contrast(),
        look.chroma
    );
    match look.cast {
        Some((a, b)) => {
            write!(s, "; tone of near-gray midtones (white balance) a* {a:+.1} b* {b:+.1} ({})", tone(a, b)).ok()
        }
        None => write!(s, "; too few near-gray midtones to read the white balance").ok(),
    };
    match look.shadows {
        Some((a, b)) => write!(s, "; gray shadows a* {a:+.1} b* {b:+.1} ({})", tone(a, b)).ok(),
        None => write!(s, "; few gray shadows").ok(),
    };
    match look.highlights {
        Some((a, b)) => write!(s, "; gray highlights a* {a:+.1} b* {b:+.1} ({})", tone(a, b)).ok(),
        None => write!(s, "; few gray highlights").ok(),
    };
    s
}

/// ค่าที่เสนอให้รูปหนึ่ง
pub struct Suggestion {
    /// การแต่งแสงสีทั้งชุดที่เสนอ: ค่าที่ค้นหาเป็นค่าจริง ค่าอื่นของรูปคงเดิม
    pub adjust: Adjust,
    /// ลุคของรูปตามที่แต่งไว้ตอนนี้ และเมื่อใช้ค่าที่เสนอ — วัดจากส่วนเดียวกันของภาพ เทียบกันและเทียบกับภาพตัวอย่างได้
    pub now: Look,
    pub after: Look,
    /// ปรับสมดุลแสงขาวด้วยหรือไม่ (ทั้งสองภาพต้องมีส่วนสีเทาจริงพอ — ไม่งั้นคงค่าเดิมของรูป)
    pub white_balance: bool,
    /// เทียบสีของเงา / ส่วนสว่างได้ (มีส่วนสีเทาพอทั้งสองภาพ) — ที่เทียบไม่ได้คงค่าเดิมของรูป
    pub zones: [bool; 2],
    /// แสงสีคัดลอกจากการแต่งของภาพตัวอย่าง (ปรับเฉพาะค่าแสงและสมดุลแสงขาวให้เข้ากับรูปนี้)
    pub copied: bool,
}

/// หาค่าเดียวในช่วง [lo, hi] ที่ทำให้ `f(x)` (เพิ่มตาม x) เข้าใกล้ 0 ที่สุด — bisection
fn solve(lo: f64, hi: f64, f: impl Fn(f64) -> f64) -> f64 {
    let (mut lo, mut hi) = (lo, hi);
    let (flo, fhi) = (f(lo), f(hi));
    if flo >= 0.0 {
        return lo;
    }
    if fhi <= 0.0 {
        return hi;
    }
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// สีของ hue บนวงล้อ Color Grading ที่ชี้ไปทาง (a*, b*) มากที่สุด
fn hue_toward(lin: &[f64; 256], da: f64, db: f64) -> f64 {
    let want = db.atan2(da);
    (0..180)
        .map(|i| i as f64 * 2.0)
        .min_by(|x, y| {
            let dist = |h: f64| {
                let c = adjust::hsl_to_rgb(h, 1.0, 0.5).map(|v| (v * 255.0).round() as u8);
                let [_, a, b] = lab(lin, c);
                let d = (b.atan2(a) - want).rem_euclid(std::f64::consts::TAU);
                d.min(std::f64::consts::TAU - d)
            };
            dist(*x).total_cmp(&dist(*y))
        })
        .unwrap_or(0.0)
}

/// ช่วงที่ค้นหา (ค่าจริงของรูป): ไม่ให้ค่าสุดโต่ง — ความอิ่มสีแคบกว่าเพราะสีของสิ่งของ (ดอกไม้ จีวร) ทำให้ค่านี้ขึ้นกับเนื้อหามาก
const EXPOSURE_RANGE: f64 = 1.5;
const WHITE_BALANCE_RANGE: f64 = 40.0;
const CONTRAST_RANGE: f64 = 40.0;
const SATURATION_RANGE: f64 = 30.0;
const GRADING_MAX: f64 = 60.0;

/// เสนอการแต่งแสงสีของรูปให้ลุคใกล้ `reference`
///
/// `base` = ภาพย่อของรูปหลังตัด/หมุน **ก่อนปรับแสงสี** · `current` = ค่าแสงสีของรูปตอนนี้ (ค่าที่ไม่ได้ค้นหาคงไว้และคิดรวม)
/// · `style` = ค่าแสงสีของภาพตัวอย่างเมื่อเป็นรูปในคลังที่แต่งในแอป: คัดลอกมาทั้งชุดแล้วปรับเฉพาะค่าแสงและสมดุลแสงขาวให้เข้ากับรูปนี้
/// — คอนทราสต์และความอิ่มสีที่วัดจากภาพขึ้นกับเนื้อหามาก (เจอจริง: ภาพตัวอย่างลดความอิ่มสี −25 แต่รูปที่จะแต่งสีจืดกว่า
/// การวัดจึงเสนอเพิ่มความอิ่มสี +30 สวนทางกับสไตล์) เมื่อรู้ค่าที่แต่งจริงจึงใช้ค่านั้นแทนการเดาจากตัวเลขของภาพ
pub fn suggest(
    base: &DynamicImage,
    current: &Adjust,
    reference: &Look,
    style: Option<&Adjust>,
    profile: Profile,
) -> Suggestion {
    let render = |d: &Adjust| labs(&adjust::apply_profile(base.clone(), d, profile).to_rgb8());
    match style {
        Some(style) => suggest_from_style(base, &render, current, reference, style),
        None => suggest_by_measuring(base, &render, current, reference),
    }
}

/// คัดลอกแสงสีของภาพตัวอย่าง แล้วหาค่าแสง (ความสว่างกลาง) และสมดุลแสงขาว (ส่วนสีเทาจริง) ให้รูปนี้
fn suggest_from_style(
    base: &DynamicImage,
    render: &dyn Fn(&Adjust) -> Vec<[f64; 3]>,
    current: &Adjust,
    reference: &Look,
    style: &Adjust,
) -> Suggestion {
    let total = (base.width() * base.height()).max(1) as f64;
    // สีเทาจริงเลือกจากรูปก่อนแต่ง (การทำโทนของสไตล์ทำให้ส่วนสีเทาไม่เทาแล้ว)
    let grays = select_grays(&render(&Adjust::default()), TARGET_NEUTRAL_CHROMA, TARGET_NEUTRAL_RATIO);
    let white_balance = reference.cast.is_some() && grays.mids.len() as f64 / total >= MIN_MIDS;
    let mut d = style.clone();
    d.exposure = 0.0;
    if white_balance {
        d.temperature = 0.0;
        d.tint = 0.0;
    }
    let eval = |d: &Adjust| look_of(&render(d), &grays);
    let cast_of = |l: &Look| l.cast.unwrap_or((0.0, 0.0));
    let want = cast_of(reference);
    for _ in 0..3 {
        d.exposure = solve(-EXPOSURE_RANGE, EXPOSURE_RANGE, |x| {
            eval(&Adjust { exposure: x, ..d.clone() }).l[2] - reference.l[2]
        });
        if white_balance {
            let r = WHITE_BALANCE_RANGE;
            d.temperature = solve(-r, r, |x| cast_of(&eval(&Adjust { temperature: x, ..d.clone() })).1 - want.1);
            d.tint = solve(-r, r, |x| cast_of(&eval(&Adjust { tint: x, ..d.clone() })).0 - want.0);
        }
    }
    d.exposure = (d.exposure * 20.0).round() / 20.0;
    d.temperature = d.temperature.round();
    d.tint = d.tint.round();
    Suggestion { now: eval(current), after: eval(&d), adjust: d, white_balance, zones: [false; 2], copied: true }
}

/// ไม่รู้ค่าที่แต่งของภาพตัวอย่าง: ค้นหาค่าจากตัวเลขของภาพ
fn suggest_by_measuring(
    base: &DynamicImage,
    render: &dyn Fn(&Adjust) -> Vec<[f64; 3]>,
    current: &Adjust,
    reference: &Look,
) -> Suggestion {
    let total = (base.width() * base.height()).max(1) as f64;
    // รูปตอนยังไม่ได้ปรับค่าที่ค้นหา (สีของเงา/ส่วนสว่างด้วย — ไม่งั้นค่าที่ใส่ไปแล้วทำให้คำตอบรอบถัดไปเปลี่ยน)
    // เลือกส่วนสีเทาจริงจากภาพนี้ครั้งเดียว
    let mut d = current.clone();
    d.exposure = 0.0;
    d.contrast = 0.0;
    d.saturation = 0.0;
    d.grading.shadows.sat = 0.0;
    d.grading.highlights.sat = 0.0;
    let mut plain = d.clone();
    plain.temperature = 0.0;
    plain.tint = 0.0;
    let grays = select_grays(&render(&plain), TARGET_NEUTRAL_CHROMA, TARGET_NEUTRAL_RATIO);
    let white_balance = reference.cast.is_some() && grays.mids.len() as f64 / total >= MIN_MIDS;
    if white_balance {
        d.temperature = 0.0;
        d.tint = 0.0;
    }
    let eval = |d: &Adjust| look_of(&render(d), &grays);
    let cast_of = |l: &Look| l.cast.unwrap_or((0.0, 0.0));
    let want = cast_of(reference);
    // หลายรอบ: ค่าแต่ละตัวกระทบกัน (เช่น คอนทราสต์เลื่อนค่ากลางความสว่าง)
    for _ in 0..3 {
        d.exposure = solve(-EXPOSURE_RANGE, EXPOSURE_RANGE, |x| {
            eval(&Adjust { exposure: x, ..d.clone() }).l[2] - reference.l[2]
        });
        if white_balance {
            let r = WHITE_BALANCE_RANGE;
            d.temperature = solve(-r, r, |x| cast_of(&eval(&Adjust { temperature: x, ..d.clone() })).1 - want.1);
            d.tint = solve(-r, r, |x| cast_of(&eval(&Adjust { tint: x, ..d.clone() })).0 - want.0);
        }
        d.contrast = solve(-CONTRAST_RANGE, CONTRAST_RANGE, |x| {
            eval(&Adjust { contrast: x, ..d.clone() }).mid_contrast() - reference.mid_contrast()
        });
        d.saturation = solve(-SATURATION_RANGE, SATURATION_RANGE, |x| {
            eval(&Adjust { saturation: x, ..d.clone() }).chroma - reference.chroma
        });
    }
    // สีของเงา/ส่วนสว่าง (split toning) ด้วย Color Grading: เลือกส่วนสีเทาของแต่ละช่วงจากภาพหลังปรับค่าข้างบน (ยังไม่ทำโทน)
    let lin = srgb_to_linear();
    let untoned = d.clone();
    let zone_grays = select_grays(&render(&untoned), TARGET_NEUTRAL_CHROMA, TARGET_NEUTRAL_RATIO);
    let zone_grays = Grays { mids: grays.mids.clone(), ..zone_grays };
    let base_look = look_of(&render(&untoned), &zone_grays);
    let mut zones = [false; 2];
    for zone in [0, 1] {
        let (want, have) = if zone == 0 {
            (reference.shadows, base_look.shadows)
        } else {
            (reference.highlights, base_look.highlights)
        };
        let (Some((ra, rb)), Some((ha, hb))) = (want, have) else {
            // เทียบไม่ได้: คงสีของช่วงนี้ตามที่รูปมีอยู่
            if zone == 0 {
                d.grading.shadows = current.grading.shadows;
            } else {
                d.grading.highlights = current.grading.highlights;
            }
            continue;
        };
        zones[zone] = true;
        let (da, db) = (ra - ha, rb - hb);
        let gap = da.hypot(db);
        let mut wheel =
            Wheel { hue: 0.0, sat: 0.0, lum: if zone == 0 { d.grading.shadows.lum } else { d.grading.highlights.lum } };
        if gap >= 2.0 {
            let hue = hue_toward(&lin, da, db);
            let (ua, ub) = (da / gap, db / gap);
            let moved = |sat: f64| {
                let mut g = untoned.grading;
                let w = Wheel { hue, sat, ..wheel };
                if zone == 0 {
                    g.shadows = w;
                } else {
                    g.highlights = w;
                }
                let after = look_of(&render(&Adjust { grading: g, ..untoned.clone() }), &zone_grays);
                let (na, nb) =
                    if zone == 0 { after.shadows.unwrap_or((ha, hb)) } else { after.highlights.unwrap_or((ha, hb)) };
                (na - ha) * ua + (nb - hb) * ub - gap
            };
            let sat = solve(0.0, GRADING_MAX, moved).round();
            if sat >= 1.0 {
                wheel = Wheel { hue: hue.round(), sat, ..wheel };
            }
        }
        if zone == 0 {
            d.grading.shadows = wheel;
        } else {
            d.grading.highlights = wheel;
        }
    }
    // ปัดให้อ่านง่าย (ค่าละเอียดกว่านี้ตาไม่เห็นความต่าง)
    d.exposure = (d.exposure * 20.0).round() / 20.0;
    for v in [&mut d.temperature, &mut d.tint, &mut d.contrast, &mut d.saturation] {
        *v = v.round();
    }
    Suggestion {
        now: look_of(&render(current), &zone_grays),
        after: look_of(&render(&d), &zone_grays),
        adjust: d,
        white_balance,
        zones,
        copied: false,
    }
}

/// ค่าที่เสนอเป็น patch แบบ JSON สำหรับ edit_photo — เฉพาะค่าที่ค้นหา (ค่าจริง) · ส่วนที่เทียบไม่ได้ไม่อยู่ใน patch (คงค่าเดิม)
/// · คัดลอกจากภาพตัวอย่าง = แสงสีทั้งชุด (แทนของเดิมทั้งหมด แบบใช้ preset)
pub fn patch(s: &Suggestion) -> Value {
    if s.copied {
        // ทุกช่อง: edit_photo รวม patch เข้ากับค่าเดิม ช่องที่ไม่ส่งจะค้างค่าเดิมของรูปไว้
        return serde_json::to_value(&s.adjust).unwrap_or_default();
    }
    let a = &s.adjust;
    let mut p = json!({ "exposure": a.exposure, "contrast": a.contrast, "saturation": a.saturation });
    if s.white_balance {
        p["temperature"] = json!(a.temperature);
        p["tint"] = json!(a.tint);
    }
    let mut grading = serde_json::Map::new();
    for (on, key, wheel) in
        [(s.zones[0], "shadows", a.grading.shadows), (s.zones[1], "highlights", a.grading.highlights)]
    {
        if on {
            grading.insert(key.into(), json!({ "hue": wheel.hue, "sat": wheel.sat, "lum": wheel.lum }));
        }
    }
    if !grading.is_empty() {
        p["grading"] = Value::Object(grading);
    }
    p
}

/// เฉพาะช่องที่ไม่ใช่ค่าเริ่มต้น (บอก AI ว่าภาพตัวอย่างแต่งอะไรไว้แบบสั้น)
pub fn compact(a: &Adjust) -> Value {
    let (Ok(Value::Object(mut map)), Ok(Value::Object(plain))) =
        (serde_json::to_value(a), serde_json::to_value(Adjust::default()))
    else {
        return json!({});
    };
    map.retain(|k, v| plain.get(k) != Some(v));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ภาพทดสอบที่มีหลายสีและหลายระดับความสว่าง (คล้ายภาพถ่ายมากกว่าสีเดียว)
    /// ส่วนบนเป็นสีหลายสี ส่วนล่างเป็นแถบสีเทาไล่ความสว่าง (เหมือนผนัง/เสื้อขาวในภาพจริง)
    fn scene(shift: u8) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(192, 128, |x, y| {
            let t = x as f64 / 191.0;
            let s = y as f64 / 127.0;
            if y >= 80 {
                let g = (20.0 + 225.0 * t) as u8;
                return image::Rgb([g, g, g]);
            }
            image::Rgb([
                (30.0 + 190.0 * t) as u8,
                (40.0 + 150.0 * s) as u8 ^ shift,
                (60.0 + 120.0 * (1.0 - t) * s) as u8,
            ])
        }))
    }

    #[test]
    fn neutral_gray_measures_as_neutral() {
        let gray = RgbImage::from_pixel(32, 32, image::Rgb([128, 128, 128]));
        let look = measure(&gray);
        assert!((look.l[2] - 53.6).abs() < 0.5, "{look:?}");
        let (a, b) = look.cast.unwrap();
        assert!(a.abs() < 0.3 && b.abs() < 0.3 && look.chroma < 0.5, "{look:?}");
        assert!(describe(&look).contains("neutral"));
        assert_eq!(distance(&look, &look), 0.0);
    }

    #[test]
    fn suggestion_recovers_a_known_look() {
        let original = scene(0);
        let wanted = Adjust { exposure: 0.5, temperature: 25.0, saturation: -20.0, ..Default::default() };
        let reference = measure(&adjust::apply(original.clone(), &wanted).to_rgb8());
        let s = suggest(&original, &Adjust::default(), &reference, None, Profile::Display);
        let d = &s.adjust;
        assert!(s.white_balance);
        assert!((d.exposure - 0.5).abs() <= 0.2, "exposure {}", d.exposure);
        assert!((d.temperature - 25.0).abs() <= 10.0, "temperature {}", d.temperature);
        assert!((d.saturation + 20.0).abs() <= 12.0, "saturation {}", d.saturation);
        assert!(d.tint.abs() <= 10.0 && d.contrast.abs() <= 15.0, "tint {} contrast {}", d.tint, d.contrast);
        // ลุคที่ได้ใกล้ภาพตัวอย่างกว่าเดิมมาก
        assert!(
            distance(&s.after, &reference) < distance(&s.now, &reference) / 4.0,
            "now {} after {}",
            distance(&s.now, &reference),
            distance(&s.after, &reference)
        );
    }

    /// ค่าที่เสนอเป็นค่าจริงที่คิดจากรูปก่อนปรับ: ใส่ตามแล้วเรียกซ้ำต้องได้คำตอบเดิม (เดิมเสนอส่วนเพิ่มบนภาพที่แต่งแล้ว
    /// รอบสองจึงสะสมแรงขึ้นเรื่อย ๆ เมื่อเนื้อหาของภาพทำให้วัดไม่ถึงภาพตัวอย่าง) · ค่าอื่นที่รูปมีอยู่คงเดิม
    #[test]
    fn asking_again_after_applying_gives_the_same_answer() {
        let original = scene(40);
        let mut wanted =
            Adjust { exposure: 0.4, temperature: 30.0, contrast: -15.0, saturation: -25.0, ..Default::default() };
        wanted.grading.shadows = Wheel { hue: 200.0, sat: 35.0, lum: 0.0 };
        let reference = measure(&adjust::apply(scene(0), &wanted).to_rgb8());
        let mine = Adjust { shadows: 30.0, vibrance: 10.0, ..Default::default() };
        let first = suggest(&original, &mine, &reference, None, Profile::Display);
        assert_eq!((first.adjust.shadows, first.adjust.vibrance), (30.0, 10.0), "ค่าที่ไม่ได้ค้นหาคงเดิม");
        let second = suggest(&original, &first.adjust, &reference, None, Profile::Display);
        assert_eq!(second.adjust, first.adjust);
        assert_eq!(patch(&second), patch(&first));
        // หลังใส่ค่าแล้ว "ตอนนี้" = "เมื่อใช้ค่าที่เสนอ"
        assert_eq!(second.now, second.after);
        assert!(distance(&second.now, &reference) < distance(&first.now, &reference));
    }

    #[test]
    fn colors_of_the_subject_are_not_mistaken_for_a_color_tone() {
        // ภาพตัวอย่างเป็นอีกฉากที่สีของสิ่งของต่างกันมาก แต่ไม่ได้แต่งโทนสีใด ๆ (ส่วนสีเทาเป็นกลางทั้งคู่)
        let s = suggest(&scene(0), &Adjust::default(), &measure(&scene(90).to_rgb8()), None, Profile::Display);
        assert!(
            s.adjust.temperature.abs() <= 8.0 && s.adjust.tint.abs() <= 8.0,
            "temperature {} tint {}",
            s.adjust.temperature,
            s.adjust.tint
        );
        let g = s.adjust.grading;
        assert!(g.shadows.sat <= 10.0 && g.highlights.sat <= 10.0, "{g:?}");
        // ภาพที่ไม่มีส่วนสีเทาเลย: ไม่แตะสมดุลแสงขาว และ patch ไม่มีค่าสมดุลแสงขาว (คงค่าเดิมของรูป)
        let colorful = DynamicImage::ImageRgb8(RgbImage::from_fn(64, 64, |x, _| image::Rgb([220, 60 + x as u8, 30])));
        let s = suggest(
            &colorful,
            &Adjust { temperature: 12.0, ..Default::default() },
            &measure(&scene(0).to_rgb8()),
            None,
            Profile::Display,
        );
        assert!(!s.white_balance);
        assert_eq!((s.adjust.temperature, s.adjust.tint), (12.0, 0.0));
        assert!(patch(&s).get("temperature").is_none());
    }

    /// ฉากโทนอุ่นที่ไม่มีสีเทาจริง (ผนังเหลืองอ่อน ดอกไม้ชมพู ผิวคน — สีอ่อนพอจะหลุดเข้าเกณฑ์ "เกือบเทา" ของภาพตัวอย่าง)
    /// เทียบกับภาพตัวอย่างที่ส่วนสีเทาเป็นกลาง: ต้องไม่ไปแก้สมดุลแสงขาวเพื่อลบสีของผนังและดอกไม้
    #[test]
    fn warm_scene_without_true_grays_keeps_its_white_balance() {
        let room = DynamicImage::ImageRgb8(RgbImage::from_fn(192, 128, |x, y| {
            let t = x as f64 / 191.0;
            let shade = |c: [f64; 3], k: f64| image::Rgb(c.map(|v| (v * k) as u8));
            match (x / 48, y / 64) {
                (0, _) | (1, 0) => shade([238.0, 226.0, 196.0], 0.55 + 0.45 * t), // ผนังเหลืองอ่อน
                (2, _) => shade([236.0, 196.0, 206.0], 0.6 + 0.4 * (y as f64 / 127.0)), // ดอกไม้ชมพูอ่อน
                (1, 1) => shade([205.0, 160.0, 135.0], 0.9),                      // ผิว
                _ => shade([224.0, 214.0, 190.0], 0.4 + 0.6 * (y as f64 / 127.0)), // ผ้าม่านครีม
            }
        }));
        let labs = labs(&room.to_rgb8());
        assert!(
            select_grays(&labs, NEUTRAL_CHROMA, f64::INFINITY).mids.len() > labs.len() / 4,
            "ฉากนี้มีสีอ่อนที่เกณฑ์ของภาพตัวอย่างนับเป็นสีเทาจริง (เงื่อนไขที่ทำให้พลาดเดิม)"
        );
        let reference = measure(&scene(0).to_rgb8());
        let s = suggest(&room, &Adjust::default(), &reference, None, Profile::Display);
        assert!(!s.white_balance, "ไม่มีสีเทาจริง = ไม่อ่านสมดุลแสงขาว");
        assert_eq!((s.adjust.temperature, s.adjust.tint), (0.0, 0.0));
        assert!(s.adjust.saturation >= -SATURATION_RANGE);
    }

    /// ภาพตัวอย่างแต่งในแอป: คัดลอกแสงสีทั้งชุด (รวมการลดความอิ่มสีที่การวัดจะพาไปทางตรงข้ามเมื่อเนื้อหาสีจืดกว่า)
    /// แล้วปรับเฉพาะค่าแสงและสมดุลแสงขาวให้เข้ากับรูปนี้ · ค่าเดิมของรูปถูกแทนทั้งหมด (แบบใช้ preset)
    #[test]
    fn an_edited_reference_lends_its_settings_and_only_exposure_and_white_balance_adapt() {
        let mut film =
            Adjust { exposure: 0.3, contrast: -10.0, temperature: 30.0, saturation: -25.0, ..Default::default() };
        film.grading.shadows = Wheel { hue: 205.0, sat: 40.0, lum: 0.0 };
        film.sat[1] = -15.0;
        film.hue[3] = 10.0;
        let reference = measure(&adjust::apply(scene(0), &film).to_rgb8());
        // รูปที่จะแต่งเป็นฉากเดียวกันแต่ถ่ายมืดกว่า 0.5 EV และสีจืดกว่า
        let darker = adjust::apply(scene(0), &Adjust { exposure: -0.5, saturation: -40.0, ..Default::default() });
        let mine = Adjust { vibrance: 30.0, temperature: -20.0, ..Default::default() };
        let s = suggest(&darker, &mine, &reference, Some(&film), Profile::Display);
        assert!(s.copied && s.white_balance);
        let d = &s.adjust;
        assert_eq!((d.contrast, d.saturation, d.vibrance), (-10.0, -25.0, 0.0), "คัดลอกจากภาพตัวอย่าง ค่าเดิมของรูปไม่ค้าง");
        assert_eq!((d.grading, d.hue, d.sat), (film.grading, film.hue, film.sat));
        assert!((d.exposure - 0.8).abs() <= 0.25, "ค่าแสงชดเชยรูปที่มืดกว่า: {}", d.exposure);
        assert!((d.temperature - 30.0).abs() <= 10.0, "temperature {}", d.temperature);
        assert!(distance(&s.after, &reference) < distance(&s.now, &reference));
        // patch มีทุกช่อง (edit_photo รวมเข้ากับค่าเดิม) และถามซ้ำได้คำตอบเดิม
        let p = patch(&s);
        assert_eq!(p["vibrance"], json!(0.0));
        assert_eq!(p["saturation"], json!(-25.0));
        let again = suggest(&darker, &s.adjust, &reference, Some(&film), Profile::Display);
        assert_eq!(again.adjust, s.adjust);
        // คำอธิบายสั้นมีเฉพาะช่องที่แต่ง
        let c = compact(&film);
        assert_eq!(c["saturation"], json!(-25.0));
        assert!(c.get("vibrance").is_none() && c.get("levels").is_none());
    }

    #[test]
    fn split_toning_is_matched_with_color_grading() {
        let original = scene(0);
        let mut wanted = Adjust::default();
        wanted.grading.shadows = Wheel { hue: 210.0, sat: 45.0, lum: 0.0 };
        wanted.grading.highlights = Wheel { hue: 40.0, sat: 35.0, lum: 0.0 };
        let reference = measure(&adjust::apply(original.clone(), &wanted).to_rgb8());
        let s = suggest(&original, &Adjust::default(), &reference, None, Profile::Display);
        let g = s.adjust.grading;
        assert!(g.shadows.sat > 10.0 && (150.0..=270.0).contains(&g.shadows.hue), "shadows {:?}", g.shadows);
        assert!(g.highlights.sat > 5.0 && (0.0..=90.0).contains(&g.highlights.hue), "highlights {:?}", g.highlights);
        let p = patch(&s);
        assert!(p["grading"]["shadows"]["sat"].as_f64().unwrap() > 10.0);
    }
}
