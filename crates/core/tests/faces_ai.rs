//! วิเคราะห์ใบหน้าอัตโนมัติ: ติดตั้งโมเดล, pipeline, ข้อเสนอของ AI, คิวตรวจสอบ และการนำรายชื่อจากรุ่น Python เข้า
//! ใช้ตัววิเคราะห์จำลอง (`common::FakeAnalyzer`) — เทสต์นี้พิสูจน์กติกาข้อมูลรอบ ๆ AI ไม่ใช่ความแม่นยำของโมเดล
//! "บุคคล" ของรูปทดสอบคือค่า seed ÷ 20 (seed 0 = รูปที่ไม่มีใบหน้า)

mod common;

use common::*;
use serde_json::{json, Value};

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array")
}

/// งานเดียว: A ×3 (seed 40), B ×2 (seed 80), C ×1 (seed 120), ไม่มีใบหน้า ×1
fn party(lib: &Lib) -> Value {
    let root = lib.photos().join("2026-11-18 งานเลี้ยง");
    for (name, seed) in [
        ("a1.jpg", 40),
        ("a2.jpg", 40),
        ("a3.jpg", 40),
        ("b1.jpg", 80),
        ("b2.jpg", 80),
        ("c1.jpg", 120),
        ("empty.jpg", 0),
    ] {
        write_jpeg(&root.join(name), seed);
    }
    lib.import(&root, "single_event", |_| true)
}

fn faces_of(lib: &Lib, filename: &str) -> Vec<Value> {
    arr(&lib.call("get_photo_details", json!({ "photoId": lib.photo_id(filename) }))["faces"]).clone()
}

