//! macOS injection chain: secure-input check → accessibility insert → clipboard paste →
//! Unicode key events. See docs/TEXT-INJECTION.md#macos and docs/spikes/s2-injection.md.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axuielement::ax_attribute::{
    AX_COMBO_BOX_ROLE, AX_FOCUSED_UI_ELEMENT_ATTRIBUTE, AX_NUMBER_OF_CHARACTERS_ATTRIBUTE,
    AX_ROLE_ATTRIBUTE, AX_SECURE_TEXT_FIELD_SUBROLE, AX_SELECTED_TEXT_ATTRIBUTE,
    AX_SELECTED_TEXT_RANGE_ATTRIBUTE, AX_SUBROLE_ATTRIBUTE, AX_TEXT_AREA_ROLE, AX_TEXT_FIELD_ROLE,
    AX_VALUE_ATTRIBUTE,
};
use axuielement::{system_wide, AXUIElement};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
};
use parking_lot::RwLock;

use super::{Error, FallbackReason, InjectionOutcome, Method, TextInjector};
use crate::pipeline::InjectionTarget;
use crate::{clipboard, permissions, settings};

const K_VK_ANSI_V: u16 = 9;
const PASTE_VERIFY_TIMEOUT: Duration = Duration::from_millis(1500);
const TYPE_VERIFY_TIMEOUT: Duration = Duration::from_millis(800);
const UNICODE_CHUNK: usize = 20;

/// A retained AXUIElement. CF objects are safe to retain, release and message from any
/// thread; the pipeline thread is the only one that uses it.
#[derive(Debug, Clone)]
pub struct ElementRef(pub AXUIElement);
unsafe impl Send for ElementRef {}
unsafe impl Sync for ElementRef {}

pub struct MacInjector {
    settings: Arc<RwLock<settings::Settings>>,
}

/// What we know about the field before and after an insertion.
#[derive(Debug, Clone)]
struct Snapshot {
    value: Option<String>,
    range: Option<(isize, isize)>,
    chars: Option<i64>,
}

impl Snapshot {
    fn take(el: &AXUIElement) -> Self {
        Self {
            value: el.string_attribute(AX_VALUE_ATTRIBUTE).ok().flatten(),
            range: el
                .range_attribute(AX_SELECTED_TEXT_RANGE_ATTRIBUTE)
                .ok()
                .flatten()
                .map(|r| (r.location, r.length)),
            chars: el
                .i64_attribute(AX_NUMBER_OF_CHARACTERS_ATTRIBUTE)
                .ok()
                .flatten(),
        }
    }

    fn readable(&self) -> bool {
        self.value.is_some() || self.range.is_some() || self.chars.is_some()
    }
}

fn utf16_len(s: &str) -> isize {
    s.encode_utf16().count() as isize
}

fn occurrences(snap: &Snapshot, needle: &str) -> usize {
    snap.value.as_ref().map_or(0, |v| v.matches(needle).count())
}

/// Did `text` arrive between `before` and `after`? Tolerates the target trimming trailing
/// whitespace (the Chrome/Brave omnibox does), which is reported as `normalised`.
/// Returns None when nothing proves delivery.
fn verify(before: &Snapshot, after: &Snapshot, text: &str) -> Option<bool> {
    let trimmed = text.trim_end();
    let (want, want_t) = (utf16_len(text), utf16_len(trimmed));
    let check = |delta: isize| -> Option<bool> {
        if delta == want {
            Some(false)
        } else if delta == want_t && want_t != want {
            Some(true)
        } else {
            None
        }
    };
    let mut normalised = None;
    if let (Some((l0, _)), Some((l1, _))) = (before.range, after.range) {
        if let Some(n) = check(l1 - l0) {
            normalised = Some(n);
        }
    }
    if let (Some(c0), Some(c1)) = (before.chars, after.chars) {
        if let Some(n) = check((c1 - c0) as isize) {
            normalised = Some(normalised.unwrap_or(n));
        }
    }
    if after.value.is_some() {
        if occurrences(after, text) == occurrences(before, text) + 1 {
            normalised = Some(normalised.unwrap_or(false));
        } else if occurrences(after, trimmed) == occurrences(before, trimmed) + 1 {
            normalised = Some(normalised.unwrap_or(true));
        }
    }
    normalised
}

