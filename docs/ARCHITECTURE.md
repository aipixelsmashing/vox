# Architecture

## Shape of the thing

One process. A Rust core does all the real work; a webview renders the settings window and
the history panel and is not involved in the dictation path at all. This matters: if the UI
layer were on the critical path, every dictation would pay IPC and render cost. The webview
is only woken to display something.

```
                    ┌────────────────────────────────────────────┐
                    │  Rust core (tokio + dedicated OS threads)  │
                    │                                            │
  keyboard  ───────►│  hotkey ──► pipeline ──► engine ──► inject │──► focused app
   (evdev/           │     │          │          │         │      │
   CGEventTap/       │     │          ▼          │         ▼      │
   WH_KEYBOARD_LL)   │     │       audio         │      history   │
                    │     │      (cpal)         │      (sqlite)  │
                    │     └──────────┬──────────┴─────────┬──────┘
                    │                ▼                    ▼
                    │            tray icon           events → webview
                    └────────────────────────────────────────────┘
                                                          │
                                             ┌────────────▼─────────────┐
                                             │ Webview (only when shown)│
                                             │ history panel · settings │
                                             └──────────────────────────┘
```

## Threads

Concurrency here is not incidental — the latency budget depends on nothing blocking anything.

| Thread | Owns | Why it's separate |
| --- | --- | --- |
| Main | Tauri event loop, tray, windows | Platform requirement: tray and windows must be on the main thread on macOS and Windows |
| Hotkey listener | `keytap::Tap` on its own run loop | Platform key taps need a dedicated run loop; a blocked callback delays every keystroke on the system |
| Audio | `cpal` input stream callback | Real-time priority thread owned by the audio subsystem. Nothing but a lock-free push into a ring buffer happens here |
| Inference | The engine. v1: a Swift bridge to SpeechAnalyzer, whose model runs in Apple's XPC service. M8: ONNX/ggml session with mmapped weights | Runs for hundreds of ms. Never on the main thread |
| Residency | Preload prediction | **M8 only.** v1 has no model of ours to unload; Apple's service retention is one option value ([FOOTPRINT.md](FOOTPRINT.md)) |
| Orchestrator | The pipeline state machine | Async, coordinates the rest, does the injection call |

Channels between them are `crossbeam` for the hotkey and audio hops (no async runtime in a
real-time callback) and `tokio::mpsc` elsewhere.

## The dictation pipeline

State machine, because half the bugs in tools like this come from treating it as a linear
function:

```
Idle ──key-down──► Arming ──audio-ready──► Recording ──key-up──► Transcribing ──► Injecting ──► Idle
  ▲                   │                        │                      │              │
  └───too-short───────┘                        │                      │              │
  ▲                                            │                      │              │
  └────────────────── Esc / cancel ────────────┴──────────────────────┘              │
  ▲                                                                                  │
  └────────────────── failure → clipboard fallback + notify ─────────────────────────┘
```

Rules the state machine enforces:

- **A second key-down while not Idle is ignored**, not queued. Overlapping dictations are a
  bug factory and the user cannot physically want two at once.
- **The target is captured on entry to `Recording`**, not on exit. `InjectionTarget` holds
  the process id, window handle, and (on macOS) the focused `AXUIElement` reference.
- **On exit from `Transcribing` the target is re-validated.** If focus moved to a different
  application, the default is to skip injection, put the text on the clipboard, and say so —
  because typing into whatever the user has since switched to is worse than not typing.
  Configurable to "insert anyway".
- **Every terminal path writes to history**, including failures, which are stored with their
  failure reason so the history panel can offer "try inserting again".
- **A successful insertion registers a correction watch** on the target field for a bounded
  window. It is fire-and-forget, off the critical path, and its failure never affects
  dictation ([LEARNING.md](LEARNING.md)).

## Modules

