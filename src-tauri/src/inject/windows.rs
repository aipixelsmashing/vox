//! Windows injection chain, built on `win-text-inject`. See docs/TEXT-INJECTION.md#windows.
//!
//! Four defects of the naive approach, all handled here:
//!   1. CF_UNICODETEXT alone opts the transcript into clipboard history and the cloud
//!      clipboard — four opt-out formats are attached instead.
//!   2. A held modifier corrupts the synthesised chord, which push-to-talk guarantees —
//!      modifiers are sanitised first.
//!   3. UIPI blocks injection into elevated windows and reports nothing at all — detected up
//!      front and reported to the user.
//!   4. Restoring the clipboard on a timer races the target's asynchronous read — delayed
//!      rendering removes the guess entirely.

use super::{Error, InjectionOutcome, TextInjector};
use crate::pipeline::InjectionTarget;

pub struct WindowsInjector;

impl WindowsInjector {
    pub fn new() -> Self {
        Self
    }
}

/// Ctrl+V is a no-op or means something else in these; use Ctrl+Shift+V.
const TERMINAL_WINDOW_CLASSES: &[&str] = &[
    "CASCADIA_HOSTING_WINDOW_CLASS", // Windows Terminal
    "VirtualConsoleClass",           // ConEmu
    "mintty",
    "Alacritty",
    "org.wezfurlong.wezterm",
];

impl TextInjector for WindowsInjector {
    fn capture_target(&self) -> Result<InjectionTarget, Error> {
        todo!()
    }
    fn inject(&self, _text: &str, _target: &InjectionTarget) -> Result<InjectionOutcome, Error> {
        // UI Automation is deliberately not used: ValuePattern.SetValue replaces the entire
        // field rather than inserting at the caret, and TextPattern is read-only. TSF
        // insertion would need a signed in-proc COM DLL loaded into every target app — a
        // separate project and a much larger security surface.
        todo!()
    }
}
