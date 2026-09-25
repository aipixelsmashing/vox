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
pub const ONBOARDING_WINDOW: &str = "onboarding";
pub const TOAST_WINDOW: &str = "toast";
pub const OVERLAY_WINDOW: &str = "overlay";
/// The pill is 180×36 in the page (src/windows/RecordingOverlay.tsx) plus 4px of shadow room
/// on every side; the window is that, fixed, so the page never has to report a size.
const OVERLAY_WIDTH: f64 = 188.0;
const OVERLAY_HEIGHT: f64 = 44.0;
/// Between the caret's rectangle and the pill.
const OVERLAY_GAP: f64 = 6.0;
const TOAST_WIDTH: f64 = 380.0;
/// Before the page has measured its text; `toast_fit` replaces it.
const TOAST_INITIAL_HEIGHT: f64 = 56.0;
const TOAST_LIFETIME: std::time::Duration = std::time::Duration::from_millis(4500);

/// The message the toast currently shows, for a window that was created after the event.
static TOAST_MESSAGE: Mutex<Option<String>> = Mutex::new(None);
/// Bumped per toast so an older hide-timer cannot take down a newer message.
static TOAST_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
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

/// Onboarding: shown on first launch until its last step is done, and from Settings
/// afterwards. The page reads the saved step itself.
pub fn show_onboarding(app: &AppHandle) {
    let window = match app.get_webview_window(ONBOARDING_WINDOW) {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(
                app,
                ONBOARDING_WINDOW,
                WebviewUrl::App(format!("index.html?window={ONBOARDING_WINDOW}").into()),
            )
            .title("Welcome to Vox")
            .inner_size(640.0, 560.0)
            .min_inner_size(520.0, 440.0)
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
                    tracing::warn!("onboarding window: {e}");
                    return;
                }
            }
        }
    };
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn toast_current() -> Option<String> {
    TOAST_MESSAGE.lock().clone()
}

/// Shows `message` in Vox's own toast at the bottom of the screen for a few seconds. Never
/// takes focus, never steals the caret from the field the user is in. Every build has this;
/// Notification Center is the second channel on signed builds (docs/PERMISSIONS.md).
pub fn show_toast(app: &AppHandle, message: &str) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;

    *TOAST_MESSAGE.lock() = Some(message.to_string());
    let generation = TOAST_GEN.fetch_add(1, Ordering::AcqRel) + 1;

    let window = match app.get_webview_window(TOAST_WINDOW) {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(
                app,
                TOAST_WINDOW,
                WebviewUrl::App(format!("index.html?window={TOAST_WINDOW}").into()),
            )
            .title("Vox")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .focusable(false)
            .skip_taskbar(true)
            .resizable(false)
            .accept_first_mouse(true)
            .visible(false)
            .inner_size(TOAST_WIDTH, TOAST_INITIAL_HEIGHT)
            .build();
            match built {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("toast window: {e}");
                    return;
                }
            }
        }
    };
    let _ = app.emit_to(
        TOAST_WINDOW,
        "vox://toast",
        serde_json::json!({ "message": message }),
    );
    // The page measures the text and calls toast_fit, which sizes, anchors and shows. If it
    // never does (a broken page), show anyway after a beat rather than lose the message.
    let fallback = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(600));
        if !fallback.is_visible().unwrap_or(false) {
            let _ = fallback.show();
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(TOAST_LIFETIME);
        if TOAST_GEN.load(Ordering::Acquire) == generation {
            if let Some(w) = handle.get_webview_window(TOAST_WINDOW) {
                let _ = w.hide();
            }
            *TOAST_MESSAGE.lock() = None;
        }
    });
}

/// Sizes the toast to its measured text, anchors it bottom centre, and shows it.
pub fn fit_toast(app: &AppHandle, height: f64) {
    let Some(window) = app.get_webview_window(TOAST_WINDOW) else {
        return;
    };
    let height = height.clamp(32.0, 320.0);
    let _ = window.set_size(LogicalSize::new(TOAST_WIDTH, height));
    if let Some(pos) = toast_position(app, height) {
        let _ = window.set_position(pos);
    }
    let _ = window.show();
}

/// Bottom centre of the primary monitor, above where the Dock usually is.
fn toast_position(app: &AppHandle, height: f64) -> Option<LogicalPosition<f64>> {
    bottom_centre(app, TOAST_WIDTH, height)
}

fn bottom_centre(app: &AppHandle, width: f64, height: f64) -> Option<LogicalPosition<f64>> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let pos = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    Some(LogicalPosition::new(
        pos.x + (size.width - width) / 2.0,
        pos.y + size.height - height - 96.0,
    ))
}

// ─── Recording overlay ───────────────────────────────────────────────────────

/// A rectangle in logical screen points with a top-left origin — the caret's, as the
/// accessibility API reports it and as Tauri positions windows: (x, y, width, height).
pub type Anchor = (f64, f64, f64, f64);

