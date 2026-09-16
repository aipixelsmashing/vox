# S4 — Footprint: idle and peak memory with SpeechAnalyzer resident

**Answer: trivial, as expected.** Our process idles at **19 MB RSS** (5.5 MB physical
footprint) with the engine retained, and peaks at the same 19 MB during transcription. The
model runs in Apple's `localspeechrecognition` XPC service, which peaked at **86 MB** and
held **~68 MB** while our process idled with `modelRetention: .processLifetime`, then
exited when our process did.

Run 2026-09-15 on macOS 26.5.2 (25F84), Apple M1 Pro, 16 GB, release build. Harness:
[`spikes/s4-footprint`](../../spikes/s4-footprint/src/main.rs), which spawns
`s3-engine --runs 3 --idle-seconds 10`, samples it every 100 ms with
`proc_pid_rusage(RUSAGE_INFO_V4)`, and scans every process on the system every 500 ms to
find where memory actually went. Two runs; numbers below from the second, first within 2 MB.

## What happened

| Measure | Value |
| --- | --- |
| Child RSS before the bridge is touched | 8.9 MB |
| Child RSS after framework load + 3 transcriptions | 19.0 MB |
| Child RSS during 10 s idle, engine retained | 19.0 MB (median over 44 samples; flat) |
| Child physical footprint (what Activity Monitor's *Memory* column shows) | 5.5–5.6 MB throughout |
| Child peak RSS | 19.0 MB |
| `localspeechrecognition.xpc` peak growth | +86 MB (run 3), +74 MB, +77 MB (earlier runs) |
| `localspeechrecognition.xpc` during our idle tail | 67.8 MB at the last scan before child exit |
| `localspeechrecognition.xpc` after child exit | exited |
| System-wide resident total, before → after | 12.6 GB → 12.55 GB (noise) |

The footprint timeline is flat from ~600 ms after spawn to the end: there is no load/unload
cycle to observe inside our process at all.

## What it means for the design

- **[FOOTPRINT.md](../FOOTPRINT.md) targets are met by an order of magnitude.** Idle target
  < 120 MB: we are at 19 MB in-process, ~87 MB counting Apple's service. Peak target
  < 900 MB: 19 MB in-process, ~105 MB counting the service. The doc's 750 MB resident
  weights, mmap, page-cache reload and predictive residency ([ADR 0010](../adr/0010-adaptive-residency.md))
  do not apply to v1. Leave them documented for M8.
- **Retention is Apple's problem, not ours.** With `.processLifetime` the XPC service stays
  up and warm (~68 MB, charged to Apple's process) for as long as we run. Whether to prefer
  `.whileInUse` (frees the ~68 MB, pays the ~90–165 ms warm-up per dictation, see S3) is the
  only residency decision left, and it is one enum value. Recommend `.processLifetime` for
  a push-to-talk tool that is used many times a day; revisit if users report the service
  in Activity Monitor.
- **Measure both processes in the footprint bench.** `benches/footprint` as designed samples
  our RSS only, which would report 19 MB and miss the service. It should also track
  `localspeechrecognition` while it exists.
- Tray-app idle in the real product will be higher than 19 MB because Tauri/WebKit add
  their own baseline; that is a Tauri number, not an engine number, and is what M3 should
  measure.

## Open items

- Behaviour under memory pressure: does the OS jetsam the XPC service, and what does the
  next dictation cost then? Not tested.
- Long-form (M6): does the service's footprint grow with a 30-minute session? Not tested.
