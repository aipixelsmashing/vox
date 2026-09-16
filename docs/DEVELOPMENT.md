# Development setup

## Prerequisites

- Rust stable (see `rust-toolchain.toml`), plus the platform targets you intend to build
- Node 20+ and pnpm
- Tauri v2 system dependencies — follow the Tauri prerequisites guide for your OS
- macOS: Xcode command line tools
- Windows: MSVC build tools, WebView2 runtime
- Linux: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `libasound2-dev`,
  `libudev-dev`, plus `xdotool` (X11) or `wl-clipboard` (Wayland) for the injection fallbacks

## First run

```bash
pnpm install
pnpm tauri dev
```

The first launch downloads no model. Use Settings → Model, or drop a model directory into
`<app-data>/models/` and import it, to avoid re-downloading during development.

## Platform notes that will otherwise waste your afternoon

**macOS.** Accessibility and Input Monitoring grants are keyed to the code signature, and a
dev build's signature changes on every rebuild. You will re-grant permission repeatedly. Two
mitigations: use an ad-hoc stable signing identity for local dev, and keep the System Settings
pane open. After granting, the app must be restarted — `pnpm tauri dev` restarts do not always
count, so quit fully.

**Windows.** Debuggers and the low-level keyboard hook interact badly: if the hook callback is
paused at a breakpoint, Windows silently unhooks it after the timeout and your hotkey stops
working until restart. Log around the hook instead of breaking inside it.

**Linux.** You need `input` group membership for evdev key capture — log out and back in after
adding it. Under Wayland, run with `WAYLAND_DEBUG=1` when the injection path misbehaves, and
check what your compositor exposes with `wayland-info | grep keyboard`.

## Layout

See [ARCHITECTURE.md](ARCHITECTURE.md#modules). In short: `src-tauri/src/` is the core,
`src/` is the small React UI, `docs/` is the specification.

## Working on the dictation path

`pnpm tauri dev -- --features mock-audio` replaces the microphone with WAV fixtures from
`fixtures/audio/`, so you can iterate on the pipeline without talking to your laptop all day.
`--features mock-inject` logs what would have been injected instead of touching other apps.

## Useful commands

```bash
cargo test --workspace
cargo bench --bench pipeline
cargo clippy --all-targets -- -D warnings
cargo deny check
pnpm test
pnpm tauri build --debug
```
