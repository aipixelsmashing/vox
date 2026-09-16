//! Everything out, in formats nobody has to reverse-engineer. See docs/adr/0015.
//!
//! The realistic risk to a user of this app is not a competitor — it is this project being
//! abandoned. Nothing they accumulate should die with it.
//!
//! tests/export_coverage.rs fails the build when a table has no exporter here.

pub struct Bundle {
    pub dir: std::path::PathBuf,
}

/// history/*.md  — one file per entry, plus a combined transcript
/// history.json  — every field, round-trippable
/// vocabulary.txt — plain list of learned terms
/// settings.json — copied as-is; it was already readable
pub fn export_all(_dest: &std::path::Path) -> anyhow::Result<Bundle> {
    todo!()
}

pub fn import_history(_path: &std::path::Path) -> anyhow::Result<usize> {
    todo!("round-trip target for the export test")
}
