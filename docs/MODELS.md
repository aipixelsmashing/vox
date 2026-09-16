# Speech models

## Selection

Two engines, several models, one default that most people will never change.

| Model | Engine | Size (quantised) | Languages | Licence | Role |
| --- | --- | --- | --- | --- | --- |
| Parakeet TDT 0.6B v3 (ONNX, int8) | `parakeet-rs` | ~700 MB | 25 European | CC-BY-4.0 | **Default** |
| Parakeet TDT 0.6B v2 (ONNX, int8) | `parakeet-rs` | ~650 MB | English only | CC-BY-4.0 | English-only, marginally faster |
| Whisper large-v3-turbo (GGML q5) | `whisper-rs` | ~950 MB | ~99 | MIT | Wide multilingual |
| Whisper small (GGML q5) | `whisper-rs` | ~250 MB | ~99 | MIT | Low-RAM machines |
| Whisper base.en (GGML q5) | `whisper-rs` | ~60 MB | English | MIT | Minimum viable / air-gapped installer |
| Nemotron Speech Streaming 0.6B | `parakeet-rs` | ~700 MB | English (v1) | OpenMDW | v1.1 streaming mode |
| **Apple SpeechAnalyzer** | `speechanalyzer` | **0 bytes** | Apple's list | OS-provided | **Default on macOS 26+** |

### Why Apple's model is the default on macOS 26+

No download, no weights of ours in memory, and it runs on the Neural Engine. An app built this
way ships as a ~4 MB binary idling around 60 MB — the entire footprint problem solved on Mac,
and the 700 MB first-run download removed at the same time.

We keep everything Apple does not offer: hold-to-talk on any key, no session cutoff, transcript
history, learned vocabulary, long-form sessions, and the same behaviour on the user's other
machines. See [adr/0013](adr/0013-os-speech-engine.md).

Users can switch to Parakeet or Whisper if Apple's model handles their vocabulary or language
badly. The engine in use is always visible in Settings.

### Why Parakeet is the default everywhere else

Accuracy and speed both point the same way for dictation-length utterances.
<cite index="16-1">Parakeet TDT 0.6B v3 posts 6.34% average word error rate on the Hugging Face Open ASR Leaderboard against Whisper large-v3's 7.44%</cite> — a margin of roughly one word in a
hundred, which is worth keeping in proportion, since real accuracy depends far more on the
user's accent, microphone and vocabulary than on that gap. The decisive factor is speed:
<cite index="15-1">its token-and-duration transducer learns when it can skip audio frames unlikely to produce new text</cite>, which makes it dramatically faster than Whisper on
plain CPU, and CPU is what most of our users have. It also emits punctuation and
capitalisation natively, so there is no second model in the path.

Whisper stays because Parakeet v3 covers 25 languages and Whisper covers around 99. For a user
dictating in Japanese, Hindi or Arabic, Parakeet is not an option at all.

### Why not the streaming model as default in v1.0

Nemotron streaming would cut perceived latency further by transcribing while the user speaks,
leaving only the tail to process on release. It is the right long-term answer and is why
`parakeet-rs` was chosen (it loads both through the same API). It is not the v1.0 default
because partial-results UI, mid-utterance correction, and the interaction with VAD all need
design work that would delay shipping the core loop. See [ROADMAP](../ROADMAP.md) M6.

## Licensing and redistribution

Read this before touching the packaging.

- **Parakeet models are CC-BY-4.0.** <cite index="70-1">NVIDIA states use is governed by the CC-BY-4.0 licence</cite> and the model is cleared for commercial and non-commercial use.
  CC-BY requires **attribution**, so `NOTICE` carries the NVIDIA credit, and that attribution
  travels with any redistribution — including a bundled-model installer variant. When
  repackaging quantised conversions, the upstream copyright and notice must be retained; the
  only permitted modifications in our pipeline are format conversion and quantisation.
- **Whisper models are MIT.** Attribution in `NOTICE`, no further obligation.
- **Nemotron is OpenMDW.** Read the terms before shipping it; it is not the same licence as
  the Parakeet family.
- **Beware third-party repackages.** Several Hugging Face mirrors of Parakeet are relicensed
  by the repackager — at least one NPU-optimised build is CC-BY-**NC**-4.0, which we cannot
  ship. Only pull from the upstream NVIDIA repo or a mirror whose licence we have read and
  recorded in the registry.

