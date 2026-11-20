//! ทดสอบพฤติกรรมตาม acceptance matrix ของ Blueprint หมวด 17.2
//! ทุกเทสต์ใช้คลัง SQLite จริง โฟลเดอร์และไฟล์รูปจริงบนดิสก์ และเรียกผ่าน `api::dispatch` แบบเดียวกับ UI

mod common;

use common::*;
use serde_json::{json, Value};
use std::path::Path;

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array")
}

/// โฟลเดอร์ปี 2569 ที่มีสองงานลงวันที่ + รูปวางตรงโฟลเดอร์แม่
fn year_folder(lib: &Lib) -> std::path::PathBuf {
    let root = lib.photos().join("2569");
    write_jpeg(&root.join("691118 งานอบรม AI/กล้อง A/IMG_0001.jpg"), 1);
    write_jpeg(&root.join("691118 งานอบรม AI/กล้อง A/IMG_0002.jpg"), 2);
    write_jpeg(&root.join("691118 งานอบรม AI/กล้อง B/DSC_0001.jpg"), 3);
    write_jpeg(&root.join("691116 ต้อนรับคณะญาติโยม/IMG_0101.jpg"), 4);
    write_jpeg(&root.join("691116 ต้อนรับคณะญาติโยม/IMG_0102.jpg"), 5);
    write_jpeg(&root.join("LOOSE_1.jpg"), 6);
    root
}

#[test]
fn t01_cancel_at_preview_leaves_nothing_permanent() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    let preview = lib.preview(&root);
    assert_eq!(preview["single"]["photoCount"], 6);
    lib.call("cancel_import_preview", json!({ "previewId": preview["previewId"] }));

    for table in ["sources", "events", "photos", "jobs", "event_source_bindings"] {
        assert_eq!(lib.count(&format!("SELECT COUNT(*) FROM {table}")), 0, "{table} ต้องว่างหลังยกเลิก");
    }
    // commit ด้วย preview ที่ยกเลิกแล้วต้องถูกปฏิเสธ ไม่ใช่สร้างกิจกรรมครึ่งหนึ่ง
    let err = lib
        .try_call(
            "commit_import",
            json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": "single_event",
                    "drafts": [{ "draftId": preview["single"]["draftId"], "title": "x", "eventDate": null }] }),
        )
        .unwrap_err();
    assert_eq!(err.code, "PREVIEW_STALE");
    assert_eq!(lib.count("SELECT COUNT(*) FROM events"), 0);
}

#[test]
fn t02_single_event_merges_camera_subfolders() {
    let lib = Lib::new();
    let root = lib.photos().join("2569").join("691118 งานอบรม AI");
    write_jpeg(&root.join("กล้อง A/IMG_0001.jpg"), 1);
    write_jpeg(&root.join("กล้อง B/IMG_0001.jpg"), 2); // ชื่อซ้ำคนละโฟลเดอร์ เป็นคนละรูป
    write_jpeg(&root.join("วันที่สอง/เช้า/IMG_0009.jpg"), 3);

    let preview = lib.preview(&root);
    assert_eq!(preview["suggestedMode"], "single_event", "งานที่ชื่อบอกวันที่ + โฟลเดอร์กล้อง = งานเดียว");
    assert_eq!(preview["single"]["suggestedTitle"], "งานอบรม AI");
    assert_eq!(preview["single"]["suggestedDate"], "2026-11-18");
    assert_eq!(preview["single"]["dateSource"], "folder_name");

    let result = lib.import(&root, "single_event", |_| true);
    assert_eq!(result["createdEvents"], 1);
    let events = lib.call("list_events", json!({}));
    assert_eq!(arr(&events).len(), 1);
    assert_eq!(events[0]["photoCount"], 3);
    assert_eq!(events[0]["thumbReadyCount"], 3);
    assert_eq!(events[0]["eventDate"], "2026-11-18");
    assert!(events[0]["coverPhotoId"].is_string());
    assert_eq!(lib.filenames(json!({})), ["IMG_0001.jpg", "IMG_0001.jpg", "IMG_0009.jpg"]);
}

#[test]
fn t03_parent_with_events_and_loose_photos_shows_every_group() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    std::fs::create_dir_all(root.join("โฟลเดอร์ว่าง")).unwrap();
    std::fs::write(root.join("691116 ต้อนรับคณะญาติโยม/clip.heic"), b"not decodable here").unwrap();
    std::fs::write(root.join("691116 ต้อนรับคณะญาติโยม/notes.txt"), b"x").unwrap();
    std::fs::write(root.join("691116 ต้อนรับคณะญาติโยม/._IMG_0101.jpg"), b"appledouble").unwrap();

    let preview = lib.preview(&root);
    assert_eq!(preview["suggestedMode"], "child_events");
    let children = arr(&preview["children"]);
    assert_eq!(children.len(), 3);
    let by_name = |n: &str| children.iter().find(|c| c["folderName"] == n).unwrap();
    assert_eq!(by_name("691118 งานอบรม AI")["photoCount"], 3);
    assert_eq!(by_name("691118 งานอบรม AI")["suggestedDate"], "2026-11-18");
    let welcome = by_name("691116 ต้อนรับคณะญาติโยม");
    assert_eq!(welcome["photoCount"], 2, "ไฟล์ซ่อน/ไฟล์ที่ไม่ใช่รูปไม่ถูกนับเป็นรูป");
    assert_eq!(welcome["unsupportedCount"], 1);
    assert_eq!(preview["unsupportedByExt"]["heic"], 1);
    let empty = by_name("โฟลเดอร์ว่าง");
    assert_eq!(empty["photoCount"], 0);
    assert_eq!(empty["defaultSelected"], false);
    // รูปที่วางตรงโฟลเดอร์แม่มีแถวของตัวเอง ไม่ตกหล่นเงียบ ๆ
    assert_eq!(preview["rootLoose"]["kind"], "root_loose");
    assert_eq!(preview["rootLoose"]["photoCount"], 1);
    assert_eq!(preview["rootLoose"]["defaultSelected"], true);
    // ผลรวมทุกกลุ่ม = จำนวนในโหมดงานเดียว
    let sum: i64 = children.iter().map(|c| c["photoCount"].as_i64().unwrap()).sum::<i64>() + 1;
    assert_eq!(preview["single"]["photoCount"], sum);

    let result = lib.import(&root, "child_events", |d| d["defaultSelected"] == true);
    assert_eq!(result["createdEvents"], 3);
    let events = lib.call("list_events", json!({}));
    let titles: Vec<&str> = arr(&events).iter().map(|e| e["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["งานอบรม AI", "ต้อนรับคณะญาติโยม", "รูปใน 2569"], "เรียงวันล่าสุดก่อน ไม่ทราบวันที่อยู่ท้าย");
    assert_eq!(lib.query(json!({}))["total"], 6);
    // กิจกรรม "รูปในโฟลเดอร์แม่" ได้เฉพาะรูปที่วางตรง ๆ
    let loose_id = arr(&events)[2]["id"].clone();
    assert_eq!(lib.filenames(json!({ "eventIds": [loose_id] })), ["LOOSE_1.jpg"]);
}

#[test]
fn t04_overlapping_parent_and_child_never_duplicate_photos() {
    // child → parent
    let lib = Lib::new();
    let root = year_folder(&lib);
    let child = root.join("691118 งานอบรม AI");
    lib.import(&child, "single_event", |_| true);
    assert_eq!(lib.query(json!({}))["total"], 3);

    let preview = lib.preview(&root);
    let known = arr(&preview["children"]).iter().find(|c| c["folderName"] == "691118 งานอบรม AI").unwrap();
    assert!(known["existingEventId"].is_string(), "preview ต้องบอกว่างานนี้อยู่ในคลังแล้ว");
    assert_eq!(known["newCount"], 0);
    assert_eq!(known["existingCount"], 3);
    assert_eq!(known["defaultSelected"], false);

    // เลือกทุกแถว รวมงานเดิม: ต้องไม่เกิดกิจกรรมหรือรูปซ้ำ
    let result = lib.import(&root, "child_events", |d| d["photoCount"].as_i64().unwrap() > 0);
    assert_eq!(result["reusedEvents"], 1);
    assert_eq!(result["createdEvents"], 2);
    assert_eq!(arr(&lib.call("list_events", json!({}))).len(), 3);
    assert_eq!(lib.query(json!({}))["total"], 6);
    assert_eq!(lib.count("SELECT COUNT(*) FROM photo_locations"), 6);

    // parent (งานเดียว) → child: โฟลเดอร์ย่อยถูกครอบคลุมแล้ว จึงชี้กิจกรรมเดิม
    let lib = Lib::new();
    let root = year_folder(&lib);
    lib.import(&root, "single_event", |_| true);
    assert_eq!(lib.query(json!({}))["total"], 6);
    let preview = lib.preview(&root.join("691118 งานอบรม AI"));
    assert!(preview["single"]["existingEventId"].is_string());
    assert_eq!(preview["single"]["newCount"], 0);
    let result = lib.import(&root.join("691118 งานอบรม AI"), "single_event", |_| true);
    assert_eq!(result["createdEvents"], 0);
    assert_eq!(arr(&lib.call("list_events", json!({}))).len(), 1, "ไม่มีการ์ดกิจกรรมซ้ำ");
    assert_eq!(lib.query(json!({}))["total"], 6);
}

#[test]
fn t04b_new_file_in_child_folder_goes_to_child_event_even_when_parent_scans() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    let child = root.join("691118 งานอบรม AI");
    lib.import(&child, "single_event", |_| true);
    lib.import(&root, "single_event", |_| true); // งาน "ทั้งปี" ได้เฉพาะรูปที่ยังไม่มีเจ้าของ
    let events = lib.call("list_events", json!({}));
    let id_of = |title: &str| arr(&events).iter().find(|e| e["title"] == title).unwrap()["id"].clone();
    let (child_event, parent_event) = (id_of("งานอบรม AI"), id_of("2569"));
    assert_eq!(lib.query(json!({ "eventIds": [parent_event.clone()] }))["total"], 3);

    // เพิ่มรูปใหม่ในโฟลเดอร์งานย่อย แล้วสั่งตรวจรูปใหม่ที่งานแม่ก่อน
    write_jpeg(&child.join("กล้อง A/IMG_NEW.jpg"), 99);
    lib.call("rescan_event", json!({ "eventId": parent_event.clone() }));
    lib.run_jobs();
    assert_eq!(lib.query(json!({ "eventIds": [parent_event] }))["total"], 3, "งานแม่ไม่แย่งรูปของงานย่อย");
    lib.call("rescan_event", json!({ "eventId": child_event.clone() }));
    lib.run_jobs();
    assert_eq!(lib.query(json!({ "eventIds": [child_event] }))["total"], 4);
}

