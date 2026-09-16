# 0007 — No telemetry, not even opt-in

**Status:** accepted

## Context

Product analytics would genuinely help: which apps insertion fails in, which models people
choose, real-world latency. The standard answer is anonymous opt-in telemetry.

## Decision

None. No analytics, no crash reporting service, no "help us improve" toggle. `telemetry.rs`
exists solely to be empty, and a test asserts it contains no network code.

## Consequences

- The privacy claim needs no asterisk, which is the entire product position. An opt-in toggle
  would mean every user has to evaluate a claim rather than read a guarantee.
- We are blind to real-world usage, so diagnostics must be *local and legible*: per-dictation
  timings and injection outcomes are stored in the history row, and Settings → Diagnostics
  shows them, so a user can paste a useful bug report by choice.
- Bug reports become more valuable and rarer; the compatibility matrix in
  [TESTING.md](../TESTING.md) partly substitutes for the feedback loop.
