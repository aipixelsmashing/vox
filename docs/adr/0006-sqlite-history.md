# 0006 — SQLite for transcript history

**Status:** accepted

## Context

History needs ordered pagination, full-text search, per-row delete without rewriting
everything, and retention pruning. Options: JSON file, JSONL append log, SQLite.

## Decision

`rusqlite` with the `bundled` feature; FTS5 virtual table for search.

## Consequences

- Per-row delete and pruning are cheap and atomic, which matters because deletion is a privacy
  feature here, not a convenience.
- Bundled SQLite means no external runtime dependency in the packaged app.
- `VACUUM` after a wipe so deleted text is not recoverable from free pages — a plain file
  would need rewriting anyway.
- Adds ~1 MB to the binary. Fine.
- Not encrypted at rest in v1.0; see [THREAT-MODEL.md](../THREAT-MODEL.md). SQLCipher is the
  obvious upgrade path and does not change the schema.
