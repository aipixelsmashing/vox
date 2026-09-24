//! Permission checks and deep links into the right OS settings pane. See docs/PERMISSIONS.md.
//!
//! Rules: ask late, ask once, explain why; degrade visibly rather than dying silently;
//! re-check on wake and window focus, since permissions can be revoked while we run.
//!
//! M1 asks at launch, because nothing works without all three and there is no onboarding
//! window yet (M3). The copy is from the deck in docs/UI-STATES.md.

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

impl Report {
    pub fn all_granted(&self) -> bool {
        [
            self.microphone,
            self.input_monitoring,
            self.accessibility,
            self.input_group,
        ]
        .iter()
        .all(|s| matches!(s, Status::Granted | Status::NotApplicable))
    }

    /// One deck string per missing permission, in the order the user should fix them.
    pub fn missing_messages(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.input_monitoring == Status::Denied {
            out.push(MSG_INPUT_MONITORING);
        }
        if self.accessibility == Status::Denied {
            out.push(MSG_ACCESSIBILITY);
        }
        if self.microphone == Status::Denied {
            out.push(MSG_MICROPHONE);
        }
        if self.input_group == Status::Denied {
            out.push(MSG_INPUT_GROUP);
        }
        out
    }
}

pub const MSG_INPUT_MONITORING: &str =
    "Vox can't see the hotkey. Grant Input Monitoring, then quit and reopen Vox.";
pub const MSG_ACCESSIBILITY: &str =
    "Vox can't place text in other apps. Grant Accessibility, then quit and reopen Vox.";
pub const MSG_MICROPHONE: &str =
    "No microphone access. Turn it on in System Settings → Privacy → Microphone.";
pub const MSG_INPUT_GROUP: &str =
    "Vox can't read the keyboard. Run `sudo usermod -aG input $USER`, then log out and back in.";

#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        pub fn IOHIDCheckAccess(request: u32) -> u32;
        pub fn IOHIDRequestAccess(request: u32) -> bool;
    }
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        pub fn IsSecureEventInputEnabled() -> u8;
    }
    extern "C" {
        // swift/SpeechAnalyzerBridge.swift
        pub fn vox_mic_status() -> i32;
        pub fn vox_mic_request() -> bool;
    }
    pub const IOHID_REQUEST_TYPE_LISTEN: u32 = 1;
    pub const IOHID_ACCESS_GRANTED: u32 = 0;
    pub const AV_AUTH_NOT_DETERMINED: i32 = 0;
    pub const AV_AUTH_AUTHORIZED: i32 = 3;
}

#[cfg(target_os = "macos")]
pub fn check() -> Report {
    // SAFETY: plain queries with no preconditions.
    let (mic, hid) = unsafe {
        (
            mac::vox_mic_status(),
            mac::IOHIDCheckAccess(mac::IOHID_REQUEST_TYPE_LISTEN),
        )
    };
    Report {
        microphone: match mic {
            mac::AV_AUTH_AUTHORIZED => Status::Granted,
            mac::AV_AUTH_NOT_DETERMINED => Status::Denied, // prompt not yet answered
            _ => Status::Denied,
        },
        input_monitoring: if hid == mac::IOHID_ACCESS_GRANTED {
            Status::Granted
        } else {
            Status::Denied
        },
        accessibility: if axuielement::is_process_trusted() {
            Status::Granted
        } else {
            Status::Denied
        },
        input_group: Status::NotApplicable,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn check() -> Report {
    Report {
        microphone: Status::NotApplicable,
        input_monitoring: Status::NotApplicable,
        accessibility: Status::NotApplicable,
        input_group: Status::NotApplicable,
    }
}

/// Shows every system prompt that can be shown, then re-checks. Blocks while the microphone
/// prompt is open, so call it off the main thread. Input Monitoring and Accessibility prompts
/// are asynchronous and their grant needs a restart, which `Report` reports as Denied until
/// then and the caller explains with the deck strings.
#[cfg(target_os = "macos")]
pub fn request_all() -> Report {
    // SAFETY: IOKit call; may show a system dialog.
    let hid_before = unsafe { mac::IOHIDCheckAccess(mac::IOHID_REQUEST_TYPE_LISTEN) };
    if hid_before != mac::IOHID_ACCESS_GRANTED {
        // keytap only checks; this is the call that makes the dialog appear (docs/PERMISSIONS.md).
        unsafe { mac::IOHIDRequestAccess(mac::IOHID_REQUEST_TYPE_LISTEN) };
    }
    if !axuielement::is_process_trusted() {
        let _ = axuielement::is_process_trusted_with_prompt();
    }
    // SAFETY: AVFoundation call via the Swift bridge; blocks until the user answers.
    if unsafe { mac::vox_mic_status() } == mac::AV_AUTH_NOT_DETERMINED {
        unsafe { mac::vox_mic_request() };
    }
    check()
}

#[cfg(not(target_os = "macos"))]
pub fn request_all() -> Report {
    check()
}

/// Opens the exact pane. `which` is one of "input-monitoring", "accessibility", "microphone".
pub fn open_pane(which: &str) -> anyhow::Result<()> {
    let url = match which {
        "input-monitoring" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        other => anyhow::bail!("unknown pane {other}"),
    };
    std::process::Command::new("open").arg(url).status()?;
    Ok(())
}

/// macOS only. Can be stuck on because another app leaked the state; when detected we say so,
/// rather than reporting it as a Vox failure.
#[cfg(target_os = "macos")]
pub fn secure_input_active() -> bool {
    // SAFETY: plain Carbon query.
    unsafe { mac::IsSecureEventInputEnabled() != 0 }
}
