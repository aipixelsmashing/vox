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
    /// Apps where the accessibility write reported success and never took (Chromium does
    /// this). Remembered so the next dictation there goes straight to paste instead of
    /// waiting the verification timeout. Keyed by bundle id, persisted to the data dir as a
    /// cache (not a setting: nothing for the user to decide, docs/adr/0010).
    ax_unusable: parking_lot::Mutex<std::collections::HashSet<String>>,
}

fn memo_path() -> Option<std::path::PathBuf> {
    settings::data_dir()
        .ok()
        .map(|d| d.join("injection-memo.json"))
}

fn load_memo() -> std::collections::HashSet<String> {
    memo_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

fn save_memo(set: &std::collections::HashSet<String>) {
    let Some(p) = memo_path() else { return };
    let mut v: Vec<&String> = set.iter().collect();
    v.sort();
    if let Ok(json) = serde_json::to_string_pretty(&v) {
        let _ = std::fs::write(p, json);
    }
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

/// Chromium, and therefore every Electron app, does not build its accessibility tree until
/// an assistive client asks for it. VoiceOver asks by setting `AXEnhancedUserInterface` on the
/// application element; Electron additionally honours `AXManualAccessibility`. Both are
/// idempotent and only sent when the ordinary routes found nothing, because
/// AXEnhancedUserInterface also changes some window behaviours in the target.
fn wake_chromium_accessibility(app: &AXUIElement) {
    let _ = app.set_bool_attribute("AXEnhancedUserInterface", true);
    let _ = app.set_bool_attribute("AXManualAccessibility", true);
}

const WAKE_POLL: Duration = Duration::from_millis(700);

/// The focused element and which route found it: system-wide first (the documented route),
/// then through the frontmost application's element, which is what actually works on
/// macOS 26.5, then once more after asking Chromium to build its tree
/// (docs/TEXT-INJECTION.md#macos).
fn focused_element(pid: u32) -> (Option<AXUIElement>, &'static str) {
    if let Some(sys) = system_wide() {
        if let Ok(Some(el)) = sys.focused_ui_element() {
            return (Some(el), "system-wide");
        }
    }
    let Some(app) = AXUIElement::from_pid(pid as i32) else {
        return (None, "no-app-element");
    };
    if let Ok(Some(el)) = app.element_attribute(AX_FOCUSED_UI_ELEMENT_ATTRIBUTE) {
        return (Some(el), "per-app");
    }
    wake_chromium_accessibility(&app);
    let t0 = Instant::now();
    while t0.elapsed() < WAKE_POLL {
        std::thread::sleep(Duration::from_millis(30));
        if let Ok(Some(el)) = app.element_attribute(AX_FOCUSED_UI_ELEMENT_ATTRIBUTE) {
            return (Some(el), "per-app-after-wake");
        }
    }
    (None, "none")
}

/// Diagnostic only, logged when no focused element could be found: what the focused window
/// contains, so the compatibility matrix can say why an app failed. Never logs text.
fn describe_focused_window(pid: u32) -> String {
    let Some(app) = AXUIElement::from_pid(pid as i32) else {
        return "no app element".into();
    };
    let Ok(Some(win)) = app.element_attribute("AXFocusedWindow") else {
        return "no focused window".into();
    };
    let mut queue = std::collections::VecDeque::from([win]);
    let mut roles: std::collections::BTreeMap<String, usize> = Default::default();
    let mut scanned = 0;
    while let Some(el) = queue.pop_front() {
        if scanned >= 300 {
            break;
        }
        scanned += 1;
        let role = el
            .string_attribute(AX_ROLE_ATTRIBUTE)
            .ok()
            .flatten()
            .unwrap_or_else(|| "?".into());
        *roles.entry(role).or_default() += 1;
        if let Ok(kids) = el.children() {
            queue.extend(kids);
        }
    }
    format!("{scanned} elements scanned; roles {roles:?}")
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
        let memo = load_memo();
        if !memo.is_empty() {
            tracing::info!("inject: {} app(s) remembered as paste-only", memo.len());
        }
        Self {
            settings,
            ax_unusable: parking_lot::Mutex::new(memo),
        }
    }

    fn ax_known_unusable(&self, target: &InjectionTarget) -> bool {
        target
            .bundle_id
            .as_ref()
            .map(|b| self.ax_unusable.lock().contains(b))
            .unwrap_or(false)
    }

    fn remember_ax_unusable(&self, target: &InjectionTarget) {
        if let Some(b) = &target.bundle_id {
            let mut set = self.ax_unusable.lock();
            if set.insert(b.clone()) {
                save_memo(&set);
            }
        }
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
            // No read-back available: the paste is still posted, because the app is the one
            // the user held the key in and revalidation confirmed it is still frontmost, but
            // nothing can prove delivery, so the outcome stays ClipboardOnly (docs/adr/0005)
            // and the transcript stays on the clipboard. Give the target a moment to read it.
            _ => {
                std::thread::sleep(Duration::from_millis(150));
                false
            }
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
        let (element, route) = focused_element(pid);
        tracing::info!(
            "target: {} via {route}, element {}",
            bundle_id.as_deref().unwrap_or(&app_name),
            if element.is_some() { "found" } else { "none" }
        );
        Ok(InjectionTarget {
            pid,
            app_name,
            bundle_id,
            element: element.map(ElementRef),
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

        // Keep the captured element if it is still alive; otherwise look again now.
        let (el, route) = match target.element.as_ref().filter(|e| e.0.pid().is_ok()) {
            Some(e) => (Some(e.0.clone()), "captured"),
            None => focused_element(target.pid),
        };

        let attr = |el: &AXUIElement, n: &str| el.string_attribute(n).ok().flatten();
        let (role, subrole, settable) = match &el {
            Some(el) => (
                attr(el, AX_ROLE_ATTRIBUTE),
                attr(el, AX_SUBROLE_ATTRIBUTE),
                el.is_attribute_settable(AX_SELECTED_TEXT_ATTRIBUTE)
                    .unwrap_or(false),
            ),
            None => (None, None, false),
        };
        tracing::info!(
            "inject: route {route}, role {:?}, subrole {:?}, selected-text settable {settable}, method setting {method}",
            role, subrole
        );
        if el.is_none() {
            tracing::info!(
                "inject: no focused element; {}",
                describe_focused_window(target.pid)
            );
        }
        if subrole.as_deref() == Some(AX_SECURE_TEXT_FIELD_SUBROLE) {
            return Ok(InjectionOutcome::ClipboardOnly {
                reason: FallbackReason::PasswordField,
            });
        }
        let text_role = matches!(
            role.as_deref(),
            Some(AX_TEXT_FIELD_ROLE) | Some(AX_TEXT_AREA_ROLE) | Some(AX_COMBO_BOX_ROLE)
        );
        let elapsed = |t0: Instant| t0.elapsed().as_millis() as u32;

        // 1. Accessibility: any element that is a text role or says its selected text is
        // settable. Verification decides, not the role. Skipped in "auto" for apps where it
        // has already failed this session, which saves the verification wait every time.
        let skip_ax = method == "auto" && self.ax_known_unusable(target);
        if skip_ax {
            tracing::info!("inject: accessibility skipped, known not to take in this app");
        }
        if matches!(method.as_str(), "auto" | "accessibility") && !skip_ax {
            if let Some(el) = el.as_ref().filter(|_| text_role || settable) {
                let ok = self.try_accessibility(el, text);
                tracing::info!(
                    "inject: accessibility {}",
                    if ok { "verified" } else { "not verified" }
                );
                if ok {
                    return Ok(InjectionOutcome::Inserted {
                        method: Method::Accessibility,
                        elapsed_ms: elapsed(t0),
                    });
                }
                self.remember_ax_unusable(target);
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
        // 2. Paste, verified by read-back where the field can be read, blind otherwise.
        if matches!(method.as_str(), "auto" | "paste") {
            let ok = self.try_paste(el.as_ref(), text, restore)?;
            tracing::info!(
                "inject: paste {}",
                if ok {
                    "verified"
                } else if el.is_some() {
                    "not verified"
                } else {
                    "blind"
                }
            );
            if ok {
                return Ok(InjectionOutcome::Inserted {
                    method: Method::Paste,
                    elapsed_ms: elapsed(t0),
                });
            }
            if method == "paste" || el.is_none() {
                return Ok(InjectionOutcome::ClipboardOnly {
                    reason: FallbackReason::MethodFailed(Method::Paste),
                });
            }
        }
        // 3. Unicode events. Only when explicitly chosen: in "auto" a failed paste has already
        // left the transcript on the clipboard, and typing on top would duplicate it.
        if method == "type" {
            let ok = self.try_unicode(el.as_ref(), text)?;
            tracing::info!(
                "inject: unicode {}",
                if ok { "verified" } else { "not verified" }
            );
            if ok {
                return Ok(InjectionOutcome::Inserted {
                    method: Method::Type,
                    elapsed_ms: elapsed(t0),
                });
            }
            return Ok(InjectionOutcome::ClipboardOnly {
                reason: FallbackReason::MethodFailed(Method::Type),
            });
        }
        Ok(InjectionOutcome::ClipboardOnly {
            reason: FallbackReason::MethodFailed(Method::Paste),
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
