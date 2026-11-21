//! ปรับที่ต้องดูพิกเซลรอบข้าง (DECISIONS ข้อ 27): Texture, Clarity, Dehaze แบบ Lightroom
//!
//! ทำหลังขั้นต่อพิกเซลทั้งหมด บนภาพผล 8 บิต (sRGB) · สูตรเดียวกับ `src/features/photos/local.ts` ที่หน้าแต่งภาพใช้
//! (ค่าอ้างอิงร่วม [`LOCAL_EXPECTED`]) · รัศมีเป็นสัดส่วนของด้านยาวของภาพ: ภาพตัวอย่างเล็กกับไฟล์เต็มขนาดได้ผลแบบเดียวกัน
//!
//! - **Texture / Clarity:** แยกรายละเอียดออกจาก "ฐาน" ที่ได้จาก guided filter (เบลอแบบรักษาขอบ — ขอบชัดไม่เกิดรัศมีรอบวัตถุ)
//!   แล้วเพิ่ม/ลดรายละเอียดของความสว่าง · Texture รัศมีเล็ก (ผิว เนื้อผ้า) · Clarity รัศมีใหญ่ เน้นช่วงกลาง (มิติของภาพ) ·
//!   ค่าลบ = เข้าหาฐาน (นุ่ม ผิวเนียน)
//! - **Dehaze:** dark channel prior (He et al. 2009) แบบย่อ — สีของหมอกจากส่วนที่ dark channel สูงสุด · ความโปร่งคิดทีละบล็อก
//!   แล้วให้ guided filter ทำตามขอบของภาพ · ค่าลบ = เติมหมอก
//!
//! guided filter คิดที่ความละเอียดต่ำแล้วขยายสัมประสิทธิ์กลับ (fast guided filter, He & Sun 2015) — ภาพเต็มขนาดใช้หน่วยความจำ
//! เพิ่มแค่ค่าความสว่างหนึ่งชุด

use crate::adjust::Adjust;

/// รัศมีเป็นสัดส่วนของด้านยาว และค่า ε ของ guided filter (ความต่างที่ถือเป็น "ขอบ" ที่ต้องรักษา — ยกกำลังสอง)
const TEXTURE_RADIUS: f32 = 0.0035;
const TEXTURE_EPS: f32 = 0.004;
const CLARITY_RADIUS: f32 = 0.015;
const CLARITY_EPS: f32 = 0.02;
/// ขนาดบล็อกของ dark channel (สัดส่วนของด้านยาว)
const DEHAZE_BLOCK: f32 = 0.01;
const DEHAZE_EPS: f32 = 0.001;
/// ความโปร่งต่ำสุด (กันส่วนที่มืดมากถูกขยายจนแตก)
const MIN_TRANSMISSION: f32 = 0.1;

/// มีขั้นที่ต้องดูพิกเซลรอบข้าง
pub fn is_active(a: &Adjust) -> bool {
    a.texture != 0.0 || a.clarity != 0.0 || a.dehaze != 0.0
}

/// ปรับในที่: `px` พิกเซลละ `stride` ไบต์ (3 = RGB, 4 = RGBA — ช่องที่สี่ไม่แตะ) · `long` = ด้านยาวของภาพทั้งกรอบเป็นพิกเซลของ `px`
/// (ปกติเท่าด้านยาวของ `px` — หน้าแต่งภาพที่ขยายดูบางส่วนส่งด้านยาวของทั้งกรอบ)
pub fn apply(px: &mut [u8], width: usize, height: usize, stride: usize, long: f32, a: &Adjust) {
    if width == 0 || height == 0 || px.len() < width * height * stride {
        return;
    }
    if a.dehaze != 0.0 {
        dehaze(px, width, height, stride, long, a.dehaze as f32 / 100.0);
    }
    if a.texture != 0.0 || a.clarity != 0.0 {
        detail(px, width, height, stride, long, a.texture as f32 / 100.0, a.clarity as f32 / 100.0);
    }
}

