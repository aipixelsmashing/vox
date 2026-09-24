//! Guard tests: the ones that protect the product's promises (docs/TESTING.md#automated).

use vox_lib::inject::{FallbackReason, InjectionOutcome, Method};

/// `InjectionOutcome` has exactly two variants. Adding a third makes this match non-exhaustive
/// and the build fails, which is the point (docs/adr/0005).
#[test]
fn injection_outcome_has_no_probably_worked_variant() {
    fn classify(o: &InjectionOutcome) -> &'static str {
        match o {
            InjectionOutcome::Inserted { .. } => "inserted",
            InjectionOutcome::ClipboardOnly { .. } => "clipboard_only",
        }
    }
    assert_eq!(
        classify(&InjectionOutcome::Inserted {
            method: Method::Accessibility,
            elapsed_ms: 1
        }),
        "inserted"
    );
    assert_eq!(
        classify(&InjectionOutcome::ClipboardOnly {
            reason: FallbackReason::NoTextTarget
        }),
        "clipboard_only"
    );
}

/// telemetry.rs stays empty of anything that could open a socket (docs/adr/0007).
#[test]
fn telemetry_module_has_no_network_code() {
    let src = include_str!("../src/telemetry.rs");
    for needle in [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "TcpStream",
        "UdpSocket",
        "http://",
        "https://",
        "std::net",
    ] {
        assert!(!src.contains(needle), "telemetry.rs mentions {needle}");
    }
}

/// Password contexts never reach the clipboard or history.
#[test]
fn secret_contexts_are_marked() {
    assert!(FallbackReason::SecureInput.is_secret_context());
    assert!(FallbackReason::PasswordField.is_secret_context());
    assert!(!FallbackReason::NoTextTarget.is_secret_context());
}

/// Every fallback reason has deck copy, and none of it is vague.
#[test]
fn every_fallback_reason_has_a_user_message() {
    for r in [
        FallbackReason::NoTextTarget,
        FallbackReason::FocusChanged {
            from: "Slack".into(),
            to: "Chrome".into(),
        },
        FallbackReason::SecureInput,
        FallbackReason::PasswordField,
        FallbackReason::ElevatedTarget,
        FallbackReason::WaylandUnverifiable,
        FallbackReason::MethodFailed(Method::Paste),
    ] {
        let m = r.user_message();
        assert!(!m.is_empty());
        assert!(!m.to_lowercase().contains("sorry"));
        assert!(!m.contains("Permission denied"));
    }
}
