//! Tray icon and menu. Four states, distinguishable by silhouette and not colour alone, plus
//! a distinct always-listening variant when pre-roll is enabled — an open microphone must
//! never be invisible. See docs/UI-SPEC.md.
//!
//! Menu: Show history · Pause dictation · Quit Vox. Left click opens the history panel
//! directly; the menu is on right click. Settings joins the menu with its window.

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
    let history = MenuItem::with_id(app, "history", "Show history", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause dictation", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Vox", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &history,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(IconState::Idle))
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
            "history" => panel::show_history(app),
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
