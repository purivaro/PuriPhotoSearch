//! ตัวช่วยของ integration tests: คลังชั่วคราว + โฟลเดอร์รูปที่สร้างขึ้นจริงบนดิสก์
#![allow(dead_code)]

use puri_core::ai::{DetectedFace, FaceAnalyzer, FaceEngineProvider, ModelFile, ModelSpec};
use puri_core::faces::BBox;
use puri_core::{api, App, AppError, NoopSink, Options};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

pub struct Lib {
    pub tmp: TempDir,
    pub app: Arc<App>,
    ai: bool,
    subject: bool,
}

/// ตัวแยกตัวคนจำลองสำหรับเทสต์เท่านั้น: "ตัวคน" คือครึ่งขวาของภาพ
pub struct FakeSegmenter;

impl puri_core::subject::Segmenter for FakeSegmenter {
    fn segment(&self, image: &image::RgbImage) -> Result<image::GrayImage, AppError> {
        let w = image.width();
        // ภาพที่มีตัวคนสีแดงสด (เทสต์ตัวผอม): ตัวคนคือส่วนที่เป็นสีแดง · ภาพอื่น: ครึ่งขวาของภาพ
        let person = |p: &image::Rgb<u8>| p[0] > 170 && p[1] < 90 && p[2] < 90;
        if image.pixels().filter(|p| person(p)).count() > (image.len() / 3) / 50 {
            return Ok(image::GrayImage::from_fn(w, image.height(), |x, y| {
                image::Luma([if person(image.get_pixel(x, y)) { 255 } else { 0 }])
            }));
        }
        Ok(image::GrayImage::from_fn(w, image.height(), |x, _| image::Luma([if x >= w / 2 { 255 } else { 0 }])))
    }
}

pub const FAKE_SUBJECT_MODEL: &[u8] = b"fake subject weights";

/// ตัวแยกตัวคนจำลอง: "ดาวน์โหลด" ไฟล์เดียวจากโฟลเดอร์ชั่วคราวผ่าน file:// ด้วยงานติดตั้งตัวจริง
fn fake_subject(base: &Path) -> puri_core::subject::SubjectProvider {
    puri_core::subject::SubjectProvider {
        model: puri_core::subject::SubjectModel {
            id: "fake-subject",
            display_name: "ตัวแยกทดสอบ",
            file: "subject.onnx",
            sha256: sha(FAKE_SUBJECT_MODEL),
            download_url: file_url(&base.join("model-source/subject.onnx")),
            download_bytes: 1_000_000,
            download_source: "โฟลเดอร์ทดสอบ",
            license_note: "ใช้ในเทสต์เท่านั้น",
        },
        factory: Arc::new(|_| Ok(Arc::new(FakeSegmenter) as Arc<dyn puri_core::subject::Segmenter>)),
        fill: Some(puri_core::subject::FillProvider {
            model: puri_core::subject::SubjectModel {
                id: "fake-fill",
                display_name: "ตัวเติมทดสอบ",
                file: "fill.onnx",
                sha256: sha(FAKE_FILL_MODEL),
                download_url: file_url(&base.join("model-source/fill.onnx")),
                download_bytes: 1_000_000,
                download_source: "โฟลเดอร์ทดสอบ",
                license_note: "ใช้ในเทสต์เท่านั้น",
            },
            factory: Arc::new(|_| Ok(Arc::new(FakeInpainter) as Arc<dyn puri_core::subject::Inpainter>)),
        }),
        parts: Some(puri_core::parts::PartsProvider {
            model: puri_core::subject::SubjectModel {
                id: "fake-parts",
                display_name: "ตัวแยกส่วนทดสอบ",
                file: "parts.onnx",
                sha256: sha(FAKE_PARTS_MODEL),
                download_url: file_url(&base.join("model-source/parts.onnx")),
                download_bytes: 1_000_000,
                download_source: "โฟลเดอร์ทดสอบ",
                license_note: "ใช้ในเทสต์เท่านั้น",
            },
            factory: Arc::new(|_| Ok(Arc::new(FakeParts) as Arc<dyn puri_core::parts::PartSegmenter>)),
        }),
        sky: Some(puri_core::sky::SkyProvider {
            model: puri_core::subject::SubjectModel {
                id: "fake-sky",
                display_name: "ตัวแยกท้องฟ้าทดสอบ",
                file: "sky.onnx",
                sha256: sha(FAKE_SKY_MODEL),
                download_url: file_url(&base.join("model-source/sky.onnx")),
                download_bytes: 1_000_000,
                download_source: "โฟลเดอร์ทดสอบ",
                license_note: "ใช้ในเทสต์เท่านั้น",
            },
            factory: Arc::new(|_| Ok(Arc::new(FakeSky) as Arc<dyn puri_core::sky::SkySegmenter>)),
        }),
    }
}

