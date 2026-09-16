# 0015 — Exit is cheap

**Status:** accepted

## Context

This is not a commercial product. There is no retention target, and the realistic risk to a
user is not that a competitor wins — it is that this project is abandoned in a few years and
nobody picks it up.

## Decision

Everything the user accumulates leaves in one click, in formats nobody has to reverse-engineer:
history as plain `.md` and `.json`, learned vocabulary as a plain list, settings as readable
JSON already, models as standard files usable by other tools. Uninstall documentation names the
exact directories.

## Consequences

- Design for being **survivable** rather than sticky. If someone leaves for a better tool, the
  handover takes a minute.
- Rules out proprietary containers, an internal note format, and any storage decision that
  makes data easier for us and harder for everyone else.
- Costs a small amount of ongoing work — every new data type needs an export path, enforced by
  a test that fails when a table has no exporter.
- This is only comfortable because there is no business model to protect. Say so.