#[test]
fn t05_exact_copies_in_two_events_stay_separate() {
    let lib = Lib::new();
    let root = lib.photos().join("งานรวม");
    write_jpeg(&root.join("2026-11-18 งานเช้า/IMG_0001.jpg"), 9);
    std::fs::create_dir_all(root.join("2026-11-19 งานบ่าย")).unwrap();
    std::fs::copy(root.join("2026-11-18 งานเช้า/IMG_0001.jpg"), root.join("2026-11-19 งานบ่าย/IMG_0001.jpg")).unwrap();
    lib.import(&root, "child_events", |_| true);

    assert_eq!(lib.query(json!({}))["total"], 2, "สำเนาเหมือนกันทุก byte ยังเป็นสองตำแหน่ง");
    assert_eq!(lib.count("SELECT COUNT(DISTINCT sha256) FROM photo_revisions"), 1, "เนื้อหาเดียวกันมี hash เดียวกัน");
    for e in arr(&lib.call("list_events", json!({}))) {
        assert_eq!(e["photoCount"], 1);
    }
    assert!(
        root.join("2026-11-18 งานเช้า/IMG_0001.jpg").is_file() && root.join("2026-11-19 งานบ่าย/IMG_0001.jpg").is_file()
    );
}

#[test]
fn t06_commit_retry_and_double_click_are_idempotent() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    let preview = lib.preview(&root);
    let args = json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": "child_events",
                       "drafts": drafts_of(&preview, "child_events", |_| true), "analyzeFaces": true, "watch": true });
    let first = lib.call("commit_import", args.clone());
    let second = lib.call("commit_import", args); // กดซ้ำ/ส่งคำขอเดิมซ้ำ
    assert_eq!(first, second);
    assert_eq!(lib.count("SELECT COUNT(*) FROM events"), 3);
    assert_eq!(lib.count("SELECT COUNT(*) FROM jobs"), 3);
    lib.run_jobs();
    assert_eq!(lib.query(json!({}))["total"], 6);

    // สั่งตรวจรูปใหม่ซ้ำขณะงานเดิมยังรอคิว: ได้งานเดิม ไม่สร้างงานสำเนา
    let event_id = first["eventIds"][0].clone();
    let a = lib.call("rescan_event", json!({ "eventId": event_id.clone() }));
    let b = lib.call("rescan_event", json!({ "eventId": event_id }));
    assert_eq!(a["id"], b["id"]);
    lib.run_jobs();
    assert_eq!(lib.query(json!({}))["total"], 6, "สแกนซ้ำไม่เพิ่มรูปซ้ำ");
    assert_eq!(lib.count("SELECT COUNT(*) FROM photo_revisions"), 6);
}

#[test]
fn t07_corrupt_and_unsupported_files_do_not_block_good_ones() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานปน");
    write_jpeg(&root.join("good_1.jpg"), 1);
    write_png(&root.join("good_2.png"));
    std::fs::write(root.join("broken.jpg"), b"\xFF\xD8\xFF this is not a real jpeg").unwrap();
    std::fs::write(root.join("raw.cr2"), b"raw").unwrap();

    let result = lib.import(&root, "single_event", |_| true);
    let job = lib.call("get_job", json!({ "jobId": result["jobIds"][0] }));
    assert_eq!(job["state"], "completed_with_errors", "มีไฟล์เสียต้องไม่ขึ้นว่าสำเร็จทั้งหมด");
    assert_eq!(job["counts"]["done"], 2);
    assert_eq!(job["counts"]["failed"], 1);
    assert_eq!(job["errorSummary"][0]["code"], "DECODE_FAILED");
    let errors = lib.call("list_job_errors", json!({ "jobId": job["id"] }));
    assert_eq!(errors[0]["filename"], "broken.jpg");

    let page = lib.query(json!({}));
    assert_eq!(page["total"], 3, "ไฟล์เสียยังอยู่ในรายการพร้อมสถานะ ไม่หายเงียบ");
    assert_eq!(page["coverage"]["thumbnailsReady"], 2);
    assert_eq!(page["coverage"]["thumbnailsFailed"], 1);
    let event = lib.call("get_event", json!({ "eventId": result["eventIds"][0] }));
    assert_eq!(event["failedCount"], 1);

    // ลองใหม่เฉพาะรายการผิดพลาด: ไม่ประมวลผลรูปดีซ้ำ
    std::fs::write(root.join("broken.jpg"), jpeg_bytes(32, 32, 5)).unwrap();
    let retry = lib.call("retry_failed_items", json!({ "jobId": job["id"] }));
    lib.run_jobs();
    let retry = lib.call("get_job", json!({ "jobId": retry["id"] }));
    assert_eq!(retry["state"], "completed");
    assert_eq!(retry["counts"]["done"], 1);
    assert_eq!(lib.query(json!({}))["coverage"]["thumbnailsReady"], 3);
}

#[test]
fn t08_changed_file_gets_new_revision_and_labels_do_not_jump() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานแก้ไฟล์");
    write_jpeg(&root.join("a.jpg"), 1);
    write_jpeg(&root.join("b.jpg"), 2);
    let result = lib.import(&root, "single_event", |_| true);
    lib.label("a.jpg", 0.1, ("name", "คุณกานต์"));
    let (a, b) = (lib.photo_id("a.jpg"), lib.photo_id("b.jpg"));

    // เนื้อหาไฟล์เปลี่ยนที่ path เดิม (ผู้ใช้แก้รูปด้วยโปรแกรมอื่น)
    std::fs::write(root.join("a.jpg"), jpeg_bytes(80, 60, 200)).unwrap();
    std::fs::write(root.join("b.jpg"), jpeg_bytes(80, 60, 201)).unwrap();
    lib.call("rescan_event", json!({ "eventId": result["eventIds"][0] }));
    lib.run_jobs();

    assert_eq!(lib.photo_id("a.jpg"), a, "path เดิม = photo ID เดิม");
    assert_eq!(lib.photo_id("b.jpg"), b);
    assert_eq!(
        lib.count("SELECT COUNT(*) FROM photo_revisions"),
        3,
        "รูปที่มีคำยืนยันได้ revision ใหม่; รูปที่ไม่มีใช้ revision เดิม"
    );
    let details = lib.call("get_photo_details", json!({ "photoId": a }));
    assert_eq!(details["width"], 80);
    assert_eq!(arr(&details["faces"]).len(), 0, "คำยืนยันเดิมไม่ถูกย้ายไป revision เนื้อหาใหม่เอง");
    assert_eq!(
        lib.count("SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL"),
        1,
        "คำยืนยันเดิมยังเก็บเป็นประวัติ"
    );
    assert_eq!(lib.call("list_people", json!({}))[0]["confirmedPhotoCount"], 0);
}

#[test]
fn t10_t11_restart_recovers_interrupted_jobs_and_keeps_paused_ones_paused() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    let preview = lib.preview(&root);
    let result = lib.call(
        "commit_import",
        json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": "child_events",
                "drafts": drafts_of(&preview, "child_events", |_| true) }),
    );
    let jobs: Vec<String> = arr(&result["jobIds"]).iter().map(|j| j.as_str().unwrap().to_string()).collect();
    // งานแรก: ผู้ใช้สั่งพัก; งานที่สอง: จำลองว่ากำลังทำแล้วแอปถูก kill; งานที่สาม: สั่งยกเลิกค้างไว้
    assert_eq!(lib.call("pause_job", json!({ "jobId": jobs[0] }))["state"], "paused");
    lib.count(&format!("UPDATE jobs SET state = 'running' WHERE id = '{}' RETURNING 1", jobs[1]));
    lib.count(&format!("UPDATE jobs SET state = 'cancel_requested' WHERE id = '{}' RETURNING 1", jobs[2]));

    let lib = lib.reopen();
    let state = |id: &str| lib.call("get_job", json!({ "jobId": id }));
    assert_eq!(state(&jobs[0])["state"], "paused", "งานที่พักไว้ต้องไม่ทำต่อเองหลังเปิดใหม่");
    assert_eq!(state(&jobs[1])["state"], "queued");
    assert_eq!(state(&jobs[1])["recovered"], true);
    assert_eq!(state(&jobs[2])["state"], "cancelled");

    lib.run_jobs();
    assert_eq!(state(&jobs[1])["state"], "completed");
    assert_eq!(state(&jobs[0])["state"], "paused");
    let before = lib.query(json!({}))["total"].as_i64().unwrap();
    assert!(before > 0);

    assert_eq!(lib.call("resume_job", json!({ "jobId": jobs[0] }))["state"], "queued");
    lib.run_jobs();
    assert_eq!(state(&jobs[0])["state"], "completed");
    assert_eq!(
        lib.count("SELECT COUNT(*) FROM photo_locations"),
        lib.count("SELECT COUNT(DISTINCT path_key) FROM photo_locations")
    );
    assert!(lib.query(json!({}))["total"].as_i64().unwrap() > before);
}

#[test]
fn t12_offline_source_keeps_metadata_and_reconnect_restores_same_ids() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานไดรฟ์นอก");
    write_jpeg(&root.join("a.jpg"), 1);
    write_jpeg(&root.join("b.jpg"), 2);
    let result = lib.import(&root, "single_event", |_| true);
    let event_id = result["eventIds"][0].clone();
    lib.label("a.jpg", 0.1, ("name", "พระมหาอนวัช"));
    let a = lib.photo_id("a.jpg");

    // ไดรฟ์หลุด
    let parked = lib.tmp.path().join("unplugged");
    std::fs::rename(&root, &parked).unwrap();
    let event = lib.call("get_event", json!({ "eventId": event_id.clone() }));
    assert_eq!(event["sourceAvailable"], false);
    assert_eq!(event["photoCount"], 2, "รูปไม่หายจากคลังเพราะหาไฟล์ไม่เจอ");
    assert_eq!(lib.filenames(json!({ "personIds": [lib.person_id("พระมหาอนวัช")] })), ["a.jpg"], "ชื่อที่ยืนยันยังค้นได้");
    let details = lib.call("get_photo_details", json!({ "photoId": a.clone() }));
    assert_eq!(details["originalAvailable"], false);
    assert_eq!(lib.try_call("open_photo", json!({ "photoId": a.clone() })).unwrap_err().code, "SOURCE_UNAVAILABLE");
    assert!(puri_core::api::asset(&lib.app, &format!("thumb/{a}")).is_ok(), "ภาพย่อที่เก็บไว้ยังดูได้");
    assert!(puri_core::api::asset(&lib.app, &format!("photo/{a}")).is_err());

    // สั่งตรวจรูปใหม่ตอนต้นทางไม่อยู่: งานรอ ไม่ล้างข้อมูล
    let job = lib.call("rescan_event", json!({ "eventId": event_id.clone() }));
    lib.run_jobs();
    let job = lib.call("get_job", json!({ "jobId": job["id"] }));
    assert_eq!(job["state"], "blocked");
    assert_eq!(lib.query(json!({}))["total"], 2);

    // ต่อไดรฟ์กลับ แล้วสั่งทำต่อ
    std::fs::rename(&parked, &root).unwrap();
    lib.call("resume_job", json!({ "jobId": job["id"] }));
    lib.run_jobs();
    assert_eq!(lib.call("get_job", json!({ "jobId": job["id"] }))["state"], "completed");
    assert_eq!(lib.call("get_event", json!({ "eventId": event_id }))["sourceAvailable"], true);
    assert_eq!(lib.photo_id("a.jpg"), a, "ID เดิม ไม่ต้องตั้งชื่อบุคคลใหม่");
    assert_eq!(lib.call("list_people", json!({}))[0]["confirmedPhotoCount"], 1);
}

