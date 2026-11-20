//! หา CLI ของ AI ที่ผู้ใช้ติดตั้งและล็อกอินไว้แล้ว (Claude Code, Codex ของ ChatGPT, Gemini CLI)
//!
//! แอปไม่ขอ ไม่อ่าน และไม่เก็บรหัสผ่าน API key หรือ token ใด ๆ — ใช้บัญชีที่ล็อกอินใน CLI นั้นอยู่แล้ว
//! (แนวทางเดียวกับ PuriCat: AI_INTEGRATION.md) · ทุกคำสั่งรันแบบไม่ผ่าน shell, ปิด stdin, หมดเวลา 8 วินาที

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Gemini];

    pub fn binary(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Gemini => "gemini",
        }
    }

    /// คำสั่งล็อกอินที่ผู้ใช้พิมพ์เองใน Terminal
    pub fn login_command(self) -> &'static str {
        match self {
            Provider::Claude => "claude auth login",
            Provider::Codex => "codex login",
            Provider::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub provider: Provider,
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// None = ไม่รู้ (CLI ไม่มีคำสั่งบอกสถานะ หรือตอบแปลก)
    pub signed_in: Option<bool>,
    pub login_command: String,
    /// สร้างภาพได้ผ่านบัญชีนี้ (Codex ของ ChatGPT มีเครื่องมือสร้างภาพในตัว)
    pub can_generate_images: bool,
}

pub fn detect_all() -> Vec<CliStatus> {
    let handles: Vec<_> = Provider::ALL.into_iter().map(|p| std::thread::spawn(move || detect(p))).collect();
    handles.into_iter().zip(Provider::ALL).map(|(h, p)| h.join().unwrap_or_else(|_| missing(p))).collect()
}

fn missing(provider: Provider) -> CliStatus {
    CliStatus {
        provider,
        installed: false,
        path: None,
        version: None,
        signed_in: None,
        login_command: provider.login_command().into(),
        can_generate_images: false,
    }
}

pub fn detect(provider: Provider) -> CliStatus {
    let Some(path) = locate(provider) else { return missing(provider) };
    let version = run(&path, &["--version"], PROBE_TIMEOUT)
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.trim().to_string()))
        .filter(|v| !v.is_empty());
    let signed_in = match provider {
        Provider::Claude => run(&path, &["auth", "status", "--json"], PROBE_TIMEOUT)
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()?.get("loggedIn")?.as_bool()),
        Provider::Codex => run(&path, &["login", "status"], PROBE_TIMEOUT).map(|o| {
            let text =
                format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).to_lowercase();
            o.status.success() && text.contains("logged in") && !text.contains("not logged in")
        }),
        // Gemini CLI ไม่มีคำสั่งบอกสถานะ: ไม่มีการตั้งค่าล็อกอินเลย = ยังไม่ล็อกอิน · มี = ไม่รู้จนกว่าจะลองคุย
        Provider::Gemini => {
            let home = home_dir();
            let configured = home.as_ref().is_some_and(|h| {
                std::fs::read_to_string(h.join(".gemini/settings.json")).is_ok_and(|s| s.contains("selectedType"))
            });
            if configured {
                None
            } else {
                Some(false)
            }
        }
    };
    CliStatus {
        provider,
        installed: true,
        path: Some(path.to_string_lossy().into_owned()),
        version,
        signed_in,
        login_command: provider.login_command().into(),
        can_generate_images: provider == Provider::Codex,
    }
}

/// ตำแหน่งของ CLI — Codex มักมีหลายตัว (npm และที่มากับแอป ChatGPT) จึงเลือกตัวที่รุ่นใหม่ที่สุด
/// (ตัวเก่าอาจคุยกับบริการไม่ได้แล้ว: codex-cli 0.133 ถูกปฏิเสธทุกรุ่นโมเดลเมื่อ 21 พ.ย. 2569)
pub fn locate(provider: Provider) -> Option<PathBuf> {
    let mut found = candidates(provider.binary());
    found.dedup();
    match provider {
        Provider::Codex if found.len() > 1 => found.into_iter().max_by_key(|p| {
            run(p, &["--version"], PROBE_TIMEOUT)
                .map(|o| version_key(&String::from_utf8_lossy(&o.stdout)))
                .unwrap_or_default()
        }),
        _ => found.into_iter().next(),
    }
}

/// "codex-cli 0.171.0-alpha.13" → [0, 171, 0] สำหรับเทียบรุ่น
fn version_key(text: &str) -> Vec<u64> {
    text.split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|w| w.split(['.', '-']).take(3).map(|n| n.parse().unwrap_or(0)).collect())
        .unwrap_or_default()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

fn executable_names(binary: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![format!("{binary}.exe"), format!("{binary}.cmd")]
    } else {
        vec![binary.to_string()]
    }
}

fn candidates(binary: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = home_dir() {
        for sub in
            [".local/bin", ".claude/local", ".npm-global/bin", ".bun/bin", ".volta/bin", "bin", "AppData/Roaming/npm"]
        {
            dirs.push(home.join(sub));
        }
        // nvm: ทุกรุ่นของ node
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            dirs.extend(entries.flatten().map(|e| e.path().join("bin")));
        }
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let mut found: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|d| executable_names(binary).into_iter().map(move |n| d.join(n)))
        .filter(|p| p.is_file())
        .collect();
    if binary == "codex" && cfg!(target_os = "macos") {
        // Codex ที่มากับแอป ChatGPT / Codex บน Mac (มักใหม่กว่าตัวที่ติดตั้งผ่าน npm)
        for app in ["/Applications/ChatGPT.app", "/Applications/Codex.app"] {
            let p = Path::new(app).join("Contents/Resources/codex-cli/bin/codex");
            if p.is_file() {
                found.push(p);
            }
        }
    }
    if found.is_empty() {
        found.extend(locate_via_login_shell(binary));
    }
    found
}

/// ทางสุดท้ายสำหรับตัวจัดการรุ่น (asdf, mise ฯลฯ): ถาม login shell ของผู้ใช้
#[cfg(unix)]
fn locate_via_login_shell(binary: &str) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/'))?;
    let output = run(Path::new(&shell), &["-lic", &format!("command -v {binary}")], PROBE_TIMEOUT)?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with('/'))
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

#[cfg(not(unix))]
fn locate_via_login_shell(_binary: &str) -> Option<PathBuf> {
    None
}

/// คำสั่งที่รัน CLI: บน Windows ไฟล์ .cmd ต้องรันผ่าน cmd
pub(crate) fn command(path: &Path) -> Command {
    let mut cmd = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd")) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(path);
        c
    } else {
        Command::new(path)
    };
    // CLI ที่เขียนด้วย Node ต้องหา node เจอแม้แอปถูกเปิดจาก Finder (PATH สั้น)
    if let Some(dir) = path.parent() {
        let mut paths = vec![dir.to_path_buf()];
        if let Some(old) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&old));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
    }
    cmd
}

/// รันคำสั่งสั้น ๆ แบบมีเวลาจำกัด — หมดเวลา = None (และฆ่า process)
pub(crate) fn run(path: &Path, args: &[&str], timeout: Duration) -> Option<Output> {
    let mut child =
        command(path).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(40)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(version_key("codex-cli 0.171.0-alpha.13"), vec![0, 171, 0]);
        assert!(version_key("codex-cli 0.171.0-alpha.13") > version_key("codex-cli 0.133.0"));
        assert_eq!(version_key("2.1.303 (Claude Code)"), vec![2, 1, 303]);
    }
}
