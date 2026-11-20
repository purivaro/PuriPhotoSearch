//! แสงและสีของการแต่งภาพ (DECISIONS ข้อ 18, 20) — สูตรเดียวกับ `adjust.ts` ฝั่ง UI (เทสต์ค่าเดียวกันทั้งสองฝั่ง)
//!
//! ขั้นที่แยกต่อช่องสีได้รวมเป็นตาราง 256 ค่าต่อช่อง: สมดุลแสงขาว ค่าแสง จุดดำ/จุดขาว ส่วนสว่าง/เงา คอนทราสต์
//! → Parametric curve → เส้นโค้งโทนแบบจุด (รวม แล้วแยกช่อง) · จากนั้นต่อพิกเซล: HSL ต่อช่วงสี → ความสด/ความอิ่มสี
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
            self.temperature,
            self.tint,
            self.vibrance,
            self.saturation,
        ]) && zero(&self.hue)
            && zero(&self.sat)
            && zero(&self.lum)
            && zero(&self.parametric)
            && [&self.curve, &self.curve_red, &self.curve_green, &self.curve_blue].iter().all(|c| curve_is_identity(c))
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
            && splits_ok;
        if ok {
            Ok(())
        } else {
            Err(AppError::invalid("ค่าแสงหรือสีอยู่นอกช่วงที่ปรับได้"))
        }
    }

    /// ตารางของขั้นที่แยกต่อช่องสีได้: [แดง, เขียว, น้ำเงิน][ค่าเข้า 0–255] → ค่าออก 0–1
    pub fn curves(&self) -> [[f64; 256]; 3] {
        let gains = [
            1.0 + 0.25 * self.temperature / 100.0,
            1.0 - 0.2 * self.tint / 100.0,
            1.0 - 0.25 * self.temperature / 100.0,
        ];
        let light = 2f64.powf(self.exposure);
        let black = -0.1 * self.blacks / 100.0;
        let white = 1.0 - 0.1 * self.whites / 100.0;
        let master = Spline::new(&self.curve);
        let channel = [Spline::new(&self.curve_red), Spline::new(&self.curve_green), Spline::new(&self.curve_blue)];
        let splits = self.parametric_split.unwrap_or(DEFAULT_SPLITS).map(|v| v / 100.0);
        let mut out = [[0.0; 256]; 3];
        for (c, table) in out.iter_mut().enumerate() {
            for (i, value) in table.iter_mut().enumerate() {
                // สมดุลแสงขาวและค่าแสงทำในแสงเชิงเส้น
                let linear = to_linear(i as f64 / 255.0) * gains[c] * light;
                let mut x = to_srgb(linear.clamp(0.0, 1.0));
                x = ((x - black) / (white - black)).clamp(0.0, 1.0);
                // เงา/ส่วนสว่าง: ยกหรือกดช่วงมืด/สว่าง ปลายสองข้างไม่ขยับ (น้ำหนักสูงสุดที่ 1/3 และ 2/3)
                x += 0.25 * self.shadows / 100.0 * 6.75 * x * (1.0 - x) * (1.0 - x);
                x += 0.25 * self.highlights / 100.0 * 6.75 * x * x * (1.0 - x);
                x = x.clamp(0.0, 1.0);
                let k = self.contrast / 100.0;
                x = if k >= 0.0 { x + k * (x * x * (3.0 - 2.0 * x) - x) } else { 0.5 + (x - 0.5) * (1.0 + 0.5 * k) };
                x = parametric(x.clamp(0.0, 1.0), &self.parametric, &splits);
                x = master.eval(x);
                x = channel[c].eval(x);
                *value = x.clamp(0.0, 1.0);
            }
        }
        out
    }

    /// ขั้นต่อพิกเซล (ค่า 0–1 หลังผ่านตาราง): HSL ต่อช่วงสี แล้วความสด/ความอิ่มสี
    pub fn color(&self, rgb: [f64; 3]) -> [f64; 3] {
        let mut rgb = rgb;
        if self.has_hsl() {
            rgb = self.hsl(rgb);
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
        let mapped = [curves[0][rgb[0] as usize], curves[1][rgb[1] as usize], curves[2][rgb[2] as usize]];
        self.color(mapped).map(|v| (v * 255.0).round() as u8)
    }
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

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
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
    fn new(points: &[[f64; 2]]) -> Spline {
        if points.len() < 2 || curve_is_identity(points) {
            return Spline { xs: vec![], ys: vec![], ms: vec![] };
        }
        let xs: Vec<f64> = points.iter().map(|p| p[0] / 255.0).collect();
        let ys: Vec<f64> = points.iter().map(|p| p[1] / 255.0).collect();
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

/// ปรับแสงและสีทั้งภาพ
pub fn apply(img: DynamicImage, a: &Adjust) -> DynamicImage {
    if a.is_neutral() {
        return img;
    }
    let curves = a.curves();
    let only_curves = a.saturation == 0.0 && a.vibrance == 0.0 && !a.has_hsl();
    // ตารางสามช่องที่ปัดเป็น 0–255 แล้ว: กรณีไม่ปรับสีต่อพิกเซล ใช้ตารางตรง ๆ เร็วที่สุด
    let table: [[u8; 256]; 3] = curves.map(|t| t.map(|v| (v * 255.0).round() as u8));
    let alpha = img.color().has_alpha();
    let mut rgba = if alpha { img.to_rgba8() } else { image::RgbaImage::new(0, 0) };
    let mut rgb = if alpha { image::RgbImage::new(0, 0) } else { img.into_rgb8() };
    let n = if alpha { 4 } else { 3 };
    let raw: &mut [u8] = if alpha { &mut rgba } else { &mut rgb };
    for px in raw.chunks_exact_mut(n) {
        let v = [px[0], px[1], px[2]];
        let out = if only_curves {
            [table[0][v[0] as usize], table[1][v[1] as usize], table[2][v[2] as usize]]
        } else {
            a.pixel(&curves, v)
        };
        px[..3].copy_from_slice(&out);
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
}
