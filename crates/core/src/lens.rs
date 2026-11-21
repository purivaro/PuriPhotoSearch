//! แก้ความโค้งของเลนส์จากข้อมูลที่กล้องบันทึกไว้ในไฟล์ RAW (DECISIONS ข้อ 34)
//!
//! ทุกยี่ห้อแปลงเป็นตารางเดียวกัน: รัศมีบนภาพที่แก้แล้ว → รัศมีบนภาพจากเซนเซอร์ (หน่วย = ครึ่งเส้นทแยงมุมของภาพเต็ม)
//! แก้ตามรัศมีอย่างเดียว ทุกช่องสีใช้เส้นเดียวกัน — ใช้ทั้งกับรูป RAW (เหมือน JPEG ที่กล้องสร้าง)
//! และกับรูปคู่ RAW+JPEG ที่ Lightroom เปิดแก้เลนส์ไว้ (กรอบตัดของ Lightroom อยู่บนภาพที่แก้แล้ว)
//!
//! - **Panasonic / Leica (RW2):** แท็ก 0x0119 "DistortionInfo" ใน IFD แรก — 16 ค่า int16 (ชื่อค่าจาก ExifTool ·
//!   สูตรตามที่ผู้อื่นวัดเทียบกับ JPEG ของกล้อง) word 7 บิตล่าง = 1 → แก้ · ρ = ระยะ ÷ word 12 ·
//!   รัศมีหลังแก้ = ρ·(1 + k2ρ² + k4ρ⁴ + k6ρ⁶) ÷ z · k2 = w8, k4 = w4, k6 = w11 (÷ 32768) · z = 1 + w5 ÷ 32768
//! - **Sony (ARW) ยังไม่แก้:** ไฟล์มีแท็ก 0x7036/0x7037 แต่ลองตีความแบบจุดตามรัศมี (× 2⁻¹⁴) ทั้งสองทิศแล้ว ภาพห่างจาก JPEG ของ Lightroom
//!   มากกว่าไม่แก้ (สำเนา 6 คู่ ILCE-7CM2) — ยังไม่รู้สูตรที่ถูก จึงไม่แก้จนกว่าจะยืนยันได้ (DECISIONS ข้อ 34)

use rayon::prelude::*;

use crate::raw::Linear;

/// จำนวนจุดของตาราง และรัศมีสูงสุดที่ตารางครอบคลุม (เผื่อเกินมุมภาพเมื่อสุ่มนอกกรอบ)
const STEPS: usize = 1024;
const R_MAX: f64 = 1.25;

/// ความโค้งของเลนส์ตามรัศมี
#[derive(Debug, Clone, PartialEq)]
pub struct Radial {
    /// รัศมีบนเซนเซอร์ที่รัศมีหลังแก้ `i ÷ (STEPS−1) × R_MAX` (หน่วยครึ่งเส้นทแยงมุม)
    table: Vec<f64>,
    /// ครึ่งความกว้าง/สูงของภาพเต็มเป็นหน่วยครึ่งเส้นทแยงมุม
    half_x: f64,
    half_y: f64,
}

impl Radial {
    /// อ่านจากไฟล์ RAW — `width`×`height` = ขนาดภาพเต็มหลังตัดขอบตามกล้อง
    /// None = ไฟล์ไม่มีข้อมูล กล้องไม่ได้สั่งให้แก้ หรือค่าดูไม่สมเหตุผล
    pub fn from_raw_bytes(bytes: &[u8], width: u32, height: u32) -> Option<Radial> {
        let half_diagonal = (f64::from(width).powi(2) + f64::from(height).powi(2)).sqrt() / 2.0;
        let source = panasonic(&panasonic_distortion_words(bytes)?, half_diagonal)?;
        let mut table: Vec<f64> = (0..STEPS).map(|i| source(R_MAX * i as f64 / (STEPS - 1) as f64)).collect();
        // ถึงมุมภาพต้องเพิ่มขึ้นตลอด (ไม่พับ) และไม่ซูมเกิน ±25% — นอกนั้นถือว่าอ่านค่าผิด
        let corner_at = ((STEPS - 1) as f64 / R_MAX).round() as usize;
        let inside = &table[..=corner_at];
        if !inside.windows(2).all(|w| w[1] > w[0])
            || !(0.75..=1.25).contains(&table[corner_at])
            || inside.iter().any(|v| !v.is_finite())
        {
            return None;
        }
        // เลยมุมภาพไป (ใช้เฉพาะจุดที่สุ่มนอกกรอบ) พหุนามของบางระยะซูมพับกลับ — ต่อเป็นเส้นตรงด้วยความชันที่มุมภาพแทน
        let slope = table[corner_at] - table[corner_at - 1];
        for i in corner_at + 1..STEPS {
            table[i] = table[i - 1] + slope;
        }
        Some(Radial {
            table,
            half_x: f64::from(width) / 2.0 / half_diagonal,
            half_y: f64::from(height) / 2.0 / half_diagonal,
        })
    }

