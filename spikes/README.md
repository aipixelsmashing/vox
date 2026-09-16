# M0 spike harnesses

Throwaway binaries, one per spike in [ROADMAP.md](../ROADMAP.md#m0--spikes). They live
outside the main crate on purpose: nothing here is product code, and nothing here should
be imported by `src-tauri/`. Findings are written up by a human in
[docs/spikes/](../docs/spikes/README.md).

All of them need the pinned toolchain from the repo's `rust-toolchain.toml` (rustup picks it
up automatically) and are run from this directory:

```bash
cd spikes && cargo run --release -p <spike> -- [args]
```

| Spike | Binary | Needs | What to paste back |
| --- | --- | --- | --- |
| S1 hotkey | `s1-hotkey [seconds]` | Input Monitoring for the terminal you run it from | the whole output, especially the summary block |

## Permissions

macOS keys Input Monitoring and Accessibility grants to the *responsible process*. For a
binary launched from a terminal that is the terminal app (Terminal, iTerm2, or the Claude
desktop app), not the binary. Grant it once to the terminal and every spike binary run from
that terminal inherits it, debug or release, across rebuilds.

System Settings → Privacy & Security → Input Monitoring (S1) / Accessibility (S2).
