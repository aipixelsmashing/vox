//! Application wiring.
//!
//! Thread layout (see docs/ARCHITECTURE.md#threads):
//!   main         — Tauri event loop, tray, windows
//!   hotkey       — keytap listener on its own thread
//!   audio        — cpal input callback, real-time priority, owned by the OS
//!   inference    — the engine; in v1 a Swift bridge to Apple's SpeechAnalyzer
//!   pipeline     — the state machine, orchestrating the rest
//!
//! M1 scope (ROADMAP.md): hotkey → record → transcribe → insert, a tray icon and a quit item.
//! No windows, no Tauri commands yet; the command surface arrives with the UI in M3.

pub mod audio;
pub mod clipboard;
pub mod commands;
pub mod context;
pub mod cues;
pub mod engine;
pub mod export;
pub mod history;
pub mod hotkey;
pub mod inject;
pub mod learning;
pub mod longform;
pub mod models;
pub mod panel;
pub mod permissions;
pub mod pipeline;
pub mod settings;
pub mod telemetry;
pub mod tray;
pub mod updater;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    pub settings: Arc<RwLock<settings::Settings>>,
    pub history: Arc<history::Store>,
    pub pipeline: Arc<pipeline::Handle>,
    pub paused: Arc<AtomicBool>,
    /// Shared with the pipeline; the history panel's "Insert" re-runs it.
    pub injector: Arc<dyn inject::TextInjector>,
    pub engine: engine::Handle,
}

