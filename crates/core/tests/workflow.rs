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
    let resized = puri_core::media::export_jpeg(&original, puri_core::media::Fit::LongEdge(1600), None, None).unwrap();
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
fn edit_crop_and_rotate_keep_faces_names_and_the_original_file() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 แต่งภาพ");
    std::fs::create_dir_all(&root).unwrap();
    // ภาพ 400×300 (EXIF ไม่หมุน) วันถ่ายอยู่ใน EXIF
    std::fs::write(root.join("group.jpg"), jpeg_with_exif(400, 300, 1, "2026:11:21 09:00:00")).unwrap();
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let id = lib.photo_id("group.jpg");
    // ใบหน้าซ้ายและขวา พร้อมชื่อ
    let left = lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id, "bbox": { "x": 0.1, "y": 0.2, "w": 0.1, "h": 0.2 }, "newPersonName": "คุณซ้าย" }),
    )["faces"][0]["id"]
        .clone();
    lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id, "bbox": { "x": 0.7, "y": 0.2, "w": 0.1, "h": 0.2 }, "newPersonName": "คุณขวา" }),
    );
    let face_of = |details: &Value, name: &str| {
        arr(&details["faces"]).iter().find(|f| f["person"]["displayName"] == name).unwrap().clone()
    };

    // ค่าที่ผิดถูกปฏิเสธ
    for bad in [
        json!({ "rotate": 45 }),
        json!({ "straighten": 60.0 }),
        json!({ "crop": { "x": 0.5, "y": 0.0, "w": 0.8, "h": 1.0 } }),
    ] {
        assert_eq!(
            lib.try_call("save_photo_edit", json!({ "photoId": id, "settings": bad })).unwrap_err().code,
            "INVALID_INPUT"
        );
    }

    // หมุนตามเข็ม 90° แล้วตัดครึ่งล่างทิ้ง: ภาพที่แต่งเป็น 300×200 · ใบหน้าซ้ายเดิมย้ายไปอยู่ขวาบน
    let settings = json!({ "rotate": 90, "straighten": 0.0, "crop": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 0.5 } });
    let details = lib.call("save_photo_edit", json!({ "photoId": id, "settings": settings }));
    assert_eq!((details["edit"]["width"].as_i64(), details["edit"]["height"].as_i64()), (Some(300), Some(200)));
    assert!(details["editId"].is_string());
    let l = face_of(&details, "คุณซ้าย");
    assert_eq!(l["id"], left, "ใบหน้าเดิม ไม่ใช่ใบใหม่");
    assert_eq!(l["outsideCrop"], false);
    let close = |v: &Value, want: f64| (v.as_f64().unwrap() - want).abs() < 1e-6;
    // (0.1, 0.2)–(0.2, 0.4) บนภาพเดิม → หมุน 90°: x = 1−0.4 … 1−0.2, y = 0.1 … 0.2 → ตัดครึ่งบน: y × 2
    assert!(
        close(&l["bbox"]["x"], 0.6)
            && close(&l["bbox"]["w"], 0.2)
            && close(&l["bbox"]["y"], 0.2)
            && close(&l["bbox"]["h"], 0.2),
        "{l}"
    );
    assert_eq!(face_of(&details, "คุณขวา")["outsideCrop"], true, "ใบหน้าขวาอยู่ครึ่งที่ตัดทิ้ง");
    assert_eq!(lib.filenames(json!({ "text": "คุณขวา" })), ["group.jpg"], "ไม่แตะไฟล์: ชื่อและการค้นหายังอยู่ครบ");

    // ภาพที่แสดงเป็นภาพที่แต่ง · ต้นฉบับยังเปิดได้สำหรับหน้าแต่งภาพ
    let dims = |kind: &str| {
        let a = puri_core::api::asset(&lib.app, &format!("{kind}/{id}")).unwrap();
        let img = image::load_from_memory(&a.bytes).unwrap();
        (img.width(), img.height())
    };
    assert_eq!(dims("photo"), (300, 200));
    assert_eq!(dims("thumb"), (300, 200));
    assert_eq!(dims("original"), (400, 300));

    // วาดกรอบบนภาพที่แต่ง: เก็บเป็นพิกัดบนภาพเดิม และแสดงกลับตรงที่วาด
    let drawn = lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id, "bbox": { "x": 0.1, "y": 0.5, "w": 0.2, "h": 0.3 }, "newPersonName": "คุณใหม่" }),
    );
    let shown = arr(&drawn["faces"]).iter().find(|f| f["person"]["displayName"] == "คุณใหม่").unwrap().clone();
    assert!(
        close(&shown["bbox"]["x"], 0.1) && close(&shown["bbox"]["y"], 0.5) && close(&shown["bbox"]["w"], 0.2),
        "{shown}"
    );
    let conn = rusqlite::Connection::open(lib.tmp.path().join("data/library.sqlite")).unwrap();
    let stored: (f64, f64) = conn
        .query_row("SELECT bbox_x, bbox_y FROM faces WHERE id = ?1", [shown["id"].as_str().unwrap()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    // บนภาพที่แต่ง (0.1, 0.5) = บนภาพหลังหมุน (0.1, 0.25)–(0.3, 0.4) → ภาพเดิม x = 0.25, y = 1−0.3 = 0.7
    assert!((stored.0 - 0.25).abs() < 1e-6 && (stored.1 - 0.7).abs() < 1e-6, "{stored:?}");

    // ส่งออก: ได้ภาพที่แต่งเต็มความละเอียด พร้อมวันถ่าย
    let dest = lib.tmp.path().join("ส่งออกที่แต่ง");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let out = std::fs::read(dest.join("group.jpg")).unwrap();
    let img = image::load_from_memory(&out).unwrap();
    assert_eq!((img.width(), img.height()), (300, 200));
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
    assert!(exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).is_some());

    // กลับเป็นต้นฉบับ: กรอบเดิม ภาพเดิม · ไฟล์ต้นฉบับไม่เคยถูกแตะ
    let details = lib.call("reset_photo_edit", json!({ "photoId": id }));
    assert!(details["edit"].is_null() && details["editId"].is_null());
    let l = face_of(&details, "คุณซ้าย");
    assert!(close(&l["bbox"]["x"], 0.1) && close(&l["bbox"]["y"], 0.2));
    assert_eq!(dims("photo"), (400, 300));
    assert_eq!(tree_state(&root), before, "แต่งภาพไม่แตะไฟล์ต้นฉบับ");
}

#[test]
fn edit_light_and_color_change_views_and_exports_but_not_faces_or_the_file() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 แสงสี");
    std::fs::create_dir_all(&root).unwrap();
    // ภาพเทากลาง 300×200
    let grey = image::RgbImage::from_pixel(300, 200, image::Rgb([90, 90, 90]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95).encode_image(&grey).unwrap();
    std::fs::write(root.join("grey.jpg"), &bytes).unwrap();
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let id = lib.photo_id("grey.jpg");
    let face = lib.call(
        "create_manual_face",
        json!({ "requestId": new_id(), "photoId": id, "bbox": { "x": 0.4, "y": 0.3, "w": 0.2, "h": 0.3 }, "newPersonName": "คุณเทา" }),
    )["faces"][0]
        .clone();
    let center = |kind: &str| {
        let a = puri_core::api::asset(&lib.app, &format!("{kind}/{id}")).unwrap();
        image::load_from_memory(&a.bytes).unwrap().to_rgb8().get_pixel(150, 100).0
    };
    assert!(center("photo")[0].abs_diff(90) <= 3);

    // ค่าเกินช่วงถูกปฏิเสธ
    let bad = json!({ "adjust": { "exposure": 9.0 } });
    assert_eq!(
        lib.try_call("save_photo_edit", json!({ "photoId": id, "settings": bad })).unwrap_err().code,
        "INVALID_INPUT"
    );

    // ค่าแสง +1 EV และอุ่นขึ้น: ภาพที่แสดงและภาพย่อสว่างขึ้นและแดงกว่าน้ำเงิน · ขนาดภาพเท่าเดิม · กรอบใบหน้าไม่ขยับ
    let settings = json!({ "adjust": { "exposure": 1.0, "temperature": 40.0 } });
    let details = lib.call("save_photo_edit", json!({ "photoId": id, "settings": settings }));
    assert_eq!((details["edit"]["width"].as_i64(), details["edit"]["height"].as_i64()), (Some(300), Some(200)));
    assert_eq!(details["faces"][0]["bbox"], face["bbox"]);
    let shown = center("photo");
    assert!(shown[1] > 110 && shown[0] > shown[2], "{shown:?}");
    let thumb = center("thumb");
    assert!(thumb[1] > 110, "{thumb:?}");
    assert!(center("original")[0].abs_diff(90) <= 3, "ต้นฉบับยังเป็นภาพเดิม");

    // สำเนาที่ส่งออกได้แสงสีชุดเดียวกัน
    let dest = lib.tmp.path().join("ส่งออกแสงสี");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [id] }, "destination": dest }));
    lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let out = image::load_from_memory(&std::fs::read(dest.join("grey.jpg")).unwrap()).unwrap().to_rgb8();
    let px = out.get_pixel(150, 100).0;
    assert!(px.iter().zip(shown).all(|(a, b)| a.abs_diff(b) <= 4), "ส่งออก {px:?} ตรงกับที่แสดง {shown:?}");
    assert_eq!(lib.filenames(json!({ "text": "คุณเทา" })), ["grey.jpg"]);
    assert_eq!(tree_state(&root), before, "แต่งแสงสีไม่แตะไฟล์ต้นฉบับ");
}

