//! Global push-to-talk capture.
//!
//! Uses `keytap` because the default binding is "hold the RIGHT option key", which needs raw
//! key-down and key-up events, left/right modifier fidelity, and modifier-only bindings —
//! none of which OS shortcut registration (or the `global-hotkey` crate) can express.
//! See docs/HOTKEYS.md and docs/adr/0003.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use keytap::{EventKind, Key, Tap};
use parking_lot::RwLock;

use crate::{pipeline, settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Record from key-down to key-up.
    Hold,
    /// First complete press starts, next press stops. Exists because sustained key pressure
    /// is an accessibility barrier, not as a convenience.
    Toggle,
    /// Two quick taps then hold. For users who rest fingers on modifier keys. M3.
    DoubleTapHold,
}

/// A press released within this long is a tap, for the lock gesture.
pub const TAP_MAX: Duration = Duration::from_millis(300);
/// The second tap has to go down within this long of the first coming up.
pub const DOUBLE_TAP_GAP: Duration = Duration::from_millis(400);

/// Matches a chord against the raw key stream. Pure, so it is testable without a tap.
///
/// In `hold` mode there is one gesture beyond hold-and-release (docs/HOTKEYS.md, "The
/// lock"): two quick taps lock a session that runs hands-free until a single tap ends it.
/// The first tap is an ordinary short press (Start, End; the pipeline discards it as under
/// the minimum hold). The second tap's down is an ordinary Start, and its release, being
/// quick and close on the first, is `Lock` instead of End: recording carries on. The next
/// press ends it on its down, and that press's release is silent.
#[derive(Debug)]
pub struct Matcher {
    keys: Vec<String>,
    cancel: String,
    mode: Mode,
    held: HashSet<String>,
    active: bool,
    /// A session is running hands-free; the next press ends it.
    locked: bool,
    /// When the current press went down.
    pressed_at: Option<Instant>,
    /// When the last tap came up, for the double-tap window.
    last_tap_up: Option<Instant>,
    /// The current press went down quickly after a tap: its release locks if it is a tap.
    arming: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    End,
    Cancel,
    /// The chord was released but the session continues, locked. Nothing goes to the
    /// pipeline; the next `End` does.
    Lock,
}

impl Matcher {
    pub fn new(hotkey: &settings::Hotkey) -> Self {
        Self {
            keys: hotkey.keys.clone(),
            cancel: hotkey.cancel_key.clone(),
            mode: hotkey.mode,
            held: HashSet::new(),
            active: false,
            locked: false,
            pressed_at: None,
            last_tap_up: None,
            arming: false,
        }
    }

    fn all_down(&self) -> bool {
        !self.keys.is_empty() && self.keys.iter().all(|k| self.held.contains(k))
    }

    /// Feed one event. `name` is keytap's `Key` Debug name, e.g. "AltRight".
    pub fn feed(&mut self, name: &str, down: bool) -> Option<Action> {
        self.feed_at(name, down, Instant::now())
    }

    /// [`Matcher::feed`] with the clock supplied, so the taps can be tested.
    pub fn feed_at(&mut self, name: &str, down: bool, now: Instant) -> Option<Action> {
        if down && name == self.cancel {
            return self
                .active
                .then_some(Action::Cancel)
                .or(Some(Action::Cancel));
        }
        let was_all = self.all_down();
        if down {
            self.held.insert(name.to_string());
        } else {
            self.held.remove(name);
        }
        let now_all = self.all_down();
        match self.mode {
            Mode::Hold => {
                if !was_all && now_all {
                    if self.locked {
                        // The stop tap. Its release is silent.
                        self.locked = false;
                        self.active = false;
                        self.pressed_at = None;
                        self.last_tap_up = None;
                        self.arming = false;
                        return Some(Action::End);
                    }
                    if self.active {
                        return None;
                    }
                    self.active = true;
                    self.pressed_at = Some(now);
                    self.arming = self
                        .last_tap_up
                        .is_some_and(|up| now.duration_since(up) <= DOUBLE_TAP_GAP);
                    Some(Action::Start)
                } else if was_all && !now_all && self.active && !self.locked {
                    let tap = self
                        .pressed_at
                        .is_some_and(|down| now.duration_since(down) <= TAP_MAX);
                    self.pressed_at = None;
                    if tap && self.arming {
                        self.arming = false;
                        self.last_tap_up = None;
                        self.locked = true;
                        return Some(Action::Lock);
                    }
                    self.arming = false;
                    self.active = false;
                    self.last_tap_up = tap.then_some(now);
                    Some(Action::End)
                } else {
                    None
                }
            }
            // Two taps then hold is how this mode starts, so no double-tap lock here.
            Mode::DoubleTapHold => {
                if !was_all && now_all && !self.active {
                    self.active = true;
                    Some(Action::Start)
                } else if was_all && !now_all && self.active {
                    self.active = false;
                    Some(Action::End)
                } else {
                    None
                }
            }
            Mode::Toggle => {
                if !was_all && now_all {
                    self.active = !self.active;
                    Some(if self.active {
                        Action::Start
                    } else {
                        Action::End
                    })
                } else {
                    None
                }
            }
        }
    }

