//! Clipboard writes, marked private so transcripts stay out of clipboard history and cloud
//! sync, plus save/restore around synthesised pastes.
//!
//! Restore happens on *evidence that the target read the clipboard*, never on a fixed timer.
//! A fixed delay is a guess, and tuning it upward only moves the failure threshold — see
//! docs/TEXT-INJECTION.md.

pub struct Guard {
    // Previous contents, restored when the target's read is observed or the bound expires.
}

/// Windows: attaches ExcludeClipboardContentFromMonitorProcessing, CanIncludeInClipboardHistory=0,
/// CanUploadToCloudClipboard=0 and Clipboard Viewer Ignore.
/// macOS: sets org.nspasteboard.ConcealedType.
/// Both are cooperative hints. The UI says so rather than implying enforcement.
pub fn set_private(_text: &str) -> anyhow::Result<()> {
    todo!()
}

/// Saves current contents and returns a guard. On timeout the guard leaves the transcript in
/// place rather than restoring over it: losing the old clipboard is bad, losing the
/// transcript is worse.
pub fn save() -> anyhow::Result<Guard> {
    todo!()
}
