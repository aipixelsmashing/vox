# Vox

Hold a key. Talk. Your words appear in whatever field you were already typing in.

Vox is a tray-resident dictation tool for macOS, Windows and Linux. Speech recognition runs
entirely on your machine — after the one-time model download the app never needs a network
connection again. No account, no subscription, no audio leaving the device.

> **Working name.** "Vox" is a placeholder until a trademark search clears a final name.
> Nothing technical depends on it — the bundle identifier, data directories and updater
> endpoint are all keyed to a stable string instead, so a rebrand costs icons and copy and
> nothing else. See [docs/adr/0009-naming.md](docs/adr/0009-naming.md).

## What it does

- Press and hold the hotkey (default: **right Option / right Alt**), speak, release.
- Text is inserted into the focused field of the app you were already using.
- Everything you dictate is kept in a small tray-resident history. Click an entry to put it
  back on the clipboard; delete entries individually or wipe the lot.
- Settings and update checks live in the same tray menu.

## Should you use this?

If macOS is your only computer and you mostly dictate short messages, **the dictation built
into your Mac is probably enough** — System Settings → Keyboard → Dictation. It's free, it's
on-device, and it needs no setup. We'd rather say that here than have you find out in a week.

Vox is for the cases the built-in tools handle badly:

- You use more than one operating system, and want the same key to do the same thing on all of
  them.
- You dictate for **minutes at a time**. macOS cuts out after about a minute; Vox runs a locked
  session until you stop it. See [docs/LONG-FORM.md](docs/LONG-FORM.md).
- Your vocabulary is full of names, libraries and jargon that generic models mangle. Vox
  watches the corrections you make and **learns your words** — locally, visibly, and deletably.
  See [docs/LEARNING.md](docs/LEARNING.md).
- You need the history as a safety net, or you need to prove to someone else that nothing left
  the machine.

## How it's built differently

1. **It never loses text.** Most tools write to the clipboard, synthesise Ctrl+V, sleep, and
   restore — a pattern that loses text, leaks transcripts into clipboard history, and reports
   success when nothing arrived. Vox verifies delivery, and when it can't, it says so and keeps
   the text. See [docs/TEXT-INJECTION.md](docs/TEXT-INJECTION.md).
2. **It gets better at your vocabulary without you configuring anything.** Every correction you
   make is a free label. This is only possible because everything is local.
3. **Offline is a guarantee, not a mode.** Two network calls exist in the entire codebase —
   model download and update check — and both can be turned off permanently. See
   [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md).
4. **Small enough to forget about.** Under 120 MB idle, because the model is memory-mapped and
   unloads when you're not using it, reloading before you reach for the key. There's no setting
   for it. See [docs/FOOTPRINT.md](docs/FOOTPRINT.md).
5. **Nothing to keep you here.** No account, no telemetry, no engagement mechanics, and
   everything you accumulate exports in one click. See [docs/VALUES.md](docs/VALUES.md).

## Status

Pre-alpha, **macOS only for now**. Windows and Linux are designed and documented but not built
until the Mac version is in daily use — see [docs/adr/0016](docs/adr/0016-macos-first.md).

Nothing is implemented yet: this repository contains the design, the architecture, and a
scaffold. The React UI runs today against a mock backend (`pnpm dev:mock`); the Rust module
bodies are `todo!()`.

Start at [HANDOFF.md](HANDOFF.md) if you're implementing, or [docs/PRD.md](docs/PRD.md) if
you're reading.

## Documentation map

| Document | What it covers |
| --- | --- |
| [HANDOFF.md](HANDOFF.md) | Where to start implementing, and what to stop and ask about |
| [docs/VALUES.md](docs/VALUES.md) | What this project optimises for, and what it refuses to |
| [docs/PRD.md](docs/PRD.md) | Problem, users, scope, requirements, success criteria |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Processes, threads, data flow, module boundaries |
| [docs/TECH-STACK.md](docs/TECH-STACK.md) | Every dependency and why it was chosen |
| [docs/HOTKEYS.md](docs/HOTKEYS.md) | Push-to-talk capture, the right-Alt problem, alternatives |
| [docs/TEXT-INJECTION.md](docs/TEXT-INJECTION.md) | Per-platform insertion chains and failure modes |
| [docs/MODELS.md](docs/MODELS.md) | Model registry, download, verification, licensing |
| [docs/LEARNING.md](docs/LEARNING.md) | Learning vocabulary from corrections, and its failure mode |
| [docs/LONG-FORM.md](docs/LONG-FORM.md) | Locked sessions for thinking out loud |
| [docs/LATENCY.md](docs/LATENCY.md) | Time-to-usable-text budget and measurement |
| [docs/FOOTPRINT.md](docs/FOOTPRINT.md) | Memory, residency, and why there's no setting for it |
| [docs/HISTORY.md](docs/HISTORY.md) | Transcript store, retention, clipboard behaviour |
| [docs/SETTINGS.md](docs/SETTINGS.md) | Config schema and defaults |
| [docs/UI-SPEC.md](docs/UI-SPEC.md) | Tray, history panel, settings, onboarding, copy |
| [docs/UI-KIT.md](docs/UI-KIT.md) | Design system: colour, type, layout, motion, and why |
| [docs/UI-CONTRACT.md](docs/UI-CONTRACT.md) | Every command and event between UI and core |
| [docs/UI-STATES.md](docs/UI-STATES.md) | Every screen state, and the copy deck |
| [docs/UI-DEVELOPMENT.md](docs/UI-DEVELOPMENT.md) | Building the UI in a browser, with no backend |
| [docs/PERMISSIONS.md](docs/PERMISSIONS.md) | OS permissions per platform and how to ask for them |
| [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) | Assets, adversaries, mitigations |
| [docs/PACKAGING.md](docs/PACKAGING.md) | Bundling, signing, notarisation, updates, channels |
| [docs/TESTING.md](docs/TESTING.md) | Test strategy including the hard-to-test parts |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Local setup |
| [docs/PRIOR-ART.md](docs/PRIOR-ART.md) | What already exists and where Vox sits |
| [docs/adr/](docs/adr/) | Architecture decision records |

## Licence

Apache-2.0 for the application code. Models are downloaded separately under their own
licences — see [NOTICE](NOTICE) and [docs/MODELS.md](docs/MODELS.md).
