//! S2 — insertion and read-back spike.
//!
//! Question (ROADMAP.md, M0): does accessibility insertion work in the target apps, and can
//! the field be read back afterwards? This follows the macOS chain in docs/TEXT-INJECTION.md:
//!
//!   1. secure-input check            → refuse if IsSecureEventInputEnabled()
//!   2. accessibility insertion       → set AXSelectedText on the focused element,
//!                                       verify by caret advance and by re-reading AXValue
//!   3. clipboard paste               → NSPasteboard + concealed type, synthesised Cmd+V,
//!                                       verify by re-reading AXValue, restore the pasteboard
//!
//! Usage:
//!   s2-inject [--delay N] [--method auto|ax|paste|both] [--no-restore] [TEXT]
//!
//! After launch it counts down `--delay` seconds (default 5) so you can click into the target
//! field. Default method `auto` tries ax, then paste if ax could not be verified. `both` runs
//! ax and then paste as two separate insertions, which is what the compatibility matrix wants.
//!
//! Paste the whole output back, plus what you actually saw appear in the field.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use axuielement::ax_attribute::{
    AX_COMBO_BOX_ROLE, AX_FOCUSED_UI_ELEMENT_ATTRIBUTE, AX_NUMBER_OF_CHARACTERS_ATTRIBUTE,
    AX_ROLE_ATTRIBUTE, AX_SECURE_TEXT_FIELD_SUBROLE, AX_SELECTED_TEXT_ATTRIBUTE,
    AX_SELECTED_TEXT_RANGE_ATTRIBUTE, AX_SUBROLE_ATTRIBUTE, AX_TEXT_AREA_ROLE, AX_TEXT_FIELD_ROLE,
    AX_TITLE_ATTRIBUTE, AX_VALUE_ATTRIBUTE,
};
use axuielement::{is_process_trusted, is_process_trusted_with_prompt, system_wide, AXUIElement};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSRunningApplication};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
};
use objc2_foundation::NSString;

const DEFAULT_TEXT: &str = "vox s2 test 123 — ünïcödé ✓ ";
const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";
const K_VK_ANSI_V: u16 = 9;
const PASTE_VERIFY_TIMEOUT: Duration = Duration::from_millis(1500);

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Auto,
    Ax,
    Paste,
    Both,
}

struct Args {
    delay: u64,
    method: Method,
    restore: bool,
    probe: bool,
    text: String,
}

fn parse_args() -> Args {
    let mut a = Args {
        delay: 5,
        method: Method::Auto,
        restore: true,
        probe: false,
        text: DEFAULT_TEXT.to_string(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--delay" => a.delay = it.next().and_then(|v| v.parse().ok()).unwrap_or(5),
            "--method" => {
                a.method = match it.next().as_deref() {
                    Some("ax") => Method::Ax,
                    Some("paste") => Method::Paste,
                    Some("both") => Method::Both,
                    _ => Method::Auto,
                }
            }
            "--no-restore" => a.restore = false,
            "--probe" => a.probe = true,
            "-h" | "--help" => {
                println!(
                    "s2-inject [--delay N] [--method auto|ax|paste|both] [--no-restore] [--probe] [TEXT]"
                );
                std::process::exit(0);
            }
            other => a.text = other.to_string(),
        }
    }
    a
}

/// How many times `needle` occurs in a snapshot's value. Verification requires this to go up
/// by one, not merely to be non-zero: with `--method both` the text is already in the field.
fn occurrences(snap: &Snapshot, needle: &str) -> usize {
    snap.value.as_ref().map_or(0, |v| v.matches(needle).count())
}

fn utf16_len(s: &str) -> isize {
    s.encode_utf16().count() as isize
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n);
    let t: String = chars[start..].iter().collect();
    if start > 0 {
        format!("…{t:?}")
    } else {
        format!("{t:?}")
    }
}

fn sh(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "?".into())
}

