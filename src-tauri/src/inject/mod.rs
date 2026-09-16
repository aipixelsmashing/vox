//! Getting text into somebody else's text field.
//!
//! Read docs/TEXT-INJECTION.md before changing anything here. The short version: the usual
//! save-clipboard / paste / sleep / restore pattern leaks transcripts into clipboard history,
//! breaks when a modifier is held (which push-to-talk guarantees), fails silently against
//! elevated windows, and races the target's asynchronous clipboard read.

use crate::pipeline::InjectionTarget;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Direct write to the focused accessibility element. Preferred: atomic, instant,
    /// immune to modifier state.
    Accessibility,
    /// Clipboard plus a synthesised paste chord, with a verified restore.
    Paste,
    /// Synthesised Unicode key events. Layout-independent, slower, upsets autocomplete.
    Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackReason {
    NoTextTarget,
    FocusChanged { from: String, to: String },
    SecureInput,
    PasswordField,
    ElevatedTarget,
    WaylandUnverifiable,
    MethodFailed(Method),
}

/// Deliberately has no "probably worked" variant. See docs/adr/0005 — a method that cannot
/// confirm delivery reports ClipboardOnly, so the user finds out immediately and the text is
/// still recoverable. A test asserts this enum stays two-variant.
#[derive(Debug, Clone)]
pub enum InjectionOutcome {
    Inserted { method: Method, elapsed_ms: u32 },
    ClipboardOnly { reason: FallbackReason },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no permission to inject: {0}")]
    Permission(String),
    #[error("platform error: {0}")]
    Platform(String),
}

pub trait TextInjector: Send + Sync {
    /// Called at hotkey-down, before the user starts speaking.
    fn capture_target(&self) -> Result<InjectionTarget, Error>;

    /// Called after transcription. Implementations run the platform fallback chain and must
    /// verify delivery before returning `Inserted`.
    fn inject(&self, text: &str, target: &InjectionTarget) -> Result<InjectionOutcome, Error>;
}

pub fn platform_injector() -> Box<dyn TextInjector> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacInjector::new());
    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsInjector::new());
    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxInjector::detect());
}

/// Transcripts longer than this are split on sentence boundaries; some targets truncate very
/// large single insertions.
pub const MAX_ATOMIC_CHARS: usize = 2000;