fn luma(p: &[u8]) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn radius(fraction: f32, long: f32) -> usize {
    (fraction * long).round().max(1.0) as usize
}

/// ค่าเฉลี่ยในหน้าต่าง (2r+1)×(2r+1) — ที่ขอบภาพนับเฉพาะพิกเซลที่อยู่ในภาพ
fn box_mean(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut prefix = vec![0f64; w.max(h) + 1];
    let mut rows = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            prefix[x + 1] = prefix[x] + src[y * w + x] as f64;
        }
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r + 1).min(w));
            rows[y * w + x] = ((prefix[b] - prefix[a]) / (b - a) as f64) as f32;
        }
    }
    let mut out = vec![0f32; w * h];
    for x in 0..w {
        for y in 0..h {
            prefix[y + 1] = prefix[y] + rows[y * w + x] as f64;
        }
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r + 1).min(h));
            out[y * w + x] = ((prefix[b] - prefix[a]) / (b - a) as f64) as f32;
        }
    }
    out
}

/// ย่อด้วยค่าเฉลี่ยของบล็อก s×s (บล็อกท้ายแถว/คอลัมน์อาจเล็กกว่า)
fn shrink(src: &[f32], w: usize, h: usize, s: usize) -> (Vec<f32>, usize, usize) {
    if s <= 1 {
        return (src.to_vec(), w, h);
    }
    let (wl, hl) = (w.div_ceil(s), h.div_ceil(s));
    let mut sum = vec![0f64; wl * hl];
    let mut n = vec![0u32; wl * hl];
    for y in 0..h {
        for x in 0..w {
            let k = (y / s) * wl + x / s;
            sum[k] += src[y * w + x] as f64;
            n[k] += 1;
        }
    }
    (sum.iter().zip(&n).map(|(s, n)| (s / *n as f64) as f32).collect(), wl, hl)
}

/// สัมประสิทธิ์ของ guided filter ที่ความละเอียดต่ำ: ค่าผลที่พิกเซลเต็มขนาด = a·guide + b (a, b ขยายแบบ bilinear)
struct Coeffs {
    a: Vec<f32>,
    b: Vec<f32>,
    w: usize,
    h: usize,
    /// พิกเซลเต็มขนาดต่อหนึ่งพิกเซลของความละเอียดต่ำ
    s: usize,
}

/// guided filter (He et al.): `input` ตามขอบของ `guide` — สองภาพขนาด w×h เท่ากัน
fn guided(guide: &[f32], input: &[f32], w: usize, h: usize, r: usize, eps: f32, s: usize) -> Coeffs {
    let mean_i = box_mean(guide, w, h, r);
    let mean_p = box_mean(input, w, h, r);
    let ip: Vec<f32> = guide.iter().zip(input).map(|(i, p)| i * p).collect();
    let ii: Vec<f32> = guide.iter().map(|i| i * i).collect();
    let (mean_ip, mean_ii) = (box_mean(&ip, w, h, r), box_mean(&ii, w, h, r));
    let mut a = vec![0f32; w * h];
    let mut b = vec![0f32; w * h];
    for k in 0..w * h {
        let cov = mean_ip[k] - mean_i[k] * mean_p[k];
        let var = mean_ii[k] - mean_i[k] * mean_i[k];
        a[k] = cov / (var + eps);
        b[k] = mean_p[k] - a[k] * mean_i[k];
    }
    Coeffs { a: box_mean(&a, w, h, r), b: box_mean(&b, w, h, r), w, h, s }
}

