# Threat model

An app that observes every keystroke, records audio, and can drive other applications' user
interfaces is a high-value target. Being open source is not a security control by itself. This
document states what we protect, from whom, and what we deliberately do not defend against.

## Assets

| Asset | Sensitivity |
| --- | --- |
| Live audio of the user speaking | High — may contain anything, including credentials spoken aloud |
| Transcripts in memory | High |
| Transcript history on disk | High, and persistent |
| Clipboard contents during injection | High, and shared with the whole system |
| The app's own permissions (accessibility, input monitoring) | Very high — they are the prize for an attacker, independent of our data |
| Update channel and signing keys | Very high — compromise means arbitrary code on every install |

## Adversaries and mitigations

### Network observer / service operator

**Not applicable by design.** Audio never leaves the machine; there is no server. The only
outbound requests in the entire codebase are the model download and the update check, both
documented, both disableable, and `network.offlineLock` prevents the HTTP client from being
constructed at all. A test asserts no sockets are opened during a dictation cycle with the
lock on. This is the claim the product is built around, so it is enforced in code, not policy.

### Local malware or another user on the machine

- History database and settings are `0600`.
- Audio is never written to disk unless debug capture is explicitly enabled, which shows a
  persistent tray warning.
- Transcripts are never written to logs at any log level.
- **Not fully mitigated in v1.0**: the history is plaintext at rest, so a process running as
  the same user can read it. Stated plainly in the UI. Encryption at rest is v1.1 and needs a
  key-recovery story first. Full-disk encryption is the recommendation in the meantime.

### Clipboard snoopers and cloud sync

Clipboard writes attach the private/concealed formats described in
[TEXT-INJECTION.md](TEXT-INJECTION.md), keeping transcripts out of Windows clipboard history,
the Microsoft cloud clipboard, and cooperating macOS clipboard managers. This is a
cooperative hint, not enforcement — a hostile clipboard manager can ignore it — and the UI
says so rather than overpromising.

### Malicious or compromised dependency

The larger risk for this product than any runtime attacker.

- `cargo-deny` and `cargo-audit` gate CI; advisories fail the build.
- Dependencies pinned with a committed lockfile; updates reviewed, not auto-merged.
- No plugin system, no scripting engine, no arbitrary shell execution in v1.0. The app holds
  accessibility permission; anything that lets third-party code run inside it inherits that.
- Model files are SHA-256 verified against the registry before loading. A model is data fed to
  a native inference runtime, and a malformed file is an attack surface — verification is not
  optional and a mismatch is a hard failure.
- Release builds come from CI from a tagged commit; signing keys never touch a developer
  machine.

### Compromised update channel

- Update manifests are minisign-signed by `tauri-plugin-updater`; the private key lives only
  in CI secrets with an offline backup.
- macOS artefacts are additionally Developer-ID signed and notarised; Windows artefacts
  Authenticode signed.
- Key rotation procedure is documented in [PACKAGING.md](PACKAGING.md) *before* it is needed.
- Users can disable updates entirely and take releases manually.

### Abuse of Vox's own permissions

Because we hold accessibility permission, compromising Vox is more valuable than the data in
it. This shapes several decisions: no plugin system, no IPC socket that other local processes
can talk to, no elevated helper on Windows, no root daemon on Linux, and the smallest
practical set of entitlements on macOS.

### The user dictating something they didn't mean to

- Escape cancels mid-recording with nothing transcribed and nothing stored.
- The text around the caret in the focused field is read at key-down as recognition hints
  when `privacy.readFocusedField` is on ([CONTEXT.md](CONTEXT.md)). Focused field only, in
  memory for one dictation, sent only to Apple's on-device speech process, never stored or
  logged; a guard test enforces it. This is the user's document, not their speech, and is
  treated as the most sensitive thing Vox handles.
- Password fields and secure-input states are refused outright — the transcript is dropped,
  not even placed on the clipboard.
- History is capped and wipeable, with an optional panic-wipe hotkey.

## Out of scope

Stated so nobody assumes protection that isn't there:

- A compromised OS or a kernel-level keylogger. Nothing we do survives that.
- Physical access to an unlocked machine.
- Other applications with accessibility permission reading the same fields we do.
- Acoustic side channels — someone in the room can hear you dictate.
- Guaranteeing a third-party clipboard manager honours a privacy hint.

## Reporting

See [SECURITY.md](../SECURITY.md).
