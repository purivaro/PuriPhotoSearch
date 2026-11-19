//! สำรองและกู้คืนคลัง (Blueprint S20, 13.3–13.4, T28) — คลัง SQLite จริงและไฟล์จริงบนดิสก์ ผ่าน `api::dispatch`

mod common;

use common::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array")
}

/// คลังที่มีหนึ่งกิจกรรม สองรูป และหนึ่งคนที่ยืนยันแล้ว
fn library_with_data() -> (Lib, PathBuf) {
    let lib = Lib::new();
    let root = lib.photos().join("691118 งานอบรม AI");
    write_jpeg(&root.join("IMG_0001.jpg"), 1);
    write_jpeg(&root.join("IMG_0002.jpg"), 2);
    lib.import(&root, "single_event", |_| true);
    lib.label("IMG_0001.jpg", 0.2, ("name", "คุณกานต์"));
    (lib, root)
}

fn dest(lib: &Lib, name: &str) -> PathBuf {
    let dir = lib.tmp.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    // path จริงของระบบ (macOS: /var → /private/var) ให้เทียบกับ path ที่แอปรายงานได้ตรง ๆ
    puri_core::util::canonical(&dir).unwrap()
}

fn people(lib: &Lib) -> Vec<String> {
    arr(&lib.call("list_people", json!({}))).iter().map(|p| p["displayName"].as_str().unwrap().to_string()).collect()
}

fn sets_in(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut sets: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    sets.retain(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix));
    sets.sort();
    sets
}

#[test]
fn t28_backup_then_restore_brings_back_the_backed_up_state_and_keeps_the_replaced_library() {
    let (lib, root) = library_with_data();
    let originals = tree_state(&root);
    let out = dest(&lib, "ชุดสำรอง");

    let backup = lib.call("create_backup", json!({ "destination": out }));
    let set = PathBuf::from(backup["path"].as_str().unwrap());
    assert!(set.starts_with(&out) && set.join("library.sqlite").is_file() && set.join("manifest.json").is_file());
    assert_eq!(
        (backup["kind"].as_str(), backup["counts"]["photos"].as_i64(), backup["counts"]["persons"].as_i64()),
        (Some("manual"), Some(2), Some(1))
    );
    assert_eq!(backup["counts"]["confirmations"], 1);
    assert!(arr(&backup["excluded"]).contains(&json!("originals")), "บอกชัดว่าไม่รวมรูปต้นฉบับ");
    // ชุดสำรองมีแค่สองไฟล์: ไม่มีไฟล์ชั่วคราวหรือไฟล์ -wal/-shm ค้าง
    let mut files: Vec<String> =
        std::fs::read_dir(&set).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(files, ["library.sqlite", "manifest.json"]);
    assert_eq!(sets_in(&out, ".").len(), 0, "ไม่เหลือโฟลเดอร์ .partial");
    let status = lib.call("get_backup_status", json!({}));
    assert_eq!(status["lastBackupPath"], backup["path"]);
    assert!(status["pendingRestore"].is_null());

    // แก้คลังหลังสำรอง: เพิ่มคน และเอาคำยืนยันเดิมออก
    lib.label("IMG_0002.jpg", 0.6, ("name", "คุณเมย์"));
    assert_eq!(people(&lib).len(), 2);

    // ดูผลกระทบก่อน แล้ววางรอ: คลังที่เปิดอยู่ยังไม่เปลี่ยน
    let preview = lib.call("preview_restore", json!({ "path": set }));
    assert_eq!((preview["sameLibrary"].as_bool(), preview["needsUpgrade"].as_bool()), (Some(true), Some(false)));
    assert_eq!(
        (preview["backup"]["counts"]["persons"].as_i64(), preview["current"]["persons"].as_i64()),
        (Some(1), Some(2))
    );
    lib.call("stage_restore", json!({ "path": set }));
    assert_eq!(people(&lib).len(), 2, "กู้คืนยังไม่เกิดจนกว่าจะเปิดแอปใหม่");
    assert_eq!(lib.call("get_backup_status", json!({}))["pendingRestore"]["counts"]["persons"], 1);

    // เปิดแอปใหม่: ได้สถานะ ณ ตอนสำรอง และคลังที่ถูกแทนถูกเก็บไว้
    let lib = lib.reopen();
    assert_eq!(people(&lib), ["คุณกานต์"]);
    assert_eq!(lib.query(json!({}))["total"], 2);
    assert_eq!(lib.filenames(json!({ "personIds": [lib.person_id("คุณกานต์")] })), ["IMG_0001.jpg"]);
    let status = lib.call("get_backup_status", json!({}));
    assert!(status["pendingRestore"].is_null());
    let kept = arr(&status["automatic"]).iter().find(|b| b["kind"] == "before_restore").expect("ชุดก่อนกู้คืน");
    assert_eq!(kept["counts"]["persons"], 2, "คลังที่ถูกแทน (มีสองคน) ยังกู้กลับได้");

    // กู้ชุด "ก่อนกู้คืน" กลับ = ย้อนการกู้คืนได้
    lib.call("stage_restore", json!({ "path": kept["path"] }));
    let lib = lib.reopen();
    assert_eq!(people(&lib).len(), 2);
    assert_eq!(tree_state(&root), originals, "ต้นฉบับไม่ถูกแตะตลอดการสำรองและกู้คืน");
}