pub const FAKE_SKY_MODEL: &[u8] = b"fake sky weights";

/// ตัวแยกท้องฟ้าจำลอง: ครึ่งบนของภาพ = ท้องฟ้า (มั่นใจเต็มที่)
pub struct FakeSky;

impl puri_core::sky::SkySegmenter for FakeSky {
    fn segment(&self, image: &image::RgbImage) -> Result<image::GrayImage, AppError> {
        let (w, h) = (32, 32);
        let _ = image;
        Ok(image::GrayImage::from_fn(w, h, |_, y| image::Luma([if y < h / 2 { 255 } else { 0 }])))
    }
}

pub const FAKE_PARTS_MODEL: &[u8] = b"fake parts weights";

/// ตัวแยกส่วนจำลอง: ครึ่งบนของภาพ = ผม · ครึ่งล่าง = เสื้อผ้า (ผิวไม่มี)
pub struct FakeParts;

impl puri_core::parts::PartSegmenter for FakeParts {
    fn segment(&self, image: &image::RgbImage) -> Result<[image::GrayImage; 4], AppError> {
        let (w, h) = image.dimensions();
        let half =
            |top: bool| image::GrayImage::from_fn(w, h, |_, y| image::Luma([if (y < h / 2) == top { 255 } else { 0 }]));
        Ok([half(true), image::GrayImage::new(w, h), image::GrayImage::new(w, h), half(false)])
    }
}

pub const FAKE_FILL_MODEL: &[u8] = b"fake fill weights";

/// ตัวเติมจำลอง: ช่องเป็นสีฟ้าล้วน (40, 120, 200)
pub struct FakeInpainter;

impl puri_core::subject::Inpainter for FakeInpainter {
    fn side(&self) -> u32 {
        64
    }

    fn inpaint(&self, image: &image::RgbImage, holes: &image::GrayImage) -> Result<image::RgbImage, AppError> {
        Ok(image::RgbImage::from_fn(64, 64, |x, y| {
            if holes.get_pixel(x, y)[0] > 127 {
                image::Rgb([40, 120, 200])
            } else {
                *image.get_pixel(x, y)
            }
        }))
    }
}

/// ตัววิเคราะห์จำลองสำหรับเทสต์เท่านั้น (ไม่มีในแอป): "บุคคล" ของรูปคือค่าสีแดงของภาพ ÷ 20
/// รูปที่สีแดงใกล้ 0 = ไม่มีใบหน้า; ค่าสีเดียวกัน = เวกเตอร์เดียวกัน; ต่างค่า = ตั้งฉากกัน
/// ค่าสีแดงที่เกินหลักยี่สิบขึ้นไป 4 หรือมากกว่า (เช่น 47) = คนเดิมแต่ตัวตรวจไม่มั่นใจ (คะแนน 0.55)
pub struct FakeAnalyzer;

fn fake_face(image_bytes: &[u8]) -> Option<(Vec<f32>, f32)> {
    // RAW: ใช้ตัวล้างภาพของแอป (ภาพตั้งตรงแล้ว) · ไฟล์อื่นอ่านตรง ๆ เหมือนเดิม
    let img = if puri_core::raw::is_raw_bytes(image_bytes) {
        puri_core::media::decode_upright(image_bytes, 512).ok()?.to_rgb8()
    } else {
        image::load_from_memory(image_bytes).ok()?.to_rgb8()
    };
    let red = img.get_pixel(img.width() / 2, img.height() / 2)[0] as f32;
    let person = (red / 20.0).round() as usize;
    if person == 0 {
        return None;
    }
    let mut v = vec![0.0f32; 512];
    v[person % 512] = 3.0; // ไม่ normalize เหมือนเวกเตอร์ดิบของโมเดลจริง
    Some((v, if red - person as f32 * 20.0 >= 4.0 { 0.55 } else { 0.9 }))
}

impl FaceAnalyzer for FakeAnalyzer {
    fn analyze(&self, image_bytes: &[u8]) -> Result<Vec<DetectedFace>, AppError> {
        Ok(fake_face(image_bytes)
            .map(|(embedding, score)| DetectedFace { bbox: BBox { x: 0.3, y: 0.2, w: 0.25, h: 0.4 }, score, embedding })
            .into_iter()
            .collect())
    }
    fn embed_crop(&self, image_bytes: &[u8]) -> Result<Option<Vec<f32>>, AppError> {
        Ok(fake_face(image_bytes).map(|f| f.0))
    }
}

