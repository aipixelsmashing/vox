//! Context from the focused field: up to twenty recognition hints, read at key-down
//! (docs/CONTEXT.md, docs/adr/0017). Off by default: `privacy.readFocusedField`.
//!
//! The rules, in the order they are enforced: only the element captured for injection;
//! never a password field, never under secure input; read once, held in memory for one
//! dictation, handed to Apple's on-device recogniser as contextual strings, dropped. Never
//! stored, never logged. Any failure to read, extract or pass hints means the dictation
//! proceeds without them.
//!
//! Nothing in this file logs, on purpose: tests/guards.rs asserts there is no `tracing`
//! call here, so neither the text nor the terms can reach a log line by accident. The
//! pipeline logs counts.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The cap. Every hint is a word the recogniser will prefer whenever the audio is close,
/// so every hint is also a chance to write a word the user did not say. Raising this is a
/// measured change against the wrong-word rate, not a default (adr/0017).
pub const MAX_TERMS: usize = 20;
/// Characters read on each side of the caret.
pub const WINDOW_CHARS: usize = 2000;
/// A read that takes longer than this is dropped; the dictation proceeds without hints.
pub const READ_DEADLINE: Duration = Duration::from_millis(150);
const MIN_TERM_CHARS: usize = 2;
const MAX_TERM_CHARS: usize = 40;

// ─── Vocabulary the user chose ───────────────────────────────────────────────

/// The hinted learned terms, when `learning.applyLearnedTerms` is on. Hinting needs only
/// the right form, so it keys on the right form: three corrections to it, however it was
/// mangled (docs/LEARNING.md). Nothing here reads the field; `privacy.readFocusedField`
/// has no say in it.
pub fn learned_terms(
    settings: &crate::settings::Settings,
    history: &crate::history::Store,
) -> Vec<String> {
    if !settings.learning.apply_learned_terms {
        return Vec::new();
    }
    let min = settings
        .learning
        .min_occurrences
        .max(crate::learning::MIN_OCCURRENCES);
    history.vocab_hinted(min).unwrap_or_default()
}

