# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Hold right Option, speak, release: the words appear in the field you were typing in.
  Transcription runs on this Mac with Apple's speech engine (macOS 26 or later).
- A menu bar icon that shows when Vox is listening or working, with Pause and Quit.
- When text can't be placed, it is put on the clipboard and a notification says why.
- Every dictation is kept in a local history database, including ones that could not be
  inserted, so nothing is lost. (The history panel itself comes later.)
- Escape cancels a dictation in progress; nothing is transcribed or stored.
- Electron and Chromium apps: Vox now asks them to expose their text fields before
  inserting. Where a field still can't be read back, the text is pasted anyway and the
  notification says it couldn't be confirmed, with the text also on the clipboard.
- A local signing identity for development, so permissions survive rebuilds.
- Transcription now runs while you hold the key, so the text appears almost as soon as you
  let go instead of a beat later.
- Apps where direct insertion never takes (Chromium-based ones) are remembered for the
  session, so later dictations there skip straight to the working method.
- Project design documents, architecture, and repository scaffolding.

<!--
Release entries are written for users, not for git. Each entry says what changed for the
person using the app. "Fixed clipboard restore race" — not "refactor PasteService".
-->
