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

/// The focused-field context (docs/CONTEXT.md, adr/0017) never reaches a log line: the
/// module that reads and extracts it contains no logging at all, so neither the window nor
/// the terms can be formatted into one by accident.
#[test]
fn context_module_never_logs() {
    let src = include_str!("../src/context.rs");
    for needle in ["tracing::", "println!", "eprintln!", "dbg!", "log::"] {
        assert!(!src.contains(needle), "context.rs mentions {needle}");
    }
}

/// The pipeline logs how many hints it sent, never which. Every log line that mentions
/// context is checked for the variables that would hold the terms.
#[test]
fn pipeline_logs_hint_counts_only() {
    let src = include_str!("../src/pipeline.rs");
    let mut in_log = false;
    for line in src.lines() {
        if line.contains("tracing::") {
            in_log = true;
        }
        if in_log {
            for needle in [
                "{terms",
                "{field",
                "{vocabulary",
                "{window",
                "terms:?",
                "field:?",
            ] {
                assert!(!line.contains(needle), "pipeline.rs logs hint text: {line}");
            }
            if line.trim_end().ends_with(';') {
                in_log = false;
            }
        }
    }
}

/// The Swift bridge writes into Vox's log through `blog`; no such line may interpolate the
/// hints. Saying "hints dropped" in English is fine; `\(terms)` is not.
#[test]
fn bridge_never_logs_context() {
    let swift = include_str!("../swift/SpeechAnalyzerBridge.swift");
    for line in swift.lines() {
        if line.contains("blog(") || line.contains("print(") {
            for needle in [
                "\\(terms",
                "\\(ctx",
                "\\(cContext",
                "contextualStrings",
                "cString: cContext",
            ] {
                assert!(!line.contains(needle), "bridge logs the hints: {line}");
            }
        }
    }
}

/// The history database has a column for how many hints were sent and no column that could
/// hold them. Every column of `transcripts` is on this list; adding one is a deliberate
/// change to this test and to docs/HISTORY.md.
#[test]
fn history_has_no_column_for_context_text() {
    let src = include_str!("../src/history.rs");
    let table = src
        .split("CREATE TABLE IF NOT EXISTS transcripts (")
        .nth(1)
        .and_then(|rest| rest.split(");").next())
        .expect("transcripts table in the schema");
    const ALLOWED: &[&str] = &[
        "id",
        "created_at",
        "text",
        "word_count",
        "duration_ms",
        "latency_ms",
        "engine_id",
        "language",
        "target_app",
        "outcome",
        "outcome_note",
        "method",
        "context_terms",
    ];
    for line in table.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let column = line.split_whitespace().next().unwrap();
        assert!(
            ALLOWED.contains(&column),
            "unexpected transcripts column {column}"
        );
        assert!(
            !column.starts_with("context") || column == "context_terms",
            "{column} could hold the hints"
        );
    }
    assert!(
        table.contains("context_terms INTEGER"),
        "the hint count is a number, not text"
    );
}

/// Reading the user's document is opt-in (adr/0017): off by default, and the flag exists in
/// the settings file so it can be inspected and diffed.
#[test]
fn read_focused_field_is_off_by_default_and_in_the_file() {
    let s = vox_lib::settings::Settings::default();
    assert!(!s.privacy.read_focused_field);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["privacy"]["readFocusedField"],
        serde_json::Value::Bool(false)
    );
}

// ─── Correction capture (docs/LEARNING.md, docs/TESTING.md "Learning tests") ─────────

/// The learning module never logs the text it reads or the forms it stores; the log gets
/// counts. Every log line is checked for the variables that would hold them.
#[test]
fn learning_logs_no_field_text_or_forms() {
    let src = include_str!("../src/learning.rs");
    let mut in_log = false;
    for line in src.lines() {
        if line.contains("tracing::") {
            in_log = true;
        }
        if in_log {
            for needle in [
                "{wrong",
                "{right",
                "{inserted",
                "{observed",
                "{value",
                "{middle",
                "{baseline",
                "{prefix",
                "{suffix",
                "{text",
                "{located",
                "wrong:?",
                "right:?",
                "value:?",
                "middle:?",
                "located:?",
                "{w}",
                "{r}",
                "{w:?",
                "{r:?",
            ] {
                assert!(!line.contains(needle), "learning.rs logs text: {line}");
            }
            if line.trim_end().ends_with(';') {
                in_log = false;
            }
        }
    }
}

