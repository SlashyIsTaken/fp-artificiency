use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::collectors::limits::UsageLimit;
use crate::pricing;
use crate::{Error, Result};

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
pub struct UsageBucket {
    /// Bucket key in local time: "2026-07-13" (day) or "2026-07-13T15:00" (hour/minute).
    pub bucket: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Minute,
    Hour,
    Day,
}

impl Bucket {
    /// SQL expression producing the bucket key. Buckets are computed in local
    /// time (the user's clock), while range filtering stays on raw UTC ts.
    fn sql_expr(self) -> &'static str {
        match self {
            Bucket::Minute => "strftime('%Y-%m-%dT%H:%M', ts, 'localtime')",
            Bucket::Hour => "strftime('%Y-%m-%dT%H:00', ts, 'localtime')",
            Bucket::Day => "date(ts, 'localtime')",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// Estimated USD cost of this model's usage in the range (0 if unpriced).
    pub cost: f64,
}

/// Per-(bucket, model) rollup carrying every chartable metric, so the UI can
/// switch which metric the stacked chart shows without re-querying.
#[derive(Debug, Serialize)]
pub struct ModelBucket {
    pub bucket: String,
    pub model: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost: f64,
}

/// Distinct sessions active per bucket. Sessions aren't attributable to a
/// single model, so this is a separate single-series metric (not stacked).
#[derive(Debug, Serialize)]
pub struct SessionBucket {
    pub bucket: String,
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct ToolStat {
    pub tool: String,
    pub calls: i64,
    pub chars: i64,
}

#[derive(Debug, Serialize)]
pub struct DupRead {
    pub target: String,
    pub reads: i64,
    /// Reads beyond the first, per session, summed.
    pub extra: i64,
    pub wasted_chars: i64,
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct BigResult {
    pub tool: String,
    pub target: Option<String>,
    pub chars: i64,
    pub ts: String,
}

#[derive(Debug, Serialize, Default)]
pub struct WasteSummary {
    pub tool_calls: i64,
    pub extra_reads: i64,
    pub wasted_chars: i64,
    pub biggest_chars: i64,
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
    /// Estimated total USD spend across priced models in the range.
    pub cost: f64,
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
        Self::migrate(&conn)?;
        Ok(Store { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn)?;
        Ok(Store { conn })
    }

    /// Idempotent data migrations for stores created by earlier versions.
    fn migrate(conn: &Connection) -> Result<()> {
        // `<synthetic>` turns are Claude Code's locally generated placeholder
        // messages (API errors etc.), not real model calls; newer collectors
        // skip them at parse time.
        conn.execute("DELETE FROM events WHERE model = '<synthetic>'", [])?;
        // Stores populated before tool-call collection existed have consumed
        // the transcripts already; reset offsets once so tool calls get
        // backfilled (event dedup keys make the re-read idempotent).
        let tools: i64 = conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |r| r.get(0))?;
        let events: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        if tools == 0 && events > 0 {
            conn.execute("DELETE FROM ingest_files", [])?;
        }
        Ok(())
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

    pub fn insert_tool_use(
        &self,
        session_id: &str,
        ts: &str,
        tool: &str,
        target: Option<&str>,
        dedup_key: &str,
    ) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO tool_calls (session_id, ts, tool, target, dedup_key)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, ts, tool, target, dedup_key],
        )?;
        Ok(n > 0)
    }