    /// The pipeline told us the session ended on its own (cap, cancel). Forget `active` so the
    /// next press starts fresh instead of reading as a release.
    pub fn reset(&mut self) {
        self.active = false;
        self.locked = false;
        self.pressed_at = None;
        self.last_tap_up = None;
        self.arming = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

/// What macOS does with the Fn (Globe) key on its own: System Settings → Keyboard, "Press
/// 🌐 key to". Anything but Do Nothing fires alongside every Vox press, so the pane says
/// so when Fn is chosen (docs/HOTKEYS.md, "Fn as the key").
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobeSetting {
    DoNothing,
    ChangeInputSource,
    Emoji,
    Dictation,
    /// Not set, so macOS's own default applies, which is not Do Nothing; or unreadable.
    Unknown,
}

/// Reads `AppleFnUsageType` from the HIToolbox defaults. A short-lived `defaults` process;
/// only ever run when the user picks or has Fn, never on the dictation path.
#[cfg(target_os = "macos")]
pub fn globe_key_setting() -> GlobeSetting {
    let out = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
        .output();
    let Ok(out) = out else {
        return GlobeSetting::Unknown;
    };
    match String::from_utf8_lossy(&out.stdout).trim() {
        "0" => GlobeSetting::DoNothing,
        "1" => GlobeSetting::ChangeInputSource,
        "2" => GlobeSetting::Emoji,
        "3" => GlobeSetting::Dictation,
        _ => GlobeSetting::Unknown,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn globe_key_setting() -> GlobeSetting {
    GlobeSetting::Unknown
}

/// keytap's name for the macOS Fn key.
pub const FN_KEY: &str = "Function";

pub fn key_name(key: Key) -> String {
    format!("{key:?}")
}

/// A pending "press the keys you want" request from Settings. While one is set, the next
/// chord the user presses and fully releases is sent here instead of reaching the matcher.
static CAPTURE: parking_lot::Mutex<Option<crossbeam_channel::Sender<Vec<String>>>> =
    parking_lot::Mutex::new(None);

/// Arms a capture; the receiver gets the keys in the order they went down.
pub fn capture_next() -> crossbeam_channel::Receiver<Vec<String>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    *CAPTURE.lock() = Some(tx);
    rx
}

pub fn capture_cancel() {
    *CAPTURE.lock() = None;
}

/// A chord such as `CmdOrCtrl+Shift+V` as groups of keytap key names; any key in a group
/// satisfies it, so either Shift works. The last group is the key that fires it.
pub fn parse_chord(spec: &str) -> Option<Vec<Vec<String>>> {
    let mut groups = Vec::new();
    for part in spec.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        let g: Vec<&str> = match part {
            "CmdOrCtrl" | "CommandOrControl" => {
                if cfg!(target_os = "macos") {
                    vec!["MetaLeft", "MetaRight"]
                } else {
                    vec!["ControlLeft", "ControlRight"]
                }
            }
            "Cmd" | "Command" | "Meta" | "Super" => vec!["MetaLeft", "MetaRight"],
            "Ctrl" | "Control" => vec!["ControlLeft", "ControlRight"],
            "Shift" => vec!["ShiftLeft", "ShiftRight"],
            "Alt" | "Option" => vec!["AltLeft", "AltRight"],
            other => vec![other],
        };
        groups.push(
            g.into_iter()
                .map(|k| {
                    if k.len() == 1 {
                        k.to_ascii_uppercase()
                    } else {
                        k.to_string()
                    }
                })
                .collect(),
        );
    }
    (groups.len() >= 2).then_some(groups)
}

/// Fires once per press of the chord's final key while every other group is held.
pub struct ChordWatch {
    groups: Vec<Vec<String>>,
    held: HashSet<String>,
}

impl ChordWatch {
    pub fn new(groups: Vec<Vec<String>>) -> Self {
        Self {
            groups,
            held: HashSet::new(),
        }
    }

    pub fn feed(&mut self, name: &str, down: bool) -> bool {
        if !down {
            self.held.remove(name);
            return false;
        }
        self.held.insert(name.to_string());
        let Some(last) = self.groups.last() else {
            return false;
        };
        last.iter().any(|k| k == name)
            && self
                .groups
                .iter()
                .all(|g| g.iter().any(|k| self.held.contains(k)))
    }
}

/// Creates the tap synchronously so a permission failure is reported to the caller, then
/// listens on its own thread. keytap fails fast with a typed error when the OS denies
/// permission rather than silently producing no events — the caller badges the tray.
pub fn spawn(
    settings: Arc<RwLock<settings::Settings>>,
    pipeline: Arc<pipeline::Handle>,
    paused: Arc<AtomicBool>,
    on_panel: Arc<dyn Fn() + Send + Sync>,
) -> anyhow::Result<()> {
    let tap = Tap::new().map_err(|e| anyhow::anyhow!("keytap: {e}"))?;
    let hotkey = settings.read().hotkey.clone();
    let chord_for = |s: &settings::Settings| {
        s.history
            .panel_hotkey
            .as_deref()
            .and_then(parse_chord)
            .map(ChordWatch::new)
    };
    let mut panel_chord = chord_for(&settings.read());
    let mut seen_gen = settings::SETTINGS_GEN.load(Ordering::Acquire);
    // True from Start to End. The cancel tap reads it to decide whether the cancel key belongs
    // to us (swallow it, send Cancel) or to the foreground app (let it through).
    let dictating = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "macos")]
    match cancel_tap::macos_keycode(&hotkey.cancel_key) {
        Some(code) => {
            if let Err(e) = cancel_tap::install(code, dictating.clone(), pipeline.clone()) {
                tracing::warn!(
                    "cancel key is observed but not swallowed; the foreground app will also see it: {e}"
                );
            }
        }
        None => tracing::warn!(
            "no macOS keycode for cancel key {:?}; it is observed but not swallowed",
            hotkey.cancel_key
        ),
    }
    std::thread::Builder::new()
        .name("vox-hotkey".into())
        .spawn(move || {
            let mut matcher = Matcher::new(&hotkey);
            // For Settings' "press the keys": what is down right now, and the largest chord
            // seen since the first key went down.
            let mut capture_seq: Vec<String> = Vec::new();
            let mut capture_down: HashSet<String> = HashSet::new();
            for event in tap.iter() {
                let (name, down) = match event.kind {
                    EventKind::KeyDown(k) => (key_name(k), true),
                    EventKind::KeyUp(k) => (key_name(k), false),
                    EventKind::KeyRepeat(_) => continue,
                };

                // Settings changed: rebuild what this thread caches, without a restart.
                let gen = settings::SETTINGS_GEN.load(Ordering::Acquire);
                if gen != seen_gen {
                    seen_gen = gen;
                    let s = settings.read();
                    matcher = Matcher::new(&s.hotkey);
                    panel_chord = chord_for(&s);
                    dictating.store(false, Ordering::Release);
                }

                if CAPTURE.lock().is_some() {
                    if down {
                        capture_down.insert(name.clone());
                        if !capture_seq.contains(&name) {
                            capture_seq.push(name.clone());
                        }
                    } else {
                        capture_down.remove(&name);
                        if capture_down.is_empty() && !capture_seq.is_empty() {
                            let chord = std::mem::take(&mut capture_seq);
                            if let Some(tx) = CAPTURE.lock().take() {
                                let _ = tx.send(chord);
                            }
                        }
                    }
                    continue;
                }
                if let Some(chord) = panel_chord.as_mut() {
                    if chord.feed(&name, down) && !matcher.is_active() {
                        on_panel();
                        continue;
                    }
                }
                // The cancel tap ended the session without this thread seeing the key.
                if matcher.is_active() && !dictating.load(Ordering::Acquire) {
                    matcher.reset();
                }
                let Some(action) = matcher.feed(&name, down) else {
                    continue;
                };
                if paused.load(Ordering::Relaxed) {
                    matcher.reset();
                    dictating.store(false, Ordering::Release);
                    continue;
                }
                match action {
                    Action::Start => {
                        dictating.store(true, Ordering::Release);
                        pipeline.send(pipeline::Event::HotkeyDown)
                    }
                    Action::Lock => {
                        tracing::info!("hotkey: session locked; a tap ends it");
                    }
                    Action::End => {
                        dictating.store(false, Ordering::Release);
                        pipeline.send(pipeline::Event::HotkeyUp)
                    }
                    Action::Cancel => {
                        matcher.reset();
                        dictating.store(false, Ordering::Release);
                        pipeline.send(pipeline::Event::Cancel)
                    }
                }
            }
            tracing::warn!("hotkey tap ended");
        })?;
    Ok(())
}

