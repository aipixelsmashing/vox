# Text injection

Getting a string into someone else's text field is the least glamorous and most failure-prone
part of this product. It is where competitors lose text, leak transcripts, and report success
when nothing happened. This document is the spec for not doing that.

## The naive approach and why it fails

Nearly every open-source dictation tool does this:

```
save clipboard → write transcript to clipboard → synthesise Ctrl+V/Cmd+V → sleep(N ms) → restore clipboard
```

Four defects, all of which we design around:

1. **Transcript leakage.** On Windows, writing `CF_UNICODETEXT` alone opts the content into
   clipboard history and the Microsoft cloud clipboard. Everything you dictate ends up in a
   synced history you did not think about. macOS clipboard managers behave similarly unless
   the content is marked concealed.
2. **Modifier corruption.** In push-to-talk a modifier key is held *by construction* at the
   moment injection fires. A synthesised Ctrl+V that arrives while right Alt is still down is
   a different chord, and produces nothing or something wrong.
3. **Silent refusal.** On Windows, UIPI blocks input into a process at a higher integrity
   level and reports nothing at all through the return value or `GetLastError`. The text
   simply vanishes. On Wayland the compositor may accept your synthetic input and route it
   nowhere.
4. **The restore race.** The target reads the clipboard asynchronously, whenever its message
   pump gets to the paste. Restoring the old clipboard on a fixed timer restores it before a
   busy target has read — and the target then pastes the *old* content. Every fixed delay is a
   guess, and tuning it upward only moves the threshold. This is a real, long-lived, unfixed
   bug in a widely used dictation app, whose shipped mitigation is a user-facing delay slider
   that still fails for some users at 400 ms.

## Principles

1. **Prefer direct insertion over synthesised keystrokes.** Setting the value of a focused
   accessibility element is atomic, instant, and immune to modifier state.
2. **Never claim success you cannot verify.** `InjectionOutcome` has exactly two variants:
   `Inserted` or `ClipboardOnly { reason }`. There is no "probably".
3. **Restore the clipboard on evidence, not on a timer.** Watch for the change that proves
   the target read it; fall back to a bounded timeout with the user informed.
4. **Mark clipboard writes private** so transcripts stay out of clipboard history and cloud
   sync.
5. **Sanitise held modifiers** before synthesising any chord.
6. **Failure is a UI event, not a log line.**
7. **Reliability outranks everything else in this project.** A silent failure here costs more
   trust than any feature earns, which is why it has its own roadmap milestone with a
   zero-tolerance gate rather than being folded into feature work.

## Common flow

```
capture_target()  ── at key-down ──►  InjectionTarget { pid, window, element?, app_name }
                                              │
        transcript ready ─────────────────────┤
                                              ▼
                                    revalidate_target()
                                     │                 │
                            same target          focus moved
                                     │                 │
                                     ▼                 ▼
                            platform chain     ClipboardOnly{FocusChanged}
                                     │
                     ┌───────────────┼───────────────┐
                     ▼               ▼               ▼
                 Inserted     next method      ClipboardOnly{reason}
```

Chunking: transcripts over `MAX_ATOMIC_CHARS` (2000) are split on sentence boundaries and
inserted sequentially, because some targets truncate very large single insertions.

Post-processing applied before injection: user dictionary replacements, optional trailing
space, optional leading-capital, and normalisation of the model's smart quotes to whatever the
setting says. Never any content rewriting.

---

## macOS

### Chain

1. **Secure input check.** If `IsSecureEventInputEnabled()` returns true, some process has
   locked keyboard input — a password field, a lock screen, sometimes a terminal doing a
   password prompt, and (notoriously) sometimes an app that leaked the state and never
   released it. Abort immediately: no injection, no clipboard write of the transcript into a
   password context. Return `ClipboardOnly { SecureInput }` and tell the user what is
   happening, because a stuck secure-input state is otherwise baffling.

2. **Accessibility insertion.** Via `axuielement`:
   - `AXUIElementCreateSystemWide()`
   - read `kAXFocusedUIElementAttribute`
   - check `kAXRole` is one of `AXTextField`, `AXTextArea`, or a `AXComboBox` that reports a
     settable `kAXSelectedTextAttribute`
   - set `kAXSelectedTextAttribute` to the transcript — this replaces the selection, or
     inserts at the caret when the selection is empty
   - verify by re-reading `kAXSelectedTextRange` and confirming the caret advanced by the
     inserted length

   Requires the Accessibility permission. Chromium- and Electron-based apps often expose a
   web area whose selected-text attribute is not settable; the failure is detected by the
   verification step, not assumed.

3. **Clipboard paste.** Write the transcript to `NSPasteboard` with `org.nspasteboard.ConcealedType`
   set so well-behaved clipboard managers skip it. Record `changeCount`. Synthesise Cmd+V with
   `CGEventPost`, having first cleared any residual modifier flags on the synthetic events.
   Restore the previous pasteboard contents once `changeCount` has moved again *or* after a
   bounded 1500 ms, whichever comes first. If the timeout wins, leave the transcript on the
   clipboard rather than restoring over it — losing the user's old clipboard is bad, losing
   the transcript is worse.

4. **Unicode key events.** `CGEventKeyboardSetUnicodeString` in chunks of ≤ 20 UTF-16 units,
   posted with zero inter-event delay. Layout-independent, handles emoji and non-Latin text,
   but slower for long strings and disruptive in apps with aggressive autocomplete. Last
   resort, or first choice for terminals when the user has selected `type` explicitly.

### Permissions

Accessibility *and* Input Monitoring, both requested during onboarding with a direct link to
the exact System Settings pane. Neither can be granted programmatically. See
[PERMISSIONS.md](PERMISSIONS.md).

---

## Windows

### Chain

