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
                        ├── read again 3 s later: the same text? then it has settled
                        │
                        └── candidate, stored with count = 1
```

**A correction is recorded once it has settled:** two consecutive reads find the same text
where the insertion was. A read can land while the user is part-way through typing the fix,
and what it sees then is neither what Vox wrote nor what they meant ("deco" on its way to
"Adi" read as "de"). So the first read to see a change records nothing and asks for a
confirming read three seconds later; scheduled reads that would come sooner are skipped, so
the two are never closer than that. If the confirming read finds something else again, that
becomes the text to confirm. A change first seen by the read at 90 s still gets its
confirming read, at 93 s. The cost, accepted: the user has to stay in the app for three
seconds after a fix, and a pause of three seconds in the middle of typing one can still be
taken for the end of it, which the threshold below is there to absorb.

At registration the watch notes where the insertion sits in the field: the text before it
and the text after it. Each read must find that surrounding text unchanged; the span
between is then compared with the insertion. If the surroundings changed, the user is
editing the document rather than the dictation, and the watch ends. A second fix in the
same dictation is measured against the field as it stood after the first, so both count.
Text typed after the insertion is tolerated as long as the rest of the insertion is still
there to anchor on.

**The last three watches run side by side.** A new dictation does not end the watch on the
one before it, because the fix often comes a sentence late: dictate A, dictate B, notice the
name in A. A's watch reads B as text typed after its insertion and still sees the fix. The
fourth dictation ends the oldest watch; a watch whose window ran out, or whose app lost
focus, frees its place. Two limits follow from the anchoring, and are accepted:

- A fix in A changes the text before B, so B's watch ends at its next read. A later fix in
  B is then not seen. It also means one fix is never counted by two watches.
- A fix on the *last* word of A, once B follows it, cannot be told from typing and is not
  recorded (the rule about the very last token, below).

Rules that keep this honest:

- **Never learn from a single instance.** The threshold is **three** independent
  corrections, across at least two distinct sessions. A **session** starts at launch and
  again after four hours without a dictation — Vox is a tray app that runs for weeks, so a
  launch alone would be no boundary at all. The setting `learning.minOccurrences` can raise
  the three, never lower it.
- **Two thresholds, because there are two things to learn.**
  - *The right form, however it was mangled.* Three corrections to "Adi" — from "Eddie",
    from "AD", from "A de" — make "Adi" a **hinted term**: it is given to the recogniser
    as a hint. The names a recogniser mangles differently every time are exactly the ones
    worth learning, and a hint needs only the right form. Counted over every pair with
    that right form, matched exactly; a session two pairs share counts once.
  - *The pair.* "Eddie" → "Adi" is a literal replacement, and replacing text is the
    stronger act, so it has to earn the threshold **by itself**: that wrong form corrected
    to that right form three times over two sessions makes the pair **applied**. Pairs are
    kept from the first correction, as evidence and for this.

  Hinted is computed from the pairs and stored nowhere, so deleting a pair takes its
  evidence with it, and a right form can stop being hinted. A suspended or rejected pair
  is no evidence for its right form.
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

**Hinted terms** are passed to the recogniser as `AnalysisContext.contextualStrings` on
every dictation, ranked ahead of the manual dictionary and the field's terms
([CONTEXT.md](CONTEXT.md), spike S5). That fixes the error rather than patching it.

Hints need the dictation module, and **learned terms switch to it on their own**: with
`learning.applyLearnedTerms` on and at least one hinted term, every dictation runs on it,
whatever field it goes into. `privacy.readFocusedField` governs reading the field and
nothing else; learned terms neither need it nor turn it on.

**What that costs.** The dictation module drops the last word of a dictation now and
then: on the S5 fixture it lost one short word at the end in three streamed runs of three
([spikes/s5-context.md](spikes/s5-context.md)). Turning learned terms on is accepting that
on every dictation, in exchange for the names coming out right. The setting's description
says so. With the setting off, or on with nothing learned yet, the recogniser is the one
it always was.

**Applied pairs** become a post-recognition replacement list, the same mechanism as the
manual dictionary (M7).

Both wait on `learning.applyLearnedTerms`.

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
  last_session  INTEGER,            -- id (start time, ms) of the last of them
  session_ids   TEXT                -- ids of all of them, comma-separated
);
```

One row is one pair, and `state` is the pair's. A right form's count and sessions are summed
over its rows when they are needed; `session_ids` is what lets a session two pairs share
count once. Rows from before it existed name the one session they still know.

It shares `history.db` ([HISTORY.md](HISTORY.md)). A row matches on the wrong form
case-insensitively and the right form exactly; the first-seen spelling of the wrong form is
kept. Kilobytes, not megabytes. The compounding asset costs nothing in footprint.

## Inspecting what has been captured

The Privacy pane states what is stored and has the delete button; the Vocabulary pane lists
what is in use (applied pairs, and every pair behind a hinted term) and, on request, the
candidates still waiting. The table itself is plain
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
`watch: registered for 90 s`, `watch: read 2: change seen, waiting for it to settle`,
`watch: read 3: candidate recorded (2 → 1 tokens), seen 1 times in 1 sessions; its right
form 3 times in 2 sessions, now hinted`, `watch: ended at read 3, focus left the app` —
never the forms.

The right forms and what they have reached:

```bash
sqlite3 -header -column ~/Library/Application\ Support/com.pixelsmashing.dictation/history.db \
  "SELECT right_form, SUM(count) AS corrections, COUNT(*) AS manglings,
          group_concat(session_ids, ',') AS sessions_seen
   FROM vocab_candidates WHERE state NOT IN ('suspended','rejected')
   GROUP BY right_form ORDER BY corrections DESC"
```

## What this is not

Not an LLM rewriting your sentences. Not tone adjustment, not summarisation, not "cleaning up"
what you said. Vox hints a term you have corrected to three times, replaces a word you
have corrected the same way three times, and does nothing else. Silent rewriting of meaning is a trust failure that no accuracy gain justifies.
