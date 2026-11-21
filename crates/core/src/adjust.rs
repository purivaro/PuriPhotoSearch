//! แสงและสีของการแต่งภาพ (DECISIONS ข้อ 18, 20) — สูตรเดียวกับ `adjust.ts` ฝั่ง UI (เทสต์ค่าเดียวกันทั้งสองฝั่ง)
//!
//! ขั้นที่แยกต่อช่องสีได้รวมเป็นตาราง 256 ค่าต่อช่อง: สมดุลแสงขาว ค่าแสง จุดดำ/จุดขาว ส่วนสว่าง/เงา คอนทราสต์
//! → Parametric curve → Levels และเส้นโค้งโทนแบบจุด (รวม แล้วแยกช่อง) · จากนั้นต่อพิกเซล: HSL ต่อช่วงสี → Color Grading
//! → ความสด/ความอิ่มสี → Vignette (ตามตำแหน่งในกรอบหลังตัด)
//! ชื่อแถบและเลขแบบเดียวกับ Lightroom เพื่อนำเข้า preset ได้ แต่สูตรเป็นของเราเอง (Adobe ไม่เปิดสูตร) จึงใกล้เคียง ไม่เท่ากันทุกพิกเซล

use crate::error::{AppError, Result};
use image::DynamicImage;
use serde::{Deserialize, Serialize};

/// แปดช่วงสีแบบ Lightroom: แดง ส้ม เหลือง เขียว ฟ้าน้ำทะเล น้ำเงิน ม่วง ม่วงแดง (จุดกลาง เป็นองศาของ hue)
pub const HUE_CENTERS: [f64; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];
/// จุดแบ่งช่วงของ Parametric curve เมื่อไม่ได้กำหนด (เท่า Lightroom)
const DEFAULT_SPLITS: [f64; 3] = [25.0, 50.0, 75.0];
const MAX_CURVE_POINTS: usize = 32;

/// ค่าแสงและสี (0 / ว่าง = ไม่ปรับ)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Adjust {
    /// ค่าแสง หน่วย EV (−3 ถึง 3)
    pub exposure: f64,
    // แถบที่เหลือ −100 ถึง 100
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
    /// ปรับที่ดูพิกเซลรอบข้าง (DECISIONS ข้อ 27): รายละเอียดเล็ก (ผิว เนื้อผ้า) · มิติช่วงกลาง · หมอก — ลบ = นุ่ม/เติมหมอก
    pub texture: f64,
    pub clarity: f64,
    pub dehaze: f64,
    /// อุณหภูมิสี: บวก = อุ่น (เหลือง) ลบ = เย็น (ฟ้า)
    pub temperature: f64,
    /// โทนสี: บวก = ม่วงแดง ลบ = เขียว
    pub tint: f64,
    pub vibrance: f64,
    pub saturation: f64,
    /// HSL ต่อช่วงสี (ลำดับตาม [`HUE_CENTERS`]): เลื่อนสี / ความอิ่ม / ความสว่าง
    pub hue: [f64; 8],
    pub sat: [f64; 8],
    pub lum: [f64; 8],
    /// Parametric curve: [เงา มืด สว่าง ส่วนสว่างมาก]
    pub parametric: [f64; 4],
    /// จุดแบ่งช่วงของ Parametric curve (0–100) — None = 25/50/75
    pub parametric_split: Option<[f64; 3]>,
    /// เส้นโค้งโทนแบบจุด (เข้า, ออก) 0–255 เรียงตามค่าเข้า · ว่าง = เส้นตรง
    pub curve: Vec<[f64; 2]>,
    pub curve_red: Vec<[f64; 2]>,
    pub curve_green: Vec<[f64; 2]>,
    pub curve_blue: Vec<[f64; 2]>,
    /// Levels แบบ Photoshop: รวมทุกช่อง แล้วแยกช่องแดง/เขียว/น้ำเงิน (DECISIONS ข้อ 21)
    pub levels: Levels,
    pub levels_red: Levels,
    pub levels_green: Levels,
    pub levels_blue: Levels,
    /// Color Grading: เติมสีให้เงา ช่วงกลาง ส่วนสว่าง และทั้งภาพ
    pub grading: Grading,
    /// ขอบภาพมืด/สว่างลง คิดตามกรอบหลังตัด
    pub vignette: Vignette,
}

/// Levels: ค่าเข้า (จุดดำ แกมมา จุดขาว) → ค่าออก (ดำ ขาว) ทุกค่า 0–255 ยกเว้นแกมมา (0.1–9.99, 1 = ไม่ปรับ)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Levels {
    pub in_black: f64,
    pub in_white: f64,
    pub gamma: f64,
    pub out_black: f64,
    pub out_white: f64,
}

impl Default for Levels {
    fn default() -> Self {
        Levels { in_black: 0.0, in_white: 255.0, gamma: 1.0, out_black: 0.0, out_white: 255.0 }
    }
}

impl Levels {
    pub fn is_identity(&self) -> bool {
        *self == Levels::default()
    }

    fn is_valid(&self) -> bool {
        let byte = |v: f64| v.is_finite() && (0.0..=255.0).contains(&v);
        [self.in_black, self.in_white, self.out_black, self.out_white].into_iter().all(byte)
            && self.in_white - self.in_black >= 2.0
            && self.out_black < self.out_white
            && self.gamma.is_finite()
            && (0.1..=9.99).contains(&self.gamma)
    }

    /// ค่าเข้า 0–1 → ค่าออก 0–1
    fn eval(&self, x: f64) -> f64 {
        if self.is_identity() {
            return x;
        }
        let v = ((x * 255.0 - self.in_black) / (self.in_white - self.in_black)).clamp(0.0, 1.0);
        (self.out_black + v.powf(1.0 / self.gamma) * (self.out_white - self.out_black)) / 255.0
    }
}

/// หนึ่งวงของ Color Grading: สี (0–360°) ความเข้มของสี (0–100) ความสว่าง (−100–100)
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Wheel {
    pub hue: f64,
    pub sat: f64,
    pub lum: f64,
}

/// Color Grading แบบ Lightroom: สามช่วงความสว่าง + ทั้งภาพ · Blending = ความกว้างที่ช่วงซ้อนกัน · Balance บวก = ให้น้ำหนักส่วนสว่าง
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Grading {
    pub shadows: Wheel,
    pub midtones: Wheel,
    pub highlights: Wheel,
    pub global: Wheel,
    pub blending: f64,
    pub balance: f64,
}