#[test]
fn model_is_optional_then_one_click_install_analyses_existing_photos() {
    let lib = Lib::with_ai();
    let caps = lib.call("get_capabilities", json!({}));
    assert_eq!(caps["faceAnalysis"]["state"], "not_installed");
    assert_eq!(caps["faceAnalysis"]["available"], false);
    assert!(caps["faceAnalysis"]["licenseNote"].as_str().is_some_and(|s| !s.is_empty()));

    // ไม่มีโมเดล: เพิ่มโฟลเดอร์และดูรูปได้ครบ; รูป "ยังไม่ได้วิเคราะห์" ≠ "ไม่พบใบหน้า" (T09)
    let import = party(&lib);
    let page = lib.query(json!({}));
    assert_eq!(page["total"], 7);
    assert_eq!(page["coverage"]["thumbnailsReady"], 7);
    assert_eq!(
        (page["coverage"]["facesAnalyzed"].as_i64(), page["coverage"]["facesPending"].as_i64()),
        (Some(0), Some(7))
    );
    assert_eq!(lib.try_call("analyze_faces", json!({})).unwrap_err().code, "MODEL_NOT_READY");
    assert_eq!(lib.call("get_review_overview", json!({}))["photosPendingAnalysis"], 7);

    // ติดตั้งด้วยคำสั่งเดียว: ดาวน์โหลด → แตกเฉพาะไฟล์ที่ใช้ → ตรวจ checksum → วิเคราะห์รูปที่มีอยู่ต่อเอง
    let first = lib.call("install_face_model", json!({}));
    assert_eq!(lib.call("install_face_model", json!({}))["id"], first["id"], "กดซ้ำได้งานเดิม");
    assert_eq!(lib.call("get_capabilities", json!({}))["faceAnalysis"]["state"], "installing");
    lib.run_jobs();
    let job = lib.call("get_job", json!({ "jobId": first["id"] }));
    assert_eq!(job["state"], "completed", "{job}");
    assert_eq!(job["kind"], "model");
    let caps = lib.call("get_capabilities", json!({}));
    assert_eq!(
        (caps["faceAnalysis"]["state"].as_str(), caps["faceAnalysis"]["available"].as_bool()),
        (Some("ready"), Some(true))
    );
    let model_dir = std::path::PathBuf::from(caps["faceAnalysis"]["modelPath"].as_str().unwrap());
    let mut files: Vec<String> =
        std::fs::read_dir(&model_dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(files, ["det.onnx", "rec.onnx", "verified"], "เก็บเฉพาะไฟล์ที่ใช้ ไม่เหลือไฟล์ดาวน์โหลด");

    let event = lib.call("get_event", json!({ "eventId": import["eventIds"][0] }));
    assert_eq!(event["faceAnalyzedCount"], 7, "รูปเดิมถูกวิเคราะห์ต่อเองหลังติดตั้ง");
    let page = lib.query(json!({}));
    assert_eq!(
        (page["coverage"]["facesAnalyzed"].as_i64(), page["coverage"]["facesPending"].as_i64()),
        (Some(7), Some(0))
    );
    assert_eq!(lib.count("SELECT COUNT(*) FROM faces WHERE origin = 'detector'"), 6);
    assert_eq!(faces_of(&lib, "empty.jpg").len(), 0, "วิเคราะห์แล้ว ไม่พบใบหน้า — ไม่ใช่ข้อผิดพลาด");
    assert_eq!(lib.count("SELECT COUNT(*) FROM job_errors"), 0);
    assert_eq!(lib.call("get_library_summary", json!({}))["unnamedFaceCount"], 6);

    // สแกนซ้ำ/เปิดใหม่: ไม่วิเคราะห์ซ้ำ ไม่เกิดใบหน้าซ้ำ และสถานะโมเดลยังพร้อม
    lib.call("rescan_event", json!({ "eventId": import["eventIds"][0] }));
    lib.run_jobs();
    let lib = lib.reopen();
    lib.run_jobs();
    assert_eq!(lib.count("SELECT COUNT(*) FROM faces"), 6);
    assert_eq!(lib.count("SELECT COUNT(*) FROM face_embeddings"), 6);
    assert_eq!(lib.call("get_capabilities", json!({}))["faceAnalysis"]["state"], "ready");

    // รูปที่เพิ่มหลังติดตั้งถูกวิเคราะห์ในงานเดียวกับการเตรียมข้อมูลรูป
    write_jpeg(&lib.photos().join("2026-11-18 งานเลี้ยง/a4.jpg"), 40);
    lib.call("rescan_event", json!({ "eventId": import["eventIds"][0] }));
    lib.run_jobs();
    assert_eq!(faces_of(&lib, "a4.jpg").len(), 1);
}

#[test]
fn corrupt_download_is_refused_and_can_be_retried() {
    let lib = Lib::with_ai();
    party(&lib);
    publish_fake_model(lib.tmp.path(), true); // ไฟล์ในแหล่งดาวน์โหลดไม่ตรง checksum ที่แอปรู้จัก
    let job = lib.install_model();
    assert_eq!(job["state"], "failed", "{job}");
    assert!(job["blockedReason"].as_str().unwrap().contains("checksum"));
    let caps = lib.call("get_capabilities", json!({}));
    assert_eq!(caps["faceAnalysis"]["state"], "not_installed", "โมเดลที่ตรวจไม่ผ่านต้องไม่ถูกใช้");
    assert_eq!(lib.count("SELECT COUNT(*) FROM faces"), 0);

    publish_fake_model(lib.tmp.path(), false);
    assert_eq!(lib.install_model()["state"], "completed");
    assert_eq!(lib.call("get_capabilities", json!({}))["faceAnalysis"]["state"], "ready");
    assert_eq!(lib.count("SELECT COUNT(*) FROM faces"), 6);
}

#[test]
fn suggestions_are_never_confirmations_and_review_acts_only_on_selected_faces() {
    let lib = Lib::with_ai();
    party(&lib);
    lib.install_model();

    // ยังไม่มีใครมีชื่อ: ไม่มีข้อเสนอ มีแต่กลุ่มใบหน้าที่ยังไม่ทราบชื่อ (กลุ่ม ≥ 2 ใบ) และใบเดี่ยว
    let overview = lib.call("get_review_overview", json!({}));
    assert_eq!(overview["unnamedFaceCount"], 6);
    assert_eq!(arr(&overview["suggested"]).len(), 0);
    let sizes: Vec<i64> = arr(&overview["clusters"]).iter().map(|c| c["faceCount"].as_i64().unwrap()).collect();
    assert_eq!(sizes, [3, 2], "A สามใบ, B สองใบ");
    assert_eq!(overview["singleFaceCount"], 1);
    assert_eq!(lib.count("SELECT COUNT(*) FROM persons"), 0, "กลุ่มใบหน้าไม่ใช่บุคคล");

    // ตั้งชื่อใบเดียวของกลุ่ม A จากตัวดูภาพ (ไม่ต้องลากกรอบเอง)
    let a1 = faces_of(&lib, "a1.jpg");
    assert_eq!(
        (a1[0]["origin"].as_str(), a1[0]["person"].is_null(), a1[0]["suggestion"].is_null()),
        (Some("detector"), true, true)
    );
    let named =
        lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [a1[0]["id"]], "newPersonName": "คุณกานต์" }));
    let kan = named["createdPerson"]["id"].as_str().unwrap().to_string();

    // ใบที่เหลือของ A ได้ "ข้อเสนอ" — ยังไม่นับเป็นรูปของคุณกานต์ และไม่อยู่ในผลค้นแบบยืนยันแล้ว (T18)
    let a2 = faces_of(&lib, "a2.jpg");
    assert!(a2[0]["person"].is_null());
    assert_eq!(a2[0]["suggestion"]["person"]["id"], kan.as_str());
    assert_eq!(a2[0]["suggestion"]["level"], "close");
    assert!(faces_of(&lib, "b1.jpg")[0]["suggestion"].is_null(), "คนอื่นไม่ถูกเสนอเป็นคุณกานต์");
    assert_eq!(lib.filenames(json!({ "personIds": [kan] })), ["a1.jpg"]);
    assert_eq!(lib.call("get_person", json!({ "personId": kan }))["confirmedPhotoCount"], 1);

    let overview = lib.call("get_review_overview", json!({}));
    let group = &overview["suggested"][0];
    assert_eq!((group["person"]["id"].as_str(), group["faceCount"].as_i64()), (Some(kan.as_str()), Some(2)));
    let listed = lib
        .call("get_review_faces", json!({ "kind": "person", "id": kan, "reviewRevision": overview["reviewRevision"] }));
    assert_eq!(listed["total"], 2);
    let mut names: Vec<&str> = arr(&listed["faces"]).iter().map(|f| f["filename"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["a2.jpg", "a3.jpg"]);

    // ยืนยันเฉพาะใบที่เลือก (a2): a3 ต้องยังเป็นข้อเสนอ (S16 Acceptance 1)
    let a2_face = arr(&listed["faces"]).iter().find(|f| f["filename"] == "a2.jpg").unwrap()["faceId"].clone();
    let a3_face = arr(&listed["faces"]).iter().find(|f| f["filename"] == "a3.jpg").unwrap()["faceId"].clone();
    lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [a2_face], "personId": kan }));
    assert_eq!(lib.filenames(json!({ "personIds": [kan] })), ["a1.jpg", "a2.jpg"]);
    assert!(faces_of(&lib, "a3.jpg")[0]["person"].is_null());

    // รายการของ revision เก่าใช้ไม่ได้แล้ว: UI ต้องโหลดคิวใหม่ ไม่ทำงานกับรายการที่ผู้ใช้ไม่ได้เห็น
    let stale = lib.try_call(
        "get_review_faces",
        json!({ "kind": "person", "id": kan, "reviewRevision": overview["reviewRevision"] }),
    );
    assert_eq!(stale.unwrap_err().code, "PREVIEW_STALE");

    // "ไม่ใช่บุคคลนี้" ต่อข้อเสนอ: ไม่เสนอคู่นี้อีก แม้เปิดแอปใหม่; ใบหน้า รูป และบุคคลยังอยู่ (T19)
    lib.call("reject_suggestions", json!({ "faceIds": [a3_face], "personId": kan }));
    let lib = lib.reopen();
    let a3 = faces_of(&lib, "a3.jpg");
    assert_eq!(a3.len(), 1);
    assert!(a3[0]["suggestion"].is_null());
    assert_eq!(arr(&lib.call("get_review_overview", json!({}))["suggested"]).len(), 0);
    assert_eq!(lib.query(json!({}))["total"], 7);
    // ยังตั้งชื่อใบนั้นเป็นคนที่ถูกได้
    lib.call(
        "assign_faces",
        json!({ "requestId": new_id(), "faceIds": [a3[0]["id"]], "newPersonName": "คุณกานต์ (อีกคน)" }),
    );
    assert_eq!(arr(&lib.call("list_people", json!({ "text": "กานต์" }))).len(), 2);

    // "ไม่ใช่ใบหน้า" เอาออกจากคิว และไม่ถูกตรวจพบกลับมาเมื่อสแกนซ้ำ
    let c1 = faces_of(&lib, "c1.jpg");
    lib.call("ignore_faces", json!({ "faceIds": [c1[0]["id"]] }));
    let events = lib.call("list_events", json!({}));
    lib.call("rescan_event", json!({ "eventId": events[0]["id"] }));
    lib.run_jobs();
    assert_eq!(faces_of(&lib, "c1.jpg").len(), 0);
    assert_eq!(lib.call("get_library_summary", json!({}))["unnamedFaceCount"], 2, "เหลือ B สองใบ");
}

