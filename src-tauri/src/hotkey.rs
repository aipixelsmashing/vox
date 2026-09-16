//! Global push-to-talk capture.
//!
//! Uses `keytap` because the default binding is "hold the RIGHT option key", which needs raw
//! key-down and key-up events, left/right modifier fidelity, and modifier-only bindings —
//! none of which OS shortcut registration (or the `global-hotkey` crate) can express.
//! See docs/HOTKEYS.md and docs/adr/0003.

use std::sync::Arc;

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
    /// Two quick taps then hold. For users who rest fingers on modifier keys.
    DoubleTapHold,
}

pub fn spawn(
    _settings: Arc<RwLock<settings::Settings>>,
    _pipeline: Arc<pipeline::Handle>,
) -> anyhow::Result<()> {
    // keytap::Tap fails fast with a typed error when the OS denies permission, rather than
    // silently producing no events — surface that as a badged tray icon, never a dead hotkey.
    //
    // Note: keytap observes, it does not consume. On layouts where right Alt is AltGr the
    // foreground app still sees the modifier. Handled by layout detection at startup plus an
    // optional consume mode implemented directly against CGEventTap / WH_KEYBOARD_LL.
    todo!("build ChordMatcher from settings, forward Start/End as pipeline events")
}

/// True when the active keyboard layout maps right Alt to AltGr, in which case onboarding
/// proposes right Control instead and explains why in one sentence.
pub fn layout_uses_altgr() -> bool {
    todo!()
}
