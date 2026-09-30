# 0017 — Recognition is biased by the text in the focused field

**Status:** accepted

## Context

Wrong proper nouns are the failure that recurs on every use. Most of the terms a user is
about to say are already in the field they are dictating into. Spike S5
([spikes/s5-context.md](../spikes/s5-context.md)) showed Apple's `DictationTranscriber`
honours `AnalysisContext.contextualStrings`, while the `SpeechTranscriber` module in use
ignores them, and that a compiled custom language model adds nothing over contextual strings.

Reading the user's document is a larger step than hearing their speech, in two ways. It is a
capability: Vox looks at what they wrote, not only what they said. And it is a lever on the
output: a contextual string is a word the recogniser will prefer whenever the audio is close,
so every hint is also a chance to write a word the user did not say.

## Decision

At key-down, read the focused element's text around the caret and pass **at most 20** terms,
nearest the caret first, with applied learned terms and the manual dictionary ranked ahead,
as contextual strings for that one dictation. Switch the engine module to
`DictationTranscriber` whenever hints exist. Only the focused field; never password fields or
secure input; never stored or logged; off the critical path.

The setting, `privacy.readFocusedField`, lives in the Privacy pane and is **off by default**.
Not for latency, and not because the data goes anywhere — it does not leave process memory
or the machine — but because Vox reading the user's documents is a capability they should
opt into knowingly, exactly as `learning.applyLearnedTerms` is. Design in
[CONTEXT.md](../CONTEXT.md). Moves from M9 to M3.

## Consequences

- The main accuracy lever the product has, one toggle away, for users who want it.
- Users who never turn it on get today's engine and today's behaviour, unchanged —
  unless they turn on `learning.applyLearnedTerms`, which since 2026-09-29 switches the
  module by itself once a term is learned, without reading any field
  ([LEARNING.md](../LEARNING.md#how-terms-are-applied)). This setting governs reading
  the field and nothing else.
- The 20-term cap bounds the injection risk per dictation; the nearest-first order puts the
  bound where the relevant names are. If real use shows the cap is too tight, raising it is
  a measured change against the wrong-word rate, not a default.
- Two modules of the same framework in play: `DictationTranscriber` with hints,
  `SpeechTranscriber` without. Equal on the one fixture measured; a real-voice comparison is
  owed before M3 ships, and the split goes away if they prove equal there.
- A guard test enforces that context never reaches a log line or a database column.
- If contextual strings prove too weak on real use, the custom-LM route exists but would
  put the user's terms in a file; that would need its own decision.