impl Coeffs {
    /// a, b ที่พิกเซล (x, y) ของภาพเต็มขนาด
    fn at(&self, x: usize, y: usize) -> (f32, f32) {
        let s = self.s as f32;
        let fx = ((x as f32 + 0.5) / s - 0.5).clamp(0.0, (self.w - 1) as f32);
        let fy = ((y as f32 + 0.5) / s - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let lerp = |v: &[f32]| {
            let top = v[y0 * self.w + x0] * (1.0 - tx) + v[y0 * self.w + x1] * tx;
            let bottom = v[y1 * self.w + x0] * (1.0 - tx) + v[y1 * self.w + x1] * tx;
            top * (1.0 - ty) + bottom * ty
        };
        (lerp(&self.a), lerp(&self.b))
    }
}

/// guided filter ของความสว่างกับตัวเอง (เบลอแบบรักษาขอบ) ที่รัศมี `fraction` ของด้านยาว — คิดที่ความละเอียด 1/s
fn smooth(y: &[f32], w: usize, h: usize, long: f32, fraction: f32, eps: f32) -> Coeffs {
    let r = radius(fraction, long);
    let s = (r / 4).max(1);
    let (low, wl, hl) = shrink(y, w, h, s);
    let rl = ((r as f32 / s as f32).round() as usize).max(1);
    guided(&low, &low, wl, hl, rl, eps, s)
}

/// Texture / Clarity: ความสว่าง + k·(ความสว่าง − ฐาน) · Clarity คูณน้ำหนักช่วงกลาง (0 ที่ดำ/ขาวสุด)
fn detail(px: &mut [u8], w: usize, h: usize, stride: usize, long: f32, texture: f32, clarity: f32) {
    let y: Vec<f32> = (0..w * h).map(|i| luma(&px[i * stride..])).collect();
    let tex = (texture != 0.0).then(|| smooth(&y, w, h, long, TEXTURE_RADIUS, TEXTURE_EPS));
    let cla = (clarity != 0.0).then(|| smooth(&y, w, h, long, CLARITY_RADIUS, CLARITY_EPS));
    for row in 0..h {
        for col in 0..w {
            let i = row * w + col;
            let l = y[i];
            let mut delta = 0.0;
            if let Some(c) = &tex {
                let (a, b) = c.at(col, row);
                delta += texture * (l - (a * l + b));
            }
            if let Some(c) = &cla {
                let (a, b) = c.at(col, row);
                let mid = 1.0 - (2.0 * l - 1.0) * (2.0 * l - 1.0);
                delta += clarity * (l - (a * l + b)) * mid;
            }
            for v in &mut px[i * stride..i * stride + 3] {
                *v = to_u8(*v as f32 / 255.0 + delta);
            }
        }
    }
}

/// Dehaze: ค่าบวก = J = (I − A)/t + A · ค่าลบ = เข้าหาสีของหมอก
fn dehaze(px: &mut [u8], w: usize, h: usize, stride: usize, long: f32, amount: f32) {
    let s = radius(DEHAZE_BLOCK, long);
    let (wl, hl) = (w.div_ceil(s), h.div_ceil(s));
    // ต่อบล็อก: ค่าต่ำสุดของ min(R,G,B) · ค่าเฉลี่ยของสีและความสว่าง
    let mut dark = vec![1f32; wl * hl];
    let mut sum = vec![[0f64; 4]; wl * hl];
    let mut n = vec![0u32; wl * hl];
    for row in 0..h {
        for col in 0..w {
            let p = &px[(row * w + col) * stride..];
            let k = (row / s) * wl + col / s;
            dark[k] = dark[k].min(p[0].min(p[1]).min(p[2]) as f32 / 255.0);
            for c in 0..3 {
                sum[k][c] += p[c] as f64 / 255.0;
            }
            sum[k][3] += luma(p) as f64;
            n[k] += 1;
        }
    }
    let mean = |k: usize, c: usize| (sum[k][c] / n[k] as f64) as f32;
    // สีของหมอก: ค่าเฉลี่ยของ 1% ของบล็อกที่ dark channel สูงสุด (เท่ากันเรียงตามตำแหน่ง)
    let mut order: Vec<usize> = (0..wl * hl).collect();
    order.sort_by(|a, b| dark[*b].total_cmp(&dark[*a]));
    let top = (wl * hl / 100).max(1);
    let mut air = [0f32; 3];
    for &k in &order[..top] {
        for (c, v) in air.iter_mut().enumerate() {
            *v += mean(k, c);
        }
    }
    let air = air.map(|v| (v / top as f32).max(0.05));
    if amount < 0.0 {
        let t = -amount * 0.5;
        for i in 0..w * h {
            for (c, a) in air.iter().enumerate() {
                let v = px[i * stride + c] as f32 / 255.0;
                px[i * stride + c] = to_u8(v + (a - v) * t);
            }
        }
        return;
    }
    // dark channel ของภาพที่หารด้วยสีของหมอก: ต่ำสุดในบล็อก แล้วต่ำสุดในบล็อกข้าง ๆ (3×3)
    let mut norm = vec![f32::MAX; wl * hl];
    for row in 0..h {
        for col in 0..w {
            let p = &px[(row * w + col) * stride..];
            let k = (row / s) * wl + col / s;
            let m = (0..3).map(|c| p[c] as f32 / 255.0 / air[c]).fold(f32::MAX, f32::min);
            norm[k] = norm[k].min(m);
        }
    }
    let mut spread = vec![0f32; wl * hl];
    for row in 0..hl {
        for col in 0..wl {
            let mut m = f32::MAX;
            for dy in row.saturating_sub(1)..(row + 2).min(hl) {
                for dx in col.saturating_sub(1)..(col + 2).min(wl) {
                    m = m.min(norm[dy * wl + dx]);
                }
            }
            spread[row * wl + col] = m;
        }
    }
    let omega = 0.95 * amount;
    let t_low: Vec<f32> = spread.iter().map(|d| 1.0 - omega * d).collect();
    let y_low: Vec<f32> = (0..wl * hl).map(|k| mean(k, 3)).collect();
    let coeffs = guided(&y_low, &t_low, wl, hl, 2, DEHAZE_EPS, s);
    for row in 0..h {
        for col in 0..w {
            let i = (row * w + col) * stride;
            let l = luma(&px[i..]);
            let (a, b) = coeffs.at(col, row);
            let t = (a * l + b).clamp(MIN_TRANSMISSION, 1.0);
            for (c, air_c) in air.iter().enumerate() {
                let v = px[i + c] as f32 / 255.0;
                px[i + c] = to_u8((v - air_c) / t + air_c);
            }
        }
    }
}

/// ภาพทดสอบที่ Rust และ `local.test.ts` สร้างเหมือนกัน (คิดด้วยจำนวนเต็มล้วน): ไล่สี + ลายตาราง + จุดละเอียด · RGB
#[doc(hidden)]
pub fn test_image(w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            out.push(((x * 37 + y * 11) % 200 + 30) as u8);
            out.push((((x / 6 + y / 4) % 2) * 120 + 60 + (x * y % 7) * 3) as u8);
            out.push((40 + x * 4 + (y % 5) * 6) as u8);
        }
    }
    out
}

