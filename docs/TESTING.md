# Testing strategy

Most of what can go wrong here cannot be caught by a unit test, so the strategy is explicit
about which layer catches what.

## Automated

**Unit** — settings migrations (every version pair), text post-processing and dictionary
substitution (snapshot tests with `insta`), history retention pruning, model manifest parsing
and hash verification, hotkey binding parsing and chord matching, state machine transitions
including every illegal transition.

**Integration, in-process** — the pipeline end to end with a mock audio source and a mock
injector: WAV fixture in, expected transcript out, expected history row written, expected
outcome recorded. Fixtures cover: clean speech, background noise, silence only, a 0.2 s tap,
a recording that hits the cap, and non-English input.

**Command surface** — `tests/settings_command.rs` builds the app on Tauri's mock runtime
with the real command handlers and drives `settings_set` and `settings_get` through the
IPC layer, argument deserialisation included, with the argument shaped exactly as
`src/lib/contract.ts` declares it. It asserts the write comes back merged, reads back, is
on disk in `settings.json`, and survives a reload; and that a mis-shaped call (the patch
wrapped in a key, a wrong value type) is an error the caller sees, never a silent no-op.
That is the class of bug that once rejected every settings call for days without a log
line. The test sets `VOX_HOME` to a temporary directory so it never touches the real file.

**Property tests** — arbitrary transcripts survive the post-processing pipeline without
mangling; arbitrary settings JSON either parses or fails cleanly, never panics.

**Benchmarks** — `criterion` per-stage timings against `benches/baseline.json`; a >15% p50
regression fails the PR. See [LATENCY.md](LATENCY.md).

**Footprint gate** — `cargo bench --bench footprint` scripts a five-minute run with three
dictations and records idle RSS, peak RSS and idle CPU. A >15% idle-RSS regression fails the
PR, exactly like a latency regression. See [FOOTPRINT.md](FOOTPRINT.md).

**Learning tests** — edit alignment against a fixture corpus of insertion/correction pairs,
including the negatives that must *not* produce candidates: whole-sentence rewrites, deletions,
edits after focus left the app, anything in a field we refused to insert into, and the
homophone guard — their/there/they're, to/too/two, its/it's, affect/effect, than/then,
your/you're, whose/who's, principal/principle, lead/led, bear/bare — none may produce a
candidate, while "cuber netties" → "Kubernetes" and "acks UI element" → "AXUIElement" must. Plus the
three-occurrence threshold, the auto-suspend rule, and the guarantee that deleting a term also
deletes its evidence.

**UI contract coverage** — every command in `src/lib/contract.ts` has a mock implementation,
and every state documented in [UI-STATES.md](UI-STATES.md) has a scenario in
`src/mock/scenarios.ts`. Both fail the build when they drift, which is what stops error and
empty states from being discovered at review time.

**Export tests** — a table with no exporter fails the build. Every export round-trips: history
out and back in without loss ([adr/0015](adr/0015-exit-is-cheap.md)).

**Guard tests** — the ones that protect the product's promises:

- With `network.offlineLock` on, a full dictation cycle plus an attempted update check opens
  zero sockets.
- No transcript text appears in log output at any log level.
- `telemetry.rs` contains no network code (asserted structurally, not by convention).
- Every model registry entry has a licence, an attribution string, and a SHA-256 per file.
- Correction candidates never contain text beyond the two forms — asserted against a fixture of
  long, sensitive-looking corrections.
- No engagement surface: no code path emits a notification, badge or summary that is not a
  failure or an explicit user request.
- `InjectionOutcome` has no variant that implies success without verification — enforced by an
  exhaustive match in a test that must be updated deliberately if the enum changes.

## Manual gates

Required per platform before any release. Automation cannot cover system-level key taps or
insertion into third-party applications.

### M1 loop (run before calling M1 done)

