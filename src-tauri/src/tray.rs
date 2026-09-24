//! Tray icon and menu. Four states, distinguishable by silhouette and not colour alone, plus
//! a distinct always-listening variant when pre-roll is enabled — an open microphone must
//! never be invisible. See docs/UI-SPEC.md.
//!
//! M1 menu: Pause dictation · Quit Vox. History and Settings come with their windows in M3.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

pub const TRAY_ID: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Idle,
    Recording,
    Transcribing,
    Attention,
    AlwaysListening,
}

fn icon(state: IconState) -> Image<'static> {
    let bytes: &'static [u8] = match state {
        IconState::Idle | IconState::AlwaysListening => include_bytes!("../icons/tray-idle.png"),
        IconState::Recording => include_bytes!("../icons/tray-recording.png"),
        IconState::Transcribing => include_bytes!("../icons/tray-transcribing.png"),
        IconState::Attention => include_bytes!("../icons/tray-attention.png"),
    };
    Image::from_bytes(bytes).expect("tray icons are valid PNGs at build time")
}

pub fn install(app: &tauri::App, paused: Arc<AtomicBool>) -> anyhow::Result<()> {
    let pause = CheckMenuItem::with_id(app, "pause", "Pause dictation", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Vox", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&pause, &PredefinedMenuItem::separator(app)?, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(IconState::Idle))
        .icon_as_template(true)
        .tooltip("Vox — hold right Option and speak")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "pause" => {
                let now = !paused.load(Ordering::Relaxed);
                paused.store(now, Ordering::Relaxed);
                let _ = pause.set_checked(now);
                set_state(
                    app,
                    if now {
                        IconState::Attention
                    } else {
                        IconState::Idle
                    },
                );
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// The icon must change within one frame of key-down regardless of pipeline state: feedback
/// that lags makes a fast system feel slow.
pub fn set_state(app: &tauri::AppHandle, state: IconState) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_icon(Some(icon(state)));
        let _ = tray.set_icon_as_template(true);
    }
}

/// A second key-down while busy: show the transcribing icon briefly rather than queue.
pub fn flash_busy(app: &tauri::AppHandle) {
    set_state(app, IconState::Transcribing);
}

pub fn show_history(_app: &tauri::AppHandle) {
    // M3: create the panel window lazily, anchored near the tray icon.
}
