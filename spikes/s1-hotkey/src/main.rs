//! S1 — hotkey spike.
//!
//! Question (ROADMAP.md, M0): does `keytap` deliver right-Option down and up, from an
//! unfocused app, with the permissions granted? What is the event latency?
//!
//! What this prints, one line per event:
//!
//!   +   1234.567ms  DOWN  AltRight   kc=61  flags=0x00080140  hid→tap 0.112ms  tap→recv 0.031ms  hid→recv 0.143ms
//!
//! - `DOWN` / `UP` / `REP` and the key name come from keytap.
//! - `kc` and `flags` come from a second, independent listen-only CGEventTap this binary
//!   installs next to keytap's. keytap stamps `Event::time` inside its own callback, so on
//!   its own it cannot say how long the OS took to deliver the event. The second tap reads
//!   the kernel's HID timestamp (`CGEventGetTimestamp`) and the two streams are paired by
//!   callback time, which lands within microseconds for the same HID event.
//! - `hid→tap`   kernel HID timestamp → keytap's callback ran.
//! - `tap→recv`  keytap's callback → this program's main loop received it (channel hop).
//! - `hid→recv`  the sum: what the pipeline would actually see.
//!
//! At exit it prints a summary block. Paste the whole output back.
//!
//! Usage: `cargo run --release -p s1-hotkey [seconds]` (default 30).

use std::collections::{BTreeMap, VecDeque};
use std::ffi::c_void;
use std::process::Command;
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};
use keytap::{Event, EventKind, Key, Tap};
use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

// ---------------------------------------------------------------------------------------
// Mach time. CGEventGetTimestamp, mach_absolute_time and (on macOS) std::time::Instant all
// count the same uptime clock; the first two in ticks, Instant in nanoseconds.
// ---------------------------------------------------------------------------------------

fn timebase() -> (u64, u64) {
    static TB: OnceLock<(u64, u64)> = OnceLock::new();
    *TB.get_or_init(|| {
        let mut info = mach2::mach_time::mach_timebase_info { numer: 0, denom: 0 };
        // SAFETY: plain out-parameter call.
        unsafe { mach2::mach_time::mach_timebase_info(&mut info) };
        (u64::from(info.numer), u64::from(info.denom))
    })
}

fn now_ticks() -> u64 {
    // SAFETY: no preconditions.
    unsafe { mach2::mach_time::mach_absolute_time() }
}

fn ticks_to_ms(ticks: i128) -> f64 {
    let (n, d) = timebase();
    (ticks as f64) * (n as f64) / (d as f64) / 1_000_000.0
}

fn ns_to_ticks(ns: u128) -> u64 {
    let (n, d) = timebase();
    (ns * u128::from(d) / u128::from(n)) as u64
}

// ---------------------------------------------------------------------------------------
// Input Monitoring. keytap only *checks* (IOHIDCheckAccess); it never prompts. Requesting
// here makes the system dialog appear on first run instead of a silent PermissionDenied.
// ---------------------------------------------------------------------------------------

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDCheckAccess(request: u32) -> u32;
    fn IOHIDRequestAccess(request: u32) -> bool;
}
const IOHID_REQUEST_TYPE_LISTEN: u32 = 1;

fn access_name(v: u32) -> &'static str {
    match v {
        0 => "granted",
        1 => "denied",
        2 => "unknown (never asked)",
        _ => "?",
    }
}

// ---------------------------------------------------------------------------------------
// The reference tap: a second listen-only CGEventTap that records the kernel timestamp.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct RawRec {
    kind: &'static str,
    keycode: u32,
    flags: u64,
    hid_ticks: u64,
    cb_ticks: u64,
}

struct RawCtx {
    tx: Sender<RawRec>,
}

