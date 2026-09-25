# S5 — Does Apple SpeechAnalyzer accept contextual hints or a custom language model?

**Answer: yes, with one condition.** `AnalysisContext.contextualStrings` biases recognition
toward supplied terms — but only for the `DictationTranscriber` module. The
`SpeechTranscriber` module Vox currently uses ignores the same hints completely. The custom
language model route (the successor of `SFCustomLanguageModel`) compiles and loads but
changed nothing with phrase counts alone.

Run 2026-09-23 on macOS 27.0, Apple M1 Pro. Harness: [`spikes/s5-context`](../../spikes/s5-context/),
a Rust binary linking a Swift bridge that transcribes one file under five conditions.
Fixture: `fixtures/audio/context-6s.wav`, 7.2 s, synthesised with `say -v Samantha`:
*"Please ask Orsolya Csernák about the Kubestrix migration, and check that keytap and
axuielement still build in the Tauri app."* Supplied terms: `Orsolya Csernák`, `Kubestrix`,
`keytap`, `axuielement`, `Tauri`.

## What the API offers

| Route | API | Applies to |
| --- | --- | --- |
| Contextual strings | `AnalysisContext.contextualStrings[.general] = [String]`, passed to `SpeechAnalyzer(…, analysisContext:)` or `setContext(_:)` | Honoured by `DictationTranscriber`; ignored by `SpeechTranscriber` |
| Custom language model | `SFCustomLanguageModelData(locale:identifier:version:) { PhraseCount / CustomPronunciation / templates }` → `export(to:)` → `SFSpeechLanguageModel.prepareCustomLanguageModel(for:configuration:)` → `DictationTranscriber.ContentHint.customizedLanguage(modelConfiguration:)` | `DictationTranscriber` only. `SFCustomLanguageModel` itself is gone; this is its replacement |

## What happened

| Condition | Transcript of the hard sentence | Terms recovered | Time |
| --- | --- | --- | --- |
| `SpeechTranscriber`, no hints | "Please ask Orsalai Zernak about the Cubestrix migration, and check that keyed up and axo elements still build in the Tory app." | 0/5 | 244 ms |
| `SpeechTranscriber` + contextual strings | identical, word for word | 0/5 | 249 ms |
| `DictationTranscriber`, no hints | "Please ask our Saul about the Custer's migration and check that key up and Axel helmet still building the toy app" | 0/5 | 808 ms |
| **`DictationTranscriber` + contextual strings** | "Please ask our Saul about the **Kubestrix** migration and check that key up and axle helmet still built in the **Tauri** app" | **2/5** | 843 ms |
| `DictationTranscriber` + custom LM (phrase counts ×50) | as no hints | 0/5 | 802 ms (+1.0 s one-time compile) |

On the clean fixture with no hints, both modules transcribe the sentence identically
(`SpeechTranscriber` 175 ms, `DictationTranscriber` 255 ms; the latter dropped the final
full stop). The base accuracy gap on the hard sentence is the synthetic voice mangling
invented words, not the module.

`DictationTranscriber` reported only a volatile result for file input and no final, even
after `finalizeAndFinishThroughEndOfInput`; the volatile text was used. In the streaming
path the app already calls `finalize(through:)` periodically, which should produce finals;
to be confirmed when the module is switched.

## What it means for the design

1. **Context-aware biasing is possible in v1 and moves from M9 to M3.** Design in
   [CONTEXT.md](../CONTEXT.md), decision in [ADR 0017](../adr/0017-context-from-focused-field.md).
2. **The engine module changes to `DictationTranscriber`** when hints are in use, which is
   whenever the context setting is on or learned/manual vocabulary exists. Same framework,
   same assets model, same streaming API, ~80 ms slower on a 6 s clip. Base accuracy was
   equal on the one real-sentence fixture; a real-voice comparison is an open item.
3. **Learned vocabulary gets its "fix it at recognition" path** ([LEARNING.md](../LEARNING.md#how-terms-are-applied)):
   applied terms and the manual dictionary go into the same contextual strings.
4. **The custom LM route is not worth it for v1.** It compiles in a second and loads, but
   phrase counts alone did nothing here, and it needs a file on disk that would hold the
   user's terms. Contextual strings are in-memory and per request, which fits the
   never-stored rule. Revisit only if contextual strings prove too weak on real use.

## Open items

- Real-voice fixture with genuinely rare terms, to size the accuracy gain and the module
  base-accuracy gap on a human speaker.
- Whether hint count or length degrades recognition or latency, and how often a hint
  produces a word the user did not say (the app passes at most 20, nearest the caret).
- ~~`DictationTranscriber` finals in the streaming path.~~ Answered 2026-09-25 in the
  product's own streaming path (`cargo test -- --ignored streams_with_hints`, macOS 27.0):
  the fixture pushed in 20 ms chunks with the periodic `finalize(through:)` gave **0 final
  / 30 volatile** results from `DictationTranscriber` (277 ms at release) against 1 final /
  36 volatile from `SpeechTranscriber` (196 ms). The module does not finalise in this path
  even through `finalizeAndFinishThroughEndOfInput`; the bridge keeps the last volatile
  result when nothing after it was finalised, and the transcript was complete and identical
  to the file run above (2/5 terms recovered). That rule is load-bearing and the log says
  "volatile tail used" whenever it fires.
- Custom LM with `CustomPronunciation` and the `weight` parameter, if ever needed.