/// ระหว่างที่งานวิเคราะห์ยังวิ่ง (มีใบหน้าใหม่เข้ามาเรื่อย ๆ) รายการที่ผู้ใช้เปิดดูอยู่ต้องยังใช้ได้
/// มีแต่การตัดสินของผู้ใช้ที่ทำให้รายการนั้นเก่า
#[test]
fn ai_matches_count_as_the_persons_photos_until_the_user_says_no() {
    let lib = Lib::with_ai();
    party(&lib);
    lib.install_model();
    let a1 = faces_of(&lib, "a1.jpg");
    let named =
        lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [a1[0]["id"]], "newPersonName": "คุณกานต์" }));
    let kan = named["createdPerson"]["id"].as_str().unwrap().to_string();

    // หน้าบุคคล: รูปที่ AI จัดให้ถูกนับแยกจากรูปที่ยืนยันแล้ว
    let person = lib.call("get_person", json!({ "personId": kan }));
    assert_eq!((person["confirmedPhotoCount"].as_i64(), person["aiPhotoCount"].as_i64()), (Some(1), Some(2)));
    assert_eq!(arr(&lib.call("list_people", json!({})))[0]["aiPhotoCount"], 2);

    // ค้นหา: ไม่ขอรวม = เฉพาะที่ยืนยันแล้ว; ขอรวม = ได้รูปที่ AI จัดให้ด้วย และบอกว่าใบหน้าไหนเป็นเหตุ
    assert_eq!(lib.filenames(json!({ "personIds": [kan] })), ["a1.jpg"]);
    let with_ai = json!({ "personIds": [kan], "includeSuggested": true, "sort": "filename" });
    let page = lib.query(with_ai.clone());
    assert_eq!(names(&page), ["a1.jpg", "a2.jpg", "a3.jpg"]);
    let items = arr(&page["items"]);
    assert!(arr(&items[0]["suggestedPeople"]).is_empty(), "รูปที่ยืนยันแล้วไม่ถูกบอกว่าเป็นของ AI");
    assert_eq!(items[0]["confirmedPeople"][0]["id"], kan.as_str());
    assert_eq!(items[1]["suggestedPeople"][0]["person"]["id"], kan.as_str());
    let a2_face = items[1]["suggestedPeople"][0]["faceId"].clone();
    assert_eq!(a2_face, faces_of(&lib, "a2.jpg")[0]["id"]);
    // พิมพ์ชื่อในช่องค้นหา และ "เลือกผลทั้งหมด" ใช้กติกาเดียวกัน
    assert_eq!(lib.filenames(json!({ "text": "กานต์" })), ["a1.jpg"]);
    assert_eq!(
        lib.filenames(json!({ "text": "กานต์", "includeSuggested": true, "sort": "filename" })),
        ["a1.jpg", "a2.jpg", "a3.jpg"]
    );
    assert_eq!(lib.call("create_selection", with_ai.clone())["count"], 3);
    // คนอื่นในภาพเดียวกัน: ต้องมีทั้งคู่ (ยืนยันหรือ AI จัดให้) จึงจะเข้าผลแบบ "ทุกคนในภาพเดียวกัน"
    let b1 = faces_of(&lib, "b1.jpg");
    let other =
        lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [b1[0]["id"]], "newPersonName": "คุณเมย์" }));
    let may = other["createdPerson"]["id"].as_str().unwrap().to_string();
    assert_eq!(lib.query(json!({ "personIds": [kan, may], "includeSuggested": true }))["total"], 0);
    assert_eq!(
        lib.query(json!({ "personIds": [kan, may], "personMode": "any", "includeSuggested": true }))["total"],
        5
    );

    // แจ้งว่าไม่ใช่: รูปนั้นหลุดจากรูปของคุณกานต์ทันทีและไม่กลับมา โดยไม่มีคำยืนยันใดถูกสร้างแทนผู้ใช้
    lib.call("reject_suggestions", json!({ "faceIds": [a2_face], "personId": kan }));
    assert_eq!(names(&lib.query(with_ai)), ["a1.jpg", "a3.jpg"]);
    assert_eq!(lib.call("get_person", json!({ "personId": kan }))["aiPhotoCount"], 1);
    assert_eq!(lib.count("SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL"), 2);

    let overview = lib.call("get_review_overview", json!({}));
    assert_eq!(overview["unexaminedFaceCount"], 0);
    let kan_group = arr(&overview["suggested"]).iter().find(|g| g["person"]["id"] == kan.as_str()).unwrap();
    assert_eq!((kan_group["faceCount"].as_i64(), kan_group["photoCount"].as_i64()), (Some(1), Some(1)));

    // รูปที่แจ้งว่าไม่ใช่ยังดูได้ และคืนได้เมื่อแจ้งผิด: กลับไปเป็นรูปที่ AI จัดให้ตามเดิม (ไม่กลายเป็นคำยืนยัน)
    assert_eq!(lib.call("get_person", json!({ "personId": kan }))["rejectedPhotoCount"], 1);
    let rejected = lib.query(json!({ "rejectedPersonId": kan }));
    assert_eq!(names(&rejected), ["a2.jpg"]);
    assert_eq!(rejected["items"][0]["rejectedFaces"], json!([a2_face]));
    lib.call("restore_suggestions", json!({ "faceIds": [a2_face], "personId": kan }));
    let person = lib.call("get_person", json!({ "personId": kan }));
    assert_eq!((person["rejectedPhotoCount"].as_i64(), person["aiPhotoCount"].as_i64()), (Some(0), Some(2)));
    let back = lib.query(json!({ "personIds": [kan], "includeSuggested": true, "sort": "filename" }));
    assert_eq!(names(&back), ["a1.jpg", "a2.jpg", "a3.jpg"]);
    assert_eq!(lib.query(json!({ "rejectedPersonId": kan }))["total"], 0);
    assert_eq!(lib.count("SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL"), 2);
}

