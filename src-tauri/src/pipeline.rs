//! The dictation state machine.
//!
//! Treating this as a linear function (record → transcribe → insert) is where most of the
//! bugs in this category of app come from. It is a state machine with explicit illegal
//! transitions, and every terminal path has a defined user-visible outcome.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{engine, history, inject, settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Arming,
    Recording,
    Transcribing,
    Injecting,
}

#[derive(Debug)]
pub enum Event {
    HotkeyDown,
    HotkeyUp,
    Cancel,
    AudioReady,
    LengthCapReached,
    TranscriptReady(engine::Transcript),
    EngineFailed(engine::Error),
}

/// Captured at hotkey-DOWN, not at key-up. Focus can move while the user is speaking, and
/// inserting into whatever they switched to is worse than not inserting at all.
#[derive(Debug, Clone)]
pub struct InjectionTarget {
    pub pid: u32,
    pub app_name: String,
    #[cfg(target_os = "macos")]
    pub element: Option<inject::macos::ElementRef>,
    #[cfg(target_os = "windows")]
    pub hwnd: isize,
    pub captured_at: Instant,
}

pub struct Deps {
    pub settings: Arc<parking_lot::RwLock<settings::Settings>>,
    pub history: Arc<history::Store>,
    pub engine: engine::Handle,
    pub injector: Box<dyn inject::TextInjector>,
    pub app: tauri::AppHandle,
}

pub struct Handle {
    tx: crossbeam_channel::Sender<Event>,
}

impl Handle {
    pub fn send(&self, event: Event) {
        // Never block the hotkey thread. A full channel means we are wedged; drop and warn.
        if self.tx.try_send(event).is_err() {
            tracing::warn!("pipeline channel full, event dropped");
        }
    }
}

pub fn spawn(deps: Deps) -> anyhow::Result<Arc<Handle>> {
    let (tx, rx) = crossbeam_channel::bounded(32);
    std::thread::Builder::new()
        .name("vox-pipeline".into())
        .spawn(move || run(deps, rx))?;
    Ok(Arc::new(Handle { tx }))
}

fn run(_deps: Deps, _rx: crossbeam_channel::Receiver<Event>) {
    // Transition rules, all of which are load-bearing:
    //
    //  Idle          + HotkeyDown        -> Arming        (capture InjectionTarget here)
    //  Arming        + AudioReady        -> Recording
    //  Arming        + HotkeyUp          -> Idle          (below minHoldMs: discard silently)
    //  Recording     + HotkeyUp          -> Transcribing
    //  Recording     + LengthCapReached  -> Transcribing  (+ tell the user why)
    //  Recording     + Cancel            -> Idle          (drop audio, write nothing)
    //  Transcribing  + Cancel            -> Idle          (drop result, write nothing)
    //  Transcribing  + TranscriptReady   -> Injecting     (revalidate target first)
    //  Transcribing  + EngineFailed      -> Idle          (surface once, restart engine thread)
    //  Injecting     + <complete>        -> Idle          (always write history, incl. failures)
    //  * (non-Idle)  + HotkeyDown        -> ignored, flash the busy state. Never queue.
    todo!("state machine")
}

/// Runs after transcription, before injection.
fn revalidate(
    _target: &InjectionTarget,
    _on_focus_change: settings::OnFocusChange,
) -> Result<(), inject::FallbackReason> {
    // If focus moved to a different process, the default is to abort injection and fall back
    // to the clipboard with a message naming both applications.
    todo!()
}

const _MAX_TRANSCRIBE_WAIT: Duration = Duration::from_secs(60);
