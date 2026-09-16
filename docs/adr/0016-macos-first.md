# 0016 — macOS first, and one engine

**Status:** accepted

## Context

The original plan built for macOS, Windows and Linux simultaneously, with Parakeet as the
default engine everywhere and Apple SpeechAnalyzer as a platform optimisation. That is the
right shape for a product serving strangers on every platform. It is the wrong shape for
getting a working tool into the hands of a small group of Mac users quickly.

The implementation is agent-driven, which collapses the cost of *writing* code but not the cost
of *verifying* it. Verification is per-platform, manual, and the actual bottleneck: whether
insertion works in Slack is not knowable by reasoning about it, and permission prompts cannot be
granted by an agent.

## Decision

Build and release macOS only. Use Apple SpeechAnalyzer as the sole engine for v1. Windows and
Linux stay fully designed and documented, and are built after the Mac version is in daily use.

## Consequences

What leaves v1 entirely, while remaining designed for M8:

- `parakeet-rs`, `whisper-rs`, the ONNX runtime, and execution-provider selection
- The model registry, resumable downloader, SHA-256 verification, sideload path, and the
  CC-BY attribution obligations that come with redistributing weights
- Memory-mapped weights, adaptive residency and preload prediction — with no weights of ours,
  idle footprint is tens of megabytes and the whole problem dissolves
- `win-text-inject`, UIPI detection, the Windows clipboard opt-out formats
- `enigo`, `ashpd`, libei, `wtype`, `ydotool`, and the Wayland verification problem
- evdev capture and the `input` group onboarding path

What this leaves: one key-capture backend (`keytap`'s CGEventTap path, its most battle-tested),
one injection chain, one engine, one packaging target. Roughly a third of the designed system,
and the third with the least uncertainty in it.

**The gating risk:** SpeechAnalyzer requires macOS 26+. If the intended users are on older
versions, `whisper-rs` with a bundled `base.en` returns as a fallback behind the existing
`SpeechEngine` trait. That is a contained addition, not a redesign — which is the reason the
trait exists.

**The accepted cost:** cross-platform parity was a genuine differentiator against the
Mac-only field ([PRIOR-ART.md](../PRIOR-ART.md)). Deferring it means competing on the crowded
side of the market first. Accepted, because a tool people use beats a positioning nobody has
tried.
