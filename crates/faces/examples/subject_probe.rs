//! ลองตัวแยกตัวคน (BiRefNet) กับรูปจริง: เวลา + ภาพซ้อน Mask (เครื่องมือของนักพัฒนา อ่านไฟล์อย่างเดียว เขียนเฉพาะในโฟลเดอร์ผล)
//!
//! ```text
//! cargo run --release -p puri-faces --example subject_probe -- <model.onnx> <โฟลเดอร์ผล> <รูป...>
//! ```

// ลิงก์ crate นี้ด้วย: build.rs ของมันลิงก์ runtime ของ clang ที่โค้ด CoreML ใน ONNX Runtime ต้องใช้
use puri_faces as _;

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn main() {
    use image::{imageops, GrayImage, Rgb};
    use ort::{session::Session, value::Tensor};
    use std::time::Instant;

    const SIDE: u32 = 1024;
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (model, out_dir, files) = (&args[0], std::path::PathBuf::from(&args[1]), &args[2..]);
    std::fs::create_dir_all(&out_dir).expect("สร้างโฟลเดอร์ผล");
    let env = ort::init().with_name("subject-probe").build().expect("ort");
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let started = Instant::now();
    // CPU เท่านั้น: ONNX Runtime บน Mac เปิด CoreML ให้เองถ้าไม่สั่ง PreferCPU — ลอง BiRefNet บน CoreML (GPU) แล้วเครื่องค้างจนรีบูต
    // (22 พ.ย. 69) และแบบ Neural Engine ก็แปลงโมเดลค้างนานเกินครึ่งชั่วโมง
    let threads = std::env::var("THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or(threads);
    let builder = Session::builder(&env)
        .expect("builder")
        .with_intra_threads(threads)
        .expect("threads")
        .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
        .expect("cpu");
    let mut builder = builder;
    let mut session = builder.commit_from_file(model).expect("โหลดโมเดล");
    println!("โหลด {:.1} วินาที", started.elapsed().as_secs_f64());
    for i in session.inputs() {
        println!("input {:?}", i);
    }
    for o in session.outputs() {
        println!("output {:?}", o);
    }
    for f in files {
        let bytes = std::fs::read(f).expect("อ่านรูป");
        let img = puri_core::media::decode_upright(&bytes, 1600).expect("ถอดรหัส").to_rgb8();
        let t = Instant::now();
        let small = imageops::resize(&img, SIDE, SIDE, imageops::FilterType::Triangle);
        let plane = (SIDE * SIDE) as usize;
        let mut input = vec![0f32; 3 * plane];
        for (x, y, p) in small.enumerate_pixels() {
            let i = (y * SIDE + x) as usize;
            for c in 0..3 {
                input[c * plane + i] = (f32::from(p[c]) / 255.0 - MEAN[c]) / STD[c];
            }
        }
        let tensor = Tensor::from_array(([1usize, 3, SIDE as usize, SIDE as usize], input)).expect("tensor");
        let outputs = session.run(ort::inputs![tensor]).expect("run");
        let last = outputs.len() - 1;
        let (shape, data) = outputs[last].try_extract_tensor::<f32>().expect("output");
        let n = data.len();
        let side = (n as f64).sqrt() as u32;
        let mask = GrayImage::from_fn(side, side, |x, y| {
            let v = data[(y * side + x) as usize];
            image::Luma([((1.0 / (1.0 + (-v).exp())) * 255.0).round() as u8])
        });
        let secs = t.elapsed().as_secs_f64();
        let mask = imageops::resize(&mask, img.width(), img.height(), imageops::FilterType::Triangle);
        let mut overlay = img.clone();
        for (x, y, p) in overlay.enumerate_pixels_mut() {
            let m = f32::from(mask.get_pixel(x, y)[0]) / 255.0;
            let tint = [255.0, 40.0, 40.0];
            *p = Rgb([0, 1, 2].map(|c| (f32::from(p[c]) * (1.0 - 0.45 * m) + tint[c] * 0.45 * m) as u8));
        }
        let name = std::path::Path::new(f).file_stem().unwrap_or_default().to_string_lossy().into_owned();
        imageops::thumbnail(&overlay, 900, 900).save(out_dir.join(format!("{name}.jpg"))).expect("เขียนผล");
        println!("{name}: {secs:.2} วินาที · output {:?}", shape);
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn main() {}
