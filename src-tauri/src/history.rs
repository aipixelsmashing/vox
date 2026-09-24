//! Local transcript store. Schema and behaviour: docs/HISTORY.md.
//!
//! Open, insert and prune serve the pipeline rule "every terminal path writes to history".
//! Search, ranking, delete, wipe and export serve the panel (M3).

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{params, Connection};

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM transcripts ORDER BY created_at DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map(params![limit], row_to_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> anyhow::Result<Option<Entry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM transcripts WHERE id = ?1"))?;
        let mut rows = stmt.query_map(params![id], row_to_entry)?;
        Ok(rows.next().transpose()?)
    }

    /// Full-text search, most recent first. Each word is a prefix match, all words must
    /// appear. Punctuation cannot break the query: every token is quoted for FTS5.
    pub fn search(&self, query: &str, limit: u32) -> anyhow::Result<Vec<Entry>> {
        let Some(fts) = fts_query(query) else {
            return self.recent(limit);
        };
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM transcripts
             WHERE id IN (SELECT rowid FROM transcripts_fts WHERE transcripts_fts MATCH ?1)
             ORDER BY created_at DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![fts, limit], row_to_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// One row, no confirmation: it is one row and it is the user's data.
    pub fn delete(&self, id: i64) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        Ok(conn.execute("DELETE FROM transcripts WHERE id = ?1", params![id])? > 0)
    }

    /// Everything, then VACUUM so the text is not recoverable from free pages.
    pub fn delete_all(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock();
        let n = conn.execute("DELETE FROM transcripts", [])?;
        conn.execute_batch("VACUUM;")?;
        Ok(n)
    }

    /// Writes every row to `dir` as `history.md` or `history.json` and returns the file.
    /// Every field goes into the JSON, so the export imports back without loss (adr/0015).
    pub fn export_to(&self, dir: &Path, format: ExportFormat) -> anyhow::Result<PathBuf> {
        let rows = self.recent(u32::MAX)?;
        let path = dir.join(match format {
            ExportFormat::Markdown => "history.md",
            ExportFormat::Json => "history.json",
        });
        let body = match format {
            ExportFormat::Json => serde_json::to_string_pretty(&rows)?,
            ExportFormat::Markdown => {
                let mut out = String::from("# Vox history\n\n");
                for r in &rows {
                    let when = chrono_like(r.created_at);
                    out.push_str(&format!(
                        "## {when}\n\n{}\n\n<sub>{} · {} words · {}</sub>\n\n",
                        r.text,
                        r.target_app.as_deref().unwrap_or("—"),
                        r.word_count,
                        r.outcome
                    ));
                }
                out
            }
        };
        std::fs::write(&path, body)?;
        Ok(path)
    }
}

/// A learned-vocabulary row (docs/LEARNING.md). Read here because it lives in the same
/// database; capture itself is M4.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabTerm {
    pub id: i64,
    pub wrong_form: String,
    pub right_form: String,
    pub count: u32,
    pub first_seen: i64,
    pub last_seen: i64,
    pub source_apps: Vec<String>,
    pub reversals: u32,
    pub state: String,
}

/// What Diagnostics shows: outcomes by app and the median release-to-text.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub by_app: Vec<(String, u32, u32)>,
    pub median_latency_ms: u32,
    pub total: u32,
}

