//! The dictation state machine.
//!
//! Treating this as a linear function (record → transcribe → insert) is where most of the
//! bugs in this category of app come from. It is a state machine with explicit illegal
//! transitions, and every terminal path has a defined user-visible outcome.
//!
//! `transition` is the pure table and is unit-tested; `run` is the thread that executes it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{audio, clipboard, engine, history, inject, settings, tray};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording,
    Transcribing,
    Injecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    HotkeyDown,
    HotkeyUp,
    Cancel,
    /// Internal: the recording cap was hit.
    LengthCapReached,
}

/// What the runner does on a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    StartRecording,
    /// Stop recording and run transcription → injection. `capped` means the user is told.
    Finish {
        capped: bool,
    },
    /// Drop audio, write nothing.
    Discard,
    /// Ignored, but the user gets a busy flash.
    Busy,
    Nothing,
}

/// The transition table. Load-bearing; see the tests.
pub fn transition(state: State, event: Event) -> (State, Action) {
    use Action::*;
    use Event::*;
    use State::*;
    match (state, event) {
        (Idle, HotkeyDown) => (Recording, StartRecording),
        (Idle, _) => (Idle, Nothing),
        (Recording, HotkeyUp) => (Transcribing, Finish { capped: false }),
        (Recording, LengthCapReached) => (Transcribing, Finish { capped: true }),
        (Recording, Cancel) => (Idle, Discard),
        (Recording, HotkeyDown) => (Recording, Nothing),
        // A second key-down while busy is ignored, never queued.
        (Transcribing | Injecting, HotkeyDown) => (state, Busy),
        (Transcribing, Cancel) => (Idle, Discard),
        (Transcribing | Injecting, _) => (state, Nothing),
    }
}

/// Captured at hotkey-DOWN, not at key-up. Focus can move while the user is speaking, and
/// inserting into whatever they switched to is worse than not inserting at all.
#[derive(Debug, Clone)]
pub struct InjectionTarget {
    pub pid: u32,
    pub app_name: String,
    pub bundle_id: Option<String>,
    #[cfg(target_os = "macos")]
    pub element: Option<inject::macos::ElementRef>,
    #[cfg(target_os = "windows")]
    pub hwnd: isize,
    pub captured_at: Instant,
}

impl InjectionTarget {
    /// Terminals get newlines collapsed (bracketed paste can execute multi-line text).
    pub fn is_terminal(&self) -> bool {
        const TERMINALS: &[&str] = &[
            "com.apple.Terminal",
            "com.googlecode.iterm2",
            "dev.warp.Warp-Stable",
            "com.github.wez.wezterm",
            "io.alacritty",
            "net.kovidgoyal.kitty",
            "com.mitchellh.ghostty",
        ];
        self.bundle_id
            .as_deref()
            .map(|b| TERMINALS.contains(&b))
            .unwrap_or(false)
    }
}

