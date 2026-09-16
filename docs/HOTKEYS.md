# Hotkeys and push-to-talk

## The requirement

Hold a key, speak, release. The key defaults to right Option (macOS) / right Alt (Windows,
Linux) and is user-configurable. This is harder than it looks, for four reasons.

## Problem 1: modifier-only bindings

Standard hotkey registration — `RegisterHotKey` on Windows, Carbon hotkeys on macOS, and
therefore the `global-hotkey` crate — registers *chords*: zero or more modifiers plus one
ordinary key. A modifier on its own is not a registrable shortcut. It also only delivers a
"pressed" event, never a release.

Push-to-talk needs the raw stream. That means a system-level tap:

| Platform | Mechanism | Permission needed |
| --- | --- | --- |
| macOS | `CGEventTap` on a dedicated `CFRunLoop` thread, listening for `flagsChanged` and key events | Accessibility, and Input Monitoring |
| Windows | `WH_KEYBOARD_LL` low-level hook with a message pump | None, but antivirus heuristics may flag it |
| Linux | `evdev` on `/dev/input/event*` (works under both X11 and Wayland) | Membership of the `input` group, or a udev rule |

`keytap` implements all three behind one API and — critically — preserves left/right modifier
identity, so `Key::AltRight` is distinguishable from `Key::AltLeft`. See
[TECH-STACK.md](TECH-STACK.md) for why the alternatives don't work.

## Problem 2: right Alt is AltGr

This is the one that will generate support tickets, so it is worth being precise.

`keytap` **observes** events, it does not consume them. When the user holds right Alt to
dictate, the foreground application still sees right Alt held. On most US layouts, right Alt
alone does nothing, so this is invisible. On German, French, Polish, Spanish, Brazilian and
many other layouts, right Alt *is* AltGr — the third-level shift used for `@`, `€`, `{`, `}`,
`\`, `~`. Holding it alone still produces no character, but:

- combined with any key the user presses during dictation, it produces an AltGr character;
- some IMEs and terminal emulators react to the modifier state itself;
- on Windows, `Ctrl+Alt` is synthesised by AltGr in some layouts, so a held right Alt can
  register as a Ctrl+Alt chord.

Decisions:

1. **Detect the active keyboard layout at startup and after layout changes.** If the layout
   maps right Alt to AltGr, onboarding proposes right Ctrl instead and explains why in one
   sentence. The user can override.
2. **Offer an optional consume mode on macOS and Windows.** Both platforms can swallow the
   event: an active `CGEventTap` inserted at `kCGHeadInsertEventTap` can return `NULL`, and a
   `WH_KEYBOARD_LL` hook can return non-zero. This removes the AltGr side effects entirely but
   means the key no longer works as a modifier while Vox is running, which will surprise
   people. Off by default, clearly labelled, with a "hold Escape at launch to disable" recovery
   path. Not available on Linux, where grabbing input requires root.
3. **Do not consume by default.** A tray app that silently eats a modifier system-wide is
   worse behaviour than the AltGr overlap.

## Problem 3: holding a key is an accessibility barrier

Push-to-talk assumes sustained key pressure, which is exactly what some users with motor
impairments cannot do. Three modes ship in v1:

| Mode | Behaviour |
| --- | --- |
| `hold` (default) | Record from key-down to key-up |
| `toggle` | First complete press starts, next press stops |
| `double-tap-hold` | Two quick taps then hold — reduces accidental triggers for users who rest fingers on modifiers |

A hold shorter than `minHoldMs` (default 120 ms) is discarded rather than transcribed, so a
stray brush of the key does nothing. In `toggle` mode a `maxRecordingSec` cap prevents an
unnoticed session recording for an hour.

## Problem 4: cancelling

While recording, **Escape cancels**: capture stops, audio is dropped, nothing is transcribed,
nothing is inserted, no history entry is written. This is a hard requirement — the moment you
realise you said the wrong thing must not cost you a transcription and a paste into your
colleague's chat window.

Escape is observed on the same tap. It is not consumed, so the foreground app also sees it;
this is acceptable because Escape in most contexts is harmless, and consuming it would be
worse.

## Binding configuration

Stored as a small structured value, not a string, so serialisation is unambiguous:

```json
{
  "hotkey": {
    "keys": ["AltRight"],
    "mode": "hold",
    "minHoldMs": 120,
    "consume": false
  }
}
```

Multiple keys in `keys` form a chord — for example `["ControlLeft", "AltLeft"]` — matched by
`keytap`'s `ChordMatcher` with momentary semantics: the chord starts when all keys are down and
ends when any of them is released.

The settings UI captures a binding by listening for the next key-down and showing what it saw,
rather than asking the user to type a shortcut string. It rejects bindings that are likely to
be destructive (a lone letter key, a lone Enter, anything with no modifier and no function key)
with an explanation rather than a silent refusal.

## Recording feedback

The user must never be uncertain whether Vox is listening.

- Tray icon changes to a distinct recording state, plus a subtle animation of input level.
- Optional short click on start and stop (default on; sound cues matter for a device where the
  visible indicator may be on another monitor).
- Optional small overlay near the caret showing a live level meter and elapsed time.
- If recognition is still running when a subsequent key-down arrives, that key-down is ignored
  and the tray icon flashes a "busy" state rather than queuing a second dictation.

## Per-platform testing gate

Because `keytap`'s Linux and Windows backends are newer than its macOS one, every release
candidate must pass a manual hotkey checklist on each platform — see
[TESTING.md](TESTING.md#manual-gates). Automated tests cannot cover a system-level key tap.