// ---------------------------------------------------------------------------------------
// Target inspection
// ---------------------------------------------------------------------------------------

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

    fn describe(&self) -> String {
        let mut s = String::new();
        match self.range {
            Some((l, n)) => {
                let _ = write!(s, "range={l}+{n} ");
            }
            None => s.push_str("range=n/a "),
        }
        match self.chars {
            Some(c) => {
                let _ = write!(s, "chars={c} ");
            }
            None => s.push_str("chars=n/a "),
        }
        match &self.value {
            Some(v) => {
                let _ = write!(s, "value(len {}, tail)={}", v.chars().count(), tail(v, 40));
            }
            None => s.push_str("value=n/a"),
        }
        s
    }
}

struct Target {
    el: AXUIElement,
    role: String,
    subrole: String,
}

fn describe_app(pid: i32) -> String {
    // SAFETY: plain AppKit class method; safe to call off the main thread for lookups.
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    match app {
        Some(app) => {
            let name = app
                .localizedName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let bid = app
                .bundleIdentifier()
                .map(|s| s.to_string())
                .unwrap_or_default();
            format!("\"{name}\" ({bid}) pid {pid}")
        }
        None => format!(
            "pid {pid} ({})",
            sh("ps", &["-p", &pid.to_string(), "-o", "comm="])
        ),
    }
}

fn find_target() -> Result<Target, String> {
    // Designed path (TEXT-INJECTION.md): system-wide element → AXFocusedUIElement.
    let mut focused: Option<AXUIElement> = None;
    let mut via = "system-wide";
    match system_wide() {
        Some(sys) => {
            match sys
                .focused_application()
                .and_then(|a| a.map(|a| a.pid()).transpose())
            {
                Ok(Some(pid)) => println!("focused app:     {} (system-wide)", describe_app(pid)),
                Ok(None) => println!("focused app:     none (system-wide)"),
                Err(e) => println!("focused app:     system-wide error {e:?}"),
            }
            match sys.focused_ui_element() {
                Ok(Some(el)) => focused = Some(el),
                Ok(None) => println!("focused element: none (system-wide)"),
                Err(e) => println!("focused element: system-wide error {e:?}"),
            }
        }
        None => println!("focused app:     AXUIElementCreateSystemWide returned null"),
    }

    // Fallback observed necessary on macOS 26.5: the system-wide element answers
    // CannotComplete for everything while per-application elements work. Ask NSWorkspace for
    // the frontmost app and read AXFocusedUIElement from its application element instead.
    if focused.is_none() {
        via = "per-app";
        let front = objc2_app_kit::NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .ok_or("NSWorkspace has no frontmost application")?;
        let pid = front.processIdentifier();
        println!("focused app:     {} (NSWorkspace)", describe_app(pid));
        let app = AXUIElement::from_pid(pid).ok_or("AXUIElementCreateApplication failed")?;
        focused = app
            .element_attribute(AX_FOCUSED_UI_ELEMENT_ATTRIBUTE)
            .map_err(|e| format!("per-app AXFocusedUIElement: {e:?}"))?;
    }

    let el = focused.ok_or("no focused UI element by either route")?;
    println!("target via:      {via}");
    let attr = |n: &str| {
        el.string_attribute(n)
            .ok()
            .flatten()
            .unwrap_or_else(|| "-".into())
    };
    let role = attr(AX_ROLE_ATTRIBUTE);
    let subrole = attr(AX_SUBROLE_ATTRIBUTE);
    println!(
        "focused element: role={role} subrole={subrole} title={}",
        attr(AX_TITLE_ATTRIBUTE)
    );
    let settable = |n: &str| match el.is_attribute_settable(n) {
        Ok(true) => "yes",
        Ok(false) => "no",
        Err(_) => "err",
    };
    println!(
        "  settable:      AXSelectedText={} AXValue={} AXSelectedTextRange={}",
        settable(AX_SELECTED_TEXT_ATTRIBUTE),
        settable(AX_VALUE_ATTRIBUTE),
        settable(AX_SELECTED_TEXT_RANGE_ATTRIBUTE)
    );
    if let Ok(names) = el.attribute_names() {
        let interesting: Vec<&String> = names
            .iter()
            .filter(|n| n.contains("Text") || n.contains("Value") || n.contains("Characters"))
            .collect();
        println!("  text attrs:    {interesting:?}");
    }
    Ok(Target { el, role, subrole })
}

