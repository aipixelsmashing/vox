# Contributing

## Before you write code

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and the ADR index in
[docs/adr/](docs/adr/). Several decisions that look arbitrary are load-bearing — particularly
[0005](docs/adr/0005-no-unverified-injection-success.md) (never report unverified insertion
success) and [0007](docs/adr/0007-no-telemetry.md) (no telemetry).

For anything larger than a bug fix, open an issue first. This is a small project with strong
opinions about scope; a rejected 2000-line PR wastes your evening and our goodwill.

## What we're unlikely to merge

- Cloud model support, even behind a flag. It's the one thing the product exists to avoid, and
  a fork is the right home for it.
- Telemetry or analytics of any form.
- A plugin or scripting system in v1.x — the app holds accessibility permission and anything
  that runs third-party code inside it inherits that.
- LLM post-processing turned on by default, or any silent rewriting of what the user said.
- Anything that makes the app report success it can't verify.
- **Folders, tags or favourites for transcripts** — see [adr/0012](docs/adr/0012-no-folders.md).
- **A memory/speed setting.** If residency needs tuning, fix the heuristic
  ([adr/0010](docs/adr/0010-adaptive-residency.md)).
- Engagement mechanics of any kind — streaks, usage summaries, share prompts. See
  [docs/VALUES.md](docs/VALUES.md).
- A new data type with no export path ([adr/0015](docs/adr/0015-exit-is-cheap.md)).

## Setup

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md), including the platform-specific traps that
will otherwise cost you an afternoon.

**Working on the UI?** You don't need any of that. `pnpm dev:mock` runs every screen and every
state in a browser with no Rust toolchain, no model download, no microphone and no permission
prompts. See [docs/UI-DEVELOPMENT.md](docs/UI-DEVELOPMENT.md). Changing the UI↔core boundary
means changing the Rust command, `src/lib/contract.ts`, and the mock in
`src/mock/backend.ts` in the same PR — a test enforces it.

## Standards

- `cargo fmt` and `cargo clippy --all-targets -- -D warnings` must pass.
- New behaviour needs a test. Where a test isn't possible — anything involving a real key tap
  or a real application's text field — add a line to the manual gates in
  [docs/TESTING.md](docs/TESTING.md) instead, and say in the PR that you did.
- Changes to `audio`, `engine`, `pipeline` or `inject` must include benchmark results. CI
  fails a p50 latency regression or a 15% idle-RSS regression.
- Reliability work outranks feature work. If a PR adds a feature while the compatibility matrix
  has a red row, expect to be asked to fix the row first — see
  [docs/VALUES.md](docs/VALUES.md#priority-order-when-things-conflict).
- Conventional commit prefixes (`feat:`, `fix:`, `docs:`, `perf:`, `refactor:`, `test:`).
- Update the docs in the same PR. The docs are the specification, not a description written
  afterwards.

## Reporting insertion bugs

The most valuable bug reports in this project are "text doesn't appear in app X". Please
include: OS and version, display server on Linux, the application and its version, and the
diagnostics from the relevant history row (Settings → Diagnostics → copy details), which
records the method attempted and the outcome. Screenshots of the failure rarely help; the
method trace always does.

## Licence

Contributions are accepted under Apache-2.0. There is no CLA.
