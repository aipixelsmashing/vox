# 0018 — An edit in the history panel is a correction

**Status:** accepted

## Context

[0011](0011-learn-from-corrections.md) learns vocabulary from one source: after a verified
insertion, a watch reads the target field back for 90 seconds and treats an aligned local
edit that has settled as a correction ([LEARNING.md](../LEARNING.md)). That source has
gaps the user feels. A dictation that ended on the clipboard was never in a field, so
nothing watches it. A fix made after the window, or after focus left the app, or in an
app whose field cannot be read back, is not seen. And the panel already shows the
transcript next to Copy and Insert: the natural place to fix a name before re-inserting
it is right there, and until now that fix taught Vox nothing.

The field watch has three rules that keep learning honest, and any second source has to
keep all three: the edit must be *aligned and local* (at most three tokens replaced by at
most three, the rest of the dictation intact), it must have *settled* (not a half-typed
word), and it must pass the *homophone guard*. The thresholds — three corrections across
two sessions — and the session clock are unchanged by where a correction comes from.

## Decision

A row's transcript in the history panel is editable in place. **What counts as a
correction is now: an aligned local edit to a dictation's text, committed by the user,
whether observed to settle in the field or saved in the panel.**

- The saved text goes through `extract_correction` exactly as a field read does: the
  same span limits, the same "rest intact" rule, the same punctuation and case handling.
  A rewrite, a deletion or an added word saves the text and teaches nothing.
- The settle rule's purpose is to never record a word mid-typing. In the panel the
  commit is the settling event: Enter or Save records, Escape or hiding the panel
  discards, and nothing is read while the user types. There is no second read because
  there is no uncertainty about whether they were done.
- The homophone guard, the word-list requirement, `learning.minOccurrences` and the
  three-across-two threshold apply unchanged, through the same `observe` call.
- The row keeps the edited text, as the field keeps the user's fix; a second edit to the
  same row aligns against the first, as the watch rebases its baseline. Copy and Insert
  then use the corrected text, which is the point of fixing it there.
- Provenance names the app the dictation went into (`source_apps`), or Vox when it went
  nowhere. The edit is attributed to the session current when it is saved.
- An edit saves whether or not `learning.captureCorrections` is on; it is a *correction*
  only when it is. No new setting.
- One fix counts once: saving an edit ends the field watch still running on that
  dictation, so the same fix made in both places is not two corrections.

## Consequences

- Clipboard-only dictations, late fixes and unreadable fields all gain a correction path.
  The signal LEARNING.md calls "currently thrown away" is thrown away less.
- A deliberate act replaces an inference. The field watch infers that an edit was a fix
  from its shape and its timing; the panel edit is a user saying so. The same rules then
  decide whether it is a *vocabulary* fix, so the panel is not an easier way to teach a
  wrong word than the field is.
- The watch and the row do not mirror each other. A fix made in the field leaves the row
  showing what was inserted; making the same fix again in the panel counts it twice, in
  one session, so it still cannot promote anything by itself. The row could follow the
  field, but that would make history a copy of the user's document rather than a record
  of what Vox said, and is not done here.
- Transcript text in `history.db` is no longer immutable. The search index follows
  updates by trigger; export writes the current text.
- `history_edit` joins the contract; the panel gains an Edit action, ⌘E, and an editing
  state. Nothing crosses onto the dictation path.
