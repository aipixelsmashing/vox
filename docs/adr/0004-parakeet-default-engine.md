# 0004 — Parakeet TDT as the default engine, Whisper as fallback

**Status:** accepted

## Context

Dictation-length utterances on consumer hardware, mostly CPU-only, with a hard latency budget.
Candidates: Whisper family via whisper.cpp, Parakeet TDT via ONNX, Nemotron streaming models.

## Decision

Parakeet TDT 0.6B v3 (int8 ONNX, via `parakeet-rs`) is the default. Whisper via `whisper-rs`
ships alongside it behind the same trait.

## Consequences

- Better English accuracy than Whisper large-v3 on the public leaderboard at roughly a third
  of the parameters, and substantially faster on CPU because the token-and-duration transducer
  skips frames — which is what the latency budget needs.
- Punctuation and capitalisation come from the model, so no second model in the path.
- CC-BY-4.0 permits redistribution with attribution; `NOTICE` carries it.
- Parakeet v3 covers 25 languages against Whisper's ~99, so Whisper is not optional — it is
  the only path for a large share of the world's users. Two engines is a permanent maintenance
  cost, accepted deliberately.
- `parakeet-rs` also loads NVIDIA's Nemotron streaming models through the same API, which is
  the intended route to streaming dictation without another dependency.
- Watch for third-party repackages: at least one popular mirror is CC-BY-**NC**, which we
  cannot ship. Registry entries record the licence per source.