#[test]
fn t13_relink_moved_folder_keeps_ids_names_and_only_binds_files_it_can_prove() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    lib.import(&root, "child_events", |d| d["defaultSelected"] == true);
    let events = lib.call("list_events", json!({}));
    let id_of = |title: &str| arr(&events).iter().find(|e| e["title"] == title).unwrap()["id"].clone();
    let (ai, welcome) = (id_of("งานอบรม AI"), id_of("ต้อนรับคณะญาติโยม"));
    lib.label("IMG_0001.jpg", 0.2, ("name", "คุณกานต์"));
    let kan = lib.person_id("คุณกานต์");
    let photo = lib.photo_id("IMG_0001.jpg");
    let before = lib.count("SELECT COUNT(*) FROM photos");

    // ย้ายทั้งโฟลเดอร์แม่ไปที่อื่นจากนอกแอป แล้วแก้ไฟล์หนึ่งและลบอีกไฟล์ในโฟลเดอร์งานอบรม
    let moved = lib.tmp.path().join("ไดรฟ์ใหม่").join("2569");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&root, &moved).unwrap();
    let ai_new = moved.join("691118 งานอบรม AI");
    std::fs::write(ai_new.join("กล้อง A/IMG_0002.jpg"), b"not the same file any more").unwrap();
    std::fs::remove_file(ai_new.join("กล้อง B/DSC_0001.jpg")).unwrap();
    assert_eq!(lib.call("get_event", json!({ "eventId": ai.clone() }))["sourceAvailable"], false);

    // ดูก่อน: จับคู่ได้เฉพาะไฟล์ที่ตำแหน่งและขนาดตรง, กิจกรรมพี่น้องที่ย้ายไปด้วยกันถูกพบเอง
    let preview = lib.call("preview_relink", json!({ "eventId": ai.clone(), "newFolder": ai_new }));
    let e = &preview["event"];
    assert_eq!(
        (e["photos"].as_i64(), e["matched"].as_i64(), e["changed"].as_i64(), e["missing"].as_i64()),
        (Some(3), Some(1), Some(1), Some(1))
    );
    let siblings: Vec<&str> = arr(&preview["siblings"]).iter().map(|s| s["title"].as_str().unwrap()).collect();
    assert_eq!(siblings, ["ต้อนรับคณะญาติโยม", "รูปใน 2569"]);
    assert_eq!(lib.call("get_event", json!({ "eventId": ai.clone() }))["sourceAvailable"], false, "ดูก่อนไม่เปลี่ยนอะไร");

    // โฟลเดอร์ของกิจกรรมอื่น หรือโฟลเดอร์ที่ไม่มีรูปของกิจกรรมนี้: ไม่เปลี่ยนตำแหน่ง
    let empty = lib.tmp.path().join("ว่าง");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(lib.try_call("commit_relink", json!({ "eventId": ai.clone(), "newFolder": empty })).is_err());
    assert_eq!(lib.call("get_event", json!({ "eventId": ai.clone() }))["sourceAvailable"], false);

    let result =
        lib.call("commit_relink", json!({ "eventId": ai.clone(), "newFolder": ai_new, "includeSiblings": true }));
    assert_eq!(
        (result["events"].as_i64(), result["photosRelinked"].as_i64(), result["photosUnresolved"].as_i64()),
        (Some(3), Some(4), Some(2))
    );

    // ID เดิม ชื่อเดิม: ไม่มีรูปหรือกิจกรรมใหม่ และรูปที่จับคู่ได้เปิดต้นฉบับได้
    assert_eq!(lib.count("SELECT COUNT(*) FROM photos"), before);
    assert_eq!(arr(&lib.call("list_events", json!({}))).len(), 3);
    for id in [&ai, &welcome] {
        assert_eq!(lib.call("get_event", json!({ "eventId": id }))["sourceAvailable"], true);
    }
    assert_eq!(lib.photo_id("IMG_0001.jpg"), photo);
    assert_eq!(lib.filenames(json!({ "personIds": [kan] })), ["IMG_0001.jpg"]);
    assert_eq!(lib.call("get_photo_details", json!({ "photoId": photo }))["originalAvailable"], true);
    assert!(puri_core::api::asset(&lib.app, &format!("photo/{photo}")).is_ok());
    // ไฟล์ที่ถูกแก้ (ขนาดไม่ตรง) และไฟล์ที่หายไป ไม่ถูกผูกกับไฟล์อื่นเอง
    for name in ["IMG_0002.jpg", "DSC_0001.jpg"] {
        let id = lib.photo_id(name);
        assert_eq!(lib.call("get_photo_details", json!({ "photoId": id }))["originalAvailable"], false, "{name}");
    }
    // ตรวจรูปใหม่ที่ตำแหน่งใหม่ทำงานต่อได้ และโฟลเดอร์แม่ของแหล่งรูปย้ายตาม
    write_jpeg(&ai_new.join("กล้อง A/IMG_NEW.jpg"), 77);
    lib.call("rescan_event", json!({ "eventId": ai.clone() }));
    lib.run_jobs();
    assert!(lib.filenames(json!({ "eventIds": [ai] })).contains(&"IMG_NEW.jpg".to_string()));
    let roots = lib.count("SELECT COUNT(*) FROM sources WHERE root_path LIKE '%ไดรฟ์ใหม่%'");
    assert_eq!(roots, 1);
}

#[test]
fn t15_t16_duplicate_names_and_groups() {
    let lib = Lib::new();
    let save = |name: &str, groups: Vec<Value>| {
        lib.call("save_person", json!({ "requestId": new_id(), "displayName": name, "groupIds": groups }))
    };
    let team = lib.call("create_group", json!({ "name": "ทีมงาน" }));
    let volunteers = lib.call("create_group", json!({ "name": "อาสาสมัคร" }));
    assert_eq!(lib.try_call("create_group", json!({ "name": " ทีมงาน " })).unwrap_err().code, "INVALID_INPUT");

    let one = save("สมชาย", vec![team["id"].clone(), volunteers["id"].clone()]);
    let two = save("สมชาย", vec![]);
    assert_ne!(one["id"], two["id"], "ชื่อซ้ำได้ แยกด้วย ID");
    assert_eq!(arr(&lib.call("list_people", json!({ "text": "สมชาย" }))).len(), 2);
    assert_eq!(arr(&one["groups"]).len(), 2);

    // แก้ชื่อคนหนึ่ง: อีกคนไม่กระทบ และ version เก่าถูกปฏิเสธ
    let renamed = lib.call(
        "save_person",
        json!({ "requestId": new_id(), "personId": two["id"], "expectedVersion": two["recordVersion"], "displayName": "สมชาย (ช่างภาพ)" }),
    );
    assert_eq!(renamed["id"], two["id"]);
    let stale = lib
        .try_call("save_person", json!({ "requestId": new_id(), "personId": two["id"], "expectedVersion": two["recordVersion"], "displayName": "x" }))
        .unwrap_err();
    assert_eq!(stale.code, "VERSION_CONFLICT");
    assert_eq!(lib.call("get_person", json!({ "personId": one["id"] }))["displayName"], "สมชาย");

    // ลบกลุ่มหนึ่ง: คนยังอยู่ และยังอยู่ในอีกกลุ่ม
    lib.call("delete_group", json!({ "groupId": team["id"] }));
    let after = lib.call("get_person", json!({ "personId": one["id"] }));
    assert_eq!(after["groups"], json!([{ "id": volunteers["id"], "name": "อาสาสมัคร" }]));
    assert_eq!(lib.call("list_groups", json!({}))[0]["memberCount"], 1);

    // ชื่อว่าง/ยาวเกิน ถูกปฏิเสธพร้อมเหตุผล ไม่ตัดเงียบ ๆ
    assert_eq!(
        lib.try_call("save_person", json!({ "requestId": new_id(), "displayName": "   " })).unwrap_err().code,
        "INVALID_INPUT"
    );
    let long = "ก".repeat(121);
    assert_eq!(
        lib.try_call("save_person", json!({ "requestId": new_id(), "displayName": long })).unwrap_err().code,
        "INVALID_INPUT"
    );
}

/// สี่รูป: A เดี่ยว (สองหน้า), B เดี่ยว, A+B, ไม่มีใคร
fn people_fixture(lib: &Lib) -> (String, String) {
    let root = lib.photos().join("2026-11-18 งานอบรม AI");
    for (i, name) in ["only_a.jpg", "only_b.jpg", "both.jpg", "nobody.jpg"].iter().enumerate() {
        write_jpeg(&root.join(name), i as u8 + 1);
    }
    lib.import(&root, "single_event", |_| true);
    let created = lib.label("only_a.jpg", 0.05, ("name", "คุณกานต์"));
    let a = created["createdPerson"]["id"].as_str().unwrap().to_string();
    lib.label("only_a.jpg", 0.6, ("id", &a)); // คนเดียวกันสองหน้าในภาพเดียว (เช่น เงาสะท้อน)
    let b = lib.label("only_b.jpg", 0.1, ("name", "คุณเมย์"))["createdPerson"]["id"].as_str().unwrap().to_string();
    lib.label("both.jpg", 0.05, ("id", &a));
    lib.label("both.jpg", 0.6, ("id", &b));
    (a, b)
}

#[test]
fn t17_and_or_search_counts_distinct_photos() {
    let lib = Lib::new();
    let (a, b) = people_fixture(&lib);

    let all = lib.query(json!({ "personIds": [a, b], "personMode": "all" }));
    assert_eq!(names(&all), ["both.jpg"], "AND: เฉพาะรูปที่มีทุกคน");
    assert_eq!(all["total"], 1);
    let any = lib.query(json!({ "personIds": [a, b], "personMode": "any" }));
    assert_eq!(names(&any), ["both.jpg", "only_a.jpg", "only_b.jpg"], "OR: ทั้งสามแบบ ไม่ซ้ำรูป");
    assert_eq!(lib.filenames(json!({ "personIds": [a, a, a] })), ["both.jpg", "only_a.jpg"], "ID ซ้ำในคำขอถูกรวม");
    assert_eq!(lib.filenames(json!({ "faceFilter": "no_person" })), ["nobody.jpg"]);

    // คนมีสองหน้าในภาพเดียว นับรูปนั้นครั้งเดียว
    let person_a = lib.call("get_person", json!({ "personId": a }));
    assert_eq!(person_a["confirmedPhotoCount"], 2);
    assert_eq!(person_a["confirmedEventCount"], 1);
    assert_eq!(lib.count("SELECT COUNT(*) FROM face_assignments WHERE revoked_at IS NULL"), 5);
    let both = &all["items"][0]["confirmedPeople"];
    assert_eq!(arr(both).len(), 2);

    // กิจกรรม: บุคคลที่พบพร้อมจำนวนรูป
    let events = lib.call("list_events", json!({}));
    let event = lib.call("get_event", json!({ "eventId": events[0]["id"] }));
    assert_eq!(event["people"][0]["person"]["displayName"], "คุณกานต์");
    assert_eq!(event["people"][0]["photoCount"], 2);
}

