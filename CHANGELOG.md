# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Vox now notices when you fix a word it got wrong: after placing text it watches that
  field for 90 seconds and keeps the wrong form, the right form, a count, the dates and
  which apps — never the sentence, never from a password field, and never a fix where both
  spellings are ordinary words. Nothing is applied yet; Settings → Privacy states what is
  stored and has a button that deletes all of it.
- Hold right Option, speak, release: the words appear in the field you were typing in.
  Transcription runs on this Mac with Apple's speech engine (macOS 26 or later).
- A menu bar icon that shows when Vox is listening or working, with Pause and Quit.
- When text can't be placed, it is put on the clipboard and a small message at the bottom
  of the screen says why (and a system notification too, once the app is signed).
- Every dictation is kept in a local history database, including ones that could not be
  inserted, so nothing is lost. (The history panel itself comes later.)
- Escape cancels a dictation in progress; nothing is transcribed or stored. The Escape is
  swallowed while you dictate, so the app you are dictating into never sees it as
  Option+Escape (which would open its completion menu).
- Electron and Chromium apps: Vox now asks them to expose their text fields before
  inserting. Where a field still can't be read back, the text is pasted anyway and the
  notification says it couldn't be confirmed, with the text also on the clipboard.
- A local signing identity for development, so permissions survive rebuilds.
- Transcription now runs while you hold the key, so the text appears almost as soon as you
  let go instead of a beat later.
- Apps where direct insertion never takes (Chromium-based ones) are remembered for the
  session, so later dictations there skip straight to the working method.
- Project design documents, architecture, and repository scaffolding.
- A short rising tone when recording starts and a falling one when it stops, so you know
  the microphone is open even when the menu bar is on another screen. On by default;
  `ui.soundCues` in settings turns it off.

- A small pill next to the cursor while you dictate: an amber ring that moves with your
  voice and the elapsed time, then "Transcribing…" until the text lands. It never takes
  focus, clicks pass through it, and it shows over full-screen apps. Settings → Dictation
  turns it off, next to the sound cues, which now have their own switch there too.
- Privacy → "Read the field you're dictating into", off by default: at key-down Vox
  reads the text around your cursor in the field you are in, and hands up to twenty
  unusual words from it (names, identifiers, your custom words) to Apple's recogniser as
  hints for that one dictation. Never a password field, never stored, never logged, never
  leaves this Mac. Each history row says how many hints it had ("· 6 hints"), and
  Diagnostics counts how many dictations had any. Your custom words in Settings → Text
  ride along as hints when the field supplied some; on their own they keep working as
  replacements, as before.
- First-launch setup in three short steps, including the one that says you may not need
  this. It remembers where you got to, and Settings → Dictation can run it again.
- Settings: six panes from the menu bar — Dictation (change the key by pressing it, mode,
  microphone, recording cap), Model, Text (how text is placed, spacing, custom words),
  Vocabulary, Privacy (what Vox may read, history retention and wipe, offline lock,
  update checks, export everything) and Diagnostics (your own numbers). Changes apply at
  once; nothing needs a restart except granted permissions.
- The history panel: click the menu bar icon (or press ⌘⇧Space) to see what you dictated,
  ranked so the thing you just said into this app is at the top and anything that
  couldn't be placed floats. Click a row to copy it; Insert puts it into whatever is
  focused; × deletes it; Delete all… wipes the database. Fully keyboard-operable.

### Changed
- Vox keeps watching your last three dictations for fixes, not only the latest one. Dictate
  a sentence, dictate the next, then go back and fix a name in the first: that fix now
  counts.

### Fixed
- Closing the history panel — by clicking a row, Enter, Escape, the shortcut or the menu bar
  icon — returns you to the app you were in. Before, Vox stayed in front, so a setup or
  settings window left open behind other apps came forward in the panel's place.
- Updated the audio ring-buffer dependency past a memory-safety advisory (RUSTSEC-2026-0293).
  Vox never used the affected calls.
- Notifications use the current macOS API instead of one the system now ignores. On a
  properly signed build the first one asks for permission and Vox then appears in System
  Settings → Notifications; development builds are refused by macOS and show nothing.
- Cancelled, too-short and silent dictations are recorded in the log (never the words), so a
  "nothing happened" can be told apart from "nothing arrived".

<!--
Release entries are written for users, not for git. Each entry says what changed for the
person using the app. "Fixed clipboard restore race" — not "refactor PasteService".
-->
