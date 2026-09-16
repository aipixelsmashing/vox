# 0009 — "Vox" is a working name, and nothing depends on it

**Status:** accepted

## Context

"Vox" is short, pronounceable and obviously voice-related — and for exactly those reasons it is
heavily used, including by a large media brand. No trademark search has been done, and the name
may still change.

Separately, the original scaffolding used `com.vox.app` as the bundle identifier. That tied a
permanent, expensive-to-change string to a provisional name, and implied ownership of a domain
we do not hold.

## Decision

**Decouple the identity from the name.**

| | Value | Changeable |
| --- | --- | --- |
| Bundle identifier | `com.pixelsmashing.dictation` | No, after the first signed release |
| Data directory | Derived from the identifier, never the product name | No |
| Updater endpoint | `https://vox.pixelsmashing.com/updates/latest.json` | Redirect target, yes; URL, no |
| Display name, icons, site, repo | "Vox" for now | Yes, freely, forever |

The identifier's last segment says what the app *is*, not what it is called, so a rebrand never
touches it.

## Consequences

- The user-facing name can change at any point, including after release, at the cost of icons
  and copy. macOS permission grants are keyed to the code signature — Team ID plus bundle
  identifier — not the display name, so nobody re-grants Accessibility because of a rebrand.
  Twitter's iOS app has shipped as `com.atebits.Tweetie2` through two rebrands for the same
  reason.
- The identifier is built on a domain we actually own, so it is not claiming someone else's
  namespace — which matters for a project whose pitch is "check what we're doing".
- The updater points at a domain under our control that redirects to GitHub Releases. A repo
  rename, an org move, or leaving GitHub entirely does not break updates for installed clients.
  **Set up that redirect before the first signed release**, not after.
- Still open: the trademark search. It gates the public name, not the architecture, and now
  gates nothing else.

## Still to settle before M8

- Trademark search in software classes for the final name.
- The `vox.pixelsmashing.com` redirect live and serving `latest.json`.
- Icon and tray assets under the final name.
- Package names for Homebrew, winget and AUR — those are real renames with user friction, so
  publish to them only once the name is settled.

Criteria for the final name: no conflicting trademark in software classes, not already a
well-known app or crate, pronounceable by a non-English speaker, and searchable — "vox
dictation" is not.