pub struct Deps {
    pub settings: Arc<parking_lot::RwLock<settings::Settings>>,
    pub history: Arc<history::Store>,
    pub engine: engine::Handle,
    pub injector: Box<dyn inject::TextInjector>,
    pub app: tauri::AppHandle,
    pub paused: Arc<AtomicBool>,
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

/// Post-processing applied before injection: dictionary replacements, optional leading
/// capital, newline collapsing for terminals, optional trailing space. Never any rewriting.
pub fn postprocess(text: &str, out: &settings::Output, is_terminal: bool) -> String {
    let mut s = text.trim().to_string();
    for r in &out.dictionary {
        s = replace_word_ci(&s, &r.from, &r.to);
    }
    if out.capitalize_first {
        let mut c = s.chars();
        if let Some(first) = c.next() {
            s = first.to_uppercase().collect::<String>() + c.as_str();
        }
    }
    if is_terminal && out.collapse_newlines_in_terminals {
        s = s.split('\n').map(str::trim).collect::<Vec<_>>().join(" ");
    }
    if out.trailing_space && !s.is_empty() {
        s.push(' ');
    }
    s
}

/// Whole-word, case-insensitive replacement. `from` may contain spaces.
fn replace_word_ci(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    let needle = from.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    let is_boundary = |idx: usize| {
        idx == 0 || idx >= bytes.len() || !text[..idx].chars().last().unwrap().is_alphanumeric()
    };
    let is_boundary_after =
        |idx: usize| idx >= bytes.len() || !text[idx..].chars().next().unwrap().is_alphanumeric();
    while let Some(pos) = lower[i..].find(&needle) {
        let start = i + pos;
        let end = start + needle.len();
        if is_boundary(start) && is_boundary_after(end) {
            out.push_str(&text[i..start]);
            out.push_str(to);
            i = end;
        } else {
            let next = start + text[start..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&text[i..next]);
            i = next;
        }
    }
    out.push_str(&text[i..]);
    out
}

fn run(deps: Deps, rx: crossbeam_channel::Receiver<Event>) {
    let mut state = State::Idle;
    let mut capture: Option<audio::Capture> = None;
    let mut target: Option<InjectionTarget> = None;
    let mut started: Option<Instant> = None;
    // True while the engine has a live streaming session for this recording.
    let mut streaming = false;

    loop {
        // While recording, poll so audio is drained and the cap is enforced.
        let event = if state == State::Recording {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(e) => e,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if let Some(c) = capture.as_mut() {
                        let fresh = c.drain();
                        if streaming && !fresh.is_empty() {
                            deps.engine.stream_push(fresh);
                        }
                    }
                    let cap = deps.settings.read().audio.max_recording_sec;
                    let over = started
                        .map(|s| s.elapsed() >= Duration::from_secs(u64::from(cap)))
                        .unwrap_or(false);
                    if over {
                        Event::LengthCapReached
                    } else {
                        continue;
                    }
                }
                Err(_) => return,
            }
        } else {
            match rx.recv() {
                Ok(e) => e,
                Err(_) => return,
            }
        };

        if event == Event::HotkeyDown && deps.paused.load(Ordering::Relaxed) {
            continue;
        }

        let (next, action) = transition(state, event);
        state = next;
        match action {
            Action::Nothing => {}
            Action::Busy => tray::flash_busy(&deps.app),
            Action::StartRecording => {
                // Audio first: the microphone must be open before the first syllable. The
                // target is captured right after, still at key-down, while the ring buffer
                // fills; waking an Electron app's accessibility tree can take a few hundred ms.
                let device = deps.settings.read().audio.input_device.clone();
                match audio::Capture::start(&device) {
                    Ok(c) => {
                        capture = Some(c);
                        started = Some(Instant::now());
                        tray::set_state(&deps.app, tray::IconState::Recording);
                    }
                    Err(e) => {
                        tracing::warn!("audio start failed: {e}");
                        notify(&deps.app, crate::permissions::MSG_MICROPHONE);
                        state = State::Idle;
                        tray::set_state(&deps.app, tray::IconState::Attention);
                        continue;
                    }
                }
                // Open the streaming session so the engine works while the user speaks.
                let hint = language_hint(&deps.settings.read());
                streaming = match deps.engine.stream_start(hint) {
                    Ok(()) => true,
                    Err(e) => {
                        tracing::info!("streaming unavailable, batch transcription: {e}");
                        false
                    }
                };
                target = match deps.injector.capture_target() {
                    Ok(t) => Some(t),
                    Err(e) => {
                        tracing::warn!("capture_target failed: {e}");
                        None
                    }
                };
            }
            Action::Discard => {
                if streaming {
                    deps.engine.stream_cancel();
                    streaming = false;
                }
                capture = None;
                target = None;
                started = None;
                tray::set_state(&deps.app, tray::IconState::Idle);
            }
            Action::Finish { capped } => {
                let held_ms = started.map(|s| s.elapsed().as_millis() as u32).unwrap_or(0);
                let min_hold = deps.settings.read().hotkey.min_hold_ms;
                let cap = capture.take();
                let tgt = target.take();
                started = None;
                if held_ms < min_hold || cap.is_none() {
                    // A brush of the key. Nothing recorded, nothing written.
                    if streaming {
                        deps.engine.stream_cancel();
                        streaming = false;
                    }
                    state = State::Idle;
                    tray::set_state(&deps.app, tray::IconState::Idle);
                    continue;
                }
                tray::set_state(&deps.app, tray::IconState::Transcribing);
                if capped {
                    let minutes = deps.settings.read().audio.max_recording_sec / 60;
                    notify(
                        &deps.app,
                        &format!("Stopped at {minutes} minutes. Transcribed what was recorded."),
                    );
                }
                let cancelled = finish(&deps, cap.expect("checked"), tgt, held_ms, streaming, &rx);
                let _ = cancelled;
                streaming = false;
                state = State::Idle;
                tray::set_state(&deps.app, tray::IconState::Idle);
            }
        }
    }
}

