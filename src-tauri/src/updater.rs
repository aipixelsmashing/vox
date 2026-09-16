//! Update checks. One HTTPS request, minisign-verified manifests, user confirms before
//! install. See docs/PACKAGING.md.

/// Returns Err when the offline lock is on: the HTTP client is not constructed at all, which
/// is what makes the guarantee in PRIVACY.md checkable rather than merely stated.
pub async fn check(_settings: &crate::settings::Network) -> anyhow::Result<Option<String>> {
    todo!()
}