#[test]
fn edit_paste_to_many_photos_replaces_only_the_chosen_parts() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 วางการแต่ง");
    std::fs::create_dir_all(&root).unwrap();
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        std::fs::write(root.join(name), jpeg_with_exif(400, 300, 1, "2026:11:21 10:00:00")).unwrap();
    }
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let (a, b, c) = (lib.photo_id("a.jpg"), lib.photo_id("b.jpg"), lib.photo_id("c.jpg"));
    let crop_a = json!({ "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 });
    let crop_b = json!({ "x": 0.3, "y": 0.2, "w": 0.6, "h": 0.6 });
    let source = json!({ "rotate": 90, "crop": crop_a, "adjust": { "exposure": 1.0, "contrast": 20.0, "temperature": 30.0, "vibrance": 15.0 } });
    lib.call("save_photo_edit", json!({ "photoId": a, "settings": source }));
    lib.call(
        "save_photo_edit",
        json!({ "photoId": b, "settings": { "crop": crop_b, "adjust": { "saturation": -50.0 } } }),
    );
    let edit_of = |id: &str| lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"].clone();
    let copied = edit_of(&a);
    let both = json!({ "kind": "explicit", "photoIds": [b, c] });

    // ไม่เลือกส่วนไหนเลย → ปฏิเสธ
    let none = json!({ "geometry": false, "light": false, "color": false });
    let err =
        lib.try_call("apply_photo_edits", json!({ "selection": both, "settings": copied, "parts": none })).unwrap_err();
    assert_eq!(err.code, "INVALID_INPUT");

    // วางเฉพาะแสง: b เก็บกรอบตัดและความอิ่มสีของตัวเอง · c ได้แค่แสง
    let light = json!({ "geometry": false, "light": true, "color": false });
    let r = lib.call("apply_photo_edits", json!({ "selection": both, "settings": copied, "parts": light }));
    assert_eq!((r["updated"].as_i64(), r["unchanged"].as_i64()), (Some(2), Some(0)));
    let eb = edit_of(&b);
    assert_eq!(eb["crop"], crop_b);
    assert_eq!((eb["adjust"]["exposure"].as_f64(), eb["adjust"]["contrast"].as_f64()), (Some(1.0), Some(20.0)));
    assert_eq!((eb["adjust"]["saturation"].as_f64(), eb["adjust"]["temperature"].as_f64()), (Some(-50.0), Some(0.0)));
    let ec = edit_of(&c);
    assert_eq!((ec["rotate"].as_i64(), ec["crop"].is_null()), (Some(0), true));
    assert_eq!(ec["adjust"]["exposure"].as_f64(), Some(1.0));
    // วางซ้ำ = ไม่มีอะไรเปลี่ยน
    let r = lib.call("apply_photo_edits", json!({ "selection": both, "settings": copied, "parts": light }));
    assert_eq!((r["updated"].as_i64(), r["unchanged"].as_i64()), (Some(0), Some(2)));

    // วางทุกส่วน: ได้ตัด/หมุนและสีของ a ด้วย · ภาพย่อเป็นภาพที่แต่ง (หมุนแล้วเป็นแนวตั้ง)
    let all = json!({ "geometry": true, "light": true, "color": true });
    lib.call("apply_photo_edits", json!({ "selection": both, "settings": copied, "parts": all }));
    assert_eq!(edit_of(&b), copied);
    let thumb = puri_core::api::asset(&lib.app, &format!("thumb/{c}")).unwrap();
    let thumb = image::load_from_memory(&thumb.bytes).unwrap();
    assert!(thumb.height() > thumb.width());

    // วางค่าว่าง (เหมือนรีเซ็ต) ทุกส่วน → กลับเป็นภาพต้นฉบับ
    lib.call("apply_photo_edits", json!({ "selection": both, "settings": {}, "parts": all }));
    assert!(lib.call("get_photo_details", json!({ "photoId": c }))["edit"].is_null());
    assert_eq!(tree_state(&root), before, "วางการแต่งไม่แตะไฟล์ต้นฉบับ");
}

/// ภาพทดสอบ 400×300: ฉากเทา + จีวรสีส้มด้านซ้ายล่าง + "ใบหน้า" สีผิวรูปวงรีที่ (fx, fy) รัศมี 30×40
fn face_scene(fx: u32, fy: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(400, 300, |x, y| {
        let (dx, dy) = ((x as f64 - fx as f64) / 30.0, (y as f64 - fy as f64) / 40.0);
        if dx * dx + dy * dy <= 1.0 {
            image::Rgb([205, 150, 125])
        } else if (40..120).contains(&x) && y >= 150 {
            image::Rgb([235, 140, 20])
        } else {
            image::Rgb([120, 120, 120])
        }
    });
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95).encode_image(&img).unwrap();
    bytes
}

#[test]
fn edit_face_mask_brightens_only_skin_and_pastes_onto_each_photos_own_faces() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 ปรับเฉพาะส่วน");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.jpg"), face_scene(200, 100)).unwrap();
    std::fs::write(root.join("b.jpg"), face_scene(320, 180)).unwrap();
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let (a, b) = (lib.photo_id("a.jpg"), lib.photo_id("b.jpg"));
    let face_box =
        |fx: f64, fy: f64| json!({ "x": (fx - 30.0) / 400.0, "y": (fy - 40.0) / 300.0, "w": 0.15, "h": 80.0 / 300.0 });
    for (id, fx, fy, name) in [(&a, 200.0, 100.0, "คุณเอ"), (&b, 320.0, 180.0, "คุณบี")] {
        lib.call(
            "create_manual_face",
            json!({ "requestId": new_id(), "photoId": id, "bbox": face_box(fx, fy), "newPersonName": name }),
        );
    }
    let pixel = |kind: &str, id: &str, x: u32, y: u32| {
        let a = puri_core::api::asset(&lib.app, &format!("{kind}/{id}")).unwrap();
        image::load_from_memory(&a.bytes).unwrap().to_rgb8().get_pixel(x, y).0
    };
    let same = |p: [u8; 3], q: [u8; 3]| p.iter().zip(q).all(|(a, b)| a.abs_diff(b) <= 3);
    let face_a = json!([[0.425, 0.2, 0.15, 80.0 / 300.0]]);
    let mask = |amount: f64| json!({ "id": "m1", "area": { "kind": "faces", "boxes": face_a, "skin": true }, "adjust": { "exposure": 1.0 }, "amount": amount });

    // ความแรงเกินช่วงถูกปฏิเสธ
    let err =
        lib.try_call("save_photo_edit", json!({ "photoId": a, "settings": { "masks": [mask(150.0)] } })).unwrap_err();
    assert_eq!(err.code, "INVALID_INPUT");

    // หน้าสว่างขึ้น · จีวร ฉากหลังข้างหน้า (อยู่ในวงรีแต่ไม่ใช่สีผิว) และฉากไกล ๆ เท่าเดิม
    let (face, beside, robe, far) = ((200, 100), (233, 100), (80, 250), (370, 30));
    let original = |id: &str, (x, y): (u32, u32)| pixel("original", id, x, y);
    lib.call("save_photo_edit", json!({ "photoId": a, "settings": { "masks": [mask(100.0)] } }));
    let shown = pixel("photo", &a, face.0, face.1);
    assert!(shown[1] > original(&a, face)[1] + 30, "หน้าสว่างขึ้น {shown:?}");
    for spot in [beside, robe, far] {
        assert!(same(pixel("photo", &a, spot.0, spot.1), original(&a, spot)), "ไม่ใช่ผิวต้องเท่าเดิมที่ {spot:?}");
    }
    let settings = lib.call("get_photo_details", json!({ "photoId": a }))["edit"]["settings"].clone();
    assert_eq!(settings["masks"][0]["area"]["boxes"], face_a);

    // สำเนาที่ส่งออกได้หน้าที่สว่างแบบเดียวกัน
    let dest = lib.tmp.path().join("ส่งออกปรับเฉพาะส่วน");
    std::fs::create_dir_all(&dest).unwrap();
    let plan = lib
        .call("preview_export", json!({ "selection": { "kind": "explicit", "photoIds": [a] }, "destination": dest }));
    lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let out = image::load_from_memory(&std::fs::read(dest.join("a.jpg")).unwrap()).unwrap().to_rgb8();
    let px = out.get_pixel(face.0, face.1).0;
    assert!(px.iter().zip(shown).all(|(p, s)| p.abs_diff(s) <= 4), "ส่งออก {px:?} ตรงกับที่แสดง {shown:?}");

    // วางเฉพาะแสง: ไม่แตะการปรับเฉพาะส่วนของรูปปลายทาง (b ยังไม่มี)
    let only = |light: bool, masks: bool| json!({ "geometry": false, "light": light, "color": false, "masks": masks });
    let to_b = json!({ "kind": "explicit", "photoIds": [b] });
    lib.call("apply_photo_edits", json!({ "selection": to_b, "settings": settings, "parts": only(true, false) }));
    assert!(lib.call("get_photo_details", json!({ "photoId": b }))["edit"].is_null(), "แสงของ a เป็นค่าเริ่มต้น");

    // วางการปรับเฉพาะส่วน: b ได้ Mask ที่ครอบใบหน้าของ b เอง (ไม่ใช่ตำแหน่งหน้าของ a)
    let r =
        lib.call("apply_photo_edits", json!({ "selection": to_b, "settings": settings, "parts": only(false, true) }));
    assert_eq!(r["updated"].as_i64(), Some(1));
    let eb = lib.call("get_photo_details", json!({ "photoId": b }))["edit"]["settings"].clone();
    let boxes = eb["masks"][0]["area"]["boxes"].as_array().unwrap().clone();
    assert_eq!(boxes.len(), 1);
    assert!((boxes[0][0].as_f64().unwrap() - 290.0 / 400.0).abs() < 1e-6, "{boxes:?}");
    assert!(pixel("photo", &b, 320, 180)[1] > original(&b, (320, 180))[1] + 30, "หน้าของ b สว่างขึ้น");
    assert!(same(pixel("photo", &b, face.0, face.1), original(&b, face)), "ตำแหน่งหน้าของ a บนรูป b ไม่โดน");
    // วางซ้ำ = ไม่มีอะไรเปลี่ยน
    let r =
        lib.call("apply_photo_edits", json!({ "selection": to_b, "settings": settings, "parts": only(false, true) }));
    assert_eq!((r["updated"].as_i64(), r["unchanged"].as_i64()), (Some(0), Some(1)));
    assert_eq!(tree_state(&root), before, "ปรับเฉพาะส่วนไม่แตะไฟล์ต้นฉบับ");
}

