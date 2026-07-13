use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::Result;

const SCHEMA: &str = include_str!("schema.sql");

pub struct Store {
    conn: Connection,
}

/// A normalized usage event, provider-agnostic (see DESIGN.md data model).
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub session_id: String,
    pub ts: String,
    pub granularity: Granularity,
    pub model: Option<String>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost: Option<f64>,
    /// Provider-specific idempotency key; re-ingesting the same record is a no-op.
    pub dedup_key: Option<String>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Turn,
    Aggregate,
}

impl Granularity {
    fn as_str(self) -> &'static str {
        match self {
            Granularity::Turn => "turn",
            Granularity::Aggregate => "aggregate",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DailyUsage {
    pub day: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

#[derive(Debug, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

#[derive(Debug, Serialize, Default)]
pub struct Overview {
    pub sessions: i64,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub db_path: Option<String>,
}

impl Store {
    /// Open (creating if needed) the store at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Default on-disk location: XDG data dir (e.g. ~/.local/share/fp-artificiency).
    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("fp-artificiency")
            .join("artificiency.db")
    }

    pub fn upsert_session(
        &self,
        id: &str,
        provider: &str,
        project: Option<&str>,
        started_at: Option<&str>,
    ) -> Result<()> {
        // Keep the earliest started_at; fill project if it was unknown.
        self.conn.execute(
            "INSERT INTO sessions (id, provider, project, started_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               project = COALESCE(sessions.project, excluded.project),
               started_at = MIN(COALESCE(sessions.started_at, excluded.started_at),
                                COALESCE(excluded.started_at, sessions.started_at))",
            params![id, provider, project, started_at],
        )?;
        Ok(())
    }

    /// Insert an event; returns false if the dedup key already existed.
    pub fn insert_event(&self, ev: &NewEvent) -> Result<bool> {
        let meta = ev.meta.as_ref().map(|m| m.to_string());
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO events
               (session_id, ts, granularity, model, tokens_in, tokens_out,
                cache_read, cache_write, cost, dedup_key, meta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                ev.session_id,
                ev.ts,
                ev.granularity.as_str(),
                ev.model,
                ev.tokens_in,
                ev.tokens_out,
                ev.cache_read,
                ev.cache_write,
                ev.cost,
                ev.dedup_key,
                meta
            ],
        )?;
        Ok(n > 0)
    }

    pub fn file_offset(&self, path: &str) -> Result<u64> {
        let off: Option<i64> = self
            .conn
            .query_row(
                "SELECT byte_offset FROM ingest_files WHERE path = ?1",
                params![path],
                |r| r.get(0),
            )
            .optional()?;
        Ok(off.unwrap_or(0).max(0) as u64)
    }

    pub fn set_file_offset(&self, path: &str, offset: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ingest_files (path, byte_offset, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(path) DO UPDATE SET
               byte_offset = excluded.byte_offset,
               updated_at = excluded.updated_at",
            params![path, offset as i64],
        )?;
        Ok(())
    }

    /// Per-day turn totals for the last `days` days (UTC days, ISO timestamps).
    /// Days without activity are absent; the UI fills gaps.
    pub fn daily(&self, days: i64) -> Result<Vec<DailyUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT date(ts), COUNT(*), SUM(tokens_in), SUM(tokens_out),
                    SUM(cache_read), SUM(cache_write)
             FROM events
             WHERE granularity = 'turn' AND date(ts) >= date('now', ?1)
             GROUP BY date(ts) ORDER BY date(ts)",
        )?;
        let rows = stmt.query_map(params![format!("-{days} days")], |r| {
            Ok(DailyUsage {
                day: r.get(0)?,
                turns: r.get(1)?,
                tokens_in: r.get(2)?,
                tokens_out: r.get(3)?,
                cache_read: r.get(4)?,
                cache_write: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Totals per model, largest output first.
    pub fn by_model(&self) -> Result<Vec<ModelUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model, 'unknown'), COUNT(*), SUM(tokens_in),
                    SUM(tokens_out), SUM(cache_read), SUM(cache_write)
             FROM events
             WHERE granularity = 'turn'
             GROUP BY COALESCE(model, 'unknown')
             ORDER BY SUM(tokens_out) DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ModelUsage {
                model: r.get(0)?,
                turns: r.get(1)?,
                tokens_in: r.get(2)?,
                tokens_out: r.get(3)?,
                cache_read: r.get(4)?,
                cache_write: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn overview(&self) -> Result<Overview> {
        let mut ov = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0),
                    COALESCE(SUM(cache_read), 0), COALESCE(SUM(cache_write), 0),
                    MIN(ts), MAX(ts)
             FROM events WHERE granularity = 'turn'",
            [],
            |r| {
                Ok(Overview {
                    turns: r.get(0)?,
                    tokens_in: r.get(1)?,
                    tokens_out: r.get(2)?,
                    cache_read: r.get(3)?,
                    cache_write: r.get(4)?,
                    first_ts: r.get(5)?,
                    last_ts: r.get(6)?,
                    ..Default::default()
                })
            },
        )?;
        ov.sessions = self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        Ok(ov)
    }
}
