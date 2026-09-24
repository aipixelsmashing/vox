# Latency budget

"Instant" is a requirement, so it gets a number, a breakdown, and a test.

## Measure time to *usable* text

A 400 ms transcription that needs two corrections is slower than a 700 ms one that needs none.
The user's clock does not stop when text appears; it stops when the text is right.

So the primary measure is **time to usable text**: release-to-text, plus the time spent
correcting what arrived. Corrections are counted by the same watch that feeds vocabulary
learning ([LEARNING.md](LEARNING.md)), which means this is measurable locally without asking
the user anything.

This reframe reorders the roadmap. Work that removes corrections — learned vocabulary, context
biasing, a better model on the words this user actually says — beats work that shaves 50 ms off
inference, even though the second one is easier to benchmark.

## The number

**Release-to-text**: the interval from the hotkey being physically released to the transcript
being visible in the target field. The floor under time-to-usable-text, not a substitute for it.

| Percentile | Target | Hard fail |
| --- | --- | --- |
| p50 | < 500 ms | 800 ms |
| p95 | < 1000 ms | 1500 ms |

Measured with a 6 second utterance, warm model, on reference hardware, with the default engine
and the direct-insertion path.

Reference hardware (CI benchmarks run on the first two; the third is a manual gate):

- **A** Apple M2, 16 GB
- **B** Ryzen 7 / 16 GB, CPU-only inference
- **C** 2019 Intel i5 laptop, 8 GB — the "slow machine" gate, allowed 2× the targets

## Breakdown

| Stage | Budget (p50, machine A) | Notes |
| --- | --- | --- |
| Key-up observed | < 5 ms | `keytap` event delivery. Measured 2.1 ms median, 6 ms p95 HID-to-consumer ([S1](spikes/s1-hotkey.md)) |
| Stop stream, drain ring buffer | 10–20 ms | Bounded by one audio callback period |
| Resample + normalise + VAD trim | < 15 ms | 6 s of audio, `rubato` + Silero |
| **Inference** | **150–400 ms** | v1: Apple SpeechAnalyzer, measured **165 ms warm, 250 ms cold** for 6 s on an M1 Pro ([S3](spikes/s3-engine.md)). M8: Parakeet TDT int8 |
| Vocabulary substitution | < 2 ms | Learned + manual terms, whole-token match |
| Post-processing (dictionary, spacing) | < 5 ms | |
| Target revalidation | < 10 ms | AX / window query |
| Insertion — AX direct | 5–20 ms | Measured 2–15 ms ([S2](spikes/s2-injection.md)) |
| Insertion — clipboard paste | 30–80 ms | Includes waiting for read confirmation. Measured 37–81 ms to read-back evidence |
| History write | < 5 ms, off the critical path | Fire-and-forget after insertion |

Everything except inference is under 150 ms combined. If a change makes the non-inference path
exceed 150 ms, that is a regression regardless of the total.

## Key-down side

Less visible but equally important: if capture starts late, the first syllable is lost, and
users experience that as inaccuracy rather than latency.

| Stage | Budget |
| --- | --- |
| Key-down observed → capture begins | < 30 ms |
| Audio device start (cold stream) | 20–80 ms, platform-dependent |

Two mitigations, one shipped and one optional:

- **Shipped**: the model is already warm, so the pipeline has nothing else to do at key-down.
- **Optional pre-roll**: keep the input stream open and continuously overwrite a 300 ms ring
  buffer, discarding it unless the hotkey fires. This recovers the device-start cost and the
  user's first syllable entirely. It is **off by default** because it means the microphone
  stream is open whenever the app is running, which is a privacy posture some users will not
  accept even though nothing is retained. When enabled, the tray icon shows a distinct
  always-listening state — no hidden mic access, ever.

## Warm start without staying warm

**v1:** the model is Apple's and lives in Apple's process. Warm-up is 90–165 ms once per
process with `modelRetention: .processLifetime`, after which every dictation is warm
([S3](spikes/s3-engine.md), [S4](spikes/s4-footprint.md)). Nothing below applies until a
model of our own returns in M8.

Cold model load is 0.5–2 s. Keeping the model resident all day removes that cost and spends
~750 MB for the 99% of the day nobody is speaking — a trade nobody should be asked to
configure.

Resolved by adaptive residency and memory-mapped weights, specified in
[FOOTPRINT.md](FOOTPRINT.md): unload after ~10 minutes idle, reload predictively on signals
already available (an editable field gained focus, in an app the user dictates into, at a time
they usually dictate). With mmap the reload is a few hundred milliseconds rather than seconds,
because the pages are still in the page cache.

There is **no residency setting**. Prediction hit rate and p95 miss-wait are measured locally;
if the heuristic is wrong it gets fixed, not exposed.

## Perceived latency

Absolute numbers are not the whole story.

- **State changes must be immediate.** The tray icon changes on key-down within one frame,
  regardless of what the pipeline is doing. Feedback that lags makes a fast system feel slow.
- **Show a transcribing state.** Between key-up and insertion the indicator changes again, so
  the gap reads as progress rather than as a hang.
- **Never block the UI thread.** Inference and injection are both off the main thread; the
  tray must stay responsive during a 15-second transcription.
- **Streaming (v1.1)** is the real answer for short-form: transcribing during the hold reduces
  release-to-text to the tail chunk plus injection, targeting p50 under 250 ms.
- **Long-form has a different clock entirely.** Text appearing within a couple of seconds of
  being spoken is what matters there, not release-to-text — see [LONG-FORM.md](LONG-FORM.md).

## Measurement

`cargo bench --bench pipeline` uses `criterion` with a fixed set of WAV fixtures and a mock
injector, producing per-stage timings. Run on every PR that touches `audio`, `engine`,
`pipeline` or `inject`; CI fails the PR if p50 regresses more than 15% against the baseline
committed in `benches/baseline.json`.

End-to-end timing including the real injector cannot be automated reliably — it depends on the
target application. Instead, every real dictation records its own per-stage timings into the
history row, and Settings → Diagnostics shows a local histogram of the last 100 dictations.
This is diagnostic data that never leaves the machine.

The same panel shows corrections per 100 words over time, which is the number that actually
tells the user whether the tool is getting better at *their* vocabulary.
