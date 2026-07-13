//! Claude Code collector: reads session transcripts from
//! `~/.claude/projects/<flattened-cwd>/<session-uuid>.jsonl`.
//!
//! Record shape verified against real transcripts (CC v2.x, 2026-07): only
//! `assistant` records carry `message.usage`. Parsing is tolerant by contract —
//! unknown types and malformed lines are skipped, never fatal, so format drift
//! between CC versions degrades to "fewer events", not a crash.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::IngestReport;
use crate::store::{Granularity, NewEvent, Store};
use crate::Result;

pub const PROVIDER: &str = "claude-code";

pub fn default_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// Ingest every transcript under `projects_dir`, resuming from stored offsets.
pub fn backfill(store: &Store, projects_dir: &Path) -> Result<IngestReport> {
    let mut report = IngestReport::default();
    let Ok(projects) = std::fs::read_dir(projects_dir) else {
        return Ok(report); // no Claude Code data on this machine: fail open
    };
    for project in projects.flatten() {
        let Ok(sessions) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for entry in sessions.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            report.files_seen += 1;
            ingest_file(store, &path, &mut report)?;
        }
    }
    Ok(report)
}

/// Read new complete lines from one transcript, starting at the stored byte
/// offset. A partial trailing line (session still being written) is left for
/// the next pass.
pub fn ingest_file(store: &Store, path: &Path, report: &mut IngestReport) -> Result<()> {
    let key = path.to_string_lossy().to_string();
    let mut offset = store.file_offset(&key)?;
    let len = std::fs::metadata(path)?.len();
    if len < offset {
        offset = 0; // file was truncated/replaced: re-read from the start
    }
    if len == offset {
        return Ok(());
    }

    let mut reader = BufReader::new(File::open(path)?);
    reader.seek(SeekFrom::Start(offset))?;
    report.files_read += 1;

    let mut consumed = offset;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        if buf.last() != Some(&b'\n') {
            break; // partial line at EOF; picked up next pass
        }
        consumed += n as u64;
        match parse_line(&String::from_utf8_lossy(&buf)) {
            Some(turn) => {
                store.upsert_session(
                    &turn.session_id,
                    PROVIDER,
                    turn.cwd.as_deref(),
                    Some(&turn.ts),
                )?;
                if store.insert_event(&turn.into_event())? {
                    report.events_added += 1;
                }
            }
            None => report.lines_skipped += 1,
        }
    }
    store.set_file_offset(&key, consumed)?;
    Ok(())
}

#[derive(Debug, PartialEq)]
pub struct Turn {
    pub session_id: String,
    pub ts: String,
    pub model: Option<String>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub is_sidechain: bool,
    pub cwd: Option<String>,
    pub uuid: String,
}

impl Turn {
    fn into_event(self) -> NewEvent {
        NewEvent {
            dedup_key: Some(format!("{PROVIDER}:{}", self.uuid)),
            meta: Some(serde_json::json!({ "sidechain": self.is_sidechain })),
            session_id: self.session_id,
            ts: self.ts,
            granularity: Granularity::Turn,
            model: self.model,
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            cost: None, // pricing table lands later (see DESIGN.md open questions)
        }
    }
}