/// The watch is registered under the `Inserted` arm and nowhere else, after the
/// secret-context return: a field we refused to insert into is never watched, by
/// construction rather than by a filter afterwards.
#[test]
fn watch_is_registered_only_after_a_verified_insertion() {
    let src = include_str!("../src/pipeline.rs");
    let lines: Vec<&str> = src.lines().collect();
    let calls: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("learning::watch_after_insertion"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(calls.len(), 1, "exactly one call site");
    let i = calls[0];
    let window = lines[i.saturating_sub(3)..i].join("\n");
    assert!(
        window.contains("InjectionOutcome::Inserted"),
        "the watch must sit under the Inserted arm:\n{window}"
    );
    let before = lines[..i].join("\n");
    assert!(
        before.contains("is_secret_context()"),
        "secret contexts must return before the watch is reached"
    );
}

/// docs/TESTING.md: the pairs that must never produce a candidate, against the real system
/// word list, while the two terms must. Without the list nothing is recorded at all.
#[test]
fn must_never_learn_pairs_against_the_system_word_list() {
    use vox_lib::learning::{observe, Observed};
    let store = vox_lib::history::Store::open_in_memory().unwrap();
    for (w, r) in [
        ("their", "there"),
        ("they're", "their"),
        ("there", "they're"),
        ("to", "too"),
        ("two", "to"),
        ("its", "it's"),
        ("it's", "its"),
        ("affect", "effect"),
        ("than", "then"),
        ("your", "you're"),
        ("whose", "who's"),
        ("principal", "principle"),
        ("lead", "led"),
        ("bear", "bare"),
    ] {
        assert_eq!(
            observe(&store, w, r, "Notes", 1, 3).unwrap(),
            Observed::Guarded,
            "{w} → {r} must not produce a candidate"
        );
    }
    assert!(store.vocab_list().unwrap().is_empty());
    let have_list = vox_lib::context::system_words().is_some();
    for (w, r) in [
        ("cuber netties", "Kubernetes"),
        ("acks UI element", "AXUIElement"),
    ] {
        let got = observe(&store, w, r, "Notes", 1, 3).unwrap();
        if have_list {
            assert!(
                matches!(got, Observed::Recorded { count: 1, .. }),
                "{w} → {r} must produce a candidate, got {got:?}"
            );
        } else {
            assert_eq!(got, Observed::Guarded, "no word list: fail closed");
        }
    }
}

/// Correction candidates never contain text beyond the two forms, even when the
/// correction sits in a long, sensitive-looking sentence.
#[test]
fn candidates_hold_the_two_forms_and_nothing_else() {
    use vox_lib::learning::{extract_correction, observe_with};
    let inserted = "my card is 4111 1111 1111 1111, the password for prea is hunter2 and the door code is 8842 ";
    let observed = "my card is 4111 1111 1111 1111, the password for Priya is hunter2 and the door code is 8842 ";
    let (w, r) = extract_correction(inserted, observed).expect("an aligned one-token fix");
    assert_eq!((w.as_str(), r.as_str()), ("prea", "Priya"));
    let store = vox_lib::history::Store::open_in_memory().unwrap();
    let common = |t: &str| {
        [
            "my", "card", "is", "the", "password", "for", "and", "door", "code",
        ]
        .contains(&t)
    };
    observe_with(&store, &w, &r, "Notes", 1, 3, Some(&common)).unwrap();
    let json = serde_json::to_string(&store.vocab_list().unwrap()).unwrap();
    for needle in ["4111", "hunter2", "8842", "card", "password", "door"] {
        assert!(
            !json.contains(needle),
            "the stored row leaks {needle}: {json}"
        );
    }
    // Every column of vocab_candidates is on this list; none can hold a sentence.
    let src = include_str!("../src/history.rs");
    let table = src
        .split("CREATE TABLE IF NOT EXISTS vocab_candidates (")
        .nth(1)
        .and_then(|rest| rest.split(");").next())
        .expect("vocab_candidates table in the schema");
    const ALLOWED: &[&str] = &[
        "id",
        "wrong_form",
        "right_form",
        "count",
        "first_seen",
        "last_seen",
        "source_apps",
        "reversals",
        "state",
        "sessions",
        "last_session",
    ];
    for line in table.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let column = line.split_whitespace().next().unwrap();
        assert!(
            ALLOWED.contains(&column),
            "unexpected vocab_candidates column {column}"
        );
    }
}

/// Capture is on from the first launch (docs/LEARNING.md#rollout) and application is not,
/// and both flags are in the settings file so they can be inspected and diffed.
#[test]
fn capture_is_on_and_application_off_by_default() {
    let s = vox_lib::settings::Settings::default();
    assert!(s.learning.capture_corrections);
    assert!(!s.learning.apply_learned_terms);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(
        json["learning"]["captureCorrections"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        json["learning"]["applyLearnedTerms"],
        serde_json::Value::Bool(false)
    );
}
