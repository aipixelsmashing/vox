# Roadmap

**macOS first.** Windows and Linux are designed for and documented, but not built until the Mac
version is something people use daily. See [docs/adr/0016](docs/adr/0016-macos-first.md).

**No durations.** Milestones end when their exit criteria are met, not when a week is up.
Estimates written for a human team are noise when the implementation is agent-driven; exit
criteria are something both a person and an agent can work against.

Milestones are ordered by risk first and user pain second. Reliability has its own milestone
above every feature, per [docs/VALUES.md](docs/VALUES.md#priority-order-when-things-conflict).

---

## M0 · Spikes

Four questions, each of which can invalidate the design. **Throwaway code, run on real
hardware, findings written to `docs/spikes/`.** These are the one part of the project a human
must do, because they are all "open a real app and look at what happened".

- **S1 — Hotkey.** Does `keytap` deliver right-Option down and up, from an unfocused app, with
  the Accessibility and Input Monitoring permissions granted? Measure event latency.
- **S2 — Insertion and read-back.** Does accessibility insertion work in Slack, Chrome, VS Code,
  Notes, Terminal and one Electron app? **Can the field be read back afterwards** — the
  mechanism correction learning depends on?
- **S3 — Engine.** Is Apple SpeechAnalyzer available and accurate enough on your machines? What
  is its latency for a 6 s utterance? **What macOS version are the people you'd share this with
  on?** — this is the gating question, see below.
- **S4 — Footprint.** Idle RSS with SpeechAnalyzer resident. Expected to be trivial; confirm.

**Exit:** four findings written up. Any revised number back in the docs before M1 starts.

### The one decision S3 gates

SpeechAnalyzer needs macOS 26+. If everyone you'd share with is on 26+, v1 needs **no model
download, no ONNX runtime, no `parakeet-rs`, no hash verification, no memory-mapped weights and
no residency prediction** — roughly a third of the designed system disappears.

If some are on older macOS, `whisper-rs` with a bundled `base.en` (~60 MB) comes back as a
fallback engine behind the same trait. Everything else still goes.

---

## M1 · The loop

Hotkey → record → transcribe → insert. Tray icon and a quit item, nothing else.

- Pipeline state machine with cancel, minimum hold, length cap
- cpal capture, resample, VAD trim
- SpeechAnalyzer engine
- macOS injection chain: secure-input check → accessibility insert with read-back verification
  → clipboard paste with change-count restore → Unicode events
- Permissions onboarding, because nothing works without it

**Exit:** you use it daily instead of typing, for a week, without reaching for the old way.

## M2 · Reliability

No screenshot, highest value in the project.

- Compatibility matrix across the app list in
  [docs/TEXT-INJECTION.md](docs/TEXT-INJECTION.md#compatibility-matrix), results committed
- Every `ClipboardOnly` reason reachable and correctly reported
- Failed rows recoverable via re-insert
- State machine fuzzed against interleaved hotkey, cancel, focus-change and engine-failure events

**Exit: zero dictations where the text did not arrive anywhere.** Nothing else starts until the
matrix is clean.

## M3 · A product

- History store and panel: decayed ranking, current-app boost, search, copy, re-insert, delete,
  wipe, export
- Settings: six panes ([docs/SETTINGS.md](docs/SETTINGS.md))
- Onboarding, including the step that tells people they may not need this
- Tray states, recording overlay, sound cues, pause
- Context-aware recognition biasing from the focused field ([docs/CONTEXT.md](docs/CONTEXT.md)),
  promoted from M9 by spike S5: read at key-down, focused field only, at most 20 terms
  nearest the caret, never stored, an opt-in Privacy-pane setting ([adr/0017](docs/adr/0017-context-from-focused-field.md))

**Exit:** someone who is not you installs it from a `.dmg` and dictates successfully without
being talked through it.

## M4 · Correction capture

Silent. Nothing user-facing except a line in the privacy pane and a delete button.

**Exit:** candidates accumulating correctly against real use, verified by inspecting the table.
Ship this early — the corpus takes months of *use* to build, and that clock only starts when
the code lands.

## M5 · Sharing it

- Apple Developer ID, notarisation, `.dmg`, signed updater manifests
- `vox.pixelsmashing.com/updates/` redirect live **before** the first signed build reaches anyone
- Minisign keypair generated, backed up offline, held by two people

**Exit:** a colleague installs from a link, with no Gatekeeper warning, and gets an update.

## M6 · Long-form sessions

Live panel, chunked transcription, destinations. The lock gesture itself shipped with the
hotkey ([docs/HOTKEYS.md](docs/HOTKEYS.md#the-lock-hands-free-without-a-second-key)).
See [docs/LONG-FORM.md](docs/LONG-FORM.md).

**Exit:** a ten-minute session produces usable text in a file, memory stays flat throughout.

## M7 · Learned vocabulary

Application of what M4 has been collecting: vocabulary pane, provenance, delete, auto-suspend,
export. Needs a real corpus behind it.

**Exit:** corrections per 100 words measurably falling for a real user over several weeks.

## M8 · Windows and Linux

Only once the Mac version is stable and in daily use by several people. This is where
`parakeet-rs`, `win-text-inject`, the model downloader and the Wayland fallback chain come back
— all already designed, none of it built.

## M9 · Under consideration

Not commitments. Each needs a case made against [docs/VALUES.md](docs/VALUES.md).

- History encryption at rest ([PRD](docs/PRD.md) Q3)
- Homebrew cask
- Reproducible builds

---

## Explicitly not on the roadmap

Cloud models, accounts, telemetry, meeting transcription, mobile apps, voice commands,
**folders or tags**, **engagement mechanics**, and **a residency setting**. Each is either a
different product, work pushed onto the user, or a violation of the core promise. See
[docs/PRD.md](docs/PRD.md#v10--explicitly-not-doing), [docs/VALUES.md](docs/VALUES.md), and
ADRs [0010](docs/adr/0010-adaptive-residency.md) and [0012](docs/adr/0012-no-folders.md).

Also not on the roadmap: LLM rewriting of what you said. Learned vocabulary changes individual
terms it has watched you correct three times. It does not touch your meaning.
