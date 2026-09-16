# 0001 — Tauri over Electron and over per-platform native

**Status:** accepted

## Context

The dictation path needs real threads, direct FFI to platform accessibility APIs, and a
resident ~700 MB inference session. The UI is a tray menu, a list, and a settings form. Three
options: Electron, Tauri, or three native apps.

## Decision

Tauri v2. Rust core, system webview for the two windows.

## Consequences

- The critical path is compiled Rust with no IPC, which is what the latency budget requires.
- Idle footprint suits a tray utility that runs all day; Electron's baseline does not.
- The genuinely platform-specific code (`inject/`, `hotkey.rs`, permission checks) is native
  regardless of this choice — roughly 800–1200 lines. Choosing native everywhere would triple
  the UI work to avoid nothing.
- Accepted cost: WebKitGTK on Linux is the weakest of the three webviews and needs
  disproportionate testing effort.
- Accepted cost: a smaller contributor pool than Electron's.
