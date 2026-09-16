//! Linux injection. Two different worlds, and honesty about the second is a design
//! requirement. See docs/TEXT-INJECTION.md#linux and docs/adr/0005.

use super::{Error, InjectionOutcome, TextInjector};
use crate::pipeline::InjectionTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// XTEST. Pass an explicit zero key delay: both xdotool and ydotool default to 12 ms per
    /// key, which makes a 100-character transcript visibly type itself out for over a second.
    X11,
    /// libei via the RemoteDesktop portal — GNOME >= 46, KDE Plasma >= 6.1. No group
    /// membership, no udev rules, Flatpak-compatible. The correct modern answer.
    WaylandPortal,
    /// zwp_virtual_keyboard_v1. wlroots only; refused by KWin and Mutter.
    Wtype,
    /// uinput. Works anywhere, needs a daemon and `input` group membership, not Flatpak-friendly.
    Ydotool,
    /// Nothing verifiable is available. We say "copied — press Ctrl+V" rather than pretending.
    ClipboardOnly,
}

pub struct LinuxInjector {
    backend: Backend,
}

impl LinuxInjector {
    pub fn detect() -> Self {
        // Probe in preference order; record the result so Settings can show which backend is
        // in use and why the others were unavailable.
        todo!()
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }
}

impl TextInjector for LinuxInjector {
    fn capture_target(&self) -> Result<InjectionTarget, Error> {
        todo!()
    }
    fn inject(&self, _text: &str, _target: &InjectionTarget) -> Result<InjectionOutcome, Error> {
        // A compositor can accept synthetic input and route it nowhere. Never return
        // Inserted on a path that cannot be confirmed — the user believing text arrived when
        // it did not is the worst failure mode available to us.
        todo!()
    }
}
