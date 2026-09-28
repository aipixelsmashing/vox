//! Learning vocabulary from corrections. Full design and failure analysis: docs/LEARNING.md.
//!
//! We already know exactly what text we inserted, and we already hold the accessibility
//! permission needed to read the field back. Every fix the user makes afterwards is a labelled
//! training pair produced at zero cost — currently thrown away by every tool in this category.
//!
//! M4 builds capture only: the post-insertion watch, aligned-edit extraction, the homophone
//! guard, and candidate storage with promotion at the threshold. Applying terms is M7.
//!
//! Two hard rules, both of which exist because a system that learns silently can be
//! confidently wrong forever:
//!   1. Never learn from a single instance (MIN_OCCURRENCES, across >= MIN_SESSIONS sessions).
//!   2. The correction path must be as good as the learning path — every term visible,
//!      provenanced, and deletable, with the evidence deleted alongside it.
//!
//! Nothing in this file logs the text it reads or the forms it stores. tests/guards.rs
//! scans every log line here for the variables that would hold them; the log gets counts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::history;
use crate::pipeline::InjectionTarget;

/// A term is applied only after this many independent corrections. The setting
/// `learning.minOccurrences` can raise this, never lower it.
pub const MIN_OCCURRENCES: u32 = 3;
/// ...spread over at least this many sessions, so one bad afternoon cannot teach a term.
pub const MIN_SESSIONS: u32 = 2;
/// How long we watch a field after inserting into it.
pub const WATCH_WINDOW: Duration = Duration::from_secs(90);
/// When the field is read back, measured from the insertion. All inside [`WATCH_WINDOW`].
pub const READ_SCHEDULE: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(90),
];
/// Two corrected-backs suspend an applied term rather than fighting the user (M7).
pub const SUSPEND_AFTER_REVERSALS: u32 = 2;
/// A correction replaces at most this many inserted tokens with at most this many new ones.
/// "acks UI element" → "AXUIElement" is three; a rewritten clause is more.
pub const MAX_SPAN_TOKENS: usize = 3;
/// A form longer than this is not a term.
pub const MAX_FORM_CHARS: usize = 64;
/// A field with more characters than this is not read back at all: the read would be slow,
/// and a document that size is being edited, not corrected.
pub const MAX_FIELD_CHARS: i64 = 100_000;
/// A dictation this long after the previous one starts a new session. Vox is a tray app that
/// runs for weeks, so a launch alone cannot be the session boundary.
pub const SESSION_GAP: Duration = Duration::from_secs(4 * 60 * 60);

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

// ─── Sessions ────────────────────────────────────────────────────────────────

/// Which session a correction belongs to. A session starts at launch and again after
/// [`SESSION_GAP`] without a dictation; its id is the wall-clock millisecond it started, so
/// the stored `last_session` column reads as a date.
pub struct SessionClock {
    id: i64,
    last_activity: Instant,
}

impl SessionClock {
    pub fn new(now: Instant, wall_millis: i64) -> Self {
        Self {
            id: wall_millis,
            last_activity: now,
        }
    }

    /// A dictation happened: the current session id, or a fresh one after a long gap.
    pub fn touch(&mut self, now: Instant, wall_millis: i64) -> i64 {
        if now.saturating_duration_since(self.last_activity) >= SESSION_GAP {
            self.id = wall_millis;
        }
        self.last_activity = now;
        self.id
    }

    pub fn id(&self) -> i64 {
        self.id
    }
}

static SESSION: Mutex<Option<SessionClock>> = Mutex::new(None);

/// The session id for a dictation happening now.
pub fn session_id() -> i64 {
    let (now, wall) = (Instant::now(), history::now_millis());
    let mut guard = SESSION.lock();
    guard
        .get_or_insert_with(|| SessionClock::new(now, wall))
        .touch(now, wall)
}

// ─── Alignment ───────────────────────────────────────────────────────────────

