# Product requirements

## The problem

Typing is the bottleneck in a lot of everyday computer use — chat replies, commit messages,
search boxes, form fields, prompts to AI tools. People speak roughly three times faster than
they type. The OS dictation built into macOS and Windows exists but is slow to invoke,
inconsistent about where text lands, and on some configurations sends audio to a server. The
good commercial tools (Wispr Flow, Superwhisper, Aqua) are subscription products that upload
audio or require an account.

The gap: a dictation tool that is **instant to invoke, lands text exactly where you were
already typing, and provably never phones home.**

## Users

**Primary — the heavy keyboard user.** Developers, writers, support agents, analysts. Already
comfortable with hotkeys. Cares about latency above almost everything: if release-to-text is
over a second, they go back to typing. Suspicious of cloud tools by default.

**Secondary — the privacy-constrained professional.** Clinicians, lawyers, journalists,
people under NDA or in regulated environments. They cannot use a cloud dictation tool at all.
For them "works offline" is not a preference, it's the entry requirement, and they need to be
able to demonstrate it to someone else.

**Tertiary — accessibility users.** People for whom typing is painful or slow. Note that
push-to-talk *requires holding a key*, which is itself a problem for some users with motor
impairments — a toggle mode is a requirement, not a nice-to-have (see [UI-SPEC](UI-SPEC.md)).

## Product principles

1. **The hotkey is the product.** Everything else is support. Time from key-up to text on
   screen is the metric that matters.
2. **Quiet by default.** Tray icon, no dock icon, no window unless asked, no notifications
   except when something failed.
3. **Never silently lose a transcript.** If insertion fails for any reason, the text is on
   the clipboard and the user is told, in one line, what to do next.
4. **No network is the default state.** After setup, an offline lock can be engaged and the
   app will never open a socket again.
5. **No accounts, no telemetry, no analytics, ever.** Not "opt-in analytics". None.
6. **It never loses text.** The unglamorous 5% — the silent insertion failure — outranks every
   feature on the roadmap. One of those costs more trust than ten good features earn.
7. **Exit is cheap.** Everything the user accumulates exports to plain formats. The realistic
   risk to them is not a competitor; it is this project being abandoned.
8. **Say when you're not the right tool.** Including in onboarding. See [VALUES.md](VALUES.md).

## Scope

### v1.0 — must have

| # | Requirement |
| --- | --- |
| R1 | Tray-resident app, launches at login, no dock/taskbar presence |
| R2 | Global push-to-talk on a configurable key; default right Option / right Alt; works when the app is unfocused |
| R3 | Recording starts on key-down, stops on key-up, with a visible and optionally audible state change |
| R4 | Fully local speech recognition via Apple SpeechAnalyzer — no model download, no network at any point |
| R5 | Transcript inserted into the focused editable field of the previously focused app |
| R6 | If insertion is impossible, transcript goes to the clipboard and the user is told why |
| R7 | History of transcripts in the tray: click to copy, delete one, delete all |
| R8 | Settings window: hotkey, mode, input device, model, insertion method, history retention, launch at login |
| R9 | Manual "Check for updates" plus a signed auto-update path |
| R10 | Escape during recording cancels with nothing inserted |
| R11 | macOS packaged, signed and notarised. Windows and Linux designed but deferred — [adr/0016](adr/0016-macos-first.md) |
| R12 | First-run onboarding: permissions, mic test, model download, hotkey test — including telling the user when they may not need this app at all |
| R13 | Correction capture: observe post-insertion edits and store candidate vocabulary locally ([LEARNING.md](LEARNING.md)) |
| R14 | Learned vocabulary applied automatically, with every term visible, provenanced and deletable |
| R15 | Long-form sessions: lock the hotkey, speak for minutes, see text as it forms, route the result to a destination ([LONG-FORM.md](LONG-FORM.md)) |
| R16 | Export everything — history, vocabulary, settings — to plain text and JSON in one click |
| R17 | Idle footprint under 120 MB, achieved by adaptive residency rather than a setting ([FOOTPRINT.md](FOOTPRINT.md)) |

### v1.0 — explicitly not doing

- Cloud model support of any kind, even opt-in. It undermines the core claim; a fork can add it.
- Real-time streaming display of partial text (v1.1 — see [ROADMAP](../ROADMAP.md)).
- LLM post-processing / "clean up my rambling". Attractive, but it doubles the compute
  budget, doubles model download size, and introduces a failure mode where the app silently
  changes what you said. v1.2 at the earliest, off by default, local models only.
