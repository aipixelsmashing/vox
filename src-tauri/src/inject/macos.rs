//! macOS injection chain: secure-input check → accessibility insert → clipboard paste →
//! Unicode key events. See docs/TEXT-INJECTION.md#macos.

use super::{Error, InjectionOutcome, TextInjector};
use crate::pipeline::InjectionTarget;

#[derive(Debug, Clone)]
pub struct ElementRef(/* AXUIElement */);

pub struct MacInjector;

impl MacInjector {
    pub fn new() -> Self {
        Self
    }

    /// 1. AXUIElementCreateSystemWide → kAXFocusedUIElementAttribute
    /// 2. role must be AXTextField / AXTextArea / a settable AXComboBox
    /// 3. set kAXSelectedTextAttribute (replaces selection, inserts at caret when empty)
    /// 4. verify by re-reading kAXSelectedTextRange and confirming the caret advanced
    ///
    /// Step 4 is the point of the whole method: Chromium and Electron apps commonly expose a
    /// web area whose selected-text attribute is not settable, and the call appears to succeed.
    fn try_accessibility(&self, _text: &str) -> Result<bool, Error> {
        todo!()
    }

    /// Cmd+V with residual modifier flags cleared on the synthetic events, then restore the
    /// pasteboard once changeCount moves again or 1500 ms elapses — whichever comes first. On
    /// timeout, leave the transcript in place rather than restoring over it.
    fn try_paste(&self, _text: &str) -> Result<bool, Error> {
        todo!()
    }

    /// CGEventKeyboardSetUnicodeString in chunks of <= 20 UTF-16 units, zero delay.
    fn try_unicode(&self, _text: &str) -> Result<bool, Error> {
        todo!()
    }
}

impl TextInjector for MacInjector {
    fn capture_target(&self) -> Result<InjectionTarget, Error> {
        todo!()
    }
    fn inject(&self, _text: &str, _target: &InjectionTarget) -> Result<InjectionOutcome, Error> {
        // Abort before anything else if IsSecureEventInputEnabled(): a password field is
        // focused (or another app leaked the state). Drop the transcript — do not even put it
        // on the clipboard.
        todo!()
    }
}