impl Store {
    pub fn vocab_list(&self) -> anyhow::Result<Vec<VocabTerm>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, wrong_form, right_form, count, first_seen, last_seen, source_apps, reversals, state
             FROM vocab_candidates ORDER BY last_seen DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let apps: Option<String> = r.get(6)?;
            Ok(VocabTerm {
                id: r.get(0)?,
                wrong_form: r.get(1)?,
                right_form: r.get(2)?,
                count: r.get(3)?,
                first_seen: r.get(4)?,
                last_seen: r.get(5)?,
                source_apps: apps
                    .map(|a| {
                        a.split(',')
                            .filter(|x| !x.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                reversals: r.get(7)?,
                state: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Deletes the term and the evidence behind it: one row holds both (docs/LEARNING.md).
    pub fn vocab_forget(&self, id: i64) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        Ok(conn.execute("DELETE FROM vocab_candidates WHERE id = ?1", params![id])? > 0)
    }

    /// `vocabulary.txt`: one `wrong → right` per line, applied terms first.
    pub fn vocab_export_to(&self, dir: &Path) -> anyhow::Result<(PathBuf, usize)> {
        let mut terms = self.vocab_list()?;
        terms.sort_by_key(|t| (t.state != "applied", t.right_form.to_lowercase()));
        let body: String = terms
            .iter()
            .map(|t| {
                format!(
                    "{} → {} ({}, {}×)\n",
                    t.wrong_form, t.right_form, t.state, t.count
                )
            })
            .collect();
        let path = dir.join("vocabulary.txt");
        std::fs::write(&path, body)?;
        Ok((path, terms.len()))
    }

    pub fn stats(&self) -> anyhow::Result<Stats> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT COALESCE(target_app, ''),
                    SUM(CASE WHEN outcome = 'inserted' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN outcome <> 'inserted' THEN 1 ELSE 0 END)
             FROM transcripts GROUP BY target_app ORDER BY 2 DESC",
        )?;
        let by_app = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let total: u32 = conn.query_row("SELECT COUNT(*) FROM transcripts", [], |r| r.get(0))?;
        let median_latency_ms: u32 = if total == 0 {
            0
        } else {
            conn.query_row(
                "SELECT latency_ms FROM transcripts ORDER BY latency_ms LIMIT 1 OFFSET ?1",
                params![total / 2],
                |r| r.get(0),
            )?
        };
        Ok(Stats {
            by_app,
            median_latency_ms,
            total,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
}

const COLUMNS: &str = "id, created_at, text, word_count, duration_ms, latency_ms, engine_id, language, target_app, outcome, outcome_note, method";

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
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
}

/// `hello wor` → `"hello"* "wor"*`. None when there is nothing to search for.
fn fts_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

/// ISO-8601 in UTC, without pulling in a date crate for one line.
fn chrono_like(millis: i64) -> String {
    let secs = millis.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant), valid for any day in the Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// The order the panel shows (docs/HISTORY.md, "Ranking, not filing"): recency, with rows
/// dictated into the app the panel was opened over pulled ahead of others of similar age,
/// and failed insertions pulled further ahead still — those have unfinished business. The
/// boosts are minutes of virtual recency, so nothing older than an hour jumps the queue.
pub fn rank(mut entries: Vec<Entry>, current_app: Option<&str>, now: i64) -> Vec<Entry> {
    const APP_BOOST_MIN: f64 = 10.0;
    const FAILED_BOOST_MIN: f64 = 30.0;
    let score = |e: &Entry| {
        let age_min = (now - e.created_at) as f64 / 60_000.0;
        let app = match (current_app, e.target_app.as_deref()) {
            (Some(a), Some(b)) if a == b => APP_BOOST_MIN,
            _ => 0.0,
        };
        let failed = if e.outcome == "inserted" {
            0.0
        } else {
            FAILED_BOOST_MIN
        };
        age_min - app - failed
    };
    entries.sort_by(|a, b| score(a).total_cmp(&score(b)));
    entries
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
    fn search_is_prefix_and_punctuation_safe() {
        let store = Store::open_in_memory().unwrap();
        let now = now_millis();
        store
            .insert(&entry("deploy the auth middleware", now))
            .unwrap();
        store.insert(&entry("lunch at noon", now - 1)).unwrap();
        assert_eq!(store.search("auth", 10).unwrap().len(), 1);
        assert_eq!(
            store.search("dep midd", 10).unwrap().len(),
            1,
            "prefixes, all words"
        );
        assert_eq!(
            store.search("\"(auth\"", 10).unwrap().len(),
            1,
            "quotes stripped"
        );
        assert_eq!(
            store.search("   ", 10).unwrap().len(),
            2,
            "blank query lists all"
        );
    }

    #[test]
    fn delete_one_and_all() {
        let store = Store::open_in_memory().unwrap();
        let a = store.insert(&entry("a", now_millis())).unwrap();
        store.insert(&entry("b", now_millis())).unwrap();
        assert!(store.delete(a).unwrap());
        assert!(!store.delete(a).unwrap());
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(store.delete_all().unwrap(), 1);
        assert_eq!(store.count().unwrap(), 0);
        assert!(
            store.search("a", 10).unwrap().is_empty(),
            "fts index emptied too"
        );
    }

    #[test]
    fn ranking_prefers_current_app_and_floats_failures_within_the_hour() {
        let now = now_millis();
        let min = 60_000;
        let mut other = entry("other app, 1 min", now - min);
        other.target_app = Some("com.other".into());
        let same = entry("same app, 5 min", now - 5 * min);
        let mut failed = entry("failed, 20 min", now - 20 * min);
        failed.outcome = "clipboard_only".into();
        let mut old_failed = entry("failed, 2 h", now - 120 * min);
        old_failed.outcome = "clipboard_only".into();
        let ranked = rank(
            vec![old_failed, other, same, failed],
            Some("com.apple.Notes"),
            now,
        );
        let texts: Vec<&str> = ranked.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "failed, 20 min",
                "same app, 5 min",
                "other app, 1 min",
                "failed, 2 h"
            ]
        );
    }

    #[test]
    fn export_json_round_trips_every_field() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert(&entry("keep \"quotes\" & all", now_millis()))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = store.export_to(dir.path(), ExportFormat::Json).unwrap();
        let back: Vec<Entry> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].text, "keep \"quotes\" & all");
        assert_eq!(back[0].target_app.as_deref(), Some("com.apple.Notes"));
        let md = store.export_to(dir.path(), ExportFormat::Markdown).unwrap();
        assert!(std::fs::read_to_string(md)
            .unwrap()
            .contains("keep \"quotes\" & all"));
    }

    #[test]
    fn stats_group_by_app_and_take_the_median() {
        let store = Store::open_in_memory().unwrap();
        let now = now_millis();
        for (i, lat) in [100u32, 300, 200].iter().enumerate() {
            let mut e = entry("x", now - i as i64);
            e.latency_ms = *lat;
            store.insert(&e).unwrap();
        }
        let mut f = entry("y", now);
        f.outcome = "clipboard_only".into();
        f.target_app = Some("com.other".into());
        store.insert(&f).unwrap();
        let s = store.stats().unwrap();
        assert_eq!(s.total, 4);
        assert_eq!(s.median_latency_ms, 200);
        assert_eq!(s.by_app[0], ("com.apple.Notes".into(), 3, 0));
        assert_eq!(s.by_app[1], ("com.other".into(), 0, 1));
    }

    #[test]
    fn vocab_reads_forgets_and_exports() {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.conn.lock();
            conn.execute(
                "INSERT INTO vocab_candidates (wrong_form, right_form, count, first_seen, last_seen, source_apps, reversals, state)
                 VALUES ('cuber netties', 'Kubernetes', 3, 1, 2, 'Slack,Code', 0, 'applied')",
                [],
            )
            .unwrap();
        }
        let terms = store.vocab_list().unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].source_apps, vec!["Slack", "Code"]);
        let dir = tempfile::tempdir().unwrap();
        let (path, n) = store.vocab_export_to(dir.path()).unwrap();
        assert_eq!(n, 1);
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("cuber netties → Kubernetes"));
        assert!(store.vocab_forget(terms[0].id).unwrap());
        assert!(store.vocab_list().unwrap().is_empty());
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
