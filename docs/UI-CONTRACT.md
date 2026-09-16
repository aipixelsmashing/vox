# UI ↔ core contract

The boundary between the React frontend and the Rust core. `src/lib/contract.ts` is the
machine-readable version and the single source of truth; this document explains the rules.

**Change the contract, change three things in the same PR:** the Rust command, the TypeScript
type, and the mock implementation in `src/mock/backend.ts`. A test fails when a command exists
in `contract.ts` with no mock.

## Rules

1. **Nothing in the dictation path crosses this boundary.** Hotkey, capture, inference and
   insertion happen entirely in Rust. The webview is woken to display things. If a feature
   would put the UI on the critical path, it is the wrong design.
2. **Commands are verbs, and they either return data or throw.** No `{ ok: false }` envelopes.
3. **Errors are structured**, never strings. The UI needs to decide what to show, and parsing
   English is not a plan.
4. **Events are one-way and cheap.** State changes push; the UI never polls.
5. **Every list command paginates.** History can be unlimited.

## Commands

Grouped by pane. Full signatures in `contract.ts`.

### History
| Command | In | Out |
| --- | --- | --- |
| `history_list` | `{ query?, limit, before? }` | `HistoryEntry[]` |
| `history_delete` | `{ id }` | `void` |
| `history_delete_all` | — | `{ deleted: number }` |
| `history_copy` | `{ id }` | `void` |
| `history_reinsert` | `{ id }` | `InjectionOutcome` |
| `history_export` | `{ format: 'md' \| 'json' }` | `{ path }` |

### Vocabulary
| Command | In | Out |
| --- | --- | --- |
| `vocab_list` | `{ state? }` | `VocabTerm[]` |
| `vocab_forget` | `{ id }` | `void` — deletes the term *and* its evidence |
| `vocab_export` | — | `{ path }` |

### Settings, models, permissions
| Command | In | Out |
| --- | --- | --- |
| `settings_get` | — | `Settings` |
| `settings_set` | `Partial<Settings>` | `Settings` — returns the merged result, so the UI never guesses |
| `hotkey_capture_start` | — | `HotkeyBinding` — resolves on the next key-down |
| `models_list` | — | `ModelInfo[]` |
| `models_download` | `{ id }` | `void` — progress arrives on `vox://model-progress` |
| `models_import` | `{ path }` | `ModelInfo` |
| `models_remove` | `{ id }` | `void` |
| `permissions_status` | — | `PermissionReport` |
| `permissions_open_pane` | `{ which }` | `void` |
| `audio_devices` | — | `AudioDevice[]` |
| `diagnostics_recent` | `{ limit }` | `Diagnostics` |
| `export_everything` | `{ dest }` | `{ path, counts }` |

### Long-form
| Command | In | Out |
| --- | --- | --- |
| `longform_stop` | `{ destination }` | `{ path? }` |
| `longform_set_destination` | `{ destination }` | `void` |

## Events

| Event | Payload | Frequency |
| --- | --- | --- |
| `vox://state` | `PipelineState` | On every transition |
| `vox://level` | `{ rms: number }` | ~20 Hz, **only while recording** |
| `vox://longform-chunk` | `{ text, elapsedMs }` | Per transcription window |
| `vox://model-progress` | `{ id, received, total, phase }` | ~2 Hz during download |
| `vox://permissions-changed` | `PermissionReport` | On change and on wake |
| `vox://vocab-updated` | `{ added, suspended }` | When a term crosses a threshold |
| `vox://insertion-result` | `InjectionOutcome & { entryId }` | After each dictation |

`vox://level` is the only high-frequency event and it exists because the level meter answers
"is it hearing me?" — the question people have at that moment. It stops the instant recording
stops.

## Error model

```ts
type VoxError = {
  kind: 'permission' | 'model' | 'engine' | 'injection' | 'io' | 'offline-lock' | 'unsupported'
  detail: string          // for the log, not for the user
  userMessage: string     // what happened + what to do, from the copy deck
  actionLabel?: string    // e.g. "Open Accessibility settings"
  action?: 'open-permissions' | 'download-model' | 'retry' | 'open-settings'
}
```

The core supplies `userMessage` and `action` because the same failure surfaces in a
notification, a toast and a pane, and it must say the same thing in all three. Copy lives in
[UI-STATES.md](UI-STATES.md#copy-deck) and is generated into both languages from one source.

## Versioning

The contract carries `CONTRACT_VERSION`. On mismatch the UI renders a single message telling
the user to restart — an updated core with a stale webview is otherwise a source of baffling
bugs.
