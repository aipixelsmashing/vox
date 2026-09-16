# 0014 — Long-form is a session, not a longer timeout

**Status:** accepted

## Context

macOS dictation cuts out after roughly a minute. Raising our own cap would make Vox tolerate
long input, but "a sentence into a field" and "thinking out loud for ten minutes" are different
jobs with different outputs, different feedback needs, and different destinations.

## Decision

A locked session mode on the same hotkey: hold, press the lock key, release. Text appears in a
panel as it forms, silence never ends the session, and the result is routed to a destination on
stop rather than inserted into the focused field.

## Consequences

- The unserved category — two to fifteen minutes of thinking aloud — becomes the thing Vox does
  that the built-in tools structurally cannot.
- No new hotkey and no mode switch in settings; users who never lock the key see no change.
- Chunked transcription with a small overlap is sufficient, so this does not have to wait for
  the streaming engine.
- Audio is transcribed and released per window, so a 30-minute session does not hold 30 minutes
  of audio. Peak memory stays close to a short dictation.
- Scope risk: sessions invite summarisation, action items, and speaker labels. All refused —
  that is a meeting-notes product with a different threat model.