/// preset .xmp แบบที่ Lightroom Classic ส่งออก (ย่อ) — สมดุลแสงขาวแบบเพิ่ม/ลด HSL เส้นโค้ง และส่วนที่เรายังทำไม่ได้
const LIGHTROOM_XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 7.0-c000">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:Version="17.0" crs:ProcessVersion="15.4" crs:WhiteBalance="Custom"
   crs:IncrementalTemperature="+12" crs:IncrementalTint="-6"
   crs:Exposure2012="+0.40" crs:Contrast2012="+10" crs:Shadows2012="+25"
   crs:Texture="+15" crs:Vibrance="+20"
   crs:LuminanceAdjustmentOrange="+30" crs:SaturationAdjustmentBlue="-25">
   <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">PuriBright เทสต์</rdf:li></rdf:Alt></crs:Name>
   <crs:Look><rdf:Description crs:Name="Adobe Vivid" crs:Amount="1"/></crs:Look>
   <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 12</rdf:li><rdf:li>128, 136</rdf:li><rdf:li>255, 250</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

#[test]
fn edit_presets_save_import_rename_delete_and_survive_restart() {
    let lib = Lib::new();
    assert_eq!(lib.call("list_edit_presets", json!({})), json!([]));

    // บันทึก: ต้องมีชื่อ และต้องปรับอะไรสักอย่าง
    let adjust = json!({ "exposure": 0.5, "lum": [0.0, 40.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], "parametric": [0.0, 10.0, 0.0, 0.0] });
    let err = lib.try_call("save_edit_preset", json!({ "name": "  ", "adjust": adjust })).unwrap_err();
    assert_eq!(err.code, "INVALID_INPUT");
    let err = lib.try_call("save_edit_preset", json!({ "name": "เปล่า", "adjust": {} })).unwrap_err();
    assert_eq!(err.code, "INVALID_INPUT");
    let mine = lib.call("save_edit_preset", json!({ "name": " ในอาคาร ไฟเหลือง ", "adjust": adjust }));
    assert_eq!((mine["name"].as_str(), mine["source"].as_str()), (Some("ในอาคาร ไฟเหลือง"), Some("user")));
    assert_eq!(mine["adjust"]["lum"][1].as_f64(), Some(40.0));

    // นำเข้า .xmp: ได้ค่าที่ทำได้ · ที่ทำไม่ได้แสดงตามจริง · ไฟล์ต้นทางไม่ถูกแตะ
    let dir = lib.photos().join("presets");
    std::fs::create_dir_all(&dir).unwrap();
    let xmp = dir.join("PuriBright.xmp");
    std::fs::write(&xmp, LIGHTROOM_XMP).unwrap();
    let imported = lib.call("import_lightroom_preset", json!({ "path": xmp.to_string_lossy() }));
    let a = &imported["adjust"];
    assert_eq!((imported["name"].as_str(), imported["source"].as_str()), (Some("PuriBright เทสต์"), Some("lightroom")));
    assert_eq!(
        (a["exposure"].as_f64(), a["shadows"].as_f64(), a["temperature"].as_f64()),
        (Some(0.4), Some(25.0), Some(12.0))
    );
    assert_eq!((a["lum"][1].as_f64(), a["sat"][5].as_f64()), (Some(30.0), Some(-25.0)));
    assert_eq!(a["curve"], json!([[0.0, 12.0], [128.0, 136.0], [255.0, 250.0]]));
    assert_eq!(imported["unsupported"], json!(["Profile “Adobe Vivid”"]), "Texture ทำตามได้แล้ว (ข้อ 27)");
    assert_eq!(a["texture"].as_f64(), Some(15.0));
    assert_eq!(std::fs::read_to_string(&xmp).unwrap(), LIGHTROOM_XMP);
    // ไฟล์ที่ไม่ใช่ preset หรือไม่มีอยู่ → ปฏิเสธพร้อมเหตุผล
    let photo = dir.join("not-a-preset.jpg");
    std::fs::write(&photo, jpeg_with_exif(64, 48, 1, "2026:11:21 10:00:00")).unwrap();
    let err = lib.try_call("import_lightroom_preset", json!({ "path": photo.to_string_lossy() })).unwrap_err();
    assert_eq!(err.code, "UNSUPPORTED_FORMAT");
    let err =
        lib.try_call("import_lightroom_preset", json!({ "path": dir.join("gone.xmp").to_string_lossy() })).unwrap_err();
    assert_eq!(err.code, "SOURCE_UNAVAILABLE");

    // ใช้ preset กับรูป แล้วลบ preset: รูปยังเก็บค่าที่แต่งไว้
    let root = lib.photos().join("2026-11-21 preset");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.jpg"), jpeg_with_exif(400, 300, 1, "2026:11:21 10:00:00")).unwrap();
    std::fs::write(root.join("b.jpg"), jpeg_with_exif(400, 300, 1, "2026:11:21 10:01:00")).unwrap();
    lib.import(&root, "single_event", |_| true);
    let photo_id = lib.photo_id("a.jpg");
    lib.call("save_photo_edit", json!({ "photoId": photo_id, "settings": { "adjust": imported["adjust"] } }));
    // ใช้ preset กับหลายรูปจากแถบเลือกรูป (แสง + สี): กรอบตัดของแต่ละรูปคงเดิม ได้ HSL และเส้นโค้งครบ
    let b = lib.photo_id("b.jpg");
    let crop = json!({ "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 });
    lib.call(
        "save_photo_edit",
        json!({ "photoId": b, "settings": { "crop": crop, "adjust": { "saturation": -40.0 } } }),
    );
    let r = lib.call(
        "apply_photo_edits",
        json!({ "selection": { "kind": "explicit", "photoIds": [b] }, "settings": { "adjust": mine["adjust"] }, "parts": { "geometry": false, "light": true, "color": true } }),
    );
    assert_eq!(r["updated"].as_i64(), Some(1));
    let eb = lib.call("get_photo_details", json!({ "photoId": b }))["edit"]["settings"].clone();
    assert_eq!(eb["crop"], crop);
    assert_eq!(eb["adjust"], mine["adjust"], "แสงและสีเท่ากับ preset ทุกค่า รวม HSL และเส้นโค้ง");

    // ลำดับที่แสดงเรียงแบบพจนานุกรมไทยที่ UI · ที่นี่ตรวจแค่ว่ามีชื่อไหนบ้าง
    let names = |list: &Value| {
        let mut v =
            list.as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        v.sort();
        v
    };
    let list = lib.call("rename_edit_preset", json!({ "id": imported["id"], "name": "สว่างใส" }));
    assert_eq!(names(&list), ["สว่างใส", "ในอาคาร ไฟเหลือง"]);
    let err = lib.try_call("rename_edit_preset", json!({ "id": "missing", "name": "x" })).unwrap_err();
    assert_eq!(err.code, "NOT_FOUND");
    let list = lib.call("delete_edit_preset", json!({ "id": imported["id"] }));
    assert_eq!(names(&list), ["ในอาคาร ไฟเหลือง"]);
    let edit = lib.call("get_photo_details", json!({ "photoId": photo_id }))["edit"]["settings"].clone();
    assert_eq!(edit["adjust"]["lum"][1].as_f64(), Some(30.0), "ลบ preset แล้วรูปที่ใช้ไปไม่เปลี่ยน");

    // ปิดแล้วเปิดคลังใหม่: preset ยังอยู่ครบ
    let lib = lib.reopen();
    let list = lib.call("list_edit_presets", json!({}));
    assert_eq!(names(&list), ["ในอาคาร ไฟเหลือง"]);
    assert_eq!(list[0]["adjust"]["parametric"][1].as_f64(), Some(10.0));
}

#[test]
fn ai_tools_find_view_edit_apply_presets_and_undo_without_touching_files() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 งานบุญ AI");
    std::fs::create_dir_all(&root).unwrap();
    for (name, t) in [("a.jpg", "10:00"), ("b.jpg", "10:01"), ("c.jpg", "10:02")] {
        std::fs::write(root.join(name), jpeg_with_exif(400, 300, 1, &format!("2026:11:21 {t}:00"))).unwrap();
    }
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let (a, b, c) = (lib.photo_id("a.jpg"), lib.photo_id("b.jpg"), lib.photo_id("c.jpg"));
    let crop = json!({ "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 });
    lib.call("save_photo_edit", json!({ "photoId": c, "settings": { "crop": crop } }));
    let (tools, turn) = puri_core::assistant::test_tools(&lib.app).unwrap();
    let text = |out: &puri_core::assistant::mcp::ToolOutput| {
        out.content.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")
    };

    // ค้นรูปตามกิจกรรม/ชื่อไฟล์ และรายการกิจกรรม
    let found = tools.call("search_photos", json!({ "event": "งานบุญ" }));
    assert!(!found.is_error && text(&found).starts_with("3 photos match"), "{}", text(&found));
    assert!(text(&tools.call("search_photos", json!({ "text": "b.jpg" }))).contains(&b));
    assert!(text(&tools.call("list_events", json!({}))).contains("งานบุญ AI"));
    assert!(text(&tools.call("search_photos", json!({ "people": ["ไม่มีคนนี้"] }))).starts_with("No person named"));

    // ดูรูป: ได้ภาพ JPEG จริง (base64) ขนาดไม่เกินที่ขอ
    let seen = tools.call("view_photo", json!({ "photo_id": a, "size": 256 }));
    assert_eq!(seen.content[0]["type"], "image");
    assert_eq!(seen.content[0]["mimeType"], "image/jpeg");
    assert!(seen.content[0]["data"].as_str().unwrap().starts_with("/9j/"), "JPEG เริ่มด้วย FF D8 FF");
    assert!(tools.call("view_photo", json!({ "photo_id": "missing" })).is_error);

    // แต่งรูปเดียว (รวมค่าเข้ากับของเดิม) แล้วหลายรูป
    let out =
        tools.call("edit_photo", json!({ "photo_id": a, "adjust": { "exposure": 0.4, "levels": { "gamma": 1.2 } } }));
    assert!(!out.is_error, "{}", text(&out));
    let out = tools.call("edit_photo", json!({ "photo_id": a, "adjust": { "lum": [0, 30, 0, 0, 0, 0, 0, 0] } }));
    assert!(!out.is_error);
    let edit_of = |id: &str| lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"].clone();
    let ea = edit_of(&a);
    assert_eq!(
        (ea["adjust"]["exposure"].as_f64(), ea["adjust"]["levels"]["gamma"].as_f64(), ea["adjust"]["lum"][1].as_f64()),
        (Some(0.4), Some(1.2), Some(30.0))
    );
    assert_eq!(ea["adjust"]["levels"]["inWhite"].as_f64(), Some(255.0), "ค่าที่ไม่ได้ให้มาคงเดิม");
    let bad = tools.call("edit_photo", json!({ "photo_id": a, "adjust": { "exposure": 9 } }));
    assert!(bad.is_error, "ค่าเกินช่วงถูกปฏิเสธ");
    let out = tools.call("edit_photos", json!({ "photo_ids": [b, c], "adjust": { "temperature": 20 } }));
    assert_eq!(text(&out), "Edited 2 photos.");
    assert_eq!(edit_of(&c)["crop"], crop, "แต่งแสงสีไม่แตะกรอบตัดเดิม");

    // preset: บันทึกจากรูปที่แต่ง แล้วใช้กับรูปอื่น (แทนแสงสีทั้งชุด คงกรอบตัด)
    assert!(!tools.call("save_preset", json!({ "name": "AI สว่างใส", "photo_id": a })).is_error);
    assert!(text(&tools.call("list_presets", json!({}))).contains("AI สว่างใส"));
    let out = tools.call("apply_preset", json!({ "preset": "AI สว่างใส", "photo_ids": [c] }));
    assert_eq!(text(&out), "Applied “AI สว่างใส” to 1 photos.");
    let ec = edit_of(&c);
    assert_eq!(
        (ec["crop"].clone(), ec["adjust"]["temperature"].as_f64(), ec["adjust"]["exposure"].as_f64()),
        (crop.clone(), Some(0.0), Some(0.4))
    );

    // แสดงรูปในแชต และเหตุการณ์ของรอบ
    assert!(!tools.call("show_photos", json!({ "photo_ids": [a, b], "title": "ที่แต่ง" })).is_error);
    let events = lib.call("ai_events", json!({ "turnId": turn, "after": 0 }));
    let kinds: Vec<&str> = events["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(
        kinds.contains(&"tool")
            && kinds.contains(&"toolDone")
            && kinds.contains(&"changed")
            && kinds.contains(&"photos")
    );
    let photos = events["events"].as_array().unwrap().iter().find(|e| e["kind"] == "photos").unwrap().clone();
    assert_eq!(photos["photoIds"], json!([a, b]));

    // ย้อนทุกการแก้ของรอบ: a, b กลับเป็นไม่แต่ง · c กลับเป็นแค่กรอบตัดเดิม · preset ที่บันทึกยังอยู่ (ผู้ใช้ลบเองได้)
    assert_eq!(lib.call("ai_undo", json!({ "turnId": turn })), json!(3));
    assert!(lib.call("get_photo_details", json!({ "photoId": a }))["edit"].is_null());
    assert!(lib.call("get_photo_details", json!({ "photoId": b }))["edit"].is_null());
    assert_eq!(edit_of(&c), json!({ "rotate": 0, "straighten": 0.0, "crop": crop, "adjust": edit_of(&c)["adjust"] }));
    assert_eq!(edit_of(&c)["adjust"]["temperature"].as_f64(), Some(0.0));
    assert_eq!(edit_of(&c)["adjust"]["exposure"].as_f64(), Some(0.0));
    assert_eq!(tree_state(&root), before, "AI ไม่แตะไฟล์ต้นฉบับ");
}

#[test]
fn ai_generated_images_are_saved_tidily_and_remember_where_they_came_from() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 งาน/กฐิน");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("IMG_1.jpg"), jpeg_with_exif(400, 300, 1, "2026:11:21 10:00:00")).unwrap();
    lib.import(&root, "single_event", |_| true);
    let before = tree_state(&root);
    let source = lib.photo_id("IMG_1.jpg");

    // ภาพที่ "AI สร้าง" (PNG แบบที่ Codex ให้มา) → JPEG ในโฟลเดอร์ของกิจกรรม ชื่อไม่ชนกัน
    let out = lib.photos().join("ภาพจาก AI").join("งาน กฐิน");
    let png = |blue: u8| {
        let img = image::RgbImage::from_fn(96, 64, |x, y| image::Rgb([x as u8 * 2, y as u8 * 3, blue]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    };
    let first = puri_core::assistant::generate::test_save(&lib.app, &source, &png(120), &out, "ภาพวาดสีน้ำ").unwrap();
    let second =
        puri_core::assistant::generate::test_save(&lib.app, &source, &png(30), &out, "ภาพวาดสีน้ำ อีกแบบ").unwrap();
    assert_eq!(first.file_name().unwrap(), "IMG_1-ai-1.jpg");
    assert_eq!(second.file_name().unwrap(), "IMG_1-ai-2.jpg");
    assert_eq!(&std::fs::read(&first).unwrap()[..3], &[0xFF, 0xD8, 0xFF], "เป็น JPEG");
    assert!(std::fs::read_dir(&out).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".part")));

    // เข้าคลังแล้วรู้ที่มา: รูปต้นทาง prompt แม่แบบ AI ที่ใช้
    lib.import(&out, "single_event", |_| true);
    let made = lib.photo_id("IMG_1-ai-1.jpg");
    let origin = lib.call("get_photo_details", json!({ "photoId": made }))["aiOrigin"].clone();
    assert_eq!(origin["sourcePhotoId"], json!(source));
    assert_eq!(
        (origin["sourceFilename"].as_str(), origin["prompt"].as_str(), origin["template"].as_str()),
        (Some("IMG_1.jpg"), Some("ภาพวาดสีน้ำ"), Some("สีน้ำ"))
    );
    assert!(lib.call("get_photo_details", json!({ "photoId": source }))["aiOrigin"].is_null(), "รูปต้นทางไม่ใช่ภาพจาก AI");
    // ภาพที่สองมีที่มาของตัวเอง (ผูกด้วย SHA-256 ของไฟล์ ไม่ใช่ path — ย้ายโฟลเดอร์แล้วยังรู้ที่มา)
    let moved = lib.call("get_photo_details", json!({ "photoId": lib.photo_id("IMG_1-ai-2.jpg") }))["aiOrigin"].clone();
    assert_eq!(moved["prompt"], "ภาพวาดสีน้ำ อีกแบบ");
    assert_eq!(tree_state(&root), before, "ไฟล์ต้นฉบับไม่ถูกแตะ");
}

#[test]
fn ai_matches_photos_to_a_reference_look() {
    use puri_core::assistant::{look, Reference};
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 ภาพตัวอย่าง");
    std::fs::create_dir_all(&root).unwrap();
    // ภาพคล้ายภาพถ่าย: ส่วนบนสีต่าง ๆ (ต่างกันเล็กน้อยแต่ละภาพ) ส่วนล่างเป็นสีเทาไล่ความสว่าง (เหมือนผนัง/เสื้อขาว)
    let photo = |shift: u8| {
        let img = image::RgbImage::from_fn(320, 240, |x, y| {
            let (t, s) = (x as f64 / 319.0, y as f64 / 239.0);
            if y >= 150 {
                let g = (20.0 + 225.0 * t) as u8;
                return image::Rgb([g, g, g]);
            }
            image::Rgb([
                (30.0 + 190.0 * t) as u8,
                ((40.0 + 150.0 * s) as u8).wrapping_add(shift),
                (60.0 + 120.0 * (1.0 - t) * s) as u8,
            ])
        });
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92).encode_image(&img).unwrap();
        out
    };
    for (name, shift) in [("ref.jpg", 0u8), ("a.jpg", 20), ("b.jpg", 35)] {
        std::fs::write(root.join(name), photo(shift)).unwrap();
    }
    lib.import(&root, "single_event", |_| true);
    let (r, a) = (lib.photo_id("ref.jpg"), lib.photo_id("a.jpg"));
    // ภาพตัวอย่าง: สว่าง อุ่น สีจาง เงาออกฟ้า (แต่งในแอป)
    let film = json!({ "exposure": 0.6, "temperature": 35, "saturation": -25, "grading": { "shadows": { "hue": 200, "sat": 35, "lum": 0 } } });
    lib.call("save_photo_edit", json!({ "photoId": r, "settings": { "adjust": film } }));
    let measure = |id: &str| {
        let asset = puri_core::api::asset(&lib.app, &format!("photo/{id}")).unwrap();
        look::measure(&image::load_from_memory(&asset.bytes).unwrap().thumbnail(256, 256).to_rgb8())
    };
    let reference = measure(&r);
    let warmth = |l: &look::Look| l.cast.map(|c| c.1).unwrap_or(0.0);
    let gap = |l: &look::Look| {
        (l.l[2] - reference.l[2]).abs() + (warmth(l) - warmth(&reference)).abs() + (l.chroma - reference.chroma).abs()
    };
    let before = gap(&measure(&a));

    let (tools, turn) =
        puri_core::assistant::test_tools_with(&lib.app, Some(Reference::Photo { photo_id: r.clone() })).unwrap();
    let text = |out: &puri_core::assistant::mcp::ToolOutput| {
        out.content.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")
    };
    let names: Vec<&str> = tools.tools().iter().map(|t| t.name).collect();
    assert!(names.contains(&"view_reference") && names.contains(&"compare_to_reference"));
    let seen = tools.call("view_reference", json!({ "size": 256 }));
    assert_eq!(seen.content[0]["type"], "image");
    assert!(
        text(&seen).contains("Reference image “ref.jpg”") && text(&seen).contains("Measured look"),
        "{}",
        text(&seen)
    );

    // ค่าที่เสนอ (เป็น JSON ท้ายข้อความ) ใช้กับ edit_photo ได้ตรง ๆ และทำให้ลุคใกล้ภาพตัวอย่างมาก
    let suggested = |id: &str| {
        let compared = text(&tools.call("compare_to_reference", json!({ "photo_id": id })));
        assert!(compared.contains("Distance to the reference"), "{compared}");
        serde_json::from_str::<serde_json::Value>(&compared[compared.rfind("): ").unwrap() + 3..]).unwrap()
    };
    let patch = suggested(&a);
    assert!(patch["exposure"].as_f64().unwrap() > 0.2 && patch["temperature"].as_f64().unwrap() > 5.0, "{patch}");
    let out = tools.call("edit_photo", json!({ "photo_id": a, "adjust": patch }));
    assert!(!out.is_error, "{}", text(&out));
    let after = gap(&measure(&a));
    assert!(after < before / 3.0, "ห่างจากภาพตัวอย่าง ก่อน {before:.1} หลัง {after:.1}");
    // ถามซ้ำหลังใส่ค่าแล้วได้ค่าเดิม (เดิมเสนอส่วนเพิ่มบนภาพที่แต่งแล้ว จึงสะสมแรงขึ้นทุกรอบ)
    assert_eq!(suggested(&a), patch);
    // ย้อนได้เหมือนการแก้อื่นของ AI
    assert_eq!(lib.call("ai_undo", json!({ "turnId": turn })), json!(1));
    assert!(lib.call("get_photo_details", json!({ "photoId": a }))["edit"].is_null());

    // ภาพตัวอย่างเป็นไฟล์จากที่อื่นได้ (PNG) · ไฟล์ที่ไม่ใช่ภาพ/ไม่มีอยู่ ถูกปฏิเสธพร้อมเหตุผล
    let png = lib.photos().join("look.png");
    image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(64, 48, |x, _| image::Rgb([200, 120 + x as u8, 60])))
        .save(&png)
        .unwrap();
    let (tools, _) =
        puri_core::assistant::test_tools_with(&lib.app, Some(Reference::File { path: png.to_string_lossy().into() }))
            .unwrap();
    assert!(text(&tools.call("view_reference", json!({}))).contains("“look.png”"));
    // ไม่รู้ค่าที่แต่งของภาพตัวอย่าง: ค้นหาจากตัวเลขของภาพ และบอกว่าคอนทราสต์/ความอิ่มสีขึ้นกับเนื้อหา
    let measured = text(&tools.call("compare_to_reference", json!({ "photo_id": a })));
    assert!(measured.contains("depend on the subject") && !measured.contains("was edited in the app"), "{measured}");
    let (tools, _) = puri_core::assistant::test_tools_with(
        &lib.app,
        Some(Reference::File { path: lib.photos().join("gone.png").to_string_lossy().into() }),
    )
    .unwrap();
    assert!(tools.call("view_reference", json!({})).is_error);
    let err = lib
        .try_call("ai_start", json!({ "provider": "claude", "messages": [{ "role": "user", "text": "แต่งตามนี้" }], "reference": { "kind": "file", "path": "/nope/x.gif" } }))
        .unwrap_err();
    assert_eq!(err.code, "UNSUPPORTED_FORMAT");

    // รูปที่เลือก → รหัสและชื่อไฟล์ (ให้หน้าต่างแสดงให้เลือกภาพตัวอย่าง)
    let sel = lib.call("selection_photos", json!({ "selection": { "kind": "explicit", "photoIds": [r, a] } }));
    assert_eq!(sel["total"], 2);
    assert_eq!(sel["items"][0]["filename"], "ref.jpg");
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
    // เทสต์อื่นที่รันพร้อมกันอาจ fork process ลูกตอนคลังเดิมกำลังปิด (ลูกถือตัวล็อกไว้ชั่วครู่จนถึง exec): รอล็อกคลายก่อน
    let mut result = open(&dir);
    for _ in 0..100 {
        if !matches!(&result, Err(e) if e.code == "LIBRARY_LOCKED") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        result = open(&dir);
    }
    assert_eq!(result.err().unwrap().code, "LIBRARY_TOO_NEW");
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

/// ไฟล์ RAW (DECISIONS ข้อ 25): โฟลเดอร์แบบที่ช่างภาพใช้ — JPEG ที่ export ไว้ในโฟลเดอร์งาน + RAW ในโฟลเดอร์ย่อย
/// นำเข้าเฉพาะเมื่อเลือกรวม RAW · ภาพย่อ ตัวดูภาพ ใบหน้า แต่ง และส่งออกเป็น JPEG ได้ครบ · ไฟล์ RAW ไม่ถูกแตะ
#[test]
fn raw_files_are_imported_on_request_developed_edited_and_exported_as_jpeg() {
    let lib = Lib::with_ai();
    assert_eq!(lib.install_model()["state"], "completed");
    let root = lib.photos().join("2026-11-21 งานถ่าย RAW");
    std::fs::create_dir_all(root.join("raw")).unwrap();
    write_jpeg(&root.join("A001.jpg"), 60);
    // ฉากในแสงเชิงเส้น: สีส้มกลางภาพ (ตัววิเคราะห์จำลองอ่านสีแดงกลางภาพเป็น "บุคคล")
    let scene = |shade: f32| {
        puri_core::raw::Linear::from_fn(96, 64, move |x, _| {
            image::Rgb([0.30 * shade, 0.12 * shade, (0.02 + 0.3 * x as f32 / 95.0) * shade])
        })
    };
    std::fs::write(root.join("raw/A001.dng"), puri_core::raw::test_dng(&scene(1.0), 6, "2026:11:21 10:20:30")).unwrap();
    std::fs::write(root.join("raw/A002.dng"), puri_core::raw::test_dng(&scene(0.5), 1, "2026:11:21 10:21:00")).unwrap();
    let before = tree_state(&root);

    // ตรวจรายการ: RAW นับแยก ไม่รวมใน photoCount · ไม่เลือกรวม RAW = นำเข้าเฉพาะ JPEG
    let preview = lib.preview(&root);
    assert_eq!((preview["single"]["photoCount"].as_i64(), preview["single"]["rawCount"].as_i64()), (Some(1), Some(2)));
    lib.import(&root, "single_event", |_| true);
    assert_eq!(lib.filenames(json!({})), vec!["A001.jpg"]);

    // เพิ่มโฟลเดอร์เดิมอีกครั้งพร้อมเลือกรวม RAW → กิจกรรมเดิมได้ RAW เพิ่ม (ตรวจรูปใหม่ครั้งต่อไปก็ยังรวม)
    let preview = lib.preview(&root);
    assert_eq!(preview["single"]["rawNewCount"].as_i64(), Some(2));
    let drafts = drafts_of(&preview, "single_event", |_| true);
    let result = lib.call(
        "commit_import",
        json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": "single_event", "drafts": drafts,
                "analyzeFaces": true, "watch": true, "includeRaw": true }),
    );
    assert_eq!(result["reusedEvents"], 1);
    lib.run_jobs();
    assert_eq!(lib.filenames(json!({})), vec!["A001.dng", "A001.jpg", "A002.dng"]);
    let event = result["eventIds"][0].as_str().unwrap().to_string();
    lib.call("rescan_event", json!({ "eventId": event }));
    lib.run_jobs();
    assert_eq!(lib.filenames(json!({})).len(), 3, "ตรวจรูปใหม่ไม่ซ้ำ ไม่หาย");

    // ข้อมูลรูป: ขนาดหลังหมุนตามไฟล์ (orientation 6) · วันถ่ายจาก RAW · เป็น RAW
    let raw_id = lib.photo_id("A001.dng");
    let details = lib.call("get_photo_details", json!({ "photoId": raw_id }));
    assert_eq!((details["width"].as_i64(), details["height"].as_i64()), (Some(64), Some(96)), "{details}");
    assert_eq!(details["takenAt"], "2026-11-21T10:20:30");
    assert_eq!((details["raw"].as_bool(), details["thumbStatus"].as_str()), (Some(true), Some("ready")));
    assert_eq!(lib.call("get_photo_details", json!({ "photoId": lib.photo_id("A001.jpg") }))["raw"], false);
    assert!(!arr(&details["faces"]).is_empty(), "วิเคราะห์ใบหน้าจากภาพที่ล้างแล้ว: {details}");

    // UI เปิดไฟล์ RAW ไม่ได้: ภาพย่อ "ต้นฉบับ" และครอปใบหน้าเป็น JPEG ที่ล้างแล้ว ตั้งตรง
    let asset = |path: String| puri_core::api::asset(&lib.app, &path).unwrap();
    for kind in ["thumb", "original", "photo"] {
        let a = asset(format!("{kind}/{raw_id}"));
        assert_eq!(a.mime, "image/jpeg", "{kind}");
        let img = image::load_from_memory(&a.bytes).unwrap();
        assert_eq!((img.width(), img.height()), (64, 96), "{kind}");
    }
    let face = arr(&details["faces"])[0]["id"].as_str().unwrap().to_string();
    assert!(asset(format!("face/{face}")).bytes.starts_with(&[0xFF, 0xD8]));

    // แต่ง RAW: ค่าแสง +1 ทำในแสงของฉาก (ส่วนสว่างไม่ขาดเหมือนภาพทั่วไป) + ตัดครึ่งบน
    let mean = |bytes: &[u8]| {
        let img = image::load_from_memory(bytes).unwrap().to_rgb8();
        img.pixels().map(|p| p.0[1] as f64).sum::<f64>() / (img.width() * img.height()) as f64
    };
    let plain = mean(&asset(format!("original/{raw_id}")).bytes);
    lib.call(
        "save_photo_edit",
        json!({ "photoId": raw_id, "settings": { "crop": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 0.5 }, "adjust": { "exposure": 1.0 } } }),
    );
    let edited = asset(format!("photo/{raw_id}")).bytes;
    let img = image::load_from_memory(&edited).unwrap();
    assert_eq!((img.width(), img.height()), (64, 48));
    assert!(mean(&edited) > plain + 20.0, "สว่างขึ้น: {plain} → {}", mean(&edited));

    // ส่งออก "ขนาดเดิม": RAW เป็น JPEG เสมอ (ชื่อ .jpg) ตามที่แต่ง พร้อมวันถ่าย · ไม่ชนกับ A001.jpg ที่มีอยู่
    let dest = lib.tmp.path().join("ส่งออก RAW");
    std::fs::create_dir_all(&dest).unwrap();
    let selection = json!({ "kind": "explicit", "photoIds": [raw_id, lib.photo_id("A002.dng")] });
    let plan = lib.call(
        "preview_export",
        json!({ "selection": selection, "destination": dest, "resize": { "mode": "original" } }),
    );
    let names: Vec<&str> = arr(&plan["sampleNames"]).iter().map(|s| s["output"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["A001.jpg", "A002.jpg"], "{plan}");
    let job = lib.call("start_export", json!({ "planId": plan["planId"], "requestId": new_id() }));
    lib.run_jobs();
    let report = lib.call("get_export_report", json!({ "jobId": job["id"] }));
    assert_eq!((report["copied"].as_i64(), report["failed"].as_i64()), (Some(2), Some(0)), "{report}");
    let out = std::fs::read(dest.join("A001.jpg")).unwrap();
    let img = image::load_from_memory(&out).unwrap();
    assert_eq!((img.width(), img.height()), (64, 48), "ตามที่แต่ง");
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
    assert_eq!(
        exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).unwrap().display_value().to_string(),
        "2026-11-21 10:20:30"
    );
    assert_eq!(exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY).unwrap().value.get_uint(0), Some(1));
    let other = image::load_from_memory(&std::fs::read(dest.join("A002.jpg")).unwrap()).unwrap();
    assert_eq!((other.width(), other.height()), (96, 64));

    // ย่อไฟล์ต้นฉบับ: RAW ไม่อยู่ในแผน (เฉพาะ JPEG)
    let shrink =
        lib.call("preview_shrink", json!({ "selection": selection, "resize": { "mode": "long_edge", "px": 2048 } }));
    assert_eq!((shrink["plannedCount"].as_i64(), shrink["notJpeg"].as_i64()), (Some(0), Some(2)), "{shrink}");

    assert_eq!(tree_state(&root), before, "ไฟล์ RAW และ JPEG ต้นฉบับไม่ถูกแตะ");
}