impl Default for Grading {
    fn default() -> Self {
        Grading {
            shadows: Wheel::default(),
            midtones: Wheel::default(),
            highlights: Wheel::default(),
            global: Wheel::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

impl Grading {
    pub fn is_active(&self) -> bool {
        [self.shadows, self.midtones, self.highlights, self.global].iter().any(|w| w.sat != 0.0 || w.lum != 0.0)
    }

    fn is_valid(&self) -> bool {
        let wheel = |w: &Wheel| {
            w.hue.is_finite()
                && (0.0..=360.0).contains(&w.hue)
                && w.sat.is_finite()
                && (0.0..=100.0).contains(&w.sat)
                && w.lum.is_finite()
                && w.lum.abs() <= 100.0
        };
        [self.shadows, self.midtones, self.highlights, self.global].iter().all(wheel)
            && self.blending.is_finite()
            && (0.0..=100.0).contains(&self.blending)
            && self.balance.is_finite()
            && self.balance.abs() <= 100.0
    }

    /// เติมสีตามน้ำหนักของช่วงความสว่างที่พิกเซลอยู่ (ไม่เปลี่ยนความสว่างของสีที่เติม — เปลี่ยนเฉพาะเมื่อปรับ lum)
    fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
        let l = luma(rgb);
        let lp = (l + self.balance / 100.0 * 0.25).clamp(0.0, 1.0);
        let width = 0.25 + 0.5 * self.blending / 100.0;
        let ws = smooth(1.0 - lp / width);
        let wh = smooth(1.0 - (1.0 - lp) / width);
        let wm = smooth(1.0 - (lp - 0.5).abs() / width);
        let mut out = rgb;
        for (wheel, w) in [(self.shadows, ws), (self.midtones, wm), (self.highlights, wh), (self.global, 1.0)] {
            if w <= 0.0 {
                continue;
            }
            if wheel.sat > 0.0 {
                let c = hsl_to_rgb(wheel.hue.rem_euclid(360.0), 1.0, 0.5);
                let cl = luma(c);
                for i in 0..3 {
                    out[i] += (c[i] - cl) * wheel.sat / 100.0 * 0.35 * w;
                }
            }
            if wheel.lum != 0.0 {
                for v in out.iter_mut() {
                    *v += wheel.lum / 100.0 * 0.2 * w;
                }
            }
        }
        out.map(|v| v.clamp(0.0, 1.0))
    }
}

/// Vignette หลังตัด: Amount ลบ = ขอบมืด บวก = ขอบสว่าง · Midpoint ต่ำ = เริ่มใกล้กลางภาพ · Feather = ความนุ่มของขอบ
/// · Roundness บวก = กลมขึ้น ลบ = เหลี่ยมขึ้น (0 = วงรีตามกรอบภาพ)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Vignette {
    pub amount: f64,
    pub midpoint: f64,
    pub feather: f64,
    pub roundness: f64,
}

impl Default for Vignette {
    fn default() -> Self {
        Vignette { amount: 0.0, midpoint: 50.0, feather: 50.0, roundness: 0.0 }
    }
}

impl Vignette {
    fn is_valid(&self) -> bool {
        let pct = |v: f64| v.is_finite() && (0.0..=100.0).contains(&v);
        let signed = |v: f64| v.is_finite() && v.abs() <= 100.0;
        signed(self.amount) && pct(self.midpoint) && pct(self.feather) && signed(self.roundness)
    }

    /// น้ำหนัก 0–1 ของพิกเซลที่ (x, y) — จุดกลางของพิกเซล ในภาพขนาด w × h
    pub fn weight(&self, x: f64, y: f64, w: f64, h: f64) -> f64 {
        let u = x / w * 2.0 - 1.0;
        let v = y / h * 2.0 - 1.0;
        let m = w.max(h);
        let ellipse = (u * u + v * v).sqrt() / std::f64::consts::SQRT_2;
        let r = self.roundness / 100.0;
        let d = if r >= 0.0 {
            let circle = ((u * w / m).powi(2) + (v * h / m).powi(2)).sqrt() / std::f64::consts::SQRT_2;
            ellipse + (circle - ellipse) * r
        } else {
            ellipse + (u.abs().max(v.abs()) - ellipse) * -r
        };
        let start = 0.15 + self.midpoint / 100.0 * 0.75;
        let width = 0.05 + self.feather / 100.0 * 0.95;
        smooth((d - start) / width)
    }

    fn apply(&self, rgb: [f64; 3], t: f64) -> [f64; 3] {
        let a = self.amount / 100.0 * t;
        if a < 0.0 {
            rgb.map(|v| v * (1.0 + a))
        } else {
            rgb.map(|v| v + (1.0 - v) * a)
        }
    }
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn luma([r, g, b]: [f64; 3]) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn curve_is_identity(points: &[[f64; 2]]) -> bool {
    points.iter().all(|p| (p[0] - p[1]).abs() < 1e-9)
}

impl Adjust {
    pub fn is_neutral(&self) -> bool {
        let zero = |v: &[f64]| v.iter().all(|x| *x == 0.0);
        zero(&[
            self.exposure,
            self.contrast,
            self.highlights,
            self.shadows,
            self.whites,
            self.blacks,
            self.texture,
            self.clarity,
            self.dehaze,
            self.temperature,
            self.tint,
            self.vibrance,
            self.saturation,
        ]) && zero(&self.hue)
            && zero(&self.sat)
            && zero(&self.lum)
            && zero(&self.parametric)
            && [&self.curve, &self.curve_red, &self.curve_green, &self.curve_blue].iter().all(|c| curve_is_identity(c))
            && self.levels_all().iter().all(|l| l.is_identity())
            && !self.grading.is_active()
            && self.vignette.amount == 0.0
    }

    fn levels_all(&self) -> [Levels; 4] {
        [self.levels, self.levels_red, self.levels_green, self.levels_blue]
    }

    /// มีขั้นที่ต้องคำนวณต่อพิกเซล (ใช้ตารางสามช่องอย่างเดียวไม่ได้)
    fn per_pixel(&self) -> bool {
        self.saturation != 0.0 || self.vibrance != 0.0 || self.has_hsl() || self.grading.is_active()
    }

    fn has_hsl(&self) -> bool {
        self.hue.iter().chain(&self.sat).chain(&self.lum).any(|v| *v != 0.0)
    }