#[test]
fn t19_wrong_person_is_fixed_per_face_only() {
    let lib = Lib::new();
    let (a, b) = people_fixture(&lib);
    let both = lib.photo_id("both.jpg");
    let faces = lib.call("get_photo_details", json!({ "photoId": both.clone() }))["faces"].clone();
    let face_a = arr(&faces).iter().find(|f| f["person"]["id"] == a.as_str()).unwrap()["id"].clone();

    // "ไม่ใช่บุคคลนี้" บนหน้าของ A: B ในภาพเดียวกันไม่กระทบ และกรอบยังอยู่
    let after = lib.call("unassign_faces", json!({ "faceIds": [face_a.clone()] }));
    assert_eq!(arr(&after["faces"]).len(), 2);
    assert_eq!(lib.filenames(json!({ "personIds": [a.clone()] })), ["only_a.jpg"]);
    assert_eq!(lib.filenames(json!({ "personIds": [b.clone()] })), ["both.jpg", "only_b.jpg"]);

    // เปลี่ยนเป็นคนที่ถูก: เพิกถอนของเดิม + สร้างใหม่ ใบหน้าเดียวมีคนที่ยืนยันได้คนเดียว
    lib.call("assign_faces", json!({ "requestId": new_id(), "faceIds": [face_a.clone()], "personId": b.clone() }));
    assert_eq!(
        lib.count(
            "SELECT MAX(n) FROM (SELECT COUNT(*) n FROM face_assignments WHERE revoked_at IS NULL GROUP BY face_id)"
        ),
        1
    );
    assert_eq!(
        lib.call("get_person", json!({ "personId": b.clone() }))["confirmedPhotoCount"],
        2,
        "B สองหน้าใน both.jpg ยังนับเป็นหนึ่งรูป"
    );
    let req = new_id();
    let args = json!({ "requestId": req, "faceIds": [face_a.clone()], "personId": a.clone() });
    lib.call("assign_faces", args.clone());
    lib.call("assign_faces", args); // retry คำขอเดิม
    assert_eq!(lib.count("SELECT COUNT(*) FROM operations WHERE kind = 'assign_faces'"), 2);

    // "ไม่ใช่ใบหน้า": เอากรอบออกจากการแสดงผล แต่เก็บการตัดสินไว้
    let after = lib.call("ignore_faces", json!({ "faceIds": [face_a] }));
    assert_eq!(arr(&after["faces"]).len(), 1);
    assert_eq!(lib.count("SELECT COUNT(*) FROM faces WHERE status = 'ignored'"), 1);
    assert_eq!(lib.filenames(json!({ "personIds": [a] })), ["only_a.jpg"]);
    // รูปและชื่อบุคคลไม่ถูกลบ
    assert_eq!(lib.query(json!({}))["total"], 4);
    assert_eq!(arr(&lib.call("list_people", json!({}))).len(), 2);
}

#[test]
fn t23_manual_face_on_exif_rotated_photo() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานแนวตั้ง");
    std::fs::create_dir_all(&root).unwrap();
    // เซนเซอร์บันทึกแนวนอน 120×80 แต่ EXIF บอกให้หมุน 90° → ภาพที่ผู้ใช้เห็นคือ 80×120
    std::fs::write(root.join("portrait.jpg"), jpeg_with_exif(120, 80, 6, "2026:11:18 09:30:00")).unwrap();
    std::fs::write(root.join("landscape.jpg"), jpeg_with_exif(120, 80, 1, "2026:11:17 08:00:00")).unwrap();
    lib.import(&root, "single_event", |_| true);

    let id = lib.photo_id("portrait.jpg");
    let details = lib.call("get_photo_details", json!({ "photoId": id.clone() }));
    assert_eq!((details["width"].as_i64(), details["height"].as_i64()), (Some(80), Some(120)), "ขนาดตามภาพที่หมุนแล้ว");
    assert_eq!(details["takenAt"], "2026-11-18T09:30:00");
    assert_eq!(details["takenSource"], "exif");
    let thumb = puri_core::api::asset(&lib.app, &format!("thumb/{id}")).unwrap();
    let thumb = image::load_from_memory(&thumb.bytes).unwrap();
    assert!(thumb.height() > thumb.width(), "ภาพย่อหมุนถูกทิศ");

    let result = lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id.clone(), "bbox": { "x": 0.25, "y": 0.1, "w": 0.5, "h": 0.3 }, "newPersonName": "ภูริ" }),
    );
    let face = &result["faces"][0];
    assert_eq!(face["origin"], "manual");
    assert_eq!(face["bbox"], json!({ "x": 0.25, "y": 0.1, "w": 0.5, "h": 0.3 }), "พิกัดเก็บตามที่วาดบนภาพหมุนแล้ว");
    assert_eq!(lib.filenames(json!({ "text": "ภูริ" })), ["portrait.jpg"], "ชื่อที่ตั้งค้นเจอทันที ไม่ต้องสแกนซ้ำ");
    let crop = puri_core::api::asset(&lib.app, &format!("face/{}", face["id"].as_str().unwrap())).unwrap();
    let crop = image::load_from_memory(&crop.bytes).unwrap();
    assert_eq!(crop.width(), crop.height());

    // กรอบนอกภาพ/เล็กเกินถูกปฏิเสธ; กรอบล้นขอบถูกหนีบให้อยู่ในภาพ
    let bad = lib.try_call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id.clone(), "bbox": { "x": 0.5, "y": 0.5, "w": 0.001, "h": 0.001 }, "newPersonName": "x" }),
    );
    assert_eq!(bad.unwrap_err().code, "INVALID_INPUT");
    let clamped = lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id, "bbox": { "x": 0.8, "y": 0.8, "w": 0.5, "h": 0.5 }, "personId": lib.person_id("ภูริ") }),
    );
    let last = arr(&clamped["faces"]).iter().find(|f| f["bbox"]["x"] == 0.8).unwrap();
    assert!(
        (last["bbox"]["w"].as_f64().unwrap() - 0.2).abs() < 1e-9
            && (last["bbox"]["h"].as_f64().unwrap() - 0.2).abs() < 1e-9
    );

    // วันถ่าย: เรียงและกรองตามเวลาท้องถิ่นจาก EXIF
    assert_eq!(lib.filenames(json!({ "sort": "taken_asc" })), ["landscape.jpg", "portrait.jpg"]);
    let day = json!({ "field": "photo_taken", "from": "2026-11-18", "to": "2026-11-18", "includeUnknown": false });
    assert_eq!(lib.filenames(json!({ "date": day })), ["portrait.jpg"]);
}

fn export_fixture(lib: &Lib) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = lib.photos().join("งานรวม");
    write_jpeg(&root.join("2026-11-18 งานเช้า/IMG_0001.jpg"), 1);
    write_jpeg(&root.join("2026-11-18 งานเช้า/IMG_0002.jpg"), 2);
    write_jpeg(&root.join("2026-11-19 งานบ่าย/IMG_0001.jpg"), 3); // ชื่อชนกับงานเช้า
    lib.import(&root, "child_events", |_| true);
    let dest = lib.tmp.path().join("ส่งออก");
    std::fs::create_dir_all(&dest).unwrap();
    (root, dest)
}

fn run_export(lib: &Lib, selection: Value, dest: &Path) -> (Value, Value) {
    let plan = lib.call("preview_export", json!({ "selection": selection, "destination": dest }));
    assert_eq!(plan["canStart"], true, "{plan}");
    let req = new_id();
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": req }));
    let again = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": req }));
    assert_eq!(job["id"], again["id"], "กดเริ่มซ้ำได้งานเดิม");
    lib.run_jobs();
    (plan, lib.call("get_export_report", json!({ "jobId": job["id"] })))
}

#[test]
fn t27_export_copies_without_overwriting_and_reports_every_input() {
    let lib = Lib::new();
    let (root, dest) = export_fixture(&lib);
    std::fs::write(dest.join("IMG_0002.jpg"), b"existing file that must survive").unwrap();
    let before = tree_state(&root);

    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    // ต้นฉบับหนึ่งรูปหายก่อนส่งออก
    let gone = root.join("2026-11-19 งานบ่าย/IMG_0001.jpg");
    let parked = lib.tmp.path().join("gone.jpg");
    std::fs::rename(&gone, &parked).unwrap();

    let (plan, report) = run_export(&lib, json!({ "kind": "explicit", "photoIds": ids }), &dest);
    assert_eq!(plan["inputCount"], 3);
    assert_eq!(plan["plannedOutputCount"], 2);
    assert_eq!(plan["unavailableInputs"], 1);
    assert_eq!(plan["renamedCount"], 1);
    assert_eq!(report["inputCount"], 3, "รายงานครบทุกรูปที่เลือก");
    assert_eq!(
        (report["copied"].as_i64(), report["unavailable"].as_i64(), report["failed"].as_i64()),
        (Some(2), Some(1), Some(0))
    );

    assert_eq!(
        std::fs::read(dest.join("IMG_0002.jpg")).unwrap(),
        b"existing file that must survive",
        "ไม่เขียนทับไฟล์ปลายทางเดิม"
    );
    assert_eq!(
        std::fs::read(dest.join("IMG_0002 (2).jpg")).unwrap(),
        std::fs::read(root.join("2026-11-18 งานเช้า/IMG_0002.jpg")).unwrap()
    );
    assert_eq!(
        std::fs::read(dest.join("IMG_0001.jpg")).unwrap(),
        std::fs::read(root.join("2026-11-18 งานเช้า/IMG_0001.jpg")).unwrap(),
        "สำเนา byte ตรงต้นฉบับ"
    );
    let leftovers: Vec<_> = std::fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "ไม่เหลือไฟล์ชั่วคราว: {leftovers:?}");

    std::fs::rename(&parked, &gone).unwrap();
    assert_eq!(tree_state(&root), before, "ต้นฉบับไม่ถูกแก้ ย้าย หรือเปลี่ยนเวลา");

    // ชื่อชนระหว่างสองงานในการส่งออกเดียวกัน: เติมเลขท้าย ไม่ทับกันเอง
    let dest2 = lib.tmp.path().join("ส่งออก2");
    std::fs::create_dir_all(&dest2).unwrap();
    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    let (_, report) = run_export(&lib, json!({ "kind": "explicit", "photoIds": ids }), &dest2);
    assert_eq!(report["copied"], 3);
    let mut out: Vec<String> =
        std::fs::read_dir(&dest2).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    out.sort();
    assert_eq!(out, ["IMG_0001 (2).jpg", "IMG_0001.jpg", "IMG_0002.jpg"]);

    // ปลายทางอยู่ในโฟลเดอร์ที่คลังติดตาม: ไม่ให้เริ่ม
    let inside = root.join("2026-11-18 งานเช้า");
    let plan = lib.call(
        "preview_export",
        json!({ "selection": { "kind": "explicit", "photoIds": [ids[0]] }, "destination": inside }),
    );
    assert_eq!(plan["canStart"], false);
    assert!(plan["blockedReason"].as_str().unwrap().contains("ติดตาม"));
}

