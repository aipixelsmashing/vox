# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Text size for the history panel: Small, Medium or Large in Settings → Dictation, or ⌘+ and
  ⌘− while the panel is open. The Mac's own Larger Text accessibility setting applies on top.
  ⌘, opens Settings from the panel.
- Tap the dictation key twice to keep listening hands-free; tap once to stop. Escape still
  cancels. This replaces the earlier design of pressing L while holding the key.
- The start tone plays once a press has lasted the minimum hold, so a brush of the key, or
  the first tap of a double tap, makes no sound.
- Fn (the Globe key) can be the dictation key. When you choose it, Vox tells you if macOS
  still has "Press 🌐 key to" set to something, and that some external keyboards never send
  Fn at all. Right Option stays the default.
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
- The app declares macOS 26 as its minimum, so on an older Mac the system says so when you
  open it, instead of Vox installing and then reporting that transcription is unavailable.
- The menu bar icon opens the history panel on any click; it no longer has a menu of its
  own. The cog in the panel's header is the menu: Settings…, Pause dictation, Check for
  updates and Quit Vox. The header also shows "Listening…" and the elapsed seconds while you
  dictate, instead of a row at the top of the list.
- The history panel looks like a Mac panel: the desktop shows through it, blurred, as in
  Control Center; the selected transcript and the search field's focus ring take the accent
  colour you chose in System Settings instead of amber; rows are inset and rounded with no
  lines between them; Copy, Insert and × appear on the row you are on rather than on every
  row; the icons match the weight of the system's own.
- Transcripts are set in the system typeface like the rest of the panel, at 13 points, and
  told apart from labels by colour and size rather than by a second font. Vox no longer
  bundles a typeface of its own.
- Settings → Vocabulary shows one row per learned word, with every way it was misheard
  listed beneath it, and the delete button removes the word with all of them. The count and
  the export sit in the header above the list, with a filter field once there are more than
  twelve words.
- A name Vox gets wrong a different way each time is now learned: three corrections to the
  same word, however it was misheard, make it a word the recogniser is told to expect.
  Replacing one specific mishearing still needs that same fix three times.
- With "Apply what it has learned" on, learned words and the words in your dictionary
  are now used in every dictation, whether or not Vox may read the field you are
  dictating into. The cost: the last word of
  a dictation is dropped now and then while it is on.
- A fix is only counted once you have finished typing it. Vox used to be able to catch a
  word half-typed and store that.
- Vox keeps watching your last three dictations for fixes, not only the latest one. Dictate
  a sentence, dictate the next, then go back and fix a name in the first: that fix now
  counts.

### Fixed
- Clicking the menu bar icon reliably opens the panel. (The panel's show, hide and
  focus-loss are now recorded in the log so a click that goes nowhere can be told from one
  that opened and immediately closed.)
- Choosing Fn (the Globe key) as the dictation key did nothing: holding it never started a
  recording, and nothing said why. It now works. The keyboard library Vox uses dropped the
  key; Vox builds with a corrected copy until the fix is released upstream.
- The menu bar icon was a solid square. macOS draws a menu bar icon from its transparency
  alone, and the images had an opaque background. They are redrawn: an outline microphone
  at rest, filled inside a ring while listening, inside an open arc while working, with a
  corner badge when something needs you, and with a small dot whenever the microphone is
  kept open between dictations.
- Updated the TLS libraries used by the update check past three advisories
  (RUSTSEC-2026-0346, RUSTSEC-2026-0347 and RUSTSEC-2026-0382) and replaced a yanked
  helper crate. The first two did not affect Vox, which has no custom session store and does
  not use Encrypted Client Hello; the third concerned certificate verification, which the
  update check does rely on.
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