pub const FAKE_MODEL_FILES: [(&str, &[u8]); 2] =
    [("det.onnx", b"fake detector weights"), ("rec.onnx", b"fake recognizer weights")];

fn sha(bytes: &[u8]) -> &'static str {
    Box::leak(puri_core::util::hex(&Sha256::digest(bytes)).into_boxed_str())
}

/// `file://` URL ที่ curl รับ: Windows ต้องเป็น `file:///C:/…`
fn file_url(path: &Path) -> String {
    let p = path.display().to_string().replace('\\', "/");
    if p.starts_with('/') {
        format!("file://{p}")
    } else {
        format!("file:///{p}")
    }
}

/// ชุดโมเดลจำลอง: "ดาวน์โหลด" จากไฟล์ zip ในโฟลเดอร์ชั่วคราวผ่าน file:// ด้วย pipeline ติดตั้งตัวจริง
fn fake_provider(base: &Path) -> FaceEngineProvider {
    let files: &'static [ModelFile] = Box::leak(Box::new([
        ModelFile { name: FAKE_MODEL_FILES[0].0, sha256: sha(FAKE_MODEL_FILES[0].1) },
        ModelFile { name: FAKE_MODEL_FILES[1].0, sha256: sha(FAKE_MODEL_FILES[1].1) },
    ]));
    FaceEngineProvider {
        spec: ModelSpec {
            id: "fake-pack",
            display_name: "ชุดโมเดลทดสอบ",
            space: "fake-space-v1",
            files,
            download_url: file_url(&base.join("model-source/pack.zip")),
            download_bytes: 2_000_000,
            download_source: "โฟลเดอร์ทดสอบ",
            license_note: "ใช้ในเทสต์เท่านั้น",
        },
        factory: Arc::new(|_| Ok(Arc::new(FakeAnalyzer) as Arc<dyn FaceAnalyzer>)),
    }
}

/// สร้างไฟล์ zip ของชุดโมเดลจำลองไว้ให้งานติดตั้ง "ดาวน์โหลด"; `corrupt` = เนื้อไฟล์ไม่ตรง checksum
pub fn publish_fake_model(base: &Path, corrupt: bool) {
    let dir = base.join("model-source");
    std::fs::create_dir_all(&dir).unwrap();
    let _ = std::fs::remove_file(dir.join("pack.zip"));
    for (name, bytes) in FAKE_MODEL_FILES {
        std::fs::write(dir.join(name), if corrupt { b"tampered".as_slice() } else { bytes }).unwrap();
    }
    std::fs::write(dir.join("unrelated.onnx"), b"not needed by the app").unwrap();
    // Windows ไม่มี `zip`: bsdtar ใน System32 สร้าง zip ได้ (`-a` เลือกรูปแบบจากนามสกุล)
    #[cfg(not(windows))]
    let mut zip = {
        let mut c = std::process::Command::new("zip");
        c.args(["-q", "pack.zip"]);
        c
    };
    #[cfg(windows)]
    let mut zip = {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let mut c = std::process::Command::new(Path::new(&root).join("System32").join("tar.exe"));
        c.args(["-a", "-cf", "pack.zip"]);
        c
    };
    let ok = zip.current_dir(&dir).args(["det.onnx", "rec.onnx", "unrelated.onnx"]).status().unwrap().success();
    assert!(ok, "zip");
}

fn open(base: &Path, ai: bool, subject: bool) -> Arc<App> {
    std::env::set_var("PURI_DISABLE_OPEN", "1");
    // เทสต์รันขนานใน process เดียว: ช่วงที่เทสต์อื่นกำลัง fork ตัว zip/curl/unzip ลูกที่ยังไม่ exec ถือสำเนา fd ของ
    // library.lock อยู่ชั่วครู่ การเปิดคลังซ้ำทันที (`reopen`) จึงเจอ LIBRARY_LOCKED ได้เป็นบางครั้ง
    // lock หลุดเองเมื่อลูก exec (fd เป็น close-on-exec) — รอแล้วลองใหม่ ข้อผิดพลาดอื่นถือว่าเทสต์ล้ม
    let started = std::time::Instant::now();
    loop {
        let opened = App::open(Options {
            data_dir: base.join("data"),
            cache_dir: base.join("cache"),
            sink: Arc::new(NoopSink),
            folder_picker: "dev_path_input",
            start_worker: false,
            face_engine: ai.then(|| fake_provider(base)),
            face_engine_unavailable: "build ทดสอบนี้ไม่มีตัววิเคราะห์ใบหน้า",
            subject_engine: subject.then(|| fake_subject(base)),
        });
        match opened {
            Ok(app) => return app,
            Err(e) if e.code == "LIBRARY_LOCKED" && started.elapsed() < std::time::Duration::from_secs(5) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => panic!("open library: {e:?}"),
        }
    }
}

