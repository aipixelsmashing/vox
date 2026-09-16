# Runbook: rotating the update signing key

Written before it is needed, because the alternative is writing it during an incident.

## When

- Suspected compromise of the CI secret
- A maintainer with access leaves the project
- Routine rotation (annually)

## The constraint

Installed clients verify update manifests against the public key **compiled into the version
they are running**. A new key cannot be verified by old clients. So rotation is a two-release
process, and skipping the first release strands every existing install.

## Procedure

1. Generate a new keypair offline. Store the private key in the CI secret store and take an
   encrypted offline backup held by two maintainers.
2. **Release N** — signed with the *old* key, shipping the *new* public key in the binary.
   Every client that updates to N can now verify keys signed with the new key.
3. Wait. Track adoption; do not proceed until the long tail has moved or you accept stranding
   it. Announce in release notes and on the download page.
4. **Release N+1** — signed with the new key. Clients still on a pre-N version can no longer
   update automatically and must reinstall manually; the download page must say so plainly.
5. Revoke the old key from CI. Destroy the old backups.

## If the private key is lost rather than compromised

No new updates can be signed for existing installs at all. Recovery is: publish a new release
signed with a new key, and tell users to reinstall manually — website banner, release notes,
and a pinned issue. This is why the offline backup exists and why restoring from it is tested
annually rather than assumed to work.