/// Swallows the cancel key while a dictation is in progress.
///
/// keytap's tap is listen-only, so the foreground app sees every key we see. While the
/// hotkey modifier is held that makes the cancel key a chord: with right Option held, Escape
/// reaches a Cocoa text view as Option+Escape, which opens the completion menu and can
/// insert a word — exactly what cancelling must never do. This active tap sits ahead of
/// keytap's, drops the cancel key's down, repeats and up while `dictating` is set, and sends
/// Cancel itself. The modifier is never swallowed here; that is `hotkey.consume`, a separate
/// and off-by-default choice (docs/HOTKEYS.md, problems 2 and 4).
#[cfg(target_os = "macos")]
mod cancel_tap {
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRunLoop};
    use objc2_core_graphics::{
        CGEvent, CGEventField, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventTapProxy, CGEventType,
    };

    use crate::pipeline;

    struct Ctx {
        keycode: i64,
        dictating: Arc<AtomicBool>,
        /// A down was swallowed; swallow its repeats and the matching up too, so the app
        /// never sees half a keystroke.
        swallowed_down: AtomicBool,
        pipeline: Arc<pipeline::Handle>,
        /// For re-enabling after the system disables a slow or interrupted tap.
        tap: AtomicPtr<CFMachPort>,
    }

    struct SendPtr(*mut Ctx);
    // SAFETY: the pointer is only dereferenced from the tap thread and the tap callback,
    // and the Ctx is never freed (the tap lives as long as the process).
    unsafe impl Send for SendPtr {}

    /// macOS virtual keycode for a keytap key name. Only the keys that make sense as a cancel
    /// key; anything else is observed but not swallowed.
    pub fn macos_keycode(name: &str) -> Option<i64> {
        match name {
            "Escape" => Some(53),
            "F1" => Some(122),
            "F2" => Some(120),
            "F3" => Some(99),
            "F4" => Some(118),
            "F5" => Some(96),
            "F6" => Some(97),
            "F7" => Some(98),
            "F8" => Some(100),
            "F9" => Some(101),
            "F10" => Some(109),
            "F11" => Some(103),
            "F12" => Some(111),
            _ => None,
        }
    }

    pub fn install(
        keycode: i64,
        dictating: Arc<AtomicBool>,
        pipeline: Arc<pipeline::Handle>,
    ) -> anyhow::Result<()> {
        let ctx = SendPtr(Box::into_raw(Box::new(Ctx {
            keycode,
            dictating,
            swallowed_down: AtomicBool::new(false),
            pipeline,
            tap: AtomicPtr::new(std::ptr::null_mut()),
        })));
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);
        std::thread::Builder::new()
            .name("vox-cancel-tap".into())
            .spawn(move || {
                let ctx = ctx;
                run(ctx.0, ready_tx)
            })?;
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| anyhow::anyhow!("cancel tap thread did not report"))?
            .map_err(anyhow::Error::msg)
    }

    fn run(ctx: *mut Ctx, ready: crossbeam_channel::Sender<Result<(), String>>) {
        let mask = (1u64 << CGEventType::KeyDown.0) | (1u64 << CGEventType::KeyUp.0);
        // SAFETY: the callback matches CGEventTapCallBack and ctx outlives the tap.
        let tap = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::HIDEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(callback),
                ctx as *mut c_void,
            )
        };
        let Some(tap) = tap else {
            let _ = ready.send(Err(
                "CGEventTapCreate returned null (Accessibility or Input Monitoring missing)".into(),
            ));
            return;
        };
        let port: *const CFMachPort = &*tap;
        // SAFETY: ctx is valid; the tap thread is its only writer.
        unsafe { (*ctx).tap.store(port.cast_mut(), Ordering::Release) };
        let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
            let _ = ready.send(Err("CFMachPortCreateRunLoopSource returned null".into()));
            return;
        };
        let Some(run_loop) = CFRunLoop::current() else {
            let _ = ready.send(Err("no current run loop".into()));
            return;
        };
        // SAFETY: reading a CoreFoundation constant.
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        CGEvent::tap_enable(&tap, true);
        let _ = ready.send(Ok(()));
        CFRunLoop::run();
        tracing::warn!("cancel tap run loop ended");
    }

    unsafe extern "C-unwind" fn callback(
        _proxy: CGEventTapProxy,
        ty: CGEventType,
        event: NonNull<CGEvent>,
        info: *mut c_void,
    ) -> *mut CGEvent {
        // SAFETY: info is the Ctx pointer given to tap_create, never freed.
        let ctx = unsafe { &*(info as *const Ctx) };
        if ty.0 == CGEventType::TapDisabledByTimeout.0
            || ty.0 == CGEventType::TapDisabledByUserInput.0
        {
            let tap = ctx.tap.load(Ordering::Acquire);
            if !tap.is_null() {
                // SAFETY: set by the tap thread to a port that lives as long as the process.
                CGEvent::tap_enable(unsafe { &*tap }, true);
            }
            return event.as_ptr();
        }
        // SAFETY: event is a valid CGEvent for the duration of the callback.
        let keycode = CGEvent::integer_value_field(
            Some(unsafe { event.as_ref() }),
            CGEventField::KeyboardEventKeycode,
        );
        if keycode != ctx.keycode {
            return event.as_ptr();
        }
        if ty.0 == CGEventType::KeyDown.0 {
            if ctx.swallowed_down.load(Ordering::Acquire) {
                // Auto-repeat of a swallowed key.
                return std::ptr::null_mut();
            }
            if ctx.dictating.swap(false, Ordering::AcqRel) {
                ctx.swallowed_down.store(true, Ordering::Release);
                ctx.pipeline.send(pipeline::Event::Cancel);
                return std::ptr::null_mut();
            }
        } else if ty.0 == CGEventType::KeyUp.0 && ctx.swallowed_down.swap(false, Ordering::AcqRel) {
            return std::ptr::null_mut();
        }
        event.as_ptr()
    }

    #[cfg(test)]
    mod tests {
        use super::macos_keycode;

        #[test]
        fn escape_has_a_keycode_and_letters_do_not() {
            assert_eq!(macos_keycode("Escape"), Some(53));
            assert_eq!(
                macos_keycode("KeyA"),
                None,
                "letters are never a cancel key"
            );
        }
    }
}

