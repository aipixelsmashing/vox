//! Undo the last dictation (docs/HOTKEYS.md, "Undo the last dictation").
//!
//! A shortcut takes the text Vox just placed back out of the field it placed it in, through
//! the accessibility API: select exactly that span, replace it with nothing, read the field
//! back. Not ⌘Z, which would be the app's own undo stack and might take something else with
//! it. The rule is the one insertion lives by (docs/adr/0005): if the field cannot be read,
//! or what it holds is not what was placed, nothing is changed and the user is told why.
//!
//! One record, replaced by every dictation. A dictation that ended on the clipboard leaves
//! a record that says so, rather than letting the shortcut reach back to an older one.
//! Nothing here logs the text; the log gets counts.

use parking_lot::Mutex;

use crate::inject::TextInjector;
use crate::learning::Located;
use crate::pipeline::InjectionTarget;

/// What the last dictation left behind.
pub enum Last {
    /// Placed in a field, and where. `located` is None when the field could not be read
    /// back right after the insertion: the shortcut then says so.
    Placed {
        pid: u32,
        app: String,
        #[cfg(target_os = "macos")]
        element: crate::inject::macos::ElementRef,
        located: Option<Located>,
    },
    /// Copied, not placed: there is nothing in a field to take back.
    NotPlaced,
}

static LAST: Mutex<Option<Last>> = Mutex::new(None);

/// Why the shortcut did nothing. Each has its words in the copy deck (docs/UI-STATES.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Nothing,
    NotPlaced,
    NotInFront(String),
    Unreadable,
    Changed,
    Rejected(String),
    Unconfirmed,
}

impl Refusal {
    pub fn user_message(&self) -> String {
        match self {
            Refusal::Nothing => "Nothing to undo.".into(),
            Refusal::NotPlaced => {
                "Nothing to undo — the last dictation was copied, not placed in a field.".into()
            }
            Refusal::NotInFront(app) => format!("Couldn't undo — {app} isn't in front."),
            Refusal::Unreadable => "Couldn't undo — the field can't be read.".into(),
            Refusal::Changed => "Couldn't undo — the text has changed since it was placed.".into(),
            Refusal::Rejected(app) => format!("Couldn't undo — {app} didn't accept the change."),
            Refusal::Unconfirmed => "Vox couldn't confirm the text was removed.".into(),
        }
    }
}

/// Where the insertion sits in the field now, as the accessibility API addresses it, and
/// what the field should hold once it is gone. None when the field no longer starts with
/// the text that was before the insertion followed by the insertion itself: an edit
/// before it or to it means the span is not ours to remove. Text typed after it is fine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub location: usize,
    pub length: usize,
    pub expected: String,
}

pub fn plan(value: &str, located: &Located) -> Option<Plan> {
    let rest = value
        .strip_prefix(located.prefix.as_str())?
        .strip_prefix(located.inserted.as_str())?;
    Some(Plan {
        location: located.prefix.encode_utf16().count(),
        length: located.inserted.encode_utf16().count(),
        expected: format!("{}{}", located.prefix, rest),
    })
}

/// A verified insertion just happened in `target`. Reads the field once to note where the
/// text sits; a field that cannot be read leaves a record that says so.
#[cfg(target_os = "macos")]
pub fn remember_placed(target: &InjectionTarget, inserted: &str) {
    use crate::learning::live;
    let Some(el) = target.element.clone() else {
        tracing::info!("undo: last dictation noted, no element to read back");
        *LAST.lock() = Some(Last::NotPlaced);
        return;
    };
    let located = (!live::refused(&el))
        .then(|| live::read_value(&el))
        .flatten()
        .and_then(|value| crate::learning::locate(&value, live::read_caret(&el), inserted));
    tracing::info!(
        "undo: last dictation noted, insertion {}",
        if located.is_some() {
            "located"
        } else {
            "not located"
        }
    );
    *LAST.lock() = Some(Last::Placed {
        pid: target.pid,
        app: target.app_name.clone(),
        element: el,
        located,
    });
}

#[cfg(not(target_os = "macos"))]
pub fn remember_placed(_target: &InjectionTarget, _inserted: &str) {
    *LAST.lock() = Some(Last::NotPlaced);
}

/// The last dictation went to the clipboard.
pub fn remember_not_placed() {
    *LAST.lock() = Some(Last::NotPlaced);
}

pub fn forget() {
    *LAST.lock() = None;
}

/// The shortcut. Removes the last insertion from its field, or says why not; the record is
/// cleared once the field has been touched, successfully or not, so a second press never
/// acts on a field in an unknown state.
pub fn undo_last(injector: &dyn TextInjector) -> Result<(), Refusal> {
    let mut guard = LAST.lock();
    match guard.as_ref() {
        None => Err(Refusal::Nothing),
        Some(Last::NotPlaced) => Err(Refusal::NotPlaced),
        #[cfg(target_os = "macos")]
        Some(Last::Placed {
            pid,
            app,
            element,
            located,
        }) => {
            if injector.frontmost().map(|(p, _)| p) != Some(*pid) {
                return Err(Refusal::NotInFront(app.clone()));
            }
            let Some(located) = located else {
                return Err(Refusal::Unreadable);
            };
            let result = live::remove(element, located, app);
            if !matches!(result, Err(Refusal::Unreadable | Refusal::Changed)) {
                // The field was written to: whatever happened, this record is spent. The
                // correction watch on that dictation, if any, now reads an empty span
                // where the insertion was, which is not a correction.
                *guard = None;
            }
            result
        }
        #[cfg(not(target_os = "macos"))]
        Some(Last::Placed { .. }) => {
            let _ = injector;
            Err(Refusal::Unreadable)
        }
    }
}

