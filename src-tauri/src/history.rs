//! Local transcript store. Schema and behaviour: docs/HISTORY.md.
//!
//! M1 implements what the pipeline rule "every terminal path writes to history" needs: open,
//! insert, prune. Search, ranking, delete, wipe and export land with the panel in M3.

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{params, Connection};

pub struct Store {
    conn: Mutex<Connection>,
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

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS transcripts (
  id           INTEGER PRIMARY KEY,
  created_at   INTEGER NOT NULL,
  text         TEXT NOT NULL,
  word_count   INTEGER NOT NULL,
  duration_ms  INTEGER NOT NULL,
  latency_ms   INTEGER NOT NULL,
  engine_id    TEXT NOT NULL,
  language     TEXT,
  target_app   TEXT,
  outcome      TEXT NOT NULL,
  outcome_note TEXT,
  method       TEXT
);
CREATE INDEX IF NOT EXISTS idx_created ON transcripts(created_at DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(text, content='transcripts', content_rowid='id');
CREATE TRIGGER IF NOT EXISTS transcripts_ai AFTER INSERT ON transcripts BEGIN
  INSERT INTO transcripts_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS transcripts_ad AFTER DELETE ON transcripts BEGIN
  INSERT INTO transcripts_fts(transcripts_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TABLE IF NOT EXISTS vocab_candidates (
  id          INTEGER PRIMARY KEY,
  wrong_form  TEXT NOT NULL,
  right_form  TEXT NOT NULL,
  count       INTEGER NOT NULL DEFAULT 1,
  first_seen  INTEGER NOT NULL,
  last_seen   INTEGER NOT NULL,
  source_apps TEXT,
  reversals   INTEGER NOT NULL DEFAULT 0,
  state       TEXT NOT NULL
);
"#;

impl Store {
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join("history.db");
        let conn = Connection::open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert(&self, entry: &Entry) -> anyhow::Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO transcripts (created_at, text, word_count, duration_ms, latency_ms, engine_id, language, target_app, outcome, outcome_note, method)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                entry.created_at,
                entry.text,
                entry.word_count,
                entry.duration_ms,
                entry.latency_ms,
                entry.engine_id,
                entry.language,
                entry.target_app,
                entry.outcome,
                entry.outcome_note,
                entry.method,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Keep the most recent `max_items` and nothing older than `max_days`, whichever bites
    /// first. Zero means unlimited for either.
    pub fn prune(&self, max_items: u32, max_days: u32) -> anyhow::Result<usize> {
        let conn = self.conn.lock();
        let mut removed = 0;
        if max_days > 0 {
            let cutoff = now_millis() - i64::from(max_days) * 86_400_000;
            removed += conn.execute(
                "DELETE FROM transcripts WHERE created_at < ?1",
                params![cutoff],
            )?;
        }
        if max_items > 0 {
            removed += conn.execute(
                "DELETE FROM transcripts WHERE id NOT IN (SELECT id FROM transcripts ORDER BY created_at DESC LIMIT ?1)",
                params![max_items],
            )?;
        }
        Ok(removed)
    }

    pub fn count(&self) -> anyhow::Result<u32> {
        let conn = self.conn.lock();
        Ok(conn.query_row("SELECT COUNT(*) FROM transcripts", [], |r| r.get(0))?)
    }

    pub fn recent(&self, limit: u32) -> anyhow::Result<Vec<Entry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, text, word_count, duration_ms, latency_ms, engine_id, language, target_app, outcome, outcome_note, method
             FROM transcripts ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(Entry {
                id: r.get(0)?,
                created_at: r.get(1)?,
                text: r.get(2)?,
                word_count: r.get(3)?,
                duration_ms: r.get(4)?,
                latency_ms: r.get(5)?,
                engine_id: r.get(6)?,
                language: r.get(7)?,
                target_app: r.get(8)?,
                outcome: r.get(9)?,
                outcome_note: r.get(10)?,
                method: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn word_count(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, created_at: i64) -> Entry {
        Entry {
            id: 0,
            created_at,
            text: text.into(),
            word_count: word_count(text),
            duration_ms: 1000,
            latency_ms: 200,
            engine_id: "speechanalyzer".into(),
            language: Some("en_US".into()),
            target_app: Some("com.apple.Notes".into()),
            outcome: "inserted".into(),
            outcome_note: None,
            method: Some("ax".into()),
        }
    }

    #[test]
    fn insert_and_read_back() {
        let store = Store::open_in_memory().unwrap();
        let id = store.insert(&entry("hello there", now_millis())).unwrap();
        assert!(id > 0);
        let rows = store.recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "hello there");
        assert_eq!(rows[0].word_count, 2);
    }

    #[test]
    fn prune_by_count_and_age() {
        let store = Store::open_in_memory().unwrap();
        let now = now_millis();
        for i in 0..5 {
            store
                .insert(&entry(&format!("t{i}"), now - i * 1000))
                .unwrap();
        }
        store
            .insert(&entry("ancient", now - 40 * 86_400_000))
            .unwrap();
        assert_eq!(store.count().unwrap(), 6);
        store.prune(3, 30).unwrap();
        let rows = store.recent(10).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].text, "t0");
        assert!(rows.iter().all(|r| r.text != "ancient"));
    }

    #[test]
    fn zero_means_unlimited() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..3 {
            store.insert(&entry("x", now_millis() - i)).unwrap();
        }
        assert_eq!(store.prune(0, 0).unwrap(), 0);
        assert_eq!(store.count().unwrap(), 3);
    }
}
