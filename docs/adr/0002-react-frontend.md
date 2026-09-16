# 0002 — React for the small amount of UI

**Status:** accepted

## Context

The UI is a history list, a settings form, and an onboarding flow. Svelte would produce a
smaller bundle and slightly simpler code; React has a much larger contributor pool.

## Decision

React 19 + TypeScript + Vite + Tailwind 4.

## Consequences

- Optimising for contributors rather than bytes, because the UI is not on the critical path
  and its size does not affect dictation latency.
- Nothing in the Rust core depends on the frontend framework; the command surface is plain
  JSON. Swapping to Svelte later is a self-contained change, which is why this decision is
  recorded rather than assumed.
