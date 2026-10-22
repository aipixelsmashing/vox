//! Tray icon and menu. Four states, distinguishable by silhouette and not colour alone, plus
//! a distinct always-listening variant when pre-roll is enabled — an open microphone must
//! never be invisible. See docs/UI-SPEC.md.
//!
//! Left click opens the history panel directly, no menu: the panel carries search, the rows
//! and the Settings cog. Right click shows a native menu with only what has no home in the
//! panel: Pause dictation · Check for updates… · Quit Vox (docs/UI-SPEC.md).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::panel;

pub const TRAY_ID: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Idle,
    Recording,
    Transcribing,
    Attention,
    AlwaysListening,
}

/// The PNG for each state. Template images: macOS reads the alpha channel only and draws the
/// opaque pixels in the menu bar's own colour, so each file is black on transparent and the
/// silhouette alone carries the state (docs/UI-KIT.md). The 2x files are embedded; the menu
/// bar scales the 22 pt canvas to its 18 pt row, and a 1x bitmap would be upscaled on a
/// Retina display. Drawn by scripts/make-tray-icons.py; tests/guards.rs checks the alpha.
pub fn icon_bytes(state: IconState) -> &'static [u8] {
    match state {
        IconState::Idle => include_bytes!("../icons/tray-idle@2x.png"),
        IconState::Recording => include_bytes!("../icons/tray-recording@2x.png"),
        IconState::Transcribing => include_bytes!("../icons/tray-transcribing@2x.png"),
        IconState::Attention => include_bytes!("../icons/tray-attention@2x.png"),
        IconState::AlwaysListening => include_bytes!("../icons/tray-always-listening@2x.png"),
    }
}

fn icon(state: IconState) -> Image<'static> {
    Image::from_bytes(icon_bytes(state)).expect("tray icons are valid PNGs at build time")
}

/// The resting state: the always-listening variant whenever pre-roll keeps the microphone
/// open, because an open microphone must never be invisible (docs/UI-SPEC.md).
pub fn idle_state(settings: &crate::settings::Settings) -> IconState {
    if settings.audio.preroll == "off" {
        IconState::Idle
    } else {
        IconState::AlwaysListening
    }
}

pub fn install(
    app: &tauri::App,
    settings: Arc<parking_lot::RwLock<crate::settings::Settings>>,
    paused: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let pause = CheckMenuItem::with_id(app, "pause", "Pause dictation", true, false, None::<&str>)?;
    // Disabled until the updater is configured (M5: a real public key in tauri.conf.json).
    // Present but grey rather than absent, so the menu never silently does nothing.
    let updates = MenuItem::with_id(app, "updates", "Check for updates…", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Vox", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &pause,
            &updates,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let initial = idle_state(&settings.read());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(initial))
        .icon_as_template(true)
        .tooltip("Vox — hold right Option and speak")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                panel::remember_tray_rect(tray.app_handle(), rect);
                panel::toggle_history(tray.app_handle());
            }
            TrayIconEvent::Enter { rect, .. } | TrayIconEvent::Move { rect, .. } => {
                panel::remember_tray_rect(tray.app_handle(), rect);
            }
            _ => {}
        })
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
                        idle_state(&settings.read())
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