#[test]
fn t27c_export_resized_copies_keep_dates_and_leave_originals_alone() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานย่อ");
    std::fs::create_dir_all(&root).unwrap();
    // กล้องบอกให้หมุน 90° (orientation 6): ภาพจริงเป็นแนวตั้ง 1200×1600
    std::fs::write(root.join("IMG_A.jpg"), jpeg_with_exif(1600, 1200, 6, "2026:11:18 09:15:00")).unwrap();
    write_jpeg(&root.join("IMG_B.jpg"), 2); // 64×48 เล็กกว่าที่ขอ: คัดลอกไฟล์เดิม
    std::fs::write(root.join("scan.png"), {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(1000, 800, |x, y| image::Rgb([x as u8, y as u8, 90])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    })
    .unwrap();
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    let selection = json!({ "kind": "explicit", "photoIds": ids });
    let dest = lib.tmp.path().join("ส่งออกย่อ");
    std::fs::create_dir_all(&dest).unwrap();

    // ค่าที่อยู่นอกช่วงถูกปฏิเสธตั้งแต่ตรวจแผน
    let bad = lib.try_call(
        "preview_export",
        json!({ "selection": selection, "destination": dest, "resize": { "mode": "long_edge", "px": 100 } }),
    );
    assert_eq!(bad.unwrap_err().code, "INVALID_INPUT");

    let plan = lib.call(
        "preview_export",
        json!({ "selection": selection, "destination": dest, "resize": { "mode": "long_edge", "px": 640 } }),
    );
    assert_eq!((plan["plannedOutputCount"].as_i64(), plan["resizedCount"].as_i64()), (Some(3), Some(2)));
    assert_eq!(plan["resize"], json!({ "mode": "long_edge", "px": 640 }));
    assert!(
        arr(&plan["sampleNames"]).iter().any(|s| s["source"] == "scan.png" && s["output"] == "scan.jpg"),
        "สำเนาที่ย่อเป็น JPEG: {plan}"
    );
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    assert!(job["title"].as_str().unwrap().contains("ย่อขนาด"));
    lib.run_jobs();
    let report = lib.call("get_export_report", json!({ "jobId": job["id"] }));
    assert_eq!((report["copied"].as_i64(), report["failed"].as_i64()), (Some(3), Some(0)), "{report}");
    assert_eq!(report["resize"]["mode"], "long_edge");
    assert!(report["outputBytes"].as_u64().unwrap() < report["sourceBytes"].as_u64().unwrap());

    // หมุนตั้งตรง ด้านยาว 640 เก็บวันถ่ายไว้ และแอปอื่นไม่หมุนซ้ำ
    let out = std::fs::read(dest.join("IMG_A.jpg")).unwrap();
    let img = image::load_from_memory(&out).unwrap();
    assert_eq!((img.width(), img.height()), (480, 640));
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
    assert_eq!(exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY).unwrap().value.get_uint(0), Some(1));
    assert_eq!(
        exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).unwrap().display_value().to_string(),
        "2026-11-18 09:15:00"
    );
    assert_eq!(
        std::fs::metadata(dest.join("IMG_A.jpg")).unwrap().modified().unwrap(),
        std::fs::metadata(root.join("IMG_A.jpg")).unwrap().modified().unwrap(),
        "เวลาแก้ไขไฟล์เท่าต้นฉบับ"
    );
    assert_eq!(std::fs::read(dest.join("IMG_B.jpg")).unwrap(), std::fs::read(root.join("IMG_B.jpg")).unwrap());
    let scan =
        image::load_from_memory_with_format(&std::fs::read(dest.join("scan.jpg")).unwrap(), image::ImageFormat::Jpeg)
            .unwrap();
    assert_eq!((scan.width(), scan.height()), (640, 512));
    assert_eq!(tree_state(&root), before, "ต้นฉบับไม่ถูกแก้ ย้าย หรือเปลี่ยนเวลา");

    // ขนาดไฟล์ไม่เกินที่ตั้ง: รูปที่ใหญ่กว่าถูกย่อจนพอดี รูปที่เล็กพออยู่แล้วคัดลอกไฟล์เดิม
    let dest2 = lib.tmp.path().join("ส่งออกขนาดไฟล์");
    std::fs::create_dir_all(&dest2).unwrap();
    let limit = 60_000u64;
    let plan = lib.call(
        "preview_export",
        json!({ "selection": selection, "destination": dest2, "resize": { "mode": "max_bytes", "bytes": limit } }),
    );
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let report = lib.call("get_export_report", json!({ "jobId": job["id"] }));
    assert_eq!(report["copied"], 3, "{report}");
    for name in ["IMG_A.jpg", "IMG_B.jpg"] {
        let src = std::fs::metadata(root.join(name)).unwrap().len();
        let out = std::fs::metadata(dest2.join(name)).unwrap().len();
        assert!(out <= limit.max(src) && out <= src, "{name}: {out} (ต้นฉบับ {src})");
    }
    assert_eq!(tree_state(&root), before);
}

fn db_text(lib: &Lib, sql: &str, arg: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(lib.tmp.path().join("data/library.sqlite")).unwrap();
    conn.query_row(sql, [arg], |r| r.get(0)).unwrap()
}

/// ต้นฉบับที่ถูกย้ายไปถังขยะระหว่างเทสต์: ตรวจว่าเป็นไฟล์เดิมทุกไบต์ แล้วลบออกจากถังขยะของเครื่อง
/// (Windows ไม่บอกตำแหน่งในถังขยะ — runner ของ CI ถูกทิ้งหลังจบงานอยู่แล้ว)
fn take_from_trash(lib: &Lib, filename: &str, original: Option<&[u8]>) {
    let trashed =
        db_text(lib, "SELECT trashed_path FROM shrink_items WHERE filename = ?1 AND status = 'shrunk'", filename);
    if cfg!(target_os = "macos") {
        let trashed = trashed.unwrap_or_else(|| panic!("{filename}: ไม่รู้ตำแหน่งในถังขยะ"));
        if let Some(original) = original {
            assert_eq!(std::fs::read(&trashed).unwrap(), original, "{filename}: ต้นฉบับในถังขยะครบทุกไบต์");
        }
        let _ = std::fs::remove_file(&trashed);
    }
}

fn no_temp_files(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir).into_iter().all(|e| !e.unwrap().file_name().to_string_lossy().contains("puri-shrink"))
}