/// ไฟล์ RAW เสียหรือกล้องที่ตัวถอดรหัสไม่รู้จัก: ล้มเฉพาะไฟล์นั้นพร้อมเหตุผล ไม่ขวางไฟล์อื่น (เหมือน JPEG เสีย — T07)
#[test]
fn a_broken_raw_file_fails_alone_with_a_reason() {
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 RAW ปน");
    std::fs::create_dir_all(&root).unwrap();
    let scene = puri_core::raw::Linear::from_fn(64, 48, |x, _| image::Rgb([0.2, 0.15, 0.02 + x as f32 / 200.0]));
    std::fs::write(root.join("good.dng"), puri_core::raw::test_dng(&scene, 1, "2026:11:21 09:00:00")).unwrap();
    std::fs::write(root.join("broken.ARW"), b"II*\0 not really a raw file").unwrap();
    let before = tree_state(&root);
    let preview = lib.preview(&root);
    assert_eq!(preview["single"]["rawCount"], 2);
    let drafts = drafts_of(&preview, "single_event", |_| true);
    let result = lib.call(
        "commit_import",
        json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": "single_event", "drafts": drafts,
                "analyzeFaces": true, "watch": true, "includeRaw": true }),
    );
    lib.run_jobs();
    let job = lib.call("get_job", json!({ "jobId": result["jobIds"][0] }));
    assert_eq!((job["counts"]["done"].as_i64(), job["counts"]["failed"].as_i64()), (Some(1), Some(1)), "{job}");
    let errors = lib.call("list_job_errors", json!({ "jobId": job["id"] }));
    assert_eq!(errors[0]["filename"], "broken.ARW");
    assert_eq!(errors[0]["code"], "DECODE_FAILED");
    assert!(errors[0]["message"].as_str().unwrap().contains("RAW"), "{errors}");
    let good = lib.call("get_photo_details", json!({ "photoId": lib.photo_id("good.dng") }));
    assert_eq!(good["thumbStatus"], "ready");
    assert_eq!(tree_state(&root), before);
}

