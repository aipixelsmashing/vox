# 0019 — Per-app defaults that learn

**Status:** accepted

## Context

Three things about the shape of a dictation are settings today, and global: whether a
space follows the text (`output.trailingSpace`), whether line breaks become spaces
(`output.collapseNewlinesInTerminals`, on for a fixed list of terminals), and how the text
is placed (`output.method`). The right answer to each depends on the app. A chat composer
wants the space; a search box does not. A terminal wants the breaks collapsed; a notes app
does not. One app takes a direct accessibility write; another accepts it and shows nothing.

[0010](0010-adaptive-residency.md) says a question whose answer depends on the situation
is for the system to decide, not for a setting. A per-app settings table would be the
wrong answer twice over: nobody fills it in, and it asks the user to know what Vox should
have noticed. [0011](0011-learn-from-corrections.md) already has the mechanism that
notices: after a verified insertion the field is read back for 90 seconds, and a change
that has settled is compared with what was inserted. Deleting the trailing space, or
turning the line breaks into spaces, is a fix of exactly that kind, made in the same place
and seen by the same watch. It is thrown away today because it is not a vocabulary pair.

## Decision

The watch recognises **shape fixes** alongside vocabulary corrections, and Vox learns a
**per-app default** from them under the vocabulary rules: three fixes of the same kind in
the same app across two sessions, and never from one.

- A settled change that equals the insertion with its trailing space removed, or added, is
  a trailing-space fix. One that equals the insertion with its line breaks turned into
  spaces, or (when Vox collapsed them) with the original breaks restored, is a line-break
  fix. The watch compares against these exact variants, computed at registration; anything
  else goes on to the vocabulary alignment as before.
- Evidence accumulates per app and aspect. A fix the other way while the evidence is still
  gathering starts it over: a user who alternates is not consistent, and Vox learns
  nothing. A fix the other way after a default is applied is a reversal; two suspend it,
  as two corrected-backs suspend a term.
- An applied default replaces the global setting for that app, in post-processing, and
  nothing else. The settings in the Text pane stay what they are for every other app.
- **The insertion method is learned from Vox's own evidence, not from a fix.** When a
  direct write reports success and never takes, verification already catches it and the
  app is remembered as paste-only (M2). A fix for a wrong method is not something the watch
  can see: the text it would read is the text it wrote. So the memo is surfaced as a
  per-app default with its own provenance, resettable like the others, and no user signal
  is invented for it. If one turns up in real use, it is a separate decision.
- Visible and resettable, in the Text pane, with provenance on every row ("3 fixes since
  12 October", or "Direct insertion never took here"), a reset per row and a reset for
  all. Export writes the list as a plain file ([0015](0015-exit-is-cheap.md)).
- Governed by the same switch as vocabulary capture, `learning.captureCorrections`, because
  it is the same watch. No new setting.

## Consequences

- The thing people set once and then fight in one app stops needing to be set. The cost is
  the one every silent adaptation carries: a behaviour that changed without being asked
  for. The row in the Text pane is the answer to "why did it stop adding a space here?",
  and the reset is one click.
- The watch now holds a few variants of the inserted text for the window, in memory, never
  stored or logged; the table holds a bundle id, an aspect, a value and counts. No text.
- Line-break learning needs dictations that contain line breaks, which until the spoken
  commands exist is rare. The mechanism is the same either way.
- Starting over on a contradicting fix means a user who genuinely changes their mind has
  to be consistent for three fixes again. Accepted: consistency is the signal.
- The paste-only memo keeps its file; the pane reads both sources as one list. Moving the
  memo into the database is a refactor for when the injector learns anything else.
