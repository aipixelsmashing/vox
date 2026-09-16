# Packaging, signing and updates

## Artefacts

| Platform | Formats | Notes |
| --- | --- | --- |
| macOS | `.dmg`, `.app` | Universal binary (arm64 + x86_64) |
| Windows | NSIS `.exe`, `.msi` | x64 and arm64 |
| Linux | `.AppImage`, `.deb`, `.rpm` | Flathub and AUR once the Wayland path settles |

Plus a **bundled-model variant** for macOS and Windows containing Whisper `base.en` (~60 MB
extra) so an air-gapped machine works on first launch. Its `NOTICE` carries the model
attribution.

Secondary channels, after the first stable release: Homebrew cask, winget, Scoop, AUR.
Not the Mac App Store — sandboxing is incompatible with system-wide accessibility insertion.

## Signing

**macOS.** Developer ID Application certificate, hardened runtime, then notarisation.
Tauri handles signing and submission when the environment provides `APPLE_SIGNING_IDENTITY`,
`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, and either `APPLE_ID` +
`APPLE_PASSWORD` + `APPLE_TEAM_ID` or the App Store Connect API key trio
(`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`). A free Apple Developer account
cannot notarise; users of an un-notarised build hit the "damaged app" dialog, so this is a
launch blocker, not a nice-to-have. Entitlements are listed in
[PERMISSIONS.md](PERMISSIONS.md); note that the accessibility grant is tied to the signature,
so a signing identity change invalidates every user's existing grant — a fact that belongs in
release notes if it ever happens.

**Windows.** Authenticode. Unsigned installers trigger SmartScreen, and an app that installs a
low-level keyboard hook without a signature will be quarantined by some antivirus products.
SignPath Foundation provides free certificates to open-source projects and is the intended
route; `CODE_SIGNING_POLICY.md` at the repository root documents the pipeline, which is a
requirement of that programme.

**Linux.** No central signing. Publish SHA-256 sums and a detached minisign signature
alongside every artefact; document verification in the release notes.

## Updates

`tauri-plugin-updater`, minisign-signed manifests, artefacts and `latest.json` on GitHub
Releases — reached through `https://vox.pixelsmashing.com/updates/latest.json`, a domain we
control that redirects there. Installed clients then survive a repo rename, an org move, or a
migration off GitHub entirely. The redirect must be live before the first signed release; see
[adr/0009](adr/0009-naming.md).

- `network.updateCheck: "startup"` by default — one HTTPS request at launch. This is a
  security-relevant app holding powerful permissions; silent staleness is its own risk.
- `"manual"` checks only when the menu item is clicked. `"off"` never checks.
- `network.offlineLock` overrides all of the above and prevents the HTTP client from existing.
- Updates never install without the user's confirmation; the release notes are shown first.
- The update endpoint is the only host contacted besides the model CDN, and both are listed in
  the Privacy pane.

**Key management.** The minisign private key lives in CI secrets with an encrypted offline
backup held by two maintainers. Losing it means no existing installation can verify a new
update — recovery requires users to reinstall manually, so the backup procedure is tested
annually. Rotation steps are written down in `docs/runbooks/rotate-update-key.md` before they
are needed rather than during an incident.

## Release process

1. Update `CHANGELOG.md`; bump version in `tauri.conf.json` (the source of truth).
2. Tag `vX.Y.Z`; push.
3. `release.yml` builds on macOS, Windows and Ubuntu runners, signs, notarises, and drafts a
   GitHub release with all artefacts, checksums, and a signed `latest.json`.
4. A maintainer runs the manual gates in [TESTING.md](TESTING.md) against the built artefacts —
   including the injection compatibility matrix and the hotkey checklist on each OS.
5. Publish. The updater picks it up on next check.

Pre-release channel: tags matching `vX.Y.Z-beta.N` publish to a separate `latest-beta.json`
that only users on the beta channel receive.

## Reproducibility

A committed `Cargo.lock` and pinned toolchain versions get us most of the way to comparable
builds. Full bit-for-bit reproducibility is not promised in v1.0 — it is worth doing for an app
of this sensitivity and is tracked as a roadmap item, not claimed prematurely.