/// True when the active keyboard layout maps right Alt to AltGr, in which case onboarding
/// proposes right Control instead and explains why in one sentence. M3, with onboarding.
pub fn layout_uses_altgr() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(keys: &[&str], mode: Mode) -> settings::Hotkey {
        settings::Hotkey {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            mode,
            ..settings::Hotkey::default()
        }
    }

    #[test]
    fn hold_single_key() {
        let mut m = Matcher::new(&hk(&["AltRight"], Mode::Hold));
        assert_eq!(m.feed("AltRight", true), Some(Action::Start));
        assert_eq!(m.feed("K", true), None);
        assert_eq!(m.feed("K", false), None);
        assert_eq!(m.feed("AltRight", false), Some(Action::End));
        assert_eq!(m.feed("AltLeft", true), None, "left is not right");
    }

    #[test]
    fn two_taps_lock_a_session_and_one_tap_ends_it() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let mut m = Matcher::new(&hk(&["AltRight"], Mode::Hold));
        // First tap: an ordinary short press, which the pipeline discards.
        assert_eq!(m.feed_at("AltRight", true, t0), Some(Action::Start));
        assert_eq!(m.feed_at("AltRight", false, t0 + ms(80)), Some(Action::End));
        // Second tap, 200 ms later: its down starts recording and its release locks.
        assert_eq!(
            m.feed_at("AltRight", true, t0 + ms(280)),
            Some(Action::Start)
        );
        assert_eq!(
            m.feed_at("AltRight", false, t0 + ms(370)),
            Some(Action::Lock)
        );
        assert!(m.is_locked() && m.is_active());
        // Keys typed meanwhile are nothing to us.
        assert_eq!(m.feed_at("K", true, t0 + ms(5000)), None);
        assert_eq!(m.feed_at("K", false, t0 + ms(5050)), None);
        // One tap ends it, on the way down; its release is silent.
        assert_eq!(
            m.feed_at("AltRight", true, t0 + ms(30_000)),
            Some(Action::End)
        );
        assert!(!m.is_locked() && !m.is_active());
        assert_eq!(m.feed_at("AltRight", false, t0 + ms(30_060)), None);
        // And that tap does not arm another lock.
        assert_eq!(
            m.feed_at("AltRight", true, t0 + ms(30_200)),
            Some(Action::Start)
        );
        assert_eq!(
            m.feed_at("AltRight", false, t0 + ms(30_260)),
            Some(Action::End)
        );
    }

    #[test]
    fn a_slow_second_press_or_a_held_second_press_does_not_lock() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let mut m = Matcher::new(&hk(&["AltRight"], Mode::Hold));
        // Tap, then a second press 600 ms later: too slow, an ordinary dictation.
        m.feed_at("AltRight", true, t0);
        m.feed_at("AltRight", false, t0 + ms(80));
        assert_eq!(
            m.feed_at("AltRight", true, t0 + ms(680)),
            Some(Action::Start)
        );
        assert_eq!(
            m.feed_at("AltRight", false, t0 + ms(760)),
            Some(Action::End)
        );
        // Tap, then quickly a real hold: a dictation, not a lock. (Well clear of the tap
        // that ended the previous case, which would otherwise arm this one.)
        m.feed_at("AltRight", true, t0 + ms(2000));
        m.feed_at("AltRight", false, t0 + ms(2080));
        assert_eq!(
            m.feed_at("AltRight", true, t0 + ms(2200)),
            Some(Action::Start)
        );
        assert_eq!(
            m.feed_at("AltRight", false, t0 + ms(4200)),
            Some(Action::End)
        );
        // A hold followed by a tap: the hold was not a tap, so nothing is armed.
        m.feed_at("AltRight", true, t0 + ms(5000));
        m.feed_at("AltRight", false, t0 + ms(7000));
        m.feed_at("AltRight", true, t0 + ms(7100));
        assert_eq!(
            m.feed_at("AltRight", false, t0 + ms(7180)),
            Some(Action::End)
        );
        assert!(!m.is_locked());
    }

    #[test]
    fn escape_ends_a_locked_session_and_the_lock_stays_out_of_other_modes() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let mut m = Matcher::new(&hk(&["AltRight"], Mode::Hold));
        m.feed_at("AltRight", true, t0);
        m.feed_at("AltRight", false, t0 + ms(80));
        m.feed_at("AltRight", true, t0 + ms(200));
        assert_eq!(
            m.feed_at("AltRight", false, t0 + ms(280)),
            Some(Action::Lock)
        );
        assert_eq!(
            m.feed_at("Escape", true, t0 + ms(2000)),
            Some(Action::Cancel)
        );
        m.reset();
        assert!(!m.is_locked());
        assert_eq!(
            m.feed_at("AltRight", true, t0 + ms(3000)),
            Some(Action::Start)
        );

        for mode in [Mode::Toggle, Mode::DoubleTapHold] {
            let mut m = Matcher::new(&hk(&["AltRight"], mode));
            m.feed_at("AltRight", true, t0);
            m.feed_at("AltRight", false, t0 + ms(80));
            m.feed_at("AltRight", true, t0 + ms(200));
            assert_ne!(
                m.feed_at("AltRight", false, t0 + ms(280)),
                Some(Action::Lock),
                "{mode:?} has no lock gesture"
            );
        }
    }

    #[test]
    fn fn_is_a_binding_like_any_other() {
        let mut m = Matcher::new(&hk(&[FN_KEY], Mode::Hold));
        assert_eq!(m.feed("Function", true), Some(Action::Start));
        assert_eq!(m.feed("Function", false), Some(Action::End));
    }

    #[test]
    fn hold_chord_ends_when_any_key_lifts() {
        let mut m = Matcher::new(&hk(&["ControlLeft", "AltLeft"], Mode::Hold));
        assert_eq!(m.feed("ControlLeft", true), None);
        assert_eq!(m.feed("AltLeft", true), Some(Action::Start));
        assert_eq!(m.feed("ControlLeft", false), Some(Action::End));
        assert_eq!(m.feed("AltLeft", false), None);
    }

    #[test]
    fn toggle_alternates() {
        let mut m = Matcher::new(&hk(&["AltRight"], Mode::Toggle));
        assert_eq!(m.feed("AltRight", true), Some(Action::Start));
        assert_eq!(m.feed("AltRight", false), None);
        assert_eq!(m.feed("AltRight", true), Some(Action::End));
        assert_eq!(m.feed("AltRight", false), None);
    }

    #[test]
    fn escape_cancels() {
        let mut m = Matcher::new(&hk(&["AltRight"], Mode::Hold));
        m.feed("AltRight", true);
        assert_eq!(m.feed("Escape", true), Some(Action::Cancel));
        m.reset();
        // The physical key is still down; releasing it must not produce a stray End.
        assert_eq!(m.feed("AltRight", false), None);
    }

    #[test]
    fn panel_chord_parses_and_fires_once_per_press() {
        let groups = parse_chord("CmdOrCtrl+Shift+Space").unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!(
            groups[2],
            vec!["Space".to_string()],
            "the shipped default parses"
        );
        let groups = parse_chord("CmdOrCtrl+Shift+V").unwrap();
        assert_eq!(groups[2], vec!["V".to_string()]);
        assert!(
            groups[1].contains(&"ShiftRight".to_string()),
            "either shift"
        );
        let mut w = ChordWatch::new(groups);
        let meta = if cfg!(target_os = "macos") {
            "MetaLeft"
        } else {
            "ControlLeft"
        };
        assert!(!w.feed(meta, true));
        assert!(!w.feed("ShiftLeft", true));
        assert!(w.feed("V", true), "all held, final key down");
        assert!(!w.feed("V", false));
        assert!(w.feed("V", true), "again while modifiers stay held");
        w.feed("ShiftLeft", false);
        assert!(!w.feed("V", true), "shift released");
        assert!(parse_chord("V").is_none(), "a lone key is not a chord");
    }

    #[test]
    fn external_cancel_then_reset_lets_the_next_press_start() {
        // The cancel tap swallows Escape before keytap sees it, so the matcher only learns
        // the session ended through reset(). After that the release is silent and the next
        // press is a fresh Start, in both modes.
        for mode in [Mode::Hold, Mode::Toggle] {
            let mut m = Matcher::new(&hk(&["AltRight"], mode));
            assert_eq!(m.feed("AltRight", true), Some(Action::Start));
            assert!(m.is_active());
            m.reset();
            assert!(!m.is_active());
            assert_eq!(m.feed("AltRight", false), None);
            assert_eq!(m.feed("AltRight", true), Some(Action::Start));
        }
    }
}