/// Polls the field until `verify` says yes or the timeout passes.
fn verify_within(el: &AXUIElement, before: &Snapshot, text: &str, timeout: Duration) -> bool {
    let t0 = Instant::now();
    loop {
        let now = Snapshot::take(el);
        if verify(before, &now, text).is_some() {
            return true;
        }
        if t0.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn frontmost_app() -> Option<(u32, String, Option<String>)> {
    let app: objc2::rc::Retained<NSRunningApplication> =
        NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier() as u32;
    let name = app
        .localizedName()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("pid {pid}"));
    let bundle = app.bundleIdentifier().map(|s| s.to_string());
    Some((pid, name, bundle))
}

/// The focused element: system-wide first (the documented route), then through the frontmost
/// application's element, which is what actually works on macOS 26.5
/// (docs/TEXT-INJECTION.md#macos).
fn focused_element(pid: u32) -> Option<AXUIElement> {
    if let Some(sys) = system_wide() {
        if let Ok(Some(el)) = sys.focused_ui_element() {
            return Some(el);
        }
    }
    let app = AXUIElement::from_pid(pid as i32)?;
    app.element_attribute(AX_FOCUSED_UI_ELEMENT_ATTRIBUTE)
        .ok()
        .flatten()
}

fn post_cmd_v() -> Result<(), Error> {
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    let down = CGEvent::new_keyboard_event(src.as_deref(), K_VK_ANSI_V, true)
        .ok_or_else(|| Error::Platform("CGEventCreateKeyboardEvent failed".into()))?;
    let up = CGEvent::new_keyboard_event(src.as_deref(), K_VK_ANSI_V, false)
        .ok_or_else(|| Error::Platform("CGEventCreateKeyboardEvent failed".into()))?;
    // Exactly Command: the push-to-talk modifier may still be physically held.
    CGEvent::set_flags(Some(&down), CGEventFlags::MaskCommand);
    CGEvent::set_flags(Some(&up), CGEventFlags::MaskCommand);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&down));
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&up));
    Ok(())
}

fn post_unicode(text: &str) -> Result<(), Error> {
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    let units: Vec<u16> = text.encode_utf16().collect();
    for chunk in units.chunks(UNICODE_CHUNK) {
        let down = CGEvent::new_keyboard_event(src.as_deref(), 0, true)
            .ok_or_else(|| Error::Platform("CGEventCreateKeyboardEvent failed".into()))?;
        let up = CGEvent::new_keyboard_event(src.as_deref(), 0, false)
            .ok_or_else(|| Error::Platform("CGEventCreateKeyboardEvent failed".into()))?;
        CGEvent::set_flags(Some(&down), CGEventFlags::empty());
        CGEvent::set_flags(Some(&up), CGEventFlags::empty());
        // SAFETY: chunk is a valid UTF-16 buffer of the stated length for the call's duration.
        unsafe {
            CGEvent::keyboard_set_unicode_string(Some(&down), chunk.len() as _, chunk.as_ptr());
            CGEvent::keyboard_set_unicode_string(Some(&up), chunk.len() as _, chunk.as_ptr());
        }
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&down));
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&up));
    }
    Ok(())
}

impl MacInjector {
    pub fn new(settings: Arc<RwLock<settings::Settings>>) -> Self {
        Self { settings }
    }

    fn try_accessibility(&self, el: &AXUIElement, text: &str) -> bool {
        let before = Snapshot::take(el);
        if el
            .set_string_attribute(AX_SELECTED_TEXT_ATTRIBUTE, text)
            .is_err()
        {
            return false;
        }
        // Web areas update their AX tree a moment later; the poll covers it.
        verify_within(el, &before, text, Duration::from_millis(300))
    }

    fn try_paste(
        &self,
        el: Option<&AXUIElement>,
        text: &str,
        restore: bool,
    ) -> Result<bool, Error> {
        let before = el.map(Snapshot::take);
        let guard = clipboard::save();
        let written_at = clipboard::write_transcript(text)?;
        post_cmd_v()?;
        let verified = match (el, &before) {
            (Some(el), Some(b)) if b.readable() => verify_within(el, b, text, PASTE_VERIFY_TIMEOUT),
            // No read-back available: nothing can prove delivery (docs/adr/0005).
            _ => false,
        };
        if verified && restore {
            // Only restore if nothing else has written the pasteboard since we did.
            if clipboard::change_count() == written_at {
                if let Some(g) = guard {
                    g.restore();
                }
            }
        }
        Ok(verified)
    }

    fn try_unicode(&self, el: Option<&AXUIElement>, text: &str) -> Result<bool, Error> {
        let before = el.map(Snapshot::take);
        post_unicode(text)?;
        Ok(match (el, &before) {
            (Some(el), Some(b)) if b.readable() => verify_within(el, b, text, TYPE_VERIFY_TIMEOUT),
            _ => false,
        })
    }
}

impl TextInjector for MacInjector {
    fn capture_target(&self) -> Result<InjectionTarget, Error> {
        let (pid, app_name, bundle_id) =
            frontmost_app().ok_or_else(|| Error::Platform("no frontmost application".into()))?;
        let element = focused_element(pid).map(ElementRef);
        Ok(InjectionTarget {
            pid,
            app_name,
            bundle_id,
            element,
            captured_at: Instant::now(),
        })
    }