// ---------------------------------------------------------------------------------------
// Method 2: accessibility insertion
// ---------------------------------------------------------------------------------------

struct Outcome {
    method: &'static str,
    inserted: bool,
    verified_by: String,
    readback: Option<String>,
}

fn insert_ax(t: &Target, text: &str) -> Outcome {
    let before = Snapshot::take(&t.el);
    println!("[ax] before:     {}", before.describe());

    let allowed = [AX_TEXT_FIELD_ROLE, AX_TEXT_AREA_ROLE, AX_COMBO_BOX_ROLE];
    if !allowed.contains(&t.role.as_str()) {
        println!(
            "[ax] role {:?} is outside the designed allow-list; trying anyway (spike only)",
            t.role
        );
    }

    let t0 = Instant::now();
    let set = t.el.set_string_attribute(AX_SELECTED_TEXT_ATTRIBUTE, text);
    let set_ms = ms(t0);
    match &set {
        Ok(()) => println!("[ax] set AXSelectedText: ok in {set_ms:.2}ms"),
        Err(e) => println!("[ax] set AXSelectedText: FAILED {e:?} in {set_ms:.2}ms"),
    }

    // Give the target a moment to update its AX tree; some apps (web areas) lag.
    std::thread::sleep(Duration::from_millis(50));
    let after = Snapshot::take(&t.el);
    println!("[ax] after:      {}", after.describe());

    let want = utf16_len(text);
    let mut evidence = Vec::new();
    if let (Some((l0, _)), Some((l1, _))) = (before.range, after.range) {
        let adv = l1 - l0;
        let ok = adv == want;
        println!(
            "[ax] caret:      advanced {adv} UTF-16 units, expected {want} → {}",
            if ok { "match" } else { "MISMATCH" }
        );
        if ok {
            evidence.push("caret");
        }
    } else {
        println!("[ax] caret:      AXSelectedTextRange unavailable, cannot verify by caret");
    }
    if let (Some(c0), Some(c1)) = (before.chars, after.chars) {
        let ok = c1 - c0 == want as i64;
        println!(
            "[ax] chars:      {c0} → {c1} (+{}), expected +{want} → {}",
            c1 - c0,
            if ok { "match" } else { "MISMATCH" }
        );
        if ok {
            evidence.push("count");
        }
    }
    let mut readback = None;
    match &after.value {
        Some(v) => {
            let (n0, n1) = (occurrences(&before, text), occurrences(&after, text));
            let found = n1 == n0 + 1;
            println!(
                "[ax] read-back:  AXValue occurrences of the text {n0} → {n1} → {}",
                if found {
                    "one new copy"
                } else {
                    "NOT one new copy"
                }
            );
            if found {
                evidence.push("readback");
            }
            readback = Some(v.clone());
        }
        None => println!("[ax] read-back:  AXValue unreadable"),
    }

    let inserted = set.is_ok() && !evidence.is_empty();
    Outcome {
        method: "ax",
        inserted,
        verified_by: if evidence.is_empty() {
            "none".into()
        } else {
            evidence.join("+")
        },
        readback,
    }
}

// ---------------------------------------------------------------------------------------
// Method 3: clipboard paste
// ---------------------------------------------------------------------------------------

fn post_cmd_v() -> Result<f64, String> {
    let t0 = Instant::now();
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    let down = CGEvent::new_keyboard_event(src.as_deref(), K_VK_ANSI_V, true)
        .ok_or("CGEventCreateKeyboardEvent(down) failed")?;
    let up = CGEvent::new_keyboard_event(src.as_deref(), K_VK_ANSI_V, false)
        .ok_or("CGEventCreateKeyboardEvent(up) failed")?;
    // Exactly Command, nothing residual from whatever is physically held.
    CGEvent::set_flags(Some(&down), CGEventFlags::MaskCommand);
    CGEvent::set_flags(Some(&up), CGEventFlags::MaskCommand);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&down));
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&up));
    Ok(ms(t0))
}

