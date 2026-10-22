//! Tray icon and menu. Four states, distinguishable by silhouette and not colour alone, plus
//! a distinct always-listening variant when pre-roll is enabled — an open microphone must
//! never be invisible. See docs/UI-SPEC.md.
//!
//! Either click opens the history panel; there is no native menu. The panel's header cog is
//! the menu: Settings… · Pause dictation · Check for updates… · Quit Vox (docs/UI-SPEC.md).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use tauri::image::Image;
use tauri::tray::{MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::panel;

pub const TRAY_ID: &str = "main";

/// Two click events closer together than this are one click delivered twice. macOS can hand
/// a status item's mouse-up to more than one responder, and a doubled click toggles the panel
/// open and shut again, which reads as "clicking the icon does nothing".
const CLICK_DEBOUNCE: Duration = Duration::from_millis(250);
static LAST_CLICK: Mutex<Option<Instant>> = Mutex::new(None);

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
) -> anyhow::Result<()> {
    let initial = idle_state(&settings.read());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(initial))
        .icon_as_template(true)
        .tooltip("Vox — hold the key and speak")
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                let now = Instant::now();
                let mut last = LAST_CLICK.lock();
                if last.is_some_and(|t| now.duration_since(t) < CLICK_DEBOUNCE) {
                    tracing::info!(
                        "tray: {button:?} click within {CLICK_DEBOUNCE:?} of the last, ignored"
                    );
                    return;
                }
                *last = Some(now);
                drop(last);
                tracing::info!(
                    "tray: {button:?} click, panel visible={}",
                    panel::is_visible(tray.app_handle())
                );
                panel::remember_tray_rect(tray.app_handle(), rect);
                panel::toggle_history(tray.app_handle());
            }
            TrayIconEvent::Enter { rect, .. } | TrayIconEvent::Move { rect, .. } => {
                panel::remember_tray_rect(tray.app_handle(), rect);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Pause from the panel's cog menu: the hotkey does nothing while paused and the tray badges.
pub fn set_paused(
    app: &tauri::AppHandle,
    paused: &AtomicBool,
    settings: &crate::settings::Settings,
    now: bool,
) {
    paused.store(now, Ordering::Relaxed);
    tracing::info!("dictation {}", if now { "paused" } else { "resumed" });
    set_state(
        app,
        if now {
            IconState::Attention
        } else {
            idle_state(settings)
        },
    );
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