/// The hotkey thread's call: off that thread, and the refusal in the deck's words.
pub fn run(app: tauri::AppHandle, injector: std::sync::Arc<dyn TextInjector>) {
    std::thread::spawn(move || match undo_last(injector.as_ref()) {
        Ok(()) => tracing::info!("undo: last dictation removed"),
        Err(why) => {
            tracing::info!("undo: refused, {why:?}");
            crate::notify(&app, &why.user_message());
        }
    });
}

#[cfg(target_os = "macos")]
mod live {
    use std::time::{Duration, Instant};

    use axuielement::ax_attribute::{AX_SELECTED_TEXT_ATTRIBUTE, AX_SELECTED_TEXT_RANGE_ATTRIBUTE};
    use axuielement::AXRange;

    use super::{plan, Refusal};
    use crate::inject::macos::ElementRef;
    use crate::learning::{live as read, Located};

    /// Select the span and replace it with nothing, then read back until the field holds
    /// what it should, or the timeout passes.
    pub fn remove(el: &ElementRef, located: &Located, app: &str) -> Result<(), Refusal> {
        if read::refused(el) {
            return Err(Refusal::Unreadable);
        }
        let value = read::read_value(el).ok_or(Refusal::Unreadable)?;
        let plan = plan(&value, located).ok_or(Refusal::Changed)?;
        let range = AXRange {
            location: plan.location as isize,
            length: plan.length as isize,
        };
        if el
            .0
            .set_range_attribute(AX_SELECTED_TEXT_RANGE_ATTRIBUTE, range)
            .is_err()
            || el
                .0
                .set_string_attribute(AX_SELECTED_TEXT_ATTRIBUTE, "")
                .is_err()
        {
            return Err(Refusal::Rejected(app.to_string()));
        }
        let t0 = Instant::now();
        loop {
            if read::read_value(el).as_deref() == Some(plan.expected.as_str()) {
                tracing::info!("undo: removed {} characters", plan.length);
                return Ok(());
            }
            if t0.elapsed() >= Duration::from_millis(300) {
                return Err(Refusal::Unconfirmed);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(prefix: &str, inserted: &str, suffix: &str) -> Located {
        Located {
            prefix: prefix.into(),
            inserted: inserted.into(),
            suffix: suffix.into(),
        }
    }

    #[test]
    fn the_span_is_addressed_in_utf16_and_the_rest_is_kept() {
        let l = at("Hi 😀 ", "meet me at standup ", "bye");
        let p = plan("Hi 😀 meet me at standup bye", &l).unwrap();
        assert_eq!(p.location, 6, "the emoji is two UTF-16 units");
        assert_eq!(p.length, 19);
        assert_eq!(p.expected, "Hi 😀 bye");
    }

    #[test]
    fn typing_after_the_insertion_is_tolerated_but_not_before_or_within() {
        let l = at("Notes. ", "call prea now ", "");
        assert_eq!(
            plan("Notes. call prea now and thanks", &l)
                .unwrap()
                .expected,
            "Notes. and thanks"
        );
        assert!(plan("Notes! call prea now ", &l).is_none(), "edited before");
        assert!(
            plan("Notes. call Priya now ", &l).is_none(),
            "edited within"
        );
        assert!(plan("Notes. call prea no", &l).is_none(), "partly deleted");
        assert!(plan("", &l).is_none());
    }

    #[test]
    fn the_record_is_one_deep_and_a_clipboard_dictation_blocks_older_ones() {
        struct NoInjector;
        impl TextInjector for NoInjector {
            fn capture_target(&self) -> Result<InjectionTarget, crate::inject::Error> {
                unreachable!()
            }
            fn inject(
                &self,
                _: &str,
                _: &InjectionTarget,
            ) -> Result<crate::inject::InjectionOutcome, crate::inject::Error> {
                unreachable!()
            }
            fn frontmost(&self) -> Option<(u32, String)> {
                None
            }
        }
        forget();
        assert_eq!(undo_last(&NoInjector), Err(Refusal::Nothing));
        remember_not_placed();
        assert_eq!(undo_last(&NoInjector), Err(Refusal::NotPlaced));
        assert_eq!(
            undo_last(&NoInjector),
            Err(Refusal::NotPlaced),
            "a refusal that touched nothing keeps the record"
        );
        forget();
        assert_eq!(undo_last(&NoInjector), Err(Refusal::Nothing));
    }

    #[test]
    fn every_refusal_says_what_happened_and_never_apologises() {
        for r in [
            Refusal::Nothing,
            Refusal::NotPlaced,
            Refusal::NotInFront("Notes".into()),
            Refusal::Unreadable,
            Refusal::Changed,
            Refusal::Rejected("Terminal".into()),
            Refusal::Unconfirmed,
        ] {
            let m = r.user_message();
            assert!(!m.is_empty());
            assert!(!m.to_lowercase().contains("sorry"));
        }
        assert!(Refusal::NotInFront("Notes".into())
            .user_message()
            .contains("Notes"));
    }
}