unsafe extern "C-unwind" fn raw_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    cg_event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let cb_ticks = now_ticks();
    // SAFETY: user_info is the Box<RawCtx> leaked in start_raw_tap and lives for the process.
    let ctx: &RawCtx = unsafe { &*(user_info as *const RawCtx) };
    // SAFETY: the tap hands us a valid event for the duration of the callback.
    let ev: &CGEvent = unsafe { cg_event.as_ref() };
    let kind = match event_type {
        CGEventType::KeyDown => "KeyDown",
        CGEventType::KeyUp => "KeyUp",
        CGEventType::FlagsChanged => "FlagsChanged",
        CGEventType::TapDisabledByTimeout => "TapDisabledByTimeout",
        CGEventType::TapDisabledByUserInput => "TapDisabledByUserInput",
        _ => "Other",
    };
    let keycode = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode) as u32;
    let flags = CGEvent::flags(Some(ev)).0;
    let hid_ticks = CGEvent::timestamp(Some(ev));
    let _ = ctx.tx.try_send(RawRec {
        kind,
        keycode,
        flags,
        hid_ticks,
        cb_ticks,
    });
    cg_event.as_ptr()
}

fn start_raw_tap() -> Result<Receiver<RawRec>, String> {
    let (tx, rx) = bounded::<RawRec>(4096);
    let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
    let ctx_ptr = Box::into_raw(Box::new(RawCtx { tx })) as usize;

    std::thread::Builder::new()
        .name("s1-raw-tap".into())
        .spawn(move || {
            let mask: u64 = (1u64 << CGEventType::KeyDown.0)
                | (1u64 << CGEventType::KeyUp.0)
                | (1u64 << CGEventType::FlagsChanged.0);
            // SAFETY: mirrors keytap's own tap creation; callback and context are valid for
            // the life of the process (this tap is never torn down; the spike just exits).
            let tap = unsafe {
                CGEvent::tap_create(
                    CGEventTapLocation::HIDEventTap,
                    CGEventTapPlacement::HeadInsertEventTap,
                    CGEventTapOptions::ListenOnly,
                    mask,
                    Some(raw_callback),
                    ctx_ptr as *mut c_void,
                )
            };
            let Some(tap) = tap else {
                let _ = ready_tx.send(Err("CGEventTapCreate returned null".into()));
                return;
            };
            let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
                let _ = ready_tx.send(Err("CFMachPortCreateRunLoopSource returned null".into()));
                return;
            };
            let Some(rl) = CFRunLoop::current() else {
                let _ = ready_tx.send(Err("CFRunLoop::current() returned None".into()));
                return;
            };
            // SAFETY: kCFRunLoopCommonModes is a valid static CFString.
            rl.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
            CGEvent::tap_enable(&tap, true);
            let _ = ready_tx.send(Ok(()));
            CFRunLoop::run();
        })
        .map_err(|e| format!("spawn: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => Ok(rx),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("reference tap handshake timed out".into()),
    }
}

// ---------------------------------------------------------------------------------------
// Pairing keytap events with reference-tap records.
// ---------------------------------------------------------------------------------------

/// Two callbacks for the same HID event run back to back on two run-loop threads. Anything
/// further apart than this is a different event.
const PAIR_WINDOW_MS: f64 = 5.0;
/// How long to wait for the reference tap's record to show up after a keytap event.
const PAIR_WAIT: Duration = Duration::from_millis(15);

struct RawBuffer {
    rx: Receiver<RawRec>,
    recent: VecDeque<RawRec>,
    seen: usize,
    disabled: usize,
}

impl RawBuffer {
    fn drain(&mut self) {
        while let Ok(r) = self.rx.try_recv() {
            self.seen += 1;
            if r.kind.starts_with("TapDisabled") {
                self.disabled += 1;
                println!("!! reference tap reported {}", r.kind);
                continue;
            }
            self.recent.push_back(r);
            while self.recent.len() > 512 {
                self.recent.pop_front();
            }
        }
    }

