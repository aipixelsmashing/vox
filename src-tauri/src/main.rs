// Vox — local push-to-talk dictation.
//
// Entry point only. Everything interesting is in lib.rs and the modules below it;
// see ../../docs/ARCHITECTURE.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vox_lib::run()
}