    pub fn validate(&self) -> Result<()> {
        let sliders = [
            self.contrast,
            self.highlights,
            self.shadows,
            self.whites,
            self.blacks,
            self.texture,
            self.clarity,
            self.dehaze,
            self.temperature,
            self.tint,
            self.vibrance,
            self.saturation,
        ];
        let in_range = |v: &f64| v.is_finite() && v.abs() <= 100.0;
        let curves_ok = [&self.curve, &self.curve_red, &self.curve_green, &self.curve_blue].iter().all(|c| {
            c.len() <= MAX_CURVE_POINTS
                && c.iter().all(|p| p.iter().all(|v| v.is_finite() && (0.0..=255.0).contains(v)))
                && c.windows(2).all(|w| w[0][0] < w[1][0])
        });
        let splits_ok = self
            .parametric_split
            .is_none_or(|s| s.iter().all(|v| v.is_finite() && (0.0..=100.0).contains(v)) && s[0] < s[1] && s[1] < s[2]);
        let ok = self.exposure.is_finite()
            && self.exposure.abs() <= 3.0
            && sliders.iter().all(in_range)
            && self.hue.iter().chain(&self.sat).chain(&self.lum).chain(&self.parametric).all(in_range)
            && curves_ok
            && splits_ok
            && self.levels_all().iter().all(Levels::is_valid)
            && self.grading.is_valid()
            && self.vignette.is_valid();
        if ok {
            Ok(())
        } else {
            Err(AppError::invalid("ค่าแสงหรือสีอยู่นอกช่วงที่ปรับได้"))
        }
    }

    /// ตารางของขั้นที่แยกต่อช่องสีได้: [แดง, เขียว, น้ำเงิน][ค่าเข้า 0–255] → ค่าออก 0–1 (ภาพทั่วไป)
    pub fn curves(&self) -> [[f64; 256]; 3] {
        self.curves_for(Profile::Display)
    }

    /// เหมือน [`Self::curves`] ตามชนิดของภาพต้นทาง — RAW: ค่าเข้าเป็นภาพ 8 บิตของโทนตั้งต้น ถอดกลับเป็นแสงเชิงเส้นก่อน
    /// (ไม่ปรับอะไร = ได้ค่าเดิมทุกระดับ)
    pub fn curves_for(&self, profile: Profile) -> [[f64; 256]; 3] {
        let chain = self.chain();
        let decoded: [f64; 256] = std::array::from_fn(|i| match profile {
            Profile::Display => to_linear(i as f64 / 255.0),
            Profile::Raw => raw_tone_inverse(i as f64 / 255.0),
        });
        std::array::from_fn(|c| std::array::from_fn(|i| chain.eval(self, c, decoded[i], profile)))
    }

    /// ตารางของ RAW ที่ค่าเข้าเป็นแสงเชิงเส้น 0–[`crate::raw::HEADROOM`] (ภาพที่ล้างแล้ว ทศนิยม): [`LINEAR_LUT`] ค่าต่อช่อง
    /// เรียงตามรากที่สองของค่าเข้า (ละเอียดในเงา) — อ่านด้วย [`linear_lut_at`]
    pub fn linear_lut(&self) -> [Vec<f64>; 3] {
        let chain = self.chain();
        let h = f64::from(crate::raw::HEADROOM);
        std::array::from_fn(|c| {
            (0..LINEAR_LUT)
                .map(|i| {
                    let s = i as f64 / (LINEAR_LUT - 1) as f64;
                    chain.eval(self, c, s * s * h, Profile::Raw)
                })
                .collect()
        })
    }

    /// ค่าที่เตรียมไว้ของขั้นที่แยกต่อช่องได้
    fn chain(&self) -> Chain {
        Chain {
            gains: [
                1.0 + 0.25 * self.temperature / 100.0,
                1.0 - 0.2 * self.tint / 100.0,
                1.0 - 0.25 * self.temperature / 100.0,
            ],
            light: 2f64.powf(self.exposure),
            black: -0.1 * self.blacks / 100.0,
            white: 1.0 - 0.1 * self.whites / 100.0,
            master: Spline::new(&self.curve),
            channel: [Spline::new(&self.curve_red), Spline::new(&self.curve_green), Spline::new(&self.curve_blue)],
            splits: self.parametric_split.unwrap_or(DEFAULT_SPLITS).map(|v| v / 100.0),
        }
    }

    /// ขั้นต่อพิกเซล (ค่า 0–1 หลังผ่านตาราง): HSL ต่อช่วงสี แล้วความสด/ความอิ่มสี
    pub fn color(&self, rgb: [f64; 3]) -> [f64; 3] {
        let mut rgb = rgb;
        if self.has_hsl() {
            rgb = self.hsl(rgb);
        }
        if self.grading.is_active() {
            rgb = self.grading.apply(rgb);
        }
        if self.saturation == 0.0 && self.vibrance == 0.0 {
            return rgb;
        }
        let [r, g, b] = rgb;
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let sat = r.max(g).max(b) - r.min(g).min(b);
        // ความสดของสี: เพิ่มมากกับสีที่ยังจืด น้อยกับสีที่สดอยู่แล้ว
        let factor = (1.0 + self.saturation / 100.0) * (1.0 + self.vibrance / 100.0 * (1.0 - sat));
        rgb.map(|v| (luma + (v - luma) * factor).clamp(0.0, 1.0))
    }

    /// HSL: น้ำหนักของสองช่วงสีที่ hue ของพิกเซลอยู่ระหว่างกลาง · ผลแรงตามความอิ่มของพิกเซล (สีเทาไม่เปลี่ยน)
    fn hsl(&self, [r, g, b]: [f64; 3]) -> [f64; 3] {
        let (h, s, l) = rgb_to_hsl(r, g, b);
        if s <= 1e-6 {
            return [r, g, b];
        }
        let (k, t) = hue_segment(h);
        let k1 = (k + 1) % 8;
        let blend = |v: &[f64; 8]| (v[k] * (1.0 - t) + v[k1] * t) / 100.0;
        let strength = (s * 1.5).min(1.0);
        let h2 = (h + blend(&self.hue) * 30.0 * strength).rem_euclid(360.0);
        let s2 = (s * (1.0 + blend(&self.sat) * strength)).clamp(0.0, 1.0);
        let l2 = (l + blend(&self.lum) * 0.25 * strength).clamp(0.0, 1.0);
        hsl_to_rgb(h2, s2, l2)
    }