#[test]
fn t27d_shrink_originals_in_place_keeps_faces_names_and_dates() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-20 งานย่อต้นฉบับ");
    std::fs::create_dir_all(&root).unwrap();
    // เซนเซอร์บันทึก 3000×2000 แต่ EXIF บอกให้หมุน 90° → ภาพที่เห็นคือแนวตั้ง 2000×3000
    let original = jpeg_with_exif(3000, 2000, 6, "2026:11:20 08:15:00");
    std::fs::write(root.join("big.jpg"), &original).unwrap();
    image::RgbImage::from_fn(2400, 1600, |x, y| image::Rgb([x as u8, y as u8, 40]))
        .save_with_format(root.join("scan.png"), image::ImageFormat::Png)
        .unwrap();
    write_jpeg(&root.join("small.jpg"), 3);
    lib.import(&root, "single_event", |_| true);
    let mtime = std::fs::metadata(root.join("big.jpg")).unwrap().modified().unwrap();
    let others = ["scan.png", "small.jpg"].map(|name| (name, std::fs::read(root.join(name)).unwrap()));

    // คำตัดสินของผู้ใช้บนรูปที่จะย่อ: ใบหน้าที่วาดเองพร้อมชื่อ
    let big = lib.photo_id("big.jpg");
    let face = lib.label("big.jpg", 0.3, ("name", "คุณกานต์"))["faces"][0].clone();
    let revision = db_text(&lib, "SELECT current_revision_id FROM photos WHERE id = ?1", &big).unwrap();
    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    let selection = json!({ "kind": "explicit", "photoIds": ids });

    // ไม่ยอมย่อต่ำกว่าภาพที่ตัววิเคราะห์ใบหน้าใช้ และต้องเลือกขนาดก่อน
    for resize in [
        json!({ "mode": "long_edge", "px": 1080 }),
        json!({ "mode": "max_bytes", "bytes": 200 * 1024 }),
        json!({ "mode": "original" }),
    ] {
        let err = lib.try_call("preview_shrink", json!({ "selection": selection, "resize": resize })).unwrap_err();
        assert_eq!(err.code, "INVALID_INPUT", "{resize}");
    }

    let plan =
        lib.call("preview_shrink", json!({ "selection": selection, "resize": { "mode": "long_edge", "px": 1600 } }));
    assert_eq!(
        (plan["plannedCount"].as_i64(), plan["alreadySmall"].as_i64(), plan["notJpeg"].as_i64()),
        (Some(1), Some(1), Some(1)),
        "{plan}"
    );
    assert!(plan["estimatedBytes"].as_u64().unwrap() < plan["sourceBytes"].as_u64().unwrap());
    let job = lib.call("start_shrink", json!({ "planId": plan["planId"], "requestId": new_id() }));
    assert_eq!(job["kind"], "shrink");
    lib.run_jobs();
    let report = lib.call("get_shrink_report", json!({ "jobId": job["id"] }));
    assert_eq!(
        (report["shrunk"].as_i64(), report["skipped"].as_i64(), report["failed"].as_i64()),
        (Some(1), Some(2), Some(0)),
        "{report}"
    );
    assert!(report["outputBytes"].as_u64().unwrap() < report["sourceBytes"].as_u64().unwrap());

    // ไฟล์ที่ที่เดิม: หมุนตั้งตรง ด้านยาว 1600 เก็บวันถ่าย เวลาแก้ไขไฟล์เท่าต้นฉบับ
    let out = std::fs::read(root.join("big.jpg")).unwrap();
    let img = image::load_from_memory(&out).unwrap();
    assert_eq!((img.width(), img.height()), (1067, 1600));
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
    assert_eq!(exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY).unwrap().value.get_uint(0), Some(1));
    assert_eq!(
        exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).unwrap().display_value().to_string(),
        "2026-11-20 08:15:00"
    );
    assert_eq!(std::fs::metadata(root.join("big.jpg")).unwrap().modified().unwrap(), mtime);
    for (name, bytes) in &others {
        assert_eq!(&std::fs::read(root.join(name)).unwrap(), bytes, "{name} ไม่ถูกแตะ");
    }
    assert!(no_temp_files(&root));
    take_from_trash(&lib, "big.jpg", Some(&original));

    // คลัง: revision เดิม ใบหน้าเดิม ชื่อเดิม ขนาดใหม่
    assert_eq!(db_text(&lib, "SELECT current_revision_id FROM photos WHERE id = ?1", &big).unwrap(), revision);
    let details = lib.call("get_photo_details", json!({ "photoId": big.clone() }));
    assert_eq!((details["width"].as_i64(), details["height"].as_i64()), (Some(1067), Some(1600)));
    assert_eq!(details["takenAt"], "2026-11-20T08:15:00");
    assert_eq!(details["faces"][0]["id"], face["id"]);
    assert_eq!(details["faces"][0]["bbox"], face["bbox"], "กรอบเป็นสัดส่วนของภาพ จึงตรงตำแหน่งเดิม");
    assert_eq!(lib.filenames(json!({ "text": "คุณกานต์" })), ["big.jpg"]);
    puri_core::api::asset(&lib.app, &format!("face/{}", face["id"].as_str().unwrap())).unwrap();

    // ตรวจรูปใหม่: ไฟล์ที่ย่อไม่ถูกมองเป็นเนื้อหาใหม่ (ไม่สร้าง revision ใหม่ ไม่วิเคราะห์ซ้ำ)
    let event_id = lib.call("list_events", json!({}))[0]["id"].clone();
    lib.call("rescan_event", json!({ "eventId": event_id }));
    lib.run_jobs();
    assert_eq!(lib.count("SELECT COUNT(*) FROM photo_revisions"), 3);
    assert_eq!(db_text(&lib, "SELECT current_revision_id FROM photos WHERE id = ?1", &big).unwrap(), revision);
    assert_eq!(lib.filenames(json!({ "text": "คุณกานต์" })), ["big.jpg"]);

    // ย่อซ้ำ: เล็กพออยู่แล้วทั้งหมด จึงไม่มีอะไรให้เริ่ม
    let again =
        lib.call("preview_shrink", json!({ "selection": selection, "resize": { "mode": "long_edge", "px": 1600 } }));
    assert_eq!(again["plannedCount"], 0);
    assert_eq!(
        lib.try_call("start_shrink", json!({ "planId": again["planId"], "requestId": new_id() })).unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn t27e_shrink_interrupted_at_any_step_finishes_or_reverts_on_next_open() {
    use sha2::Digest;
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-20 ปิดกลางคัน");
    std::fs::create_dir_all(&root).unwrap();
    let names = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"];
    for name in names {
        std::fs::write(root.join(name), jpeg_with_exif(2400, 1600, 1, "2026:11:20 09:00:00")).unwrap();
    }
    lib.import(&root, "single_event", |_| true);
    lib.label("a.jpg", 0.4, ("name", "คุณเอ"));
    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    let plan = lib.call(
        "preview_shrink",
        json!({ "selection": { "kind": "explicit", "photoIds": ids }, "resize": { "mode": "long_edge", "px": 1600 } }),
    );
    let job = lib.call("start_shrink", json!({ "planId": plan["planId"], "requestId": new_id() }));

    // จำลองแอปปิดตรงแต่ละขั้น ด้วยไฟล์และสถานะเดียวกับที่งานจริงทิ้งไว้
    let original = std::fs::read(root.join("a.jpg")).unwrap();
    let resized = puri_core::media::export_jpeg(&original, puri_core::media::Fit::LongEdge(1600), None).unwrap();
    let sha = puri_core::util::hex(&sha2::Sha256::digest(&resized));
    let (w, h) = image::load_from_memory(&resized).map(|i| (i.width(), i.height())).unwrap();
    let conn = rusqlite::Connection::open(lib.tmp.path().join("data/library.sqlite")).unwrap();
    let mark = |name: &str, status: &str, temp: &Path| {
        conn.execute(
            "UPDATE shrink_items SET status = ?2, temp_path = ?3, new_sha256 = ?4, new_width = ?5, new_height = ?6 WHERE filename = ?1",
            rusqlite::params![name, status, temp.to_string_lossy(), sha, w, h],
        )
        .unwrap();
    };
    let temp = |name: &str| root.join(format!(".puri-shrink-test-{name}.part"));
    let away = lib.tmp.path().join("ถังขยะจำลอง");
    std::fs::create_dir_all(&away).unwrap();
    // a: ต้นฉบับไปถังขยะแล้ว ไฟล์ที่ย่อยังเป็นไฟล์ชั่วคราว
    std::fs::write(temp("a"), &resized).unwrap();
    std::fs::rename(root.join("a.jpg"), away.join("a.jpg")).unwrap();
    mark("a.jpg", "replacing", &temp("a"));
    // b: เขียนไฟล์ที่ย่อแล้ว แต่ต้นฉบับยังไม่ถูกย้าย
    std::fs::write(temp("b"), &resized).unwrap();
    mark("b.jpg", "replacing", &temp("b"));
    // c: วางไฟล์ที่ย่อแทนแล้ว แต่ยังไม่ได้บันทึกในคลัง
    std::fs::rename(root.join("c.jpg"), away.join("c.jpg")).unwrap();
    std::fs::write(root.join("c.jpg"), &resized).unwrap();
    mark("c.jpg", "replacing", &temp("c"));
    // d: ไฟล์ชั่วคราวค้างก่อนถึงขั้นแทนที่
    std::fs::write(temp("d"), &resized).unwrap();
    mark("d.jpg", "planned", &temp("d"));
    drop(conn);

    let lib = lib.reopen();
    let status = |name: &str| db_text(&lib, "SELECT status FROM shrink_items WHERE filename = ?1", name).unwrap();
    assert_eq!(
        (status("a.jpg"), status("b.jpg"), status("c.jpg"), status("d.jpg")),
        ("shrunk".into(), "planned".into(), "shrunk".into(), "planned".into())
    );
    assert!(no_temp_files(&root), "ไม่มีไฟล์ชั่วคราวค้างในโฟลเดอร์ของผู้ใช้");
    for name in ["a.jpg", "c.jpg"] {
        assert_eq!(std::fs::read(root.join(name)).unwrap(), resized);
        let size = lib.count(&format!(
            "SELECT r.size FROM photo_revisions r JOIN photos p ON p.current_revision_id = r.id JOIN photo_locations l ON l.photo_id = p.id WHERE l.filename = '{name}'"
        ));
        assert_eq!(size as usize, resized.len(), "{name}: คลังรู้จักไฟล์ที่ย่อแล้ว");
    }
    assert_eq!(std::fs::read(root.join("b.jpg")).unwrap(), original, "b: ต้นฉบับอยู่ที่เดิม");
    assert_eq!(lib.filenames(json!({ "text": "คุณเอ" })), ["a.jpg"]);

    // งานทำต่อเฉพาะรูปที่ยังไม่เสร็จ
    lib.run_jobs();
    let report = lib.call("get_shrink_report", json!({ "jobId": job["id"] }));
    assert_eq!(
        (report["shrunk"].as_i64(), report["failed"].as_i64(), report["pending"].as_i64()),
        (Some(4), Some(0), Some(0)),
        "{report}"
    );
    take_from_trash(&lib, "b.jpg", Some(&original));
    take_from_trash(&lib, "d.jpg", Some(&original));
    let event_id = lib.call("list_events", json!({}))[0]["id"].clone();
    lib.call("rescan_event", json!({ "eventId": event_id }));
    lib.run_jobs();
    assert_eq!(lib.count("SELECT COUNT(*) FROM photo_revisions"), 4, "ไม่มีรูปถูกมองเป็นเนื้อหาใหม่");
    assert_eq!(lib.filenames(json!({ "text": "คุณเอ" })), ["a.jpg"]);
}

#[test]
fn t27f_export_with_watermark_marks_every_copy_and_never_the_original() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-20 ลายน้ำ");
    std::fs::create_dir_all(&root).unwrap();
    // กล้องสั่งหมุน 90°: ภาพที่เห็นเป็นแนวตั้ง 1200×1600
    std::fs::write(root.join("IMG_A.jpg"), jpeg_with_exif(1600, 1200, 6, "2026:11:20 10:00:00")).unwrap();
    write_jpeg(&root.join("IMG_B.jpg"), 2);
    write_png(&root.join("scan.png"));
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);

    // โลโก้ PNG 400×200: ครึ่งซ้ายน้ำเงินทึบ ครึ่งขวาโปร่งใส · ไฟล์ที่ไม่ใช่รูปถูกปฏิเสธ
    let logo = lib.tmp.path().join("โลโก้วัด.png");
    image::RgbaImage::from_fn(400, 200, |x, _| image::Rgba(if x < 200 { [0, 0, 255, 255] } else { [0, 0, 0, 0] }))
        .save_with_format(&logo, image::ImageFormat::Png)
        .unwrap();
    let junk = lib.tmp.path().join("notes.txt");
    std::fs::write(&junk, "hello").unwrap();
    assert_eq!(lib.try_call("add_watermark", json!({ "path": junk })).unwrap_err().code, "UNSUPPORTED_FORMAT");
    let wm = lib.call("add_watermark", json!({ "path": logo }));
    assert_eq!(
        (wm["name"].as_str(), wm["transparent"].as_bool(), wm["width"].as_i64()),
        (Some("โลโก้วัด"), Some(true), Some(400))
    );
    assert_eq!(arr(&lib.call("list_watermarks", json!({}))).len(), 1);
    let id = wm["id"].as_str().unwrap().to_string();
    let png = puri_core::api::asset(&lib.app, &format!("watermark/{id}")).unwrap();
    assert_eq!((png.mime, image::load_from_memory(&png.bytes).unwrap().width()), ("image/png", 400));

    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    let selection = json!({ "kind": "explicit", "photoIds": ids });
    let spec = |id: &str, scale: u32| json!({ "id": id, "anchor": "bottom_right", "scale": scale, "opacity": 100 });
    let plan_for = |dest: &Path, watermark: Value| {
        std::fs::create_dir_all(dest).unwrap();
        lib.try_call("preview_export", json!({ "selection": selection, "destination": dest, "watermark": watermark }))
    };
    let dest = lib.tmp.path().join("ส่งออกลายน้ำ");
    assert_eq!(plan_for(&dest, spec(&id, 200)).unwrap_err().code, "INVALID_INPUT");
    assert_eq!(plan_for(&dest, spec("ไม่มีจริง", 25)).unwrap_err().code, "NOT_FOUND");

    // ขนาดเดิม + ลายน้ำ: ทุกรูปถูกบันทึกใหม่ (PNG เป็น .jpg) แต่ไม่มีรูปไหนถูกย่อ
    let plan = plan_for(&dest, spec(&id, 25)).unwrap();
    assert_eq!((plan["plannedOutputCount"].as_i64(), plan["resizedCount"].as_i64()), (Some(3), Some(0)), "{plan}");
    assert_eq!(plan["watermark"]["anchor"], "bottom_right");
    assert!(arr(&plan["sampleNames"]).iter().any(|s| s["output"] == "scan.jpg"), "{plan}");
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    assert!(job["title"].as_str().unwrap().contains("ลายน้ำ"));
    lib.run_jobs();
    let report = lib.call("get_export_report", json!({ "jobId": job["id"] }));
    assert_eq!((report["copied"].as_i64(), report["failed"].as_i64()), (Some(3), Some(0)), "{report}");
    assert_eq!(report["watermark"]["id"], id.as_str());

    // ขนาดเดิมและตั้งตรง วันถ่ายอยู่ครบ · โลโก้กว้าง 300 (25% ของด้านสั้น 1200) ชิดขวาล่าง ขอบ 36 → กรอบเริ่มที่ (864, 1414)
    let out = std::fs::read(dest.join("IMG_A.jpg")).unwrap();
    let img = image::load_from_memory(&out).unwrap().to_rgb8();
    assert_eq!(img.dimensions(), (1200, 1600));
    let far = |p: [u8; 3], q: [u8; 3]| p.iter().zip(q).map(|(a, b)| a.abs_diff(b)).max().unwrap();
    assert!(far(img.get_pixel(900, 1489).0, [0, 0, 255]) <= 20, "ส่วนทึบของโลโก้: {:?}", img.get_pixel(900, 1489));
    assert!(far(img.get_pixel(1100, 1489).0, [0, 0, 255]) > 50, "ส่วนโปร่งใสเห็นรูปเดิม");
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
    assert_eq!(exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY).unwrap().value.get_uint(0), Some(1));
    assert!(exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).is_some());
    assert_eq!(image::load_from_memory(&std::fs::read(dest.join("IMG_B.jpg")).unwrap()).unwrap().width(), 64);
    assert_eq!(tree_state(&root), before, "ต้นฉบับไม่ถูกแตะ");

    // ลบลายน้ำหลังตรวจแผน: เริ่มงานไม่ได้ · ลบหลังเริ่มงาน: งานหยุดรอพร้อมเหตุผล ไม่ส่งออกสำเนาที่ไม่มีลายน้ำแทน
    let stale = plan_for(&lib.tmp.path().join("ส่งออก2"), spec(&id, 25)).unwrap();
    assert_eq!(arr(&lib.call("delete_watermark", json!({ "id": id }))).len(), 0);
    let err = lib.try_call("start_export", json!({ "planId": stale["planId"], "requestId": new_id() })).unwrap_err();
    assert_eq!(err.code, "NOT_FOUND");
    let id2 = lib.call("add_watermark", json!({ "path": logo }))["id"].as_str().unwrap().to_string();
    let dest3 = lib.tmp.path().join("ส่งออก3");
    let plan = plan_for(&dest3, spec(&id2, 25)).unwrap();
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.call("delete_watermark", json!({ "id": id2 }));
    lib.run_jobs();
    let job = lib.call("get_job", json!({ "jobId": job["id"] }));
    assert_eq!(job["state"], "blocked", "{job}");
    assert!(job["blockedReason"].as_str().unwrap().contains("ลายน้ำ"));
    assert_eq!(std::fs::read_dir(&dest3).unwrap().count(), 0);
}

