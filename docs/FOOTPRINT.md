# Footprint

## The number that matters

**Idle resident memory** decides whether a tray app stays installed. It is what people see in
Activity Monitor or Task Manager when they go looking for what is eating their laptop, and it
is the one claim in this project a sceptic can verify in ten seconds without trusting anyone.
"Audio never leaves your machine" needs a network monitor and some faith. "Uses less than a
browser tab" is right there.

| Measure | Target | Hard fail |
| --- | --- | --- |
| **Idle RSS** (not dictating) | < 120 MB | 200 MB |
| Peak RSS during a 6 s dictation | < 900 MB | 1.4 GB |
| Peak RSS during a 30-minute long-form session | < 950 MB | 1.4 GB |
| Idle CPU | < 0.3% | 0.5% |
| Idle energy impact (macOS) | "Low", always | — |
| Install size (app, no model) | < 20 MB | 30 MB |

Note the gap between idle and peak. That gap is the whole design — for a model we load
ourselves. For v1 it is not our model, and the numbers below are what was measured.

## Measured, v1 (macOS 26.5, M1 Pro, [S4](spikes/s4-footprint.md))

| Measure | Our process | Apple's `localspeechrecognition` XPC service |
| --- | --- | --- |
| Idle, engine retained | **19 MB RSS**, 5.5 MB physical footprint | ~68 MB |
| Peak during a 6 s dictation | 19 MB | ~86 MB |
| After our process exits | — | exits |

The service is charged to Apple's process in Activity Monitor, not ours, and it exits when we
do. Both targets above are met by an order of magnitude. The tray app's real idle number will
be higher because Tauri's webview adds its own baseline; that is a Tauri number and M3
measures it.

## The conflict, stated honestly

"Instant" and "small" pull in opposite directions. A warm Parakeet int8 session is roughly
750 MB resident. Keeping it warm all day buys ~0.5–2 s on the first dictation after a gap and
costs 750 MB for the 99% of the day nobody is speaking.

The original design picked "always warm" and exposed the trade-off as a setting. That was
wrong twice: it picked the worse default, and it asked users to make an engineering judgement
they have no basis for. Settings are where product decisions go to die.

## Three mechanisms instead of a slider

Mechanisms 1 and 2 exist for a model we load ourselves. **They are not built in v1**, where
there is no such model ([adr/0016](adr/0016-macos-first.md)); they return with Parakeet in
M8. Mechanism 3 is v1.

### 1. Adaptive residency

Unload after ~10 minutes idle. Reload **predictively**, using signals already in hand:

- The user focused an editable text field.
- In an application they have dictated into before.
- At a time of day they usually dictate.
- The hotkey's first key of a chord went down.

The model loads while their hand is still moving toward the key. When prediction misses, the
cost is a warm reload (below), not a cold one.

Prediction quality is measured locally: hit rate, and the p95 wait when it misses. If hit rate
falls below 80% in real use the heuristic is wrong and gets fixed — it does not get a setting.

### 2. Memory-mapped weights

Load weights with `mmap`, zero-copy, backed by the page cache.

- Reported RSS drops sharply, because the pages are file-backed rather than anonymous.
- The OS evicts them under memory pressure instead of the user blaming us for a hard 750 MB.
- The **second** load costs a few hundred milliseconds rather than the 0.5–2 s cold figure,
  because the pages are usually still cached.

This alone changes the economics of unloading: with mmap, unloading is nearly free to reverse,
which is what makes adaptive residency viable rather than annoying.

### 3. The OS model where one exists

On macOS 26+, Apple's SpeechAnalyzer runs on the Neural Engine with no weights of ours in
memory at all. Measured: 19 MB in our process, ~68 MB in Apple's service while retained
([S4](spikes/s4-footprint.md)). That is the footprint problem solved outright on Mac, and it
removes the 700 MB first-run download at the same time (Apple fetches its own locale assets
on first use, under a second on a machine that already had dictation installed).

The one residency decision left is a single enum: `SpeechAnalyzer.Options.modelRetention`.
`.processLifetime` keeps Apple's service warm (~68 MB, not ours) and makes every dictation
after the first cost ~165 ms; `.whileInUse` frees it and pays ~90–165 ms of warm-up each time
([S3](spikes/s3-engine.md)). v1 uses `.processLifetime`. Still no setting.

Parakeet remains the answer on Windows and Linux, where no comparable OS model exists. See
[adr/0013](adr/0013-os-speech-engine.md).

## The uncomfortable implication

If adaptive residency and mmap work as well as they should, the right default engine on each
platform becomes **the smallest model that clears the accuracy bar**, not the most accurate one
that fits in RAM. Learned vocabulary ([LEARNING.md](LEARNING.md)) pushes the same way: a
smaller model plus the user's own terms may beat a larger generic one on the words that
actually matter to them.

That is an assumption, not a finding. Spike S4 tests it before the architecture is built around
Parakeet.

## Enforcement

Footprint regressions fail a PR exactly like latency regressions. `cargo bench --bench
footprint` records idle RSS, peak RSS and idle CPU over a scripted five-minute run with three
dictations, and CI compares against `benches/baseline.json`. A 15% regression in idle RSS is a
failure, not a note in the changelog. On macOS the bench must sample
`localspeechrecognition.xpc` alongside our own process while it exists; sampling only our RSS
would report 19 MB and miss the engine entirely.
