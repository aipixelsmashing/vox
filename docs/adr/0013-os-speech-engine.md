# 0013 — Use the OS speech model where one exists

**Status:** accepted

## Context

macOS 26 introduced SpeechAnalyzer, an on-device speech framework built for the Neural Engine
and available to third-party apps. Meanwhile our default path asks Mac users to download 700 MB
and carry a large resident model.

## Decision

Add SpeechAnalyzer as a third engine behind the existing `SpeechEngine` trait, and make it the
default on macOS 26+. Parakeet remains the default on Windows and Linux, where no comparable OS
model exists, and remains available on macOS for users whose language or vocabulary it handles
better.

## Consequences

- No model download on Mac at all, and idle memory in the tens of megabytes rather than
  hundreds. An app built this way ships as a ~4 MB binary.
- Removes the largest piece of first-run friction on the most common platform.
- We keep everything Apple's own dictation does not offer: hold-to-talk on any key, no session
  cutoff, transcript history, learned vocabulary, long-form sessions, and identical behaviour
  on the user's other machines.
- Accuracy and behaviour now differ by platform, which complicates support and the evaluation
  harness. The engine in use is always visible in Settings and recorded in every history row.
- A third backend to maintain, and one that Apple can change under us. Acceptable: the trait
  already exists, and the fallback is a model we ship anyway.
