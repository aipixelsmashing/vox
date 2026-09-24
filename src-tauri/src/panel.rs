//! The history panel window: frameless, always on top, anchored under the tray icon, created
//! lazily and hidden rather than closed. Not a native menu, because a native menu cannot do a
//! scrolling searchable list with per-row actions (docs/HISTORY.md, docs/ARCHITECTURE.md).
//!
//! The webview is only ever woken to display something; nothing here is on the dictation
//! path. Keyboard focus goes to the panel so typing filters, which means the panel is the
//! frontmost app while it is open — the app it was opened *over* is remembered so the list
//! can rank that app's transcripts first and "Insert" can go back to it.

use parking_lot::Mutex;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub const HISTORY_WINDOW: &str = "history";
pub const SETTINGS_WINDOW: &str = "settings";
const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 520.0;
const GAP_BELOW_TRAY: f64 = 6.0;

/// Last known tray icon rectangle in logical pixels: (x, y, width, height). Updated on every
/// tray event that carries one, so the panel can open under the icon even from the menu.
static TRAY_RECT: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

/// Bundle id of the app that was frontmost when the panel last opened. Read by
/// `history_list` for the current-app boost.
static OPENED_OVER: Mutex<Option<String>> = Mutex::new(None);

pub fn remember_tray_rect(app: &AppHandle, rect: tauri::Rect) {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    let pos = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    *TRAY_RECT.lock() = Some((pos.x, pos.y, size.width, size.height));
}

pub fn opened_over() -> Option<String> {
    OPENED_OVER.lock().clone()
}

pub fn is_visible(app: &AppHandle) -> bool {
    app.get_webview_window(HISTORY_WINDOW)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

pub fn toggle_history(app: &AppHandle) {
    if is_visible(app) {
        hide_history(app);
    } else {
        show_history(app);
    }
}

pub fn hide_history(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(HISTORY_WINDOW) {
        let _ = w.hide();
    }
}

pub fn show_history(app: &AppHandle) {
    // Remember what the user was in before we take focus.
    *OPENED_OVER.lock() = frontmost_bundle_id();

    let window = match app.get_webview_window(HISTORY_WINDOW) {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(
                app,
                HISTORY_WINDOW,
                WebviewUrl::App(format!("index.html?window={HISTORY_WINDOW}").into()),
            )
            .title("History")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .visible(false)
            .inner_size(WIDTH, HEIGHT)
            .build();
            match built {
                Ok(w) => {
                    // Losing focus means the user moved on; the panel goes away by itself.
                    let handle = w.clone();
                    w.on_window_event(move |e| {
                        if let tauri::WindowEvent::Focused(false) = e {
                            let _ = handle.hide();
                        }
                    });
                    w
                }
                Err(e) => {
                    tracing::warn!("history window: {e}");
                    return;
                }
            }
        }
    };

    let _ = window.set_size(LogicalSize::new(WIDTH, HEIGHT));
    if let Some(pos) = anchor_position(app) {
        let _ = window.set_position(pos);
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// Under the tray icon, centred on it, clamped to the monitor the icon is on. Falls back to
/// the top-right corner of the primary monitor when no tray event has been seen yet.
fn anchor_position(app: &AppHandle) -> Option<LogicalPosition<f64>> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let mon_pos = monitor.position().to_logical::<f64>(scale);
    let mon_size = monitor.size().to_logical::<f64>(scale);
    let (x, y) = match *TRAY_RECT.lock() {
        Some((tx, ty, tw, th)) => (tx + tw / 2.0 - WIDTH / 2.0, ty + th + GAP_BELOW_TRAY),
        None => (mon_pos.x + mon_size.width - WIDTH - 12.0, mon_pos.y + 30.0),
    };
    let max_x = mon_pos.x + mon_size.width - WIDTH - 8.0;
    let x = x.min(max_x).max(mon_pos.x + 8.0);
    Some(LogicalPosition::new(x, y))
}

/// Settings: an ordinary window (docs/ARCHITECTURE.md), created lazily, hidden on close so
/// its state survives, shown and focused on every request.
pub fn show_settings(app: &AppHandle) {
    let window = match app.get_webview_window(SETTINGS_WINDOW) {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(
                app,
                SETTINGS_WINDOW,
                WebviewUrl::App(format!("index.html?window={SETTINGS_WINDOW}").into()),
            )
            .title("Vox Settings")
            .inner_size(780.0, 580.0)
            .min_inner_size(640.0, 420.0)
            .resizable(true)
            .visible(false)
            .build();
            match built {
                Ok(w) => {
                    let handle = w.clone();
                    w.on_window_event(move |e| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                            api.prevent_close();
                            let _ = handle.hide();
                        }
                    });
                    w
                }
                Err(e) => {
                    tracing::warn!("settings window: {e}");
                    return;
                }
            }
        }
    };
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    crate::inject::macos::frontmost_app().and_then(|(_, _, bundle)| bundle)
}

#[cfg(not(target_os = "macos"))]
fn frontmost_bundle_id() -> Option<String> {
    None
}
