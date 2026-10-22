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

use crate::cues;
use tauri::Emitter;

use crate::commands::{InsertionResultEvent, PipelineStateDto};
use crate::{audio, clipboard, context, engine, history, inject, panel, settings, tray};

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
    pub injector: Arc<dyn inject::TextInjector>,
    pub app: tauri::AppHandle,
    pub paused: Arc<AtomicBool>,
}

pub struct Handle {
    tx: crossbeam_channel::Sender<Event>,
}

impl Handle {
    /// A handle with no thread behind it, for tests that need an `AppState` but never
    /// dictate. Every `send` is dropped with the usual warning.
    #[doc(hidden)]
    pub fn disconnected() -> Self {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        Self { tx }
    }

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
    // When the start cue is due: `minHoldMs` after key-down, so a tap that will be
    // discarded stays silent. None once played or once the press is over.
    let mut cue_due: Option<Instant> = None;
    // True while the engine has a live streaming session for this recording.
    let mut streaming = false;
    let mut last_state_emit = Instant::now();
    // Input level since the last `vox://level`, for the overlay's ring (~20 Hz).
    let mut level = LevelMeter::default();
    // This dictation's recognition hints (docs/CONTEXT.md), held so the batch fallback can
    // use them if streaming did not start, and counted into the history row. Dropped with
    // the dictation; never logged.
    let mut terms: Vec<String> = Vec::new();

