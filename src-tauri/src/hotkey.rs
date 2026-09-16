//! Global push-to-talk capture.
//!
//! Uses `keytap` because the default binding is "hold the RIGHT option key", which needs raw
//! key-down and key-up events, left/right modifier fidelity, and modifier-only bindings —
//! none of which OS shortcut registration (or the `global-hotkey` crate) can express.
//! See docs/HOTKEYS.md and docs/adr/0003.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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

/// Matches a chord against the raw key stream. Pure, so it is testable without a tap.
#[derive(Debug)]
pub struct Matcher {
    keys: Vec<String>,
    cancel: String,
    mode: Mode,
    held: HashSet<String>,
    active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    End,
    Cancel,
}

impl Matcher {
    pub fn new(hotkey: &settings::Hotkey) -> Self {
        Self {
            keys: hotkey.keys.clone(),
            cancel: hotkey.cancel_key.clone(),
            mode: hotkey.mode,
            held: HashSet::new(),
            active: false,
        }
    }

    fn all_down(&self) -> bool {
        !self.keys.is_empty() && self.keys.iter().all(|k| self.held.contains(k))
    }

    /// Feed one event. `name` is keytap's `Key` Debug name, e.g. "AltRight".
    pub fn feed(&mut self, name: &str, down: bool) -> Option<Action> {
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
            Mode::Hold | Mode::DoubleTapHold => {
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
    }
}

pub fn key_name(key: Key) -> String {
    format!("{key:?}")
}

/// Creates the tap synchronously so a permission failure is reported to the caller, then
/// listens on its own thread. keytap fails fast with a typed error when the OS denies
/// permission rather than silently producing no events — the caller badges the tray.
pub fn spawn(
    settings: Arc<RwLock<settings::Settings>>,
    pipeline: Arc<pipeline::Handle>,
    paused: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let tap = Tap::new().map_err(|e| anyhow::anyhow!("keytap: {e}"))?;
    let hotkey = settings.read().hotkey.clone();
    std::thread::Builder::new()
        .name("vox-hotkey".into())
        .spawn(move || {
            let mut matcher = Matcher::new(&hotkey);
            for event in tap.iter() {
                let (name, down) = match event.kind {
                    EventKind::KeyDown(k) => (key_name(k), true),
                    EventKind::KeyUp(k) => (key_name(k), false),
                    EventKind::KeyRepeat(_) => continue,
                };
                let Some(action) = matcher.feed(&name, down) else {
                    continue;
                };
                if paused.load(Ordering::Relaxed) {
                    matcher.reset();
                    continue;
                }
                match action {
                    Action::Start => pipeline.send(pipeline::Event::HotkeyDown),
                    Action::End => pipeline.send(pipeline::Event::HotkeyUp),
                    Action::Cancel => {
                        matcher.reset();
                        pipeline.send(pipeline::Event::Cancel)
                    }
                }
            }
            tracing::warn!("hotkey tap ended");
        })?;
    Ok(())
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
}
