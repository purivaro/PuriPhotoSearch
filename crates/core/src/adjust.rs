//! แสงและสีของการแต่งภาพ (DECISIONS ข้อ 18) — สูตรเดียวกับ `adjust.ts` ฝั่ง UI (เทสต์ค่าเดียวกันทั้งสองฝั่ง)
//!
//! ขั้นที่แยกต่อช่องสีได้ (สมดุลแสงขาว ค่าแสง จุดดำ/จุดขาว ส่วนสว่าง/เงา คอนทราสต์) รวมเป็นตาราง 256 ค่าต่อช่อง
//! แล้วจึงปรับความอิ่มสีและความสดของสีต่อพิกเซล — เร็วพอทั้งตัวอย่างสดในหน้าจอและภาพเต็มความละเอียด

use crate::error::{AppError, Result};
use image::DynamicImage;
use serde::{Deserialize, Serialize};

/// ค่าแสงและสี (0 = ไม่ปรับ)
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Adjust {
    /// ค่าแสง หน่วย EV (−3 ถึง 3)
    pub exposure: f64,
    // ที่เหลือ −100 ถึง 100
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
}

impl Adjust {
    pub fn is_neutral(&self) -> bool {
        *self == Adjust::default()
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
        let ok = self.exposure.is_finite()
            && self.exposure.abs() <= 3.0
            && sliders.iter().all(|v| v.is_finite() && v.abs() <= 100.0);
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
                *value = x.clamp(0.0, 1.0);
            }
        }
        out
    }

    /// ความอิ่มสี/ความสดของสีของพิกเซลหนึ่ง (ค่า 0–1 หลังผ่านตาราง)
    pub fn color(&self, rgb: [f64; 3]) -> [f64; 3] {
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

    /// ค่าออก 0–255 ของพิกเซลหนึ่ง (ใช้ทั้งภาพจริงและเทสต์เทียบกับ UI)
    pub fn pixel(&self, curves: &[[f64; 256]; 3], rgb: [u8; 3]) -> [u8; 3] {
        let mapped = [curves[0][rgb[0] as usize], curves[1][rgb[1] as usize], curves[2][rgb[2] as usize]];
        self.color(mapped).map(|v| (v * 255.0).round() as u8)
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
    let only_curves = a.saturation == 0.0 && a.vibrance == 0.0;
    // ตารางสามช่องที่ปัดเป็น 0–255 แล้ว: กรณีไม่ปรับสี ใช้ตารางตรง ๆ เร็วที่สุด
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
        }
    }

    #[test]
    fn neutral_changes_nothing() {
        let a = Adjust::default();
        let curves = a.curves();
        for v in [0u8, 1, 64, 128, 200, 255] {
            assert_eq!(a.pixel(&curves, [v, v / 2, 255 - v]), [v, v / 2, 255 - v]);
        }
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
        assert!(px(contrast, [60, 60, 60])[0] < 60 && px(contrast, [190, 190, 190])[0] > 190);
        assert_eq!(
            px(Adjust { saturation: -100.0, ..Default::default() }, [200, 80, 40]),
            [103, 103, 103],
            "ขาวดำ = ความสว่างเดิม"
        );
    }

    #[test]
    fn matches_the_ui_formula() {
        // ผลชุดนี้คัดลอกไปไว้ใน adjust.test.ts — แก้สูตรฝั่งไหนต้องแก้ทั้งสองฝั่ง
        let a = sample();
        let curves = a.curves();
        assert_eq!(a.pixel(&curves, [10, 20, 30]), [11, 30, 47]);
        assert_eq!(a.pixel(&curves, [120, 90, 200]), [148, 108, 251]);
        assert_eq!(a.pixel(&curves, [240, 230, 220]), [255, 255, 252]);
    }
}
