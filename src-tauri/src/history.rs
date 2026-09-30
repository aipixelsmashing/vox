//! Local transcript store. Schema and behaviour: docs/HISTORY.md.
//!
//! Open, insert and prune serve the pipeline rule "every terminal path writes to history".
//! Search, ranking, delete, wipe and export serve the panel (M3).

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};

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
    /// How many recognition hints the engine session was given (docs/CONTEXT.md). The
    /// count only; the hints themselves are never stored anywhere.
    pub context_terms: u32,
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
  method       TEXT,
  context_terms INTEGER NOT NULL DEFAULT 0
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
  state       TEXT NOT NULL,
  sessions    INTEGER NOT NULL DEFAULT 1,
  last_session INTEGER,
  session_ids TEXT
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
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert(&self, entry: &Entry) -> anyhow::Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO transcripts (created_at, text, word_count, duration_ms, latency_ms, engine_id, language, target_app, outcome, outcome_note, method, context_terms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                entry.context_terms,
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
    /// Corrections to this right form over every way it was mangled, and the distinct
    /// sessions they were seen in. The same on every row that shares the right form.
    pub term_count: u32,
    pub term_sessions: u32,
    /// The right form has reached the threshold and is passed to the recogniser as a hint.
    pub hinted: bool,
}

/// What one correction did (docs/LEARNING.md, "Two thresholds"). The pair is the wrong form
/// with the right form; the term is the right form alone, however it was mangled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VocabObserved {
    pub count: u32,
    pub sessions: u32,
    /// This correction took the pair to applied.
    pub promoted: bool,
    pub term_count: u32,
    pub term_sessions: u32,
    /// This correction made the right form a hinted term.
    pub hinted: bool,
}

/// One pair's evidence, as the term-level sums need it.
struct PairEvidence {
    count: u32,
    sessions: u32,
    session_ids: Vec<i64>,
}

fn parse_session_ids(ids: Option<String>) -> Vec<i64> {
    ids.map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_default()
}

/// Corrections and distinct sessions for one right form, over all its pairs. A session two
/// pairs share counts once. A row from before session ids were kept knows how many
/// sessions it saw but not which; those it cannot name count as its own.
fn term_totals(pairs: &[PairEvidence]) -> (u32, u32) {
    let count = pairs.iter().map(|p| p.count).sum();
    let named: std::collections::BTreeSet<i64> = pairs
        .iter()
        .flat_map(|p| p.session_ids.iter().copied())
        .collect();
    let unnamed: u32 = pairs
        .iter()
        .map(|p| p.sessions.saturating_sub(p.session_ids.len() as u32))
        .sum();
    (count, named.len() as u32 + unnamed)
}

fn is_hinted(count: u32, sessions: u32, min_occurrences: u32) -> bool {
    count >= min_occurrences && sessions >= crate::learning::MIN_SESSIONS
}

/// What Diagnostics shows: outcomes by app, the median release-to-text, and how many
/// dictations went to the engine with recognition hints.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub by_app: Vec<(String, u32, u32)>,
    pub median_latency_ms: u32,
    pub total: u32,
    pub with_hints: u32,
}

impl Store {
    /// Every pair, newest first, each carrying its right form's totals. `min_occurrences`
    /// is the threshold a right form is hinted at.
    pub fn vocab_list_with(&self, min_occurrences: u32) -> anyhow::Result<Vec<VocabTerm>> {
        let mut terms = self.vocab_pairs()?;
        let conn = self.conn.lock();
        let mut totals: std::collections::HashMap<String, (u32, u32)> = Default::default();
        for t in &mut terms {
            let (count, sessions) = match totals.get(&t.right_form) {
                Some(hit) => *hit,
                None => {
                    let got = term_totals(&pairs_for(&conn, &t.right_form)?);
                    totals.insert(t.right_form.clone(), got);
                    got
                }
            };
            t.term_count = count;
            t.term_sessions = sessions;
            t.hinted = is_hinted(count, sessions, min_occurrences);
        }
        Ok(terms)
    }

    /// [`Store::vocab_list_with`] at the built-in threshold.
    pub fn vocab_list(&self) -> anyhow::Result<Vec<VocabTerm>> {
        self.vocab_list_with(crate::learning::MIN_OCCURRENCES)
    }