#[test]
fn t27b_export_refuses_source_changed_since_import_and_cancel_keeps_finished_copies() {
    let lib = Lib::new();
    let (root, dest) = export_fixture(&lib);
    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    // ไฟล์ถูกแก้หลังเพิ่มเข้าคลังและยังไม่ได้ตรวจรูปใหม่: ไม่อ้างว่าเป็นสำเนาของรุ่นที่รู้จัก
    std::fs::write(root.join("2026-11-18 งานเช้า/IMG_0002.jpg"), jpeg_bytes(20, 20, 250)).unwrap();
    let (_, report) = run_export(&lib, json!({ "kind": "explicit", "photoIds": ids.clone() }), &dest);
    assert_eq!((report["copied"].as_i64(), report["failed"].as_i64()), (Some(2), Some(1)));
    let failed = arr(&report["items"]).iter().find(|i| i["status"] == "failed").unwrap();
    assert_eq!(failed["filename"], "IMG_0002.jpg");
    assert!(!dest.join("IMG_0002.jpg").exists());

    // ยกเลิกงานที่ยังรอคิว: ทุกรายการถูกรายงานว่ายกเลิก ไม่มีไฟล์ถูกสร้าง
    let dest2 = lib.tmp.path().join("ยกเลิก");
    std::fs::create_dir_all(&dest2).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": ids }, "destination": dest2 }));
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    assert_eq!(lib.call("cancel_job", json!({ "jobId": job["id"] }))["state"], "cancelled");
    lib.run_jobs();
    let report = lib.call("get_export_report", json!({ "jobId": job["id"] }));
    assert_eq!(report["cancelled"], 3);
    assert_eq!(std::fs::read_dir(&dest2).unwrap().count(), 0);
}

#[test]
fn t26_select_all_results_is_a_frozen_snapshot_beyond_the_loaded_page() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานใหญ่");
    for i in 0..7 {
        write_jpeg(&root.join(format!("IMG_{i:04}.jpg")), i as u8);
    }
    let result = lib.import(&root, "single_event", |_| true);

    // UI โหลดหน้าแรกแค่ 3 รูป
    let page = lib.query(json!({ "limit": 3 }));
    assert_eq!(arr(&page["items"]).len(), 3);
    assert_eq!(page["total"], 7);
    let page2 = lib.query(json!({ "limit": 3, "cursor": page["nextCursor"] }));
    assert_eq!(names(&page2), ["IMG_0003.jpg", "IMG_0004.jpg", "IMG_0005.jpg"], "หน้าถัดไปไม่วนผลเดิม");

    let snapshot = lib.call("create_selection", json!({ "personIds": [], "personMode": "all", "eventIds": [], "faceFilter": "any", "sort": "filename", "limit": 3 }));
    assert_eq!(snapshot["count"], 7, "เลือกผลค้นหาทั้งหมด ไม่ใช่แค่หน้าที่โหลด");

    // มีรูปใหม่เข้ามาหลังเลือก: ต้องไม่เข้าชุดส่งออกเอง
    write_jpeg(&root.join("IMG_9999.jpg"), 99);
    lib.call("rescan_event", json!({ "eventId": result["eventIds"][0] }));
    lib.run_jobs();
    assert_eq!(lib.query(json!({}))["total"], 8);

    let dest = lib.tmp.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    let excluded = lib.photo_id("IMG_0000.jpg");
    let (plan, report) = run_export(
        &lib,
        json!({ "kind": "snapshot", "selectionToken": snapshot["selectionToken"], "excludedPhotoIds": [excluded] }),
        &dest,
    );
    assert_eq!(plan["inputCount"], 6);
    assert_eq!(report["copied"], 6);
    assert!(
        !dest.join("IMG_9999.jpg").exists()
            && !dest.join("IMG_0000.jpg").exists()
            && dest.join("IMG_0006.jpg").exists()
    );
    let stale = lib.try_call("preview_export", json!({ "selection": { "kind": "snapshot", "selectionToken": "nope", "excludedPhotoIds": [] }, "destination": dest }));
    assert_eq!(stale.unwrap_err().code, "PREVIEW_STALE");
}

#[test]
fn t30_thai_and_special_character_search_is_literal() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-18 งานค้นหา");
    // Windows ไม่ยอมให้ชื่อไฟล์มีเครื่องหมาย " จึงทดสอบชื่อนั้นเฉพาะระบบที่สร้างไฟล์ได้
    let quoted = (!cfg!(windows)).then_some("it's \"quoted\".jpg");
    let names: Vec<&str> =
        ["ภูริ.jpg", "100%_done.jpg", "100x_done.jpg", "น้ำ.jpg", "นำ.jpg", "😀 emoji.jpg", "'; DROP TABLE photos;--.jpg"]
            .into_iter()
            .chain(quoted)
            .collect();
    for name in &names {
        write_jpeg(&root.join(name), name.len() as u8);
    }
    lib.import(&root, "single_event", |_| true);
    assert_eq!(lib.query(json!({}))["total"], names.len());

    assert_eq!(lib.filenames(json!({ "text": "ภู" })), ["ภูริ.jpg"], "คำไทยสั้นสองตัวอักษรค้นแบบ substring ได้");
    assert_eq!(lib.filenames(json!({ "text": "100%" })), ["100%_done.jpg"], "% เป็นตัวอักษรธรรมดา ไม่ใช่ wildcard");
    assert_eq!(lib.filenames(json!({ "text": "0%_d" })), ["100%_done.jpg"], "_ เป็นตัวอักษรธรรมดา");
    if let Some(quoted) = quoted {
        assert_eq!(lib.filenames(json!({ "text": "\"quoted\"" })), [quoted]);
    }
    assert_eq!(lib.filenames(json!({ "text": "น้ำ" })), ["น้ำ.jpg"], "ไม่ตัดวรรณยุกต์เพื่อให้ตรงมากขึ้น");
    assert_eq!(lib.filenames(json!({ "text": "นำ" })), ["นำ.jpg"]);
    assert_eq!(lib.filenames(json!({ "text": "😀" })), ["😀 emoji.jpg"]);
    assert_eq!(
        lib.filenames(json!({ "text": "DROP TABLE" })),
        ["'; DROP TABLE photos;--.jpg"],
        "ข้อความไม่ถูกตีความเป็นคำสั่ง"
    );
    assert_eq!(lib.query(json!({}))["total"], names.len());
    assert_eq!(lib.query(json!({ "text": "งานค้นหา" }))["total"], names.len(), "ค้นจากชื่อกิจกรรมได้");
    assert_eq!(lib.query(json!({ "text": "ไม่มีคำนี้" }))["total"], 0);

    // ชื่อบุคคล ชื่อเล่น และชื่ออื่น
    let person = lib.label("ภูริ.jpg", 0.1, ("name", "พระมหาอนวัช"))["createdPerson"].clone();
    let detail = lib.call("get_person", json!({ "personId": person["id"] }));
    lib.call(
        "save_person",
        json!({ "requestId": new_id(), "personId": person["id"], "expectedVersion": detail["recordVersion"],
                "displayName": "พระมหาอนวัช", "nickname": "หลวงพี่วัช", "aliases": ["Anawat", "ท่านวัช"] }),
    );
    for q in ["มหาอน", "หลวงพี่วัช", "anawat", "ท่านวัช"] {
        assert_eq!(lib.filenames(json!({ "text": q })), ["ภูริ.jpg"], "ค้นด้วย {q}");
        assert_eq!(arr(&lib.call("list_people", json!({ "text": q }))).len(), 1, "รายชื่อค้นด้วย {q}");
    }
    let s = lib.call("search_suggestions", json!({ "text": "ภู" }));
    assert_eq!((arr(&s["people"]).len(), arr(&s["events"]).len(), arr(&s["files"]).len()), (0, 0, 1));
    let s = lib.call("search_suggestions", json!({ "text": "อนวัช" }));
    assert_eq!(s["people"][0]["displayName"], "พระมหาอนวัช");
}