impl Lib {
    pub fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let app = open(tmp.path(), false, false);
        Lib { tmp, app, ai: false, subject: false }
    }

    /// คลังที่ build มีตัวแยกตัวคน (ยังไม่ติดตั้งจนกว่าจะเรียก `install_subject_model`) — `published` = มีไฟล์ให้ดาวน์โหลด
    pub fn with_subject(published: bool) -> Self {
        let tmp = TempDir::new().unwrap();
        if published {
            let dir = tmp.path().join("model-source");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("subject.onnx"), FAKE_SUBJECT_MODEL).unwrap();
            std::fs::write(dir.join("fill.onnx"), FAKE_FILL_MODEL).unwrap();
            std::fs::write(dir.join("parts.onnx"), FAKE_PARTS_MODEL).unwrap();
            std::fs::write(dir.join("sky.onnx"), FAKE_SKY_MODEL).unwrap();
        }
        let app = open(tmp.path(), false, true);
        Lib { tmp, app, ai: false, subject: true }
    }

    pub fn install_subject_model(&self) -> Value {
        let job = self.call("install_subject_model", json!({}));
        self.run_jobs();
        self.call("get_job", json!({ "jobId": job["id"] }))
    }

    /// คลังที่ build มีตัววิเคราะห์ใบหน้า (ยังไม่ติดตั้งโมเดลจนกว่าจะเรียก `install_model`)
    pub fn with_ai() -> Self {
        let tmp = TempDir::new().unwrap();
        publish_fake_model(tmp.path(), false);
        let app = open(tmp.path(), true, false);
        Lib { tmp, app, ai: true, subject: false }
    }

    pub fn install_model(&self) -> Value {
        let job = self.call("install_face_model", json!({}));
        self.run_jobs();
        self.call("get_job", json!({ "jobId": job["id"] }))
    }

    /// ปิดแล้วเปิดคลังเดิมใหม่ (จำลอง restart)
    pub fn reopen(self) -> Self {
        let Lib { tmp, app, ai, subject } = self;
        app.shutdown();
        drop(app);
        let app = open(tmp.path(), ai, subject);
        Lib { tmp, app, ai, subject }
    }

    /// โฟลเดอร์สำหรับวางรูปทดสอบ (canonical path เหมือนที่แอปเห็น)
    pub fn photos(&self) -> PathBuf {
        let p = self.tmp.path().join("Photos");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::canonicalize(p).unwrap()
    }

    pub fn try_call(&self, cmd: &str, args: Value) -> Result<Value, AppError> {
        api::dispatch(&self.app, cmd, args)
    }

    pub fn call(&self, cmd: &str, args: Value) -> Value {
        self.try_call(cmd, args).unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    pub fn run_jobs(&self) {
        self.app.run_pending_jobs().unwrap();
    }

    pub fn preview(&self, root: &Path) -> Value {
        self.call("preview_import", json!({ "path": root }))
    }

    /// commit ตาม draft ที่เลือกด้วย `pick` (ชื่อ/วันที่ใช้ค่าที่เสนอ) แล้วทำงานเบื้องหลังจนเสร็จ
    pub fn import(&self, root: &Path, mode: &str, pick: impl Fn(&Value) -> bool) -> Value {
        let preview = self.preview(root);
        let drafts = drafts_of(&preview, mode, pick);
        let result = self.call(
            "commit_import",
            json!({ "previewId": preview["previewId"], "requestId": new_id(), "mode": mode, "drafts": drafts, "analyzeFaces": true, "watch": true }),
        );
        self.run_jobs();
        result
    }

    pub fn query(&self, params: Value) -> Value {
        let mut p = json!({ "personIds": [], "personMode": "all", "eventIds": [], "faceFilter": "any", "sort": "filename", "limit": 200 });
        for (k, v) in params.as_object().unwrap() {
            p[k] = v.clone();
        }
        self.call("query_photos", p)
    }

    pub fn filenames(&self, params: Value) -> Vec<String> {
        names(&self.query(params))
    }

    pub fn photo_id(&self, filename: &str) -> String {
        let page = self.query(json!({ "text": filename }));
        let hit = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["filename"] == filename)
            .unwrap_or_else(|| panic!("no photo {filename}"));
        hit["id"].as_str().unwrap().to_string()
    }

    /// วาดกรอบใบหน้าเองแล้วตั้งชื่อ: `person` เป็น ("id", ..) หรือ ("name", ..)
    pub fn label(&self, filename: &str, x: f64, person: (&str, &str)) -> Value {
        let mut args = json!({ "requestId": new_id(), "photoId": self.photo_id(filename), "bbox": { "x": x, "y": 0.1, "w": 0.2, "h": 0.3 } });
        match person {
            ("id", id) => args["personId"] = json!(id),
            (_, name) => args["newPersonName"] = json!(name),
        }
        self.call("create_manual_face", args)
    }

    pub fn person_id(&self, name: &str) -> String {
        let people = self.call("list_people", json!({ "text": name }));
        people
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["displayName"] == name)
            .unwrap_or_else(|| panic!("no person {name}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub fn count(&self, sql: &str) -> i64 {
        let conn = rusqlite::Connection::open(self.tmp.path().join("data/library.sqlite")).unwrap();
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

pub fn drafts_of(preview: &Value, mode: &str, pick: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut all: Vec<Value> = vec![];
    if mode == "single_event" {
        all.push(preview["single"].clone());
    } else {
        all.extend(preview["children"].as_array().unwrap().iter().cloned());
        if !preview["rootLoose"].is_null() {
            all.push(preview["rootLoose"].clone());
        }
    }
    all.iter()
        .filter(|d| pick(d))
        .map(|d| json!({ "draftId": d["draftId"], "title": d["suggestedTitle"], "eventDate": d["suggestedDate"] }))
        .collect()
}

pub fn names(page: &Value) -> Vec<String> {
    page["items"].as_array().unwrap().iter().map(|i| i["filename"].as_str().unwrap().to_string()).collect()
}

pub fn new_id() -> String {
    puri_core::util::new_id()
}

pub fn jpeg_bytes(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([seed, (x % 251) as u8, (y % 241) as u8]));
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img).unwrap();
    out
}