    /// The right forms that have reached the threshold, most recently corrected first:
    /// what the recogniser is given as hints. A pair the user kept changing back, or
    /// rejected, is no evidence for its right form.
    pub fn vocab_hinted(&self, min_occurrences: u32) -> anyhow::Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for t in self.vocab_list_with(min_occurrences)? {
            if t.hinted && !out.contains(&t.right_form) {
                out.push(t.right_form);
            }
        }
        Ok(out)
    }

    fn vocab_pairs(&self) -> anyhow::Result<Vec<VocabTerm>> {
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
                term_count: 0,
                term_sessions: 0,
                hinted: false,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Deletes the term and the evidence behind it: one row holds both (docs/LEARNING.md).
    pub fn vocab_forget(&self, id: i64) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        Ok(conn.execute("DELETE FROM vocab_candidates WHERE id = ?1", params![id])? > 0)
    }

    /// Every correction ever stored, candidates included, then VACUUM so the forms are not
    /// recoverable from free pages. The Privacy pane's delete button.
    pub fn vocab_forget_all(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock();
        let n = conn.execute("DELETE FROM vocab_candidates", [])?;
        conn.execute_batch("VACUUM;")?;
        Ok(n)
    }

    /// One observed correction (docs/LEARNING.md, "How capture works"). Upserts the pair
    /// matched on the wrong form case-insensitively and the right form exactly, counts it,
    /// counts the session if it is a new one, and adds the app name.
    ///
    /// Two thresholds, both `min_occurrences` corrections over at least
    /// [`crate::learning::MIN_SESSIONS`] sessions. The *pair* reaching it by itself becomes
    /// applied: a literal replacement. The *right form* reaching it over all its pairs,
    /// however it was mangled, becomes a hinted term; that is computed from the pairs
    /// and not stored, so deleting a pair takes its evidence with it.
    /// Only ever stores the two forms: the caller has already reduced the edit to them.
    pub fn vocab_observe(
        &self,
        wrong: &str,
        right: &str,
        app: &str,
        session: i64,
        now: i64,
        min_occurrences: u32,
    ) -> anyhow::Result<VocabObserved> {
        let conn = self.conn.lock();
        let (before_count, before_sessions) = term_totals(&pairs_for(&conn, right)?);
        let was_hinted = is_hinted(before_count, before_sessions, min_occurrences);
        let (count, sessions, promoted) =
            observe_pair(&conn, wrong, right, app, session, now, min_occurrences)?;
        let (term_count, term_sessions) = term_totals(&pairs_for(&conn, right)?);
        Ok(VocabObserved {
            count,
            sessions,
            promoted,
            term_count,
            term_sessions,
            hinted: !was_hinted && is_hinted(term_count, term_sessions, min_occurrences),
        })
    }

    /// `vocabulary.txt`: the hinted terms, one per line, then one `wrong → right` per
    /// line, applied pairs first.
    pub fn vocab_export_to(&self, dir: &Path) -> anyhow::Result<(PathBuf, usize)> {
        let mut terms = self.vocab_list()?;
        let mut hinted: Vec<(String, u32)> = Vec::new();
        for t in terms.iter().filter(|t| t.hinted) {
            if !hinted.iter().any(|(form, _)| *form == t.right_form) {
                hinted.push((t.right_form.clone(), t.term_count));
            }
        }
        hinted.sort_by_key(|(form, _)| form.to_lowercase());
        terms.sort_by_key(|t| (t.state != "applied", t.right_form.to_lowercase()));
        let mut body: String = hinted
            .iter()
            .map(|(form, count)| format!("{form} (hinted, {count}×)\n"))
            .collect();
        body.extend(terms.iter().map(|t| {
            format!(
                "{} → {} ({}, {}×)\n",
                t.wrong_form, t.right_form, t.state, t.count
            )
        }));
        let path = dir.join("vocabulary.txt");
        std::fs::write(&path, body)?;
        Ok((path, terms.len()))
    }
}

