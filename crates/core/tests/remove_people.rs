//! ลบคน/สิ่งของ (DECISIONS ข้อ 42): ลากกรอบรอบคนที่ไม่ต้องการ → แอปหาคนในกรอบแล้วเติมฉากหลัง — ตัวดูภาพ ตัวอย่างสด และสำเนาที่ส่งออก
//! คนในกรอบหายไป คนอื่นและฉากหลังเท่าเดิม ต้นฉบับไม่ถูกแตะ (ตัวแยกจำลอง: ส่วนที่เป็นสีแดงสด · ตัวเติมจำลอง: สีที่ถูกปรับให้ต่อกับฉากจริง)

mod common;

use common::*;
use serde_json::{json, Value};

/// ฉากหลังไล่สีเขียวตามแนวนอน
fn background(x: u32) -> image::Rgb<u8> {
    image::Rgb([40, (30 + x * 200 / 300) as u8, 200])
}

/// รูป 300×200: คนสีแดงสองคน (x 60–100 และ 200–240, y 40–190)
fn two_people() -> image::RgbImage {
    image::RgbImage::from_fn(300, 200, |x, y| {
        if ((60..100).contains(&x) || (200..240).contains(&x)) && (40..190).contains(&y) {
            image::Rgb([225, 25, 25])
        } else {
            background(x)
        }
    })
}

fn event(lib: &Lib) -> std::path::PathBuf {
    let root = lib.photos().join("2026-11-22 ลบคน");
    std::fs::create_dir_all(&root).unwrap();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 97).encode_image(&two_people()).unwrap();
    std::fs::write(root.join("two.jpg"), &bytes).unwrap();
    lib.import(&root, "single_event", |_| true);
    root
}

fn red(p: &image::Rgb<u8>) -> bool {
    p[0] > 150 && p[1] < 100
}

/// สัดส่วนของพิกเซลสีแดงในช่วง x (สัดส่วนของความกว้าง) ที่แถวกลางภาพ
fn red_share(img: &image::RgbImage, x0: f64, x1: f64) -> f64 {
    let (w, h) = img.dimensions();
    let xs = (x0 * w as f64) as u32..(x1 * w as f64) as u32;
    let n = xs.len() as f64;
    xs.filter(|&x| red(img.get_pixel(x, h / 2))).count() as f64 / n
}

fn asset(lib: &Lib, path: &str) -> image::RgbImage {
    let a = puri_core::api::asset(&lib.app, path).unwrap();
    image::load_from_memory(&a.bytes).unwrap().to_rgb8()
}

/// กรอบรอบคนขวา (ล้นตัวคนเล็กน้อย)
const RIGHT: [f64; 4] = [190.0 / 300.0, 30.0 / 200.0, 60.0 / 300.0, 165.0 / 200.0];

fn removing_right(extra: Value) -> Value {
    let mut s = json!({ "removals": [{ "id": "r1", "rect": RIGHT }] });
    s.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    s
}

#[test]
fn the_person_in_the_box_disappears_in_views_previews_and_exports() {
    let lib = Lib::with_subject(true);
    let root = event(&lib);
    let before = tree_state(&root);
    let id = lib.photo_id("two.jpg");

    // ยังไม่ติดตั้ง: คิดไม่ได้ บอกให้ดาวน์โหลด · บันทึกได้ แต่ภาพยังไม่เปลี่ยน
    let e = lib
        .try_call("prepare_removals", json!({ "photoId": id, "removals": [{ "id": "r1", "rect": RIGHT }] }))
        .unwrap_err();
    assert_eq!((e.code, e.next_action), ("MODEL_NOT_READY", Some("install_subject_model")));
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": removing_right(json!({})) }));
    let plain = asset(&lib, &format!("photo/{id}"));
    assert!(red_share(&plain, 0.68, 0.79) > 0.9, "ยังไม่มีภาพที่เติม: คนขวายังอยู่");

    let job = lib.call("install_subject_model", json!({ "fill": true }));
    lib.run_jobs();
    assert_eq!(lib.call("get_job", json!({ "jobId": job["id"] }))["state"], "completed");
    assert_eq!(
        lib.call("prepare_removals", json!({ "photoId": id, "removals": [{ "id": "r1", "rect": RIGHT }] }))["found"],
        json!([true])
    );

    let check = |img: &image::RgbImage, what: &str| {
        assert!(red_share(img, 0.68, 0.79) < 0.02, "{what}: คนขวาหายไป");
        assert!(red_share(img, 0.21, 0.32) > 0.95, "{what}: คนซ้ายอยู่ครบ");
        let (w, h) = img.dimensions();
        for x in [0.05, 0.5, 0.95] {
            let (p, want) = (img.get_pixel((x * w as f64) as u32, h / 2), background((x * 300.0) as u32));
            assert!(p.0.iter().zip(want.0).all(|(a, b)| a.abs_diff(b) <= 8), "{what}: ฉากหลังที่ x={x} {p:?}");
        }
        // ตรงที่ลบเป็นฉากหลังที่ต่อกับฉากจริง
        let p = img.get_pixel((0.733 * w as f64) as u32, h / 2);
        assert!(p[2] > 170 && p[1].abs_diff(background(220)[1]) <= 30, "{what}: ตรงที่ลบ {p:?}");
    };
    check(&asset(&lib, &format!("photo/{id}")), "ตัวดูภาพ");

    let (w, h) = (150u32, 100u32);
    let rgba = puri_core::api::render_preview(
        &lib.app,
        json!({ "photoId": id, "source": "original", "settings": removing_right(json!({})), "width": w, "height": h,
                "view": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "whole": true }),
    )
    .unwrap();
    check(&image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(w, h, rgba).unwrap()).to_rgb8(), "ตัวอย่างสด");

    let dest = lib.tmp.path().join("ส่งออกลบคน");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    check(&image::load_from_memory(&std::fs::read(dest.join("two.jpg")).unwrap()).unwrap().to_rgb8(), "ส่งออก");
    assert_eq!(tree_state(&root), before, "ลบคนไม่แตะไฟล์ต้นฉบับ");

    // ค่าที่เก็บมีแค่กรอบ
    let details = lib.call("get_photo_details", json!({ "photoId": id }));
    assert_eq!(details["edit"]["settings"]["removals"], json!([{ "id": "r1", "rect": RIGHT }]));
}