#[test]
fn review_list_stays_usable_while_analysis_adds_faces() {
    let lib = Lib::with_ai();
    party(&lib);
    lib.install_model();
    let overview = lib.call("get_review_overview", json!({}));
    let cluster = overview["clusters"][0]["clusterId"].clone();
    let args = json!({ "kind": "cluster", "id": cluster, "reviewRevision": overview["reviewRevision"] });
    assert_eq!(lib.call("get_review_faces", args.clone())["total"], 3);

    // รูปใหม่ของคนกลุ่มเดียวกันถูกวิเคราะห์เสร็จหลังจากคิวออกไปแล้ว
    let root = lib.photos().join("2026-11-19 งานที่สอง");
    write_jpeg(&root.join("a4.jpg"), 40);
    write_jpeg(&root.join("d1.jpg"), 160);
    lib.import(&root, "single_event", |_| true);
    assert_eq!(lib.call("get_library_summary", json!({}))["unnamedFaceCount"], 8);

    let listed = lib.call("get_review_faces", args.clone());
    assert_eq!(
        (listed["total"].as_i64(), listed["reviewRevision"].as_i64()),
        (Some(3), overview["reviewRevision"].as_i64())
    );

    // คิวรอบถัดไปเห็นใบหน้าใหม่ และรายการของรอบก่อนใช้ไม่ได้แล้ว (คิวเดียวต่อครั้ง)
    let next = lib.call("get_review_overview", json!({}));
    assert_ne!(next["reviewRevision"], overview["reviewRevision"]);
    assert_eq!((next["unnamedFaceCount"].as_i64(), next["clusters"][0]["faceCount"].as_i64()), (Some(8), Some(4)));
    assert_eq!(lib.try_call("get_review_faces", args).unwrap_err().code, "PREVIEW_STALE");

    // เก็บบุคคลออกจากรายชื่อ = การตัดสิน: ข้อเสนอของคนนั้นหายจากคิว และรายการเดิมเก่า
    let a1 = faces_of(&lib, "a1.jpg");
    let named =
        lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [a1[0]["id"]], "newPersonName": "คุณกานต์" }));
    let kan = named["createdPerson"]["id"].clone();
    let with_kan = lib.call("get_review_overview", json!({}));
    assert_eq!(with_kan["suggested"][0]["faceCount"], 3);
    let args = json!({ "kind": "person", "id": kan, "reviewRevision": with_kan["reviewRevision"] });
    lib.call("get_review_faces", args.clone());
    lib.call("set_person_archived", json!({ "personId": kan, "archived": true }));
    assert_eq!(lib.try_call("get_review_faces", args).unwrap_err().code, "PREVIEW_STALE");
    assert_eq!(arr(&lib.call("get_review_overview", json!({}))["suggested"]).len(), 0);
}

