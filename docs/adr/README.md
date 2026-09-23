# Architecture decision records

Short records of decisions that were not obvious, so that "why is it like this?" has an answer
and reversing a decision is a deliberate act rather than an accident.

Format: context, decision, consequences. One page maximum. Status is `accepted`, `superseded
by NNNN`, or `proposed`.

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-tauri-over-electron-and-native.md) | Tauri over Electron and over per-platform native | accepted |
| [0002](0002-react-frontend.md) | React for the small amount of UI | accepted |
| [0003](0003-keytap-for-push-to-talk.md) | `keytap` for global key capture | accepted |
| [0004](0004-parakeet-default-engine.md) | Parakeet TDT as the default engine, Whisper as fallback | accepted |
| [0005](0005-no-unverified-injection-success.md) | Never report insertion success we cannot verify | accepted |
| [0006](0006-sqlite-history.md) | SQLite for transcript history | accepted |
| [0007](0007-no-telemetry.md) | No telemetry, not even opt-in | accepted |
| [0008](0008-models-not-bundled.md) | Models downloaded, not bundled | accepted |
| [0009](0009-naming.md) | "Vox" is a working name, and nothing depends on it | accepted |
| [0010](0010-adaptive-residency.md) | Adaptive residency instead of a memory/speed setting | accepted |
| [0011](0011-learn-from-corrections.md) | Learn vocabulary from corrections, locally | accepted |
| [0012](0012-no-folders.md) | No folders, tags, or filing for transcripts | accepted |
| [0013](0013-os-speech-engine.md) | Use the OS speech model where one exists | accepted |
| [0014](0014-long-form-sessions.md) | Long-form is a session, not a longer timeout | accepted |
| [0015](0015-exit-is-cheap.md) | Exit is cheap | accepted |
| [0016](0016-macos-first.md) | macOS first, and one engine | accepted |
| [0017](0017-context-from-focused-field.md) | Recognition is biased by the text in the focused field | proposed |
