# Screen and state catalogue

Every screen, every state it can be in, and the exact words for each. Built for the person
implementing the UI: if a state is not in this list, the design is incomplete, not the code.

The mock backend ([UI-DEVELOPMENT.md](UI-DEVELOPMENT.md)) has a scenario for each row marked
**[S]**, so any state can be rendered in a browser in one click.

## Screens

| Screen | Window | Reached by |
| --- | --- | --- |
| Tray menu | Native, not web | Click the tray icon |
| History panel | Frameless, always-on-top, 380px | Tray, or `Cmd/Ctrl+Shift+Space` |
| Long-form panel | Frameless, resizable | Locking the hotkey |
| Recording overlay | Click-through, near the caret | Automatic while recording |
| Toast | Frameless, never focused, bottom centre, 4.5 s | Every failure notification |
| Settings | Ordinary window, 6 panes | Tray |
| Onboarding | Ordinary window, 6 steps | First launch |

## History panel

| State | What renders | **[S]** |
| --- | --- | --- |
| Populated | Rows ranked by decayed recency, current app boosted | `history-populated` |
| Empty, first run | "Nothing dictated yet. Hold right Option and speak." | `history-empty` |
| Empty, history disabled | "History is off. Turn it on in Settings → Privacy." | `history-disabled` |
| Search, no match | "No transcripts match *kubernetes*." | `history-no-match` |
| Contains failures | Failed rows float, marked with the reason inline | `history-with-failures` |
| Long transcript | Two lines, then fade. Full text on expand | `history-long` |
| Loading | Rows render progressively; no skeleton shimmer | `history-slow` |

## Recording overlay

A pill near the caret while a dictation is in progress; bottom centre when the caret cannot
be located. Click-through, never focused, gone the moment the text is placed.

| State | What renders | **[S]** |
| --- | --- | --- |
| Recording | Amber ring filling with the input level, elapsed time as `0:04` | `recording` |
| Working | Ring filled solid, "Transcribing…" then "Placing the text…" | `recording` |
| Ended in the clipboard | Same as working; the toast carries the reason, the pill just goes | `recording-fails` |
| Reduced motion | The ring becomes a static three-step meter; turn on Reduce Motion in System Settings → Accessibility → Display to review it | — |
| Turned off | `ui.levelOverlay` false: nothing, ever | — |

## Settings panes

| Pane | Key states | **[S]** |
| --- | --- | --- |
| Dictation | Capturing a binding; AltGr layout warning; device list empty | `hotkey-capturing`, `hotkey-altgr` |
| Model | Not downloaded; downloading with progress; verification failed; import; engine in use | `model-none`, `model-downloading`, `model-hash-fail` |
| Text | Manual dictionary empty vs populated | `dictionary-empty` |
| Vocabulary | No terms yet; terms with provenance; a suspended term; learning off | `vocab-empty`, `vocab-populated`, `vocab-suspended`, `vocab-off` |
| Privacy | Read-focused-field on/off; offline lock on/off; export running; wipe confirmation | `offline-locked` |
| Diagnostics | Corrections trend; insertion outcomes by app; not enough data yet | `diagnostics-thin`, `diagnostics-rich` |

## Onboarding

| State | What renders | **[S]** |
| --- | --- | --- |
| First launch, nothing granted | Step 1 with the "you may not need this" paragraph; step 2 shows every missing grant with a button | `onboarding-fresh` |
| Everything granted, at the last step | Step 3 with the two choices and the text box focused; a completed dictation lands in it and the step completes itself | `onboarding-ready` |
| A grant that needs a restart | Step 2 shows "Granted — reopen Vox to use it" with "Quit and reopen" | `permissions-restart` |
| Engine unavailable on this Mac | Step 2's engine row: "Needs macOS 26 or later" | `model-none` |
| AltGr layout | Step 3 opens with the Right Control suggestion | `hotkey-altgr` |

## Cross-cutting states

These can appear over any screen and are the ones most often forgotten:

| State | Behaviour | **[S]** |
| --- | --- | --- |
| A permission is missing | Persistent strip at the top of Settings, badge on the tray | `permissions-missing` |
| Permission granted, restart needed | Same strip, with a "Quit and reopen" button | `permissions-restart` |
| Model missing | Dictation disabled, tray badged, one-click fix | `model-none` |
| Engine crashed | One error, not one per attempt; engine restarts silently | `engine-crashed` |
| Offline lock on | Update and download controls disabled with an inline reason | `offline-locked` |
| Contract version mismatch | Single full-window message: restart Vox | `contract-mismatch` |
| Dictation in progress | History panel shows a live row at the top | `recording` |
| Text could not be placed | Toast at the bottom of the screen with the deck's reason, plus the system notification on signed builds | `recording-fails` |