#[test]
fn a_people_mask_does_not_bring_back_a_removed_person() {
    let lib = Lib::with_subject(true);
    event(&lib);
    let id = lib.photo_id("two.jpg");
    lib.call("install_subject_model", json!({ "fill": true }));
    lib.run_jobs();
    let brighter = json!({ "masks": [{ "id": "p", "area": { "kind": "subject" }, "adjust": { "exposure": 1.5 }, "amount": 100, "invert": false }] });
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": removing_right(brighter) }));
    let img = asset(&lib, &format!("photo/{id}"));
    // คนซ้ายสว่างขึ้น · ตรงที่ลบคนขวาไม่สว่างขึ้น (ไม่เห็นเงาของคนที่ลบ)
    let (w, h) = img.dimensions();
    let left = img.get_pixel((0.27 * w as f64) as u32, h / 2);
    assert!(left[1] > 35 && left[0] > 240, "คนซ้ายสว่างขึ้น (เดิม 225, 25, 25) {left:?}");
    let gone = img.get_pixel((0.733 * w as f64) as u32, h / 2);
    assert!(gone[1].abs_diff(background(220)[1]) <= 30 && gone[2] <= 215, "ตรงที่ลบไม่ถูกทำให้สว่าง {gone:?}");
}

#[test]
fn the_assistant_reads_the_faces_removes_people_and_slims_bodies() {
    let lib = Lib::with_subject(true);
    event(&lib);
    let id = lib.photo_id("two.jpg");
    // ใบหน้าของคนขวา (ตั้งชื่อไว้) ให้ AI เห็นใน get_photo
    lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id, "bbox": { "x": 0.68, "y": 0.21, "w": 0.1, "h": 0.12 }, "newPersonName": "คนเดินผ่าน" }),
    );
    let (tools, _turn) = puri_core::assistant::test_tools(&lib.app).unwrap();
    let text = |out: &puri_core::assistant::mcp::ToolOutput| {
        out.content.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")
    };
    let info = text(&tools.call("get_photo", json!({ "photo_id": id })));
    let face_id =
        lib.call("get_photo_details", json!({ "photoId": id }))["faces"][0]["id"].as_str().unwrap().to_string();
    assert!(info.contains(&format!("- {face_id}: คนเดินผ่าน [0.680, 0.210, 0.100, 0.120]")), "{info}");

    // ยังไม่มีโมเดล: บอก AI ให้บอกผู้ใช้ดาวน์โหลดเอง
    let out = tools.call("remove_people", json!({ "photo_id": id, "face_ids": [face_id] }));
    assert!(out.is_error && text(&out).contains("ลบคนหรือสิ่งของ"), "{}", text(&out));
    assert!(tools.call("slim_body", json!({ "photo_ids": [id], "amount": 50 })).is_error);

    lib.call("install_subject_model", json!({ "fill": true }));
    lib.run_jobs();
    // ลบคนขวาจากใบหน้า (จุดยึด) แล้วดูรูป: คนขวาหาย คนซ้ายอยู่
    let out = tools.call("remove_people", json!({ "photo_id": id, "face_ids": [face_id] }));
    assert!(!out.is_error, "{}", text(&out));
    assert!(!tools.call("view_photo", json!({ "photo_id": id, "size": 256 })).is_error);
    let img = asset(&lib, &format!("photo/{id}"));
    assert!(red_share(&img, 0.68, 0.79) < 0.02, "คนขวาหายไป");
    assert!(red_share(&img, 0.21, 0.32) > 0.95, "คนซ้ายอยู่ครบ");
    let removals = lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"]["removals"].clone();
    assert_eq!(removals.as_array().unwrap().len(), 1);
    assert!(removals[0]["anchor"].is_array(), "ลบจากใบหน้ามีจุดยึด {removals}");

    // กรอบบนภาพเดิม + ตัวผอม
    let out = tools.call("remove_people", json!({ "photo_id": id, "boxes": [[0.18, 0.15, 0.17, 0.83]] }));
    assert!(!out.is_error && text(&out).contains("2 in total"), "{}", text(&out));
    assert!(red_share(&asset(&lib, &format!("photo/{id}")), 0.21, 0.32) < 0.02, "คนซ้ายหายด้วย");
    assert!(tools.call("remove_people", json!({ "photo_id": id, "face_ids": ["ไม่มี"] })).is_error);
    let out = tools.call("slim_body", json!({ "photo_ids": [id], "amount": 50 }));
    assert!(!out.is_error, "{}", text(&out));
    let settings = lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"].clone();
    assert_eq!(settings["retouch"]["body"], 50.0);
    assert_eq!(settings["removals"].as_array().unwrap().len(), 2, "ตัวผอมไม่ลบส่วนที่ลบไว้");
}
