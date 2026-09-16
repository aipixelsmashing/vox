# 0010 — Adaptive residency instead of a memory/speed setting

**Status:** accepted — supersedes the `keepWarm` / `idleUnloadMin` settings in 0004's original
runtime notes

## Context

A warm Parakeet int8 session is roughly 750 MB resident. Keeping it warm all day buys 0.5–2 s
on the first dictation after a gap and costs 750 MB for the 99% of the day nobody is speaking.
Idle resident memory is what decides whether a tray utility survives its first look in Activity
Monitor.

The original design kept the model warm always and exposed the trade-off as a setting.

## Decision

No residency setting. Memory-map the weights, unload after ~10 minutes idle, and reload
predictively on signals already available: an editable field gained focus, in an app the user
dictates into, at a time of day they usually dictate, or the first key of the hotkey chord went
down.

## Consequences

- Idle RSS target drops from ~750 MB to under 120 MB.
- mmap makes reload a few hundred milliseconds from page cache, which is what makes unloading
  acceptable rather than annoying — the two mechanisms only work together.
- Prediction can miss. Hit rate and p95 miss-wait are measured locally; below 80% the heuristic
  is wrong and gets fixed, not exposed as a toggle.
- Asking users to trade memory against speed is asking them to make an engineering judgement
  they have no basis for. Settings are where product decisions go to die.
- Follow-on question for spike S4: if reload is cheap, the right default model may be the
  *smallest* one that clears the accuracy bar rather than the most accurate one that fits.
