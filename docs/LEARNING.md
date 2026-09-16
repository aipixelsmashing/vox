# Learning from corrections

## The problem

Mangled proper nouns are the most-cited failure of every dictation tool, built-in or
otherwise. Library names, colleagues' names, repo names, acronyms, product names. It is the
failure that recurs on *every* use, and every existing fix asks the user to open a settings
pane and type words into a list — which almost nobody does.

## The signal that is currently thrown away

Vox knows exactly what text it inserted, and it can read the field afterwards through the
accessibility API it already requires. When the user changes "cuber netties" to "Kubernetes",
that is a labelled training pair, produced in the moment, at zero cost, with nobody
configuring anything.

Collect enough of those and the app knows the user's vocabulary without ever having asked.

**This is only possible because everything is local.** Apple does not retain your text.
Cloud tools could do this, but only server-side, where it becomes a privacy liability they
have to explain. Here, privacy is the condition that makes the personalisation acceptable
rather than a constraint that prevents it.

## How capture works

After a successful insertion, the pipeline registers a **watch** on the target field for a
bounded window (default 90 s, cancelled when focus leaves the app).

```
insert "meet me at cuber netties standup"
        │
        ├── observe field content at T+2s, T+10s, T+30s (AX read, cheap)
        │
        └── diff against what we inserted
                │
                └── aligned token change: "cuber netties" → "Kubernetes"
                        │
                        └── candidate, stored with count = 1
```

Rules that keep this honest:

- **Never learn from a single instance.** A candidate becomes an applied term at **three**
  independent occurrences, across at least two distinct sessions.
- **Only local, aligned edits count.** If the user rewrote the whole sentence, that is editing,
  not correcting — discard it. Alignment must map a contiguous span of inserted tokens to a
  contiguous replacement.
- **Never store surrounding text.** The candidate record holds the wrong form, the right form,
  a count, and a timestamp. Not the sentence it appeared in.
- **Never watch a field we refused to insert into.** Password fields and secure-input contexts
  are excluded before the watch is registered, not after.
- **Watches are cheap and bounded.** An accessibility read every few seconds for 90 s, only on
  the field we just wrote to. If the read fails, the watch is dropped silently — this feature
  never degrades the dictation path.

## How terms are applied

Applied terms become a post-recognition replacement list, the same mechanism as the manual
dictionary, and where the engine supports vocabulary biasing they are also passed to the model
as a hint before recognition, which fixes the error rather than patching it afterwards.

Matching is case-insensitive and whole-token; replacement preserves sentence-initial
capitalisation.

## The failure mode, stated plainly

A system that learns silently can learn something wrong and then be confidently wrong forever,
with the user unable to work out why. This is a real risk and it is the reason for the design
below rather than a caveat appended to it.

**The correction path must be as good as the learning path.**

- **Vocabulary pane** — every learned term, the form it replaced, how many times it was
  observed, when it was last used, and a delete button on each row.
- **Provenance on every row.** "Learned from 3 corrections in Slack, first seen 12 March."
- **One-click delete**, and deleting a term also deletes the candidate record behind it, so it
  cannot be re-learned from the same evidence.
- **Undo in place.** When an applied term fires and the user immediately corrects it back, the
  term is suspended after two such events and flagged in the pane.
- **Export as a plain list**, per [VALUES.md](VALUES.md).

## Rollout

Learning ships **off by default for the first year**, with an onboarding card explaining what
it does and what it costs. Without commercial pressure we can afford to find out how often it
goes wrong before making it the default. The decision to flip the default is a data question:
suspended-term rate and corrected-back rate, both measured locally and reported by users who
choose to.

Capture is separate from application. Capture runs from the first release (silently, locally,
storing candidates only) because the asset takes months to accumulate and every week without
it is lost signal. A user who has never turned learning on can still see what would have been
learned, and delete the lot.

## Storage

```sql
CREATE TABLE vocab_candidates (
  id            INTEGER PRIMARY KEY,
  wrong_form    TEXT NOT NULL,
  right_form    TEXT NOT NULL,
  count         INTEGER NOT NULL DEFAULT 1,
  first_seen    INTEGER NOT NULL,
  last_seen     INTEGER NOT NULL,
  source_apps   TEXT,               -- distinct app names, for provenance only
  state         TEXT NOT NULL       -- candidate | applied | suspended | rejected
);
```

Kilobytes, not megabytes. The compounding asset costs nothing in footprint.

## What this is not

Not an LLM rewriting your sentences. Not tone adjustment, not summarisation, not "cleaning up"
what you said. Vox changes individual terms it has watched you correct three times, and
nothing else. Silent rewriting of meaning is a trust failure that no accuracy gain justifies.