/// Whitespace-separated tokens with surrounding punctuation removed, so "standup." and
/// "standup" compare equal and a comma the user added is not a correction. Inner
/// punctuation stays: "it's", "k8s", "e.g".
pub fn tokens(s: &str) -> Vec<&str> {
    s.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Diff the span that now occupies where the insertion was against what we inserted, and
/// extract a candidate.
///
/// Only *aligned local* edits count: a contiguous span of at most [`MAX_SPAN_TOKENS`]
/// inserted tokens replaced by a contiguous span of at most as many new ones, with the rest
/// of the insertion intact around it. A rewritten sentence is editing, not correcting —
/// discard it, because learning from it produces nonsense terms. Deletions and insertions
/// are not corrections either.
///
/// Text typed after the insertion is tolerated (the tail of the insertion must reappear
/// after the change, and anything after that is ignored), with one exception: when the
/// change reaches the end of the insertion there is no tail to anchor on, so the
/// replacement may then not be longer, in tokens, than what it replaced. "prea" → "Priya"
/// followed by typing is caught; "Priya" → "Priya Sharma" at the very end is not.
pub fn extract_correction(inserted: &str, observed: &str) -> Option<(String, String)> {
    let a = tokens(inserted);
    let b = tokens(observed);
    if a.is_empty() || b.is_empty() || a == b {
        return None;
    }
    let lp = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if lp == a.len() {
        // The insertion is intact and something was typed after it.
        return None;
    }
    for k in 1..=MAX_SPAN_TOKENS.min(a.len() - lp) {
        let wrong = &a[lp..lp + k];
        let tail = &a[lp + k..];
        let right: &[&str] = if tail.is_empty() {
            if lp == 0 {
                // Nothing of the insertion survived: a rewrite, whatever its length.
                return None;
            }
            let right = &b[lp..];
            if right.is_empty() || right.len() > k {
                return None;
            }
            right
        } else {
            let Some(start) =
                (lp..=b.len().saturating_sub(tail.len())).find(|&s| &b[s..s + tail.len()] == tail)
            else {
                continue;
            };
            if start == lp {
                // The tail follows the prefix directly: the span was deleted, not replaced.
                return None;
            }
            &b[lp..start]
        };
        if right.len() > MAX_SPAN_TOKENS {
            return None;
        }
        let (w, r) = (wrong.join(" "), right.join(" "));
        if w.chars().count() > MAX_FORM_CHARS || r.chars().count() > MAX_FORM_CHARS {
            return None;
        }
        return Some((w, r));
    }
    None
}

/// Where the insertion sits in the field's value right after inserting: the text before
/// it, the text after it, and the insertion as the field holds it (some targets trim the
/// trailing space). `caret_utf16` is the selection start the accessibility API reports,
/// which is the end of the insertion when the app updated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub prefix: String,
    pub suffix: String,
    pub inserted: String,
}

pub fn locate(value: &str, caret_utf16: Option<usize>, inserted: &str) -> Option<Located> {
    let trimmed = inserted.trim_end();
    let mut candidates = vec![inserted];
    if trimmed != inserted && !trimmed.is_empty() {
        candidates.push(trimmed);
    }
    for cand in candidates {
        let at_caret = caret_utf16
            .map(|c| crate::context::byte_offset_for_utf16(value, c))
            .filter(|&end| value.is_char_boundary(end) && value[..end].ends_with(cand))
            .map(|end| end - cand.len());
        let Some(start) = at_caret.or_else(|| value.rfind(cand)) else {
            continue;
        };
        let end = start + cand.len();
        return Some(Located {
            prefix: value[..start].to_string(),
            suffix: value[end..].to_string(),
            inserted: cand.to_string(),
        });
    }
    None
}

impl Located {
    /// The span that now occupies where the insertion was, if the text around it is still
    /// what it was. None means the user edited elsewhere in the field, and the watch is
    /// no longer looking at its own insertion.
    pub fn middle<'a>(&self, value: &'a str) -> Option<&'a str> {
        value
            .strip_prefix(self.prefix.as_str())?
            .strip_suffix(self.suffix.as_str())
    }
}

// ─── The homophone guard ─────────────────────────────────────────────────────

