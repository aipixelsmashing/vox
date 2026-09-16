# What this project optimises for

Vox is not a business. There is no funnel, no conversion target, and no reason to want your
attention. That removes some pressures that shape most software, and the design should
actually reflect that rather than just claiming it.

## Success is the user thinking about Vox less

The goal is that someone types less and notices this app not at all. So:

- **No engagement mechanics.** No streaks, no word-count achievements, no "you dictated 40,000
  words this month" summaries. If someone dictates less this month because they had less to
  write, that is fine and the app has no opinion about it.
- **No notifications except failures.** Text arriving in the field is the success notification.
- **No growth surface.** No share prompts, no referral, no "rate this app".

## We tell people when they don't need this

Onboarding says, in plain words: *if this is your only Mac and you mostly dictate short
messages, macOS's built-in dictation is probably enough — here's how to turn it on.* A product
with revenue targets cannot write that sentence. We can, and it is the most trust-building
thing in the app.

Same rule in the README, and in answers to "is this better than X". Where the honest answer is
"probably not for you", say so.

## Exit must be cheap

The realistic risk to a user of this app is not a competitor. It is that the project is
abandoned in 2029 and nobody picks it up. The mitigation is making sure nothing they have
accumulated dies with it:

- History exports to plain text and JSON, in full, in one click.
- The learned vocabulary exports as a plain list.
- Settings are readable JSON, already.
- Models are standard files usable by other tools — no proprietary container.
- Uninstall documentation says exactly which directories to delete.

Design for being **survivable**, not sticky. If someone leaves for a better tool, the handover
should take a minute.

## Priority order when things conflict

1. **It never loses text.** Unglamorous, unscreenshottable, and the highest-value work in the
   project. One silent failure costs more trust than ten good features earn.
2. **Fewer corrections.** The pain that recurs on every single use.
3. **Length.** A job nobody else serves.
4. **Footprint and speed.** Background quality — noticed only when bad.

Feature work that outranks reliability work in a sprint is a mistake, however good the demo.

## Measure the user's experience, not the system's properties

Latency and memory are means. The metrics that matter are in
[PRD.md](PRD.md#success-criteria), they are computed locally, and they are shown to the person
in Diagnostics so they can decide for themselves whether this tool is earning its place.