Every entry in the model registry carries its licence string, its source URL, and its
attribution text, and CI fails if any entry is missing one.

## Distribution

Models are **not** bundled in the default installer. Reasons: a 700 MB installer for a tray
utility is hostile, users on low-RAM machines want a different model anyway, and model updates
should not require an app update.

### Registry

`models.json` ships inside the app and is also published at a stable URL for updates:

```json
{
  "schemaVersion": 1,
  "models": [
    {
      "id": "parakeet-tdt-0.6b-v3-int8",
      "engine": "parakeet",
      "displayName": "Parakeet v3 (25 languages)",
      "sizeBytes": 712849408,
      "languages": ["en","es","fr","de","it","pt","nl","pl","sv","da","fi","cs","sk","sl","hr","bg","el","et","lv","lt","hu","mt","ro","ru","uk"],
      "license": { "spdx": "CC-BY-4.0", "attribution": "NVIDIA Parakeet TDT 0.6B v3 — https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3" },
      "files": [
        { "name": "encoder.int8.onnx", "sha256": "…", "urls": ["https://…", "https://mirror…"] },
        { "name": "decoder.int8.onnx", "sha256": "…", "urls": ["https://…"] },
        { "name": "tokenizer.json",    "sha256": "…", "urls": ["https://…"] }
      ],
      "minRamMb": 2048,
      "recommendedFor": ["default"]
    }
  ]
}
```

### Download

- Resumable HTTP range requests; a dropped connection resumes rather than restarts.
- **SHA-256 verified before the file is moved into place.** A mismatch is reported as a hash
  mismatch — never as a generic network error — and the partial file is deleted.
- Downloads land in `models/.staging/` and are atomically renamed on success.
- Progress is shown per file and in aggregate, with a byte count, because a silent 700 MB
  download feels broken.
- The download is the **only** outbound connection the app makes other than the update check,
  and it happens once. This is asserted by a test.

### Sideloading and air-gapped installs

For machines that will never have internet access:

1. `vox model export --id parakeet-tdt-0.6b-v3-int8 --out ./bundle` on a connected machine
   produces a directory with the model files and a signed manifest.
2. Copy it across. In Settings → Models → *Import from folder*, or
   `vox model import ./bundle`.
3. Hashes are verified on import exactly as they are on download.

A separate installer variant that bundles Whisper `base.en` (~60 MB) exists so that a
completely offline machine has a working dictation path immediately, with an upgrade to a
better model available by sideload.

## Runtime behaviour

- **Adaptive residency, not warm-always.** Weights are memory-mapped and unloaded after ~10
  minutes idle, then reloaded predictively before the user reaches for the key. There is no
  setting for this. Full specification in [FOOTPRINT.md](FOOTPRINT.md).
- **Vocabulary biasing.** Where the engine supports it, learned terms
  ([LEARNING.md](LEARNING.md)) are passed as a recognition hint rather than patched in
  afterwards, which fixes the error instead of correcting it.
- **Execution provider selection**: auto-detect, with per-platform preferences —
  CPU or WebGPU on Apple silicon (note `parakeet-rs` documents CoreML as unstable for this
  model), CUDA where available on Windows/Linux, DirectML as a Windows fallback, CPU
  everywhere else. The chosen provider is shown in Settings so a slow machine can be diagnosed.
- **Model switching** unloads the old model before loading the new one, and blocks dictation
  with a visible state while it happens.
- The engine runs on its own thread; a panic inside inference restarts the thread rather than
  taking down the app, and surfaces one error rather than one per attempt.

## Evaluation

Before changing the default model, run `cargo run -p vox-eval`:

- WER on a held-out set of dictation-style utterances (short, first-person, punctuated) —
  public benchmark suites are read-aloud corpora and are not representative of this workload.
- Real-time factor and absolute latency for 3 s, 6 s, 15 s and 60 s inputs on each reference
  machine.
- Peak RSS.
- Punctuation and capitalisation quality, scored separately from WER, since a model that gets
  every word right and no commas is worse for dictation than the reverse.
- **Idle and peak RSS**, and warm-reload time from page cache. A model that is 1% more accurate
  and 300 MB larger is not obviously better once residency is adaptive.
- **Accuracy on the user's own corrected vocabulary**, once a corpus exists. The relevant
  question is not average WER but whether it gets *their* proper nouns right — which is what
  makes a smaller model plus learned terms a live option (spike S4).