/// Transcribe → revalidate → inject → history. Returns true if the user cancelled.
fn language_hint(s: &settings::Settings) -> engine::LanguageHint {
    if s.engine.language == "auto" {
        engine::LanguageHint::Auto
    } else {
        engine::LanguageHint::Fixed(s.engine.language.clone())
    }
}

fn finish(
    deps: &Deps,
    capture: audio::Capture,
    target: Option<InjectionTarget>,
    held_ms: u32,
    streaming: bool,
    rx: &crossbeam_channel::Receiver<Event>,
) -> bool {
    let released_at = Instant::now();
    let (vad, hint, output, history_cfg) = {
        let s = deps.settings.read();
        (
            s.audio.vad.clone(),
            language_hint(&s),
            s.output.clone(),
            s.history.clone(),
        )
    };

    let fin = match capture.finish(&vad) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("audio finish failed: {e}");
            if streaming {
                deps.engine.stream_cancel();
            }
            return false;
        }
    };
    let Some(pcm) = fin.speech else {
        // No speech: silent no-op, no history entry (docs/ARCHITECTURE.md#failure-handling).
        tracing::debug!("no speech detected in {held_ms} ms");
        if streaming {
            deps.engine.stream_cancel();
        }
        return false;
    };
    let duration_ms = (pcm.len() as u64 * 1000 / u64::from(audio::TARGET_SAMPLE_RATE)) as u32;

    let result = if streaming {
        if !fin.tail.is_empty() {
            deps.engine.stream_push(fin.tail);
        }
        deps.engine.stream_finish()
    } else {
        deps.engine.transcribe(pcm, hint)
    };
    let transcript = match result {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("engine failed: {e}");
            notify(
                &deps.app,
                "Transcription failed. The last recording was lost.",
            );
            return false;
        }
    };

    // A Cancel that arrived while the engine was busy still counts: drop the result.
    while let Ok(ev) = rx.try_recv() {
        if ev == Event::Cancel {
            return true;
        }
    }
    if transcript.text.trim().is_empty() {
        return false;
    }

    let is_terminal = target.as_ref().map(|t| t.is_terminal()).unwrap_or(false);
    let text = postprocess(&transcript.text, &output, is_terminal);
    tray::set_state(&deps.app, tray::IconState::Transcribing);

    // Revalidate: has focus moved to another application since key-down?
    let outcome = match target {
        Some(ref t) => {
            let moved = deps.injector.frontmost().filter(|(pid, _)| *pid != t.pid);
            match moved {
                Some((_, to)) if output.on_focus_change == settings::OnFocusChange::Clipboard => {
                    inject::InjectionOutcome::ClipboardOnly {
                        reason: inject::FallbackReason::FocusChanged {
                            from: t.app_name.clone(),
                            to,
                        },
                    }
                }
                _ => match deps.injector.inject(&text, t) {
                    Ok(o) => o,
                    Err(e) => {
                        tracing::warn!("inject failed: {e}");
                        inject::InjectionOutcome::ClipboardOnly {
                            reason: inject::FallbackReason::MethodFailed(
                                inject::Method::Accessibility,
                            ),
                        }
                    }
                },
            }
        }
        None => inject::InjectionOutcome::ClipboardOnly {
            reason: inject::FallbackReason::NoTextTarget,
        },
    };
    let latency_ms = released_at.elapsed().as_millis() as u32;

    let (outcome_str, note, method) = match &outcome {
        inject::InjectionOutcome::Inserted { method, .. } => (
            "inserted".to_string(),
            None,
            Some(method.as_str().to_string()),
        ),
        inject::InjectionOutcome::ClipboardOnly { reason } => {
            if reason.is_secret_context() {
                // Never on the clipboard, never in history.
                notify(&deps.app, &reason.user_message());
                return false;
            }
            if let Err(e) = clipboard::set_private(&text) {
                tracing::warn!("clipboard fallback failed: {e}");
            }
            notify(&deps.app, &reason.user_message());
            ("clipboard_only".to_string(), Some(reason.code()), None)
        }
    };

    if history_cfg.enabled {
        let entry = history::Entry {
            id: 0,
            created_at: history::now_millis(),
            word_count: history::word_count(&text),
            text,
            duration_ms,
            latency_ms,
            engine_id: "speechanalyzer".into(),
            language: transcript.language.clone(),
            target_app: target.as_ref().and_then(|t| t.bundle_id.clone()),
            outcome: outcome_str,
            outcome_note: note,
            method,
        };
        if let Err(e) = deps.history.insert(&entry) {
            tracing::warn!("history insert failed: {e}");
        }
        let _ = deps
            .history
            .prune(history_cfg.max_items, history_cfg.max_days);
    }
    tracing::info!(
        "dictation: {} ms audio, inference {} ms, release-to-text {} ms, outcome {:?}",
        duration_ms,
        transcript.inference_ms,
        latency_ms,
        match &outcome {
            inject::InjectionOutcome::Inserted { method, .. } => method.as_str(),
            inject::InjectionOutcome::ClipboardOnly { .. } => "clipboard_only",
        }
    );
    false
}

