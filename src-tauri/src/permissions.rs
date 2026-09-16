//! Permission checks and deep links into the right OS settings pane. See docs/PERMISSIONS.md.
//!
//! Rules: ask late, ask once, explain why; degrade visibly rather than dying silently;
//! re-check on wake and window focus, since permissions can be revoked while we run.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Granted,
    Denied,
    /// Granted, but the process must restart before it takes effect — true for macOS
    /// Accessibility and Input Monitoring.
    NeedsRestart,
    NotApplicable,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub microphone: Status,
    pub input_monitoring: Status,
    pub accessibility: Status,
    /// Linux: membership of the `input` group, needed for evdev key capture.
    pub input_group: Status,
}

pub fn check() -> Report {
    todo!()
}

/// Opens the exact pane, e.g. x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent
pub fn open_pane(_which: &str) -> anyhow::Result<()> {
    todo!()
}

/// macOS only. Can be stuck on because another app leaked the state; when detected we say so,
/// rather than reporting it as a Vox failure.
#[cfg(target_os = "macos")]
pub fn secure_input_active() -> bool {
    todo!("IsSecureEventInputEnabled")
}