impl AppState {
    /// Whether the engine loaded; the Model pane's "installed" for the built-in engine.
    pub fn pipeline_engine_ok(&self) -> bool {
        self.engine.status().is_ok()
    }
}

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch is the user asking to see the app: show the history panel.
            panel::show_history(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings = Arc::new(RwLock::new(settings::Settings::load_or_default()?));
            init_logging(&settings.read().advanced.log_level);
            tracing::info!("vox {} starting", env!("CARGO_PKG_VERSION"));

            let history = Arc::new(history::Store::open(&settings::data_dir()?)?);
            let paused = Arc::new(AtomicBool::new(false));
            tray::install(app, paused.clone())?;

            // Ask for what we need and say what is missing, in the deck's words. Blocks on
            // the microphone prompt, so not on the main thread.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let report = permissions::request_all();
                let missing = report.missing_messages();
                if missing.is_empty() {
                    return;
                }
                tray::set_state(&handle, tray::IconState::Attention);
                for msg in &missing {
                    notify(&handle, msg);
                }
                // Open the first relevant pane; one at a time is enough.
                let pane = if report.input_monitoring != permissions::Status::Granted {
                    "input-monitoring"
                } else if report.accessibility != permissions::Status::Granted {
                    "accessibility"
                } else {
                    "microphone"
                };
                let _ = permissions::open_pane(pane);
            });

            let engine = engine::spawn(settings.clone())?;
            {
                // Surface an unavailable engine once, at launch, rather than per attempt.
                let engine = engine.clone();
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    if let Err(e) = engine.status() {
                        tracing::warn!("engine unavailable: {e}");
                        tray::set_state(&handle, tray::IconState::Attention);
                        notify(&handle, &format!("Transcription is unavailable: {e}"));
                    }
                });
            }

            if settings.read().privacy.read_focused_field {
                // The word list and the dictation module, ready before the first key-down.
                context::warm();
                engine.prepare_context();
            }

            let injector: Arc<dyn inject::TextInjector> =
                Arc::from(inject::platform_injector(settings.clone()));
            let pipeline = pipeline::spawn(pipeline::Deps {
                settings: settings.clone(),
                history: history.clone(),
                engine: engine.clone(),
                injector: injector.clone(),
                app: app.handle().clone(),
                paused: paused.clone(),
            })?;

            let on_panel: Arc<dyn Fn() + Send + Sync> = Arc::new({
                let handle = app.handle().clone();
                move || panel::toggle_history(&handle)
            });
            if let Err(e) =
                hotkey::spawn(settings.clone(), pipeline.clone(), paused.clone(), on_panel)
            {
                // keytap fails fast when Input Monitoring is missing: badge, never a dead key.
                tracing::warn!("hotkey unavailable: {e}");
                tray::set_state(app.handle(), tray::IconState::Attention);
                notify(app.handle(), permissions::MSG_INPUT_MONITORING);
            }

            // First launch, or a setup left unfinished: the window comes up by itself.
            if !settings.read().onboarding.done {
                panel::show_onboarding(app.handle());
            }

            app.manage(AppState {
                settings,
                history,
                pipeline,
                paused,
                injector,
                engine,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::history_list,
            commands::history_delete,
            commands::history_delete_all,
            commands::history_copy,
            commands::history_reinsert,
            commands::history_export,
            commands::settings_get,
            commands::settings_set,
            commands::hotkey_capture_start,
            commands::permissions_status,
            commands::permissions_open_pane,
            commands::audio_devices,
            commands::models_list,
            commands::models_download,
            commands::models_import,
            commands::models_remove,
            commands::vocab_list,
            commands::vocab_forget,
            commands::vocab_export,
            commands::diagnostics_recent,
            commands::export_everything,
            commands::panel_hide,
            commands::app_relaunch,
            commands::mic_test_start,
            commands::mic_test_stop,
            commands::onboarding_open,
            commands::toast_current,
            commands::toast_fit,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build application")
        .run(|_app, event| {
            // The tray app has no windows most of the time. Closing the last window must not
            // exit the process; an explicit Quit (exit code set) must.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// One notification per failure, never for success: text arriving is the success
/// notification (docs/VALUES.md). Errors here are swallowed; a failed notification must not
/// take the dictation path down with it.
pub fn notify(app: &tauri::AppHandle, body: &str) {
    // Vox's own toast on every build; the system channel below is the second copy.
    panel::show_toast(app, body);
    // macOS: UserNotifications through the Swift bridge. The plugin's NSUserNotification path
    // is dropped silently by current macOS. Falls through to the plugin when running
    // unbundled (`cargo run`), where UserNotifications is unavailable.
    #[cfg(target_os = "macos")]
    {
        let title = std::ffi::CString::new("Vox").unwrap_or_default();
        if let Ok(body_c) = std::ffi::CString::new(body) {
            // SAFETY: both pointers are valid NUL-terminated strings for the duration of the
            // call; the bridge copies them.
            if unsafe { mac::vox_notify(title.as_ptr(), body_c.as_ptr()) } {
                tracing::info!("notification handed to UserNotifications");
                return;
            }
            tracing::warn!("UserNotifications unavailable (unbundled run); using the plugin");
        }
    }
    if let Err(e) = app.notification().builder().title("Vox").body(body).show() {
        tracing::warn!("notification failed: {e}");
    }
}

#[cfg(target_os = "macos")]
mod mac {
    extern "C" {
        // swift/SpeechAnalyzerBridge.swift
        pub fn vox_notify(title: *const std::ffi::c_char, body: *const std::ffi::c_char) -> bool;
    }
}

/// Called by the Swift bridge to write into this process's log. The bridge never passes
/// transcript text; its call sites are the notification and permission paths only.
///
/// # Safety
/// `message` must be a valid NUL-terminated string for the duration of the call.
#[cfg(target_os = "macos")]
#[no_mangle]
pub unsafe extern "C" fn vox_bridge_log(message: *const std::ffi::c_char) {
    if message.is_null() {
        return;
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string.
    let msg = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy();
    tracing::info!("bridge: {msg}");
}

/// Rolling file log under the OS log directory. Transcripts are never logged, at any level;
/// tests/guards.rs asserts the pipeline never formats transcript text into a log call.
fn init_logging(level: &str) {
    let Ok(dir) = settings::log_dir() else {
        return;
    };
    let appender = tracing_appender::rolling::daily(dir, "vox.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = tracing_subscriber::EnvFilter::try_new(level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
    let _ = LOG_GUARD.set(guard);
}
