# Audio fixtures

WAV files used by the integration tests and by `--features mock-audio`. Not committed (see
`.gitignore`) — generate or record them locally, then place them here:

| File | Contents |
| --- | --- |
| `clean-6s.wav` | Clear dictation, ~6 s, the benchmark reference |
| `noisy-6s.wav` | Same phrase with background noise |
| `silence-3s.wav` | Silence only — must produce no transcript and no history entry |
| `tap-200ms.wav` | Accidental key tap |
| `long-130s.wav` | Exceeds the default recording cap |
| `non-english-6s.wav` | Language auto-detection check |

All at 16 kHz mono. `hound` is available for generating them programmatically.
