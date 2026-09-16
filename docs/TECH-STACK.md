# Technology stack

> **v1 is macOS-only and uses one engine.** Everything below marked *deferred* is designed and
> documented but not built until Windows and Linux land — see
> [adr/0016](adr/0016-macos-first.md). Deferred: `parakeet-rs`, `whisper-rs`, the ONNX runtime,
> the model registry and downloader, memory-mapped residency, `win-text-inject`, `enigo`,
> `ashpd`, and the Wayland fallback chain. What v1 actually needs: Tauri, `keytap` (CGEventTap
> backend), `cpal`, `rubato`, a VAD, `axuielement`, `core-graphics`, `rusqlite`, `arboard`, and
> Apple's SpeechAnalyzer.

Every dependency here was chosen against a specific requirement. Where a library was rejected
the reason is recorded, because "why not X" comes up in every contributor conversation.

Versions are the ones current at the time of writing (September 2026). Pin exact versions in
`Cargo.toml`; treat this document as the rationale, not the lockfile.

## Application shell

**Tauri 2** — Rust core with a system webview for UI.

Chosen over Electron because the dictation path must live in a compiled language with real
threads and direct FFI to platform accessibility APIs, and because a tray utility that idles
all day should not carry a 150 MB Chromium. Chosen over a fully native app per platform
(SwiftUI + WinUI + GTK) because that triples the UI work for a product whose UI is a menu and
two panels; the platform-specific code that genuinely needs to be native is the ~800 lines in
`inject/` and `hotkey.rs`, and those are native regardless.

The cost, stated plainly: WebKitGTK on Linux is the weakest of the three webviews, and Tauri's
Linux story generally needs more testing effort than the other two platforms.

Plugins used: `tauri-plugin-single-instance`, `tauri-plugin-autostart`,
`tauri-plugin-updater`, `tauri-plugin-opener`, `tauri-plugin-notification`.

## Push-to-talk key capture

**`keytap`** (`crates.io/crates/keytap`) — observe-only global keyboard taps.

This is the crate that makes the stated default hotkey possible. The requirement is "hold the
*right* Option key", which needs three things most hotkey libraries don't provide: raw
key-down *and* key-up events, left/right modifier distinction, and modifier-only bindings.

- `global-hotkey` (the Tauri ecosystem's own) registers named shortcuts with the OS. It gives
  no raw event stream and no left/right distinction, so "hold right Alt" is not expressible.
- `rdev` gives raw events but collapses modifiers on some paths, has a known crash on
  macOS 14+ when called from background threads, and has no clean shutdown.
- `keytap` was built specifically to fix those trade-offs: `Key::AltRight` as a distinct
  value, `EventKind::KeyDown`/`KeyUp`, a `ChordMatcher` with momentary and toggle modes,
  clean shutdown on drop, and a typed error when the OS denies permission rather than silently
  producing no events. Backends: CGEventTap (macOS), `WH_KEYBOARD_LL` (Windows), evdev (Linux,
  works under Wayland).

Note its deliberate limitation: it observes, it does not *consume*. Consequences are discussed
in [HOTKEYS.md](HOTKEYS.md). Its Linux and Windows backends are less battle-tested than its
macOS backend, so [TESTING.md](TESTING.md) treats hotkey capture as a per-platform manual gate,
and [ROADMAP.md](../ROADMAP.md) M0 exists to validate it before anything else is built.

## Audio capture

**`cpal`** for input streams on all three platforms (CoreAudio / WASAPI / ALSA + PipeWire via
ALSA compat). **`rubato`** for resampling to the 16 kHz mono the models expect. A lock-free
`ringbuf` between the audio callback and the pipeline, because allocating or locking in a
real-time audio callback is how you get dropouts.

**`voice_activity_detector`** (Silero VAD v5 via ONNX) to trim silence and reject
speechless recordings. Fixed window sizes — 512 samples at 16 kHz — which is fine since that's
our native rate. If its ONNX Runtime dependency proves awkward to co-load alongside the ASR
session, `fast-vad` is a pure-DSP fallback that is much faster and adequate for
trim-and-reject (we are not doing endpointing).

## Speech recognition

Two engines behind one trait. See [MODELS.md](MODELS.md) for the model registry and licences.

**Default: `parakeet-rs`** — NVIDIA Parakeet TDT via ONNX Runtime (`ort`).

Parakeet TDT 0.6B v3 is the current sweet spot for dictation: <cite index="16-1">it posts 6.34% word error rate on the Hugging Face Open ASR Leaderboard against Whisper large-v3's 7.44%</cite>, at roughly a
third of the parameters, and its token-and-duration transducer skips audio frames it predicts
carry no new tokens — which is exactly why it is fast on a CPU. It covers 25 European
languages with automatic language detection, and emits punctuation and capitalisation without
a second model. <cite index="70-1">NVIDIA releases it under CC-BY-4.0</cite>, so we can redistribute
with attribution.

`parakeet-rs` also exposes NVIDIA's Nemotron streaming models through the same loader, which
is the intended path to partial-results dictation in v1.1. Note its own guidance that CoreML
is unstable for this model on Apple platforms — use the WebGPU execution provider or plain CPU.

**Fallback: `whisper-rs`** — whisper.cpp bindings, GGML models, Metal/CUDA/Vulkan acceleration.

Whisper stays in the product for the ~74 languages Parakeet doesn't cover, for very low-RAM
machines (`base.en` quantised is a few hundred megabytes), and as an escape hatch when a
Parakeet build misbehaves on someone's hardware. MIT licensed.

Rejected: `sherpa-rs` / sherpa-onnx — capable and broad, but pulls a large C++ toolchain and a
model-format ecosystem we'd have to explain to users, for capabilities (diarization, TTS,
keyword spotting) this product doesn't want. Rejected: any Python-backed runtime — shipping a
Python environment inside a tray utility is a packaging and startup-time tax we won't pay.