    loop {
        // While recording, poll so audio is drained and the cap is enforced.
        let event = if state == State::Recording {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(e) => e,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if let Some(c) = capture.as_mut() {
                        let fresh = c.drain();
                        level.feed(&fresh);
                        if streaming && !fresh.is_empty() {
                            deps.engine.stream_push(fresh);
                        }
                    }
                    if let Some(rms) = level.take_if_due() {
                        emit_level(&deps.app, rms);
                    }
                    if cue_due.is_some_and(|due| Instant::now() >= due) {
                        cue_due = None;
                        cues::play(cues::Cue::Start);
                    }
                    if last_state_emit.elapsed() >= Duration::from_millis(500) {
                        last_state_emit = Instant::now();
                        emit_state(
                            &deps.app,
                            PipelineStateDto::Recording {
                                elapsed_ms: started
                                    .map(|s| s.elapsed().as_millis() as u64)
                                    .unwrap_or(0),
                                long_form: false,
                            },
                        );
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
            tracing::info!("hotkey ignored: dictation is paused");
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
                        last_state_emit = Instant::now();
                        tray::set_state(&deps.app, tray::IconState::Recording);
                        // The cue plays once the press has lasted the minimum hold, from
                        // the poll loop: a tap that will be discarded is silent, and the
                        // cue never delays the first syllable (docs/HOTKEYS.md).
                        let s = deps.settings.read();
                        cue_due = s.ui.sound_cues.then(|| {
                            Instant::now() + Duration::from_millis(u64::from(s.hotkey.min_hold_ms))
                        });
                        drop(s);
                        emit_state(
                            &deps.app,
                            PipelineStateDto::Recording {
                                elapsed_ms: 0,
                                long_form: false,
                            },
                        );
                    }
                    Err(e) => {
                        tracing::warn!("audio start failed: {e}");
                        notify(&deps.app, crate::permissions::MSG_MICROPHONE);
                        state = State::Idle;
                        tray::set_state(&deps.app, tray::IconState::Attention);
                        emit_state(&deps.app, PipelineStateDto::Idle);
                        continue;
                    }
                }
                // Open the streaming session so the engine works while the user speaks.
                // With the focused-field setting off this happens before the target is
                // captured; with it on, the field has to be read first so the hints can
                // go in at session start. With learned terms turned on, those and the
                // dictionary are hints by themselves and need no field
                // (context::hints_for).
                let (hint, read_field, learned, dictionary, vocabulary_switches) = {
                    let s = deps.settings.read();
                    (
                        language_hint(&s),
                        s.privacy.read_focused_field,
                        context::learned_terms(&s, &deps.history),
                        context::dictionary_terms(&s),
                        s.learning.apply_learned_terms,
                    )
                };
                terms = Vec::new();
                if !read_field {
                    terms = context::hints_for(
                        learned.clone(),
                        dictionary.clone(),
                        Vec::new(),
                        vocabulary_switches,
                    );
                    if !terms.is_empty() {
                        tracing::info!(
                            "context: {} hints sent, {} learned, {} from the dictionary, the field not read",
                            terms.len(),
                            learned.len(),
                            dictionary.len()
                        );
                    }
                    streaming = start_stream(&deps, hint.clone(), terms.clone());
                }
                target = match deps.injector.capture_target() {
                    Ok(t) => Some(t),
                    Err(e) => {
                        tracing::warn!("capture_target failed: {e}");
                        None
                    }
                };
                if read_field {
                    let t0 = Instant::now();
                    let (field, note) = match target.as_ref() {
                        Some(t) => field_hints(t),
                        None => (Vec::new(), "no target"),
                    };
                    let from_field = field.len();
                    let from_learned = learned.len();
                    terms = context::hints_for(learned, dictionary, field, vocabulary_switches);
                    tracing::info!(
                        "context: {} hints sent, {} learned, {} candidates from the field ({}), read in {} ms",
                        terms.len(),
                        from_learned,
                        from_field,
                        note,
                        t0.elapsed().as_millis()
                    );
                    streaming = start_stream(&deps, hint, terms.clone());
                }
                // The overlay goes up once the target is known so it can sit by the caret;
                // the tray icon and the cue have already answered "is it on?" by now.
                if deps.settings.read().ui.level_overlay {
                    let anchor = target.as_ref().and_then(caret_anchor);
                    panel::show_overlay(&deps.app, anchor);
                    level = LevelMeter::default();
                    emit_state(
                        &deps.app,
                        PipelineStateDto::Recording {
                            elapsed_ms: started
                                .map(|s| s.elapsed().as_millis() as u64)
                                .unwrap_or(0),
                            long_form: false,
                        },
                    );
                }
            }
            Action::Discard => {
                cue_due = None;
                tracing::info!(
                    "cancelled after {} ms; nothing transcribed or stored",
                    started.map(|s| s.elapsed().as_millis()).unwrap_or(0)
                );
                if streaming {
                    deps.engine.stream_cancel();
                    streaming = false;
                }
                capture = None;
                target = None;
                started = None;
                go_idle(&deps);
            }
            Action::Finish { capped } => {
                let held_ms = started.map(|s| s.elapsed().as_millis() as u32).unwrap_or(0);
                let min_hold = deps.settings.read().hotkey.min_hold_ms;
                let cap = capture.take();
                let tgt = target.take();
                started = None;
                // A press that ends just past the minimum, before the poll loop got to the
                // start cue, gets the stop cue only.
                cue_due = None;
                if held_ms < min_hold || cap.is_none() {
                    // A brush of the key. Nothing recorded, nothing written.
                    tracing::info!("discarded: held {held_ms} ms, under the {min_hold} ms minimum");
                    if streaming {
                        deps.engine.stream_cancel();
                        streaming = false;
                    }
                    state = State::Idle;
                    go_idle(&deps);
                    continue;
                }
                tray::set_state(&deps.app, tray::IconState::Transcribing);
                if deps.settings.read().ui.sound_cues {
                    cues::play(cues::Cue::Stop);
                }
                emit_state(&deps.app, PipelineStateDto::Transcribing);
                if capped {
                    let minutes = deps.settings.read().audio.max_recording_sec / 60;
                    notify(
                        &deps.app,
                        &format!("Stopped at {minutes} minutes. Transcribed what was recorded."),
                    );
                }
                let cancelled = finish(
                    &deps,
                    cap.expect("checked"),
                    tgt,
                    held_ms,
                    streaming,
                    std::mem::take(&mut terms),
                    &rx,
                );
                let _ = cancelled;
                streaming = false;
                state = State::Idle;
                go_idle(&deps);
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
    terms: Vec<String>,
    rx: &crossbeam_channel::Receiver<Event>,
) -> bool {
    let hints = terms.len() as u32;
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
        tracing::info!("no speech in {held_ms} ms of audio; nothing stored");
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
        deps.engine.transcribe(pcm, hint, terms)
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
    emit_state(&deps.app, PipelineStateDto::Injecting);

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
                tracing::info!(
                    "refused: {}; nothing on the clipboard, nothing stored",
                    reason.code()
                );
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

    // A verified insertion gets a correction watch (docs/LEARNING.md). Off the critical
    // path; a refused or unverified insertion never reaches this arm.
    if let (inject::InjectionOutcome::Inserted { .. }, Some(t)) = (&outcome, target.as_ref()) {
        crate::learning::watch_after_insertion(deps, t, &text);
    }

    let mut entry_id = 0;
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
            context_terms: hints,
        };
        match deps.history.insert(&entry) {
            Ok(id) => entry_id = id,
            Err(e) => tracing::warn!("history insert failed: {e}"),
        }
        let _ = deps
            .history
            .prune(history_cfg.max_items, history_cfg.max_days);
    }
    let _ = deps.app.emit(
        "vox://insertion-result",
        InsertionResultEvent {
            outcome: crate::commands::outcome_dto(&outcome, latency_ms),
            entry_id,
        },
    );
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