#[test]
fn t34_original_files_are_never_modified_by_any_workflow() {
    let lib = Lib::new();
    let root = year_folder(&lib);
    std::fs::write(root.join("691116 ต้อนรับคณะญาติโยม/rotated.jpg"), jpeg_with_exif(90, 60, 8, "2026:11:16 07:00:00"))
        .unwrap();
    std::fs::write(root.join("691116 ต้อนรับคณะญาติโยม/broken.jpg"), b"junk").unwrap();
    let before = tree_state(&root);
    let dirs_before: Vec<_> = walkdir::WalkDir::new(&root).into_iter().map(|e| e.unwrap().into_path()).collect();

    let result = lib.import(&root, "child_events", |_| true);
    let event_id = result["eventIds"][0].clone();
    let detail = lib.call("get_event", json!({ "eventId": event_id.clone() }));
    lib.call("update_event", json!({ "eventId": event_id.clone(), "expectedVersion": detail["recordVersion"], "title": "ชื่อใหม่ในแอป", "eventDate": "2026-01-02", "notes": "หมายเหตุ" }));
    lib.label("IMG_0001.jpg", 0.1, ("name", "คุณกานต์"));
    lib.label("rotated.jpg", 0.1, ("name", "คุณเมย์"));
    for item in arr(&lib.query(json!({}))["items"]) {
        let id = item["id"].as_str().unwrap();
        let _ = puri_core::api::asset(&lib.app, &format!("thumb/{id}"));
        let _ = puri_core::api::asset(&lib.app, &format!("photo/{id}"));
        lib.call("get_photo_details", json!({ "photoId": id }));
        lib.call("reveal_photo", json!({ "photoId": id }));
    }
    for face in 0..lib.count("SELECT COUNT(*) FROM faces") {
        let conn = rusqlite::Connection::open(lib.tmp.path().join("data/library.sqlite")).unwrap();
        let id: String =
            conn.query_row("SELECT id FROM faces ORDER BY id LIMIT 1 OFFSET ?1", [face], |r| r.get(0)).unwrap();
        puri_core::api::asset(&lib.app, &format!("face/{id}")).unwrap();
    }
    lib.call("rescan_event", json!({ "eventId": event_id }));
    lib.run_jobs();
    let dest = lib.tmp.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    let ids: Vec<Value> = arr(&lib.query(json!({}))["items"]).iter().map(|i| i["id"].clone()).collect();
    run_export(&lib, json!({ "kind": "explicit", "photoIds": ids }), &dest);
    let lib = lib.reopen();
    lib.run_jobs();

    assert_eq!(tree_state(&root), before, "ไม่มี byte หรือเวลาแก้ไขของต้นฉบับเปลี่ยน");
    let dirs_after: Vec<_> = walkdir::WalkDir::new(&root).into_iter().map(|e| e.unwrap().into_path()).collect();
    assert_eq!(dirs_after, dirs_before, "ไม่มีไฟล์หรือโฟลเดอร์ถูกเพิ่ม ลบ หรือเปลี่ยนชื่อในโฟลเดอร์ต้นทาง");
    // ชื่อกิจกรรมที่แก้ในแอปไม่เปลี่ยนชื่อโฟลเดอร์จริง
    assert!(arr(&lib.call("list_events", json!({ "text": "ชื่อใหม่ในแอป" }))).len() == 1);
}

#[test]
fn t36_t37_pins_recents_and_coappearance_survive_restart() {
    let lib = Lib::new();
    let (a, b) = people_fixture(&lib);
    let c = lib.call("save_person", json!({ "requestId": new_id(), "displayName": "คนไม่มีรูป" }))["id"]
        .as_str()
        .unwrap()
        .to_string();

    // ยังไม่มี pin: ค่าเริ่มต้นคือเปิดล่าสุด และไม่มีคนตัวอย่าง
    let home = lib.call("list_home_people", json!({}));
    assert_eq!(home["tab"], "recent");
    assert_eq!((arr(&home["pinned"]).len(), arr(&home["recent"]).len()), (0, 0));
    // อ่านข้อมูล/ดูเร็วโดยไม่ได้เรียก record_person_view ไม่แก้ "เปิดล่าสุด"
    lib.call("get_person", json!({ "personId": a.clone() }));
    lib.call("get_person_quick_view", json!({ "personId": a.clone() }));
    assert_eq!(arr(&lib.call("list_home_people", json!({}))["recent"]).len(), 0);

    lib.call("set_person_pinned", json!({ "personId": b.clone(), "pinned": true }));
    lib.call("set_person_pinned", json!({ "personId": a.clone(), "pinned": true }));
    lib.call("set_person_pinned", json!({ "personId": a.clone(), "pinned": true })); // กดซ้ำไม่ซ้ำแถว
    lib.call("record_person_view", json!({ "personId": c.clone() }));
    lib.call("record_person_view", json!({ "personId": a.clone() }));
    lib.call("record_person_view", json!({ "personId": c.clone() }));
    let counts_before = lib.call("get_person", json!({ "personId": a.clone() }))["confirmedPhotoCount"].clone();

    let lib = lib.reopen();
    let home = lib.call("list_home_people", json!({}));
    assert_eq!(home["tab"], "pinned", "มี pin แล้วและยังไม่เคยเลือกแท็บ → ปักหมุด");
    let ids = |v: &Value| arr(v).iter().map(|p| p["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(ids(&home["pinned"]), [b.clone(), a.clone()], "เรียงตามเวลาที่ปักหมุด");
    assert_eq!(ids(&home["recent"]), [c.clone(), a.clone()], "เปิดล่าสุดอยู่หน้า ไม่ซ้ำคน");
    assert_eq!(
        lib.call("get_person", json!({ "personId": a.clone() }))["confirmedPhotoCount"],
        counts_before,
        "pin ไม่เพิ่มจำนวนรูป"
    );
    lib.call("set_home_people_tab", json!({ "tab": "recent" }));
    let lib = lib.reopen();
    assert_eq!(lib.call("list_home_people", json!({}))["tab"], "recent", "จำแท็บที่ผู้ใช้เลือก");

    // เก็บบุคคล: หายจากแถวปกติแต่ pin ยังอยู่เพื่อคืนกลับ
    lib.call("set_person_archived", json!({ "personId": b.clone(), "archived": true }));
    assert_eq!(ids(&lib.call("list_home_people", json!({}))["pinned"]), std::slice::from_ref(&a));
    assert_eq!(arr(&lib.call("list_people", json!({}))).len(), 2);
    assert_eq!(arr(&lib.call("list_people", json!({ "includeArchived": true }))).len(), 3);
    // ปรากฏร่วมในภาพไม่เสนอคนที่เก็บไว้
    assert_eq!(arr(&lib.call("get_person_quick_view", json!({ "personId": a.clone() }))["coappearances"]).len(), 0);
    lib.call("set_person_archived", json!({ "personId": b.clone(), "archived": false }));
    assert_eq!(ids(&lib.call("list_home_people", json!({}))["pinned"]), [b.clone(), a.clone()]);

    // T37: ปรากฏร่วมในภาพ — นับรูปไม่ซ้ำจากคำยืนยัน ไม่รวมตัวเอง และจำนวนตรงกับผล AND
    let quick = lib.call("get_person_quick_view", json!({ "personId": a.clone() }));
    assert_eq!(quick["person"]["confirmedPhotoCount"], 2);
    assert_eq!(quick["confirmedEventCount"], 1);
    assert_eq!(quick["hasConfirmedFaces"], true);
    assert_eq!(quick["scope"], "all_events");
    assert_eq!(arr(&quick["recentPhotos"]).len(), 2);
    let co = arr(&quick["coappearances"]);
    assert_eq!(co.len(), 1);
    assert_eq!(co[0]["person"]["id"], b.as_str());
    let shared = co[0]["sharedPhotoCount"].as_i64().unwrap();
    let and = lib.query(json!({ "personIds": [a.clone(), b.clone()], "personMode": "all" }));
    assert_eq!(and["total"], shared, "กดคนร่วม → ผล AND ต้องเท่ากับจำนวนที่แสดง");
    // เพิ่มหน้าที่สองของ B ในรูปเดียวกัน: จำนวนร่วมไม่เพิ่ม
    lib.label("both.jpg", 0.3, ("id", &b));
    assert_eq!(
        lib.call("get_person_quick_view", json!({ "personId": a }))["coappearances"][0]["sharedPhotoCount"],
        shared
    );
    let empty = lib.call("get_person_quick_view", json!({ "personId": c }));
    assert_eq!(empty["hasConfirmedFaces"], false, "ระเบียนคนเปล่าไม่ได้ป้ายยืนยัน");
}

#[test]
fn library_open_rules() {
    // ไฟล์ SQLite อื่น (เช่นฐานของรุ่น Python) ต้องไม่ถูกเปิดเป็นคลังและไม่ถูกแก้
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("data")).unwrap();
    let foreign = tmp.path().join("data/library.sqlite");
    {
        let conn = rusqlite::Connection::open(&foreign).unwrap();
        conn.execute_batch(
            "CREATE TABLE persons (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO persons(name) VALUES ('x');",
        )
        .unwrap();
    }
    let bytes_before = std::fs::read(&foreign).unwrap();
    let open = |dir: &Path| {
        puri_core::App::open(puri_core::Options {
            data_dir: dir.join("data"),
            cache_dir: dir.join("cache"),
            sink: std::sync::Arc::new(puri_core::NoopSink),
            folder_picker: "native",
            start_worker: false,
            face_engine: None,
            face_engine_unavailable: "",
        })
    };
    assert_eq!(open(tmp.path()).err().unwrap().code, "DATABASE_CORRUPT");
    assert_eq!(std::fs::read(&foreign).unwrap(), bytes_before, "ไฟล์ที่ไม่ใช่คลังของเราไม่ถูกแตะ");

    // คลังที่สร้างด้วยแอปรุ่นใหม่กว่า: ปฏิเสธ ไม่ downgrade
    let lib = Lib::new();
    let dir = lib.tmp.path().to_path_buf();
    // คลังเดียวกันเปิดซ้อนไม่ได้
    assert_eq!(open(&dir).err().unwrap().code, "LIBRARY_LOCKED");
    lib.app.shutdown();
    let Lib { tmp: keep, app, .. } = lib;
    drop(app);
    rusqlite::Connection::open(dir.join("data/library.sqlite"))
        .unwrap()
        .pragma_update(None, "user_version", 999)
        .unwrap();
    assert_eq!(open(&dir).err().unwrap().code, "LIBRARY_TOO_NEW");
    drop(keep);
}

#[test]
fn ipc_contract_matches_typescript_command_table() {
    let ts = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/lib/ipc/types.ts")).unwrap();
    let table = ts.split("export interface Commands {").nth(1).unwrap().split("\n}\n").next().unwrap();
    let mut ts_names: Vec<&str> = table
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("/**") && !l.starts_with('*') && !l.starts_with("//"))
        .filter_map(|l| l.split_once(": [").map(|(name, _)| name))
        .collect();
    let mut rust_names = puri_core::api::COMMAND_NAMES.to_vec();
    ts_names.sort();
    rust_names.sort();
    assert_eq!(ts_names, rust_names, "ตารางคำสั่งใน types.ts ต้องตรงกับ COMMAND_NAMES");

    // ทุกชื่อใน COMMAND_NAMES ต้องรู้จักจริง (ผิดที่ argument ได้ แต่ต้องไม่ใช่ "ไม่รู้จักคำสั่ง")
    let lib = Lib::new();
    for name in puri_core::api::COMMAND_NAMES {
        if let Err(e) = lib.try_call(name, json!({})) {
            assert!(!e.message.contains("unknown variant"), "{name}: {e}");
        }
    }
    let unknown = lib.try_call("run_sql", json!({ "sql": "DROP TABLE photos" })).unwrap_err();
    assert_eq!(unknown.code, "INVALID_INPUT", "ไม่มีคำสั่ง SQL/shell อิสระ");
    let caps = lib.call("get_capabilities", json!({}));
    assert_eq!(caps["imageFormats"], json!(["jpg", "jpeg", "png"]));
    assert_eq!(caps["faceAnalysis"]["available"], false, "build ที่ไม่มีตัววิเคราะห์ต้องบอกตามจริง");
    assert_eq!(caps["faceAnalysis"]["state"], "unsupported");
    // asset รับเฉพาะ ID ทึบ: path traversal ไม่ถูกตีความเป็นตำแหน่งไฟล์
    assert!(puri_core::api::asset(&lib.app, "photo/../../etc/passwd").is_err());
    assert!(puri_core::api::asset(&lib.app, "photo//etc/passwd").is_err());
}