/// ครอปใบหน้าถูกทำตอนวิเคราะห์: คิวตรวจสอบไม่ต้องเปิดต้นฉบับทีละใบ และยังแสดงได้เมื่อไดรฟ์รูปไม่ได้ต่ออยู่
#[test]
fn face_crops_are_ready_after_analysis_without_reopening_originals() {
    let lib = Lib::with_ai();
    party(&lib);
    lib.install_model();
    let crops = std::fs::read_dir(lib.app.cache_dir.join("faces")).unwrap().count();
    assert_eq!(crops, 6, "หนึ่งไฟล์ต่อใบหน้าที่ตรวจพบ ก่อนมีใครขอดู");

    let a1 = faces_of(&lib, "a1.jpg");
    let id = a1[0]["id"].as_str().unwrap();
    assert!(lib.app.face_crop_path(id).is_file());
    std::fs::rename(lib.photos(), lib.photos().with_extension("unplugged")).unwrap();
    let crop = puri_core::api::asset(&lib.app, &format!("face/{id}")).unwrap();
    let img = image::load_from_memory(&crop.bytes).unwrap();
    assert_eq!((img.width(), crop.mime), (img.height(), "image/jpeg"), "ครอปเป็นสี่เหลี่ยมจัตุรัส");
}

/// ใบหน้าที่ตัวตรวจไม่มั่นใจ (คะแนนต่ำกว่า 0.6): AI ไม่เสนอชื่อและไม่จัดกลุ่มให้ แต่ยังเห็นและตั้งชื่อเองได้
#[test]
fn low_confidence_detections_are_not_suggested_or_clustered_but_can_be_named() {
    let lib = Lib::with_ai();
    let root = lib.photos().join("2026-11-18 งานเลี้ยง");
    for (name, seed) in [("a_blur.jpg", 47), ("x_blur1.jpg", 107), ("x_blur2.jpg", 107)] {
        write_jpeg(&root.join(name), seed);
    }
    party(&lib);
    lib.install_model();
    assert_eq!(lib.count("SELECT COUNT(*) FROM faces WHERE score < 0.6"), 3, "ตัววิเคราะห์จำลองให้คะแนนต่ำสามใบ");

    // สองใบเบลอของ "คนเดียวกัน" ไม่ถูกจับเป็นกลุ่ม; ทั้งสามใบยังนับเป็นใบหน้าที่ยังไม่มีชื่อ
    let overview = lib.call("get_review_overview", json!({}));
    assert_eq!(overview["unnamedFaceCount"], 9);
    let sizes: Vec<i64> = arr(&overview["clusters"]).iter().map(|c| c["faceCount"].as_i64().unwrap()).collect();
    assert_eq!(sizes, [3, 2], "กลุ่มมีแต่ใบที่ตัวตรวจมั่นใจ");
    assert_eq!(overview["singleFaceCount"], 4, "C หนึ่งใบ + ใบคะแนนต่ำสามใบ");

    // ตั้งชื่อ A แล้ว: ใบคะแนนต่ำของ A ไม่ได้ข้อเสนอ ทั้งในตัวดูภาพและในคิว
    let a1 = faces_of(&lib, "a1.jpg");
    let named =
        lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [a1[0]["id"]], "newPersonName": "คุณกานต์" }));
    let kan = named["createdPerson"]["id"].as_str().unwrap().to_string();
    assert_eq!(faces_of(&lib, "a2.jpg")[0]["suggestion"]["person"]["id"], kan.as_str());
    let blur = faces_of(&lib, "a_blur.jpg");
    assert_eq!(blur.len(), 1, "กรอบยังอยู่ให้เห็นในตัวดูภาพ");
    assert!(blur[0]["suggestion"].is_null() && blur[0]["person"].is_null());
    assert_eq!(lib.call("get_review_overview", json!({}))["suggested"][0]["faceCount"], 2);

    // ผู้ใช้ยังตั้งชื่อใบนั้นเองได้ และค้นเจอทันที
    lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [blur[0]["id"]], "personId": kan }));
    assert_eq!(lib.filenames(json!({ "personIds": [kan] })), ["a1.jpg", "a_blur.jpg"]);
}