/// Every token is an ordinary word (or has no letters at all: a number, a symbol).
/// `is_common` answers for a lower-cased token. Contractions are ordinary when their base
/// is: the system word list has no apostrophes, and "its" → "it's" is exactly the kind of
/// fix the guard exists for.
pub fn is_ordinary(form: &str, is_common: &dyn Fn(&str) -> bool) -> bool {
    const CONTRACTIONS: &[&str] = &["s", "re", "ve", "ll", "d", "t", "m"];
    tokens(form).iter().all(|t| {
        if !t.chars().any(char::is_alphabetic) {
            return true;
        }
        let lower = t.to_lowercase().replace('’', "'");
        if is_common(&lower) {
            return true;
        }
        match lower.rsplit_once('\'') {
            Some((base, suffix)) => CONTRACTIONS.contains(&suffix) && is_common(base),
            None => false,
        }
    })
}

/// No candidate when both forms are ordinary words: "their" → "there" is the user fixing
/// a homophone the engine chose wrongly in that sentence, not teaching a term, and learning
/// it would make the next sentence wrong. Proper nouns, product names, acronyms and jargon
/// pass because they are not dictionary words.
pub fn passes_homophone_guard(wrong: &str, right: &str, is_common: &dyn Fn(&str) -> bool) -> bool {
    !(is_ordinary(wrong, is_common) && is_ordinary(right, is_common))
}

// ─── Observation ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// Both forms are ordinary words, or no word list could be loaded to say otherwise.
    Guarded,
    Recorded {
        count: u32,
        sessions: u32,
        promoted: bool,
    },
}

/// Guard, then record: upsert the candidate and promote it once it has been seen
/// `min_occurrences` times across [`MIN_SESSIONS`] sessions. `is_common` is None when no
/// word list is available, and then nothing is recorded: without the list every word looks
/// unusual and the guard could not do its job.
pub fn observe_with(
    store: &history::Store,
    wrong: &str,
    right: &str,
    app: &str,
    session: i64,
    min_occurrences: u32,
    is_common: Option<&dyn Fn(&str) -> bool>,
) -> anyhow::Result<Observed> {
    let Some(is_common) = is_common else {
        return Ok(Observed::Guarded);
    };
    if !passes_homophone_guard(wrong, right, is_common) {
        return Ok(Observed::Guarded);
    }
    let (count, sessions, promoted) = store.vocab_observe(
        wrong,
        right,
        app,
        session,
        history::now_millis(),
        min_occurrences.max(MIN_OCCURRENCES),
    )?;
    Ok(Observed::Recorded {
        count,
        sessions,
        promoted,
    })
}

/// [`observe_with`] against the system word list (docs/LEARNING.md, the homophone guard).
pub fn observe(
    store: &history::Store,
    wrong: &str,
    right: &str,
    app: &str,
    session: i64,
    min_occurrences: u32,
) -> anyhow::Result<Observed> {
    let words = crate::context::system_words();
    let is_common = words.map(|w| move |t: &str| w.contains(t));
    observe_with(
        store,
        wrong,
        right,
        app,
        session,
        min_occurrences,
        is_common.as_ref().map(|f| f as &dyn Fn(&str) -> bool),
    )
}

/// Applied terms are passed to the engine as a recognition hint where supported (fixing the
/// error) and applied as whole-token replacement otherwise (patching it). Case-insensitive
/// match, sentence-initial capitalisation preserved. M7.
pub fn apply(_text: &str, _terms: &[Term]) -> String {
    todo!("M7: application of learned terms")
}

// ─── The watch ───────────────────────────────────────────────────────────────