fn insert_paste(t: Option<&Target>, text: &str, restore: bool) -> Outcome {
    let before = t.map(|t| Snapshot::take(&t.el));
    if let Some(b) = &before {
        println!("[paste] before:  {}", b.describe());
    }

    let pb = NSPasteboard::generalPasteboard();
    // SAFETY: reading a static extern constant.
    let string_type = unsafe { NSPasteboardTypeString };
    let saved = pb.stringForType(string_type).map(|s| s.to_string());
    let saved_types: Vec<String> = pb
        .types()
        .map(|a| a.iter().map(|t| t.to_string()).collect())
        .unwrap_or_default();
    let cc0 = pb.changeCount();
    println!(
        "[paste] saved:   changeCount={cc0} types={saved_types:?} string={}",
        saved
            .as_ref()
            .map(|s| format!("{} chars", s.chars().count()))
            .unwrap_or("none".into())
    );

    pb.clearContents();
    let ok_text = pb.setString_forType(&NSString::from_str(text), string_type);
    let ok_conceal =
        pb.setString_forType(&NSString::from_str(""), &NSString::from_str(CONCEALED_TYPE));
    let cc1 = pb.changeCount();
    println!("[paste] wrote:   string={ok_text} concealed={ok_conceal} changeCount={cc1}");

    match post_cmd_v() {
        Ok(t) => {
            println!("[paste] posted:  Cmd+V (kc {K_VK_ANSI_V}, flags Command only) in {t:.2}ms")
        }
        Err(e) => println!("[paste] posted:  FAILED {e}"),
    }

    // Verify by evidence from the target's AX tree, bounded by the design's 1500 ms.
    let want = utf16_len(text);
    let t0 = Instant::now();
    let mut evidence = Vec::new();
    let mut readback = None;
    let mut last: Option<Snapshot> = None;
    if let (Some(t), Some(b)) = (t, &before) {
        while t0.elapsed() < PASTE_VERIFY_TIMEOUT {
            let now = Snapshot::take(&t.el);
            let by_caret =
                matches!((b.range, now.range), (Some((l0, _)), Some((l1, _))) if l1 - l0 == want);
            let by_count =
                matches!((b.chars, now.chars), (Some(c0), Some(c1)) if c1 - c0 == want as i64);
            let by_value = occurrences(&now, text) == occurrences(b, text) + 1;
            if by_caret || by_count || by_value {
                if by_caret {
                    evidence.push("caret");
                }
                if by_count {
                    evidence.push("count");
                }
                if by_value {
                    evidence.push("readback");
                }
                last = Some(now);
                break;
            }
            last = Some(now);
            std::thread::sleep(Duration::from_millis(10));
        }
        let waited = ms(t0);
        match last {
            Some(s) if !evidence.is_empty() => {
                println!(
                    "[paste] verify:  evidence after {waited:.0}ms via {} — {}",
                    evidence.join("+"),
                    s.describe()
                );
                readback = s.value;
            }
            Some(s) => {
                println!(
                    "[paste] verify:  NO evidence within {waited:.0}ms — {}",
                    s.describe()
                );
                readback = s.value;
            }
            None => println!("[paste] verify:  no snapshot"),
        }
    } else {
        println!(
            "[paste] verify:  no AX target to read back; waiting {PASTE_VERIFY_TIMEOUT:?} blind"
        );
        std::thread::sleep(PASTE_VERIFY_TIMEOUT);
    }

    // Restore policy from TEXT-INJECTION.md: on timeout, leave the transcript on the clipboard.
    let cc_now = pb.changeCount();
    if cc_now != cc1 {
        println!("[paste] restore: skipped, something else wrote the pasteboard (changeCount {cc1}→{cc_now})");
    } else if !restore {
        println!("[paste] restore: skipped (--no-restore)");
    } else if evidence.is_empty() {
        println!("[paste] restore: skipped, insertion unverified so the transcript stays on the clipboard");
    } else {
        pb.clearContents();
        if let Some(s) = &saved {
            pb.setString_forType(&NSString::from_str(s), string_type);
        }
        println!(
            "[paste] restore: previous string {} (changeCount={}); non-string types {:?} were NOT restored",
            if saved.is_some() { "put back" } else { "was empty, left empty" },
            pb.changeCount(),
            saved_types.iter().filter(|t| *t != "public.utf8-plain-text").collect::<Vec<_>>()
        );
    }

    Outcome {
        method: "paste",
        inserted: !evidence.is_empty(),
        verified_by: if evidence.is_empty() {
            "none".into()
        } else {
            evidence.join("+")
        },
        readback,
    }
}

