//! Intentionally empty.
//!
//! Vox collects no analytics, no crash reports, and no usage data — not even opt-in. See
//! docs/adr/0007 for the reasoning: an opt-in toggle would mean every user has to evaluate a
//! claim rather than read a guarantee.
//!
//! Diagnostics exist, but they are local: per-dictation timings and injection outcomes are
//! stored in the history row and shown in Settings → Diagnostics, so a user can choose to
//! paste a useful bug report.
//!
//! tests/no_telemetry.rs asserts that this file contains no network or HTTP client code.
//! If you are here to add some, read the ADR first and open a discussion.