#[test]
fn manual_labels_survive_analysis_and_gain_an_embedding() {
    let lib = Lib::with_ai();
    party(&lib);
    // ผู้ใช้ลากกรอบและตั้งชื่อเองก่อนมีโมเดล (กรอบทับตำแหน่งที่ตัววิเคราะห์จะพบ)
    let manual = lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": lib.photo_id("b1.jpg"), "bbox": { "x": 0.32, "y": 0.22, "w": 0.24, "h": 0.38 }, "newPersonName": "คุณเมย์" }),
    );
    let may = manual["createdPerson"]["id"].as_str().unwrap().to_string();
    lib.install_model();

    let b1 = faces_of(&lib, "b1.jpg");
    assert_eq!(b1.len(), 1, "ไม่มีกรอบซ้ำบนใบหน้าเดียวกัน");
    assert_eq!(
        (b1[0]["origin"].as_str(), b1[0]["person"]["id"].as_str()),
        (Some("manual"), Some(may.as_str())),
        "คำยืนยันของผู้ใช้ไม่ถูกทับ"
    );
    assert_eq!(
        lib.count(
            "SELECT COUNT(*) FROM face_embeddings fe JOIN faces f ON f.id = fe.face_id WHERE f.origin = 'manual'"
        ),
        1
    );
    // กรอบที่วาดเองกลายเป็นตัวอย่างให้ AI เสนอใบอื่นของคนเดียวกัน
    assert_eq!(faces_of(&lib, "b2.jpg")[0]["suggestion"]["person"]["id"], may.as_str());
    assert_eq!(lib.filenames(json!({ "personIds": [may] })), ["b1.jpg"]);
}

/// ฐานข้อมูลแบบเดียวกับ PuriPhotoSearch รุ่น Python (เฉพาะตารางที่ตัวนำเข้าอ่าน)
fn legacy_db(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE persons (id INTEGER PRIMARY KEY, name TEXT NOT NULL, created_at TEXT, group_name TEXT);
         CREATE TABLE person_embeddings (id INTEGER PRIMARY KEY, person_id INTEGER, photo_path TEXT, embedding BLOB, thumbnail BLOB, is_primary INTEGER);
         CREATE TABLE event_folders (id INTEGER PRIMARY KEY, folder_path TEXT);
         CREATE TABLE photos (id INTEGER PRIMARY KEY, file_path TEXT);
         CREATE TABLE faces (id INTEGER PRIMARY KEY, photo_id INTEGER, embedding BLOB);
         INSERT INTO persons VALUES (1, 'คุณกานต์', '2026-03-09 10:00:00', 'ทีมงาน'), (2, 'สมชาย', '2026-03-09 10:01:00', NULL),
                                    (3, 'สมชาย', '2026-03-10 09:00:00', 'ทีมงาน');",
    )
    .unwrap();
    let old_vector = vec![7u8; 2048]; // เวกเตอร์ของโมเดลเดิม: ต้องไม่ถูกนำมาใช้
    for (person, seed, primary) in [(1, 40u8, 1), (1, 40, 0), (2, 80, 1), (3, 200, 1)] {
        conn.execute(
            "INSERT INTO person_embeddings(person_id, photo_path, embedding, thumbnail, is_primary) VALUES (?1, '/gone.jpg', ?2, ?3, ?4)",
            rusqlite::params![person, old_vector, jpeg_bytes(48, 48, seed), primary],
        )
        .unwrap();
    }
}

