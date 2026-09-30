# Interface specification

> **Implementing this?** [UI-KIT.md](UI-KIT.md) has the tokens,
> [UI-CONTRACT.md](UI-CONTRACT.md) the data, [UI-STATES.md](UI-STATES.md) every state and every
> string, and [UI-DEVELOPMENT.md](UI-DEVELOPMENT.md) how to build all of it in a browser with
> no Rust toolchain. This document is the intent behind them.

## Design intent

This is a tool that should be invisible in use. The interface exists for four moments:
setting up, checking that it's listening, retrieving something, and fixing something that
broke. Everything else is the hotkey.

Visual direction is restrained and system-native in feel, with one deliberate exception: the
**recording indicator**, which is the only place the app spends any visual energy. It needs to
be legible at a glance from across a desk, distinct from every other tray icon, and honest
about state. That is where the boldness goes; everything else stays quiet.

Copy follows one rule: say what happened and what to do about it. Errors don't apologise and
are never vague. Empty states are invitations, not decoration.

## Tray icon

Four states, distinguishable by silhouette and not by colour alone:

| State | Icon | Meaning |
| --- | --- | --- |
| Idle | Outline mark | Ready |
| Recording | Filled mark with a live level ring | Listening now |
| Transcribing | Filled mark, indeterminate progress | Working |
| Attention | Outline mark with a badge | Missing permission, model, or a failed insert |

Plus a distinct **always-listening** variant when `audio.preroll` is enabled, because an open
microphone stream must never be invisible.

Tray menu, native and short:

```
Show history          ⌘⇧Space
Settings…
─────────────────────────
Pause dictation
Check for updates…
─────────────────────────
Quit Vox
```

`Pause dictation` is one click and unmistakable — for meetings, screen shares, and the moment
you realise you don't want a key you're about to press to start recording.

## History panel

Specified in [HISTORY.md](HISTORY.md).

## Settings

Specified in [SETTINGS.md](SETTINGS.md). Six panes: Dictation, Model, Text, Vocabulary, Privacy, Diagnostics.

## Onboarding

Three steps, each revisitable later from Settings. Progress is saved, so a user who quits
part-way doesn't start over. (It began as six; most of them only told the user something,
and telling is not a step.)

