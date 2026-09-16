# Transcript history and clipboard

## What it is

Every completed dictation is kept locally so the user can retrieve it after the fact — because
insertion can go to the wrong window, get pasted over, or land in a field that then got closed.
The history is the safety net that makes the rest of the product safe to trust.

It is reached from the tray: **Show history**, or a global shortcut (default
Cmd/Ctrl+Shift+V, configurable, off if it conflicts).

## The panel

A frameless, always-on-top window anchored near the tray icon. Not a native menu — native menus
can't do a scrolling list with search and per-row actions across all three platforms.

```
┌──────────────────────────────────────────┐
│  Search transcripts…                  ⚙  │
├──────────────────────────────────────────┤
│  2 min ago · Slack · 34 words            │
│  Can you take a look at the deploy…      │
│                       [Copy] [Insert] [×]│
├──────────────────────────────────────────┤
│  18 min ago · VS Code · 12 words         │
│  refactor the auth middleware to…        │
│                       [Copy] [Insert] [×]│
├──────────────────────────────────────────┤
│  1 h ago · ⚠ not inserted · 56 words     │
│  The quarterly numbers came in…          │
│                       [Copy] [Insert] [×]│
├──────────────────────────────────────────┤
│  47 items                  Delete all…   │
└──────────────────────────────────────────┘
```

Behaviour:

- **Click a row** — copies to the clipboard and closes the panel. This is the primary action
  and needs no button; the buttons exist for discoverability.
- **Copy** — clipboard, marked private so it stays out of clipboard history and cloud sync.
- **Insert** — re-runs the injection pipeline against whatever is focused now. Useful after a
  failed insertion, which is why failed rows are marked.
- **Rows are watched after insertion** for a bounded window, so that corrections the user makes
  in the target app become vocabulary candidates ([LEARNING.md](LEARNING.md)). The history row
  stores only the transcript; correction candidates live in their own table and never keep
  surrounding text.
- **×** — deletes that row immediately, no confirmation. It's one row and it's the user's data.
- **Delete all** — confirms once, then wipes the table and vacuums the database file so the
  text is not recoverable from free pages.
- **Failed rows** carry the reason inline: "not inserted — window was elevated".
- **Empty state**: "Nothing dictated yet. Hold right Option and speak."

Keyboard: arrow keys move, Enter copies, Cmd/Ctrl+Enter inserts, Delete removes, Escape closes,
typing filters. The panel is fully operable without a mouse.

## Storage

SQLite at `<app-data>/history.db`, file mode `0600`.

```sql
CREATE TABLE transcripts (
  id           INTEGER PRIMARY KEY,
  created_at   INTEGER NOT NULL,        -- unix millis
  text         TEXT NOT NULL,
  word_count   INTEGER NOT NULL,
  duration_ms  INTEGER NOT NULL,        -- audio length
  latency_ms   INTEGER NOT NULL,        -- release-to-text
  engine_id    TEXT NOT NULL,
  language     TEXT,
  target_app   TEXT,                    -- bundle id / exe name, for diagnostics
  outcome      TEXT NOT NULL,           -- inserted | clipboard_only
  outcome_note TEXT,                    -- failure reason when clipboard_only
  method       TEXT                     -- ax | paste | unicode | none
);
CREATE INDEX idx_created ON transcripts(created_at DESC);
CREATE VIRTUAL TABLE transcripts_fts USING fts5(text, content='transcripts', content_rowid='id');

-- Vocabulary learned from corrections (docs/LEARNING.md). Kilobytes, not megabytes.
-- Note what is absent: no surrounding text, ever.
CREATE TABLE vocab_candidates (
  id          INTEGER PRIMARY KEY,
  wrong_form  TEXT NOT NULL,
  right_form  TEXT NOT NULL,
  count       INTEGER NOT NULL DEFAULT 1,
  first_seen  INTEGER NOT NULL,
  last_seen   INTEGER NOT NULL,
  source_apps TEXT,                     -- app names only, for provenance in the UI
  reversals   INTEGER NOT NULL DEFAULT 0,
  state       TEXT NOT NULL             -- candidate | applied | suspended | rejected
);
```

**Audio is never stored.** It exists in memory during the pipeline and is dropped. A debug
setting can retain the last N recordings on disk for bug reports; it is off by default,
shows a persistent tray warning while on, and offers a one-click purge.

## Ranking, not filing

Most transcripts have a half-life of about four minutes. What people want is "the thing I just
said", and occasionally "that thing I said last Tuesday". Neither needs folders.

The list is ordered by a decayed score, not raw recency:

- Recency dominates for the first hour.
- Entries created while the **currently focused app** was the target rank above others of the
  same age — you are usually looking for what you said into this window.
- Failed insertions float, because those are the ones with unfinished business.
- Typing filters instantly across the full text.

There are no folders, tags, or favourites, and there will not be — see
[adr/0012](adr/0012-no-folders.md). Long-form output is routed to a real destination on
creation rather than filed here afterwards ([LONG-FORM.md](LONG-FORM.md)).

## Export

**Export everything** in Settings → Privacy writes the full history to a folder as `.md` (one
file per entry, or one combined file) and `.json` with every field. The learned vocabulary
exports as a plain list alongside it.

This exists because the realistic risk to a user of this app is that the project is abandoned,
not that a competitor wins. Nothing they accumulate should die with it. See
[VALUES.md](VALUES.md).

## Retention

Defaults: keep the most recent **200** items and nothing older than **30 days**, whichever bites
first. Pruning runs after each insert and at startup. Both limits are configurable, including
"unlimited", and history can be turned off entirely — in which case nothing is written at all,
and the panel says so rather than showing an empty list.

**Panic wipe**: a configurable hotkey (off by default) that deletes everything and vacuums,
with no confirmation. For users who need to be able to clear the device quickly.

## Clipboard rules

- Writes are marked private: `CanIncludeInClipboardHistory` = 0, `CanUploadToCloudClipboard` = 0,
  `ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore` on Windows;
  `org.nspasteboard.ConcealedType` on macOS. This is not a formal guarantee — a clipboard
  manager can ignore the hint — and the UI says so once, in Settings, rather than implying
  something stronger than it is.
- Save/restore around synthesised pastes follows the rules in
  [TEXT-INJECTION.md](TEXT-INJECTION.md): restore on evidence of a read, not a timer, and when
  in doubt leave the transcript on the clipboard rather than overwriting it.
- Only the plain-text flavour is written. No RTF, no HTML.

## Privacy posture

The history is plaintext on disk in v1.0, protected by file permissions. This is stated
plainly in Settings, next to the retention controls, because a user who assumes encryption and
doesn't have it is worse off than one who knows. Encryption at rest via the OS keychain is
tracked for v1.1 (see [PRD](PRD.md) Q3) — it needs a key-loss recovery story before it ships.
