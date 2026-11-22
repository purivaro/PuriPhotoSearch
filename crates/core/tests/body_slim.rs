//! ตัวผอม (DECISIONS ข้อ 41): ติดตั้งตัวแยกตัวคน + ตัวเติมฉากหลัง → เตรียมรูป → ตัวดูภาพ ตัวอย่างสด และสำเนาที่ส่งออก
//! ลำตัวแคบลง หัวและฉากหลังเท่าเดิม ต้นฉบับไม่ถูกแตะ (ตัวแยกจำลอง: ส่วนที่เป็นสีแดงสด · ตัวเติมจำลอง: สีฟ้าที่ถูกปรับให้ต่อกับฉากจริง)

mod common;

use common::*;
use serde_json::{json, Value};

/// ฉากหลังไล่สีเขียวตามแนวนอน
fn background(x: u32) -> image::Rgb<u8> {
    image::Rgb([40, (30 + x * 200 / 300) as u8, 200])
}

/// รูป 300×400: คนสีแดง หัวกลม (กลาง 150,70 รัศมี 30) ลำตัวกว้าง 80 (x 110–190, y 100–390)
fn person_photo() -> image::RgbImage {
    image::RgbImage::from_fn(300, 400, |x, y| {
        let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
        let head = (fx - 150.0).powi(2) + (fy - 70.0).powi(2) <= 30.0f64.powi(2);
        let torso = (110.0..190.0).contains(&fx) && (100.0..390.0).contains(&fy);
        if head || torso {
            image::Rgb([225, 25, 25])
        } else {
            background(x)
        }
    })
}

fn person_event(lib: &Lib) -> std::path::PathBuf {
    let root = lib.photos().join("2026-11-22 ตัวผอม");
    std::fs::create_dir_all(&root).unwrap();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 97).encode_image(&person_photo()).unwrap();
    std::fs::write(root.join("person.jpg"), &bytes).unwrap();
    lib.import(&root, "single_event", |_| true);
    root
}

fn red(p: &image::Rgb<u8>) -> bool {
    p[0] > 150 && p[1] < 100
}

/// ความกว้างของส่วนสีแดงที่แถว `y` (สัดส่วนของความสูง) เป็นสัดส่วนของความกว้างภาพ
fn red_width(img: &image::RgbImage, y: f64) -> f64 {
    let row = (y * img.height() as f64) as u32;
    (0..img.width()).filter(|&x| red(img.get_pixel(x, row))).count() as f64 / img.width() as f64
}

fn asset(lib: &Lib, path: &str) -> image::RgbImage {
    let a = puri_core::api::asset(&lib.app, path).unwrap();
    image::load_from_memory(&a.bytes).unwrap().to_rgb8()
}

fn slim(amount: f64) -> Value {
    json!({ "retouch": { "faces": [], "body": amount } })
}

/// ลำตัวเดิมกว้าง 80/300 · หัวเดิมกว้าง 60/300
const TORSO: f64 = 80.0 / 300.0;
const HEAD: f64 = 60.0 / 300.0;

