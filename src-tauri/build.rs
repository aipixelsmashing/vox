//! Build script: Tauri codegen, plus the Swift bridge for Apple SpeechAnalyzer.
//!
//! SpeechAnalyzer is Swift-only (no C or Objective-C surface), so the engine is a small Swift
//! file compiled here with `swiftc` into a static library and linked into the binary. The same
//! shape `axuielement` uses for its own bridge. See docs/TECH-STACK.md#speech-recognition.
use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    build_swift_bridge();
}

#[cfg(target_os = "macos")]
fn build_swift_bridge() {
    println!("cargo:rerun-if-changed=swift/SpeechAnalyzerBridge.swift");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    let out = env::var("OUT_DIR").unwrap();
    let lib = format!("{out}/libVoxSpeechBridge.a");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let triple = match arch.as_str() {
        "aarch64" => "arm64-apple-macos15.0",
        "x86_64" => "x86_64-apple-macos15.0",
        other => panic!("unsupported macOS arch {other}"),
    };
    // Deployment target 15.0 on purpose: the binary must load on an older macOS and report
    // the engine unavailable, so every SpeechAnalyzer use sits behind #available(macOS 26).
    let status = Command::new("swiftc")
        .args([
            "-parse-as-library",
            "-emit-library",
            "-static",
            "-O",
            "-swift-version",
            "5",
            "-target",
            triple,
            "-module-name",
            "VoxSpeechBridge",
            "-o",
            &lib,
            "swift/SpeechAnalyzerBridge.swift",
        ])
        .status()
        .expect("swiftc not found: install the Xcode Command Line Tools");
    assert!(
        status.success(),
        "swiftc failed building the SpeechAnalyzer bridge"
    );

    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=VoxSpeechBridge");
    for fw in [
        "Foundation",
        "AVFoundation",
        "CoreMedia",
        "Speech",
        "UserNotifications",
        "Carbon",
        "IOKit",
    ] {
        println!("cargo:rustc-link-lib=framework={fw}");
    }
    // Swift runtime dylibs live in /usr/lib/swift on every supported macOS. The static
    // compatibility shims live in the toolchain, whose layout differs between Xcode.app and
    // the Command Line Tools; axuielement's own build script only knows the former, so this
    // also fixes its link (docs/spikes/s2-injection.md).
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    if let Ok(o) = Command::new("xcode-select").arg("-p").output() {
        let dev = String::from_utf8_lossy(&o.stdout).trim().to_string();
        for sub in [
            "usr/lib/swift/macosx",
            "Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx",
        ] {
            let d = format!("{dev}/{sub}");
            if Path::new(&d).is_dir() {
                println!("cargo:rustc-link-search=native={d}");
            }
        }
    }
    for lib in [
        "swiftCore",
        "swiftFoundation",
        "swift_Concurrency",
        "swiftDispatch",
        "swiftAVFoundation",
        "swiftCoreMedia",
        "swiftObjectiveC",
        "swiftDarwin",
    ] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}
