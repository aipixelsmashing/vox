//! Learning vocabulary from corrections. Full design and failure analysis: docs/LEARNING.md.
//!
//! We already know exactly what text we inserted, and we already hold the accessibility
//! permission needed to read the field back. Every fix the user makes afterwards is a labelled
//! training pair produced at zero cost — currently thrown away by every tool in this category.
//!
//! Two hard rules, both of which exist because a system that learns silently can be
//! confidently wrong forever:
//!   1. Never learn from a single instance (MIN_OCCURRENCES, across >= 2 sessions).
//!   2. The correction path must be as good as the learning path — every term visible,
//!      provenanced, and deletable, with the evidence deleted alongside it.

use std::time::Duration;

/// A term is applied only after this many independent corrections.
pub const MIN_OCCURRENCES: u32 = 3;
/// How long we watch a field after inserting into it.
pub const WATCH_WINDOW: Duration = Duration::from_secs(90);
/// Two corrected-backs suspend an applied term rather than fighting the user.
pub const SUSPEND_AFTER_REVERSALS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TermState {
    /// Seen fewer than MIN_OCCURRENCES times. Stored, never applied.
    Candidate,
    Applied,
    /// The user kept changing it back. Flagged in the Vocabulary pane with the reason.
    Suspended,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct Term {
    pub id: i64,
    pub wrong_form: String,
    pub right_form: String,
    pub count: u32,
    pub first_seen: i64,
    pub last_seen: i64,
    /// App names only, for provenance in the UI. Never the surrounding text.
    pub source_apps: Vec<String>,
    pub state: TermState,
}

/// Registered after a successful insertion. Fire-and-forget: this is off the critical path and
/// its failure must never affect dictation.
pub struct Watch {
    // target: InjectionTarget, inserted: String, deadline: Instant
}

impl Watch {
    /// Never registered for a field we refused to insert into — password fields and
    /// secure-input contexts are excluded before this is called, not filtered afterwards.
    pub fn register(_target: &crate::pipeline::InjectionTarget, _inserted: &str) -> Option<Self> {
        todo!("schedule AX reads at +2s, +10s, +30s; cancel when focus leaves the app")
    }
}

/// Diff the field against what we inserted and extract a candidate.
///
/// Only *aligned local* edits count: a contiguous span of inserted tokens replaced by a
/// contiguous span of new ones. A rewritten sentence is editing, not correcting — discard it,
/// because learning from it produces nonsense terms.
pub fn extract_correction(_inserted: &str, _observed: &str) -> Option<(String, String)> {
    todo!()
}

/// Applied terms are passed to the engine as a recognition hint where supported (fixing the
/// error) and applied as whole-token replacement otherwise (patching it). Case-insensitive
/// match, sentence-initial capitalisation preserved.
pub fn apply(_text: &str, _terms: &[Term]) -> String {
    todo!()
}

pub struct Store {
    // Shares history.db. Kilobytes — the compounding asset costs nothing in footprint.
}

impl Store {
    pub fn observe(&self, _wrong: &str, _right: &str, _app: &str) -> anyhow::Result<()> {
        todo!("upsert candidate, promote at MIN_OCCURRENCES across >= 2 sessions")
    }

    /// Deletes the term *and the candidate evidence behind it*, so it cannot be re-learned
    /// from corrections the user has already disowned.
    pub fn forget(&self, _id: i64) -> anyhow::Result<()> {
        todo!()
    }

    pub fn export(&self) -> anyhow::Result<String> {
        todo!("plain list — docs/adr/0015")
    }
}