    /// รัศมีหลังแก้ → รัศมีบนเซนเซอร์ (เทียบตาราง)
    fn source(&self, r: f64) -> f64 {
        let x = (r / R_MAX * (STEPS - 1) as f64).max(0.0);
        let i = (x.floor() as usize).min(STEPS - 2);
        let t = x - i as f64;
        self.table[i] * (1.0 - t) + self.table[i + 1] * t
    }

    /// จุดบนภาพที่แก้แล้ว (สัดส่วน 0–1) → จุดเดียวกันบนภาพจากเซนเซอร์ (สัดส่วน 0–1)
    /// `same_axes` = แกนของภาพที่ใช้พิกัดนี้ตรงกับขนาดที่ให้ตอนอ่านค่า (false = หมุน 90° เทียบกัน แกนสลับ)
    pub fn source_point(&self, x: f64, y: f64, same_axes: bool) -> (f64, f64) {
        let (hx, hy) = if same_axes { (self.half_x, self.half_y) } else { (self.half_y, self.half_x) };
        let (dx, dy) = ((x - 0.5) * 2.0 * hx, (y - 0.5) * 2.0 * hy);
        let r = (dx * dx + dy * dy).sqrt();
        if r < 1e-9 {
            return (x, y);
        }
        let k = self.source(r) / r;
        (0.5 + dx * k / (2.0 * hx), 0.5 + dy * k / (2.0 * hy))
    }

    /// ภาพที่แก้แล้วขนาดเท่าเดิม (สุ่มแบบ bilinear) — ภาพที่ล้างแบบย่อก็ใช้ได้ เพราะคิดเป็นสัดส่วนของภาพ
    pub fn correct(&self, img: &Linear) -> Linear {
        let (w, h) = img.dimensions();
        let same_axes = (w >= h) == (self.half_x >= self.half_y);
        let src = img.as_raw();
        let mut out = vec![0.0f32; src.len()];
        let row = w as usize * 3;
        out.par_chunks_mut(row).enumerate().for_each(|(y, line)| {
            for x in 0..w as usize {
                let (u, v) = ((x as f64 + 0.5) / f64::from(w), (y as f64 + 0.5) / f64::from(h));
                let (su, sv) = self.source_point(u, v, same_axes);
                let px = bilinear(src, w as usize, h as usize, su * f64::from(w) - 0.5, sv * f64::from(h) - 0.5);
                line[x * 3..x * 3 + 3].copy_from_slice(&px);
            }
        });
        Linear::from_raw(w, h, out).expect("ขนาดเท่าภาพเดิม")
    }
}

/// Panasonic: รัศมีหลังแก้ → รัศมีบนเซนเซอร์ (ย้อนพหุนามด้วย Newton) หน่วยครึ่งเส้นทแยงมุม
fn panasonic(words: &[i16; 16], half_diagonal: f64) -> Option<impl Fn(f64) -> f64> {
    if words[7] & 0x0f != 1 {
        return None;
    }
    let norm = f64::from(words[12]);
    // ตัวหารรัศมีต้องใกล้ครึ่งเส้นทแยงมุมของภาพ — ไม่ใกล้ = อ่านผิดหรือขนาดภาพไม่ตรง
    if norm <= 0.0 || !(0.5..=2.0).contains(&(norm / half_diagonal)) {
        return None;
    }
    let q = |w: i16| f64::from(w) / 32768.0;
    let (k2, k4, k6, zoom) = (q(words[8]), q(words[4]), q(words[11]), 1.0 + q(words[5]));
    if !(0.75..=1.25).contains(&zoom) {
        return None;
    }
    let corrected = move |rd: f64| {
        let r2 = rd * rd;
        rd * (1.0 + r2 * (k2 + r2 * (k4 + r2 * k6))) / zoom
    };
    let slope = move |rd: f64| {
        let r2 = rd * rd;
        (1.0 + r2 * (3.0 * k2 + r2 * (5.0 * k4 + r2 * 7.0 * k6))) / zoom
    };
    let scale = half_diagonal / norm;
    Some(move |r: f64| {
        let ru = r * scale;
        let mut rd = ru * zoom;
        for _ in 0..8 {
            let d = slope(rd);
            if d.abs() < 1e-9 {
                break;
            }
            let step = (corrected(rd) - ru) / d;
            rd -= step;
            if step.abs() < 1e-12 {
                break;
            }
        }
        rd / scale
    })
}

