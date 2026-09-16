# S1 — Hotkey: does `keytap` deliver right Option, and how fast?

**Answer: yes.** Right Option arrives as a distinct key with its own keycode and flag bit, on
every press, with end-to-end latency of about 2 ms median and 6 ms p95.

Run 2026-09-15 on macOS 26.5.2 (25F84), Apple M1 Pro, built-in keyboard, `keytap` 0.4.0,
release build. Harness: [`spikes/s1-hotkey`](../../spikes/s1-hotkey/src/main.rs). Keys were
pressed with another application focused, not the terminal running the spike
(unconfirmed which app; see open items).

## What was tried

The harness installs `keytap`'s tap and, beside it, a second listen-only `CGEventTap` that
reads each event's kernel HID timestamp (`CGEventGetTimestamp`). The two streams are paired
by callback time. `keytap` stamps `Event::time` inside its own callback, so on its own it can
only measure the channel hop, not OS delivery.

Sequence: tap right Option ×7, tap left Option ×9, hold right Option and press letters, hold
left Option and press letters, hold right Option for 2.6 s.

## What happened

| Check | Result |
| --- | --- |
| Right Option down / up | 17 / 17, `Key::AltRight`, keycode 61, flag `0x00080140` |
| Left Option down / up | 9 / 9, `Key::AltLeft`, keycode 58, flag `0x00080120` |
| Right ≠ left | Always. Confirmed independently by keytap's key name and the raw keycode |
| Events dropped | 0 of 70. Reference tap saw the same 70 |
| Auto-repeat during a 2.6 s hold | None. One down, one up |
| Letters under a held Option | Captured as plain keycodes (K, L, O, S); layout-independent |
| Command keys (earlier run) | `MetaRight` keycode 54 / `MetaLeft` keycode 55, also distinct |
| `Tap::new()` | 12–19 ms |
| `drop(tap)` | 0.2–1.1 ms, clean |

Latency, 70 events:

| Stage | min | median | p95 | max |
| --- | --- | --- | --- | --- |
| HID timestamp → keytap callback | 0.58 ms | 1.91 ms | 5.31 ms | 11.26 ms |
| keytap callback → consumer thread | 0.01 ms | 0.03 ms | 0.48 ms | 3.96 ms |
| HID timestamp → consumer thread | 0.59 ms | 2.05 ms | 6.06 ms | 11.28 ms |

The maximum was the first event after the process had been idle, consistent with a cold
thread wake. Steady-state events sat between 0.6 and 5 ms.

## What it means for the design

- [ADR 0003](../adr/0003-keytap-for-push-to-talk.md) holds. No change.
- The hotkey contributes ~2 ms to release-to-text; it is not on the critical path of the
  [latency budget](../LATENCY.md).
- Modifier keys do not auto-repeat through `FlagsChanged`, so the pipeline's hold detection
  needs no repeat suppression for Option. Ordinary keys do repeat and must be ignored if a
  non-modifier chord is ever offered.
- **`keytap` never prompts for Input Monitoring.** It calls `IOHIDCheckAccess` and returns
  `PermissionDenied`. The app must call `IOHIDRequestAccess(kIOHIDRequestTypeListen)` itself
  during onboarding so the system dialog appears; the spike does this. Add to
  `permissions.rs` and [PERMISSIONS.md](../PERMISSIONS.md) in M1.
- The grant attaches to the responsible process. For a binary launched from a terminal that
  is the terminal app, which is why a `cargo run` build works once Terminal is granted.

## Open items

- Confirm which application was focused during the run and that the typed letters arrived
  there (i.e. the listen-only tap did not consume them). Expected yes; not yet stated.
- Not measured: behaviour when the tap thread is starved under heavy load, and whether
  `TapDisabledByTimeout` ever fires. Neither appeared in ~3 minutes of running.
