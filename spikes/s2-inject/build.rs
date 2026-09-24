//! Workaround for axuielement 0.9's build script, which assumes `xcode-select -p` points at
//! Xcode.app and adds `<path>/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx` to the
//! link search path. With only Command Line Tools installed the Swift compatibility
//! libraries live at `<path>/usr/lib/swift/macosx` instead, and the link fails with
//! `__swift_FORCE_LOAD_$_swiftCompatibility56` undefined. Same fix will be needed in
//! src-tauri/ (see docs/spikes/s2-injection.md).
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    let Ok(out) = Command::new("xcode-select").arg("-p").output() else {
        return;
    };
    let dev = String::from_utf8_lossy(&out.stdout).trim().to_string();
    for sub in ["usr/lib/swift/macosx", "usr/lib/swift-5.5/macosx"] {
        let dir = format!("{dev}/{sub}");
        if Path::new(&dir).is_dir() {
            println!("cargo:rustc-link-search=native={dir}");
        }
    }
}