/// Extract a usage turn from one transcript line, or None for anything else
/// (user prompts, housekeeping records, malformed JSON).
pub fn parse_line(line: &str) -> Option<Turn> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let usage = v.get("message")?.get("usage")?;
    let int = |u: &Value, k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
    Some(Turn {
        session_id: v.get("sessionId")?.as_str()?.to_string(),
        ts: v.get("timestamp")?.as_str()?.to_string(),
        uuid: v.get("uuid")?.as_str()?.to_string(),
        model: v
            .pointer("/message/model")
            .and_then(Value::as_str)
            .map(String::from),
        tokens_in: int(usage, "input_tokens"),
        tokens_out: int(usage, "output_tokens"),
        cache_read: int(usage, "cache_read_input_tokens"),
        cache_write: int(usage, "cache_creation_input_tokens"),
        is_sidechain: v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false),
        cwd: v.get("cwd").and_then(Value::as_str).map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // Mirrors the verified real record shape (fields we don't read are elided).
    const ASSISTANT_LINE: &str = r#"{"type":"assistant","uuid":"aaa-1","sessionId":"s-1","timestamp":"2026-07-13T10:00:00.000Z","cwd":"/home/u/proj","isSidechain":false,"message":{"model":"claude-opus-4-8","role":"assistant","usage":{"input_tokens":5733,"output_tokens":475,"cache_read_input_tokens":10319,"cache_creation_input_tokens":2590,"cache_creation":{"ephemeral_1h_input_tokens":2590,"ephemeral_5m_input_tokens":0}}}}"#;
    const USER_LINE: &str =
        r#"{"type":"user","uuid":"bbb-1","sessionId":"s-1","message":{"role":"user","content":"hi"}}"#;
    const HOUSEKEEPING_LINE: &str = r#"{"type":"file-history-snapshot","messageId":"m"}"#;

    #[test]
    fn parses_assistant_usage() {
        let t = parse_line(ASSISTANT_LINE).expect("assistant line should parse");
        assert_eq!(t.session_id, "s-1");
        assert_eq!(t.tokens_in, 5733);
        assert_eq!(t.tokens_out, 475);
        assert_eq!(t.cache_read, 10319);
        assert_eq!(t.cache_write, 2590);
        assert_eq!(t.model.as_deref(), Some("claude-opus-4-8"));
        assert!(!t.is_sidechain);
    }

    #[test]
    fn skips_non_assistant_and_garbage() {
        assert!(parse_line(USER_LINE).is_none());
        assert!(parse_line(HOUSEKEEPING_LINE).is_none());
        assert!(parse_line("not json at all").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn ingest_is_incremental_and_idempotent() {
        let dir = std::env::temp_dir().join(format!("artificiency-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s-1.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "{USER_LINE}").unwrap();
        writeln!(f, "{ASSISTANT_LINE}").unwrap();
        write!(f, r#"{{"type":"assistant","note":"par"#).unwrap(); // no trailing newline
        f.flush().unwrap();

        let store = Store::open_in_memory().unwrap();
        let mut report = IngestReport::default();
        ingest_file(&store, &path, &mut report).unwrap();
        assert_eq!(report.events_added, 1);
        assert_eq!(report.lines_skipped, 1); // the user line

        // Second pass: nothing new, partial line still pending.
        let mut report2 = IngestReport::default();
        ingest_file(&store, &path, &mut report2).unwrap();
        assert_eq!(report2.events_added, 0);

        // Complete the partial line with a second turn; only it is added.
        writeln!(f, r#"tial","uuid":"aaa-2","sessionId":"s-1","timestamp":"2026-07-13T10:01:00.000Z","message":{{"usage":{{"input_tokens":10,"output_tokens":2}}}}}}"#).unwrap();
        f.flush().unwrap();
        let mut report3 = IngestReport::default();
        ingest_file(&store, &path, &mut report3).unwrap();
        assert_eq!(report3.events_added, 1);

        let ov = store.overview(None).unwrap();
        assert_eq!(ov.turns, 2);
        assert_eq!(ov.tokens_in, 5743);
        assert_eq!(ov.sessions, 1);

        // All-time rollups over the ingested turns.
        let series = store.series(None, crate::store::Bucket::Day).unwrap();
        assert_eq!(series.len(), 1); // both turns fall on the same (local) day
        assert_eq!(series[0].turns, 2);
        assert_eq!(series[0].tokens_out, 477);

        let models = store.by_model(None).unwrap();
        assert_eq!(models.len(), 2); // opus + the model-less second turn
        assert_eq!(models[0].model, "claude-opus-4-8");
        assert_eq!(models[1].model, "unknown");

        // Range filtering: the fixed 2026-07-13 timestamps are in the past,
        // so a 1-hour window is empty.
        let recent = store.overview(Some(1)).unwrap();
        assert_eq!(recent.turns, 0);
        assert_eq!(recent.sessions, 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