```
src-tauri/src/
├── main.rs              entry, single-instance guard, tray, window management
├── lib.rs               wiring, app state, Tauri command surface
├── pipeline.rs          the state machine above
├── hotkey.rs            keytap wrapper, binding parsing, hold/toggle/double-tap modes
├── audio.rs             cpal capture, ring buffer, resample to 16 kHz mono f32, VAD
├── cues.rs              start/stop sound cues, synthesised, played via cpal output
├── engine/
│   ├── mod.rs           `SpeechEngine` trait + registry
│   ├── residency.rs     adaptive unload/preload, mmap weights (docs/FOOTPRINT.md)
│   ├── parakeet.rs      parakeet-rs / ONNX Runtime backend
│   ├── whisper.rs       whisper-rs (whisper.cpp) backend
│   └── speechanalyzer.rs  Apple SpeechAnalyzer, macOS 26+, via swift/ (Swift-only API)
├── swift/               SpeechAnalyzer bridge, compiled by build.rs with swiftc, linked statically
├── learning.rs          post-insertion correction watch, vocabulary candidates
├── longform.rs          locked sessions, chunked transcription, destinations
├── export.rs            everything out, as plain text and JSON
├── inject/
│   ├── mod.rs           `TextInjector` trait, fallback chain, verification
│   ├── macos.rs         AX insert → clipboard paste → unicode events
│   ├── windows.rs       win-text-inject: delayed-render clipboard → paste → unicode
│   └── linux.rs         X11 XTEST / Wayland libei / wtype / ydotool / clipboard-only
├── commands.rs          the Tauri command surface (src/lib/contract.ts is the truth)
├── panel.rs             history panel and settings windows, created lazily, hidden not closed
├── clipboard.rs         private clipboard writes, save/restore with change detection
├── history.rs           SQLite store, retention, search, wipe
├── models.rs            registry, resumable download, SHA-256 verify, sideload
├── settings.rs          schema, load/save, migration, hot reload
├── permissions.rs       per-platform permission checks and deep links to settings panes
├── updater.rs           tauri-plugin-updater wrapper, offline lock enforcement
└── telemetry.rs         deliberately empty; a test asserts it stays that way
```

## Key interfaces

```rust
pub trait SpeechEngine: Send {
    fn id(&self) -> &str;
    fn load(&mut self, opts: &EngineOptions) -> Result<()>;
    fn unload(&mut self);
    fn is_loaded(&self) -> bool;
    /// 16 kHz mono f32, -1.0..=1.0
    fn transcribe(&mut self, pcm: &[f32], hint: &LanguageHint) -> Result<Transcript>;
}

pub trait TextInjector: Send {
    fn capture_target(&self) -> Result<InjectionTarget>;
    fn inject(&self, text: &str, target: &InjectionTarget) -> Result<InjectionOutcome>;
}

pub enum InjectionOutcome {
    Inserted { method: Method },
    /// Text is on the clipboard; the user must paste it. Always carries a reason.
    ClipboardOnly { reason: FallbackReason },
}
```

`InjectionOutcome` has no "probably worked" variant on purpose. A method that cannot confirm
delivery returns `ClipboardOnly` — this is the rule that keeps Wayland and elevated-window
behaviour honest.

## Data on disk

| Path (macOS example) | Contents |
| --- | --- |
| `~/Library/Application Support/vox/settings.json` | Settings, mode `0600` |
| `~/Library/Application Support/vox/history.db` | SQLite transcripts and vocabulary, mode `0600` |
| `~/Library/Application Support/vox/models/` | Downloaded model files + `manifest.json` |
| `~/Library/Logs/vox/vox.log` | Rolling log, transcripts never written to it |

Windows: `%APPDATA%\vox\`. Linux: `$XDG_CONFIG_HOME/vox` and `$XDG_DATA_HOME/vox`.

Audio never touches disk unless debug capture is explicitly enabled.

## Frontend

Tauri v2 webview, React + TypeScript + Vite. Two windows, both created lazily:

- **History panel** — a small frameless always-on-top window anchored near the tray icon. Not
  a native tray menu, because native menus can't do a scrolling searchable list with
  per-row actions on all three platforms.
- **Settings** — an ordinary window.
- **Toast** — a tiny never-focused window at the bottom of the screen for the one message
  per failure; created on first use, hidden between messages.

Tray menu itself stays native and short: *Show history · Settings · Check for updates ·
Pause dictation · Quit*.

The frontend talks to the core through Tauri commands (`history_list`, `history_delete`,
`settings_get`, `settings_set`, `models_download`, `check_permissions`, …) and receives
`vox://state` events for recording state so the panel can show a live indicator.

## Failure handling

Every failure has a defined user-visible result. No silent drops.

| Failure | Result |
| --- | --- |
| Microphone permission missing | Onboarding pane, dictation disabled until granted |
| Accessibility / input-monitoring permission missing | Hotkey doesn't fire; tray icon shows a warning badge with a direct link to the OS settings pane |
| Model missing or corrupt | v1: Apple's locale assets not yet installed — tray offers the one-time install. M8: dictation disabled, tray offers re-download; hash mismatch is reported as a hash mismatch, not "download failed" |
| No speech detected | Silent no-op, no history entry |
| Focus changed during recording | Clipboard fallback + one-line notice naming both apps |
| Target refuses insertion (elevated window, secure input, password field) | Clipboard fallback + reason |
| Inference error / panic in engine thread | Engine thread restarts, transcript lost, error surfaced once (not per-attempt) |
| Recording hit the length cap | Transcribe what was captured, tell the user the cap was reached |