1. **What this does — and whether you need it.** Three sentences, the statement that audio
   never leaves the machine, and on macOS the honest paragraph: *your Mac already has
   dictation built in, and for short messages it's probably enough — here's how to turn it
   on.* Then what Vox adds that it doesn't: minutes-long sessions, your own vocabulary, a
   history, and the same key on your other machines. No marketing. See
   [VALUES.md](VALUES.md#we-tell-people-when-they-dont-need-this).
2. **Set up.** One page, four rows: microphone (request, then a live level meter — a meter
   that visibly moves is the fastest proof the right device is selected), Input Monitoring
   and Accessibility (a direct link to each pane, and "Quit and reopen" where a restart is
   required), and the speech engine's status. On macOS 26+ there is nothing to download;
   the M8 engines add a download with progress and "import from folder" here.
3. **Two choices, then try it.** The learning card and the read-the-field card, off by
   default — this is where someone turns them on knowingly — and a text box in the app
   itself: "Hold right Option and say something." On success the transcript appears in the
   box and the step completes itself. If the layout maps right Alt to AltGr, the step opens
   with the alternative binding suggested and one sentence explaining why.

## Vocabulary pane

The trust surface for [LEARNING.md](LEARNING.md). Without it, a system that quietly changes
your words is indistinguishable from a system that is broken.

```
┌──────────────────────────────────────────────────────┐
│  Words Vox has learned (14)   [Filter words]  Export │
├──────────────────────────────────────────────────────┤
│  Kubernetes          was "cuber netties"             │
│  3 corrections in Slack, VS Code · since 12 Mar   ×  │
├──────────────────────────────────────────────────────┤
│  Priya               was "prea", "pre a"             │
│  6 corrections in Slack, Mail · since 2 Mar       ×  │
├──────────────────────────────────────────────────────┤
│  ⚠ Tailwind          suspended                       │
│  You changed this back twice — not applying it    ×  │
├──────────────────────────────────────────────────────┤
│  Waiting: 6 words seen once or twice     Show    ⌫   │
└──────────────────────────────────────────────────────┘
```

One row per learned word, not per pair: a name the recogniser mishears three different ways
is one word to the user. Beneath it, every wrong form it replaced; its corrections summed and
its apps merged. The header carries the count. Every row carries provenance, because "where
did this come from?" is the first question anyone asks. Deleting a word deletes it and every
pair behind it, so it cannot be re-learned from the same corrections. Suspended words explain
themselves rather than silently stopping. The header row holds the count, the export and,
past twelve words, a filter field that matches the word or any of its wrong forms. Nothing
actionable sits below the list; the list scrolls with the pane, with no inner scroll region.

## Long-form session panel

Specified in [LONG-FORM.md](LONG-FORM.md). Unlike the recording overlay, this one is meant to
be looked at: it shows the text forming as you speak.

## Diagnostics

Settings → Diagnostics shows the user their own numbers: corrections per 100 words over time,
insertion outcomes by application, per-stage timings, and memory. Computed locally, never
transmitted.

It exists so someone can decide for themselves whether this tool is earning its place — and so
that "it's got worse since the update" becomes a checkable claim rather than a feeling. There
are no achievements here and no framing of usage as a score.

## Recording overlay

Optional, on by default (`ui.levelOverlay`): a small pill near the caret (or bottom-centre
if the caret can't be located) showing a level meter and elapsed seconds. It disappears the
moment insertion completes. Never steals focus, never accepts clicks, respects reduced-motion.

What it shows, and when:

- **Recording.** The amber level ring from [UI-KIT.md](UI-KIT.md#motion) and the elapsed
  time as `0:04`, so the recording cap is never a surprise. The ring is the one animated
  thing in the product; under reduced motion it is a static three-step meter.
- **Working.** The ring fills solid and the text reads "Transcribing…" then "Placing the
  text…", the same strings as the history panel's live row.
- **Gone** on the transition back to idle, whichever way it ends: text placed, clipboard
  fallback, cancel, or a brush of the key. The toast, not the overlay, carries a reason.

Placement: just below the caret with the pill's left edge on it, from the focused element's
`AXBoundsForRange` at the selected range; above the caret when there is no room below;
clamped to the monitor the caret is on. Bottom centre, above the Dock, when the app cannot
say where its caret is (Chromium usually can; some Electron apps cannot). The window is
created on the first dictation and hidden afterwards, joins every Space including a
full-screen app's, and ignores the mouse. It is shown once the target element is known,
which is after the tray icon and the start cue: those answer "is it on?"; the ring answers
"is it hearing me?".

## Notifications

Used sparingly and only for things the user must know:

- Insertion fell back to the clipboard, with the reason and what to do:
  "Copied instead — Terminal is running as administrator. Press Ctrl+Shift+V to paste."
- Recording hit the length cap.
- Model download finished or failed verification.
- An update is ready to install.

Two channels carry the same words. Vox's own **toast** — a small pill at the bottom centre
of the screen, never focused, gone after 4.5 seconds or on click — shows every one of these
on every build, and gives Vox control of placement and timing. The system notification is
the second copy, and only on Apple-signed builds, because Notification Center refuses
self-signed ones ([PERMISSIONS.md](PERMISSIONS.md)).

Never used for successful dictations. The text appearing in the field is the notification.

Never used for engagement: no usage summaries, no streaks, no "you've dictated 40,000 words".
Success here is the user thinking about this app less.

## Accessibility

- Every function reachable by keyboard; the history panel is fully operable without a mouse.
- Toggle mode exists precisely because sustained key-holding excludes some users
  ([HOTKEYS.md](HOTKEYS.md)).
- State is never signalled by colour alone — icon silhouette differs per state.
- Respects reduced-motion, system font scaling, and high-contrast modes.
- Sound cues are an alternative channel for recording state, not decoration; they are on by
  default and independently toggleable.
- Panel and settings windows carry proper accessibility labels — a dictation tool that is
  itself inaccessible to screen readers would be an embarrassment.