    /// Record the result size for a previously inserted tool call.
    pub fn set_tool_result_chars(&self, dedup_key: &str, chars: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tool_calls SET result_chars = ?2 WHERE dedup_key = ?1",
            params![dedup_key, chars],
        )?;
        Ok(())
    }

    /// Calls and result volume per tool within the range, largest volume first.
    pub fn tool_stats(&self, hours: Option<i64>) -> Result<Vec<ToolStat>> {
        let (clause, param) = Self::since_clause(hours);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT tool, COUNT(*), COALESCE(SUM(result_chars), 0)
             FROM tool_calls WHERE 1=1 {clause}
             GROUP BY tool ORDER BY SUM(COALESCE(result_chars, 0)) DESC"
        ))?;
        let rows = stmt.query_map(params![param], |r| {
            Ok(ToolStat {
                tool: r.get(0)?,
                calls: r.get(1)?,
                chars: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Files read more than once within a session, aggregated across sessions,
    /// worst waste first. Wasted volume estimates the repeats at the file's
    /// average result size.
    pub fn duplicate_reads(&self, hours: Option<i64>, limit: i64) -> Result<Vec<DupRead>> {
        let (clause, param) = Self::since_clause(hours);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT target, SUM(cnt), SUM(cnt - 1),
                    CAST(SUM((cnt - 1.0) * avg_chars) AS INTEGER), COUNT(*)
             FROM (
               SELECT session_id, target, COUNT(*) AS cnt,
                      AVG(COALESCE(result_chars, 0)) AS avg_chars
               FROM tool_calls
               WHERE tool = 'Read' AND target IS NOT NULL {clause}
               GROUP BY session_id, target HAVING COUNT(*) > 1
             )
             GROUP BY target ORDER BY 4 DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![param, limit], |r| {
            Ok(DupRead {
                target: r.get(0)?,
                reads: r.get(1)?,
                extra: r.get(2)?,
                wasted_chars: r.get(3)?,
                sessions: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Largest single tool results within the range.
    pub fn largest_results(&self, hours: Option<i64>, limit: i64) -> Result<Vec<BigResult>> {
        let (clause, param) = Self::since_clause(hours);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT tool, target, result_chars, ts FROM tool_calls
             WHERE result_chars IS NOT NULL {clause}
             ORDER BY result_chars DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![param, limit], |r| {
            Ok(BigResult {
                tool: r.get(0)?,
                target: r.get(1)?,
                chars: r.get(2)?,
                ts: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn waste_summary(&self, hours: Option<i64>) -> Result<WasteSummary> {
        let (clause, param) = Self::since_clause(hours);
        let mut sum = self.conn.query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(MAX(result_chars), 0)
                 FROM tool_calls WHERE 1=1 {clause}"
            ),
            params![param],
            |r| {
                Ok(WasteSummary {
                    tool_calls: r.get(0)?,
                    biggest_chars: r.get(1)?,
                    ..Default::default()
                })
            },
        )?;
        let (clause, param) = Self::since_clause(hours);
        (sum.extra_reads, sum.wasted_chars) = self.conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(cnt - 1), 0),
                        COALESCE(CAST(SUM((cnt - 1.0) * avg_chars) AS INTEGER), 0)
                 FROM (
                   SELECT COUNT(*) AS cnt, AVG(COALESCE(result_chars, 0)) AS avg_chars
                   FROM tool_calls
                   WHERE tool = 'Read' AND target IS NOT NULL {clause}
                   GROUP BY session_id, target HAVING COUNT(*) > 1
                 )"
            ),
            params![param],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(sum)
    }

    /// Remember the last successful subscription limits so a later fetch
    /// failure (rate limit, offline) can fall back to them instead of the
    /// widget vanishing.
    pub fn cache_usage_limits(&self, limits: &[UsageLimit]) -> Result<()> {
        let json = serde_json::to_string(limits).map_err(|e| Error::Other(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES ('usage_limits', ?1, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![json],
        )?;
        Ok(())
    }

    /// Generic small-value store (config baselines, cached widgets, …).
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value],
        )?;
        Ok(())
    }

    /// The last cached subscription limits, if any were ever stored.
    pub fn cached_usage_limits(&self) -> Result<Option<Vec<UsageLimit>>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'usage_limits'", [], |r| r.get(0))
            .optional()?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
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

    /// SQL fragment + parameter for range filtering. `hours = None` means
    /// all time. The threshold is rendered in the same ISO-T shape as stored
    /// timestamps so plain string comparison is correct.
    fn since_clause(hours: Option<i64>) -> (&'static str, Option<String>) {
        match hours {
            Some(h) => (
                "AND ts >= strftime('%Y-%m-%dT%H:%M:%S', 'now', ?1)",
                Some(format!("-{h} hours")),
            ),
            None => ("AND ?1 IS NULL", None),
        }
    }

    /// Bucketed turn totals over the last `hours` (or all time). Buckets
    /// without activity are absent; the UI fills gaps.
    pub fn series(&self, hours: Option<i64>, bucket: Bucket) -> Result<Vec<UsageBucket>> {
        let (clause, param) = Self::since_clause(hours);
        let expr = bucket.sql_expr();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {expr}, COUNT(*), SUM(tokens_in), SUM(tokens_out),
                    SUM(cache_read), SUM(cache_write)
             FROM events
             WHERE granularity = 'turn' {clause}
             GROUP BY {expr} ORDER BY {expr}"
        ))?;
        let rows = stmt.query_map(params![param], |r| {
            Ok(UsageBucket {
                bucket: r.get(0)?,
                turns: r.get(1)?,
                tokens_in: r.get(2)?,
                tokens_out: r.get(3)?,
                cache_read: r.get(4)?,
                cache_write: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Every chartable metric per (bucket, model) — the stacked-chart series.
    pub fn series_by_model(&self, hours: Option<i64>, bucket: Bucket) -> Result<Vec<ModelBucket>> {
        let (clause, param) = Self::since_clause(hours);
        let expr = bucket.sql_expr();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {expr}, COALESCE(model, 'unknown'), COUNT(*), SUM(tokens_in),
                    SUM(tokens_out), SUM(cache_read), SUM(cache_write)
             FROM events
             WHERE granularity = 'turn' {clause}
             GROUP BY 1, 2 ORDER BY 1"
        ))?;
        let rows = stmt.query_map(params![param], |r| {
            let model: String = r.get(1)?;
            let tokens_in: i64 = r.get(3)?;
            let tokens_out: i64 = r.get(4)?;
            let cache_read: i64 = r.get(5)?;
            let cache_write: i64 = r.get(6)?;
            Ok(ModelBucket {
                cost: pricing::cost_of(Some(&model), tokens_in, tokens_out, cache_read, cache_write),
                bucket: r.get(0)?,
                model,
                turns: r.get(2)?,
                tokens_in,
                tokens_out,
                cache_read,
                cache_write,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Distinct sessions active per bucket (the non-stacked Sessions metric).
    pub fn series_sessions(&self, hours: Option<i64>, bucket: Bucket) -> Result<Vec<SessionBucket>> {
        let (clause, param) = Self::since_clause(hours);
        let expr = bucket.sql_expr();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {expr}, COUNT(DISTINCT session_id)
             FROM events
             WHERE granularity = 'turn' {clause}
             GROUP BY 1 ORDER BY 1"
        ))?;
        let rows = stmt.query_map(params![param], |r| {
            Ok(SessionBucket {
                bucket: r.get(0)?,
                sessions: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Totals per model within the range, largest output first.
    pub fn by_model(&self, hours: Option<i64>) -> Result<Vec<ModelUsage>> {
        let (clause, param) = Self::since_clause(hours);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT COALESCE(model, 'unknown'), COUNT(*), SUM(tokens_in),
                    SUM(tokens_out), SUM(cache_read), SUM(cache_write)
             FROM events
             WHERE granularity = 'turn' {clause}
             GROUP BY COALESCE(model, 'unknown')
             ORDER BY SUM(tokens_out) DESC"
        ))?;
        let rows = stmt.query_map(params![param], |r| {
            let model: String = r.get(0)?;
            let tokens_in: i64 = r.get(2)?;
            let tokens_out: i64 = r.get(3)?;
            let cache_read: i64 = r.get(4)?;
            let cache_write: i64 = r.get(5)?;
            Ok(ModelUsage {
                cost: pricing::cost_of(Some(&model), tokens_in, tokens_out, cache_read, cache_write),
                model,
                turns: r.get(1)?,
                tokens_in,
                tokens_out,
                cache_read,
                cache_write,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Totals over the last `hours`, or all time when `hours` is None.
    /// Sessions counts sessions *active in the range* (distinct in events).
    pub fn overview(&self, hours: Option<i64>) -> Result<Overview> {
        let (clause, param) = Self::since_clause(hours);
        let mut ov = self.conn.query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0),
                        COALESCE(SUM(cache_read), 0), COALESCE(SUM(cache_write), 0),
                        MIN(ts), MAX(ts), COUNT(DISTINCT session_id)
                 FROM events WHERE granularity = 'turn' {clause}"
            ),
            params![param],
            |r| {
                Ok(Overview {
                    turns: r.get(0)?,
                    tokens_in: r.get(1)?,
                    tokens_out: r.get(2)?,
                    cache_read: r.get(3)?,
                    cache_write: r.get(4)?,
                    first_ts: r.get(5)?,
                    last_ts: r.get(6)?,
                    sessions: r.get(7)?,
                    ..Default::default()
                })
            },
        )?;
        // Cost is per-model, so sum the priced model rollups rather than
        // pricing the flat aggregate (which has mixed models at one rate).
        ov.cost = self.by_model(hours)?.iter().map(|m| m.cost).sum();
        Ok(ov)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_limits_cache_round_trips() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.cached_usage_limits().unwrap().is_none()); // nothing yet
        let limits = vec![UsageLimit {
            kind: "session".into(),
            label: "Session".into(),
            percent: 77,
            severity: "warning".into(),
            resets_at: Some("2026-07-14T14:20:00Z".into()),
        }];
        store.cache_usage_limits(&limits).unwrap();
        assert_eq!(store.cached_usage_limits().unwrap().as_deref(), Some(limits.as_slice()));
        // A later successful fetch overwrites the cache.
        store.cache_usage_limits(&[]).unwrap();
        assert_eq!(store.cached_usage_limits().unwrap(), Some(vec![]));
    }
}
