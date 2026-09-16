# Handoff to Claude Code

Read this first, then [docs/VALUES.md](docs/VALUES.md), then
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), then the ADR index at
[docs/adr/](docs/adr/).

## What this is

A macOS tray app for push-to-talk dictation. Hold right Option, speak, release, and the text
appears in whatever field was focused. Everything runs on-device. See
[README.md](README.md).

The repository currently contains the full design and a scaffold: every Rust module exists with
its types, trait definitions and comments, and every function body is `todo!()`. The React UI
runs today against a mock backend. Your job is to fill in the bodies, in the order below.

## Scope for v1

**macOS only.** Apple SpeechAnalyzer as the only engine. Do not implement the Parakeet or
Whisper backends, the model downloader, the Windows or Linux injection paths, or the residency
predictor — all are designed for a later milestone and documented, and building them now is
wasted work. See [docs/adr/0016](docs/adr/0016-macos-first.md).

## Order of work

Follow [ROADMAP.md](ROADMAP.md). Each milestone has exit criteria; do not start the next one
until they are met. The order is deliberate — M2 (reliability) sits above every feature because
one silent insertion failure costs more trust than ten features earn.

Within a milestone, work in this order: types → tests → implementation. The types already
exist; write the test before the body wherever a test is possible.

## Stop and ask

These need a human. Do not guess, do not work around, do not stub past them.

1. **Anything in M0.** The four spikes are all "open a real application and observe". Ask for
   the findings; do not assume the answers.
2. **Permission grants.** macOS Accessibility and Input Monitoring cannot be granted
   programmatically. Ask the user to grant them and restart the app.
3. **Whether insertion works in a given application.** You cannot verify this. Build the
   mechanism, then ask for a compatibility matrix run.
4. **Any change to a decision recorded in an ADR.** If implementation reveals an ADR is wrong,
   say so and stop. Do not silently take the other path.
5. **Anything on the "will not merge" list** in [CONTRIBUTING.md](CONTRIBUTING.md).
6. **Apple Developer account, signing, notarisation.** Money and identity; not yours to do.

## Rules that are not negotiable

These come from ADRs and exist because breaking them is how this category of app fails.

- **`InjectionOutcome` has two variants.** `Inserted` or `ClipboardOnly { reason }`. If a path
  cannot confirm delivery, it returns `ClipboardOnly`. Never add a third variant, never return
  `Inserted` on an unverified path. ([adr/0005](docs/adr/0005-no-unverified-injection-success.md))
- **No telemetry.** `telemetry.rs` stays empty. No analytics, no crash reporting, no opt-in
  toggle. ([adr/0007](docs/adr/0007-no-telemetry.md))
- **Transcripts never appear in logs**, at any level.
- **Never learn from a single correction.** Three occurrences, two sessions, aligned local edits
  only. ([docs/LEARNING.md](docs/LEARNING.md))
- **No new setting** to resolve a design question. If the right answer depends on the situation,
  make the system decide. ([adr/0010](docs/adr/0010-adaptive-residency.md))
- **The UI never touches the dictation path.** Hotkey, capture, inference and insertion are
  entirely in Rust.
- **Only `src/lib/commands.ts` and `src/lib/events.ts` import `@tauri-apps/api`.**
- **Only tokens** from `src/styles/tokens.css`; **only copy** from the deck in
  [docs/UI-STATES.md](docs/UI-STATES.md).
- **Every data type gets an export path.** ([adr/0015](docs/adr/0015-exit-is-cheap.md))

## Where work happens

macOS keys its Accessibility and Input Monitoring grants to the **code signature**. A fresh
clone with a fresh build has a different signature, which means the permission has to be
granted again in System Settings and the app restarted. That single fact decides where each
kind of work belongs.

| Work | Environment | Why |
| --- | --- | --- |
| M0 spikes, M1, M2 — anything that launches the app, captures keys, or inserts into another application | **One persistent local checkout**, with a stable dev signing identity | The permission grant survives rebuilds only here |
| M3 onward — UI, history store, settings, export, tests, docs | **Parallel workspaces**, branch per slice, CI-gated | `pnpm dev:mock` needs no Rust, no model, no microphone and no permissions |

**Rule: never open a fresh workspace for work that has to run the app and hold a permission.**
If a fix touches `hotkey.rs`, `inject/`, `permissions.rs` or the pipeline's live behaviour, it
belongs in the persistent checkout. Opening a workspace for it costs an hour of re-granting and
proves nothing, because the thing being tested is whether it works on a real machine with real
permissions.

Corollary: agent work on those files can be *written* anywhere, but it is not *verified* until
it has run in the persistent checkout. Do not mark such work done on the strength of a clean
build.

## How to work on the UI

`pnpm dev:mock` runs every screen and all 28 states in a browser — no Rust, no model, no
microphone, no permissions. `src/windows/HistoryPanel.tsx` is the reference implementation;
follow its shape. Every screen needs its loading, empty and error states handled before it is
done, and there is a scenario for each in `src/mock/scenarios.ts`.

Changing the UI↔core boundary means changing three things in one commit: the Rust command,
`src/lib/contract.ts`, and the mock in `src/mock/backend.ts`.

## Definition of done, per change

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` pass
- New behaviour has a test, or a line added to the manual gates in
  [docs/TESTING.md](docs/TESTING.md) explaining why a test is not possible
- Docs updated in the same commit — the docs are the specification, not a description written
  afterwards
- If `audio`, `engine`, `pipeline` or `inject` changed: benchmark results included

## Where the answers are

| Question | Document |
| --- | --- |
| What are we optimising for? | [docs/VALUES.md](docs/VALUES.md) |
| What does v1 include? | [docs/PRD.md](docs/PRD.md) |
| How is it structured? | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Why this library? | [docs/TECH-STACK.md](docs/TECH-STACK.md) |
| How does insertion work, and fail? | [docs/TEXT-INJECTION.md](docs/TEXT-INJECTION.md) |
| How does the hotkey work? | [docs/HOTKEYS.md](docs/HOTKEYS.md) |
| What are the settings? | [docs/SETTINGS.md](docs/SETTINGS.md) |
| What states does each screen have? | [docs/UI-STATES.md](docs/UI-STATES.md) |
| What are the exact words for an error? | [docs/UI-STATES.md](docs/UI-STATES.md#copy-deck) |
| Why was X decided? | [docs/adr/](docs/adr/) |

If something is not written down, ask rather than deciding. An undocumented decision made
quietly is the thing this repository exists to prevent.
