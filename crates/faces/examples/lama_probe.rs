//! ลองตัวเติมภาพ (LaMa) กับภาพ 512×512 + Mask (ขาว = ต้องเติม) — เครื่องมือของนักพัฒนา เขียนเฉพาะไฟล์ผล
//!
//! ```text
//! cargo run --release -p puri-faces --example lama_probe -- <lama_fp32.onnx> <ภาพ.png> <mask.png> <ผล.png> [ภาพ mask ผล ...]
//! ```

// ลิงก์ crate นี้ด้วย: build.rs ของมันลิงก์ runtime ของ clang ที่โค้ด CoreML ใน ONNX Runtime ต้องใช้
use puri_faces as _;

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use ort::{session::Session, value::Tensor};
    use std::time::Instant;

    const SIDE: usize = 512;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let model = &args[0];
    let env = ort::init().with_name("lama-probe").build().expect("ort");
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let threads = std::env::var("THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or(threads);
    let started = Instant::now();
    // CPU เท่านั้น (ข้อ 40: โมเดลใหญ่บน CoreML ทำเครื่องรีบูต)
    let mut builder = Session::builder(&env)
        .expect("builder")
        .with_intra_threads(threads)
        .expect("threads")
        .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
        .expect("cpu");
    let mut session = builder.commit_from_file(model).expect("โหลดโมเดล");
    println!("โหลด {:.1} วินาที", started.elapsed().as_secs_f64());
    for i in session.inputs() {
        println!("input {:?}", i);
    }
    for o in session.outputs() {
        println!("output {:?}", o);
    }
    for job in args[1..].chunks(3) {
        let img = image::open(&job[0]).expect("ภาพ").to_rgb8();
        let mask = image::open(&job[1]).expect("mask").to_luma8();
        assert_eq!((img.width() as usize, img.height() as usize), (SIDE, SIDE));
        let plane = SIDE * SIDE;
        let mut pixels = vec![0f32; 3 * plane];
        let mut holes = vec![0f32; plane];
        for (x, y, p) in img.enumerate_pixels() {
            let i = y as usize * SIDE + x as usize;
            let hole = mask.get_pixel(x, y)[0] > 127;
            // ช่องที่ต้องเติมต้องเป็นศูนย์ (โมเดลไม่ลบให้เอง — ไม่ลบจะเห็นเงาของเดิม)
            for c in 0..3 {
                pixels[c * plane + i] = if hole { 0.0 } else { f32::from(p[c]) / 255.0 };
            }
            holes[i] = if hole { 1.0 } else { 0.0 };
        }
        let t = Instant::now();
        let image = Tensor::from_array(([1usize, 3, SIDE, SIDE], pixels)).expect("tensor");
        let mask_t = Tensor::from_array(([1usize, 1, SIDE, SIDE], holes)).expect("tensor");
        let outputs = session.run(ort::inputs!["image" => image, "mask" => mask_t]).expect("run");
        let (shape, data) = outputs[0].try_extract_tensor::<f32>().expect("output");
        let (lo, hi) = data.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let scale = if hi <= 1.5 { 255.0 } else { 1.0 };
        let out = image::RgbImage::from_fn(SIDE as u32, SIDE as u32, |x, y| {
            let i = y as usize * SIDE + x as usize;
            image::Rgb([0, 1, 2].map(|c| (data[c * plane + i] * scale).round().clamp(0.0, 255.0) as u8))
        });
        out.save(&job[2]).expect("เขียนผล");
        println!("{}: {:.2} วินาที · output {:?} ช่วง {lo:.3}..{hi:.3}", job[2], t.elapsed().as_secs_f64(), shape);
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {}