Implemented on top of `win-text-inject`, which encapsulates most of the following.

1. **Target check.** `Target::accepts_injection()` — determine whether the foreground window
   belongs to a process at a higher integrity level. If UIPI will block us, do not attempt;
   return `ClipboardOnly { ElevatedTarget }` and tell the user that this window is running as
   administrator.

2. **Modifier sanitisation.** Read the current keyboard state and synthesise key-up events for
   any modifier still held, before sending the paste chord. Restore afterwards if the physical
   key is still down.

3. **Private clipboard write.** Attach all four opt-out formats alongside `CF_UNICODETEXT`:
   `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` = 0,
   `CanUploadToCloudClipboard` = 0, `Clipboard Viewer Ignore`. These are honoured by Windows
   clipboard history, the cloud clipboard, and cooperating third-party managers.

4. **Delayed rendering instead of a sleep.** Publish a promise — `SetClipboardData(CF_UNICODETEXT, NULL)`
   with a hidden owner window — so the data is produced when the target actually asks for it
   via `WM_RENDERFORMAT`. That event is the proof the target read the clipboard, and it is
   what triggers the restore. No guessed delay anywhere in the path.

5. **Paste chord.** `SendInput` with Ctrl+V, or **Ctrl+Shift+V** when the foreground window
   class matches a known terminal (Windows Terminal, ConEmu, mintty, Alacritty, WezTerm),
   where plain Ctrl+V is a no-op or means something else.

6. **Unicode fallback.** `SendInput` with `KEYEVENTF_UNICODE`, zero delay, for strings under
   200 characters when the clipboard path fails.

UI Automation is deliberately not used for insertion: `ValuePattern.SetValue` replaces the
entire field contents rather than inserting at the caret, which is destructive, and
`TextPattern` is read-only. Text Services Framework insertion — the technically correct answer
— requires shipping a registered, per-architecture, signed in-process COM DLL loaded into
every target application. That is a separate project, and a much larger security surface.

---

## Linux

Two different worlds, and honesty about the second is a design requirement.

### X11

`enigo`'s XTEST backend, or `xdotool type` if present. **Pass a zero key delay explicitly** —
both `xdotool type` and `ydotool type` default to a 12 ms inter-key delay, which makes a
100-character transcript visibly type itself out over more than a second. This is a real
reported bug in another dictation app; the fix is `--delay 0` / `--key-delay 0`.

Clipboard fallback uses `xclip`/`xsel`, with Ctrl+Shift+V for terminals.

### Wayland

Wayland's security model deliberately has no equivalent of XTEST. In order of preference:

1. **libei via the RemoteDesktop portal** — the correct modern answer. Works on GNOME ≥ 46 and
   KDE Plasma ≥ 6.1, requires no group membership, no udev rules, and is Flatpak-compatible.
   It is the same portal family as the global-shortcuts portal, which keeps the Linux code
   coherent.
2. **`wtype`** (`zwp_virtual_keyboard_v1`) — wlroots compositors only; refused by KWin and
   Mutter.
3. **`ydotool`** — works anywhere via `uinput`, but needs a daemon and `input` group
   membership, and is not Flatpak-friendly.
4. **Clipboard-only** — write to the clipboard via `wl-copy`, tell the user to press Ctrl+V.

**We never report `Inserted` on a path we cannot confirm.** A compositor can accept synthetic
input and deliver it nowhere; treating the call's return value as proof produces the worst
failure mode there is — the user believes the text was inserted, moves on, and loses it. On
unverifiable paths the outcome is `ClipboardOnly { WaylandUnverifiable }` with the transcript
safely on the clipboard.

---

## Special targets

| Target | Handling |
| --- | --- |
| Password fields | Detected via AX role (macOS `AXSecureTextField`) or secure-input state; refuse to insert and refuse to write to the clipboard — the transcript is dropped and the user is told. Dictating into a password field is almost always a mistake |
| Terminals | Ctrl+Shift+V rather than Ctrl+V; bracketed paste mode means multi-line text may execute, so newlines are collapsed to spaces unless the user opts out |
| Code editors | Auto-indent and autocomplete mangle synthesised typing; prefer clipboard paste over unicode events here |
| Browsers / Electron | AX insertion frequently unavailable; the verification step catches it and falls through to paste |
| Remote desktop / VM windows | Nothing works reliably; detect known window classes and go straight to clipboard-only with a clear message |
| No editable field focused | `ClipboardOnly { NoTextTarget }` — "Copied. No text field was focused." |

## Compatibility matrix

Maintained as a living test artefact. Each release candidate is checked against this list on
each platform, results recorded in `docs/compat/<version>.md`.

Baseline set: native notes/editor app, Chrome address bar, Chrome web textarea, Firefox,
Safari/Edge, VS Code, JetBrains IDE, Slack, Discord, Terminal/Windows Terminal, Word, Excel
formula bar, Figma, Notion, Obsidian, system search (Spotlight / Start menu), and one
elevated/admin window.

## Reading back what we wrote

Two things depend on being able to re-read the target field after insertion: verification (did
the text actually arrive?) and correction capture ([LEARNING.md](LEARNING.md)). They use the
same mechanism, so spike S2 validates both at once.

The read is best-effort. Where it is unavailable, verification falls back to the platform's own
evidence — caret advance on macOS, `WM_RENDERFORMAT` on Windows — and correction capture simply
does not run for that application. It never degrades the dictation path, and it never runs on a
field we refused to insert into.

## Instrumentation

Every injection records: method attempted, method that succeeded, outcome, elapsed time, and
the target application's bundle id or executable name. Stored **locally** in the history row,
never transmitted. This is what makes "it doesn't work in app X" a debuggable report: the user
can open the history entry and see exactly which method was tried and what it returned.
