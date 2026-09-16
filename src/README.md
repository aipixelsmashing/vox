# Frontend

Two lazily-created windows: the history panel and settings, plus the long-form panel, the
recording overlay, and onboarding. None of them is on the dictation critical path — the Rust
core does the work and the webview is only woken to display something. Keep it that way;
anything that makes the UI a dependency of a dictation costs latency on every use.

## Start here

```bash
pnpm dev:mock      # every screen, every state, in a browser — no Rust, no model, no mic
```

Then open `http://localhost:1420/?window=history&scenario=history-with-failures`, or use the
dev overlay to browse all of them.

| Document | What it gives you |
| --- | --- |
| [../docs/UI-DEVELOPMENT.md](../docs/UI-DEVELOPMENT.md) | The mock, scenarios, screenshot sheet |
| [../docs/UI-KIT.md](../docs/UI-KIT.md) | Tokens, and why each one is what it is |
| [../docs/UI-CONTRACT.md](../docs/UI-CONTRACT.md) | Every command and event |
| [../docs/UI-STATES.md](../docs/UI-STATES.md) | Every screen state, and every string |
| [../docs/UI-SPEC.md](../docs/UI-SPEC.md) | The intent behind all of it |

## Layout

```
src/
├── main.tsx              entry; routes to a window by ?window=
├── windows/
│   ├── HistoryPanel.tsx
│   ├── LongFormPanel.tsx
│   ├── RecordingOverlay.tsx
│   ├── Settings.tsx      six panes
│   └── Onboarding.tsx    six steps
├── lib/
│   ├── contract.ts       ← source of truth for the UI↔core boundary
│   ├── commands.ts       typed calls; routes to Tauri or the mock
│   ├── events.ts         one-way pushes from the core
│   └── copy.ts           every user-facing string, from the copy deck
├── mock/
│   ├── backend.ts        the mock core, incl. scripted dictations
│   ├── fixtures.ts       realistic data for every screen
│   └── scenarios.ts      one per state in UI-STATES.md
├── styles/tokens.css     design tokens
└── ui/                   small shared components
```

## Rules

1. **Only `lib/commands.ts` and `lib/events.ts` import `@tauri-apps/api`.** Enforced by lint.
   This is what keeps every screen buildable in a browser.
2. **Only tokens.** No hard-coded colours or sizes.
3. **Only the copy deck.** No strings invented in components — the same failure must read
   identically in a notification, a toast and a pane.
4. **Every list ships with an empty, loading and error state.** There is a scenario for each,
   so there is no excuse.
5. **No component library.** The UI is a list, a form and a menu. shadcn or MUI would be more
   code than the thing they build, and every dependency is one more thing shipped into a
   webview in a privacy tool.
