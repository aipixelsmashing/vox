# 0003 — `keytap` for global key capture

**Status:** accepted

## Context

The default binding is "hold the right Option key". That requires raw key-down and key-up
events, left/right modifier distinction, and modifier-only bindings, while the app is
unfocused, on three platforms.

`global-hotkey` registers OS shortcuts: no raw stream, no release events, no left/right
distinction — "hold right Alt" is not expressible. `rdev` provides raw events but collapses
modifiers on some paths, has a known crash on macOS 14+ when called from background threads,
and offers no clean shutdown. `hotkey-listener` has no Windows support.

## Decision

Use `keytap`: CGEventTap on macOS, `WH_KEYBOARD_LL` on Windows, evdev on Linux (works under
Wayland), with left/right modifier fidelity, a chord matcher with momentary and toggle modes,
clean shutdown on drop, and a typed error when the OS denies permission.

## Consequences

- The stated default hotkey becomes implementable, and the same crate covers all three
  platforms and both Linux display servers.
- `keytap` is observe-only: it does not consume events, so right Alt still behaves as AltGr in
  the foreground app. Handled in [HOTKEYS.md](../HOTKEYS.md) with layout detection and an
  optional consume mode implemented directly against the platform APIs on macOS and Windows.
- Its Linux and Windows backends are newer than its macOS backend, so hotkey capture is a
  per-platform manual release gate and validating it is the first milestone in the roadmap.
- If the crate becomes unmaintained, the fallback is to implement the three backends directly;
  the abstraction in `hotkey.rs` exists to make that a contained change.
