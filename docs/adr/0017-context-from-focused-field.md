# 0017 — Recognition is biased by the text in the focused field

**Status:** proposed

## Context

Wrong proper nouns are the failure that recurs on every use. Most of the terms a user is
about to say are already in the field they are dictating into. Spike S5
([spikes/s5-context.md](../spikes/s5-context.md)) showed Apple's `DictationTranscriber`
honours `AnalysisContext.contextualStrings`, while the `SpeechTranscriber` module in use
ignores them, and that a compiled custom language model adds nothing over contextual strings.

Reading the user's document is a larger privacy step than reading their speech. It needs a
recorded decision, not a feature flag.

## Decision

At key-down, read the focused element's text around the caret, extract non-dictionary terms,
and pass them, together with applied learned terms and the manual dictionary, as contextual
strings for that one dictation. Switch the engine module to `DictationTranscriber` whenever
hints exist. Rules: only the focused field, never password fields or secure input, never
stored or logged, off the critical path, a Privacy-pane setting on by default. Design in
[CONTEXT.md](../CONTEXT.md). Moves from M9 to M3.

## Consequences

- The main accuracy lever the product has, delivered without the user configuring anything.
- Vox now reads the user's text as well as hearing their speech. The reading is bounded,
  in-memory, single-use and on-device, and a guard test enforces that it stays that way.
- Two modules of the same framework in play: `DictationTranscriber` with hints,
  `SpeechTranscriber` without. Equal on the one fixture measured; a real-voice comparison is
  owed before M3 ships, and the module split goes away if they prove equal there.
- If contextual strings prove too weak on real use, the custom-LM route exists but would
  put the user's terms in a file; that would need its own decision.