/// The manual dictionary's right-hand sides.
pub fn dictionary_terms(settings: &crate::settings::Settings) -> Vec<String> {
    settings
        .output
        .dictionary
        .iter()
        .map(|r| r.to.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// The hints for one dictation. Hints move the recogniser to the dictation module, which
/// drops the last word of a streamed dictation now and then (docs/spikes/s5-context.md).
/// Two things are worth that: a name on screen, and a word the user has taught Vox by
/// correcting it three times. The manual dictionary alone is not: it stays a
/// post-processing replacement, and rides along as hints when the module is switched
/// anyway. Learned terms first, then the dictionary, then the field's, because the user
/// chose the first two.
pub fn hints_for(learned: Vec<String>, dictionary: Vec<String>, field: Vec<String>) -> Vec<String> {
    if learned.is_empty() && field.is_empty() {
        return Vec::new();
    }
    merge(learned.into_iter().chain(dictionary).collect(), field)
}

/// Vocabulary first, then the field's terms nearest the caret; case-insensitive
/// de-duplication; at most [`MAX_TERMS`].
pub fn merge(vocabulary: Vec<String>, field: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(MAX_TERMS);
    let mut seen: Vec<String> = Vec::with_capacity(MAX_TERMS);
    for term in vocabulary.into_iter().chain(field) {
        if out.len() >= MAX_TERMS {
            break;
        }
        let key = term.to_lowercase();
        if term.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(term);
    }
    out
}

// ─── The system word list ────────────────────────────────────────────────────

/// `/usr/share/dict/words`, lower-cased, sorted, held as one string plus offsets: about
/// 3.5 MB for 236 000 words, loaded once in the background and only when the setting is
/// on, so a user who never turns it on pays nothing.
pub struct WordList {
    blob: String,
    offsets: Vec<u32>,
}

impl WordList {
    pub fn load(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let mut words: Vec<String> = raw
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        words.sort_unstable();
        words.dedup();
        if words.is_empty() {
            return None;
        }
        let mut blob = String::with_capacity(words.iter().map(|w| w.len() + 1).sum());
        let mut offsets = Vec::with_capacity(words.len() + 1);
        for w in &words {
            offsets.push(blob.len() as u32);
            blob.push_str(w);
            blob.push('\n');
        }
        offsets.push(blob.len() as u32);
        Some(Self { blob, offsets })
    }

    fn word_at(&self, i: usize) -> &str {
        let start = self.offsets[i] as usize;
        let end = self.offsets[i + 1] as usize - 1;
        &self.blob[start..end]
    }

    /// `word` must already be lower-case.
    pub fn contains(&self, word: &str) -> bool {
        let n = self.offsets.len() - 1;
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.word_at(mid).cmp(word) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return true,
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

static WORDS: OnceLock<Option<WordList>> = OnceLock::new();

/// The system word list, loaded on first use. Also the correction watch's homophone guard
/// (docs/LEARNING.md), which runs on its own thread and can afford the first load.
pub fn system_words() -> Option<&'static WordList> {
    WORDS
        .get_or_init(|| WordList::load(Path::new("/usr/share/dict/words")))
        .as_ref()
}

/// Load the word list on a background thread. Called at launch and when the setting is
/// turned on, so the first dictation afterwards does not pay for it inside the deadline.
pub fn warm() {
    if WORDS.get().is_none() {
        let _ = std::thread::Builder::new()
            .name("vox-context-warm".into())
            .spawn(|| {
                let _ = system_words();
            });
    }
}

// ─── Extraction ──────────────────────────────────────────────────────────────

/// Terms from the text around the caret, nearest first. `caret` is a UTF-16 offset into
/// `window`, as the accessibility API reports it. Errors when the word list is not loaded
/// yet, because without it every word looks unusual and the hints would be the user's
/// whole sentence.
pub fn field_terms(window: &str, caret_utf16: usize) -> Result<Vec<String>, &'static str> {
    let words = system_words().ok_or("word list not loaded")?;
    let caret = byte_offset_for_utf16(window, caret_utf16);
    Ok(extract(window, caret, &|w| words.contains(w)))
}

/// Pure extraction: `caret` is a byte offset into `window`; `is_common` answers for a
/// lower-cased word. Kept free of I/O so it is unit-testable with a toy word list.
///
/// A term is a token that is not an ordinary word: an identifier (digits, underscores or
/// dots inside), a mixed-case word, an acronym, a capitalised word the list does not
/// know, or any word the list does not know. Adjacent capitalised unknown words join into
/// one term ("Orsolya Csernák"). Ordinary words are never sent: they add nothing and they
/// are the user's text.
pub fn extract(window: &str, caret: usize, is_common: &dyn Fn(&str) -> bool) -> Vec<String> {
    #[derive(Clone)]
    struct Cand {
        start: usize,
        end: usize,
        text: String,
        capitalised: bool,
    }
    let mut cands: Vec<Cand> = Vec::new();
    for (start, end) in tokens(window) {
        let t = &window[start..end];
        if let Some(capitalised) = classify(t, is_common) {
            cands.push(Cand {
                start,
                end,
                text: t.to_string(),
                capitalised,
            });
        }
    }
    let mut merged: Vec<Cand> = Vec::with_capacity(cands.len());
    for c in cands {
        if let Some(last) = merged.last_mut() {
            if last.capitalised && c.capitalised && &window[last.end..c.start] == " " {
                last.end = c.end;
                last.text.push(' ');
                last.text.push_str(&c.text);
                continue;
            }
        }
        merged.push(c);
    }
    let distance = |c: &Cand| -> usize {
        if caret >= c.start && caret <= c.end {
            0
        } else if caret < c.start {
            c.start - caret
        } else {
            caret - c.end
        }
    };
    merged.sort_by_key(distance);
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for c in merged {
        let key = c.text.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(c.text);
        if out.len() >= MAX_TERMS {
            break;
        }
    }
    out
}

/// Byte ranges of tokens: runs of letters, digits, `_` and `.`, with leading and trailing
/// dots trimmed. Apostrophes and hyphens split, so "don't" and "well-known" fall apart
/// into ordinary words rather than surviving as identifiers.
fn tokens(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let is_tok = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    for (i, c) in s.char_indices() {
        match (start, is_tok(c)) {
            (None, true) => start = Some(i),
            (Some(st), false) => {
                out.push((st, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(st) = start {
        out.push((st, s.len()));
    }
    out.into_iter()
        .filter_map(|(st, en)| {
            let t = &s[st..en];
            let trimmed = t.trim_matches('.');
            if trimmed.is_empty() {
                return None;
            }
            let lead = t.len() - t.trim_start_matches('.').len();
            Some((st + lead, st + lead + trimmed.len()))
        })
        .collect()
}

/// Some(capitalised) when the token is a term; None when it is an ordinary word or noise.
fn classify(t: &str, is_common: &dyn Fn(&str) -> bool) -> Option<bool> {
    let chars: Vec<char> = t.chars().collect();
    let n = chars.len();
    if !(MIN_TERM_CHARS..=MAX_TERM_CHARS).contains(&n) {
        return None;
    }
    if !chars.iter().any(|c| c.is_alphabetic()) {
        return None;
    }
    let has_digit_or_underscore = chars.iter().any(|c| c.is_ascii_digit() || *c == '_');
    let has_dot = chars.contains(&'.');
    if has_digit_or_underscore {
        return Some(false);
    }
    if has_dot {
        // "vox.pixelsmashing.com" and "Node.js" are terms; "e.g" and "i.e" are not.
        let longest = t
            .split('.')
            .map(|seg| seg.chars().filter(|c| c.is_alphabetic()).count())
            .max()
            .unwrap_or(0);
        return (longest >= 3).then_some(false);
    }
    let first_upper = chars[0].is_uppercase();
    let inner_upper = chars[1..].iter().any(|c| c.is_uppercase());
    let all_upper = chars
        .iter()
        .filter(|c| c.is_alphabetic())
        .all(|c| c.is_uppercase());
    if inner_upper && !all_upper {
        // AXUIElement, iPhone, macOS: mixed case is never an ordinary word.
        return Some(false);
    }
    let lower = t.to_lowercase();
    if is_common(&lower) {
        return None;
    }
    Some(first_upper && !all_upper)
}

/// The byte offset of a UTF-16 code-unit offset, clamped to the string.
pub fn byte_offset_for_utf16(s: &str, utf16: usize) -> usize {
    let mut units = 0usize;
    for (i, c) in s.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// A slice of `s` by UTF-16 offsets, clamped, on character boundaries.
pub fn slice_utf16(s: &str, start: usize, end: usize) -> &str {
    let a = byte_offset_for_utf16(s, start);
    let b = byte_offset_for_utf16(s, end.max(start));
    &s[a..b]
}

// ─── Reading the field (macOS) ───────────────────────────────────────────────

/// The text around the caret and the caret's UTF-16 offset within it, from the element
/// captured for injection. Refuses password fields and secure input before reading
/// anything (the same checks injection makes), reads only the window around the caret
/// where the app supports `AXStringForRange`, and gives up if the read took longer than
/// [`READ_DEADLINE`]. The Err is one static word for the pipeline's log line.
#[cfg(target_os = "macos")]
pub fn read_field(el: &crate::inject::macos::ElementRef) -> Result<(String, usize), &'static str> {
    use axuielement::ax_attribute::{
        AX_COMBO_BOX_ROLE, AX_NUMBER_OF_CHARACTERS_ATTRIBUTE, AX_ROLE_ATTRIBUTE,
        AX_SECURE_TEXT_FIELD_SUBROLE, AX_SELECTED_TEXT_RANGE_ATTRIBUTE, AX_SUBROLE_ATTRIBUTE,
        AX_TEXT_AREA_ROLE, AX_TEXT_FIELD_ROLE, AX_VALUE_ATTRIBUTE,
    };
    use axuielement::{AXRange, AXValue};

    if crate::permissions::secure_input_active() {
        return Err("secure input");
    }
    let el = &el.0;
    let attr = |n: &str| el.string_attribute(n).ok().flatten();
    if attr(AX_SUBROLE_ATTRIBUTE).as_deref() == Some(AX_SECURE_TEXT_FIELD_SUBROLE) {
        return Err("password field");
    }
    if !matches!(
        attr(AX_ROLE_ATTRIBUTE).as_deref(),
        Some(AX_TEXT_FIELD_ROLE) | Some(AX_TEXT_AREA_ROLE) | Some(AX_COMBO_BOX_ROLE)
    ) {
        return Err("not a text field");
    }
    let t0 = Instant::now();
    let range = el
        .range_attribute(AX_SELECTED_TEXT_RANGE_ATTRIBUTE)
        .ok()
        .flatten()
        .ok_or("no caret")?;
    let caret = range.location.max(0) as usize;
    let total = el
        .i64_attribute(AX_NUMBER_OF_CHARACTERS_ATTRIBUTE)
        .ok()
        .flatten()
        .map(|n| n.max(0) as usize);
    let start = caret.saturating_sub(WINDOW_CHARS);
    let end = match total {
        Some(t) => (caret + WINDOW_CHARS).min(t.max(caret)),
        None => caret + WINDOW_CHARS,
    };
    // The window only, where the app can give it; otherwise the value, sliced.
    let windowed = AXValue::from_range(AXRange {
        location: start as isize,
        length: (end - start) as isize,
    })
    .and_then(|p| {
        el.parameterized_attribute("AXStringForRange", &p)
            .ok()
            .flatten()
    })
    .and_then(|v| v.as_string());
    let text = match windowed {
        Some(w) => w,
        None => {
            let value = attr(AX_VALUE_ATTRIBUTE).ok_or("no value")?;
            slice_utf16(&value, start, end).to_string()
        }
    };
    if t0.elapsed() > READ_DEADLINE {
        return Err("read took too long");
    }
    Ok((text, caret - start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn common(w: &str) -> bool {
        const WORDS: &[&str] = &[
            "say",
            "is",
            "too",
            "the",
            "meeting",
            "has",
            "been",
            "moved",
            "to",
            "thursday",
            "at",
            "three",
            "please",
            "ask",
            "about",
            "and",
            "check",
            "that",
            "still",
            "build",
            "in",
            "app",
            "migration",
            "new",
            "york",
            "deploy",
            "script",
            "well",
            "known",
            "don",
            "it",
            "us",
            "ok",
            "and",
            "chrome",
            "slack",
        ];
        WORDS.contains(&w)
    }

    #[test]
    fn ordinary_words_are_never_sent() {
        let t = extract(
            "The meeting has been moved to Thursday at three.",
            0,
            &common,
        );
        assert!(t.is_empty(), "{t:?}");
    }

    #[test]
    fn names_identifiers_and_acronyms_are() {
        let s = "Please ask Orsolya Csernák about the Kubestrix migration, and check that keytap and AXUIElement still build in the Tauri app v2_beta NASA e.g. vox.pixelsmashing.com";
        let t = extract(s, 0, &common);
        assert_eq!(
            t,
            vec![
                "Orsolya Csernák",
                "Kubestrix",
                "keytap",
                "AXUIElement",
                "Tauri",
                "v2_beta",
                "NASA",
                "vox.pixelsmashing.com"
            ]
        );
    }

    #[test]
    fn apostrophes_and_hyphens_split_and_common_caps_are_dropped() {
        let t = extract(
            "Don't say well-known; New York is OK and US too.",
            0,
            &common,
        );
        assert!(t.is_empty(), "{t:?}");
    }

    #[test]
    fn nearest_the_caret_first_walking_outwards() {
        let s = "Alpha the Bravo and Charlie has Delta been Echo";
        let caret = s.find("Charlie").unwrap() + 3;
        let t = extract(s, caret, &common);
        assert_eq!(t[0], "Charlie");
        assert_eq!(&t[1..3], &["Bravo", "Delta"]);
        assert_eq!(&t[3..], &["Alpha", "Echo"]);
    }

    #[test]
    fn capped_at_twenty_nearest_and_deduplicated() {
        let words: Vec<String> = (0..60).map(|i| format!("Term{i}")).collect();
        let s = format!("{} {} ", words.join(" "), words.join(" "));
        let caret = s.len() / 2;
        let t = extract(&s, caret, &common);
        assert_eq!(t.len(), MAX_TERMS);
        let mut lower: Vec<String> = t.iter().map(|x| x.to_lowercase()).collect();
        lower.sort();
        lower.dedup();
        assert_eq!(lower.len(), MAX_TERMS, "no duplicates");
    }

    #[test]
    fn vocabulary_ranks_ahead_and_shares_the_cap() {
        let vocab: Vec<String> = (0..5).map(|i| format!("Chosen{i}")).collect();
        let field: Vec<String> = (0..30).map(|i| format!("Field{i}")).collect();
        let t = merge(vocab.clone(), field);
        assert_eq!(t.len(), MAX_TERMS);
        assert_eq!(&t[..5], &vocab[..]);
        assert_eq!(t[5], "Field0");
        let t = merge(
            vec!["Kubernetes".into()],
            vec!["kubernetes".into(), "X1".into()],
        );
        assert_eq!(
            t,
            vec!["Kubernetes", "X1"],
            "case-insensitive de-dup keeps the chosen form"
        );
    }

    #[test]
    fn learned_terms_and_the_field_switch_the_module_and_the_dictionary_does_not() {
        let learned = vec!["Adi".to_string()];
        let dictionary = vec!["Kubernetes".to_string(), "OurCompany".to_string()];
        assert!(
            hints_for(Vec::new(), dictionary.clone(), Vec::new()).is_empty(),
            "dictionary alone never switches the module"
        );
        assert_eq!(
            hints_for(learned.clone(), Vec::new(), Vec::new()),
            vec!["Adi"],
            "a learned term switches it with no field read at all"
        );
        assert_eq!(
            hints_for(learned.clone(), dictionary.clone(), Vec::new()),
            vec!["Adi", "Kubernetes", "OurCompany"],
            "once switched, the dictionary rides along"
        );
        assert_eq!(
            hints_for(Vec::new(), dictionary.clone(), vec!["Priya".into()]),
            vec!["Kubernetes", "OurCompany", "Priya"],
        );
        assert_eq!(
            hints_for(learned, dictionary, vec!["Priya".into()]),
            vec!["Adi", "Kubernetes", "OurCompany", "Priya"],
            "learned, then the dictionary, then the field"
        );
    }

    #[test]
    fn learned_terms_wait_on_their_own_setting_and_no_other() {
        let store = crate::history::Store::open_in_memory().unwrap();
        store
            .vocab_observe("Eddie", "Adi", "Notes", 1, 10, 3)
            .unwrap();
        store.vocab_observe("AD", "Adi", "Notes", 1, 11, 3).unwrap();
        store
            .vocab_observe("A de", "Adi", "Notes", 2, 12, 3)
            .unwrap();
        let mut s = crate::settings::Settings::default();
        assert!(learned_terms(&s, &store).is_empty(), "off by default");
        s.privacy.read_focused_field = true;
        assert!(
            learned_terms(&s, &store).is_empty(),
            "reading the field does not turn learning on"
        );
        s.privacy.read_focused_field = false;
        s.learning.apply_learned_terms = true;
        assert_eq!(learned_terms(&s, &store), vec!["Adi"]);
    }

    #[test]
    fn utf16_offsets_land_on_character_boundaries() {
        let s = "aé😀b";
        assert_eq!(byte_offset_for_utf16(s, 0), 0);
        assert_eq!(byte_offset_for_utf16(s, 1), 1);
        assert_eq!(byte_offset_for_utf16(s, 2), 3);
        assert_eq!(byte_offset_for_utf16(s, 4), 7, "the emoji is two units");
        assert_eq!(byte_offset_for_utf16(s, 99), s.len());
        assert_eq!(slice_utf16(s, 1, 4), "é😀");
    }

    #[test]
    fn word_list_loads_and_answers_lower_case() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("words");
        std::fs::write(&p, "Zebra\napple\nThursday\n\napple\n").unwrap();
        let w = WordList::load(&p).unwrap();
        assert_eq!(w.len(), 3, "de-duplicated after lower-casing");
        assert!(w.contains("apple"));
        assert!(w.contains("thursday"));
        assert!(w.contains("zebra"));
        assert!(!w.contains("kubernetes"));
        assert!(!w.contains(""));
    }

    #[test]
    fn default_is_off() {
        assert!(
            !crate::settings::Settings::default()
                .privacy
                .read_focused_field
        );
    }
}