## Copy deck

The contract is that **errors say what happened and what to do**. They do not apologise and
they are never vague. One string per situation, used identically in notification, toast and
pane.

| Situation | String |
| --- | --- |
| No editable field focused | "Copied. No text field was focused." |
| Focus changed mid-dictation | "Copied instead — you switched from Slack to Chrome while speaking." |
| Elevated window (Windows) | "Copied instead — Terminal is running as administrator. Press Ctrl+Shift+V to paste." |
| Secure input (macOS) | "Not inserted — a password field is active. Nothing was saved." |
| Wayland unverifiable | "Copied — your compositor doesn't allow typing into other apps. Press Ctrl+V." |
| Recording cap reached | "Stopped at 2 minutes. Transcribed what was recorded." |
| Input Monitoring missing | "Vox can't see the hotkey. Grant Input Monitoring, then quit and reopen Vox." |
| Accessibility missing | "Vox can't place text in other apps. Grant Accessibility, then quit and reopen Vox." |
| Microphone denied | "No microphone access. Turn it on in System Settings → Privacy → Microphone." |
| Linux input group | "Vox can't read the keyboard. Run `sudo usermod -aG input $USER`, then log out and back in." |
| Model missing | "No model installed. Download one, or import a folder." |
| Hash mismatch | "Download didn't verify — the file doesn't match its checksum. Try again or use a mirror." |
| Offline lock blocks an action | "Offline lock is on. Turn it off in Privacy to download." |
| Engine failed | "Transcription failed. The last recording was lost." |
| Term suspended | "You changed this back twice — Vox has stopped applying it." |
| Wipe confirmation | "Delete all 47 transcripts? This can't be undone." |
| Export finished | "Exported 47 transcripts and 12 learned words to ~/Documents/vox-export." |
| History database could not be read | "Couldn't read Vox's data. Try again." with the action "Try again" |
| A feature this build does not have | "That isn't available in this version of Vox." |
| Privacy pane and onboarding card, read-focused-field toggle (off by default) | "Read the field you're dictating into — Vox looks at the text around your cursor to recognise the names and terms you're likely to say. Read once per dictation, never stored, never leaves this Mac. Off in password fields." |

Empty states are invitations, not apologies:

| Screen | String |
| --- | --- |
| History | "Nothing dictated yet. Hold right Option and speak." |
| History, while a dictation is in progress (live row) | "Opening the microphone…" · "Listening…" · "Transcribing…" · "Placing the text…" |
| History, any list still loading | "Loading…" |
| Vocabulary, learning on | "No words learned yet. Vox picks them up when you correct it." |
| Vocabulary, learning off | "Learning is off. Vox won't watch your corrections. Turn it on to teach it your words." |
| Diagnostics, thin data | "Not enough dictations yet to show a trend." |
| Dictionary | "No custom words. Vox learns most of these on its own — add one here if you'd rather not wait." |

### Labels

Buttons and short controls, sentence case, no full stops. Row actions in the history panel:
"Copy", "Insert", "Delete" (rendered as ×, labelled for screen readers), "More" / "Less" on a
long transcript. Footer: "Delete all…" → "Delete all 47 transcripts? This can't be undone."
with "Delete" and "Cancel". After Copy without closing: "Copied". Counts: "47 items",
"34 words", singular "1 item", "1 word".

### Settings copy

The panes' labels and hints live in `src/lib/copy.ts` under `settings`, one block per pane,
and are the deck for that window; the situations that also surface elsewhere (permissions,
offline lock, empty states, wipe confirmation) reuse the strings above verbatim. Two strings
worth quoting because they carry a promise: the Model pane's "No memory-versus-speed
setting: Vox decides. If that ever feels wrong, that is a bug to report, not a knob to turn"
([adr/0010](adr/0010-adaptive-residency.md)), and the Privacy pane's "Vox makes one request:
the update check, which sends nothing but the request itself." ([PRIVACY.md](../PRIVACY.md)).

### Onboarding copy

Three steps, strings in `src/lib/copy.ts` under `onboarding`. The step-one paragraph is the
one [VALUES.md](VALUES.md#we-tell-people-when-they-dont-need-this) commits to: "Your Mac
already has dictation built in, and for short messages it's probably enough. It's in System
Settings → Keyboard → Dictation." The learning card is the place someone turns capture on
knowingly, and the read-the-field card sits beside it ([adr/0017](adr/0017-context-from-focused-field.md)).
