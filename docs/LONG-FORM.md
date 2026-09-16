# Long-form sessions

## Two jobs, one key

**Short-form** — a sentence into a field. Under 30 seconds. The dictation loop everything else
in this repo describes.

**Long-form** — thinking out loud for two to fifteen minutes. A different job: the output is
not going into the field you are looking at, you need to see the thought forming, and stopping
to breathe should not end the session.

macOS's built-in dictation cuts out after roughly a minute, so the entire long-form category is
unserved by the tool most people already have. That is why length appears twice on the
priority list.

## The interaction

Same key, one additional gesture:

| Gesture | Result |
| --- | --- |
| Hold, speak, release | Short-form. Text into the focused field. |
| Hold, then press the lock key (default: `L` while held), release | Session starts. Runs until stopped. |
| Press the hotkey again, or click Stop | Session ends. |
| Escape | Cancels, discards everything. |

No separate hotkey to learn, no mode switch in settings, and the short-form path is unchanged
for people who never use this.

## During a session

A panel appears — not an overlay, since this one is meant to be looked at:

```
┌────────────────────────────────────────────────┐
│  ● 04:12                          Pause   Stop │
├────────────────────────────────────────────────┤
│  …so the problem with the current approach is  │
│  that we're paying the model load cost on      │
│  every single dictation, which means the       │
│  first one after lunch always feels broken.    │
│                                                │
│  What if we predicted it instead ▊             │
├────────────────────────────────────────────────┤
│  Send to ▾                          412 words  │
└────────────────────────────────────────────────┘
```

- **Text appears as you speak**, in chunks, so you can see the thought forming. Chunked
  transcription is enough for this; it does not require the streaming engine, which is why
  long-form does not have to wait for v1.1.
- **Silence does not end the session.** Long pauses are what thinking sounds like. Only the
  configured cap (default 30 minutes) and an explicit stop end it.
- **Pause** stops capture without ending the session.
- The panel is editable — you can fix something without leaving.

## Where it goes

Long-form output does **not** go into the focused field. On stop, the user picks a destination
once, and the choice is remembered per session type:

| Destination | Behaviour |
| --- | --- |
| Clipboard | Copied, panel closes. The default. |
| New file | Plain `.md` in a folder the user chooses, auto-titled from the first sentence. |
| Insert at cursor | For people who really do want 400 words in the field. |
| Append to file | For a running journal or scratch file. |

**Destinations, not folders.** Vox routes long-form output to where the user's real work lives
and gets out of the way. It does not become a place where transcripts accumulate and have to be
organised — see [adr/0012](adr/0012-no-folders.md).

## Chunking and memory

Audio is processed in ~30 second windows with a small overlap, transcribed incrementally, and
**not held in full**. A 30-minute session never holds 30 minutes of audio in memory: each window
is transcribed and released. Peak memory during a long session is the model plus a few seconds
of audio, not the model plus the session.

Text is kept, because text is kilobytes.

## Auto-titling

The session title is the first sentence, trimmed, with no model call involved. Deterministic,
instant, and wrong in an obvious way rather than a confident one. Renaming is one click.

## What long-form does not do

No summarisation, no action-item extraction, no speaker labels, no meeting capture. Those make
this a different product with a different threat model. The job here is "get the thought out of
my head", and the thought is the deliverable.