fn bilinear(src: &[f32], w: usize, h: usize, x: f64, y: f64) -> [f32; 3] {
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
    let at = |xx: usize, yy: usize, c: usize| src[(yy * w + xx) * 3 + c];
    let mut px = [0.0f32; 3];
    for (c, p) in px.iter_mut().enumerate() {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        *p = top * (1.0 - fy) + bottom * fy;
    }
    px
}

// ───────────────────────── อ่าน TIFF ─────────────────────────

struct Tiff<'a> {
    bytes: &'a [u8],
    le: bool,
}

/// ค่าของแท็กหนึ่งใน IFD: (ชนิด, จำนวน, ตำแหน่งข้อมูล)
type Entry = (u16, u32, usize);

impl<'a> Tiff<'a> {
    fn new(bytes: &'a [u8]) -> Option<(Tiff<'a>, u16)> {
        let le = match bytes.get(0..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let t = Tiff { bytes, le };
        let magic = t.u16(2)?;
        Some((t, magic))
    }

    fn u16(&self, p: usize) -> Option<u16> {
        let b = self.bytes.get(p..p + 2)?;
        Some(if self.le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    }

    fn u32(&self, p: usize) -> Option<u32> {
        let b = self.bytes.get(p..p + 4)?;
        Some(if self.le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    }

    fn first_ifd(&self) -> Option<usize> {
        Some(self.u32(4)? as usize)
    }

    fn entry(&self, ifd: usize, tag: u16) -> Option<Entry> {
        let count = self.u16(ifd)? as usize;
        for i in 0..count.min(1024) {
            let e = ifd + 2 + i * 12;
            if self.u16(e)? != tag {
                continue;
            }
            let (typ, n) = (self.u16(e + 2)?, self.u32(e + 4)?);
            let size = match typ {
                3 | 8 => 2,
                4 | 9 => 4,
                _ => 1,
            } * n as usize;
            let at = if size <= 4 { e + 8 } else { self.u32(e + 8)? as usize };
            return Some((typ, n, at));
        }
        None
    }
}

/// 16 ค่าของแท็ก 0x0119 ใน IFD แรกของไฟล์ RW2/RWL (TIFF ที่ magic เป็น 0x55)
fn panasonic_distortion_words(bytes: &[u8]) -> Option<[i16; 16]> {
    let (t, magic) = Tiff::new(bytes)?;
    if magic != 0x55 {
        return None;
    }
    let (typ, n, at) = t.entry(t.first_ifd()?, 0x0119)?;
    // ชนิด UNDEFINED (7) ยาว 32 ไบต์ — อย่างอื่นไม่ใช่รูปแบบที่รู้จัก
    if typ != 7 || n != 32 {
        return None;
    }
    let raw = bytes.get(at..at + 32)?;
    let mut words = [0i16; 16];
    for (k, w) in words.iter_mut().enumerate() {
        *w = i16::from_le_bytes([raw[k * 2], raw[k * 2 + 1]]);
    }
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// แท็ก 0x0119 จากไฟล์ Lumix DC-S5 จริง (ค่าตัวเลขของเลนส์ ไม่ใช่ภาพ)
    const S5_WORDS: [u8; 32] = [
        0xeb, 0x17, 0x51, 0x9d, 0xdd, 0x00, 0xa0, 0x00, 0x8b, 0xfc, 0xf4, 0xfb, 0x85, 0x00, 0x01, 0xfa, 0x9c, 0xfe,
        0x37, 0x01, 0x58, 0x01, 0xd8, 0x00, 0x15, 0x0e, 0x2b, 0x03, 0x86, 0xeb, 0xe2, 0xb7,
    ];

    /// TIFF แบบ RW2 เล็กที่สุดที่มีแท็ก 0x0119
    fn rw2_with(words: &[u8; 32]) -> Vec<u8> {
        let mut b = vec![b'I', b'I', 0x55, 0x00, 8, 0, 0, 0];
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&0x0119u16.to_le_bytes());
        b.extend_from_slice(&7u16.to_le_bytes());
        b.extend_from_slice(&32u32.to_le_bytes());
        b.extend_from_slice(&26u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(words);
        b
    }

    #[test]
    fn lumix_s5_values_give_an_exact_inverse_of_the_polynomial() {
        let lens = Radial::from_raw_bytes(&rw2_with(&S5_WORDS), 6000, 4000).expect("อ่านได้");
        // ภาพ 6000×4000: ครึ่งเส้นทแยงมุม 3605.55 ≈ word 12 (3605)
        let (k2, k4, k6, zoom) = (-356.0 / 32768.0, -885.0 / 32768.0, 216.0 / 32768.0, 1.0 - 1036.0 / 32768.0);
        let scale = 3605.551275463989 / 3605.0;
        for i in 0..=100 {
            let rd = f64::from(i) / 100.0;
            let ru = rd * (1.0 + rd * rd * (k2 + rd * rd * (k4 + rd * rd * k6))) / zoom;
            // รัศมีหลังแก้ (หน่วยครึ่งเส้นทแยงมุม) → รัศมีบนเซนเซอร์ ต้องได้ rd คืน
            assert!((lens.source(ru / scale) * scale - rd).abs() < 1e-5, "{rd}");
        }
        assert_eq!(lens.source_point(0.5, 0.5, true), (0.5, 0.5));
        let (x, y) = lens.source_point(0.0, 0.0, true);
        assert!((0.0..0.05).contains(&x) && (0.0..0.05).contains(&y), "มุมภาพยังอยู่ในภาพจากเซนเซอร์ ({x}, {y})");
    }

    /// อีกระยะซูมของไฟล์ DC-S5 จริง: พหุนามพับกลับที่รัศมี ~1.17 (นอกมุมภาพ)
    const S5_FOLDING_WORDS: [u8; 32] = [
        0x69, 0x6c, 0xa0, 0x9b, 0x0c, 0x01, 0x80, 0x02, 0x1d, 0xfe, 0xf3, 0xff, 0x0c, 0x00, 0x01, 0xf9, 0x8c, 0x0f,
        0x15, 0x02, 0x58, 0x01, 0x8f, 0xf8, 0x15, 0x0e, 0x09, 0x04, 0x4b, 0x7d, 0x58, 0xb4,
    ];

    #[test]
    fn a_curve_that_folds_only_outside_the_frame_is_still_used() {
        let lens = Radial::from_raw_bytes(&rw2_with(&S5_FOLDING_WORDS), 6000, 4000).expect("ในภาพไม่พับ ต้องแก้");
        // ทั้งตารางเพิ่มขึ้นตลอด รวมช่วงเลยมุมภาพที่ต่อเป็นเส้นตรง
        assert!(lens.table.windows(2).all(|w| w[1] > w[0]));
        let corner = lens.source(1.0);
        assert!((0.9..1.0).contains(&corner), "มุมภาพดึงจากด้านในเซนเซอร์ ({corner})");
    }

    #[test]
    fn no_correction_when_the_camera_says_off_or_the_data_is_odd() {
        let mut off = S5_WORDS;
        off[14] = 0x00; // word 7 บิตล่าง = 0
        assert!(Radial::from_raw_bytes(&rw2_with(&off), 6000, 4000).is_none());
        // ตัวหารรัศมีไม่เข้ากับขนาดภาพ
        assert!(Radial::from_raw_bytes(&rw2_with(&S5_WORDS), 600, 400).is_none());
        // ไม่ใช่ไฟล์ RAW ที่รู้จัก
        assert!(Radial::from_raw_bytes(b"\xFF\xD8\xFF", 6000, 4000).is_none());
    }

    #[test]
    fn correcting_moves_a_point_along_its_radius() {
        let lens = Radial::from_raw_bytes(&rw2_with(&S5_WORDS), 6000, 4000).unwrap();
        let (x, y) = lens.source_point(0.8, 0.3, true);
        let before = ((0.8f64 - 0.5) * 6000.0).atan2((0.3 - 0.5) * 4000.0);
        let after = ((x - 0.5) * 6000.0).atan2((y - 0.5) * 4000.0);
        assert!((before - after).abs() < 1e-9);
        // ภาพหมุน 90° (สลับแกน) ให้จุดเดียวกันเมื่อสลับพิกัด
        let (sx, sy) = lens.source_point(0.3, 0.8, false);
        assert!((sx - y).abs() < 1e-12 && (sy - x).abs() < 1e-12);
    }
}