pub fn write_jpeg(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, jpeg_bytes(64, 48, seed)).unwrap();
}

pub fn write_png(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(40, 30, image::Rgb([10, 200, 30]))
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

/// JPEG ที่มี EXIF Orientation และ DateTimeOriginal (แทรก APP1 หลัง SOI)
pub fn jpeg_with_exif(w: u32, h: u32, orientation: u16, datetime: &str) -> Vec<u8> {
    add_exif(&jpeg_bytes(w, h, 77), orientation, datetime)
}

/// แทรก EXIF Orientation และ DateTimeOriginal ลงใน JPEG ที่มีอยู่
pub fn add_exif(base: &[u8], orientation: u16, datetime: &str) -> Vec<u8> {
    assert_eq!(datetime.len(), 19);
    let mut tiff: Vec<u8> = vec![];
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes());
    let entry = |tag: u16, typ: u16, count: u32, value: u32| {
        let mut e = vec![];
        e.extend_from_slice(&tag.to_le_bytes());
        e.extend_from_slice(&typ.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&value.to_le_bytes());
        e
    };
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend(entry(0x0112, 3, 1, orientation as u32));
    tiff.extend(entry(0x8769, 4, 1, 38));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(tiff.len(), 38);
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend(entry(0x9003, 2, 20, 56));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(tiff.len(), 56);
    tiff.extend_from_slice(datetime.as_bytes());
    tiff.push(0);

    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(tiff);
    let mut out = base[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend(payload);
    out.extend_from_slice(&base[2..]);
    out
}

/// hash + mtime ของทุกไฟล์ใต้โฟลเดอร์ — ใช้ยืนยันว่าแอปไม่แตะต้นฉบับ (T34)
pub fn tree_state(dir: &Path) -> BTreeMap<PathBuf, (String, std::time::SystemTime)> {
    let mut out = BTreeMap::new();
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let bytes = std::fs::read(entry.path()).unwrap();
            let sha = puri_core::util::hex(&Sha256::digest(&bytes));
            out.insert(entry.path().to_path_buf(), (sha, entry.metadata().unwrap().modified().unwrap()));
        }
    }
    out
}
