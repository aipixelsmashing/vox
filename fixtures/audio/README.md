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
| `context-6s.wav` | The S5 sentence with five invented terms, synthesised; the biasing reference |
| `real-*.m4a` / `real-*.wav` | Recordings of a real voice, below. Never committed |

All at 16 kHz mono. `hound` is available for generating them programmatically.

## Real-voice recordings

Every fixture above is `say -v Samantha`, so every accuracy number in
[docs/spikes/](../../docs/spikes/) is for a synthetic voice. Two findings are owed on a
human one before M3 ships: S3's accuracy on a real voice, and S5's comparison of the two
engine modules (ADR 0017: `DictationTranscriber` with hints against `SpeechTranscriber`
without) plus how often a hint puts in a word that was not said. Four short recordings
answer all of it.

**How to record.** QuickTime Player → File → New Audio Recording. In the menu next to the
record button pick the microphone you actually dictate with. Record one sentence, stop,
File → Save, name it as below, anywhere (the Desktop is fine). Speak as you would to
Vox: normal pace, ordinary room, no headset unless that is what you use. The spikes read
`.m4a` directly; convert only if you want the file as a test fixture:

```bash
afconvert -f WAVE -d LEI16@16000 -c 1 ~/Desktop/real-s3.m4a fixtures/audio/real-s3.wav
```

**What to say.** Read each sentence once. Do not rehearse; a first take is the realistic one.

1. `real-s3.m4a` — the S3 reference sentence, so the number is comparable:
   *"The meeting has been moved to Thursday at three o'clock, so please update the shared
   calendar before you leave today."*
2. `real-s3-noisy.m4a` — the same sentence with something on in the room: a fan, a kettle,
   or music at the volume you would tolerate on a call.
3. `real-s5.m4a` — the S5 sentence, whose five terms are invented so nothing in the
   model already knows them:
   *"Please ask Orsolya Csernák about the Kubestrix migration, and check that keytap and
   axuielement still build in the Tauri app."*
4. `real-mine.m4a` — one sentence of your own with three to five names you actually
   dictate: people, projects, products. Write the sentence down first so the transcript
   can be scored against it.

**How to run.** The harnesses are built once and read any audio file:

```bash
cd spikes
cargo run --release -p s3-engine -- --runs 3 ~/Desktop/real-s3.m4a
cargo run --release -p s3-engine -- --runs 3 ~/Desktop/real-s3-noisy.m4a
cargo run --release -p s5-context -- --context "Orsolya Csernák,Kubestrix,keytap,axuielement,Tauri" ~/Desktop/real-s5.m4a
cargo run --release -p s5-context -- --context "<your three to five names, comma-separated>" ~/Desktop/real-mine.m4a
```

Then the run that measures the risk the 20-term cap exists for: the same personal
recording with twenty hints you did **not** say. Take them from a real document you might
dictate into, the names and terms nearest the cursor, none of them in the sentence:

```bash
cargo run --release -p s5-context -- --context "<twenty terms not in the sentence>" ~/Desktop/real-mine.m4a
```

Any hinted term that appears in the `dt+ctx` transcript is a word the recogniser put in
your mouth. Zero is the hope; one in five runs is the level at which the cap comes down.

**What to paste back.** The whole output of each run, and for 4 the sentence as written.
The S5 output has five rows per file; the ones that matter are `st` (today's engine),
`dt` (the module Vox switches to when hints exist, without them) and `dt+ctx` (with
them). If `st` and `dt` differ on a real voice, that is the ADR 0017 open item and
decides whether the module split stays.
