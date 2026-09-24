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

impl Method {
    /// The value stored in the history row (docs/HISTORY.md: ax | paste | unicode).
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Accessibility => "ax",
            Method::Paste => "paste",
            Method::Type => "unicode",
        }
    }

    /// The contract's spelling (`InjectionMethod` in src/lib/contract.ts) for a stored value.
    pub fn contract_name(stored: &str) -> &'static str {
        match stored {
            "ax" | "accessibility" => "accessibility",
            "paste" => "paste",
            _ => "type",
        }
    }
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

impl FallbackReason {
    /// The copy deck string for this situation (docs/UI-STATES.md#copy-deck). Used
    /// identically in the notification and, later, the history row and pane.
    pub fn user_message(&self) -> String {
        match self {
            FallbackReason::NoTextTarget => "Copied. No text field was focused.".into(),
            FallbackReason::FocusChanged { from, to } => {
                format!("Copied instead — you switched from {from} to {to} while speaking.")
            }
            FallbackReason::SecureInput | FallbackReason::PasswordField => {
                "Not inserted — a password field is active. Nothing was saved.".into()
            }
            FallbackReason::ElevatedTarget => {
                "Copied instead — the window is running as administrator. Press Ctrl+Shift+V to paste.".into()
            }
            FallbackReason::WaylandUnverifiable => {
                "Copied — your compositor doesn't allow typing into other apps. Press Ctrl+V.".into()
            }
            FallbackReason::MethodFailed(_) => {
                "Vox couldn't confirm the text arrived. It's on the clipboard — press ⌘V if it's missing.".into()
            }
        }
    }

    /// The contract's `FallbackReason` name for a stored reason code.
    pub fn contract_kind_from_code(code: &str) -> &'static str {
        match code.split(':').next().unwrap_or(code) {
            "no_text_target" => "noTextTarget",
            "focus_changed" => "focusChanged",
            "secure_input" => "secureInput",
            "password_field" => "passwordField",
            "elevated_target" => "elevatedTarget",
            "wayland_unverifiable" => "waylandUnverifiable",
            _ => "methodFailed",
        }
    }

    /// What a history row says after "not inserted —" for a stored reason code
    /// (docs/HISTORY.md: "not inserted — window was elevated").
    pub fn note_from_code(code: &str) -> String {
        let (kind, arg) = match code.split_once(':') {
            Some((k, a)) => (k, Some(a)),
            None => (code, None),
        };
        match kind {
            "no_text_target" => "no text field was focused".into(),
            "focus_changed" => match arg.and_then(|a| a.split_once("->")) {
                Some((from, to)) => format!("you switched from {from} to {to}"),
                None => "you switched apps while speaking".into(),
            },
            "secure_input" | "password_field" => "a password field was active".into(),
            "elevated_target" => "window was elevated".into(),
            "wayland_unverifiable" => "the compositor doesn't allow typing into other apps".into(),
            "method_failed" => match arg {
                Some("paste") => "the paste could not be confirmed".into(),
                Some("ax") => "the field refused the text".into(),
                _ => "the text could not be placed".into(),
            },
            other => other.replace('_', " "),
        }
    }

    /// Password contexts never get the transcript on the clipboard, or in history.
    pub fn is_secret_context(&self) -> bool {
        matches!(
            self,
            FallbackReason::SecureInput | FallbackReason::PasswordField
        )
    }

    /// Short form for the history row's outcome_note.
    pub fn code(&self) -> String {
        match self {
            FallbackReason::NoTextTarget => "no_text_target".into(),
            FallbackReason::FocusChanged { from, to } => format!("focus_changed:{from}->{to}"),
            FallbackReason::SecureInput => "secure_input".into(),
            FallbackReason::PasswordField => "password_field".into(),
            FallbackReason::ElevatedTarget => "elevated_target".into(),
            FallbackReason::WaylandUnverifiable => "wayland_unverifiable".into(),
            FallbackReason::MethodFailed(m) => format!("method_failed:{}", m.as_str()),
        }
    }
}

/// Deliberately has no "probably worked" variant. See docs/adr/0005 — a method that cannot
/// confirm delivery reports ClipboardOnly, so the user finds out immediately and the text is
/// still recoverable. tests/guards.rs asserts this enum stays two-variant.
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

    /// The frontmost application right now, for revalidation: (pid, display name).
    fn frontmost(&self) -> Option<(u32, String)>;
}

pub fn platform_injector(
    settings: std::sync::Arc<parking_lot::RwLock<crate::settings::Settings>>,
) -> Box<dyn TextInjector> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacInjector::new(settings));
    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsInjector::new());
    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxInjector::detect());
}

/// Transcripts longer than this are split on sentence boundaries; some targets truncate very
/// large single insertions.
pub const MAX_ATOMIC_CHARS: usize = 2000;
