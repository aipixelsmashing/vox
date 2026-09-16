# 0012 — No folders, tags, or filing for transcripts

**Status:** accepted

## Context

An obvious request for a transcript history is organisation: folders, tags, favourites. It is
what a typical product team would build, and it demos well.

## Decision

None of it. Ranking, search, and destinations instead.

## Consequences

- Filing is work the user does for the system's benefit. Most transcripts have a half-life of
  about four minutes; almost none of them will ever be filed, and the feature would quietly
  accuse the user of being disorganised every time they opened the panel.
- The real needs are "the thing I just said" (recency plus the app currently focused) and
  "that thing last Tuesday" (search). Both are solved without a hierarchy.
- Long-form output is routed to a **destination** — clipboard, a file, a note — at the moment
  it is created, so it lives where the user's real work lives instead of accumulating here.
- This is also a scope defence: build folders and you have become a notes app competing with
  Obsidian and Granola, with a worse editor and no sync.
- If a user genuinely wants a library, export exists and their filesystem is better at this
  than we would be.