#[test]
fn legacy_people_import_is_read_only_idempotent_and_only_suggests() {
    let lib = Lib::with_ai();
    party(&lib);
    lib.install_model();
    let legacy = lib.tmp.path().join("photosearch.db");
    legacy_db(&legacy);
    let before = std::fs::read(&legacy).unwrap();

    // ข้อเสนอตอนเปิดแอปครั้งแรก: นับคนที่ยังไม่ได้นำเข้า และจำว่าผู้ใช้ปิดข้อเสนอแล้ว
    let pending = |lib: &Lib| puri_core::legacy::pending_people(&lib.app, legacy.to_str()).unwrap();
    assert_eq!(pending(&lib), 3);
    assert_eq!(lib.call("get_setup_state", json!({}))["dismissed"], false);
    lib.call("dismiss_setup", json!({}));
    lib.call("dismiss_setup", json!({}));
    assert_eq!(lib.call("get_setup_state", json!({}))["dismissed"], true);

    let req = new_id();
    let args = json!({ "path": legacy, "requestId": req });
    let result = lib.call("import_legacy_people", args.clone());
    assert_eq!(pending(&lib), 0, "นำเข้าแล้วไม่เหลือให้เสนอ");
    assert_eq!(
        (result["personsCreated"].as_i64(), result["groupsCreated"].as_i64(), result["samplesImported"].as_i64()),
        (Some(3), Some(1), Some(4))
    );
    assert_eq!(lib.call("import_legacy_people", args), result, "retry คำขอเดิมได้ผลเดิม");
    let again = lib.call("import_legacy_people", json!({ "path": legacy, "requestId": new_id() }));
    assert_eq!(
        (again["personsCreated"].as_i64(), again["personsSkipped"].as_i64(), again["samplesImported"].as_i64()),
        (Some(0), Some(3), Some(0)),
        "รันซ้ำไม่สร้างคนซ้ำ"
    );
    assert_eq!(std::fs::read(&legacy).unwrap(), before, "ฐานเดิมไม่ถูกแก้แม้ byte เดียว");
    assert!(!legacy.with_extension("db-wal").exists() && !lib.tmp.path().join("photosearch.db-shm").exists());

    // ชื่อซ้ำยังเป็นคนละคน, กลุ่มถูกสร้างครั้งเดียว, ภาพตัวแทนมาจากใบหน้าตัวอย่าง และยังไม่มีรูปยืนยัน
    let people = lib.call("list_people", json!({}));
    assert_eq!(arr(&people).len(), 3);
    assert_eq!(arr(&lib.call("list_people", json!({ "text": "สมชาย" }))).len(), 2);
    assert_eq!(lib.call("list_groups", json!({}))[0]["memberCount"], 2);
    let kan = arr(&people).iter().find(|p| p["displayName"] == "คุณกานต์").unwrap().clone();
    assert_eq!(kan["confirmedPhotoCount"], 0);
    assert!(kan["coverFaceId"].is_null());
    let cover = kan["coverAsset"].as_str().unwrap();
    assert!(cover.starts_with("sample/"));
    assert_eq!(puri_core::api::asset(&lib.app, cover).unwrap().mime, "image/jpeg");

    // worker สร้างเวกเตอร์ของภาพตัวอย่างด้วยโมเดลปัจจุบัน (ไม่ใช้เวกเตอร์เดิม) แล้วจึงมีข้อเสนอ
    assert_eq!(result["samplesPendingEmbedding"], 4);
    lib.run_jobs();
    assert_eq!(
        lib.count("SELECT COUNT(*) FROM person_reference_samples WHERE vector IS NOT NULL AND space = 'fake-space-v1'"),
        4
    );
    assert_eq!(
        lib.count(
            "SELECT COUNT(*) FROM person_reference_samples WHERE length(vector) = 2048 AND vector = zeroblob(2048)"
        ),
        0
    );
    let overview = lib.call("get_review_overview", json!({}));
    let by_name: Vec<(String, i64)> = arr(&overview["suggested"])
        .iter()
        .map(|g| (g["person"]["displayName"].as_str().unwrap().to_string(), g["faceCount"].as_i64().unwrap()))
        .collect();
    assert_eq!(by_name, [("คุณกานต์".to_string(), 3), ("สมชาย".to_string(), 2)], "เสนอจากใบหน้าตัวอย่างที่นำเข้า");
    let kan_id = kan["id"].as_str().unwrap();
    assert_eq!(lib.query(json!({ "personIds": [kan_id] }))["total"], 0, "ข้อเสนอจากข้อมูลเดิมไม่นับเป็นยืนยันแล้ว");

    // ผู้ใช้ยืนยันทั้งกลุ่มที่เห็น → ค้นเจอ และจำนวนรูปนับจากคำยืนยัน
    let listed = lib.call(
        "get_review_faces",
        json!({ "kind": "person", "id": kan_id, "reviewRevision": overview["reviewRevision"] }),
    );
    let ids: Vec<Value> = arr(&listed["faces"]).iter().map(|f| f["faceId"].clone()).collect();
    lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": ids, "personId": kan_id }));
    assert_eq!(lib.filenames(json!({ "personIds": [kan_id] })), ["a1.jpg", "a2.jpg", "a3.jpg"]);
    let kan = lib.call("get_person", json!({ "personId": kan_id }));
    assert_eq!(kan["confirmedPhotoCount"], 3);
    assert!(kan["coverAsset"].as_str().unwrap().starts_with("face/"), "มีใบหน้ายืนยันแล้ว ใช้เป็นภาพตัวแทนแทนภาพที่นำเข้า");
}

#[test]
fn library_from_v1_upgrades_in_place() {
    // คลังของรุ่น 2.0.0-alpha.1 (schema v1) ที่มีข้อมูลอยู่ ต้องเปิดได้และข้อมูลครบหลังอัปเกรด
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("data")).unwrap();
    {
        let conn = rusqlite::Connection::open(tmp.path().join("data/library.sqlite")).unwrap();
        conn.execute_batch(include_str!("../migrations/0001_init.sql")).unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 1; PRAGMA application_id = 1347768905;
             INSERT INTO meta VALUES ('library_id', 'lib-1'), ('name', 'คลังเดิม'), ('created_at', 'x'), ('data_revision', '9');
             INSERT INTO events(id, title, title_key, created_at) VALUES ('e1', 'งานเดิม', 'งานเดิม', 'x');
             INSERT INTO jobs(id, kind, state, stage, title, event_id, dedup_key, total, done, created_at, updated_at)
               VALUES ('j1', 'catalog', 'completed', 'done', 'งานเดิม', 'e1', 'catalog:e1', 5, 5, 'x', 'x');
             INSERT INTO job_errors(job_id, filename, code, message, created_at) VALUES ('j1', 'bad.jpg', 'DECODE_FAILED', 'm', 'x');
             INSERT INTO persons(id, display_name, name_key, created_at) VALUES ('p1', 'คุณกานต์', 'คุณกานต์', 'x');",
        )
        .unwrap();
    }
    let app = puri_core::App::open(puri_core::Options {
        data_dir: tmp.path().join("data"),
        cache_dir: tmp.path().join("cache"),
        sink: std::sync::Arc::new(puri_core::NoopSink),
        folder_picker: "native",
        start_worker: false,
        face_engine: None,
        face_engine_unavailable: "",
    })
    .unwrap();
    let call = |cmd: &str, args: Value| puri_core::api::dispatch(&app, cmd, args).unwrap();
    assert_eq!(call("get_capabilities", json!({}))["schemaVersion"], 4);
    assert_eq!(call("get_library_summary", json!({}))["name"], "คลังเดิม");
    let job = call("get_job", json!({ "jobId": "j1" }));
    assert_eq!((job["state"].as_str(), job["counts"]["done"].as_i64()), (Some("completed"), Some(5)));
    assert_eq!(call("list_job_errors", json!({ "jobId": "j1" }))[0]["filename"], "bad.jpg");
    assert_eq!(call("list_people", json!({}))[0]["displayName"], "คุณกานต์");
    assert!(call("list_people", json!({}))[0]["coverAsset"].is_null());

    // คลังเดิมถูกเก็บไว้ก่อนเปลี่ยน schema: ชุดสำรองยังเป็นรุ่น 1 และกู้กลับได้
    let kept = call("get_backup_status", json!({}));
    let before = &kept["automatic"][0];
    assert_eq!((before["kind"].as_str(), before["schemaVersion"].as_i64()), (Some("before_upgrade"), Some(1)));
    assert_eq!(before["counts"]["persons"], 1);
    let preview = call("preview_restore", json!({ "path": before["path"] }));
    assert_eq!(preview["needsUpgrade"], true);
}

