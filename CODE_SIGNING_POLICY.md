# Code signing policy

## Windows

Free code signing for Windows artefacts is provided by [SignPath.io](https://signpath.io),
with a certificate issued by the SignPath Foundation to this open-source project.

- **Committers and reviewers:** members of the project's GitHub organisation with write
  access, listed in `MAINTAINERS.md`.
- **Approvers:** the maintainers listed in `MAINTAINERS.md`.
- **Privacy policy:** [PRIVACY.md](PRIVACY.md). The application collects no user data and
  transmits nothing except the model download and update check described there.

Signing runs inside the release workflow on artefacts built by CI from a tagged commit.
Signing material is never present on a developer machine.

## macOS

Artefacts are signed with an Apple Developer ID Application certificate and notarised by
Apple. Certificate material lives only in CI secrets.

## Linux

Artefacts are unsigned by platform convention. Every release publishes SHA-256 checksums and a
detached minisign signature; verification instructions are in the release notes.

## Update manifests

All platforms: the updater manifest is minisign-signed. The private key is held in CI secrets
with an encrypted offline backup. Rotation is documented in
`docs/runbooks/rotate-update-key.md`.
