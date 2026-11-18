//! ตัวช่วยเล็ก ๆ: ID, เวลา, การเทียบข้อความ และการเดาวันที่จากชื่อโฟลเดอร์

use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime};
use unicode_normalization::UnicodeNormalization;

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default()
}

/// คีย์สำหรับค้น/เทียบข้อความ: NFC + ตัวพิมพ์เล็ก ไม่ตัดสระหรือวรรณยุกต์ไทย (Blueprint 11.4)
pub fn text_key(s: &str) -> String {
    s.trim().nfc().collect::<String>().to_lowercase()
}

/// แปลงข้อความผู้ใช้เป็น pattern ของ `LIKE ... ESCAPE '\'` แบบ substring โดย `%` `_` เป็นตัวอักษรธรรมดา
pub fn like_pattern(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('%');
    for c in text_key(s).chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// `canonicalize` ที่คืน path แบบที่ผู้ใช้และโปรแกรมอื่นของระบบรู้จัก: บน Windows std คืนรูป verbatim (`\\?\C:\…`)
/// ซึ่ง Explorer ไม่รับ และไม่ตรงกับ path ที่ตัวเลือกโฟลเดอร์ส่งมา
pub fn canonical(path: impl AsRef<Path>) -> std::io::Result<std::path::PathBuf> {
    let p = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return Ok(format!(r"\\{rest}").into());
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return Ok(rest.to_string().into());
        }
    }
    Ok(p)
}

/// คีย์เทียบตำแหน่งไฟล์: ใช้ path ตามที่ระบบไฟล์รายงาน ไม่ lowercase และไม่ normalize ทับ (Blueprint 7.4)
pub fn path_key(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn is_valid_date(s: &str) -> bool {
    parse_iso_date(s).is_some()
}

fn parse_iso_date(s: &str) -> Option<Date> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    make_date(s[0..4].parse().ok()?, s[5..7].parse().ok()?, s[8..10].parse().ok()?)
}

fn make_date(y: i32, m: u8, d: u8) -> Option<Date> {
    Date::from_calendar_date(y, Month::try_from(m).ok()?, d).ok()
}

