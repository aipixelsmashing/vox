# Settings

Stored as JSON at `<app-config>/settings.json`, mode `0600`. Plain text on purpose: a
privacy tool should let you read, diff and version-control your own configuration.

Unknown keys are preserved on write so a downgrade doesn't destroy newer settings. `version`
drives migrations; each migration is a pure function with a test.

## Schema

```jsonc
{
  "version": 1,

  "hotkey": {
    "keys": ["AltRight"],          // keytap key names; multiple = chord
    "mode": "hold",                // hold | toggle | double-tap-hold
    "minHoldMs": 120,              // shorter presses are ignored
    "consume": false,              // swallow the key system-wide (macOS/Windows only)
    "cancelKey": "Escape"
  },

  "audio": {
    "inputDevice": "default",      // device id, or "default" to follow the OS
    "preroll": "off",              // off | 300ms — see LATENCY.md
    "maxRecordingSec": 120,
    "vad": { "enabled": true, "trimSilence": true, "minSpeechMs": 250 }
  },

  "engine": {
    "modelId": "auto",             // "auto" picks the OS model on macOS 26+, Parakeet elsewhere
    "device": "auto",              // auto | cpu | gpu
    "language": "auto"             // auto | ISO code
    // No residency setting. The system unloads when idle and reloads predictively —
    // see docs/FOOTPRINT.md. Users should not be asked to trade memory against speed.
  },

  "learning": {
    "captureCorrections": true,    // store candidates locally; costs nothing, asset compounds
    "applyLearnedTerms": false,    // off by default for the first year — docs/LEARNING.md
    "minOccurrences": 3
  },

  "longForm": {
    "lockKey": "KeyL",             // pressed while the hotkey is held
    "maxSessionMin": 30,
    "defaultDestination": "clipboard",  // clipboard | new-file | insert | append-file
    "fileDirectory": null
  },

  "output": {
    "method": "auto",              // auto | accessibility | paste | type
    "restoreClipboard": true,
    "trailingSpace": true,
    "capitalizeFirst": false,
    "collapseNewlinesInTerminals": true,
    "onFocusChange": "clipboard",  // clipboard | insert-anyway
    "dictionary": [
      { "from": "kubernetes", "to": "Kubernetes" },
      { "from": "our company name", "to": "OurCompany" }
    ]
  },

  "privacy": {
    "readFocusedField": false      // opt in: read the text around the caret at key-down as up to
                                   // 20 recognition hints; never stored, never leaves the
                                   // machine — docs/CONTEXT.md
  },

  "history": {
    "enabled": true,
    "maxItems": 200,
    "maxDays": 30,
    "panelHotkey": "CmdOrCtrl+Shift+V",
    "panicWipeHotkey": null,
    "storeAudioForDebug": false
  },

  "network": {
    "updateCheck": "startup",      // startup | manual | off
    "offlineLock": false           // when true, no socket is opened for any reason
  },

  "ui": {
    "theme": "system",
    "soundCues": true,
    "levelOverlay": true,
    "launchAtLogin": true,
    "language": "system"
  },

  "advanced": {
    "logLevel": "warn",
    "diagnosticsPanel": false
  }
}
```

## Notable defaults and why

| Setting | Default | Reasoning |
| --- | --- | --- |
| `hotkey.keys` | `AltRight` | As specified. Onboarding proposes `ControlRight` instead when the layout maps right Alt to AltGr — see [HOTKEYS.md](HOTKEYS.md) |
| `hotkey.consume` | `false` | Eating a modifier system-wide is worse than the AltGr overlap |
| `audio.preroll` | `off` | Keeping the mic stream open is a privacy posture, not a default |
| `engine.modelId` | `auto` | Apple's model on macOS 26+ (no download, ~60 MB idle), Parakeet elsewhere |
| `learning.captureCorrections` | `true` | Local, kilobytes, and the corpus takes months to build. Delete it any time |
| `learning.applyLearnedTerms` | `false` | A system that learns silently can be confidently wrong. Earn the default with a year of data |
| `output.onFocusChange` | `clipboard` | Typing into whatever the user switched to is worse than not typing |
| `privacy.readFocusedField` | `false` | Vox reading the user's documents is a capability to opt into knowingly, like `learning.applyLearnedTerms`. In the Privacy pane because it is about what Vox may read, not a speed trade-off ([adr/0017](adr/0017-context-from-focused-field.md)) |
| `history.maxItems` | `200` | Enough to recover from a bad day, small enough that a wipe is quick |
| `network.updateCheck` | `startup` | Security updates matter for an app holding accessibility permission. One request, disableable, documented |
| `network.offlineLock` | `false` | Opt-in, because it also disables updates. Once on, it survives updates |

## Offline lock

`network.offlineLock: true` is enforced at the lowest practical level: the HTTP client is not
constructed at all, and any code path that would open a socket returns an error. Its state is
shown in the tray menu, and a test asserts that with the lock on, the process opens no sockets
during a full dictation cycle plus an update-check attempt.

## Settings UI

One window, four panes, no search, no nesting deeper than one level:

1. **Dictation** — hotkey (captured by pressing keys, not typing a string), mode, cancel key,
   input device, mic level meter, recording cap.
2. **Model** — installed models with size and languages, download/remove, import from folder,
   execution provider in use, warm/lazy.
3. **Text** — insertion method, clipboard restore, spacing and capitalisation, manual
   dictionary.
4. **Vocabulary** — every learned term with its provenance and a delete button, the learning
   toggle, and an export. Specified in [LEARNING.md](LEARNING.md#the-failure-mode-stated-plainly).
5. **Privacy** — reading the focused field for recognition hints ([CONTEXT.md](CONTEXT.md)),
   history retention, wipe, offline lock, update checks, **Export everything**, plus one plain
   paragraph stating exactly what the app sends and when.
6. **Diagnostics** — corrections per 100 words over time, insertion outcomes, per-stage
   timings, memory. The user's own numbers, computed locally, so they can judge whether this
   is earning its place.

Permissions status is a persistent strip at the top of the window when anything is missing,
with a button that opens the exact OS settings pane. Errors say what happened and what to do:
"Vox can't see the hotkey. Grant Input Monitoring, then quit and reopen Vox." — not
"Permission denied."