/// `vox://state`, for the history panel's live row. Cheap and fire-and-forget; the UI is
/// never on the dictation path.
fn emit_state(app: &tauri::AppHandle, state: PipelineStateDto) {
    let _ = app.emit("vox://state", state);
}

/// Opens the engine's streaming session with this dictation's hints. False means the
/// batch path will transcribe the whole clip at release.
fn start_stream(deps: &Deps, hint: engine::LanguageHint, terms: Vec<String>) -> bool {
    match deps.engine.stream_start(hint, terms) {
        Ok(()) => true,
        Err(e) => {
            tracing::info!("streaming unavailable, batch transcription: {e}");
            false
        }
    }
}

/// Hints from the focused field, and one word for the log about how the read went.
#[cfg(target_os = "macos")]
fn field_hints(target: &InjectionTarget) -> (Vec<String>, &'static str) {
    let Some(el) = target.element.as_ref() else {
        return (Vec::new(), "no focused element");
    };
    match context::read_field(el) {
        Ok((window, caret)) => match context::field_terms(&window, caret) {
            Ok(terms) => (terms, "ok"),
            Err(why) => (Vec::new(), why),
        },
        Err(why) => (Vec::new(), why),
    }
}

#[cfg(not(target_os = "macos"))]
fn field_hints(_target: &InjectionTarget) -> (Vec<String>, &'static str) {
    (Vec::new(), "unsupported platform")
}

/// `vox://level`, only while recording (docs/UI-CONTRACT.md). The overlay's ring.
fn emit_level(app: &tauri::AppHandle, rms: f32) {
    let _ = app.emit("vox://level", serde_json::json!({ "rms": rms }));
}

/// Back to Idle: tray, state event, and the overlay comes down the moment the text is
/// placed (or the dictation is dropped). Every path out of a dictation ends here.
fn go_idle(deps: &Deps) {
    tray::set_state(&deps.app, tray::IconState::Idle);
    panel::hide_overlay(&deps.app);
    emit_state(&deps.app, PipelineStateDto::Idle);
}

#[cfg(target_os = "macos")]
fn caret_anchor(target: &InjectionTarget) -> Option<panel::Anchor> {
    target
        .element
        .as_ref()
        .and_then(inject::macos::caret_bounds)
}

#[cfg(not(target_os = "macos"))]
fn caret_anchor(_target: &InjectionTarget) -> Option<panel::Anchor> {
    None
}

/// RMS of the samples since the last emit, scaled so ordinary speech fills most of the
/// meter (the same scale the onboarding microphone test uses), at most every 50 ms.
#[derive(Default)]
struct LevelMeter {
    sum_sq: f64,
    n: usize,
    last: Option<Instant>,
}

impl LevelMeter {
    const INTERVAL: Duration = Duration::from_millis(50);
    const GAIN: f32 = 6.0;

    fn feed(&mut self, pcm: &[f32]) {
        self.sum_sq += pcm
            .iter()
            .map(|x| f64::from(*x) * f64::from(*x))
            .sum::<f64>();
        self.n += pcm.len();
    }

    /// The scaled level if 50 ms have passed and any audio arrived; resets either way when
    /// it fires.
    fn take_if_due(&mut self) -> Option<f32> {
        let due = self.last.is_none_or(|t| t.elapsed() >= Self::INTERVAL);
        if !due || self.n == 0 {
            return None;
        }
        let rms = (self.sum_sq / self.n as f64).sqrt() as f32;
        self.sum_sq = 0.0;
        self.n = 0;
        self.last = Some(Instant::now());
        Some((rms * Self::GAIN).min(1.0))
    }
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
    fn level_meter_scales_rms_and_throttles() {
        let mut m = LevelMeter::default();
        assert_eq!(m.take_if_due(), None, "nothing fed, nothing emitted");
        m.feed(&[0.1; 1600]);
        let first = m.take_if_due().expect("first emit is immediate");
        assert!((first - 0.6).abs() < 1e-3, "0.1 rms × 6 = {first}");
        m.feed(&[0.5; 100]);
        assert_eq!(m.take_if_due(), None, "within 50 ms of the last emit");
        m.last = Some(Instant::now() - Duration::from_millis(60));
        m.feed(&[0.5; 100]);
        assert_eq!(m.take_if_due(), Some(1.0), "clamped to 1.0");
    }

    #[test]
    fn postprocess_capitalize_first() {
        let mut o = out();
        o.capitalize_first = true;
        assert_eq!(postprocess("hello", &o, false), "Hello ");
        assert_eq!(postprocess("", &o, false), "");
    }
}
