//! Mask ตัวคน (DECISIONS ข้อ 40): ติดตั้งตัวแยกตัวคน → คิดภาพน้ำหนักของรูป → ตัวดูภาพ ภาพย่อ ตัวอย่างสด และสำเนาที่ส่งออก
//! ปรับเฉพาะตัวคน · preset ของ Lightroom ที่มี Mask "Select Subject" นำเข้าและใช้กับหลายรูปได้ (ตัวแยกจำลอง: ครึ่งขวาของภาพ)

mod common;

use common::*;
use serde_json::{json, Value};

/// ภาพเทากลาง 300×200 ในโฟลเดอร์ของกิจกรรม
fn grey_event(lib: &Lib, names: &[&str]) -> std::path::PathBuf {
    let root = lib.photos().join("2026-11-22 ตัวคน");
    std::fs::create_dir_all(&root).unwrap();
    let grey = image::RgbImage::from_pixel(300, 200, image::Rgb([90, 90, 90]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95).encode_image(&grey).unwrap();
    for name in names {
        std::fs::write(root.join(name), &bytes).unwrap();
    }
    lib.import(&root, "single_event", |_| true);
    root
}

/// ค่าสีเขียวที่ซ้ายและขวาของภาพ
fn sides(img: &image::RgbImage) -> (u8, u8) {
    let (w, h) = img.dimensions();
    (img.get_pixel(w / 4, h / 2)[1], img.get_pixel(w * 3 / 4, h / 2)[1])
}

fn asset_sides(lib: &Lib, path: &str) -> (u8, u8) {
    let a = puri_core::api::asset(&lib.app, path).unwrap();
    sides(&image::load_from_memory(&a.bytes).unwrap().to_rgb8())
}

fn subject_mask(exposure: f64) -> Value {
    json!({ "id": "person", "area": { "kind": "subject" }, "adjust": { "exposure": exposure }, "amount": 100, "invert": false })
}

#[test]
fn subject_mask_brightens_only_the_person_in_views_previews_and_exports() {
    let lib = Lib::with_subject(true);
    let root = grey_event(&lib, &["grey.jpg"]);
    let before = tree_state(&root);
    let id = lib.photo_id("grey.jpg");

    // ยังไม่ติดตั้ง: บอกให้ดาวน์โหลด · บันทึกค่าที่แต่งได้ แต่ภาพยังไม่มีผลของ Mask นี้ (และไม่ถูกเก็บลง cache)
    assert_eq!(lib.call("get_subject_status", json!({}))["state"], "not_installed");
    let e = lib.try_call("prepare_subject", json!({ "photoId": id })).unwrap_err();
    assert_eq!((e.code, e.next_action), ("MODEL_NOT_READY", Some("install_subject_model")));
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": { "masks": [subject_mask(1.0)] } }));
    let (left, right) = asset_sides(&lib, &format!("thumb/{id}"));
    assert!(left.abs_diff(90) <= 3 && right.abs_diff(90) <= 3, "ยังไม่มีภาพน้ำหนัก: {left} {right}");

    // ติดตั้ง (ดาวน์โหลดผ่าน file:// ตรวจ SHA-256) แล้วคิดภาพน้ำหนักของรูป
    let job = lib.install_subject_model();
    assert_eq!(job["state"], "completed", "{job}");
    assert_eq!(lib.call("get_subject_status", json!({}))["state"], "ready");
    assert_eq!(lib.call("prepare_subject", json!({ "photoId": id }))["found"], true);
    let maps: Vec<_> = walk(&lib.tmp.path().join("cache/subject"));
    assert_eq!(maps.len(), 1, "ภาพน้ำหนักเก็บใน cache ตาม revision: {maps:?}");

    // ตัวดูภาพและภาพย่อ: เฉพาะครึ่งขวา (ตัวคน) สว่างขึ้น — ภาพย่อที่ได้ก่อนติดตั้งไม่ได้ถูกเก็บไว้
    for kind in ["photo", "thumb"] {
        let (left, right) = asset_sides(&lib, &format!("{kind}/{id}"));
        assert!(left.abs_diff(90) <= 3 && right > 120, "{kind}: {left} {right}");
    }

    // ตัวอย่างสดของหน้าแต่งภาพ: ผลเดียวกัน
    let (w, h) = (60u32, 40u32);
    let rgba = puri_core::api::render_preview(
        &lib.app,
        json!({ "photoId": id, "source": "original", "settings": { "masks": [subject_mask(1.0)] }, "width": w, "height": h,
                "view": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "whole": true }),
    )
    .unwrap();
    let preview = image::RgbaImage::from_raw(w, h, rgba).unwrap();
    let preview = image::DynamicImage::ImageRgba8(preview).to_rgb8();
    let (left, right) = sides(&preview);
    assert!(left.abs_diff(90) <= 3 && right > 120, "ตัวอย่างสด: {left} {right}");

    // สำเนาที่ส่งออก: ผลเดียวกัน · ต้นฉบับไม่ถูกแตะ
    let dest = lib.tmp.path().join("ส่งออกตัวคน");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let out = image::load_from_memory(&std::fs::read(dest.join("grey.jpg")).unwrap()).unwrap().to_rgb8();
    let (left, right) = sides(&out);
    assert!(left.abs_diff(90) <= 3 && right > 120, "ส่งออก: {left} {right}");
    assert_eq!(tree_state(&root), before, "Mask ตัวคนไม่แตะไฟล์ต้นฉบับ");

    // ค่าที่เก็บไม่มีภาพน้ำหนัก (เป็นของรูป ไม่ใช่ของค่าที่แต่ง)
    let details = lib.call("get_photo_details", json!({ "photoId": id }));
    assert_eq!(details["edit"]["settings"]["masks"][0]["area"], json!({ "kind": "subject" }));
}

#[test]
fn a_tampered_download_is_rejected_and_export_refuses_to_drop_the_mask() {
    let lib = Lib::with_subject(true);
    std::fs::write(lib.tmp.path().join("model-source/subject.onnx"), b"tampered").unwrap();
    grey_event(&lib, &["grey.jpg"]);
    let id = lib.photo_id("grey.jpg");
    let job = lib.install_subject_model();
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(lib.call("get_subject_status", json!({}))["state"], "not_installed");

    // ส่งออกรูปที่มี Mask ตัวคนโดยยังไม่มีตัวแยก: รูปนั้นล้มเหลวพร้อมเหตุผล ไม่ได้สำเนาที่ขาดส่วนที่แต่งไว้
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": { "masks": [subject_mask(1.0)] } }));
    let dest = lib.tmp.path().join("ส่งออก");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let job = lib.call("get_job", json!({ "jobId": job["id"] }));
    assert_eq!(job["counts"]["failed"], 1, "{job}");
    assert_eq!(job["errorSummary"][0]["code"], "MODEL_NOT_READY");
    assert!(!dest.join("grey.jpg").exists());
}

#[test]
fn lightroom_subject_masks_come_in_with_presets_and_join_each_photos_own_masks() {
    let lib = Lib::with_subject(true);
    grey_event(&lib, &["a.jpg", "b.jpg"]);
    let (a, b) = (lib.photo_id("a.jpg"), lib.photo_id("b.jpg"));

    // preset แบบที่ Lightroom Classic เขียน (บริเวณเป็น attribute บน `<rdf:li/>`): ค่าแสงทั้งภาพ + Mask "Subject 1"
    // (Texture −35 Clarity −15 ความแรง 90%)
    let xmp = lib.tmp.path().join("Bright.xmp");
    std::fs::write(
        &xmp,
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
 <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:PresetType="Normal" crs:Exposure2012="+0.30" crs:Texture="-10">
   <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Bright test</rdf:li></rdf:Alt></crs:Name>
   <crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li><rdf:Description crs:What="Correction" crs:CorrectionAmount="0.9"
       crs:CorrectionActive="true" crs:CorrectionName="Mask 1" crs:CorrectionSyncID="0E4E" crs:LocalTexture="-0.35"
       crs:LocalClarity2012="-0.15"><crs:CorrectionMasks><rdf:Seq><rdf:li
         crs:What="Mask/Image" crs:MaskActive="true" crs:MaskName="Subject 1" crs:MaskBlendMode="0"
         crs:MaskInverted="false" crs:MaskValue="1" crs:MaskSubType="1"/>
       </rdf:Seq></crs:CorrectionMasks></rdf:Description></rdf:li></rdf:Seq>
   </crs:MaskGroupBasedCorrections>
 </rdf:Description></rdf:RDF></x:xmpmeta>"#,
    )
    .unwrap();
    let preset = lib.call("import_lightroom_preset", json!({ "path": xmp }));
    assert_eq!(preset["unsupported"], json!([]), "Mask ตัวคนนำเข้าได้ ไม่อยู่ในรายการที่ทำไม่ได้");
    assert_eq!(preset["masks"].as_array().unwrap().len(), 1);
    assert_eq!(preset["masks"][0]["area"], json!({ "kind": "subject" }));
    assert_eq!(preset["masks"][0]["amount"], 90.0);
    assert_eq!(preset["masks"][0]["adjust"]["texture"], -35.0);

    // รูป a มี Mask ใบหน้าของตัวเองอยู่แล้ว
    let face = json!({ "id": "face", "area": { "kind": "faces", "boxes": [[0.4, 0.3, 0.2, 0.3]], "skin": true },
                       "adjust": { "exposure": 0.3 }, "amount": 100, "invert": false });
    lib.call("save_photo_edit", json!({ "photoId": a, "settings": { "masks": [face] } }));

    // ใช้ preset กับสองรูป: Mask ของ preset ถูกเพิ่ม ของเดิมของแต่ละรูปอยู่ครบ · ใช้ซ้ำไม่ซ้อน
    let apply = |lib: &Lib| {
        lib.call(
            "apply_photo_edits",
            json!({ "selection": { "kind": "explicit", "photoIds": [a, b] },
                    "settings": { "adjust": preset["adjust"], "masks": preset["masks"] },
                    "parts": { "geometry": false, "light": true, "color": true, "addMasks": true } }),
        )
    };
    assert_eq!(apply(&lib)["updated"], 2);
    let masks = |id: &str| lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"]["masks"].clone();
    let kinds = |id: &str| -> Vec<String> {
        masks(id).as_array().unwrap().iter().map(|m| m["area"]["kind"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(kinds(&a), ["faces", "subject"]);
    assert_eq!(kinds(&b), ["subject"]);
    assert_eq!(apply(&lib)["unchanged"], 2, "ใช้ preset เดิมซ้ำ = ค่าเท่าเดิม");
    assert_eq!(kinds(&a), ["faces", "subject"]);

    // preset ที่ผู้ใช้บันทึกเก็บเฉพาะ Mask ตัวคน (Mask ใบหน้าผูกกับใบหน้าของรูปนั้น)
    let saved = lib.call("save_edit_preset", json!({ "name": "ตัวคนเนียน", "adjust": {}, "masks": masks(&a) }));
    assert_eq!(saved["masks"].as_array().unwrap().len(), 1);
    assert_eq!(saved["masks"][0]["area"]["kind"], "subject");
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

#[test]
fn the_assistant_adds_skin_and_subject_masks_and_takes_them_off_again() {
    let lib = Lib::with_subject(true);
    grey_event(&lib, &["a.jpg", "b.jpg"]);
    let (a, b) = (lib.photo_id("a.jpg"), lib.photo_id("b.jpg"));
    lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": a, "bbox": { "x": 0.6, "y": 0.2, "w": 0.2, "h": 0.3 }, "newPersonName": "คุณเทา" }),
    );
    let (tools, turn) = puri_core::assistant::test_tools(&lib.app).unwrap();
    let text = |out: &puri_core::assistant::mcp::ToolOutput| {
        out.content.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")
    };
    let masks = |id: &str| lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"]["masks"].clone();

    // ยังไม่มีตัวแยกตัวคน: บอก AI ให้บอกผู้ใช้ดาวน์โหลด · Mask ผิวใช้ได้เลย (รูปที่ไม่มีใบหน้าข้าม)
    let out = tools.call("add_mask", json!({ "photo_ids": [a], "area": "subject", "adjust": { "texture": -35 } }));
    assert!(out.is_error && text(&out).contains("ตัวคน (AI)"), "{}", text(&out));
    let out = tools.call("add_mask", json!({ "photo_ids": [a, b], "area": "skin", "adjust": { "exposure": 0.3 } }));
    assert_eq!(text(&out), "Added the skin mask to 1 photos. Skipped 1 photos without faces.");
    assert_eq!(masks(&a)[0]["area"]["kind"], "faces");
    assert_eq!(masks(&a)[0]["area"]["skin"], true);

    // ติดตั้งแล้ว: Mask ตัวคน · เรียกซ้ำ = แทนของเดิม (ไม่ซ้อน)
    lib.install_subject_model();
    for texture in [-20, -35] {
        let out = tools.call(
            "add_mask",
            json!({ "photo_ids": [a], "area": "subject", "adjust": { "texture": texture }, "amount": 90 }),
        );
        assert!(!out.is_error, "{}", text(&out));
    }
    let m = masks(&a);
    assert_eq!(m.as_array().unwrap().len(), 2, "{m}");
    assert_eq!(
        (m[1]["area"]["kind"].as_str(), m[1]["adjust"]["texture"].as_f64(), m[1]["amount"].as_f64()),
        (Some("subject"), Some(-35.0), Some(90.0))
    );
    let seen = tools.call("view_photo", json!({ "photo_id": a, "size": 256 }));
    assert!(!seen.is_error, "ดูรูปที่มี Mask ตัวคนได้ (คิดภาพน้ำหนักให้เอง)");
    assert!(
        tools.call("add_mask", json!({ "photo_ids": [a], "area": "subject", "adjust": {} })).is_error,
        "ไม่ได้ปรับอะไร"
    );

    // เอาออกเฉพาะ Mask ตัวคน แล้วย้อนทั้งรอบ = กลับเป็นไม่ได้แต่ง
    assert_eq!(
        text(&tools.call("remove_masks", json!({ "photo_ids": [a, b], "area": "subject" }))),
        "Removed masks from 1 photos."
    );
    assert_eq!(masks(&a).as_array().unwrap().len(), 1);
    lib.call("ai_undo", json!({ "turnId": turn }));
    assert!(lib.call("get_photo_details", json!({ "photoId": a }))["edit"].is_null());
}