    /// ค่าออก 0–255 ของพิกเซลหนึ่ง (ใช้ทั้งภาพจริงและเทสต์เทียบกับ UI)
    pub fn pixel(&self, curves: &[[f64; 256]; 3], rgb: [u8; 3]) -> [u8; 3] {
        self.pixel_at(curves, rgb, 0.0)
    }

    /// เหมือน [`Self::pixel`] พร้อมน้ำหนัก Vignette ของตำแหน่งพิกเซล (0 = ไม่มีผล)
    pub fn pixel_at(&self, curves: &[[f64; 256]; 3], rgb: [u8; 3], vignette: f64) -> [u8; 3] {
        let mapped = [curves[0][rgb[0] as usize], curves[1][rgb[1] as usize], curves[2][rgb[2] as usize]];
        let mut out = self.color(mapped);
        if vignette > 0.0 {
            out = self.vignette.apply(out, vignette);
        }
        out.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
}

/// ชนิดของภาพต้นทางของการแต่ง
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    /// ภาพทั่วไป (JPEG/PNG): ค่า sRGB ที่กล้องหรือโปรแกรมอื่นทำโทนมาแล้ว
    #[default]
    Display,
    /// RAW: แสงเชิงเส้นจากเซนเซอร์ — ค่าแสงและสมดุลแสงขาวทำก่อนเส้นโค้งโทนตั้งต้น [`raw_tone`] (แบบโปรแกรมแต่ง RAW)
    /// ส่วนสว่างจึงม้วนเข้าหาสีขาวแทนการตัดเมื่อเพิ่มค่าแสง
    Raw,
}

/// ขั้นที่แยกต่อช่องได้ (ค่าที่เตรียมไว้)
struct Chain {
    gains: [f64; 3],
    light: f64,
    black: f64,
    white: f64,
    master: Spline,
    channel: [Spline; 3],
    splits: [f64; 3],
}

impl Chain {
    /// แสงเชิงเส้นของช่อง `c` → ค่าออก 0–1
    fn eval(&self, a: &Adjust, c: usize, linear: f64, profile: Profile) -> f64 {
        // สมดุลแสงขาวและค่าแสงทำในแสงเชิงเส้น
        let linear = linear * self.gains[c] * self.light;
        let mut x = match profile {
            Profile::Display => to_srgb(linear.clamp(0.0, 1.0)),
            Profile::Raw => raw_tone(linear),
        };
        x = ((x - self.black) / (self.white - self.black)).clamp(0.0, 1.0);
        // เงา/ส่วนสว่าง: ยกหรือกดช่วงมืด/สว่าง ปลายสองข้างไม่ขยับ (น้ำหนักสูงสุดที่ 1/3 และ 2/3)
        x += 0.25 * a.shadows / 100.0 * 6.75 * x * (1.0 - x) * (1.0 - x);
        x += 0.25 * a.highlights / 100.0 * 6.75 * x * x * (1.0 - x);
        x = x.clamp(0.0, 1.0);
        let k = a.contrast / 100.0;
        x = if k >= 0.0 { x + k * (x * x * (3.0 - 2.0 * x) - x) } else { 0.5 + (x - 0.5) * (1.0 + 0.5 * k) };
        x = parametric(x.clamp(0.0, 1.0), &a.parametric, &self.splits);
        x = a.levels.eval(x);
        x = self.master.eval(x);
        x = [a.levels_red, a.levels_green, a.levels_blue][c].eval(x);
        x = self.channel[c].eval(x);
        x.clamp(0.0, 1.0)
    }
}

/// เส้นโค้งโทนตั้งต้นของ RAW: (log2 ของแสงเชิงเส้นเทียบระดับขาวของเซนเซอร์, ค่าที่แสดงแบบ sRGB)
/// วัดจากภาพ JPEG ที่กล้องฝังไว้ในไฟล์ RAW จริง (Sony ILCE-7CM2 30 ไฟล์ + Panasonic DC-S5 49 ไฟล์ — ค่ากลางของสองกล้อง
/// · TEST_REPORT หมวด RAW) RAW จึงเปิดมาได้โทนใกล้ภาพที่กล้องแสดง · เหนือระดับขาว 1 stop (ถึงได้เมื่อเพิ่มค่าแสง) ค่อย ๆ ขาวสุด
/// สูตรเดียวกับ `RAW_TONE` ใน adjust.ts
pub const RAW_TONE: [[f64; 2]; 15] = [
    [-10.0, 0.012],
    [-9.0, 0.018],
    [-8.0, 0.028],
    [-7.0, 0.054],
    [-6.0, 0.107],
    [-5.0, 0.210],
    [-4.0, 0.356],
    [-3.0, 0.558],
    [-2.5, 0.660],
    [-2.0, 0.756],
    [-1.5, 0.842],
    [-1.0, 0.909],
    [-0.5, 0.958],
    [0.0, 0.993],
    [1.0, 1.0],
];

fn raw_tone_spline() -> &'static Spline {
    static SPLINE: std::sync::OnceLock<Spline> = std::sync::OnceLock::new();
    SPLINE.get_or_init(|| {
        Spline::from_xy(RAW_TONE.iter().map(|p| p[0]).collect(), RAW_TONE.iter().map(|p| p[1]).collect())
    })
}

/// โทนตั้งต้นของ RAW: แสงเชิงเส้น (1 = ระดับขาวของเซนเซอร์) → ค่าที่แสดง 0–1 · ต่ำกว่า 2⁻¹⁰ ลดลงเป็นเส้นตรงถึงดำ
pub fn raw_tone(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return 0.0;
    }
    let l = x.log2();
    if l <= RAW_TONE[0][0] {
        RAW_TONE[0][1] * x / RAW_TONE[0][0].exp2()
    } else if l >= RAW_TONE[RAW_TONE.len() - 1][0] {
        1.0
    } else {
        raw_tone_spline().eval(l)
    }
}

