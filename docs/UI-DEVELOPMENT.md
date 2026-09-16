# Building the UI without the backend

The point of this setup: **you can build and review every screen in a browser, with no Rust
toolchain, no model download, no microphone, and no permissions.** Design work should not be
gated on the dictation engine existing.

```bash
pnpm dev:mock          # http://localhost:1420 — mock backend, no Tauri
pnpm dev:mock --open   # with the scenario switcher visible
pnpm tauri dev         # the real thing, when you need it
```

## How the mock works

`src/lib/commands.ts` is the only place the UI talks to the core. It checks for the Tauri
runtime and routes to either the real `invoke` or `src/mock/backend.ts`. Nothing else in the
UI knows which one it's talking to, and nothing in the UI imports `@tauri-apps/api` directly —
a lint rule enforces that.

```ts
import { commands } from '@/lib/commands'

const entries = await commands.history_list({ limit: 50 })
```

The mock returns fixtures from `src/mock/fixtures.ts`, emits the same events on a timer, and
simulates latency (150ms default) so you see the real loading behaviour rather than instant
data that hides it.

## Scenarios

Every state in [UI-STATES.md](UI-STATES.md) has a named scenario. Switch with a query
parameter or the dev overlay:

```
http://localhost:1420/?window=history&scenario=history-with-failures
http://localhost:1420/?window=settings&pane=vocabulary&scenario=vocab-suspended
http://localhost:1420/?window=onboarding&step=4&scenario=permissions-missing
```

The overlay (shift-clicked into view in mock mode) lists every scenario and every window, so
reviewing "all the empty states" is a minute's work, not an afternoon of contriving them.

Adding a state to the design means adding a scenario. `src/mock/scenarios.ts` is checked
against the table in UI-STATES.md by a test — a documented state with no scenario fails CI.

## Windows in one bundle

All windows are the same Vite app, routed by `?window=`. Tauri opens each with a different
query string. That keeps the build simple and means a designer can flip between the history
panel and settings without restarting anything.

| `?window=` | Screen |
| --- | --- |
| `history` | History panel |
| `settings` | Settings, plus `&pane=` |
| `onboarding` | Onboarding, plus `&step=` |
| `longform` | Long-form session panel |
| `overlay` | Recording overlay |

## Simulating the dictation loop

`?scenario=recording` runs a scripted dictation against the mock: state events fire in
sequence, the level meter animates against a recorded amplitude envelope, a transcript arrives
after a plausible delay, and an insertion outcome follows. You can build and tune the recording
indicator and the long-form panel entirely against this.

`?scenario=recording-fails` runs the same script ending in `ClipboardOnly`, which is the state
most likely to be under-designed because it is hard to trigger by hand.

## Conventions

- **Tokens, not values.** Every colour, size and radius comes from `src/styles/tokens.css`. A
  hard-coded hex fails review. Rationale in [UI-KIT.md](UI-KIT.md).
- **Copy comes from the deck.** Strings live in `src/lib/copy.ts`, generated from the table in
  [UI-STATES.md](UI-STATES.md). Inventing a string in a component means the same failure gets
  worded differently in a toast and a pane.
- **Every list has an empty state, a loading state, and an error state** before it is
  considered done. This is the most common review comment; the scenarios exist so it doesn't
  have to be.
- **Keyboard first.** The history panel is fully operable without a mouse; that is a
  requirement, not a polish item, and it is checked in the manual gates.
- No component library. The whole UI is a list, a form and a menu — shadcn or MUI would be more
  code than the thing they build, and every dependency is one more thing shipped to a webview
  in a privacy tool.

## Screenshot review

`pnpm ui:shots` renders every window × scenario to `.ui-shots/` at both themes and both
densities. Not an assertion-based visual regression suite — a contact sheet, so a design review
is one image rather than forty clicks. Diffing them against `main` is a manual read.

## When you do need the real backend

Three things the mock cannot fake, all of which belong to the manual gates in
[TESTING.md](TESTING.md): actual key capture, actual insertion into another app, and actual
permission prompts. Everything else — including all the failure states — is faster to build
against the mock.
