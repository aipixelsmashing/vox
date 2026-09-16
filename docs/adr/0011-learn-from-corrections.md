# 0011 — Learn vocabulary from corrections, locally

**Status:** accepted

## Context

Mangled proper nouns are the most-cited failure of every dictation tool. Every existing fix
asks the user to type words into a settings pane, which almost nobody does.

Vox already knows exactly what it inserted and can read the field afterwards through the
accessibility permission it already holds. Every correction the user makes is a labelled
training pair produced at zero cost.

## Decision

Watch the target field for a bounded window after insertion, detect aligned local edits, and
accumulate vocabulary candidates locally. Apply a term after three occurrences across at least
two sessions. Ship capture from the first release; ship *application* off by default for the
first year.

## Consequences

- The app gets better at the user's own vocabulary without anyone configuring anything, and it
  compounds with use.
- Only possible because everything is local. Apple does not retain your text; cloud tools
  would have to do this server-side, where it becomes a liability they must explain. Privacy
  becomes the condition that makes the personalisation acceptable rather than a constraint.
- **The failure mode is real**: a system that learns silently can be confidently wrong forever
  and leave the user unable to work out why. Mitigated by making the correction path as good as
  the learning path — visible terms, provenance, one-click delete, auto-suspend on repeated
  corrected-backs — and by not shipping it on by default until we have data on how often it
  goes wrong.
- Capture and application are separate switches on purpose. The corpus takes months to
  accumulate; delaying capture wastes signal that cannot be recovered later.
- Never keep surrounding text. Candidates store the wrong form, the right form, a count, a
  timestamp, and the app names for provenance.
