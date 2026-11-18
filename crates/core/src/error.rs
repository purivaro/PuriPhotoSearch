use serde::Serialize;

/// ข้อผิดพลาดที่ส่งถึง UI: รหัสคงที่ + ข้อความภาษาไทยพร้อมแสดง (Blueprint 12.3)
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{code}: {message}")]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<&'static str>,
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), retryable: false, affected_ids: None, next_action: None }
    }
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
    pub fn next(mut self, action: &'static str) -> Self {
        self.next_action = Some(action);
        self
    }
    pub fn ids(mut self, ids: Vec<String>) -> Self {
        self.affected_ids = Some(ids);
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message)
    }
    pub fn not_found(what: &str) -> Self {
        Self::new("NOT_FOUND", format!("ไม่พบ{what}นี้ในคลัง อาจถูกนำออกไปแล้ว"))
    }
    pub fn version_conflict() -> Self {
        Self::new("VERSION_CONFLICT", "ข้อมูลนี้เปลี่ยนหลังจากที่เปิดหน้านี้ กรุณาตรวจข้อมูลล่าสุดก่อนบันทึก")
    }
    pub fn source_unavailable() -> Self {
        Self::new("SOURCE_UNAVAILABLE", "โฟลเดอร์ต้นทางไม่พร้อมใช้งาน เชื่อมต่อไดรฟ์หรือเลือกตำแหน่งใหม่").retryable().next("relink")
    }
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::new("INTERNAL", format!("เกิดข้อผิดพลาดภายในแอป ({detail})")).retryable()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode as C;
        if let rusqlite::Error::SqliteFailure(f, _) = &e {
            match f.code {
                C::DiskFull => return Self::new("DISK_FULL", "พื้นที่ดิสก์ไม่พอสำหรับบันทึกข้อมูลของแอป").next("free_space"),
                C::DatabaseCorrupt | C::NotADatabase => {
                    return Self::new("DATABASE_CORRUPT", "ไฟล์คลังข้อมูลเสียหายหรือไม่ใช่คลังของ Puri Photo Search")
                }
                _ => {}
            }
        }
        Self::internal(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        match e.kind() {
            K::NotFound => Self::source_unavailable(),
            K::PermissionDenied => {
                Self::new("ACCESS_DENIED", "แอปไม่มีสิทธิ์เข้าถึงตำแหน่งนี้ กรุณาอนุญาตหรือเลือกโฟลเดอร์ใหม่").next("grant_access")
            }
            K::StorageFull => Self::new("DISK_FULL", "พื้นที่ดิสก์ปลายทางไม่พอ").next("free_space"),
            _ => Self::internal(e),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::invalid(format!("รูปแบบคำสั่งไม่ถูกต้อง ({e})"))
    }
}