/// Shows the recording overlay near `anchor`, or bottom centre when the caret could not be
/// located (docs/UI-SPEC.md, "Recording overlay"). Click-through and never focused: the
/// user is typing into another app and must not notice this window exists except by eye.
/// Created lazily on the first dictation and hidden afterwards, like the toast.
pub fn show_overlay(app: &AppHandle, anchor: Option<Anchor>) {
    let window = match app.get_webview_window(OVERLAY_WINDOW) {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(
                app,
                OVERLAY_WINDOW,
                WebviewUrl::App(format!("index.html?window={OVERLAY_WINDOW}").into()),
            )
            .title("Vox")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .focusable(false)
            .skip_taskbar(true)
            .resizable(false)
            .accept_first_mouse(false)
            .visible_on_all_workspaces(true)
            .visible(false)
            .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
            .build();
            match built {
                Ok(w) => {
                    if let Err(e) = w.set_ignore_cursor_events(true) {
                        tracing::warn!("overlay: could not make the window click-through: {e}");
                    }
                    float_over_full_screen(&w);
                    w
                }
                Err(e) => {
                    tracing::warn!("overlay window: {e}");
                    return;
                }
            }
        }
    };
    let pos = overlay_position(app, anchor);
    if let Some(pos) = pos {
        let _ = window.set_position(pos);
    }
    let _ = window.show();
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OVERLAY_WINDOW) {
        let _ = w.hide();
    }
}

/// Just below the caret, left edge on it; above the caret when there is no room below;
/// clamped to the monitor the caret is on. Bottom centre when there is no caret.
fn overlay_position(app: &AppHandle, anchor: Option<Anchor>) -> Option<LogicalPosition<f64>> {
    let Some((x, y, w, h)) = anchor else {
        return bottom_centre(app, OVERLAY_WIDTH, OVERLAY_HEIGHT);
    };
    let monitor = app
        .monitor_from_point(x + w / 2.0, y + h / 2.0)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let mon = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    Some(place_overlay(
        (x, y, w, h),
        (mon.x, mon.y, size.width, size.height),
    ))
}

/// Pure placement, unit-tested: caret rect and monitor rect in, window origin out.
fn place_overlay(caret: Anchor, monitor: Anchor) -> LogicalPosition<f64> {
    const MARGIN: f64 = 8.0;
    let (cx, cy, _cw, ch) = caret;
    let (mx, my, mw, mh) = monitor;
    // The pill has 4px of shadow room; shift so the pill's edge, not the window's, aligns.
    let mut x = cx - 4.0 - MARGIN;
    let mut y = cy + ch + OVERLAY_GAP;
    if y + OVERLAY_HEIGHT > my + mh - MARGIN {
        y = cy - OVERLAY_GAP - OVERLAY_HEIGHT;
    }
    let max_x = mx + mw - OVERLAY_WIDTH - MARGIN;
    x = x.min(max_x).max(mx + MARGIN);
    y = y.max(my + MARGIN);
    LogicalPosition::new(x, y)
}

/// A floating Tauri window still hides behind a full-screen app: macOS puts each full-screen
/// app in its own Space and only windows that opt in may join it. Opt the overlay in, and
/// keep it out of the window cycle, so dictating into a full-screen editor shows the pill.
#[cfg(target_os = "macos")]
fn float_over_full_screen(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ptr = ptr as usize;
    let _ = window.run_on_main_thread(move || {
        // SAFETY: `ns_window` returned this NSWindow, which Tauri keeps alive for the
        // window's lifetime, and we are on the main thread as AppKit requires.
        let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
    });
}

#[cfg(not(target_os = "macos"))]
fn float_over_full_screen(_window: &tauri::WebviewWindow) {}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    crate::inject::macos::frontmost_app().and_then(|(_, _, bundle)| bundle)
}

#[cfg(not(target_os = "macos"))]
fn frontmost_bundle_id() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Anchor = (0.0, 0.0, 1440.0, 900.0);

    #[test]
    fn overlay_sits_below_the_caret_left_aligned() {
        let p = place_overlay((300.0, 200.0, 1.0, 18.0), MON);
        assert_eq!(p.x, 300.0 - 12.0);
        assert_eq!(p.y, 200.0 + 18.0 + OVERLAY_GAP);
    }

    #[test]
    fn overlay_moves_above_the_caret_at_the_bottom_of_the_screen() {
        let p = place_overlay((300.0, 880.0, 1.0, 18.0), MON);
        assert_eq!(p.y, 880.0 - OVERLAY_GAP - OVERLAY_HEIGHT);
    }

    #[test]
    fn overlay_stays_on_the_monitor() {
        let right = place_overlay((1435.0, 200.0, 1.0, 18.0), MON);
        assert_eq!(right.x, 1440.0 - OVERLAY_WIDTH - 8.0);
        let left = place_overlay((2.0, 200.0, 1.0, 18.0), MON);
        assert_eq!(left.x, 8.0);
        // A second monitor to the left has negative x; clamping is relative to it.
        let second = place_overlay((-1000.0, 50.0, 1.0, 18.0), (-1920.0, 0.0, 1920.0, 1080.0));
        assert_eq!(second.x, -1012.0);
    }
}