/// The evidence for one right form: its pairs, except those the user turned down.
fn pairs_for(conn: &Connection, right: &str) -> anyhow::Result<Vec<PairEvidence>> {
    let mut stmt = conn.prepare(
        "SELECT count, sessions, session_ids FROM vocab_candidates
         WHERE right_form = ?1 AND state NOT IN ('suspended', 'rejected')",
    )?;
    let rows = stmt.query_map(params![right], |r| {
        Ok(PairEvidence {
            count: r.get(0)?,
            sessions: r.get(1)?,
            session_ids: parse_session_ids(r.get(2)?),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The pair's own row and its own threshold. Returns (count, sessions, promoted).
fn observe_pair(
    conn: &Connection,
    wrong: &str,
    right: &str,
    app: &str,
    session: i64,
    now: i64,
    min_occurrences: u32,
) -> anyhow::Result<(u32, u32, bool)> {
    {
        let existing = conn
            .query_row(
                "SELECT id, count, sessions, last_session, source_apps, state, session_ids
                 FROM vocab_candidates
                 WHERE lower(wrong_form) = lower(?1) AND right_form = ?2",
                params![wrong, right],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, count, sessions, last_session, apps, state, ids)) = existing else {
            conn.execute(
                "INSERT INTO vocab_candidates
                   (wrong_form, right_form, count, first_seen, last_seen, source_apps, reversals, state, sessions, last_session, session_ids)
                 VALUES (?1, ?2, 1, ?3, ?3, ?4, 0, 'candidate', 1, ?5, ?6)",
                params![wrong, right, now, app, session, session.to_string()],
            )?;
            return Ok((1, 1, false));
        };
        let count = count + 1;
        let mut ids = parse_session_ids(ids);
        let sessions = if last_session == Some(session) || ids.contains(&session) {
            sessions
        } else {
            sessions + 1
        };
        if !ids.contains(&session) {
            ids.push(session);
        }
        let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let mut apps: Vec<String> = apps
            .map(|a| {
                a.split(',')
                    .filter(|x| !x.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if !app.is_empty() && !apps.iter().any(|a| a == app) {
            apps.push(app.to_string());
        }
        let promoted = state == "candidate"
            && count >= min_occurrences
            && sessions >= crate::learning::MIN_SESSIONS;
        let state = if promoted { "applied" } else { state.as_str() };
        conn.execute(
            "UPDATE vocab_candidates
             SET count = ?2, last_seen = ?3, source_apps = ?4, state = ?5, sessions = ?6, last_session = ?7, session_ids = ?8
             WHERE id = ?1",
            params![id, count, now, apps.join(","), state, sessions, session, ids],
        )?;
        Ok((count, sessions, promoted))
    }
}

impl Store {
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
        let with_hints: u32 = conn.query_row(
            "SELECT COUNT(*) FROM transcripts WHERE context_terms > 0",
            [],
            |r| r.get(0),
        )?;
        Ok(Stats {
            by_app,
            median_latency_ms,
            total,
            with_hints,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
}

const COLUMNS: &str = "id, created_at, text, word_count, duration_ms, latency_ms, engine_id, language, target_app, outcome, outcome_note, method, context_terms";

/// Columns added after a database may already exist. `CREATE TABLE IF NOT EXISTS` does not
/// touch an existing table, so each addition is checked against `PRAGMA table_info` and
/// added when missing. Every entry is a pure statement with a test.
fn migrate(conn: &Connection) -> anyhow::Result<()> {
    let has_context: bool = conn
        .prepare("PRAGMA table_info(transcripts)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "context_terms");
    if !has_context {
        conn.execute_batch(
            "ALTER TABLE transcripts ADD COLUMN context_terms INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // M4: the session columns behind "three occurrences across two sessions". Rows from
    // before count as one session, which cannot promote anything on its own.
    let vocab_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(vocab_candidates)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !vocab_columns.iter().any(|c| c == "sessions") {
        conn.execute_batch(
            "ALTER TABLE vocab_candidates ADD COLUMN sessions INTEGER NOT NULL DEFAULT 1;",
        )?;
    }
    if !vocab_columns.iter().any(|c| c == "last_session") {
        conn.execute_batch("ALTER TABLE vocab_candidates ADD COLUMN last_session INTEGER;")?;
    }
    // Which sessions, not only how many: a right form's sessions are counted over all its
    // pairs, and two pairs fixed in one sitting must count that sitting once. A row from
    // before names the one session it still knows.
    if !vocab_columns.iter().any(|c| c == "session_ids") {
        conn.execute_batch(
            "ALTER TABLE vocab_candidates ADD COLUMN session_ids TEXT;
             UPDATE vocab_candidates SET session_ids = CAST(last_session AS TEXT)
             WHERE last_session IS NOT NULL;",
        )?;
    }
    Ok(())
}

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
        context_terms: r.get(12)?,
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
            context_terms: 0,
        }
    }

    #[test]
    fn a_database_from_before_context_terms_gets_the_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE transcripts (
               id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL, text TEXT NOT NULL,
               word_count INTEGER NOT NULL, duration_ms INTEGER NOT NULL, latency_ms INTEGER NOT NULL,
               engine_id TEXT NOT NULL, language TEXT, target_app TEXT, outcome TEXT NOT NULL,
               outcome_note TEXT, method TEXT);
             INSERT INTO transcripts VALUES (1, 1, 'old row', 2, 1000, 200, 'speechanalyzer', NULL, NULL, 'inserted', NULL, 'ax');",
        )
        .unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let store = Store {
            conn: Mutex::new(conn),
        };
        let rows = store.recent(10).unwrap();
        assert_eq!(rows[0].text, "old row");
        assert_eq!(rows[0].context_terms, 0, "old rows read as unhinted");
        let mut e = entry("new row", 2);
        e.context_terms = 7;
        store.insert(&e).unwrap();
        assert_eq!(store.recent(1).unwrap()[0].context_terms, 7);
        assert_eq!(store.stats().unwrap().with_hints, 1);
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

    fn pair(seen: VocabObserved) -> (u32, u32, bool) {
        (seen.count, seen.sessions, seen.promoted)
    }

    #[test]
    fn a_session_two_pairs_share_counts_once_for_the_right_form() {
        let p = |count, sessions, ids: &[i64]| PairEvidence {
            count,
            sessions,
            session_ids: ids.to_vec(),
        };
        assert_eq!(term_totals(&[]), (0, 0));
        assert_eq!(term_totals(&[p(1, 1, &[10]), p(1, 1, &[10])]), (2, 1));
        assert_eq!(term_totals(&[p(2, 2, &[10, 20]), p(1, 1, &[20])]), (3, 2));
        // From before session ids were kept: three sessions, one of them named.
        assert_eq!(term_totals(&[p(4, 3, &[20]), p(1, 1, &[20])]), (5, 3));
        assert_eq!(term_totals(&[p(2, 1, &[])]), (2, 1));
    }

    #[test]
    fn a_table_from_before_session_ids_names_the_session_it_knows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE vocab_candidates (
               id INTEGER PRIMARY KEY, wrong_form TEXT NOT NULL, right_form TEXT NOT NULL,
               count INTEGER NOT NULL DEFAULT 1, first_seen INTEGER NOT NULL, last_seen INTEGER NOT NULL,
               source_apps TEXT, reversals INTEGER NOT NULL DEFAULT 0, state TEXT NOT NULL,
               sessions INTEGER NOT NULL DEFAULT 1, last_session INTEGER);
             INSERT INTO vocab_candidates VALUES (1, 'Eddie', 'Adi', 1, 1, 1, 'Notes', 0, 'candidate', 1, 100);
             INSERT INTO vocab_candidates VALUES (2, 'AD', 'Adi', 1, 2, 2, 'Notes', 0, 'candidate', 1, 100);
             INSERT INTO vocab_candidates VALUES (3, 'A de', 'Adi', 1, 3, 3, 'Notes', 0, 'candidate', 1, 200);",
        )
        .unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let store = Store {
            conn: Mutex::new(conn),
        };
        // Three corrections to one name over two sittings, each mangled its own way.
        assert_eq!(store.vocab_hinted(3).unwrap(), vec!["Adi".to_string()]);
        let seen = store
            .vocab_observe("eddie", "Adi", "Notes", 200, 4, 3)
            .unwrap();
        assert_eq!((seen.count, seen.sessions), (2, 2));
        assert_eq!((seen.term_count, seen.term_sessions), (4, 2));
        assert!(!seen.hinted, "it already was");
    }

    #[test]
    fn a_pair_the_user_turned_down_is_no_evidence_for_its_right_form() {
        let store = Store::open_in_memory().unwrap();
        store
            .vocab_observe("Eddie", "Adi", "Notes", 1, 10, 3)
            .unwrap();
        store.vocab_observe("AD", "Adi", "Notes", 1, 11, 3).unwrap();
        store
            .vocab_observe("A de", "Adi", "Notes", 2, 12, 3)
            .unwrap();
        assert_eq!(store.vocab_hinted(3).unwrap(), vec!["Adi".to_string()]);
        store
            .conn
            .lock()
            .execute(
                "UPDATE vocab_candidates SET state = 'suspended' WHERE wrong_form = 'AD'",
                [],
            )
            .unwrap();
        assert!(store.vocab_hinted(3).unwrap().is_empty());
    }

    #[test]
    fn the_export_lists_hinted_terms_then_the_pairs() {
        let store = Store::open_in_memory().unwrap();
        store
            .vocab_observe("Eddie", "Adi", "Notes", 1, 10, 3)
            .unwrap();
        store.vocab_observe("AD", "Adi", "Notes", 1, 11, 3).unwrap();
        store
            .vocab_observe("A de", "Adi", "Notes", 2, 12, 3)
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (path, n) = store.vocab_export_to(dir.path()).unwrap();
        assert_eq!(n, 3);
        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.starts_with("Adi (hinted, 3×)\n"), "{body}");
        assert_eq!(body.matches(" → Adi (candidate, 1×)").count(), 3);
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
    fn a_vocab_table_from_before_sessions_gets_the_columns_and_counts_as_one() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE vocab_candidates (
               id INTEGER PRIMARY KEY, wrong_form TEXT NOT NULL, right_form TEXT NOT NULL,
               count INTEGER NOT NULL DEFAULT 1, first_seen INTEGER NOT NULL, last_seen INTEGER NOT NULL,
               source_apps TEXT, reversals INTEGER NOT NULL DEFAULT 0, state TEXT NOT NULL);
             INSERT INTO vocab_candidates VALUES (1, 'prea', 'Priya', 2, 1, 2, 'Slack', 0, 'candidate');",
        )
        .unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let store = Store {
            conn: Mutex::new(conn),
        };
        // Two old corrections count as one session: a third in a new session promotes.
        assert_eq!(
            pair(
                store
                    .vocab_observe("Prea", "Priya", "Code", 7, 3, 3)
                    .unwrap()
            ),
            (3, 2, true)
        );
        let t = &store.vocab_list().unwrap()[0];
        assert_eq!(t.state, "applied");
        assert_eq!(t.source_apps, vec!["Slack", "Code"]);
        assert_eq!(t.wrong_form, "prea", "the first-seen spelling is kept");
    }

    #[test]
    fn observe_counts_sessions_distinctly_and_never_repromotes() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            pair(store.vocab_observe("a", "B", "Notes", 1, 10, 3).unwrap()),
            (1, 1, false)
        );
        assert_eq!(
            pair(store.vocab_observe("a", "B", "Notes", 1, 11, 3).unwrap()),
            (2, 1, false)
        );
        assert_eq!(
            pair(store.vocab_observe("a", "B", "", 2, 12, 3).unwrap()),
            (3, 2, true)
        );
        assert_eq!(
            pair(store.vocab_observe("a", "B", "Notes", 3, 13, 3).unwrap()),
            (4, 3, false),
            "already applied"
        );
        assert_eq!(
            pair(store.vocab_observe("a", "C", "Notes", 3, 13, 3).unwrap()),
            (1, 1, false),
            "a different right form is its own row"
        );
        let terms = store.vocab_list().unwrap();
        assert_eq!(terms.len(), 2);
        let applied = terms.iter().find(|t| t.right_form == "B").unwrap();
        assert_eq!((applied.first_seen, applied.last_seen), (10, 13));
        assert_eq!(
            applied.source_apps,
            vec!["Notes"],
            "an empty app name is not provenance"
        );
    }

    #[test]
    fn forget_all_empties_the_table() {
        let store = Store::open_in_memory().unwrap();
        store.vocab_observe("a", "B", "Notes", 1, 10, 3).unwrap();
        store.vocab_observe("c", "D", "Notes", 1, 10, 3).unwrap();
        assert_eq!(store.vocab_forget_all().unwrap(), 2);
        assert!(store.vocab_list().unwrap().is_empty());
        assert_eq!(store.vocab_forget_all().unwrap(), 0);
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