Cannot be automated: every step involves a real key tap, a real microphone, or a real
application's text field. Notifications only appear on an Apple-signed build
([PERMISSIONS.md](PERMISSIONS.md#macos)); on a dev build, check the log line for the same
outcome instead (`dictation: … outcome "clipboard_only"`, `refused: …`, `cancelled …`).

1. Fresh launch with no permissions granted: three prompts appear (Input Monitoring,
   Accessibility, microphone), each notification uses the deck wording, the tray shows the
   attention badge, and after granting and relaunching the badge clears. The first
   notification also brings the macOS notification-permission prompt; after allowing, Vox is
   listed in System Settings → Notifications.
2. Hold right Option in Notes, say a sentence, release: the tray goes recording → working →
   idle and the text appears at the caret with a trailing space.
3. Same in Terminal: text appears via paste (check the history row's `method`).
4. Same in a Chromium browser's address bar and in a web textarea.
4a. Same in an Electron app (the Claude desktop app, Slack, VS Code): the log shows the
    element found "per-app-after-wake", and the text arrives, inserted or at least pasted
    with the "couldn't confirm" notification. Never nothing.
5. Hold, say something, switch apps by clicking another app's window, release: nothing is
   typed into the new app, the text is on the clipboard, and the notification names both
   apps. (⌘Tab does not open the app switcher while Option is held, so use the mouse.)
6. Hold, press Escape, release: nothing inserted, nothing in history, and the app did not
   react to the Escape either — in Notes, no completion menu (Option+Escape) appears.
7. Tap right Option for under 120 ms: nothing happens.
8. Hold in a password field: "Not inserted — a password field is active." and nothing on
   the clipboard.
9. Pause dictation from the tray: the hotkey does nothing; unpause: it works again.
10. Quit from the tray: the process exits; nothing keeps running.

### Sound cues (M3)

Cannot be automated: the assertion is that a human hears it.

1. Hold right Option: a short rising two-tone plays as recording starts, after the
   microphone is open. Release: a falling one. The two are distinguishable with eyes closed.
2. Set `ui.soundCues` to false: both are silent; dictation is otherwise unchanged.
3. With the output routed to headphones, the cue is not audible in the room and the
   transcript is unaffected either way.

### History panel (M3)

Cannot be automated: a real tray, a real focus change, a real clipboard.

1. Left-click the tray icon: the panel opens under it, focused, search box ready. Click
   elsewhere: it hides. Left-click again: it toggles. Right-click: the menu, with "Show
   history" doing the same.
2. `history.panelHotkey` (default ⌘⇧Space): toggles the panel from any app, and the app
   underneath does nothing with the chord.
3. Arrow keys move the selection, Enter copies and closes, ⌘Enter inserts, Delete removes,
   Space expands a long row, Escape closes, typing filters — all without the mouse, and all
   after having clicked a button first (WebKit leaves focus on the body after a click).
4. Dictate while the panel is open: a live row appears at the top and the new transcript
   arrives in the list when the dictation ends.
5. "Insert" on a row with a text field focused in the app underneath: the panel hides and
   the text lands there. With no text field focused: the text goes to the clipboard and
   the row shows why.
6. "Delete all…" asks once, then the list is empty and `history.db` has been vacuumed
   (file size drops).
7. `history.enabled` false: the panel says history is off and offers nothing else.

### Settings window (M3)

Cannot be automated: real permissions, a real key capture, a real restart.

1. Tray → Settings…: the window opens on Dictation. Close it: it hides and reopens on the
   same pane with its state intact.
2. With a permission missing, the strip at the top names it in the deck's words and "Open
   System Settings" lands on the right pane. After granting: "Quit and reopen" restarts Vox.
3. Dictation → Change…: press a key; the row shows it and dictation uses it at once, without
   a restart. Set it back.
4. Dictation → turn off sound cues, dictate: silent. Privacy → turn history off: the panel
   says so and nothing new is stored. Turn both back on.
5. Privacy → Export everything: the folder holds `history.md`, `history.json` and
   `vocabulary.txt`, and the note names the counts.
6. Diagnostics: the app table matches what you dictated into tonight; memory is a number.

### Onboarding (M3)

Cannot be automated: real prompts, a real microphone, a real key.

1. Move `settings.json` aside and launch: the onboarding window opens on step 1 with the
   "you may not need this" paragraph. Quit at step 2, relaunch: it opens on step 2.
2. Step 2: the meter moves while you talk, before any hotkey permission exists; each missing
   grant has a button that lands on the right pane; after granting, "Quit and reopen"
   restarts Vox and the rows read "Granted".
3. Step 3: toggle a choice and see it stick in `settings.json`; hold right Option, speak,
   release: the text lands in the box and the step says so. Done hides the window;
   Settings → Dictation → "Run setup again" brings it back.

### Toast (M3)

Cannot be automated: the assertion is about focus and placement on a real screen.

1. Dictate with no text field focused (Finder in front): a toast appears at the bottom
   centre reading "Copied. No text field was focused.", sized to its text, and the caret in
   whatever you were in is untouched — the toast never takes focus.
2. It goes away by itself after about four seconds; a click dismisses it sooner.
3. Two failures in a row: the second message replaces the first and the timer restarts.
4. Dictate successfully: no toast. Success is the text appearing.

### Recording overlay (M3)

Cannot be automated: placement relative to a real caret, and a real full-screen Space.

1. Hold right Option in Notes with the caret mid-paragraph: a pill appears just below the
   caret within a beat, the ring moves with your voice, the time counts up. Release: it
   reads "Transcribing…" and is gone when the text lands. Nothing in Notes lost focus.
2. Same with the caret on the last line of a window at the bottom of the screen: the pill
   sits above the caret instead.
3. Same in a Chromium web textarea, and in an app with no readable caret (Finder in front):
   the pill appears at the bottom centre of the screen.
4. Click on the pill while recording: the click goes through to whatever is under it.
5. Full-screen app (Notes in full screen): the pill still appears.
6. Escape while recording, and a tap under 120 ms: the pill goes away with nothing placed.
7. Settings → Dictation → turn the level meter off: no pill; dictation otherwise unchanged.
   Turn on Reduce Motion in System Settings → Accessibility → Display: the ring is a
   three-step meter that steps rather than animates.

### Context biasing (M3)

1. Put a rare name on screen in the field, say it: wrong with `privacy.readFocusedField`
   off, right with it on.
2. Same name in a password field: unchanged, and the log shows the read was refused.
3. Turn the setting off mid-session: the next dictation sends no hints (its history row
   shows no hint count; Diagnostics' "Sent for N of M" stops rising). With a custom word
   in Settings → Text the count is 1: the dictionary is a hint too.
4. First dictation after turning the setting on: the log shows "dictation module ready"
   before it, or "hints dropped" with a count if it came too soon. Never a missing or
   late transcript either way.

### Hotkey checklist

1. Hold the binding in a text editor: recording starts, stops on release.
2. Hold it while the app is unfocused and while a full-screen app is frontmost.
3. Tap it briefly — nothing happens.
4. Hold, press Escape — nothing is inserted, nothing is stored.
5. Hold during an existing transcription — ignored, busy state shown.
6. On an AltGr layout: confirm the warning appears and the suggested alternative works.
7. Toggle mode and double-tap-hold mode.
8. Revoke the OS permission mid-session: the tray badges within one check cycle.

### Injection compatibility matrix

The application list in [TEXT-INJECTION.md](TEXT-INJECTION.md), per platform, recording for
each: method used, outcome, and observed latency. Results committed to
`docs/compat/<version>.md` so regressions are visible across releases.

### Long-form and learning

1. Lock a session, speak for five minutes with long pauses — session does not end, memory stays
   flat, text keeps appearing.
2. Stop, route to each destination in turn.
3. Dictate a known-wrong proper noun, correct it in the target app, three times across two
   sessions — term appears in the Vocabulary pane with correct provenance.
4. Delete it; dictate and correct once more — it does not immediately reappear.
5. Turn learning off; confirm applied terms stop firing and capture can be wiped.

### Install and update

Fresh install on a clean VM per platform; first-run onboarding to first successful dictation;
update from the previous release with settings and history preserved; uninstall leaves no
background process and removes or clearly documents remaining data.

## Hardware matrix

Benchmarks and the slow-machine gate run on the reference machines listed in
[LATENCY.md](LATENCY.md). Additionally test at least one machine with no GPU acceleration
available and one with 8 GB RAM, since the memory-resident model is the biggest risk on
low-spec hardware.

## CI

`ci.yml` on every PR and on `main`:

- **rust** (macOS 26 runner, `src-tauri/`): `fmt --check`, `clippy --all-targets -D warnings`,
  `test`, and `bench --no-run` so the benches keep compiling. macOS only while v1 is macOS
  only ([adr/0016](adr/0016-macos-first.md)); the Linux and Windows legs return with M8. The
  >15% p50 and idle-RSS regression gate is wired when M2 lands the real benches and
  `benches/baseline.json`; until then the benches are placeholders that print so.
- **supply-chain** (`cargo-deny`, `src-tauri/deny.toml`): advisories, licence allow-list,
  sources. Unmaintained crates reached only through Tauri are ignored by ID with the reason
  written next to it; a vulnerability is never ignored.
- **frontend**: `pnpm typecheck` and `vitest`. The two tests that exist today are the
  contract-coverage and scenario-coverage checks above.
- **docs**: `typos` (false positives go in `_typos.toml` with a comment) and an offline
  `lychee` link check over `docs/` and the root Markdown files.

All four run locally with the same commands; `brew install typos-cli lychee cargo-deny` for
the tools.