pub fn notify(app: &tauri::AppHandle, body: &str) {
    crate::notify(app, body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;
    use Event::*;
    use State::*;

    #[test]
    fn happy_path() {
        assert_eq!(transition(Idle, HotkeyDown), (Recording, StartRecording));
        assert_eq!(
            transition(Recording, HotkeyUp),
            (Transcribing, Finish { capped: false })
        );
        assert_eq!(transition(Transcribing, HotkeyUp), (Transcribing, Nothing));
    }

    #[test]
    fn cap_finishes_with_a_message() {
        assert_eq!(
            transition(Recording, LengthCapReached),
            (Transcribing, Finish { capped: true })
        );
    }

    #[test]
    fn cancel_drops_everything() {
        assert_eq!(transition(Recording, Cancel), (Idle, Discard));
        assert_eq!(transition(Transcribing, Cancel), (Idle, Discard));
        assert_eq!(transition(Idle, Cancel), (Idle, Nothing));
    }

    #[test]
    fn second_key_down_is_ignored_never_queued() {
        assert_eq!(transition(Recording, HotkeyDown), (Recording, Nothing));
        assert_eq!(transition(Transcribing, HotkeyDown), (Transcribing, Busy));
        assert_eq!(transition(Injecting, HotkeyDown), (Injecting, Busy));
    }

    #[test]
    fn stray_events_in_idle_do_nothing() {
        assert_eq!(transition(Idle, HotkeyUp), (Idle, Nothing));
        assert_eq!(transition(Idle, LengthCapReached), (Idle, Nothing));
    }

    #[test]
    fn every_state_and_event_pair_is_defined() {
        for s in [Idle, Recording, Transcribing, Injecting] {
            for e in [HotkeyDown, HotkeyUp, Cancel, LengthCapReached] {
                let _ = transition(s, e);
            }
        }
    }

    fn out() -> settings::Output {
        settings::Output::default()
    }

    #[test]
    fn postprocess_trailing_space_and_trim() {
        assert_eq!(postprocess("  hello world ", &out(), false), "hello world ");
        let mut o = out();
        o.trailing_space = false;
        assert_eq!(postprocess("hello", &o, false), "hello");
    }

    #[test]
    fn postprocess_dictionary_is_whole_word_case_insensitive() {
        let mut o = out();
        o.dictionary = vec![settings::Replacement {
            from: "kubernetes".into(),
            to: "Kubernetes".into(),
        }];
        assert_eq!(
            postprocess("Deploy to Kubernetes and kubernetes.", &o, false),
            "Deploy to Kubernetes and Kubernetes. "
        );
        assert_eq!(
            postprocess("kubernetesish stays", &o, false),
            "kubernetesish stays "
        );
    }

    #[test]
    fn postprocess_terminal_collapses_newlines() {
        assert_eq!(
            postprocess("ls -la\ncd foo", &out(), true),
            "ls -la cd foo "
        );
        assert_eq!(
            postprocess("ls -la\ncd foo", &out(), false),
            "ls -la\ncd foo "
        );
    }

    #[test]
    fn postprocess_capitalize_first() {
        let mut o = out();
        o.capitalize_first = true;
        assert_eq!(postprocess("hello", &o, false), "Hello ");
        assert_eq!(postprocess("", &o, false), "");
    }
}
