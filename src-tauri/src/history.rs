//! Local transcript store. Schema and behaviour: docs/HISTORY.md.

pub struct Store {
    // conn: parking_lot::Mutex<rusqlite::Connection>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i64,
    pub created_at: i64,
    pub text: String,
    pub word_count: u32,
    pub duration_ms: u32,
    /// Release-to-text, the number docs/LATENCY.md is about.
    pub latency_ms: u32,
    pub engine_id: String,
    pub language: Option<String>,
    pub target_app: Option<String>,
    pub outcome: String,
    pub outcome_note: Option<String>,
    pub method: Option<String>,
}

impl Store {
    pub fn open(_dir: &std::path::Path) -> anyhow::Result<Self> {
        todo!("open with mode 0600, run migrations, create FTS5 index")
    }

    pub fn insert(&self, _entry: &Entry) -> anyhow::Result<i64> {
        todo!("insert, then prune to maxItems/maxDays")
    }

    pub fn search(&self, _query: &str, _limit: u32, _before: Option<i64>) -> anyhow::Result<Vec<Entry>> {
        todo!()
    }

    pub fn delete(&self, _id: i64) -> anyhow::Result<()> {
        todo!()
    }

    /// Deletes everything and VACUUMs, so text is not recoverable from free pages.
    pub fn wipe(&self) -> anyhow::Result<()> {
        todo!()
    }
}