/// ตำแหน่งที่เทียบใน [`LOCAL_EXPECTED`]
#[doc(hidden)]
pub const LOCAL_SAMPLES: [(usize, usize); 5] = [(0, 0), (5, 5), (20, 10), (40, 25), (47, 31)];

/// ค่าอ้างอิงร่วมกับ `local.test.ts`: ภาพ [`test_image`] 48×32 ปรับ [Texture, Clarity, Dehaze] แล้วอ่านพิกเซลที่ [`LOCAL_SAMPLES`]
#[doc(hidden)]
pub const LOCAL_EXPECTED: [([f64; 3], [[u8; 3]; 5]); 5] = [
    ([-60.0, 0.0, 0.0], [[35, 65, 45], [68, 190, 58], [80, 192, 120], [184, 77, 199], [110, 63, 234]]),
    ([0.0, 50.0, 0.0], [[27, 57, 37], [75, 197, 65], [81, 193, 121], [185, 78, 200], [110, 63, 234]]),
    ([0.0, 0.0, 40.0], [[17, 50, 27], [58, 192, 47], [67, 192, 110], [183, 67, 198], [101, 52, 235]]),
    ([0.0, 0.0, -30.0], [[57, 80, 67], [91, 192, 84], [100, 192, 135], [189, 96, 203], [125, 83, 232]]),
    ([40.0, -30.0, 0.0], [[29, 59, 39], [68, 190, 58], [79, 191, 119], [186, 79, 201], [110, 63, 234]]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn adjust(texture: f64, clarity: f64, dehaze: f64) -> Adjust {
        Adjust { texture, clarity, dehaze, ..Default::default() }
    }

    fn run(a: &Adjust) -> Vec<u8> {
        let mut px = test_image(48, 32);
        apply(&mut px, 48, 32, 3, 48.0, a);
        px
    }

    #[test]
    fn texture_smooths_or_sharpens_fine_detail_but_keeps_strong_edges() {
        // ไล่ระดับเรียบ + เม็ดละเอียด ±6 ระดับ (แบบผิว) + ขอบชัดกลางภาพ
        let (w, h) = (96, 64);
        let base = |x: usize| 70.0 + 60.0 * x as f32 / w as f32 + if x >= w / 2 { 80.0 } else { 0.0 };
        let mut px = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let v = (base(x) + ((x * 31 + y * 17) % 13) as f32 - 6.0).round() as u8;
                px.extend([v, v, v]);
            }
        }
        let grain = |px: &[u8]| {
            let d: Vec<f64> = (8..56)
                .flat_map(|y| (8..40).map(move |x| (x, y)))
                .map(|(x, y)| px[(y * w + x) * 3] as f64 - base(x) as f64)
                .collect();
            let m = d.iter().sum::<f64>() / d.len() as f64;
            d.iter().map(|v| (v - m).powi(2)).sum::<f64>() / d.len() as f64
        };
        let run = |t: f64| {
            let mut out = px.clone();
            apply(&mut out, w, h, 3, 1000.0, &adjust(t, 0.0, 0.0));
            out
        };
        let (plain, soft, sharp) = (grain(&px), grain(&run(-100.0)), grain(&run(100.0)));
        assert!(soft < plain * 0.4 && sharp > plain * 1.8, "ผิวเนียนลง / คมขึ้น: {plain} → {soft}, {sharp}");
        // ขอบชัด (ต่างกัน 80 ระดับ) ยังคมอยู่: ไม่ถูกเบลอเป็นทางลาด
        let soft_px = run(-100.0);
        let jump = soft_px[(32 * w + w / 2) * 3] as i32 - soft_px[(32 * w + w / 2 - 1) * 3] as i32;
        assert!(jump > 60, "ขอบชัดไม่หาย: {jump}");
    }

    #[test]
    fn clarity_and_dehaze_change_contrast_in_the_right_direction() {
        let std = |px: &[u8]| {
            let l: Vec<f64> = px.chunks(3).map(|p| luma(p) as f64).collect();
            let m = l.iter().sum::<f64>() / l.len() as f64;
            (l.iter().map(|x| (x - m).powi(2)).sum::<f64>() / l.len() as f64).sqrt()
        };
        let plain = test_image(48, 32);
        assert!(std(&run(&adjust(0.0, 100.0, 0.0))) > std(&plain), "Clarity บวก = มิติมากขึ้น");
        assert!(std(&run(&adjust(0.0, -100.0, 0.0))) < std(&plain), "Clarity ลบ = นุ่มลง");
        assert!(std(&run(&adjust(0.0, 0.0, 60.0))) > std(&plain), "Dehaze บวก = ใสขึ้น");
        assert!(std(&run(&adjust(0.0, 0.0, -60.0))) < std(&plain), "Dehaze ลบ = หมอกมากขึ้น");
        assert_eq!(run(&adjust(0.0, 0.0, 0.0)), plain, "ไม่ปรับ = ภาพเดิม");
    }

    #[test]
    fn matches_the_shared_reference_values() {
        for (amounts, expected) in LOCAL_EXPECTED {
            let px = run(&adjust(amounts[0], amounts[1], amounts[2]));
            let got =
                LOCAL_SAMPLES.map(|(x, y)| [px[(y * 48 + x) * 3], px[(y * 48 + x) * 3 + 1], px[(y * 48 + x) * 3 + 2]]);
            assert_eq!(got, expected, "{amounts:?}");
        }
    }
}
