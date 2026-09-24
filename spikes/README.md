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
| S4 footprint | `s4-footprint [--interval-ms 100] [-- <command>...]` | nothing (wraps s3-engine by default) | the whole output |
| S5 context | `s5-context [--context "a,b,c"] [AUDIO]` | nothing | the whole output; also what the audio says |
| S3 engine | `s3-engine [--locale en-US] [--runs 3] [--download] [AUDIO]` | network once, for the model assets | the whole output; also say what the audio actually said |
| S2 inject | `s2-inject [--delay N] [--method auto\|ax\|paste\|both] [--no-restore] [TEXT]` | Accessibility for the terminal | the whole output per app, plus what you saw appear in the field |

## Permissions

macOS keys Input Monitoring and Accessibility grants to the *responsible process*. For a
binary launched from a terminal that is the terminal app (Terminal, iTerm2, or the Claude
desktop app), not the binary. Grant it once to the terminal and every spike binary run from
that terminal inherits it, debug or release, across rebuilds.

System Settings → Privacy & Security → Input Monitoring (S1) / Accessibility (S2).

## S3 notes

- SpeechAnalyzer is Swift-only. `s3-engine/build.rs` compiles `bridge/S3Bridge.swift` with
  `swiftc` into a static library and links it; the product will need the same shape because
  `objc2-speech` only wraps the old `SFSpeechRecognizer` API.
- The default fixture is `fixtures/audio/clean-6s.wav`, generated with `say` (synthetic
  voice, 6.06 s). For an accuracy judgement record yourself in QuickTime (File → New Audio
  Recording, ~6 s, save as .m4a) and pass that path; AVAudioFile reads it directly.
- The first run needs `--download` to fetch the on-device model for the locale.

## S2 notes

- After launch it counts down (default 5 s) so you can click into the target field first.
- `--method both` runs the accessibility path and then the paste path into the same field,
  which is one row of the compatibility matrix per run.
- The paste path restores your previous clipboard string only when the insertion was
  verified; otherwise the transcript is left on the clipboard, per docs/TEXT-INJECTION.md.
- Building `axuielement` with only Command Line Tools needs the link-path workaround in
  `s2-inject/build.rs`.