    fn inject(&self, text: &str, target: &InjectionTarget) -> Result<InjectionOutcome, Error> {
        let t0 = Instant::now();
        // Abort before anything else if a password field is focused (or another app leaked
        // the state). The caller drops the transcript — it does not even reach the clipboard.
        if permissions::secure_input_active() {
            return Ok(InjectionOutcome::ClipboardOnly {
                reason: FallbackReason::SecureInput,
            });
        }
        let (method, restore) = {
            let s = self.settings.read();
            (s.output.method.clone(), s.output.restore_clipboard)
        };

        // Re-resolve if the captured element is gone; keep the captured one otherwise.
        let el = target
            .element
            .as_ref()
            .filter(|e| e.0.pid().is_ok())
            .map(|e| e.0.clone())
            .or_else(|| focused_element(target.pid));

        let (role, subrole) = match &el {
            Some(el) => (
                el.string_attribute(AX_ROLE_ATTRIBUTE).ok().flatten(),
                el.string_attribute(AX_SUBROLE_ATTRIBUTE).ok().flatten(),
            ),
            None => (None, None),
        };
        if subrole.as_deref() == Some(AX_SECURE_TEXT_FIELD_SUBROLE) {
            return Ok(InjectionOutcome::ClipboardOnly {
                reason: FallbackReason::PasswordField,
            });
        }
        let ax_ok_role = matches!(
            role.as_deref(),
            Some(AX_TEXT_FIELD_ROLE) | Some(AX_TEXT_AREA_ROLE) | Some(AX_COMBO_BOX_ROLE)
        );
        let elapsed = |t0: Instant| t0.elapsed().as_millis() as u32;

        // 1. Accessibility.
        if matches!(method.as_str(), "auto" | "accessibility") {
            if let Some(el) = &el {
                if ax_ok_role && self.try_accessibility(el, text) {
                    return Ok(InjectionOutcome::Inserted {
                        method: Method::Accessibility,
                        elapsed_ms: elapsed(t0),
                    });
                }
            }
            if method == "accessibility" {
                return Ok(InjectionOutcome::ClipboardOnly {
                    reason: if el.is_none() {
                        FallbackReason::NoTextTarget
                    } else {
                        FallbackReason::MethodFailed(Method::Accessibility)
                    },
                });
            }
        }
        // With no element at all there is nothing to read back, so neither paste nor typing
        // can be verified. Say so rather than guess.
        if el.is_none() {
            return Ok(InjectionOutcome::ClipboardOnly {
                reason: FallbackReason::NoTextTarget,
            });
        }
        // 2. Paste.
        if matches!(method.as_str(), "auto" | "paste")
            && self.try_paste(el.as_ref(), text, restore)?
        {
            return Ok(InjectionOutcome::Inserted {
                method: Method::Paste,
                elapsed_ms: elapsed(t0),
            });
        }
        if method == "paste" {
            return Ok(InjectionOutcome::ClipboardOnly {
                reason: FallbackReason::MethodFailed(Method::Paste),
            });
        }
        // 3. Unicode events. Only when explicitly chosen: in "auto" a failed paste has already
        // left the transcript on the clipboard, and typing on top would duplicate it.
        if method == "type" && self.try_unicode(el.as_ref(), text)? {
            return Ok(InjectionOutcome::Inserted {
                method: Method::Type,
                elapsed_ms: elapsed(t0),
            });
        }
        Ok(InjectionOutcome::ClipboardOnly {
            reason: FallbackReason::MethodFailed(if method == "type" {
                Method::Type
            } else {
                Method::Paste
            }),
        })
    }

    fn frontmost(&self) -> Option<(u32, String)> {
        frontmost_app().map(|(pid, name, _)| (pid, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(value: &str, caret: isize) -> Snapshot {
        Snapshot {
            value: Some(value.into()),
            range: Some((caret, 0)),
            chars: Some(value.encode_utf16().count() as i64),
        }
    }

    #[test]
    fn exact_insert_verifies() {
        let b = snap("hello ", 6);
        let a = snap("hello world ", 12);
        assert_eq!(verify(&b, &a, "world "), Some(false));
    }

    #[test]
    fn trailing_space_trim_is_normalised_not_failed() {
        let b = snap("", 0);
        let a = snap("world", 5);
        assert_eq!(verify(&b, &a, "world "), Some(true));
    }

    #[test]
    fn nothing_changed_is_not_verified() {
        let b = snap("x", 1);
        assert_eq!(verify(&b, &b, "world "), None);
    }

    #[test]
    fn already_present_text_needs_one_new_copy() {
        let b = snap("world world ", 12);
        let a = snap("world world ", 12);
        assert_eq!(verify(&b, &a, "world "), None);
        let a2 = snap("world world world ", 18);
        assert_eq!(verify(&b, &a2, "world "), Some(false));
    }
}
