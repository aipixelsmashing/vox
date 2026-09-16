//! Application wiring.
//!
//! Thread layout (see docs/ARCHITECTURE.md#threads):
//!   main         — Tauri event loop, tray, windows
//!   hotkey       — keytap listener on its own run loop
//!   audio        — cpal input callback, real-time priority, owned by the OS
//!   inference    — the loaded model; never touches the main thread
//!   orchestrator — the pipeline state machine

pub mod audio;
pub mod clipboard;
pub mod engine;
pub mod export;
pub mod history;
pub mod hotkey;
pub mod inject;
pub mod learning;
pub mod longform;
pub mod models;
pub mod permissions;
pub mod pipeline;
pub mod settings;
pub mod telemetry;
pub mod tray;
pub mod updater;

use std::sync::Arc;

use parking_lot::RwLock;

pub struct AppState {
    pub settings: Arc<RwLock<settings::Settings>>,
    pub history: Arc<history::Store>,
    pub pipeline: Arc<pipeline::Handle>,
}

pub fn run() {
    let _log_guard = init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch shows the history panel rather than starting a second tray icon.
            tray::show_history(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let settings = Arc::new(RwLock::new(settings::Settings::load_or_default()?));
            let history = Arc::new(history::Store::open(&settings::data_dir()?)?);

            // Weights are mmapped and unloaded when idle; a preload predictor reloads them
            // before the user reaches for the key. No residency setting — docs/FOOTPRINT.md.
            // On macOS 26+ this resolves to Apple's SpeechAnalyzer: no download, no weights.
            let engine = engine::spawn(settings.clone())?;

            let pipeline = pipeline::spawn(pipeline::Deps {
                settings: settings.clone(),
                history: history.clone(),
                engine,
                injector: inject::platform_injector(),
                app: app.handle().clone(),
            })?;

            hotkey::spawn(settings.clone(), pipeline.clone())?;
            tray::install(app, pipeline.clone())?;

            app.manage(AppState { settings, history, pipeline });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // History panel
            commands::history_list,
            commands::history_delete,
            commands::history_delete_all,
            commands::history_copy,
            commands::history_reinsert,
            commands::history_export,
            // Vocabulary — the trust surface for learning. Every term visible and deletable.
            commands::vocab_list,
            commands::vocab_forget,
            commands::vocab_export,
            // Long-form sessions
            commands::longform_stop,
            commands::longform_set_destination,
            // Settings
            commands::settings_get,
            commands::settings_set,
            commands::hotkey_capture_start,
            // Models
            commands::models_list,
            commands::models_download,
            commands::models_import,
            commands::models_remove,
            // Onboarding / diagnostics
            commands::permissions_status,
            commands::permissions_open_pane,
            commands::audio_devices,
            commands::diagnostics_recent,
            commands::export_everything,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build application")
        .run(|_app, event| {
            // The tray app has no windows most of the time. Closing the last window must not
            // exit the process.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    // Transcripts are never logged, at any level. Enforced by a test in tests/no_transcript_logs.rs.
    todo!("rolling file appender + env filter")
}

pub mod commands {
    //! Tauri command surface. Thin: every command validates input and delegates.
    //! Nothing here is on the dictation critical path.
}