/// ย้อน [`raw_tone`]: ค่าที่แสดง → แสงเชิงเส้น (ค้นแบบแบ่งครึ่ง — เส้นโค้งเพิ่มตลอด) · 1 = ขาวสุดของช่วงที่เก็บไว้
pub fn raw_tone_inverse(v: f64) -> f64 {
    let h = f64::from(crate::raw::HEADROOM);
    if v <= 0.0 {
        return 0.0;
    }
    if v >= 1.0 {
        return h;
    }
    let (mut lo, mut hi) = (0.0, h);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if raw_tone(mid) < v {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// จำนวนค่าต่อช่องของ [`Adjust::linear_lut`]
pub const LINEAR_LUT: usize = 4096;

/// อ่านตารางจาก [`Adjust::linear_lut`] ที่แสงเชิงเส้น `x` (ประมาณค่าระหว่างช่อง)
pub fn linear_lut_at(table: &[f64], x: f32) -> f64 {
    let s = (x / crate::raw::HEADROOM).clamp(0.0, 1.0).sqrt() as f64 * (LINEAR_LUT - 1) as f64;
    let i = (s as usize).min(LINEAR_LUT - 2);
    let t = s - i as f64;
    table[i] * (1.0 - t) + table[i + 1] * t
}

/// ปรับแสงและสีภาพ RAW ที่ล้างแล้ว (แสงเชิงเส้น ตั้งตรง ตัด/หมุนแล้ว) → ภาพ 8 บิต — โทนตั้งต้นของ RAW อยู่ในขั้นนี้เสมอ
/// (ไม่ได้แต่งก็ต้องผ่าน)
pub fn apply_linear(img: &image::Rgb32FImage, a: &Adjust) -> image::RgbImage {
    apply_linear_in_frame(img, a, img.width().max(img.height()) as f64)
}

/// เหมือน [`apply_linear`] สำหรับภาพที่ตัดแล้ว: `long` = ด้านยาวของกรอบก่อนตัด เป็นพิกเซลของภาพนี้
/// ([`crate::edit::Geometry::frame_long`]) — รัศมีของ Texture/Clarity/Dehaze คิดจากทั้งกรอบแบบตัวอย่างสด ตัดภาพแล้วผลไม่เปลี่ยน
pub fn apply_linear_in_frame(img: &image::Rgb32FImage, a: &Adjust, long: f64) -> image::RgbImage {
    let lut = a.linear_lut();
    let vignette = a.vignette.amount != 0.0;
    let per_pixel = a.per_pixel();
    let (w, h) = (img.width() as f64, img.height() as f64);
    let width = img.width() as usize;
    let mut out = image::RgbImage::new(img.width(), img.height());
    for (i, (src, dst)) in img.pixels().zip(out.pixels_mut()).enumerate() {
        let mapped =
            [linear_lut_at(&lut[0], src.0[0]), linear_lut_at(&lut[1], src.0[1]), linear_lut_at(&lut[2], src.0[2])];
        let mut v = if per_pixel { a.color(mapped) } else { mapped };
        if vignette {
            let t = a.vignette.weight((i % width) as f64 + 0.5, (i / width) as f64 + 0.5, w, h);
            v = a.vignette.apply(v, t);
        }
        dst.0 = v.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    if crate::local::is_active(a) {
        let (w, h) = (out.width() as usize, out.height() as usize);
        crate::local::apply(&mut out, w, h, 3, long, a);
    }
    out
}

/// Parametric curve: สี่ช่วงตามจุดแบ่ง ยก/กดแต่ละช่วงด้วยน้ำหนักรูประฆัง ปลายสองข้างไม่ขยับ
fn parametric(x: f64, amounts: &[f64; 4], splits: &[f64; 3]) -> f64 {
    if amounts.iter().all(|a| *a == 0.0) {
        return x;
    }
    let edges = [0.0, splits[0], splits[1], splits[2], 1.0];
    let pin = (8.0 * x * (1.0 - x)).min(1.0);
    let mut out = x;
    for (i, amount) in amounts.iter().enumerate() {
        let (a, b) = (edges[i], edges[i + 1]);
        let center = (a + b) / 2.0;
        let width = ((b - a) * 0.75).max(0.02);
        let weight = (-((x - center) / width).powi(2)).exp();
        out += amount / 100.0 * 0.12 * weight * pin;
    }
    out.clamp(0.0, 1.0)
}

/// ช่วงสีที่ hue อยู่ระหว่าง: (ช่วงซ้าย, ระยะไปช่วงขวา 0–1) วนรอบ 360°
fn hue_segment(h: f64) -> (usize, f64) {
    for k in 0..8 {
        let a = HUE_CENTERS[k];
        let b = if k == 7 { 360.0 } else { HUE_CENTERS[k + 1] };
        if h >= a && h < b {
            return (k, (h - a) / (b - a));
        }
    }
    (0, 0.0)
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= 1e-12 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

pub(crate) fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [(r + m).clamp(0.0, 1.0), (g + m).clamp(0.0, 1.0), (b + m).clamp(0.0, 1.0)]
}

/// เส้นโค้งผ่านจุดแบบ monotone cubic (Fritsch–Carlson): ไม่แกว่งเกินจุด — ค่านอกช่วงจุดแรก/สุดท้ายเท่าจุดปลาย
struct Spline {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ms: Vec<f64>,
}

impl Spline {
    /// จุดของเส้นโค้งโทนในหน่วย 0–255 · เส้นตรงหรือน้อยกว่าสองจุด = ไม่ปรับ
    fn new(points: &[[f64; 2]]) -> Spline {
        if points.len() < 2 || curve_is_identity(points) {
            return Spline { xs: vec![], ys: vec![], ms: vec![] };
        }
        Spline::from_xy(points.iter().map(|p| p[0] / 255.0).collect(), points.iter().map(|p| p[1] / 255.0).collect())
    }

    /// จุดใดก็ได้ (ค่า x เพิ่มขึ้น) อย่างน้อยสองจุด
    fn from_xy(xs: Vec<f64>, ys: Vec<f64>) -> Spline {
        let n = xs.len();
        let d: Vec<f64> = (0..n - 1).map(|k| (ys[k + 1] - ys[k]) / (xs[k + 1] - xs[k])).collect();
        let mut ms = vec![0.0; n];
        ms[0] = d[0];
        ms[n - 1] = d[n - 2];
        for k in 1..n - 1 {
            ms[k] = if d[k - 1] * d[k] <= 0.0 { 0.0 } else { (d[k - 1] + d[k]) / 2.0 };
        }
        for k in 0..n - 1 {
            if d[k] == 0.0 {
                ms[k] = 0.0;
                ms[k + 1] = 0.0;
                continue;
            }
            let (a, b) = (ms[k] / d[k], ms[k + 1] / d[k]);
            let len = a * a + b * b;
            if len > 9.0 {
                let tau = 3.0 / len.sqrt();
                ms[k] = tau * a * d[k];
                ms[k + 1] = tau * b * d[k];
            }
        }
        Spline { xs, ys, ms }
    }

    fn eval(&self, x: f64) -> f64 {
        let n = self.xs.len();
        if n == 0 {
            return x;
        }
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let k = (0..n - 1).find(|&k| x < self.xs[k + 1]).unwrap_or(n - 2);
        let h = self.xs[k + 1] - self.xs[k];
        let t = (x - self.xs[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.ys[k]
            + (t3 - 2.0 * t2 + t) * h * self.ms[k]
            + (-2.0 * t3 + 3.0 * t2) * self.ys[k + 1]
            + (t3 - t2) * h * self.ms[k + 1]
    }
}

fn to_linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// ปรับแสงและสีทั้งภาพ (ภาพทั่วไป)
pub fn apply(img: DynamicImage, a: &Adjust) -> DynamicImage {
    apply_profile(img, a, Profile::Display)
}

/// ปรับแสงและสีภาพ 8 บิตตามชนิดของภาพต้นทาง — RAW: ภาพ 8 บิตของโทนตั้งต้น (ภาพย่อหรือภาพของตัวดูภาพ) แบบเดียวกับตัวอย่างสด
/// ในหน้าแต่งภาพ · ไม่ได้แต่ง = ภาพเดิมทั้งสองชนิด
pub fn apply_profile(img: DynamicImage, a: &Adjust, profile: Profile) -> DynamicImage {
    let long = img.width().max(img.height()) as f64;
    apply_profile_in_frame(img, a, profile, long)
}

/// เหมือน [`apply_profile`] สำหรับภาพที่ตัดแล้ว: `long` = ด้านยาวของกรอบก่อนตัด เป็นพิกเซลของภาพนี้
/// ([`crate::edit::Geometry::frame_long`]) — รัศมีของ Texture/Clarity/Dehaze คิดจากทั้งกรอบแบบตัวอย่างสด ตัดภาพแล้วผลไม่เปลี่ยน
pub fn apply_profile_in_frame(img: DynamicImage, a: &Adjust, profile: Profile, long: f64) -> DynamicImage {
    if a.is_neutral() {
        return img;
    }
    let curves = a.curves_for(profile);
    let vignette = a.vignette.amount != 0.0;
    let only_curves = !a.per_pixel() && !vignette;
    let (w, h) = (img.width() as f64, img.height() as f64);
    let width = img.width() as usize;
    let height = img.height() as usize;
    // ตารางสามช่องที่ปัดเป็น 0–255 แล้ว: กรณีไม่ปรับสีต่อพิกเซล ใช้ตารางตรง ๆ เร็วที่สุด
    let table: [[u8; 256]; 3] = curves.map(|t| t.map(|v| (v * 255.0).round() as u8));
    let alpha = img.color().has_alpha();
    let mut rgba = if alpha { img.to_rgba8() } else { image::RgbaImage::new(0, 0) };
    let mut rgb = if alpha { image::RgbImage::new(0, 0) } else { img.into_rgb8() };
    let n = if alpha { 4 } else { 3 };
    let raw: &mut [u8] = if alpha { &mut rgba } else { &mut rgb };
    for (i, px) in raw.chunks_exact_mut(n).enumerate() {
        let v = [px[0], px[1], px[2]];
        let out = if only_curves {
            [table[0][v[0] as usize], table[1][v[1] as usize], table[2][v[2] as usize]]
        } else {
            let t = if vignette {
                a.vignette.weight((i % width) as f64 + 0.5, (i / width) as f64 + 0.5, w, h)
            } else {
                0.0
            };
            a.pixel_at(&curves, v, t)
        };
        px[..3].copy_from_slice(&out);
    }
    if crate::local::is_active(a) {
        crate::local::apply(raw, width, height, n, long, a);
    }
    if alpha {
        DynamicImage::ImageRgba8(rgba)
    } else {
        DynamicImage::ImageRgb8(rgb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ค่าเดียวกับ `adjust.test.ts` ฝั่ง UI
    pub(crate) fn sample() -> Adjust {
        Adjust {
            exposure: 0.5,
            contrast: 30.0,
            highlights: -40.0,
            shadows: 50.0,
            whites: 10.0,
            blacks: -10.0,
            temperature: 20.0,
            tint: -10.0,
            vibrance: 25.0,
            saturation: 10.0,
            ..Default::default()
        }
    }

    /// แบบ preset "more white skin" ของ Lightroom: HSL + เส้นโค้ง + parametric
    pub(crate) fn sample_full() -> Adjust {
        let mut a = sample();
        a.hue = [0.0, 3.0, -1.0, 7.0, 0.0, 5.0, 0.0, 0.0];
        a.sat = [32.0, -3.0, -13.0, -33.0, 15.0, -9.0, 0.0, 0.0];
        a.lum = [20.0, 42.0, 2.0, 11.0, 25.0, 0.0, 5.0, 6.0];
        a.parametric = [-5.0, 4.0, 12.0, 0.0];
        a.curve = vec![[2.0, 11.0], [60.0, 60.0], [138.0, 145.0], [255.0, 255.0]];
        a
    }

    /// sample_full + Levels + Color Grading + Vignette (ใส่ค่าเดียวกันใน adjust.test.ts)
    pub(crate) fn sample_pro() -> Adjust {
        let mut a = sample_full();
        a.levels = Levels { in_black: 10.0, in_white: 240.0, gamma: 1.3, out_black: 5.0, out_white: 250.0 };
        a.levels_blue = Levels { in_black: 0.0, in_white: 230.0, gamma: 0.9, out_black: 0.0, out_white: 255.0 };
        a.grading = Grading {
            shadows: Wheel { hue: 220.0, sat: 30.0, lum: -10.0 },
            midtones: Wheel { hue: 35.0, sat: 15.0, lum: 0.0 },
            highlights: Wheel { hue: 45.0, sat: 25.0, lum: 5.0 },
            global: Wheel { hue: 0.0, sat: 0.0, lum: 0.0 },
            blending: 60.0,
            balance: -20.0,
        };
        a.vignette = Vignette { amount: -40.0, midpoint: 40.0, feather: 60.0, roundness: 20.0 };
        a
    }

    /// ค่าอ้างอิงของ `sample_pro` ที่สามตำแหน่งในภาพ 400×300 (ใส่ค่าเดียวกันใน adjust.test.ts)
    /// (สีเข้า, ตำแหน่งพิกเซล, สีออก)
    type ProCase = ([u8; 3], (f64, f64), [u8; 3]);
    const PRO_EXPECTED: [ProCase; 3] = [
        ([10, 20, 30], (200.5, 150.5), [9, 46, 82]),
        ([200, 130, 80], (20.5, 15.5), [177, 147, 107]),
        ([60, 140, 90], (390.5, 150.5), [99, 176, 121]),
    ];

    #[test]
    fn levels_grading_and_vignette_match_the_ui_formula() {
        let a = sample_pro();
        let c = a.curves();
        for (rgb, (x, y), want) in PRO_EXPECTED {
            assert_eq!(a.pixel_at(&c, rgb, a.vignette.weight(x, y, 400.0, 300.0)), want, "{rgb:?} at ({x}, {y})");
        }
        assert!(a.validate().is_ok());
    }

    #[test]
    fn levels_grading_and_vignette_do_what_their_names_say() {
        let px = |a: &Adjust, v: [u8; 3]| a.pixel(&a.curves(), v);
        // Levels: จุดดำเข้า 20 → ค่า 20 กลายเป็นดำ · แกมมา > 1 ยกช่วงกลาง · ค่าออกดำ 30 = ยกจุดดำ
        let mut l = Adjust::default();
        l.levels.in_black = 20.0;
        assert_eq!(px(&l, [20, 20, 20]), [0, 0, 0]);
        l.levels = Levels { gamma: 2.0, ..Levels::default() };
        assert!(px(&l, [100, 100, 100])[0] > 140);
        l.levels = Levels { out_black: 30.0, ..Levels::default() };
        assert_eq!(px(&l, [0, 0, 0]), [30, 30, 30]);
        // แยกช่อง: น้ำเงินอย่างเดียว
        let blue = Adjust { levels_blue: Levels { out_black: 40.0, ..Levels::default() }, ..Default::default() };
        assert_eq!(px(&blue, [0, 0, 0]), [0, 0, 40]);
        // Color Grading: เงาสีน้ำเงิน → พิกเซลมืดเป็นสีน้ำเงินขึ้น · พิกเซลสว่างแทบไม่เปลี่ยน
        let mut g = Adjust::default();
        g.grading.shadows = Wheel { hue: 230.0, sat: 60.0, lum: 0.0 };
        let dark = px(&g, [40, 40, 40]);
        assert!(dark[2] > dark[0] + 10, "{dark:?}");
        let bright = px(&g, [235, 235, 235]);
        assert!(bright.iter().all(|v| v.abs_diff(235) <= 2), "{bright:?}");
        // Vignette: กลางภาพไม่เปลี่ยน มุมภาพมืดลง
        let v = Vignette { amount: -100.0, ..Vignette::default() };
        assert_eq!(v.weight(200.0, 150.0, 400.0, 300.0), 0.0);
        assert!(v.weight(0.5, 0.5, 400.0, 300.0) > 0.95);
        // ค่าที่ไม่สมเหตุสมผลถูกปฏิเสธ
        let bad =
            Adjust { levels: Levels { in_black: 200.0, in_white: 100.0, ..Levels::default() }, ..Default::default() };
        assert!(bad.validate().is_err());
        let bad = Adjust { vignette: Vignette { midpoint: 150.0, ..Vignette::default() }, ..Default::default() };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn neutral_changes_nothing() {
        let a = Adjust::default();
        let curves = a.curves();
        for v in [0u8, 1, 64, 128, 200, 255] {
            assert_eq!(a.pixel(&curves, [v, v / 2, 255 - v]), [v, v / 2, 255 - v]);
        }
        // เส้นโค้งที่เป็นเส้นตรง = ไม่ปรับ
        let straight = Adjust { curve: vec![[0.0, 0.0], [255.0, 255.0]], ..Default::default() };
        assert!(straight.is_neutral());
    }

    #[test]
    fn each_slider_moves_the_image_the_way_its_name_says() {
        let px = |a: Adjust, v: [u8; 3]| a.pixel(&a.curves(), v);
        let grey = [100u8, 100, 100];
        assert!(px(Adjust { exposure: 1.0, ..Default::default() }, grey)[0] > 120, "ค่าแสง +1 สว่างขึ้น");
        assert!(px(Adjust { shadows: 100.0, ..Default::default() }, [40, 40, 40])[0] > 50, "ยกเงา");
        assert!(px(Adjust { highlights: -100.0, ..Default::default() }, [215, 215, 215])[0] < 205, "กดส่วนสว่าง");
        let warm = px(Adjust { temperature: 100.0, ..Default::default() }, grey);
        assert!(warm[0] > warm[2], "อุ่น = แดงมากกว่าน้ำเงิน");
        let contrast = Adjust { contrast: 100.0, ..Default::default() };
        assert!(px(contrast.clone(), [60, 60, 60])[0] < 60 && px(contrast, [190, 190, 190])[0] > 190);
        assert_eq!(
            px(Adjust { saturation: -100.0, ..Default::default() }, [200, 80, 40]),
            [103, 103, 103],
            "ขาวดำ = ความสว่างเดิม"
        );
        // HSL: ความสว่างของสีส้ม (ผิว) +100 → สีส้มสว่างขึ้น สีเทาและสีน้ำเงินไม่เปลี่ยน
        let mut skin = Adjust::default();
        skin.lum[1] = 100.0;
        let orange = [200u8, 130, 80];
        let lit = px(skin.clone(), orange);
        assert!(lit[0] >= orange[0] && lit[1] > orange[1] && lit[2] > orange[2], "{lit:?}");
        assert_eq!(px(skin.clone(), grey), grey);
        assert_eq!(px(skin, [40, 60, 200]), [40, 60, 200]);
        // เส้นโค้งยกจุดดำ: ค่าเข้า 0 ออกเท่าจุดแรกของเส้น
        let lifted = Adjust { curve: vec![[0.0, 30.0], [255.0, 255.0]], ..Default::default() };
        assert_eq!(px(lifted, [0, 0, 0]), [30, 30, 30]);
    }

    #[test]
    fn matches_the_ui_formula() {
        // ผลชุดนี้คัดลอกไปไว้ใน adjust.test.ts — แก้สูตรฝั่งไหนต้องแก้ทั้งสองฝั่ง
        let a = sample();
        let curves = a.curves();
        assert_eq!(a.pixel(&curves, [10, 20, 30]), [11, 30, 47]);
        assert_eq!(a.pixel(&curves, [120, 90, 200]), [148, 108, 251]);
        assert_eq!(a.pixel(&curves, [240, 230, 220]), [255, 255, 252]);
        let full = sample_full();
        let curves = full.curves();
        let got: Vec<[u8; 3]> =
            [[10, 20, 30], [200, 130, 80], [60, 140, 90]].iter().map(|v| full.pixel(&curves, *v)).collect();
        assert_eq!(got, FULL_EXPECTED.to_vec());
    }

    /// ค่าอ้างอิงของ `sample_full` (ใส่ค่าเดียวกันใน adjust.test.ts)
    const FULL_EXPECTED: [[u8; 3]; 3] = [[20, 37, 53], [255, 190, 135], [77, 179, 116]];

    /// ชุดที่เทสต์สุ่ม (`tests/preview_parity.rs` ชุด 765, 2067, 5995) จับได้ว่าตัวอย่างสดต่างจากไฟล์: การวน hue ด้วย
    /// `((x % m) + m) % m` ของ JavaScript ปัดเศษเพิ่ม และ hue ที่ตกที่ 360° พอดี — ค่าเดียวกันใน adjust.test.ts
    /// ([hue, sat, lum] ทั้ง 8 ช่วง, พิกเซลเข้า, พิกเซลออก)
    type HslCase = ([[f64; 8]; 3], [u8; 3], [u8; 3]);
    const HSL_EXPECTED: [HslCase; 3] = [
        (
            [
                [100.0, 0.0, 0.0, 100.0, 100.0, -73.0, 0.0, -100.0],
                [72.0, 100.0, 0.0, 100.0, 0.0, 0.0, 67.0, 0.0],
                [0.0, 0.0, 18.0, -100.0, 65.0, 0.0, 0.0, -100.0],
            ],
            [218, 47, 41],
            [255, 129, 4],
        ),
        (
            [
                [0.0, 92.089, -100.0, 100.0, -89.0, -100.0, -100.0, 26.0],
                [-93.0, 0.0, 0.0, 100.0, -18.0, 72.0, -64.0, 100.0],
                [0.0, 0.0, -22.0, -100.0, -100.0, 0.0, 100.0, 0.0],
            ],
            [204, 21, 143],
            [225, 0, 131],
        ),
        (
            [
                [-100.0, 0.0, -100.0, 100.0, 0.0, 100.0, -64.0, -44.0],
                [-48.0, 0.0, 0.0, -21.0, 13.0, 0.0, 0.0, -87.0],
                [0.0, 39.0, 0.0, 0.0, -83.0, 0.0, 44.0, -8.0],
            ],
            [246, 117, 74],
            [229, 116, 116],
        ),
    ];

    #[test]
    fn hue_wraps_around_360_exactly_like_the_live_preview() {
        for ([hue, sat, lum], input, want) in HSL_EXPECTED {
            let a = Adjust { hue, sat, lum, ..Default::default() };
            assert_eq!(a.pixel(&a.curves(), input), want, "{input:?}");
        }
    }

    #[test]
    fn raw_tone_matches_the_camera_curve_and_round_trips() {
        assert_eq!(raw_tone(0.0), 0.0);
        assert!((raw_tone(1.0) - 0.993).abs() < 1e-9 && raw_tone(2.0) == 1.0);
        assert!((raw_tone(0.125) - 0.558).abs() < 1e-9, "เทากลาง 3 stop ใต้ระดับขาว");
        let mut last = -1.0;
        for i in 0..=2000 {
            let v = raw_tone(i as f64 / 1000.0);
            assert!(v >= last, "เพิ่มตลอด");
            last = v;
        }
        // ไม่ปรับอะไร = ภาพ 8 บิตของโทนตั้งต้นเหมือนเดิมทุกระดับ (ถอดกลับเป็นแสงเชิงเส้นแล้วทำโทนซ้ำ)
        let c = Adjust::default().curves_for(Profile::Raw);
        for (i, v) in c[1].iter().enumerate() {
            assert_eq!((v * 255.0).round() as usize, i);
        }
    }

    #[test]
    fn raw_exposure_works_on_scene_light_so_highlights_roll_off() {
        let up = Adjust { exposure: 1.0, ..Default::default() };
        let (display, raw) = (up.curves(), up.curves_for(Profile::Raw));
        let level = |t: &[f64; 256], i: usize| (t[i] * 255.0).round() as u8;
        // ภาพทั่วไป: ค่าแสง +1 ทำให้ส่วนที่สว่างกว่า ~190 ขาวสุดหมด · RAW: ยังไล่น้ำหนักได้
        assert_eq!(level(&display[1], 200), 255);
        assert!(
            level(&raw[1], 200) < 254 && level(&raw[1], 230) < 255,
            "{} {}",
            level(&raw[1], 200),
            level(&raw[1], 230)
        );
        assert!(level(&raw[1], 120) > 150, "ช่วงกลางสว่างขึ้นราวหนึ่ง stop");
        // ภาพที่ล้างแล้ว (ทศนิยม) กับภาพ 8 บิตของโทนตั้งต้นให้ผลใกล้กันเมื่อแต่งชุดเดียวกัน
        let a = sample();
        let lut = a.linear_lut();
        let eight = a.curves_for(Profile::Raw);
        for x in [0.003f32, 0.02, 0.09, 0.3, 0.8] {
            let t = (raw_tone(x as f64) * 255.0).round() as usize;
            let from_linear = linear_lut_at(&lut[1], x);
            assert!((from_linear - eight[1][t]).abs() * 255.0 <= 2.5, "x {x}: {from_linear} vs {}", eight[1][t]);
        }
    }

    /// ค่าอ้างอิงของ `sample` ด้วยสูตรของ RAW (ใส่ค่าเดียวกันใน adjust.test.ts)
    const RAW_EXPECTED: [[u8; 3]; 3] = [[11, 35, 53], [154, 117, 236], [255, 244, 228]];

    #[test]
    fn raw_profile_matches_the_ui_formula() {
        let a = sample();
        let c = a.curves_for(Profile::Raw);
        let got: Vec<[u8; 3]> =
            [[10, 20, 30], [120, 90, 200], [240, 230, 220]].iter().map(|v| a.pixel(&c, *v)).collect();
        assert_eq!(got, RAW_EXPECTED.to_vec());
    }
}
