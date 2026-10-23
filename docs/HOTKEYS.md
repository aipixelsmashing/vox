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

Confirmed on macOS 26.5 ([S1](spikes/s1-hotkey.md)): right and left Option arrive as
distinct keys (keycodes 61 and 58) on every press, a 2.6 s hold produces one down and one up
with no auto-repeat, and delivery is ~2 ms median. `keytap` does not prompt for Input
Monitoring — the app calls `IOHIDRequestAccess` itself ([PERMISSIONS.md](PERMISSIONS.md)).

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

## The lock: hands-free without a second key

Long-form sessions ([LONG-FORM.md](LONG-FORM.md)) need the key let go of. The first design
was a lock key pressed while the hotkey was held (`L`); it is a gesture on the same key now,
so there is nothing to remember and nothing that collides with what the foreground app does
with Option+L:

| Gesture, in `hold` mode | Result |
| --- | --- |
| Tap, tap (the second down within 400 ms of the first up, each press under 300 ms) | Recording starts on the second tap's down and carries on after its release. |
| One tap, while locked | Recording ends on the tap's down; its release does nothing. |
| Escape | Cancels, as always. |

The first tap is an ordinary short press: recording opens and is discarded as under
`minHoldMs`, silently, because the start cue plays only once a press has lasted that long.
The lock belongs to `hold` mode only: `toggle` is already hands-free, and `double-tap-hold` uses the
double tap to start. The recording cap (`audio.maxRecordingSec`) still applies to a locked
session until M6 brings the long-form pipeline; the lock arrived with the hotkey design
because it is a hotkey question.

## Problem 4: cancelling

While recording, **Escape cancels**: capture stops, audio is dropped, nothing is transcribed,
nothing is inserted, no history entry is written. This is a hard requirement — the moment you
realise you said the wrong thing must not cost you a transcription and a paste into your
colleague's chat window.

Escape is observed on the same tap, but it cannot be left for the foreground app to see.
The hotkey modifier is still held when Escape is pressed, so the app does not receive a bare
Escape: it receives **Option+Escape**, which in every Cocoa text view is the "show
completions" shortcut. The M1 gate found exactly that: cancelling in Notes opened the
completion menu and left an "I" behind when it closed.

So on macOS the cancel key is swallowed while a dictation is in progress, by a second, active
`CGEventTap` inserted ahead of keytap's listen-only one. It drops the cancel key's down, its
auto-repeats and its up, and sends the cancel to the pipeline itself; outside a dictation it
passes everything through untouched. It swallows only the cancel key, never the modifier —
consuming the modifier is `hotkey.consume`, a separate and off-by-default choice (problem 2).
If the active tap cannot be created (permission missing), the cancel still works through
keytap's tap and the app sees the chord; the log says so.

Windows will need the same via the `WH_KEYBOARD_LL` hook; Linux cannot swallow without root
and will need a cancel key that is harmless in combination with the modifier.

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

### Fn as the key

Fn (the Globe key on Apple keyboards) is a first-class choice, not the default. macOS delivers
it to the tap as a flag change like the other modifiers (keycode 63, flag bit `0x00800000`,
`kCGEventFlagMaskSecondaryFn`), `keytap` names it `Function`, and `"keys": ["Function"]`
matches like any modifier. `keytap` 0.4.0 dropped that flag change: its macOS FlagsChanged
branch had no modifier bit for `Function`, so it emitted nothing and the binding was silently
dead ([S1](spikes/s1-hotkey.md), 2026-10-21). Vox builds against a patched `keytap`
(`[patch.crates-io]` in `src-tauri/Cargo.toml`: a one-line fix on a fork, the same change as
[jamiepine/keytap#3](https://github.com/jamiepine/keytap/pull/3)) until upstream ships it.
Two things are true of Fn that are not true of right Option, and the pane says both when Fn
is chosen:

- **macOS uses the key itself.** System Settings → Keyboard, "Press 🌐 key to", fires on a
  bare press: Change Input Source, Show Emoji & Symbols, or Start Dictation. Vox reads
  `AppleFnUsageType` from the HIToolbox defaults when Fn is captured and while it is the
  binding, and tells the user to set it to Do Nothing unless it already is. Vox does not
  change it: that is the user's setting.
- **Some external keyboards never send Fn.** On many third-party boards Fn is handled in the
  keyboard's own firmware and the Mac never sees a key. The pane says so; if holding it does
  nothing, choose another key.

[S1](spikes/s1-hotkey.md) measured right Option on 2026-09-15 and Fn on 2026-10-21. The OS
delivers Fn from the built-in keyboard; a dictation completing with the patched build
([TESTING.md](TESTING.md), hotkey gate item 10) is still owed.

## Undo the last dictation

A chord, default **⌃⌘Z** (`hotkey.undo`, configurable in Settings → Dictation, null to turn
it off), takes the text Vox just placed back out of the field it placed it in. It is not
⌘Z: the app's own undo stack is untouched, so it cannot take something else with it, and
it works in apps whose undo does not know about an accessibility insertion.

It lives by the rule insertion lives by ([adr/0005](adr/0005-no-unverified-injection-success.md)):
read the field, act only on what can be seen, verify. After every verified insertion Vox
reads the field once and notes the text before and after the insertion, as the correction
watch does ([LEARNING.md](LEARNING.md#how-capture-works)). On the chord:

1. The app the text went into must be frontmost. Otherwise: *Couldn't undo — Notes isn't
   in front.*
2. The field must read back, now and right after the insertion. Otherwise: *Couldn't undo
   — the field can't be read.*
3. The field must still start with the text that was before the insertion, followed by the
   insertion itself. Text typed after it is fine; an edit before it or to it is not:
   *Couldn't undo — the text has changed since it was placed.*
4. The span is selected through `kAXSelectedTextRange` and replaced with nothing through
   `kAXSelectedText`, the same two attributes insertion uses. A field that refuses the
   write: *Couldn't undo — Terminal didn't accept the change.* A field that accepts it
   but does not read back as expected within 300 ms: *Vox couldn't confirm the text was
   removed.*

There is one record, replaced by every dictation. A dictation that ended on the clipboard
leaves a record that says so (*Nothing to undo — the last dictation was copied, not placed
in a field.*) rather than letting the chord reach back to an older one. Once the field has
been written to, successfully or not, the record is spent: a second press says *Nothing to
undo.* The transcript stays in history, so an undo you did not mean is one Insert away.

Like the panel chord, the undo chord is observed, not swallowed, so the app in front sees
it too; the default is one no text app binds. Settings captures it the way it captures the
hold key, by pressing it, and keeps it only if the keys make a chord: one or more modifiers
and one other key.

## Recording feedback

The user must never be uncertain whether Vox is listening.

- Tray icon changes to a distinct recording state, plus a subtle animation of input level.
- Optional short click on start and stop (default on; sound cues matter for a device where the
  visible indicator may be on another monitor). The start cue plays once the press has lasted
  `minHoldMs`, after the microphone is open and never ahead of it, so a tap that is discarded
  makes no sound and the cue never delays the first syllable.
- Optional small overlay near the caret showing a live level meter and elapsed time.
- If recognition is still running when a subsequent key-down arrives, that key-down is ignored
  and the tray icon flashes a "busy" state rather than queuing a second dictation.

## Per-platform testing gate

Because `keytap`'s Linux and Windows backends are newer than its macOS one, every release
candidate must pass a manual hotkey checklist on each platform — see
[TESTING.md](TESTING.md#manual-gates). Automated tests cannot cover a system-level key tap.
