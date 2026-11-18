//! ONNX Runtime ที่ `ort` ดาวน์โหลดมาเป็น static library มีโค้ด CoreML (Objective-C++) ที่เรียก
//! `__isPlatformVersionAtLeast` จาก runtime ของ clang; rustc ลิงก์ด้วย `-nodefaultlibs` จึงต้องบอกให้ลิงก์เอง
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let output = Command::new("clang").arg("--print-file-name=libclang_rt.osx.a").output();
    let Ok(output) = output else { return };
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Some(dir) = std::path::Path::new(&path).parent().filter(|d| d.is_dir()) {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
}