/// Registered after a verified insertion. Fire-and-forget: this is off the critical path and
/// its failure must never affect dictation. Dropping it cancels the thread at its next check.
pub struct Watch {
    cancel: Arc<AtomicBool>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// At most one watch at a time: a new insertion replaces the previous watch, whose field
/// has changed under it anyway.
static CURRENT: Mutex<Option<Watch>> = Mutex::new(None);

/// The pipeline's one call, after an insertion that verified. Never reached for a refused
/// or unverified insertion: password fields and secure input are excluded before this is
/// called, not filtered afterwards (tests/guards.rs checks the call site).
pub fn watch_after_insertion(
    deps: &crate::pipeline::Deps,
    target: &InjectionTarget,
    inserted: &str,
) {
    let (on, min_occurrences) = {
        let s = deps.settings.read();
        (s.learning.capture_corrections, s.learning.min_occurrences)
    };
    let session = session_id();
    if !on {
        return;
    }
    let watch = Watch::register(
        target,
        inserted,
        deps.history.clone(),
        deps.injector.clone(),
        session,
        min_occurrences,
    );
    *CURRENT.lock() = watch;
}

impl Watch {
    /// Schedules the reads in [`READ_SCHEDULE`] on their own thread. None when the field
    /// cannot be read back, or the insertion cannot be found in it: no watch, no log
    /// beyond a count, the dictation is already done.
    #[cfg(target_os = "macos")]
    pub fn register(
        target: &InjectionTarget,
        inserted: &str,
        store: Arc<history::Store>,
        injector: Arc<dyn crate::inject::TextInjector>,
        session: i64,
        min_occurrences: u32,
    ) -> Option<Self> {
        let el = target.element.clone()?;
        if live::refused(&el) {
            tracing::info!("watch: not registered, field refused");
            return None;
        }
        let Some(value) = live::read_value(&el) else {
            tracing::info!("watch: not registered, field not readable");
            return None;
        };
        let Some(located) = locate(&value, live::read_caret(&el), inserted) else {
            tracing::info!("watch: not registered, insertion not found in the field");
            return None;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = WatchContext {
            pid: target.pid,
            app: target.app_name.clone(),
            located,
            store,
            injector,
            session,
            min_occurrences,
            cancel: cancel.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name("vox-watch".into())
            .spawn(move || live::run(el, ctx));
        match spawned {
            Ok(_) => {
                tracing::info!("watch: registered for {} s", WATCH_WINDOW.as_secs());
                Some(Self { cancel })
            }
            Err(e) => {
                tracing::warn!("watch: thread failed to start: {e}");
                None
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn register(
        _target: &InjectionTarget,
        _inserted: &str,
        _store: Arc<history::Store>,
        _injector: Arc<dyn crate::inject::TextInjector>,
        _session: i64,
        _min_occurrences: u32,
    ) -> Option<Self> {
        None
    }
}

/// What the watch thread carries. Nothing here is logged.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct WatchContext {
    pid: u32,
    app: String,
    located: Located,
    store: Arc<history::Store>,
    injector: Arc<dyn crate::inject::TextInjector>,
    session: i64,
    min_occurrences: u32,
    cancel: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
mod live {
    use super::*;
    use axuielement::ax_attribute::{
        AX_NUMBER_OF_CHARACTERS_ATTRIBUTE, AX_SECURE_TEXT_FIELD_SUBROLE,
        AX_SELECTED_TEXT_RANGE_ATTRIBUTE, AX_SUBROLE_ATTRIBUTE, AX_VALUE_ATTRIBUTE,
    };

    use crate::inject::macos::ElementRef;

    /// The same refusals injection makes, checked again before any read.
    pub fn refused(el: &ElementRef) -> bool {
        if crate::permissions::secure_input_active() {
            return true;
        }
        el.0.string_attribute(AX_SUBROLE_ATTRIBUTE)
            .ok()
            .flatten()
            .as_deref()
            == Some(AX_SECURE_TEXT_FIELD_SUBROLE)
    }

    /// The field's value, unless it is too large to be a field someone is correcting.
    pub fn read_value(el: &ElementRef) -> Option<String> {
        if let Ok(Some(n)) = el.0.i64_attribute(AX_NUMBER_OF_CHARACTERS_ATTRIBUTE) {
            if n > MAX_FIELD_CHARS {
                return None;
            }
        }
        let value = el.0.string_attribute(AX_VALUE_ATTRIBUTE).ok().flatten()?;
        (value.len() as i64 <= MAX_FIELD_CHARS * 4).then_some(value)
    }

    pub fn read_caret(el: &ElementRef) -> Option<usize> {
        el.0.range_attribute(AX_SELECTED_TEXT_RANGE_ATTRIBUTE)
            .ok()
            .flatten()
            .map(|r| r.location.max(0) as usize)
    }

    /// Sleeps until `deadline`, waking often enough that a cancelled watch ends promptly.
    fn wait_until(deadline: Instant, cancel: &AtomicBool) -> bool {
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100).min(deadline - Instant::now()));
        }
        !cancel.load(Ordering::Relaxed)
    }

    pub fn run(el: ElementRef, ctx: WatchContext) {
        let t0 = Instant::now();
        // The insertion as the field held it, rebased after each recorded correction so a
        // second fix in the same dictation aligns against the first.
        let mut baseline = ctx.located.inserted.clone();
        let mut reported: Vec<(String, String)> = Vec::new();
        for (i, due) in READ_SCHEDULE.iter().enumerate() {
            if !wait_until(t0 + *due, &ctx.cancel) {
                tracing::info!("watch: cancelled before read {}", i + 1);
                return;
            }
            if refused(&el) {
                tracing::info!("watch: dropped at read {}, field refused", i + 1);
                return;
            }
            if ctx.injector.frontmost().map(|(pid, _)| pid) != Some(ctx.pid) {
                tracing::info!("watch: ended at read {}, focus left the app", i + 1);
                return;
            }
            let Some(value) = read_value(&el) else {
                tracing::info!("watch: dropped at read {}, field not readable", i + 1);
                return;
            };
            let Some(middle) = ctx.located.middle(&value) else {
                tracing::info!(
                    "watch: ended at read {}, text around the insertion changed",
                    i + 1
                );
                return;
            };
            if middle == baseline {
                continue;
            }
            let Some((wrong, right)) = extract_correction(&baseline, middle) else {
                continue;
            };
            if reported.iter().any(|(w, r)| *w == wrong && *r == right) {
                continue;
            }
            match observe(
                &ctx.store,
                &wrong,
                &right,
                &ctx.app,
                ctx.session,
                ctx.min_occurrences,
            ) {
                Ok(Observed::Guarded) => {
                    tracing::info!("watch: read {}: edit guarded, both forms ordinary", i + 1);
                }
                Ok(Observed::Recorded {
                    count,
                    sessions,
                    promoted,
                }) => {
                    tracing::info!(
                        "watch: read {}: candidate recorded ({} → {} tokens), seen {} times in {} sessions{}",
                        i + 1,
                        tokens(&wrong).len(),
                        tokens(&right).len(),
                        count,
                        sessions,
                        if promoted { ", promoted" } else { "" }
                    );
                }
                Err(e) => tracing::warn!("watch: store failed: {e}"),
            }
            reported.push((wrong, right));
            baseline = middle.to_string();
        }
        tracing::info!("watch: window over, {} correction(s)", reported.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_common(w: &str) -> bool {
        const WORDS: &[&str] = &[
            "meet",
            "me",
            "at",
            "standup",
            "use",
            "the",
            "api",
            "their",
            "there",
            "they're",
            "to",
            "too",
            "two",
            "its",
            "it's",
            "affect",
            "effect",
            "than",
            "then",
            "your",
            "you're",
            "whose",
            "who's",
            "principal",
            "principle",
            "lead",
            "led",
            "bear",
            "bare",
            "call",
            "now",
            "send",
            "it",
            "later",
            "text",
            "please",
            "apple",
            "ship",
            "cuber",
            "acks",
            "ui",
            "element",
            "deploy",
            "tomorrow",
            "and",
            "thanks",
            "for",
            "update",
        ];
        WORDS.contains(&w)
    }

    fn extract(a: &str, b: &str) -> Option<(String, String)> {
        extract_correction(a, b)
    }

    #[test]
    fn tokens_strip_outer_punctuation_only() {
        assert_eq!(
            tokens("Hello, world! (it's k8s.)"),
            ["Hello", "world", "it's", "k8s"]
        );
        assert_eq!(tokens(" — "), Vec::<&str>::new());
    }

    #[test]
    fn a_multi_word_term_fixed_in_place_is_a_candidate() {
        assert_eq!(
            extract(
                "meet me at cuber netties standup ",
                "meet me at Kubernetes standup "
            ),
            Some(("cuber netties".into(), "Kubernetes".into()))
        );
        assert_eq!(
            extract("use the acks UI element API", "use the AXUIElement API"),
            Some(("acks UI element".into(), "AXUIElement".into()))
        );
        assert_eq!(
            extract("call prea now", "call Priya now"),
            Some(("prea".into(), "Priya".into()))
        );
    }

    #[test]
    fn punctuation_and_case_only_edits() {
        assert_eq!(
            extract("meet me at standup", "Meet me at standup."),
            Some(("meet".into(), "Meet".into()))
        );
        assert_eq!(
            extract("meet me at standup", "meet me at standup, ok"),
            None
        );
        assert_eq!(extract("meet me at standup", "meet me, at standup"), None);
    }

    #[test]
    fn unchanged_deleted_inserted_and_rewritten_are_not_corrections() {
        let a = "meet me at cuber netties standup";
        assert_eq!(extract(a, a), None, "unchanged");
        assert_eq!(extract(a, ""), None, "everything deleted");
        assert_eq!(extract(a, "meet me at standup"), None, "term deleted");
        assert_eq!(
            extract("meet me at standup", "meet me at the standup"),
            None,
            "word inserted"
        );
        assert_eq!(
            extract("meet me at standup", "meet me at standup tomorrow"),
            None,
            "typed after"
        );
        assert_eq!(
            extract(a, "let's sync on the cluster after lunch"),
            None,
            "rewritten"
        );
        assert_eq!(
            extract("I think we should deploy tomorrow", "I want to ship it now"),
            None,
            "rewritten from the same first word"
        );
        assert_eq!(extract("send it now", "ship it later"), None, "two edits");
        assert_eq!(
            extract("cuber netties", "Kubernetes"),
            None,
            "nothing of the insertion survived"
        );
    }

    #[test]
    fn spans_are_bounded() {
        assert_eq!(
            extract("we use a b c d e here", "we use X here"),
            None,
            "five tokens replaced is a rewrite"
        );
        assert_eq!(
            extract("we use X here", "we use a b c d here"),
            None,
            "four-token replacement is a rewrite"
        );
        let long = "x".repeat(MAX_FORM_CHARS + 1);
        assert_eq!(
            extract("we use X here", &format!("we use {long} here")),
            None
        );
    }

    #[test]
    fn typing_after_the_insertion_is_tolerated_when_the_tail_anchors() {
        assert_eq!(
            extract("call prea now ", "call Priya now and tell her thanks"),
            Some(("prea".into(), "Priya".into()))
        );
        // The fix is on the last token: no tail to anchor on, so the replacement may not
        // grow. "Priya" alone is caught, "Priya thanks" is not distinguishable from typing.
        assert_eq!(
            extract("send it to prea ", "send it to Priya "),
            Some(("prea".into(), "Priya".into()))
        );
        assert_eq!(extract("send it to prea ", "send it to Priya thanks"), None);
        assert_eq!(
            extract("at cuber netties ", "at Kubernetes standup"),
            Some(("cuber netties".into(), "Kubernetes standup".into())),
            "ambiguous but bounded: the guard and the threshold decide"
        );
    }

    #[test]
    fn locate_uses_the_caret_then_the_last_occurrence_and_tolerates_a_trimmed_space() {
        let v = "Hi. meet me at standup ";
        let l = locate(v, Some(v.encode_utf16().count()), "meet me at standup ").unwrap();
        assert_eq!(l.prefix, "Hi. ");
        assert_eq!(l.suffix, "");
        assert_eq!(l.inserted, "meet me at standup ");
        // Caret elsewhere (the app reported it late): last occurrence.
        let l = locate("x meet me at standup y", Some(0), "meet me at standup ").unwrap();
        assert_eq!((l.prefix.as_str(), l.suffix.as_str()), ("x ", "y"));
        assert_eq!(l.inserted, "meet me at standup ");
        // Omnibox-style trim of the trailing space.
        let l = locate("meet me at standup", None, "meet me at standup ").unwrap();
        assert_eq!(l.inserted, "meet me at standup");
        assert!(locate("something else", None, "meet me at standup ").is_none());
        // A caret past the end of a non-ASCII value does not panic.
        assert!(locate("héllo", Some(99), "zzz").is_none());
    }

    #[test]
    fn middle_requires_the_surroundings_intact() {
        let l = Located {
            prefix: "Hi. ".into(),
            suffix: " bye".into(),
            inserted: "meet me at standup".into(),
        };
        assert_eq!(
            l.middle("Hi. meet me at Kubernetes bye"),
            Some("meet me at Kubernetes")
        );
        assert_eq!(l.middle("Hello. meet me at standup bye"), None);
        assert_eq!(l.middle("Hi. meet me at standup, bye now"), None);
    }

    #[test]
    fn the_homophone_guard_blocks_ordinary_pairs_and_passes_terms() {
        let common = &fixture_common as &dyn Fn(&str) -> bool;
        for (a, b) in [
            ("their", "there"),
            ("to", "too"),
            ("its", "it's"),
            ("affect", "effect"),
            ("than", "then"),
            ("your", "you're"),
            ("whose", "who's"),
            ("principal", "principle"),
            ("lead", "led"),
            ("bear", "bare"),
            ("apple", "Apple"),
            ("to", "2"),
        ] {
            assert!(
                !passes_homophone_guard(a, b, common),
                "{a} → {b} must be guarded"
            );
        }
        assert!(passes_homophone_guard(
            "cuber netties",
            "Kubernetes",
            common
        ));
        assert!(passes_homophone_guard(
            "acks UI element",
            "AXUIElement",
            common
        ));
        assert!(
            passes_homophone_guard("prea", "Priya", common),
            "neither form is a word"
        );
        assert!(
            passes_homophone_guard("call", "Priya", common),
            "one side unusual is enough"
        );
    }

    #[test]
    fn observing_records_then_promotes_at_three_across_two_sessions() {
        let store = history::Store::open_in_memory().unwrap();
        let common = &fixture_common as &dyn Fn(&str) -> bool;
        let obs = |session: i64| {
            observe_with(
                &store,
                "cuber netties",
                "Kubernetes",
                "Slack",
                session,
                3,
                Some(common),
            )
            .unwrap()
        };
        assert_eq!(
            obs(1),
            Observed::Recorded {
                count: 1,
                sessions: 1,
                promoted: false
            }
        );
        assert_eq!(
            obs(1),
            Observed::Recorded {
                count: 2,
                sessions: 1,
                promoted: false
            }
        );
        assert_eq!(
            obs(1),
            Observed::Recorded {
                count: 3,
                sessions: 1,
                promoted: false
            },
            "three in one session is not enough"
        );
        assert_eq!(
            obs(2),
            Observed::Recorded {
                count: 4,
                sessions: 2,
                promoted: true
            }
        );
        let terms = store.vocab_list().unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].state, "applied");
        assert_eq!(terms[0].count, 4);
        assert_eq!(terms[0].source_apps, vec!["Slack"]);
    }

    #[test]
    fn a_guarded_pair_and_a_missing_word_list_store_nothing() {
        let store = history::Store::open_in_memory().unwrap();
        let common = &fixture_common as &dyn Fn(&str) -> bool;
        assert_eq!(
            observe_with(&store, "their", "there", "Notes", 1, 3, Some(common)).unwrap(),
            Observed::Guarded
        );
        assert_eq!(
            observe_with(&store, "cuber netties", "Kubernetes", "Notes", 1, 3, None).unwrap(),
            Observed::Guarded,
            "no word list: fail closed"
        );
        assert!(store.vocab_list().unwrap().is_empty());
    }

    #[test]
    fn the_minimum_cannot_be_lowered_by_the_setting() {
        let store = history::Store::open_in_memory().unwrap();
        let common = &fixture_common as &dyn Fn(&str) -> bool;
        let mut last = Observed::Guarded;
        for s in [1, 2] {
            last = observe_with(&store, "prea", "Priya", "Slack", s, 1, Some(common)).unwrap();
        }
        assert_eq!(
            last,
            Observed::Recorded {
                count: 2,
                sessions: 2,
                promoted: false
            }
        );
    }

    #[test]
    fn sessions_roll_over_after_the_gap() {
        let t0 = Instant::now();
        let mut clock = SessionClock::new(t0, 1_000);
        assert_eq!(clock.touch(t0 + Duration::from_secs(60), 2_000), 1_000);
        assert_eq!(
            clock.touch(t0 + SESSION_GAP, 3_000),
            1_000,
            "gap measured from the last dictation"
        );
        assert_eq!(
            clock.touch(t0 + 2 * SESSION_GAP + Duration::from_secs(1), 4_000),
            4_000
        );
        assert_eq!(clock.id(), 4_000);
        assert!(session_id() > 0);
    }
}