/// JPEG ที่ "Lightroom ส่งออก" จาก DNG สังเคราะห์: ภาพจาก RAW ในกรอบที่ตัด/เอียง ค่าแสง `exposure`
/// พร้อม EXIF ของกล้อง และ XMP แบบที่ Lightroom เขียน (`xmp` = None → JPEG จากกล้อง ไม่มี XMP)
fn exported_jpeg(dng: &[u8], frame: puri_core::pair::Frame, exposure: f64, xmp: Option<&str>) -> Vec<u8> {
    let info = puri_core::raw::info(dng).unwrap();
    let (fw, fh) = frame.output_size((info.width, info.height), info.exif.orientation);
    let base = puri_core::pair::RawBase {
        raw_path: Default::default(),
        frame,
        jpeg: (fw.round() as u32, fh.round() as u32),
        start: Default::default(),
        unsupported: vec![],
    };
    let settings = puri_core::edit::EditSettings {
        adjust: puri_core::adjust::Adjust { exposure, ..Default::default() },
        from_raw: true,
        ..Default::default()
    };
    let img = puri_core::pair::render(dng, &base, &settings, u32::MAX).unwrap();
    let mut encoded = vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 95).encode_image(&img).unwrap();
    let mut out = encoded[..2].to_vec();
    out.extend(info.exif_app1());
    if let Some(xmp) = xmp {
        let payload = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), xmp.as_bytes()].concat();
        out.extend([0xFF, 0xE1]);
        out.extend(((payload.len() + 2) as u16).to_be_bytes());
        out.extend(payload);
    }
    out.extend(&encoded[2..]);
    out
}