// ---------------------------------------------------------------------------------------

fn report(o: &Outcome) {
    println!(
        "RESULT method={} outcome={} verified_by={} readback={}",
        o.method,
        if o.inserted {
            "Inserted"
        } else {
            "ClipboardOnly{Unverified}"
        },
        o.verified_by,
        match &o.readback {
            Some(v) => format!("yes ({} chars)", v.chars().count()),
            None => "no".into(),
        }
    );
}

// ---------------------------------------------------------------------------------------
// --probe: read-only diagnosis of the AX read path. Compares the Swift bridge with the raw
// C API and prints what kind of process we are, so a CannotComplete can be attributed.
// ---------------------------------------------------------------------------------------

extern "C" {
    fn sandbox_check(pid: i32, operation: *const std::ffi::c_char, kind: i32, ...) -> i32;
}

fn ax_err_name(code: i32) -> &'static str {
    match code {
        0 => "Success",
        -25200 => "Failure",
        -25201 => "IllegalArgument",
        -25202 => "InvalidUIElement",
        -25204 => "CannotComplete",
        -25205 => "AttributeUnsupported",
        -25211 => "APIDisabled",
        -25212 => "NoValue",
        _ => "?",
    }
}

fn probe() {
    use axuielement::ffi::*;
    use std::ptr;

    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| "-".into());
    println!("== probe ==");
    println!(
        "pid {}  ppid {}  parent comm {}",
        std::process::id(),
        unsafe { libc_getppid() },
        sh(
            "ps",
            &["-o", "comm=", "-p", &unsafe { libc_getppid() }.to_string()]
        )
    );
    println!(
        "TERM_PROGRAM={}  __CFBundleIdentifier={}",
        env("TERM_PROGRAM"),
        env("__CFBundleIdentifier")
    );
    // SAFETY: private but stable libsystem_sandbox call; NULL operation asks "is pid sandboxed".
    let sb = unsafe { sandbox_check(std::process::id() as i32, ptr::null(), 0) };
    println!("sandbox_check(self) = {sb}  (0 = not sandboxed)");
    println!("AXIsProcessTrusted (raw) = {}", unsafe {
        AXIsProcessTrusted()
    });

    // Raw system-wide → AXFocusedApplication.
    let cf = |s: &str| unsafe {
        CFStringCreateWithCString(
            kCFAllocatorDefault,
            format!("{s}\0").as_ptr().cast(),
            0x0800_0100,
        )
    };
    let sys = unsafe { AXUIElementCreateSystemWide() };
    let mut out: CFTypeRef = ptr::null_mut();
    let e = unsafe { AXUIElementCopyAttributeValue(sys, cf("AXFocusedApplication"), &mut out) };
    println!(
        "raw system-wide AXFocusedApplication → {e} {}",
        ax_err_name(e)
    );
    let mut out2: CFTypeRef = ptr::null_mut();
    let e = unsafe { AXUIElementCopyAttributeValue(sys, cf("AXFocusedUIElement"), &mut out2) };
    println!(
        "raw system-wide AXFocusedUIElement   → {e} {}",
        ax_err_name(e)
    );

    // Raw per-application element for the frontmost app, bypassing the system-wide element.
    // SAFETY: NSWorkspace shared instance is safe to read from any thread for this query.
    let front = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication();
    match front {
        Some(app) => {
            let pid = app.processIdentifier();
            println!("NSWorkspace frontmost: {}", describe_app(pid));
            let el = unsafe { AXUIElementCreateApplication(pid) };
            let mut role: CFTypeRef = ptr::null_mut();
            let e = unsafe { AXUIElementCopyAttributeValue(el, cf("AXRole"), &mut role) };
            println!(
                "raw app({pid}) AXRole               → {e} {}",
                ax_err_name(e)
            );
            let mut fe: CFTypeRef = ptr::null_mut();
            let e = unsafe { AXUIElementCopyAttributeValue(el, cf("AXFocusedUIElement"), &mut fe) };
            println!(
                "raw app({pid}) AXFocusedUIElement   → {e} {}",
                ax_err_name(e)
            );
            if e == 0 && !fe.is_null() {
                let mut r: CFTypeRef = ptr::null_mut();
                let e =
                    unsafe { AXUIElementCopyAttributeValue(fe.cast_mut(), cf("AXRole"), &mut r) };
                println!(
                    "raw focused element AXRole          → {e} {}",
                    ax_err_name(e)
                );
            }
        }
        None => println!("NSWorkspace frontmost: none"),
    }

    // Bridge, for comparison.
    match system_wide() {
        Some(sw) => {
            println!(
                "bridge focused_application → {:?}",
                sw.focused_application().map(|o| o.is_some())
            );
            println!(
                "bridge focused_ui_element  → {:?}",
                sw.focused_ui_element().map(|o| o.is_some())
            );
        }
        None => println!("bridge system_wide() → None"),
    }
}

