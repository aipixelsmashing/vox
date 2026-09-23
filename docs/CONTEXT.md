# Context from the focused field

Mangled proper nouns are the failure that recurs on every use. Most of the terms a user
dictates are already on their screen: the thread they are replying to, the code around the
cursor, the names in the document. Reading the text field the user is dictating into and
handing its unusual words to the recogniser as hints fixes those terms *at recognition*,
without the user teaching anything.

Spike S5 ([spikes/s5-context.md](spikes/s5-context.md)) established that Apple's
`DictationTranscriber` honours `AnalysisContext.contextualStrings`. Decision:
[ADR 0017](adr/0017-context-from-focused-field.md). Milestone: M3.

## Rules, in priority order

1. **Only the focused field.** The element captured at key-down for injection, and nothing
   else. Never another window, never another app, never the whole document tree.
2. **Never a password field, never under secure input.** The same checks that refuse
   insertion refuse reading, and they run first.
3. **Context never leaves the machine and is never stored.** It lives in memory for one
   dictation, goes to Apple's on-device recogniser as hint strings, and is dropped. It is
   never written to history, logs, the learning tables or the clipboard. A guard test
   asserts no log line and no database column can carry it.
4. **Read at key-down, not key-up.** The read happens right after the audio stream opens,
   while the user is still speaking, so it costs nothing on the release-to-text path. If it
   has not completed when the user releases, the dictation proceeds without hints.
5. **A setting in the Privacy pane, off by default.** This is about what Vox is allowed to
   read, not a speed trade-off, so it sits with history retention and the offline lock,
   with one plain sentence of what it does. Off by default for the same reason
   `learning.applyLearnedTerms` is: Vox reading the user's documents is a capability they
   should opt into knowingly, not discover. The onboarding privacy card offers it with the
   same sentence the pane uses.
6. **Never degrade the dictation path.** Any failure to read, extract or pass hints is
   logged as a count, not a text, and the dictation continues as if the setting were off.

## How it works

```
key-down
  ├── audio stream opens                       (unchanged)
  ├── target captured: frontmost app, focused element   (unchanged)
  ├── if privacy.readFocusedField and not secure and not password:
  │     read AXValue + AXSelectedTextRange of the focused element      ~1–10 ms
  │     take the window around the caret          ≤ 2 000 chars each side
  │     extract hint terms, nearest the caret first, at most 20
  │     add applied learned terms + manual dictionary
  │     engine.stream_start(hint, context)         → AnalysisContext.contextualStrings
  └── … speaking …
key-up
  └── finalize → text                              (unchanged)
```

**Extraction.** Hints are terms, not sentences. From the window, take tokens that are not
ordinary dictionary words (the same system word list the homophone guard uses,
[LEARNING.md](LEARNING.md)): capitalised words and runs of them, identifiers with digits,
underscores, dots or mixed case, acronyms, and anything the word list does not know.
**Cap at 20 terms, nearest the caret first**, walking outwards alternately before and after
it, so the sentence the user is continuing weighs most. Applied learned terms and the manual
dictionary count towards the same cap and rank ahead of extracted terms, because the user
chose them. Common words are never sent: they add nothing and they are the user's text.

The cap is small on purpose. A contextual string is a word the recogniser will prefer
whenever the audio is close, which means every hint is also a chance to write a word the
user did not say. Twenty nearby terms is enough to catch the name in the thread being
answered; two hundred would be two hundred such chances per dictation.

**Timing.** The AX read and extraction run on the pipeline thread before the first audio
chunk is pushed, inside the ~700 ms the Chromium wake-up may already take. If the element
cannot be read within 150 ms the dictation proceeds without context.

**Engine.** `DictationTranscriber` with `contentHints: [.shortForm]`, replacing
`SpeechTranscriber` whenever any hints exist. Without hints the module choice does not matter
for accuracy (S5); the engine keeps using `SpeechTranscriber` then, so a user who turns the
setting off and has no vocabulary gets exactly today's engine. Hints are set per session via
`AnalysisContext` at `stream_start`; the streaming API is otherwise unchanged.

## What the user sees

Nothing, when it works: fewer wrong names. The setting is off until they turn it on; the
Privacy pane and the onboarding privacy card both show:

> **Read the field you're dictating into** — Vox looks at the text around your cursor to
> recognise the names and terms you're likely to say. Read once per dictation, never stored,
> never leaves this Mac. Off in password fields.

Diagnostics shows, per dictation, whether context was used and how many terms, never the
terms.

## What it is not

- Not reading other windows, the clipboard, files, or the app's document model.
- Not stored context, not a profile, not "learning from what you read". Learning stays what
  [LEARNING.md](LEARNING.md) says: corrections, three times, aligned.
- Not a rewrite. Hints bias which of two similar-sounding words the recogniser picks; they
  do not change what was said.

## Threat model additions

- Context is the most sensitive data Vox touches after the transcript itself: it is the
  user's document, not their speech. It exists only in process memory for the length of one
  dictation, and only reaches Apple's speech process on the same machine. See
  [THREAT-MODEL.md](THREAT-MODEL.md).
- A bug that logged or stored context would be a privacy incident. The guard test is the
  control; the setting is the user's control.

## Testing

- Unit: extraction (dictionary words excluded, identifiers kept, the 20-term cap, nearest-
  first ordering, learned and manual terms ranked ahead), the password/secure-input
  refusal, the 150 ms deadline path, and the default being off.
- Guard: no `tracing` call in the context path formats the window or the terms; the
  history schema has no column for them; the settings file has the flag.
- Manual gate ([TESTING.md](TESTING.md)): dictate a name that is on screen and wrong
  without the setting; right with it; unchanged in a password field.