#[test]
fn raw_and_jpeg_of_one_shot_become_one_photo_edited_from_the_raw() {
    use puri_core::pair::Frame;
    let lib = Lib::new();
    let root = lib.photos().join("2026-11-21 ส่งออกจาก Lightroom");
    std::fs::create_dir_all(root.join("sony_raw")).unwrap();
    // ฉากในแสงเชิงเส้น: ครึ่งซ้ายส้ม ครึ่งขวาฟ้า แถบบนไล่สีเทา (ตัด/เอียงแล้วเนื้อหาเลื่อนให้เห็น)
    let scene = puri_core::raw::Linear::from_fn(150, 100, |x, y| {
        if y < 30 {
            let v = 0.05 + 0.5 * x as f32 / 149.0;
            image::Rgb([v, v, v])
        } else if x < 75 {
            image::Rgb([0.40, 0.12, 0.04])
        } else {
            image::Rgb([0.05, 0.18, 0.36])
        }
    });
    // ภาพแนวตั้ง (Orientation 6): กรอบของ Lightroom เป็นพิกัดบนภาพตามแนวที่เก็บในไฟล์
    let shot = puri_core::raw::test_dng(&scene, 6, "2026:11:21 09:00:00");
    let frame = Frame { left: 0.04, top: 0.02, right: 0.97, bottom: 0.95, angle: 1.5 };
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
       crs:RawFileName="SHOT001.DNG" crs:WhiteBalance="As Shot" crs:Exposure2012="+0.50" crs:Texture="-10"
       crs:HasCrop="True" crs:CropLeft="0.04" crs:CropTop="0.02" crs:CropRight="0.97" crs:CropBottom="0.95"
       crs:CropAngle="1.5" crs:LensProfileEnable="1"/></rdf:RDF></x:xmpmeta>"#;
    std::fs::write(root.join("SHOT001.jpg"), exported_jpeg(&shot, frame, 0.5, Some(xmp))).unwrap();
    std::fs::write(root.join("sony_raw/SHOT001.DNG"), &shot).unwrap();
    // RAW+JPEG จากกล้อง: ชื่อเดียวกันในโฟลเดอร์เดียวกัน ไม่มี XMP
    let camera = puri_core::raw::test_dng(&scene, 1, "2026:11:21 09:01:00");
    std::fs::write(root.join("DSC0002.JPG"), exported_jpeg(&camera, Frame::FULL, 0.0, None)).unwrap();
    std::fs::write(root.join("DSC0002.DNG"), &camera).unwrap();
    // RAW ที่ไม่ได้ส่งออก และ JPEG ที่ไฟล์ RAW ไม่ได้อยู่ในโฟลเดอร์นี้
    std::fs::write(root.join("sony_raw/SHOT009.DNG"), puri_core::raw::test_dng(&scene, 1, "2026:11:21 09:09:00"))
        .unwrap();
    std::fs::write(
        root.join("SHOT003.jpg"),
        exported_jpeg(&camera, Frame::FULL, 0.0, Some(&xmp.replace("SHOT001.DNG", "SHOT003.DNG"))),
    )
    .unwrap();
    let before = tree_state(&root);

    // ตรวจรายการ: RAW ที่เป็นคู่ไม่นับเป็นรูป · สวิตช์รวม RAW มีผลกับ RAW ที่ไม่มีคู่เท่านั้น
    let preview = lib.preview(&root);
    let single = &preview["single"];
    assert_eq!(
        (single["photoCount"].as_i64(), single["rawPairedCount"].as_i64(), single["rawCount"].as_i64()),
        (Some(3), Some(2), Some(1)),
        "{single}"
    );
    assert_eq!(preview["suggestedMode"], "single_event", "โฟลเดอร์ RAW ของ JPEG ไม่ใช่งานแยก");
    lib.import(&root, "single_event", |_| true);
    assert_eq!(lib.filenames(json!({})), vec!["DSC0002.JPG", "SHOT001.jpg", "SHOT003.jpg"], "รูปละหนึ่ง ไม่มี RAW ซ้ำ");
    let page = lib.query(json!({}));
    let pairs: Vec<&Value> = arr(&page["items"]).iter().map(|i| &i["rawPair"]).collect();
    assert_eq!(pairs, vec![&json!("DSC0002.DNG"), &json!("SHOT001.DNG"), &Value::Null]);

    // ข้อมูลรูป: ขนาด ใบหน้า วันที่ ยังเป็นของ JPEG · แต่งจาก RAW ได้ ค่าเริ่มต้นจาก Lightroom (ค่าแสงใกล้ +0.5)
    let id = lib.photo_id("SHOT001.jpg");
    let details = lib.call("get_photo_details", json!({ "photoId": id }));
    let jpeg_dims = (details["width"].as_u64().unwrap() as u32, details["height"].as_u64().unwrap() as u32);
    assert!(jpeg_dims.1 > jpeg_dims.0, "แนวตั้ง: {jpeg_dims:?}");
    let info = &details["rawPairInfo"];
    assert_eq!((info["lightroom"].as_bool(), info["editFromRaw"].as_bool()), (Some(true), Some(true)), "{info}");
    assert!(info["problem"].is_null(), "{info}");
    let start = info["start"].clone();
    assert_eq!(start["fromRaw"], true);
    let exposure = start["adjust"]["exposure"].as_f64().unwrap();
    assert!((exposure - 0.5).abs() < 0.15, "ค่าแสงที่ทำให้ภาพเท่ากับ JPEG: {exposure}");
    let unsupported: Vec<&str> = arr(&info["unsupported"]).iter().filter_map(Value::as_str).collect();
    assert_eq!(unsupported, vec!["แก้เลนส์ (Lens Corrections)"], "Texture ทำตามได้ (ข้อ 27)");
    assert_eq!(start["adjust"]["texture"], -10.0);
    let camera_info =
        lib.call("get_photo_details", json!({ "photoId": lib.photo_id("DSC0002.JPG") }))["rawPairInfo"].clone();
    assert_eq!((camera_info["lightroom"].as_bool(), camera_info["editFromRaw"].as_bool()), (Some(false), Some(true)));
    assert!(camera_info["start"]["adjust"]["exposure"].as_f64().unwrap().abs() < 0.15, "{camera_info}");
    assert!(lib.call("get_photo_details", json!({ "photoId": lib.photo_id("SHOT003.jpg") }))["rawPairInfo"].is_null());

    // ต้นทางของหน้าแต่งภาพ: ภาพจาก RAW ในกรอบเดียวกับ JPEG
    let asset = |path: String| puri_core::api::asset(&lib.app, &path).unwrap().bytes;
    let rgb = |bytes: &[u8]| image::load_from_memory(bytes).unwrap().to_rgb8();
    let frame_img = rgb(&asset(format!("rawframe/{id}")));
    assert_eq!(frame_img.dimensions(), jpeg_dims);

    // แต่ง: ค่าที่ไม่ได้แต่งของรูปคู่คือค่าเริ่มต้น — แต่งต่อจากนั้นได้ภาพจาก RAW ที่แทบเท่ากับ JPEG (+ ที่ปรับ)
    let original = rgb(&asset(format!("original/{id}")));
    let mut settings = start.clone();
    settings["adjust"]["contrast"] = json!(1.0);
    let saved = lib.call("save_photo_edit", json!({ "photoId": id, "settings": settings }));
    assert_eq!(saved["edit"]["settings"]["fromRaw"], true, "{saved}");
    let edited = rgb(&asset(format!("photo/{id}")));
    assert_eq!(edited.dimensions(), original.dimensions());
    let diff = edited.pixels().zip(original.pixels()).map(|(a, b)| (a[1] as f64 - b[1] as f64).abs()).sum::<f64>()
        / (edited.width() * edited.height()) as f64;
    assert!(diff < 6.0, "กรอบและโทนตรงกับ JPEG: ต่างเฉลี่ย {diff}");
    // บันทึกค่าเริ่มต้นกลับไป = ไม่ได้แต่ง (แสดง JPEG ตามเดิม) · กลับเป็นต้นฉบับ = ลบการแต่ง ไม่ใช่ RAW ที่ไม่ได้แต่ง
    assert!(lib.call("save_photo_edit", json!({ "photoId": id, "settings": start }))["edit"].is_null());
    settings["adjust"]["exposure"] = json!(1.5);
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": settings }));
    assert!(lib.call("reset_photo_edit", json!({ "photoId": id }))["edit"].is_null());
    let brighter = lib.call("save_photo_edit", json!({ "photoId": id, "settings": settings }));
    assert_eq!(brighter["edit"]["settings"]["fromRaw"], true);

    // ผู้ช่วย AI: แต่งต่อจากค่าของ Lightroom (ไม่ใช่ RAW ที่ไม่ได้แต่ง) · ย้อนแล้วกลับเป็น JPEG ตามเดิม
    lib.call("reset_photo_edit", json!({ "photoId": id }));
    let (tools, turn) = puri_core::assistant::test_tools(&lib.app).unwrap();
    let out = tools.call("edit_photo", json!({ "photo_id": id, "adjust": { "contrast": 12 } }));
    assert!(!out.is_error, "{:?}", out.content);
    let by_ai = lib.call("get_photo_details", json!({ "photoId": id }))["edit"]["settings"].clone();
    assert_eq!((by_ai["fromRaw"].as_bool(), by_ai["adjust"]["contrast"].as_f64()), (Some(true), Some(12.0)));
    assert_eq!(by_ai["adjust"]["exposure"], start["adjust"]["exposure"], "คงค่าแสงที่ตรงกับ JPEG");
    assert_eq!(lib.call("ai_undo", json!({ "turnId": turn })), json!(1));
    assert!(lib.call("get_photo_details", json!({ "photoId": id }))["edit"].is_null());
    lib.call("save_photo_edit", json!({ "photoId": id, "settings": settings }));

    // ส่งออก: ภาพจาก RAW ตามที่แต่ง ขนาดเท่า JPEG · EXIF (วันถ่าย) จาก JPEG
    let dest = lib.tmp.path().join("ส่งออกรูปคู่");
    std::fs::create_dir_all(&dest).unwrap();
    let (_, report) = run_export(&lib, json!({ "kind": "explicit", "photoIds": [id] }), &dest);
    assert_eq!((report["copied"].as_i64(), report["failed"].as_i64()), (Some(1), Some(0)), "{report}");
    let out = std::fs::read(dest.join("SHOT001.jpg")).unwrap();
    let exported = rgb(&out);
    assert_eq!(exported.dimensions(), jpeg_dims);
    let mean =
        |img: &image::RgbImage| img.pixels().map(|p| p[1] as f64).sum::<f64>() / (img.width() * img.height()) as f64;
    assert!(mean(&exported) > mean(&original) + 15.0, "สว่างขึ้นตามที่แต่ง");
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
    assert_eq!(
        exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).unwrap().display_value().to_string(),
        "2026-11-21 09:00:00"
    );
    assert_eq!(tree_state(&root), before, "ไฟล์ JPEG และ RAW ไม่ถูกแตะ");

    // เลือกรวม RAW: เฉพาะ RAW ที่ไม่มีคู่เป็นรูปเพิ่ม · ตรวจรูปใหม่ไม่ซ้ำ
    let preview = lib.preview(&root);
    let drafts = drafts_of(&preview, "single_event", |_| true);
    let result = lib.call(
        "commit_import",
        json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": "single_event", "drafts": drafts,
                "analyzeFaces": true, "watch": true, "includeRaw": true }),
    );
    lib.run_jobs();
    let names = vec!["DSC0002.JPG", "SHOT001.jpg", "SHOT003.jpg", "SHOT009.DNG"];
    assert_eq!(lib.filenames(json!({})), names);
    let event = result["eventIds"][0].as_str().unwrap().to_string();
    lib.call("rescan_event", json!({ "eventId": event }));
    lib.run_jobs();
    assert_eq!(lib.filenames(json!({})), names);

    // ไฟล์ RAW หาย: ยังเป็นรูปเดิม บอกเหตุ · ภาพที่แต่งจาก RAW แสดง JPEG ตามเดิมแทน (ไม่ล้ม) · ส่งออกบอกเหตุรายไฟล์
    std::fs::rename(root.join("sony_raw/SHOT001.DNG"), lib.tmp.path().join("SHOT001.DNG")).unwrap();
    lib.call("rescan_event", json!({ "eventId": event }));
    lib.run_jobs();
    let details = lib.call("get_photo_details", json!({ "photoId": id }));
    assert!(details["rawPairInfo"]["problem"].as_str().unwrap().contains("SHOT001.DNG"), "{details}");
    assert_eq!(details["rawPairInfo"]["editFromRaw"], true, "ยังแต่งจาก RAW ไว้");
    assert_eq!(rgb(&asset(format!("photo/{id}"))).dimensions(), jpeg_dims);
    let (_, report) = run_export(&lib, json!({ "kind": "explicit", "photoIds": [id] }), &dest);
    assert_eq!(report["failed"].as_i64(), Some(1), "{report}");
}