fn iso(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

/// ปีสี่หลักเป็น ค.ศ.: ช่วง 2400–2699 ถือเป็น พ.ศ., ช่วง 1900–2199 เป็น ค.ศ., นอกนั้นไม่เดา
fn to_ce(year: i32) -> Option<i32> {
    match year {
        2400..=2699 => Some(year - 543),
        1900..=2199 => Some(year),
        _ => None,
    }
}

/// ผลการอ่านชื่อโฟลเดอร์: ชื่อกิจกรรมที่เสนอ และวันที่ (ISO ค.ศ.) เมื่อมีหลักฐานพอ
#[derive(Debug, PartialEq, Eq)]
pub struct FolderGuess {
    pub title: String,
    pub date: Option<String>,
}

/// เสนอชื่อ/วันที่จากชื่อโฟลเดอร์ (Blueprint 2.3)
/// - `2026-11-18 …` / `2569-11-18 …` / `20261118 …` / `25691118 …` → เสนอวันที่เมื่อเป็นวันจริง
/// - `691118 …` (หกหลัก) → เสนอเฉพาะเมื่อชื่อโฟลเดอร์แม่เป็นปีที่ลงท้ายตรงกัน เช่น `2569` หรือ `2026`
/// - อย่างอื่น → ไม่เสนอวันที่ ("ยังไม่ระบุวันที่")
pub fn guess_from_folder(name: &str, parent_name: Option<&str>) -> FolderGuess {
    let trimmed = name.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    let digits_at = |start: usize, n: usize| -> Option<u32> {
        if chars.len() < start + n || !chars[start..start + n].iter().all(|c| c.is_ascii_digit()) {
            return None;
        }
        chars[start..start + n].iter().collect::<String>().parse().ok()
    };
    let is_digit_at = |i: usize| chars.get(i).is_some_and(|c| c.is_ascii_digit());

    let mut found: Option<(Date, usize)> = None;
    // YYYY-MM-DD (รองรับตัวคั่น - . _)
    if let (Some(y), Some(m), Some(d)) = (digits_at(0, 4), digits_at(5, 2), digits_at(8, 2)) {
        let sep_ok = matches!(chars[4], '-' | '.' | '_') && chars[7] == chars[4] && !is_digit_at(10);
        if sep_ok {
            if let Some(date) = to_ce(y as i32).and_then(|y| make_date(y, m as u8, d as u8)) {
                found = Some((date, 10));
            }
        }
    }
    // YYYYMMDD
    if found.is_none() && !is_digit_at(8) {
        if let (Some(y), Some(m), Some(d)) = (digits_at(0, 4), digits_at(4, 2), digits_at(6, 2)) {
            if let Some(date) = to_ce(y as i32).and_then(|y| make_date(y, m as u8, d as u8)) {
                found = Some((date, 8));
            }
        }
    }
    // YYMMDD: ต้องมีปีจากโฟลเดอร์แม่ช่วยยืนยัน
    if found.is_none() && !is_digit_at(6) {
        if let (Some(yy), Some(m), Some(d)) = (digits_at(0, 2), digits_at(2, 2), digits_at(4, 2)) {
            let parent_year = parent_name.map(str::trim).filter(|p| p.len() == 4).and_then(|p| p.parse::<i32>().ok());
            if let Some(py) = parent_year.filter(|py| py.rem_euclid(100) == yy as i32) {
                if let Some(date) = to_ce(py).and_then(|y| make_date(y, m as u8, d as u8)) {
                    found = Some((date, 6));
                }
            }
        }
    }

    match found {
        Some((date, used)) => {
            let rest: String = chars[used..].iter().collect();
            let title = clean_title(&rest);
            FolderGuess { title: if title.is_empty() { trimmed.to_string() } else { title }, date: Some(iso(date)) }
        }
        None => {
            let title = clean_title(trimmed);
            FolderGuess { title: if title.is_empty() { trimmed.to_string() } else { title }, date: None }
        }
    }
}

fn clean_title(s: &str) -> String {
    let replaced: String = s.chars().map(|c| if c == '_' { ' ' } else { c }).collect();
    let trimmed = replaced.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '.' | '–' | '—'));
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(name: &str, parent: Option<&str>) -> (String, Option<String>) {
        let r = guess_from_folder(name, parent);
        (r.title, r.date)
    }

    #[test]
    fn explicit_dates_ce_and_be() {
        assert_eq!(g("2026-11-18_งานอบรม_AI", None), ("งานอบรม AI".into(), Some("2026-11-18".into())));
        assert_eq!(g("2569-11-18 งานอบรม AI", None), ("งานอบรม AI".into(), Some("2026-11-18".into())));
        assert_eq!(g("25691118 ตักบาตร", None), ("ตักบาตร".into(), Some("2026-11-18".into())));
    }

    #[test]
    fn six_digits_need_parent_year() {
        assert_eq!(g("691118 งานอบรม AI", Some("2569")), ("งานอบรม AI".into(), Some("2026-11-18".into())));
        assert_eq!(g("261118 งานอบรม", Some("2026")), ("งานอบรม".into(), Some("2026-11-18".into())));
        // ไม่มีปีจากโฟลเดอร์แม่ หรือปีไม่ตรง → ไม่เดา และคงชื่อเดิมไว้ทั้งหมด
        assert_eq!(g("691118 งานอบรม AI", None), ("691118 งานอบรม AI".into(), None));
        assert_eq!(g("691118 งานอบรม AI", Some("2568")), ("691118 งานอบรม AI".into(), None));
    }

    #[test]
    fn invalid_or_missing_dates() {
        assert_eq!(g("2026-02-30 งาน", None).1, None);
        assert_eq!(g("691345 งาน", Some("2569")).1, None);
        assert_eq!(g("กล้อง A", Some("2569")), ("กล้อง A".into(), None));
        assert_eq!(g("2569", None), ("2569".into(), None));
        assert_eq!(g("2026-11-18", None), ("2026-11-18".into(), Some("2026-11-18".into())));
    }

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("100%_a\\"), "%100\\%\\_a\\\\%");
        assert_eq!(like_pattern("  ภูริ "), "%ภูริ%");
    }
}
