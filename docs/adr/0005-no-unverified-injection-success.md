# 0005 — Never report insertion success we cannot verify

**Status:** accepted

## Context

Synthetic input can be accepted by the OS and delivered nowhere: UIPI silently drops input into
elevated windows on Windows, and a Wayland compositor can accept events without routing them
to the focused surface. Tools that treat a successful API call as successful delivery produce
the worst possible failure — the user believes the text arrived, moves on, and loses it.

## Decision

`InjectionOutcome` has exactly two variants: `Inserted { method }` and
`ClipboardOnly { reason }`. Any path that cannot confirm delivery returns `ClipboardOnly`, and
the transcript stays on the clipboard with the user told why in one line.

## Consequences

- On Wayland without a confirmable path, Vox says "copied — press Ctrl+V" instead of pretending.
  Some users will read this as the app being worse than a competitor that lies. Accepted.
- Verification work per platform: read-back of the caret position after accessibility
  insertion on macOS, `WM_RENDERFORMAT` as proof of clipboard read on Windows.
- An enum with no "probably" variant, enforced by a test, so nobody adds one under deadline
  pressure without a deliberate discussion.
