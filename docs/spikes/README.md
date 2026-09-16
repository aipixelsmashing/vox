# Spikes

Milestone M0 was four questions that could each kill the project. Each spike was throwaway
code plus a written finding in this directory. All four were answered on 2026-09-15 on macOS
26.5.2 / M1 Pro; the harnesses are in [`spikes/`](../../spikes/README.md) and still run.

| Spike | Question | Answer | Finding |
| --- | --- | --- | --- |
| S1 | Does `keytap` deliver right-Option down/up from an unfocused app, and how fast? | Yes; ~2 ms median | [s1-hotkey.md](s1-hotkey.md) |
| S2 | Does accessibility insertion work, and can the field be read back afterwards? | Yes where `AXSelectedText` is settable; read-back works everywhere tested; three apps done, four for M2 | [s2-injection.md](s2-injection.md) |
| S3 | Is Apple SpeechAnalyzer available and fast enough, and which macOS are the users on? | Yes; 165 ms warm for 6 s; users are on 26+, so it is the only v1 engine | [s3-engine.md](s3-engine.md) |
| S4 | Idle RSS with SpeechAnalyzer resident? | 19 MB in-process; ~68 MB in Apple's service | [s4-footprint.md](s4-footprint.md) |

The six design changes the findings required are folded into
[TEXT-INJECTION.md](../TEXT-INJECTION.md), [TECH-STACK.md](../TECH-STACK.md),
[FOOTPRINT.md](../FOOTPRINT.md), [PERMISSIONS.md](../PERMISSIONS.md),
[LATENCY.md](../LATENCY.md), [ARCHITECTURE.md](../ARCHITECTURE.md) and
[HOTKEYS.md](../HOTKEYS.md).

A finding is one page: what was tried, what happened, numbers, and what it means for the
design. A spike that concludes "it works" without numbers has not concluded anything.