#[test]
fn merging_duplicate_people_keeps_samples_confirmations_groups_and_pins() {
    let lib = Lib::with_ai();
    party(&lib);
    lib.install_model();
    let legacy = lib.tmp.path().join("photosearch.db");
    legacy_db(&legacy);
    let import = json!({ "path": legacy, "requestId": new_id() });
    lib.call("import_legacy_people", import);
    lib.run_jobs();

    // รุ่นเดิมมี "สมชาย" สองคน: คนแรกไม่มีกลุ่ม, คนที่สองอยู่ในกลุ่มทีมงาน
    let somchai = arr(&lib.call("list_people", json!({ "text": "สมชาย" }))).clone();
    assert_eq!(somchai.len(), 2);
    let in_group = |p: &Value| !arr(&p["groups"]).is_empty();
    let keep = somchai.iter().find(|p| !in_group(p)).unwrap()["id"].as_str().unwrap().to_string();
    let dup = somchai.iter().find(|p| in_group(p)).unwrap()["id"].as_str().unwrap().to_string();
    let b1 = faces_of(&lib, "b1.jpg")[0]["id"].clone();
    lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [b1], "personId": dup }));
    lib.call("set_person_pinned", json!({ "personId": dup, "pinned": true }));
    let version = lib.call("get_person", json!({ "personId": dup }))["recordVersion"].clone();
    lib.call(
        "save_person",
        json!({ "personId": dup, "expectedVersion": version, "requestId": new_id(), "displayName": "สมชาย", "aliases": ["ชาย"] }),
    );

    let req = new_id();
    let args = json!({ "targetId": keep, "sourceIds": [dup, keep], "requestId": req });
    let merged = lib.call("merge_persons", args.clone());
    assert_eq!(
        (merged["personId"].as_str(), merged["merged"].as_i64()),
        (Some(keep.as_str()), Some(1)),
        "ไม่รวมคนเข้ากับตัวเอง"
    );
    assert_eq!(lib.call("merge_persons", args), merged, "ส่งคำขอเดิมซ้ำได้ผลเดิม");

    let people = arr(&lib.call("list_people", json!({ "text": "สมชาย", "includeArchived": true }))).clone();
    assert_eq!(people.len(), 1, "เหลือสมชายคนเดียว");
    let person = lib.call("get_person", json!({ "personId": keep }));
    assert_eq!(person["pinned"], true, "หมุดของคนที่ถูกรวมย้ายมา");
    assert_eq!(arr(&person["groups"]).len(), 1, "กลุ่มของคนที่ถูกรวมย้ายมา");
    assert_eq!(person["aliases"], json!(["ชาย"]), "ชื่ออื่นย้ายมา (ชื่อเดียวกันไม่ซ้ำเป็นชื่ออื่น)");
    assert_eq!(lib.filenames(json!({ "personIds": [keep] })), ["b1.jpg"], "คำยืนยันใบหน้าย้ายมา");
    assert_eq!(lib.count(&format!("SELECT COUNT(*) FROM person_reference_samples WHERE person_id = '{keep}'")), 2);
    assert_eq!(lib.count(&format!("SELECT COUNT(*) FROM persons WHERE id = '{dup}'")), 0);

    // นำเข้ารุ่นเดิมอีกครั้งไม่สร้างคนที่รวมไปแล้วกลับมา
    let again = lib.call("import_legacy_people", json!({ "path": legacy, "requestId": new_id() }));
    assert_eq!((again["personsCreated"].as_i64(), again["personsSkipped"].as_i64()), (Some(0), Some(3)));

    let gone = lib.try_call("merge_persons", json!({ "targetId": keep, "sourceIds": [dup], "requestId": new_id() }));
    assert_eq!(gone.unwrap_err().code, "NOT_FOUND");
    let alone = lib.try_call("merge_persons", json!({ "targetId": keep, "sourceIds": [keep], "requestId": new_id() }));
    assert_eq!(alone.unwrap_err().code, "INVALID_INPUT");
}
