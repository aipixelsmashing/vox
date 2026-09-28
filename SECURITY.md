# Security policy

## Reporting a vulnerability

Report privately through GitHub's **Report a vulnerability** button on the Security tab, or by
email to `aipixelsmashing@gmail.com` (PGP key in `docs/security-key.asc`). Please do not open a
public issue for a vulnerability.

Include what you can: affected version and platform, reproduction steps, and impact. A working
proof of concept helps but is not required.

**Response targets:** acknowledgement within 3 working days, an assessment within 10, and a fix
or a documented mitigation within 90 days for anything we accept as valid. If we disagree that
a report is a vulnerability we will say so, with reasoning.

Coordinated disclosure: we will credit you unless you prefer otherwise, and we will agree a
publication date with you rather than sitting on a fix indefinitely.

## Scope

In scope: the application, the update mechanism, the model download and verification path, the
history store, and the injection paths.

Particularly interesting to us, given what this app holds:

- Anything that gets code or data execution inside a process holding accessibility permission
- Bypasses of model hash verification
- Anything that causes transcripts to leave the machine
- Update manifest or signature verification weaknesses
- Transcripts leaking into logs, clipboard history, or crash artefacts

Out of scope: findings that require a compromised OS or root access; missing hardening that
has no exploit path; reports from automated scanners without demonstrated impact; the
inherent risk of granting accessibility permission to any application.

## Supported versions

The latest release, plus the previous minor version for security fixes only.

## Our own commitments

- Dependencies are gated by `cargo-deny` and `cargo-audit` in CI.
- Release artefacts are signed; the update manifest is minisign-signed and the key never
  touches a developer machine.
- No plugin system or scripting engine, deliberately — see
  [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md).
