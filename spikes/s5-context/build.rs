//! Compiles the Swift bridge into a static library and links it. SpeechAnalyzer is Swift-only
//! (no C or Objective-C surface, so `objc2-speech` cannot reach it); the product will need
//! this same shape. Deployment target is macOS 15 on purpose: the bridge must *run* on an
//! older OS and report "unavailable" rather than fail to load.
use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=bridge/S5Bridge.swift");
    let out = env::var("OUT_DIR").unwrap();
    let lib = format!("{out}/libS5Bridge.a");
    let status = Command::new("swiftc")
        .args([
            "-parse-as-library",
            "-emit-library",
            "-static",
            "-O",
            "-swift-version",
            "5",
            "-target",
            "arm64-apple-macos15.0",
            "-module-name",
            "S5Bridge",
            "-o",
            &lib,
            "bridge/S5Bridge.swift",
        ])
        .status()
        .expect("swiftc not found; install Xcode Command Line Tools");
    assert!(status.success(), "swiftc failed");

    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=S5Bridge");
    for fw in ["Foundation", "AVFoundation", "CoreMedia", "Speech"] {
        println!("cargo:rustc-link-lib=framework={fw}");
    }
    // Swift runtime: dylibs live in /usr/lib/swift on every macOS since 10.14.4; the static
    // compatibility shims live in the toolchain. Handle both Xcode.app and CLT layouts.
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