## Text injection

Per platform, and this is where the real engineering is. Full detail in
[TEXT-INJECTION.md](TEXT-INJECTION.md).

- **macOS** — `axuielement` (safe Rust bindings to Apple's Accessibility API) for direct
  insertion via `kAXSelectedTextAttribute`, with `core-graphics` for the synthesised-paste and
  Unicode-event fallbacks.
- **Windows** — **`win-text-inject`**, a crate that exists precisely because the naive
  approach is broken. Its documentation is worth reading in full; the summary is that
  clipboard-and-Ctrl+V leaks transcripts into Windows clipboard history and the cloud
  clipboard unless you attach four specific opt-out formats, that a held modifier corrupts the
  synthesised chord (guaranteed to happen in push-to-talk, where a modifier *is* the trigger),
  that injection into elevated windows fails silently under UIPI, and that restoring the
  previous clipboard on a fixed timer races the target's asynchronous read — the defect behind
  a long-running open issue in a popular dictation app, where the shipped mitigation is a delay
  slider that users report still failing. It solves the race with delayed rendering rather than
  a guessed sleep.
- **Linux** — `enigo` for X11 XTEST; on Wayland, libei via the RemoteDesktop portal where
  available (GNOME ≥ 46, KDE Plasma ≥ 6.1), then `wtype`, then `ydotool`, then clipboard-only.
  See ADR 0005 for why we don't pretend Wayland injection succeeded when we can't confirm it.

**`arboard`** for cross-platform clipboard reads/writes, with platform-specific code layered
on top for the private/concealed clipboard formats.

## Storage

**`rusqlite`** with the `bundled` feature for the transcript history. SQLite rather than a
JSON file because history needs search, ordered pagination, and per-row delete without
rewriting the whole file. Bundled rather than system so the packaged app has no external
dependency. **`serde` + `serde_json`** for settings; a plain file the user can read, diff, and
version-control is the right call for a privacy tool.

## Updates and packaging

`tauri-plugin-updater` with minisign-signed manifests, artifacts on GitHub Releases, static
`latest.json`. macOS: Developer ID + notarisation, via `notarytool` in CI. Windows:
Authenticode, with SignPath Foundation's free OSS certificate programme as the intended route.
Linux: AppImage, `.deb`, `.rpm`, then Flathub and AUR once the Wayland story settles. Details
in [PACKAGING.md](PACKAGING.md).

## Frontend

React 19 + TypeScript + Vite + Tailwind 4. Chosen for contributor familiarity rather than any
technical need — the UI is a list, a form, and a menu. Svelte would produce a smaller bundle
and is a reasonable fork-point; the decision is recorded in ADR 0002 so it can be revisited
cheaply, since nothing in the core depends on it.

## Development tooling

`cargo-deny` (licence and advisory gate in CI), `cargo-audit`, `clippy` with `-D warnings`,
`rustfmt`, `criterion` for the latency benchmarks, `insta` for snapshot tests on text
post-processing, `vitest` for the small amount of frontend logic, `typos` and `lychee` for
docs hygiene.
