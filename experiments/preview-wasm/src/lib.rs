//! ต้นแบบชั่วคราว: สูตรแต่งภาพของ puri-core (ไฟล์เดียวกันทุกตัว) คอมไพล์เป็น WASM เพื่อวัดความเร็วเทียบตัวอย่างสดแบบ TS
#![allow(dead_code)]

#[path = "../../../crates/core/src/adjust.rs"]
pub mod adjust;
#[path = "../../../crates/core/src/local.rs"]
pub mod local;
#[path = "../../../crates/core/src/masks.rs"]
pub mod masks;

pub mod error {
    #[derive(Debug)]
    pub struct AppError(pub String);
    impl AppError {
        pub fn invalid(m: impl Into<String>) -> Self {
            AppError(m.into())
        }
    }
    pub type Result<T> = std::result::Result<T, AppError>;
}

pub mod raw {
    pub const HEADROOM: f32 = 2.0;
}

/// สำเนาของ Rect / EditSettings / Geometry จาก edit.rs (edit.rs ผูกกับฐานข้อมูล คอมไพล์เป็น WASM ไม่ได้)
pub mod edit {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct Rect {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    pub const FULL: Rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default)]
    pub struct EditSettings {
        pub rotate: u16,
        pub straighten: f64,
        pub crop: Option<Rect>,
        pub adjust: crate::adjust::Adjust,
        pub masks: Vec<crate::masks::Mask>,
        pub from_raw: bool,
    }

    pub struct Geometry {
        w: f64,
        h: f64,
        quarter: u16,
        cos: f64,
        sin: f64,
        scale: f64,
        crop: Rect,
    }

    impl Geometry {
        pub fn new(width: u32, height: u32, s: &EditSettings) -> Geometry {
            let (w, h) = if s.rotate % 180 == 90 { (height, width) } else { (width, height) };
            let (w, h) = (w.max(1) as f64, h.max(1) as f64);
            let (sin, cos) = s.straighten.to_radians().sin_cos();
            let (sa, ca) = (sin.abs(), cos.abs());
            let scale = (w / (w * ca + h * sa)).min(h / (w * sa + h * ca));
            Geometry { w, h, quarter: s.rotate, cos, sin, scale, crop: s.crop.unwrap_or(FULL) }
        }
        pub fn frame_long(&self, width: u32, height: u32) -> f64 {
            (width as f64 / self.crop.w).max(height as f64 / self.crop.h)
        }
        fn quarter_forward(&self, u: f64, v: f64) -> (f64, f64) {
            match self.quarter {
                90 => (1.0 - v, u),
                180 => (1.0 - u, 1.0 - v),
                270 => (v, 1.0 - u),
                _ => (u, v),
            }
        }
        fn quarter_inverse(&self, u: f64, v: f64) -> (f64, f64) {
            match self.quarter {
                90 => (v, 1.0 - u),
                180 => (1.0 - u, 1.0 - v),
                270 => (1.0 - v, u),
                _ => (u, v),
            }
        }
        pub fn forward(&self, u: f64, v: f64) -> (f64, f64) {
            let (u, v) = self.quarter_forward(u, v);
            let (px, py) = ((u - 0.5) * self.w, (v - 0.5) * self.h);
            let (rx, ry) = (px * self.cos - py * self.sin, px * self.sin + py * self.cos);
            let (fu, fv) = (rx / (self.scale * self.w) + 0.5, ry / (self.scale * self.h) + 0.5);
            ((fu - self.crop.x) / self.crop.w, (fv - self.crop.y) / self.crop.h)
        }
        fn to_rotated(&self, a: f64, b: f64) -> (f64, f64) {
            let (fu, fv) = (self.crop.x + a * self.crop.w, self.crop.y + b * self.crop.h);
            let (rx, ry) = ((fu - 0.5) * self.scale * self.w, (fv - 0.5) * self.scale * self.h);
            let (px, py) = (rx * self.cos + ry * self.sin, -rx * self.sin + ry * self.cos);
            (px / self.w + 0.5, py / self.h + 0.5)
        }
        pub fn inverse(&self, a: f64, b: f64) -> (f64, f64) {
            let (u, v) = self.to_rotated(a, b);
            self.quarter_inverse(u, v)
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    settings: edit::EditSettings,
    profile: String,
    source_w: u32,
    source_h: u32,
}

#[no_mangle]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(p: *mut u8, n: usize) {
    drop(Vec::from_raw_parts(p, 0, n));
}

/// เหมือน renderPixels (canvas = กรอบที่ตัดแล้วทั้งกรอบ) บนพิกเซล RGBA ในหน่วยความจำของ WASM · คืน 0 = สำเร็จ
#[no_mangle]
pub unsafe extern "C" fn render(px: *mut u8, w: u32, h: u32, job: *const u8, job_len: usize) -> i32 {
    let px = std::slice::from_raw_parts_mut(px, (w * h * 4) as usize);
    let Ok(job) = serde_json::from_slice::<Job>(std::slice::from_raw_parts(job, job_len)) else { return -1 };
    let profile = if job.profile == "raw" { adjust::Profile::Raw } else { adjust::Profile::Display };
    let g = edit::Geometry::new(job.source_w, job.source_h, &job.settings);
    let long = g.frame_long(w, h);
    adjust::apply_pixels(px, w as usize, h as usize, 4, &job.settings.adjust, profile, long);
    masks::apply(px, w as usize, h as usize, 4, &g, &job.settings.masks);
    0
}