#[test]
fn body_slim_narrows_the_torso_in_views_previews_and_exports_and_keeps_the_head_and_background() {
    let lib = Lib::with_subject(true);
    let root = person_event(&lib);
    let before = tree_state(&root);
    let id = lib.photo_id("person.jpg");

    // ยังไม่ติดตั้ง: ตัวเติมฉากหลังบอกให้ดาวน์โหลด · บันทึกได้ แต่ภาพยังไม่หด
    let status = lib.call("get_subject_status", json!({}));
    assert_eq!(
        (status["state"].as_str(), status["fill"]["state"].as_str()),
        (Some("not_installed"), Some("not_installed"))
    );
    let e = lib.try_call("prepare_subject", json!({ "photoId": id, "fill": true })).unwrap_err();
    assert_eq!(e.code, "MODEL_NOT_READY");
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": slim(80.0) }));
    let plain = asset(&lib, &format!("photo/{id}"));
    assert!((red_width(&plain, 0.75) - TORSO).abs() < 0.01, "ยังไม่มีฉากหลังที่เติม: ยังไม่หด");

    // ติดตั้งทั้งสองตัวด้วยคำสั่งเดียว (ดาวน์โหลดผ่าน file:// ตรวจ SHA-256)
    let job = lib.call("install_subject_model", json!({ "fill": true }));
    lib.run_jobs();
    assert_eq!(lib.call("get_job", json!({ "jobId": job["id"] }))["state"], "completed");
    let status = lib.call("get_subject_status", json!({}));
    assert_eq!(
        (status["state"].as_str(), status["fill"]["state"].as_str()),
        (Some("ready"), Some("ready")),
        "{status}"
    );
    assert_eq!(lib.call("prepare_subject", json!({ "photoId": id, "fill": true }))["found"], true);
    let cached: Vec<_> = walk(&lib.tmp.path().join("cache/subject"));
    assert_eq!(cached.len(), 2, "ภาพน้ำหนัก + ฉากหลังที่เติม เก็บใน cache ตาม revision: {cached:?}");

    // ตัวดูภาพ: ลำตัวแคบลงราว 9.6% (ระดับ 80) หัวเท่าเดิม ฉากหลังไกล ๆ เท่าเดิม ตรงที่หดออกเป็นฉากหลัง (ไม่ใช่สีแดง)
    let check = |img: &image::RgbImage, what: &str| {
        let torso = red_width(img, 0.75);
        assert!(torso < TORSO * 0.94 && torso > TORSO * 0.86, "{what}: ลำตัว {torso} เดิม {TORSO}");
        let head = red_width(img, 70.0 / 400.0);
        assert!((head - HEAD).abs() < 0.015, "{what}: หัว {head} เดิม {HEAD}");
        let (w, h) = img.dimensions();
        let at = |x: f64, y: f64| *img.get_pixel((x * w as f64) as u32, (y * h as f64) as u32);
        for x in [0.1, 0.25, 0.75, 0.9] {
            let (p, want) = (at(x, 0.75), background((x * 300.0) as u32));
            assert!(p.0.iter().zip(want.0).all(|(a, b)| a.abs_diff(b) <= 8), "{what}: ฉากหลังที่ x={x} {p:?} {want:?}");
        }
        // ขอบซ้ายเดิมของลำตัว (x 111/300): หดออกแล้วเป็นฉากหลังที่ต่อกับฉากจริง
        let gap = at(112.0 / 300.0, 0.75);
        assert!(!red(&gap) && gap[2] > 170 && gap[1].abs_diff(background(112)[1]) <= 25, "{what}: ช่องที่หดออก {gap:?}");
    };
    check(&asset(&lib, &format!("photo/{id}")), "ตัวดูภาพ");

    // ตัวอย่างสดของหน้าแต่งภาพ (ครึ่งขนาด): ผลเดียวกัน
    let (w, h) = (150u32, 200u32);
    let rgba = puri_core::api::render_preview(
        &lib.app,
        json!({ "photoId": id, "source": "original", "settings": slim(80.0), "width": w, "height": h,
                "view": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "whole": true }),
    )
    .unwrap();
    let preview = image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(w, h, rgba).unwrap()).to_rgb8();
    check(&preview, "ตัวอย่างสด");

    // สำเนาที่ส่งออก: ผลเดียวกัน · ต้นฉบับไม่ถูกแตะ
    let dest = lib.tmp.path().join("ส่งออกตัวผอม");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let out = image::load_from_memory(&std::fs::read(dest.join("person.jpg")).unwrap()).unwrap().to_rgb8();
    check(&out, "ส่งออก");
    assert_eq!(tree_state(&root), before, "ตัวผอมไม่แตะไฟล์ต้นฉบับ");

    // ค่าที่เก็บมีแค่ระดับ (ภาพน้ำหนักและฉากหลังเป็นของรูป ไม่ใช่ของค่าที่แต่ง)
    let details = lib.call("get_photo_details", json!({ "photoId": id }));
    assert_eq!(
        details["edit"]["settings"]["retouch"],
        json!({ "faces": [], "smooth": 0.0, "wrinkles": 0.0, "slim": 0.0, "thin": 0.0, "body": 80.0 })
    );
}

#[test]
fn export_refuses_body_slim_without_the_fill_model() {
    let lib = Lib::with_subject(true);
    std::fs::write(lib.tmp.path().join("model-source/fill.onnx"), b"tampered").unwrap();
    person_event(&lib);
    let id = lib.photo_id("person.jpg");
    // ตัวแยกตัวคนติดตั้งได้ ตัวเติมฉากหลังไฟล์ไม่ตรง: งานของตัวเติมล้มเหลว
    let job = lib.call("install_subject_model", json!({ "fill": true }));
    lib.run_jobs();
    assert_eq!(lib.call("get_job", json!({ "jobId": job["id"] }))["state"], "failed");
    let status = lib.call("get_subject_status", json!({}));
    assert_eq!((status["state"].as_str(), status["fill"]["state"].as_str()), (Some("ready"), Some("not_installed")));

    lib.call("save_photo_edit", json!({ "photoId": id, "settings": slim(50.0) }));
    let dest = lib.tmp.path().join("ส่งออก");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let job = lib.call("get_job", json!({ "jobId": job["id"] }));
    assert_eq!(job["counts"]["failed"], 1, "{job}");
    assert_eq!(job["errorSummary"][0]["code"], "MODEL_NOT_READY");
    assert!(!dest.join("person.jpg").exists(), "ไม่ได้สำเนาที่ขาดตัวผอมโดยไม่รู้ตัว");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
