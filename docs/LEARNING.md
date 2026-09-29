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

After a *verified* insertion, the pipeline registers a **watch** on the target field for a
bounded window (90 s, cancelled when focus leaves the app). The watch runs on its own thread
and reads the field back at T+2 s, T+10 s, T+30 s, T+60 s and T+90 s.

```
insert "meet me at cuber netties standup"
        │
        ├── observe field content at T+2s, T+10s, T+30s, T+60s, T+90s (AX read, cheap)
        │
        └── diff against what we inserted
                │
                └── aligned token change: "cuber netties" → "Kubernetes"
                        │
                        └── candidate, stored with count = 1
```

At registration the watch notes where the insertion sits in the field: the text before it
and the text after it. Each read must find that surrounding text unchanged; the span
between is then compared with the insertion. If the surroundings changed, the user is
editing the document rather than the dictation, and the watch ends. A second fix in the
same dictation is measured against the field as it stood after the first, so both count.
Text typed after the insertion is tolerated as long as the rest of the insertion is still
there to anchor on.

Rules that keep this honest:

- **Never learn from a single instance.** A candidate becomes an applied term at **three**
  independent occurrences, across at least two distinct sessions. A **session** starts at
  launch and again after four hours without a dictation — Vox is a tray app that runs for
  weeks, so a launch alone would be no boundary at all. The setting
  `learning.minOccurrences` can raise the three, never lower it.
- **Only local, aligned edits count.** If the user rewrote the whole sentence, that is editing,
  not correcting — discard it. Alignment must map a contiguous span of at most three
  inserted tokens to a contiguous replacement of at most three, with the rest of the
  insertion intact around it. Deletions are not corrections; neither are insertions. Tokens
  compare with surrounding punctuation removed, so adding a comma changes nothing. When the
  fix is on the very last token there is nothing after it to anchor on, so the replacement
  may then not be longer than what it replaced — "prea" → "Priya" is caught there, "Priya"
  → "Priya Sharma" is not.
- **Never store surrounding text.** The candidate record holds the wrong form, the right form,
  a count, and a timestamp. Not the sentence it appeared in.
- **Never watch a field we refused to insert into.** Password fields and secure-input contexts
  are excluded before the watch is registered, not after.
- **Watches are cheap and bounded.** An accessibility read every few seconds for 90 s, only on
  the field we just wrote to. If the read fails, the watch is dropped silently — this feature
  never degrades the dictation path.
- **The homophone guard: no candidate when both forms are ordinary words.** "their" →
  "there", "to" → "too", "affect" → "effect" are the user fixing a homophone the engine
  chose wrongly *in that sentence*, not teaching a term. Learning it would make the next
  sentence wrong. A candidate is only recorded when at least one of the two forms is not in
  the system word list (`/usr/share/dict/words` on macOS, the platform spell-checker
  elsewhere), case-insensitively, token by token for multi-word forms. Proper nouns,
  product names, acronyms and jargon pass because they are not dictionary words; grammar
  fixes never do. A contraction counts as ordinary when its base is ("it's", "you're"),
  because the word list has no apostrophes; a token with no letters (a number) counts as
  ordinary too. If the word list cannot be loaded, nothing is recorded: without it every
  word looks unusual and the guard could not do its job. [TESTING.md](TESTING.md) lists
  the homophone pairs that must produce no candidate.

## How terms are applied

Applied terms become a post-recognition replacement list, the same mechanism as the manual
dictionary. When the recogniser is already on the dictation module because the focused
field supplied hints ([CONTEXT.md](CONTEXT.md), spike S5), they are also passed as
`AnalysisContext.contextualStrings`, ranked ahead of the field's terms, which fixes the
error rather than patching it. They never switch the module on their own: that module
costs a dropped last word now and then, which is worth it for a name on screen and not
for a replacement the post-processing pass makes anyway.

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
  reversals     INTEGER NOT NULL DEFAULT 0,
  state         TEXT NOT NULL,      -- candidate | applied | suspended | rejected
  sessions      INTEGER NOT NULL DEFAULT 1,  -- distinct sessions the fix was seen in
  last_session  INTEGER             -- id (start time, ms) of the last of them
);
```

It shares `history.db` ([HISTORY.md](HISTORY.md)). A row matches on the wrong form
case-insensitively and the right form exactly; the first-seen spelling of the wrong form is
kept. Kilobytes, not megabytes. The compounding asset costs nothing in footprint.

## Inspecting what has been captured

The Privacy pane states what is stored and has the delete button; the Vocabulary pane lists
applied terms and, on request, the candidates still waiting. The table itself is plain
SQLite, readable with the `sqlite3` that ships with macOS:

```bash
sqlite3 -header -column ~/Library/Application\ Support/com.pixelsmashing.dictation/history.db \
  "SELECT id, wrong_form, right_form, count, sessions, state, source_apps,
          datetime(first_seen/1000,'unixepoch','localtime') AS first_seen,
          datetime(last_seen/1000,'unixepoch','localtime')  AS last_seen
   FROM vocab_candidates ORDER BY last_seen DESC"
```

The log (`~/Library/Logs/com.pixelsmashing.dictation/vox.log.<date>`, dated in UTC) says
what each watch did in counts only —
`watch: registered for 90 s`, `watch: read 2: candidate recorded (2 → 1 tokens), seen 1
times in 1 sessions`, `watch: ended at read 3, focus left the app` — never the forms.

## What this is not

Not an LLM rewriting your sentences. Not tone adjustment, not summarisation, not "cleaning up"
what you said. Vox changes individual terms it has watched you correct three times, and
nothing else. Silent rewriting of meaning is a trust failure that no accuracy gain justifies.