#[test]
fn restore_refuses_damaged_foreign_and_newer_backups_and_can_be_cancelled() {
    let (lib, root) = library_with_data();
    let out = dest(&lib, "backups-out");
    let set = PathBuf::from(lib.call("create_backup", json!({ "destination": out }))["path"].as_str().unwrap());

    // โฟลเดอร์ที่ไม่ใช่ชุดสำรอง
    let err = lib.try_call("preview_restore", json!({ "path": root })).unwrap_err();
    assert_eq!(err.code, "INVALID_INPUT");
    // ไฟล์ฐานข้อมูลถูกแก้หลังสำรอง
    let damaged = dest(&lib, "damaged");
    for name in ["library.sqlite", "manifest.json"] {
        std::fs::copy(set.join(name), damaged.join(name)).unwrap();
    }
    let mut bytes = std::fs::read(damaged.join("library.sqlite")).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(damaged.join("library.sqlite"), bytes).unwrap();
    assert_eq!(lib.try_call("preview_restore", json!({ "path": damaged })).unwrap_err().code, "DATABASE_CORRUPT");
    assert_eq!(lib.try_call("stage_restore", json!({ "path": damaged })).unwrap_err().code, "DATABASE_CORRUPT");
    // ชุดจากแอปรุ่นใหม่กว่า (schema สูงกว่าที่รุ่นนี้รู้จัก)
    let newer = dest(&lib, "newer");
    std::fs::copy(set.join("library.sqlite"), newer.join("library.sqlite")).unwrap();
    rusqlite::Connection::open(newer.join("library.sqlite")).unwrap().pragma_update(None, "user_version", 999).unwrap();
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(set.join("manifest.json")).unwrap()).unwrap();
    manifest["sha256"] = json!(puri_core::util::hex(&{
        use sha2::Digest;
        sha2::Sha256::digest(std::fs::read(newer.join("library.sqlite")).unwrap())
    }));
    std::fs::write(newer.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(lib.try_call("preview_restore", json!({ "path": newer })).unwrap_err().code, "LIBRARY_TOO_NEW");
    assert!(lib.call("get_backup_status", json!({}))["pendingRestore"].is_null(), "ชุดที่ไม่ผ่านไม่ถูกวางรอ");

    // ปลายทางสำรองในโฟลเดอร์รูปที่คลังติดตาม: ไม่เขียน
    assert_eq!(lib.try_call("create_backup", json!({ "destination": root })).unwrap_err().code, "INVALID_INPUT");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2, "ไม่มีอะไรถูกเขียนลงโฟลเดอร์รูป");

    // วางรอแล้วยกเลิก: เปิดใหม่ได้คลังเดิม
    lib.label("IMG_0002.jpg", 0.6, ("name", "คุณเมย์"));
    lib.call("stage_restore", json!({ "path": set }));
    assert!(lib.call("cancel_restore", json!({}))["pendingRestore"].is_null());
    let lib = lib.reopen();
    assert_eq!(people(&lib).len(), 2);
}

#[test]
fn automatic_backup_runs_once_a_day_and_keeps_the_latest_sets() {
    let empty = Lib::new();
    assert!(puri_core::backup::auto_backup(&empty.app.db_path).unwrap().is_none(), "คลังว่างไม่ต้องสำรอง");

    let (lib, _root) = library_with_data();
    let first = puri_core::backup::auto_backup(&lib.app.db_path).unwrap().expect("สำรองครั้งแรก");
    assert_eq!((first.manifest.kind.as_str(), first.manifest.counts.persons), ("auto", 1));
    assert!(puri_core::backup::auto_backup(&lib.app.db_path).unwrap().is_none(), "ยังไม่ครบวัน ไม่สำรองซ้ำ");
    let status = lib.call("get_backup_status", json!({}));
    assert_eq!(arr(&status["automatic"]).len(), 1);
    assert!(Path::new(status["backupsDir"].as_str().unwrap()).ends_with("data/backups"));
}
