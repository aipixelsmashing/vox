# S3 — Engine: is Apple SpeechAnalyzer available and fast enough?

**Answer: yes.** On macOS 26.5.2 the framework is present, the en-US model transcribes a
6 s utterance in ~165 ms warm and ~250 ms cold, and the transcript of the test fixture was
word-perfect. Target users are on macOS 26+ (decision recorded 2026-09-15), so
SpeechAnalyzer is the sole v1 engine and the Whisper fallback stays out.

Run 2026-09-15 on macOS 26.5.2 (25F84), Apple M1 Pro, 16 GB, release build. Harness:
[`spikes/s3-engine`](../../spikes/s3-engine/), a Rust binary linking a Swift bridge
([`bridge/S3Bridge.swift`](../../spikes/s3-engine/bridge/S3Bridge.swift)). Fixture:
`fixtures/audio/clean-6s.wav`, 6.06 s, 16 kHz mono, synthesised with `say -v Samantha`.
**No real-voice recording was tested**; see open items.

## What was tried

`SpeechTranscriber(locale:preset: .transcription)` inside a `SpeechAnalyzer` with
`modelRetention: .processLifetime`, fed the file through `analyzeSequence(from:)` then
`finalizeAndFinish(through:)`, results collected from `transcriber.results`. Three passes in
one process: the first cold, the next two warm.

## What happened

| Check | Result |
| --- | --- |
| Framework present | yes (`#available(macOS 26.0, *)` true; `SpeechTranscriber.isAvailable` true) |
| Locales | 30 supported; 9 English variants already installed; `en-US` → `en_US` |
| Model assets | `AssetInventory.status` was `supported` (not installed); `assetInstallationRequest` + `downloadAndInstall` took **595 ms** and left it `installed`. `reservedLocales` = `[en_US]` afterwards |
| Audio format | analyzer's best format equals the file's: 16 kHz mono float |
| Transcript | `"The meeting has been moved to Thursday at 3 o'clock, so please update the shared calendar before you leave today."` — identical to the spoken text except `three` → `3` (Apple's number formatting) |
| Results | 1 final result per run, 0 volatile, with the `.transcription` preset |

Latency, 6.06 s of audio:

| Pass | prepare | first result | analyze | finalize | **total** | RTF |
| --- | --- | --- | --- | --- | --- | --- |
| cold (first in process, run 1) | 165.5 ms | 253.1 ms | 10.1 ms | 243.4 ms | **253.6 ms** | 0.042 |
| cold (run 2, assets cached) | 90.3 ms | 224.2 ms | 5.8 ms | 224.4 ms | **230.2 ms** | 0.038 |
| warm | 4.6–5.3 ms | 157–168 ms | 3.7–4.3 ms | 160–169 ms | **164–173 ms** | 0.027–0.029 |

`analyze` is the file being pushed in; the model's work happens in `finalize`. Process RSS
grew from 9 MB to 19 MB; the model does not live in our process (see S4).

## What it means for the design

- **The engine budget in [LATENCY.md](../LATENCY.md) (150–400 ms inference) holds** with
  margin, on the slowest supported Apple Silicon. Release-to-text p50 < 500 ms is achievable.
- **Model warm-up costs ~90–165 ms once per process.** With `.processLifetime` retention,
  subsequent dictations skip it. That is the only "residency" question left, and it is a
  one-line option, not a subsystem. [ADR 0010](../adr/0010-adaptive-residency.md)'s
  machinery stays out of v1.
- **First use needs a network fetch of Apple's model** even on a machine that already had
  English dictation installed. It took under a second here, but onboarding must handle
  `supported` vs `installed` and show progress via `AssetInstallationRequest.progress`.
  Add to [PERMISSIONS.md](../PERMISSIONS.md) / onboarding: no download of *ours*, but one of
  Apple's.
- **SpeechAnalyzer is Swift-only.** `objc2-speech` 0.3 wraps `SFSpeechRecognizer` only. The
  product needs a Swift bridge compiled by `build.rs` and linked statically, exactly the
  shape of the spike. Update [TECH-STACK.md](../TECH-STACK.md): drop `objc2-speech` from the
  engine path.
- Numbers, punctuation and casing are formatted by the engine. `three` became `3`. Whether
  that is wanted is a settings question for later; nothing in the post-processing chain
  should undo it silently.

## Open items

- **Accuracy on a real voice.** The fixture is synthetic and clean. Record a real 6 s
  utterance (QuickTime → `.m4a` works directly) and re-run; also `noisy-6s.wav` per
  [fixtures/audio/README.md](../../fixtures/audio/README.md).
- Streaming: the `.progressiveTranscription` preset and `volatileResults` were not
  exercised. Relevant only to M9's sub-250 ms goal.
- Behaviour on a machine where the locale assets have never been present, and offline.