- Meeting or system-audio transcription. Different product, different threat model.
- Mobile, browser extension, voice commands, dictation macros.
- **Folders, tags, or any filing system for transcripts.** Filing is work the user does for the
  system's benefit, and nobody will do it. Recency, context ranking and search cover the real
  need; long-form output is routed to a destination instead of accumulating here. See
  [adr/0012](adr/0012-no-folders.md).
- **Engagement mechanics of any kind** — streaks, usage summaries framed as achievement, share
  prompts. Success is the user thinking about this app less. See [VALUES.md](VALUES.md).
- A residency or memory/speed setting. The system decides; see [FOOTPRINT.md](FOOTPRINT.md).
- Storing audio. Audio is held in memory, transcribed, and dropped. An opt-in debug mode may
  keep the last N recordings on disk for bug reports.

## Functional requirements in detail

### Capture

- Key-down begins capture within 30 ms. Key-up ends it.
- A hold shorter than `minHoldMs` (default 120 ms) is discarded as an accidental tap.
- Recording is capped at `maxRecordingSec` (default 120 s); at the cap, capture stops, the
  audio so far is transcribed, and the user is told the cap was hit.
- Voice activity detection trims leading and trailing silence and rejects recordings with no
  speech, so an accidental hold produces nothing rather than a hallucinated line of text.
- The insertion target (process, window, focused element) is captured at key-**down**, not
  key-up, so that switching windows mid-utterance can be detected and handled.

### Recognition

- Default engine is a local ONNX Parakeet TDT model; Whisper via whisper.cpp is available for
  languages Parakeet doesn't cover. See [MODELS.md](MODELS.md).
- The model stays resident in memory between uses (configurable) so there is no per-use load cost.
- Language is auto-detected by default and can be pinned.
- A user dictionary applies literal find/replace after recognition, for names and jargon the
  model gets wrong. Case-insensitive match, case-preserving replace.

### Insertion

Detailed in [TEXT-INJECTION.md](TEXT-INJECTION.md). The requirement in one line: **either the
text arrives in the field, or the user finds out immediately and the text is on the clipboard.**

### History

Detailed in [HISTORY.md](HISTORY.md). Text only by default, capped, searchable, deletable
individually or wholesale, stored with restrictive file permissions.

## Success criteria

Measure the user's experience, not the system's properties. Latency and memory are means, and
they are tracked in [LATENCY.md](LATENCY.md) and [FOOTPRINT.md](FOOTPRINT.md). These are the
ends.

| Measure | Target | How |
| --- | --- | --- |
| **Dictations where the text did not arrive anywhere** | **Zero** | Counted per release across the compatibility matrix and from local diagnostics |
| **Corrections per 100 dictated words** | Trending down for a given user, month over month | Derived from the correction watch ([LEARNING.md](LEARNING.md)) |
| **Sessions where the user gave up and typed instead** | Falling | Dictation abandoned or deleted within 30 s of insertion |
| Time from install to first successful dictation | < 5 minutes including model fetch | Onboarding instrumentation, local |
| Idle memory | < 120 MB | [FOOTPRINT.md](FOOTPRINT.md) |
| Release-to-**usable** text, 6 s utterance | p50 < 500 ms with no corrections needed | [LATENCY.md](LATENCY.md) |
| Network connections after setup, with offline lock on | 0, verifiable by the user | Guard test + the user's own network monitor |

All three headline measures are computed **locally**, from data that never leaves the machine,
and shown to the person in Settings → Diagnostics. They are reading their own numbers to
decide whether this tool earns its place — which is the right relationship with someone who is
not paying for it. See [VALUES.md](VALUES.md).

The first measure outranks every other line in this document. One silent failure costs more
trust than ten good features earn.

## Open questions

- **Q1.** Should right Option be the default given it is AltGr on many European layouts? See
  [HOTKEYS.md](HOTKEYS.md) — current answer is yes, with layout detection and a warning.
- **Q2.** Ship a small model bundled in the installer for instant first use, at the cost of a
  ~200 MB download for everyone? Current answer: no, but offer a bundled-model installer
  variant for offline/air-gapped installs.
- **Q3.** Encrypt the history database at rest using an OS keychain key? Adds a keychain
  prompt and a recovery problem. Deferred to v1.1; v1.0 relies on file permissions and a
  clear statement in the UI.
- **Q4.** Does the tray history panel need search in v1.0, or is a 200-item scroll enough?
- **Q5.** How often does correction learning go wrong? Unknown until capture has run for
  months. This is why learning ships off by default for the first year rather than shipping
  with a confident default. See [LEARNING.md](LEARNING.md#rollout).
- **Q6.** If adaptive residency and mmap work, is the right default engine the *smallest* model
  that clears the accuracy bar rather than the most accurate one that fits? Spike S4.