extern "C" {
    #[link_name = "getppid"]
    fn libc_getppid() -> i32;
}

fn main() {
    let args = parse_args();
    if args.probe {
        probe();
        return;
    }
    println!("== S2 inject spike ==");
    println!(
        "macOS {} ({})  axuielement 0.9  text={:?} ({} UTF-16 units)",
        sh("sw_vers", &["-productVersion"]),
        sh("sw_vers", &["-buildVersion"]),
        args.text,
        utf16_len(&args.text)
    );

    let trusted = is_process_trusted();
    println!("Accessibility trusted: {trusted}");
    if !trusted {
        let after = is_process_trusted_with_prompt();
        println!("Requested via prompt → {after}");
        if !after {
            println!();
            println!(
                "Grant Accessibility to the app you launched this from (Terminal / iTerm), in"
            );
            println!("System Settings → Privacy & Security → Accessibility, then re-run.");
            std::process::exit(1);
        }
    }

    print!("Click into the target field now. Starting in");
    for i in (1..=args.delay).rev() {
        print!(" {i}…");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_secs(1));
    }
    println!();

    // 1. Secure input.
    // SAFETY: plain Carbon query.
    let secure = unsafe { IsSecureEventInputEnabled() } != 0;
    println!(
        "secure input:    {}",
        if secure {
            "ENABLED — refusing to insert"
        } else {
            "off"
        }
    );
    if secure {
        println!("RESULT method=none outcome=ClipboardOnly{{SecureInput}}");
        return;
    }

    // Target.
    let target = match find_target() {
        Ok(t) => {
            if t.subrole == AX_SECURE_TEXT_FIELD_SUBROLE {
                println!("RESULT method=none outcome=Refused{{SecureTextField}}");
                return;
            }
            Some(t)
        }
        Err(e) => {
            println!("focused element: unavailable ({e})");
            None
        }
    };

    let mut outcomes = Vec::new();
    match args.method {
        Method::Ax => {
            if let Some(t) = &target {
                outcomes.push(insert_ax(t, &args.text));
            } else {
                println!("RESULT method=ax outcome=ClipboardOnly{{NoTextTarget}}");
            }
        }
        Method::Paste => outcomes.push(insert_paste(target.as_ref(), &args.text, args.restore)),
        Method::Both => {
            if let Some(t) = &target {
                outcomes.push(insert_ax(t, &args.text));
            }
            println!("-- now the paste path, same field --");
            outcomes.push(insert_paste(target.as_ref(), &args.text, args.restore));
        }
        Method::Auto => {
            let ax = target.as_ref().map(|t| insert_ax(t, &args.text));
            match ax {
                Some(o) if o.inserted => outcomes.push(o),
                other => {
                    if let Some(o) = other {
                        outcomes.push(o);
                    }
                    println!("-- ax not verified, falling through to paste --");
                    outcomes.push(insert_paste(target.as_ref(), &args.text, args.restore));
                }
            }
        }
    }

    println!();
    println!("== summary ==");
    for o in &outcomes {
        report(o);
    }
    if let Some(t) = &target {
        println!("role={} subrole={}", t.role, t.subrole);
    }
    println!("Now say what you actually saw appear in the field.");
}