    /// Find and remove the record whose callback time is closest to `cb_ticks`.
    fn take_nearest(&mut self, cb_ticks: u64) -> Option<RawRec> {
        let deadline = Instant::now() + PAIR_WAIT;
        loop {
            self.drain();
            let mut best: Option<(usize, f64)> = None;
            for (i, r) in self.recent.iter().enumerate() {
                let d = ticks_to_ms(i128::from(r.cb_ticks) - i128::from(cb_ticks)).abs();
                if d <= PAIR_WINDOW_MS && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            if let Some((i, _)) = best {
                return self.recent.remove(i);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    }
}

// ---------------------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Series(Vec<f64>);

impl Series {
    fn push(&mut self, v: f64) {
        self.0.push(v);
    }
    fn line(&self, label: &str) -> String {
        if self.0.is_empty() {
            return format!("  {label:<10} n=0");
        }
        let mut v = self.0.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        format!(
            "  {label:<10} n={:<4} min {:7.3}ms  median {:7.3}ms  p95 {:7.3}ms  max {:7.3}ms",
            v.len(),
            v[0],
            q(0.5),
            q(0.95),
            v[v.len() - 1]
        )
    }
}

fn kind_label(kind: &EventKind) -> (&'static str, Key) {
    match *kind {
        EventKind::KeyDown(k) => ("DOWN", k),
        EventKind::KeyUp(k) => ("UP", k),
        EventKind::KeyRepeat(k) => ("REP", k),
    }
}

fn sh(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "?".into())
}

// ---------------------------------------------------------------------------------------

fn main() {
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let (tn, td) = timebase();
    println!("== S1 hotkey spike ==");
    println!(
        "macOS {} ({})  chip {}  keytap 0.4  profile {}",
        sh("sw_vers", &["-productVersion"]),
        sh("sw_vers", &["-buildVersion"]),
        sh("sysctl", &["-n", "machdep.cpu.brand_string"]),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!(
        "mach timebase {tn}/{td}  (1 tick = {:.4} ns)",
        tn as f64 / td as f64
    );
    println!(
        "Input Monitoring before request: {}",
        access_name(unsafe { IOHIDCheckAccess(IOHID_REQUEST_TYPE_LISTEN) })
    );

    // SAFETY: plain IOKit call; may show the system permission dialog.
    let granted = unsafe { IOHIDRequestAccess(IOHID_REQUEST_TYPE_LISTEN) };
    println!(
        "Input Monitoring after request:  {} (IOHIDRequestAccess returned {granted})",
        access_name(unsafe { IOHIDCheckAccess(IOHID_REQUEST_TYPE_LISTEN) })
    );

    let raw_rx = match start_raw_tap() {
        Ok(rx) => Some(rx),
        Err(e) => {
            println!("!! reference tap failed: {e} — latency columns will be blank");
            None
        }
    };

    let t_create = Instant::now();
    let tap = match Tap::new() {
        Ok(t) => t,
        Err(e) => {
            println!("!! keytap Tap::new() failed: {e:?}");
            println!();
            println!("If this says PermissionDenied: System Settings → Privacy & Security → Input");
            println!("Monitoring → enable the app you launched this from (Terminal, iTerm, or the");
            println!(
                "Claude desktop app). If the row is missing, click + and add it. Then re-run."
            );
            std::process::exit(1);
        }
    };
    println!(
        "keytap Tap::new() ok in {:.1}ms",
        t_create.elapsed().as_secs_f64() * 1e3
    );

    // Instant ↔ mach ticks calibration. Taken back to back; error is well under 1 µs.
    let cal_ticks = now_ticks();
    let cal_instant = Instant::now();
    let instant_to_ticks = move |t: Instant| -> u64 {
        cal_ticks + ns_to_ticks(t.saturating_duration_since(cal_instant).as_nanos())
    };

    println!();
    println!("Listening for {secs}s. Now:");
    println!(
        "  1. click into ANOTHER app (Notes, Slack, a browser) so this terminal is not focused"
    );
    println!("  2. press and release RIGHT Option a few times, then LEFT Option a few times");
    println!("  3. hold RIGHT Option for ~2s, release");
    println!(
        "  4. type a few letters, confirm they still arrive in that app (tap must not eat keys)"
    );
    println!("  5. hold RIGHT Option and press a letter, release both");
    println!();

    let mut raw = raw_rx.map(|rx| RawBuffer {
        rx,
        recent: VecDeque::new(),
        seen: 0,
        disabled: 0,
    });

    let mut hid_to_tap = Series::default();
    let mut tap_to_recv = Series::default();
    let mut hid_to_recv = Series::default();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    let mut unmatched = 0usize;
    let mut first_event_at: Option<Instant> = None;

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(secs);
    while Instant::now() < deadline {
        let ev: Event = match tap.recv_timeout(Duration::from_millis(100)) {
            Ok(e) => e,
            Err(_) => {
                if let Some(r) = raw.as_mut() {
                    r.drain();
                }
                continue;
            }
        };
        let recv_ticks = now_ticks();
        first_event_at.get_or_insert_with(Instant::now);
        total += 1;

        let (label, key) = kind_label(&ev.kind);
        *counts.entry(format!("{label} {key:?}")).or_default() += 1;

        let ev_ticks = instant_to_ticks(ev.time);
        let hop_ms = ticks_to_ms(i128::from(recv_ticks) - i128::from(ev_ticks));
        tap_to_recv.push(hop_ms);

        let rel_ms = ev.time.saturating_duration_since(t0).as_secs_f64() * 1e3;
        let mut line = format!("+{rel_ms:10.3}ms  {label:<4}  {key:<14?}");

        match raw.as_mut().and_then(|r| r.take_nearest(ev_ticks)) {
            Some(r) => {
                let os_ms = ticks_to_ms(i128::from(r.cb_ticks) - i128::from(r.hid_ticks));
                let tot_ms = ticks_to_ms(i128::from(recv_ticks) - i128::from(r.hid_ticks));
                hid_to_tap.push(os_ms);
                hid_to_recv.push(tot_ms);
                line.push_str(&format!(
                    "  kc={:<3} flags=0x{:08x} {:<12}  hid→tap {os_ms:6.3}ms  tap→recv {hop_ms:6.3}ms  hid→recv {tot_ms:6.3}ms",
                    r.keycode, r.flags, r.kind
                ));
            }
            None => {
                unmatched += 1;
                line.push_str(&format!(
                    "  (no reference record)                      tap→recv {hop_ms:6.3}ms"
                ));
            }
        }
        println!("{line}");
    }

    println!();
    println!("== summary ==");
    println!("events from keytap: {total}   unmatched to reference tap: {unmatched}");
    if let Some(r) = raw.as_ref() {
        println!(
            "reference tap saw {} raw events, {} TapDisabled notices",
            r.seen, r.disabled
        );
    }
    println!("latency (ms):");
    println!("{}", hid_to_tap.line("hid→tap"));
    println!("{}", tap_to_recv.line("tap→recv"));
    println!("{}", hid_to_recv.line("hid→recv"));
    println!("counts by event:");
    for (k, n) in &counts {
        println!("  {n:>4}  {k}");
    }
    let n = |s: &str| counts.get(s).copied().unwrap_or(0);
    println!("checks:");
    println!(
        "  right Option down/up seen:        {} / {}",
        n("DOWN AltRight"),
        n("UP AltRight")
    );
    println!(
        "  left Option down/up seen:         {} / {}",
        n("DOWN AltLeft"),
        n("UP AltLeft")
    );
    println!(
        "  right ≠ left distinguishable:     {}",
        if n("DOWN AltRight") > 0 && n("DOWN AltLeft") > 0 {
            "yes (both appeared under different names)"
        } else {
            "cannot say — press both keys"
        }
    );
    println!(
        "  repeat events for Option hold:    {}",
        counts
            .iter()
            .filter(|(k, _)| k.starts_with("REP Alt"))
            .map(|(_, n)| *n)
            .sum::<usize>()
    );

    let t_drop = Instant::now();
    drop(tap);
    println!(
        "tap dropped cleanly in {:.1}ms",
        t_drop.elapsed().as_secs_f64() * 1e3
    );
}
