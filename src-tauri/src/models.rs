//! Model registry, download, verification and sideloading. See docs/MODELS.md.

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Registry {
    pub schema_version: u32,
    pub models: Vec<ModelSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub engine: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub languages: Vec<String>,
    /// CI fails if any entry lacks a licence or attribution — several third-party repackages
    /// of Parakeet are relicensed CC-BY-NC and cannot be shipped.
    pub license: License,
    pub files: Vec<FileSpec>,
    pub min_ram_mb: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct License {
    pub spdx: String,
    pub attribution: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileSpec {
    pub name: String,
    pub sha256: String,
    pub urls: Vec<String>,
}

/// Resumable, staged, and hash-verified before the file is moved into place. A mismatch is
/// reported as a hash mismatch — never as a generic network error — and the partial is deleted.
pub async fn download(_spec: &ModelSpec, _on_progress: impl Fn(u64, u64)) -> anyhow::Result<()> {
    todo!()
}

/// Air-gapped install path: import a directory produced by `vox model export`, verified with
/// the same hashes as a download.
pub fn import_from_dir(_dir: &std::path::Path) -> anyhow::Result<()> {
    todo!()
}
