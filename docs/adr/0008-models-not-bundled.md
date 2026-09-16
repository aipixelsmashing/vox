# 0008 — Models downloaded, not bundled

**Status:** accepted

## Context

The default model is ~700 MB. Bundling it makes first use instant and works offline
immediately; downloading keeps the installer small and lets models update independently.

## Decision

Download by default, with two escape hatches: an installer variant bundling Whisper `base.en`
(~60 MB) for offline machines, and a sideload path for air-gapped installs.

## Consequences

- A ~15 MB installer for a tray utility, which is what people expect.
- First run requires a download, so onboarding has to make that step feel deliberate rather
  than like a bait-and-switch: recommended model, size shown up front, resumable, verified.
- Model updates ship independently of app updates.
- Download and verification code is a permanent piece of the product, including SHA-256
  verification before load — a model file is data fed to a native runtime, so verification is
  a security control, not a nicety.
