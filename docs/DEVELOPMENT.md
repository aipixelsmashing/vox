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
pnpm tauri build --bundles app
cp -R src-tauri/target/release/bundle/macos/Vox.app /Applications/
open /Applications/Vox.app
```

`pnpm tauri dev` also works, but a tray app that needs three TCC grants keyed to its
signature is easier to live with as a real bundle in /Applications: grant once, relaunch,
done. The first launch shows the three permission prompts and opens System Settings on the
first missing one; grant all three, quit Vox from the tray, and open it again.

The first launch downloads no model of ours. Apple's speech model for your locale is
installed by the OS on first use if it is not already present (under a second on a machine
that has ever used dictation).

The engine bridge needs `swiftc` (Xcode Command Line Tools are enough; Xcode.app is not
required). The pinned Rust toolchain in `rust-toolchain.toml` is installed by rustup on the
first build.

## Platform notes that will otherwise waste your afternoon

**macOS.** Accessibility and Input Monitoring grants are keyed to the code signature, and a
dev build's signature changes on every rebuild. You will re-grant permission repeatedly. Two
mitigations: use an ad-hoc stable signing identity for local dev, and keep the System Settings
pane open. After granting, the app must be restarted — `pnpm tauri dev` restarts do not always
count, so quit fully. When re-granting, remove the stale Vox row from the pane with `−` and
add the new bundle with `+`; toggling the old row off and on is not enough.

The system-wide accessibility element does not work on macOS 26.5 even when trusted; the
injector goes through the frontmost application's element instead
([TEXT-INJECTION.md](TEXT-INJECTION.md#macos)). If insertion reports "no text field was
focused" everywhere, check that the app you are dictating into is actually frontmost.

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
