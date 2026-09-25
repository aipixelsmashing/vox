//! The settings commands, driven through Tauri's IPC layer the way the webview drives them
//! (docs/TESTING.md, "Command surface").
//!
//! For days every `settings_set` call was rejected by argument deserialisation before the
//! command ran: the contract passes the patch as the whole argument, the Rust signature
//! expected it under a key, and nothing logged the mismatch. This test invokes the real
//! handlers on the mock runtime with the argument shaped exactly as `src/lib/contract.ts`
//! declares it, so that gap cannot reopen silently, and asserts a mis-shaped call is an
//! error the caller sees rather than a no-op.
//!
//! One test function: `VOX_HOME` is process-wide, and the phases share one settings file.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow};

use vox_lib::inject::{Error as InjectError, InjectionOutcome, TextInjector};
use vox_lib::pipeline::InjectionTarget;
use vox_lib::{commands, engine, history, pipeline, settings, AppState};

/// Never called: the settings commands do not inject. Exists so `AppState` can be built.
struct NoInjector;

impl TextInjector for NoInjector {
    fn capture_target(&self) -> Result<InjectionTarget, InjectError> {
        Err(InjectError::Platform("not in this test".into()))
    }
    fn inject(
        &self,
        _text: &str,
        _target: &InjectionTarget,
    ) -> Result<InjectionOutcome, InjectError> {
        Err(InjectError::Platform("not in this test".into()))
    }
    fn frontmost(&self) -> Option<(u32, String)> {
        None
    }
}

fn build_app() -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let state = AppState {
        settings: Arc::new(parking_lot::RwLock::new(settings::Settings::default())),
        history: Arc::new(history::Store::open_in_memory().expect("in-memory store")),
        pipeline: Arc::new(pipeline::Handle::disconnected()),
        paused: Arc::new(AtomicBool::new(false)),
        injector: Arc::new(NoInjector),
        engine: engine::Handle::disconnected(),
    };
    let app = mock_builder()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::settings_get,
            commands::settings_set
        ])
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "settings", Default::default())
        .build()
        .expect("mock webview");
    (app, webview)
}

/// One IPC call, exactly as the webview makes it: a local origin, the contract's command
/// name, and the argument object as the body.
fn invoke(webview: &WebviewWindow<MockRuntime>, cmd: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().expect("JSON response"))
}

#[test]
fn settings_set_writes_and_reads_back_through_the_command_surface() {
    let home = tempfile::tempdir().expect("tempdir");
    // Process-wide, before anything touches the settings path. Nothing else in this test
    // binary reads it.
    std::env::set_var("VOX_HOME", home.path());
    let file = home.path().join("config").join("settings.json");

    let (app, webview) = build_app();

    // ── The contract's shape: settings_set(partial), the patch is the whole argument ──
    let merged = invoke(
        &webview,
        "settings_set",
        json!({ "privacy": { "readFocusedField": true }, "history": { "maxItems": 50 } }),
    )
    .expect("the documented shape is accepted");
    assert_eq!(merged["privacy"]["readFocusedField"], json!(true));
    assert_eq!(merged["history"]["maxItems"], json!(50));
    assert_eq!(
        merged["history"]["maxDays"],
        json!(30),
        "untouched siblings survive"
    );
    assert_eq!(merged["version"], json!(settings::CURRENT_VERSION));

    // ── Reads back through the same surface ──
    let got = invoke(&webview, "settings_get", json!({})).expect("settings_get");
    assert_eq!(got["privacy"]["readFocusedField"], json!(true));
    assert_eq!(got["history"]["maxItems"], json!(50));

    // ── And from the in-memory state the pipeline reads per dictation ──
    let state = app.state::<AppState>();
    assert!(state.settings.read().privacy.read_focused_field);
    assert_eq!(state.settings.read().history.max_items, 50);

    // ── And from disk, where a relaunch reads it (mode 0600) ──
    let raw = std::fs::read_to_string(&file).expect("settings.json was written");
    let on_disk: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(on_disk["privacy"]["readFocusedField"], json!(true));
    assert_eq!(on_disk["history"]["maxItems"], json!(50));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "settings.json is private");
    }
    let reloaded = settings::Settings::load_or_default().expect("relaunch reads it");
    assert!(reloaded.privacy.read_focused_field);
    assert_eq!(reloaded.history.max_items, 50);

    // ── The old wrong shape, the patch under a key: an error, never a silent no-op ──
    let err = invoke(
        &webview,
        "settings_set",
        json!({ "patch": { "privacy": { "readFocusedField": false } } }),
    )
    .expect_err("a wrapped patch is rejected");
    assert!(
        err.to_string().contains("patch"),
        "the error names the offending key: {err}"
    );
    assert!(
        state.settings.read().privacy.read_focused_field,
        "a rejected call changes nothing"
    );
    let raw_after: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(
        raw_after.get("patch").is_none(),
        "nothing leaks into the file"
    );

    // ── A wrong value type: a structured VoxError the panes can render ──
    let err = invoke(
        &webview,
        "settings_set",
        json!({ "history": { "maxItems": "many" } }),
    )
    .expect_err("a wrong type is rejected");
    assert!(err["userMessage"].is_string(), "structured error: {err}");
    assert_eq!(state.settings.read().history.max_items, 50, "unchanged");

    // ── Turning the setting back off round-trips too ──
    let merged = invoke(
        &webview,
        "settings_set",
        json!({ "privacy": { "readFocusedField": false } }),
    )
    .unwrap();
    assert_eq!(merged["privacy"]["readFocusedField"], json!(false));
    assert!(
        !settings::Settings::load_or_default()
            .unwrap()
            .privacy
            .read_focused_field
    );
}
