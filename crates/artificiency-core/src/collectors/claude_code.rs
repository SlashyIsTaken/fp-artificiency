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
        let parsed = parse_line(&String::from_utf8_lossy(&buf));
        let empty = parsed.turn.is_none()
            && parsed.tool_uses.is_empty()
            && parsed.tool_results.is_empty()
            && parsed.hook_calls.is_empty();
        if let Some(turn) = parsed.turn {
            store.upsert_session(&turn.session_id, PROVIDER, turn.cwd.as_deref(), Some(&turn.ts))?;
            if store.insert_event(&turn.into_event())? {
                report.events_added += 1;
            }
        }
        for tu in &parsed.tool_uses {
            store.insert_tool_use(
                &tu.session_id,
                &tu.ts,
                &tu.tool,
                tu.target.as_deref(),
                &format!("{PROVIDER}:{}", tu.id),
            )?;
        }
        for tr in &parsed.tool_results {
            store.set_tool_result_chars(&format!("{PROVIDER}:{}", tr.tool_use_id), tr.chars)?;
        }
        for hc in &parsed.hook_calls {
            store.insert_hook_call(
                &hc.session_id,
                &hc.ts,
                &hc.script,
                hc.duration_ms,
                &format!("{PROVIDER}:hook:{}:{}", hc.uuid, hc.idx),
            )?;
        }
        if empty {
            report.lines_skipped += 1;
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

#[derive(Debug, Default)]
pub struct Parsed {
    pub turn: Option<Turn>,
    pub tool_uses: Vec<ToolUse>,
    pub tool_results: Vec<ToolResult>,
    pub hook_calls: Vec<HookCall>,
}

#[derive(Debug)]
pub struct HookCall {
    pub session_id: String,
    pub ts: String,
    pub script: String,
    pub duration_ms: Option<i64>,
    pub uuid: String,
    pub idx: usize, // position within the record's hookInfos, for the dedup key
}

/// The hook script name from a command. Commands reference
/// `${CLAUDE_PLUGIN_ROOT}/hooks/<script>` (root unexpanded); we key on the
/// script basename, falling back to the command's last token.
fn hook_script(command: &str) -> String {
    if let Some(rest) = command.split("${CLAUDE_PLUGIN_ROOT}/").nth(1) {
        let path: String = rest.chars().take_while(|c| !"\"' \t".contains(*c)).collect();
        return path.rsplit('/').next().unwrap_or(&path).to_string();
    }
    command.split_whitespace().last().unwrap_or(command).chars().take(40).collect()
}

#[derive(Debug)]
pub struct ToolUse {
    pub id: String,
    pub tool: String,
    pub target: Option<String>,
    pub session_id: String,
    pub ts: String,
}

#[derive(Debug)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub chars: i64,
}

/// The tool-input field that best identifies what a call touched.
const TARGET_KEYS: &[&str] = &["file_path", "path", "command", "url", "pattern", "query", "skill"];

fn extract_target(input: &Value) -> Option<String> {
    for key in TARGET_KEYS {
        if let Some(s) = input.get(key).and_then(Value::as_str) {
            // char-wise cap: byte-indexed truncate panics mid-codepoint
            return Some(s.chars().take(160).collect());
        }
    }
    None
}

/// Extract everything of interest from one transcript line. Turns come from
/// `assistant` records with usage (except `<synthetic>` — Claude Code's
/// locally generated placeholders, not real model calls); tool uses from
/// assistant content blocks; tool result sizes from `user` records.
pub fn parse_line(line: &str) -> Parsed {
    let mut out = Parsed::default();
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
        return out;
    };
    let (Some(kind), Some(session_id), Some(ts)) = (
        v.get("type").and_then(Value::as_str),
        v.get("sessionId").and_then(Value::as_str),
        v.get("timestamp").and_then(Value::as_str),
    ) else {
        return out;
    };
    let content = v.pointer("/message/content").and_then(Value::as_array);

    match kind {
        "assistant" => {
            let model = v.pointer("/message/model").and_then(Value::as_str);
            if model != Some("<synthetic>") {
                if let (Some(usage), Some(uuid)) = (
                    v.pointer("/message/usage"),
                    v.get("uuid").and_then(Value::as_str),
                ) {
                    let int = |k: &str| usage.get(k).and_then(Value::as_i64).unwrap_or(0);
                    out.turn = Some(Turn {
                        session_id: session_id.to_string(),
                        ts: ts.to_string(),
                        uuid: uuid.to_string(),
                        model: model.map(String::from),
                        tokens_in: int("input_tokens"),
                        tokens_out: int("output_tokens"),
                        cache_read: int("cache_read_input_tokens"),
                        cache_write: int("cache_creation_input_tokens"),
                        is_sidechain: v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false),
                        cwd: v.get("cwd").and_then(Value::as_str).map(String::from),
                    });
                }
                for block in content.into_iter().flatten() {
                    if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let (Some(id), Some(tool)) = (
                        block.get("id").and_then(Value::as_str),
                        block.get("name").and_then(Value::as_str),
                    ) else {
                        continue;
                    };
                    out.tool_uses.push(ToolUse {
                        id: id.to_string(),
                        tool: tool.to_string(),
                        target: block.get("input").and_then(extract_target),
                        session_id: session_id.to_string(),
                        ts: ts.to_string(),
                    });
                }
            }
        }
        "user" => {
            for block in content.into_iter().flatten() {
                if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                    continue;
                };
                let chars = block
                    .get("content")
                    .map(|c| match c.as_str() {
                        Some(s) => s.len() as i64,
                        None => c.to_string().len() as i64,
                    })
                    .unwrap_or(0);
                out.tool_results.push(ToolResult {
                    tool_use_id: id.to_string(),
                    chars,
                });
            }
        }
        "system" => {
            if let Some(hooks) = v.get("hookInfos").and_then(Value::as_array) {
                let uuid = v.get("uuid").and_then(Value::as_str).unwrap_or_default().to_string();
                for (idx, h) in hooks.iter().enumerate() {
                    let Some(cmd) = h.get("command").and_then(Value::as_str) else {
                        continue;
                    };
                    // durationMs is sometimes a number, sometimes a string.
                    let duration_ms = h.get("durationMs").and_then(|d| {
                        d.as_i64().or_else(|| d.as_str().and_then(|s| s.parse().ok()))
                    });
                    out.hook_calls.push(HookCall {
                        session_id: session_id.to_string(),
                        ts: ts.to_string(),
                        script: hook_script(cmd),
                        duration_ms,
                        uuid: uuid.clone(),
                        idx,
                    });
                }
            }
        }
        _ => {}
    }
    out
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

    const TOOL_USE_LINE: &str = r#"{"type":"assistant","uuid":"ccc-1","sessionId":"s-1","timestamp":"2026-07-13T10:02:00.000Z","message":{"model":"claude-opus-4-8","role":"assistant","usage":{"input_tokens":1,"output_tokens":1},"content":[{"type":"text","text":"reading"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/repo/main.rs"}}]}}"#;
    const TOOL_RESULT_LINE: &str = r#"{"type":"user","uuid":"ddd-1","sessionId":"s-1","timestamp":"2026-07-13T10:02:01.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"0123456789"}]}}"#;
    const SYNTHETIC_LINE: &str = r#"{"type":"assistant","uuid":"eee-1","sessionId":"s-1","timestamp":"2026-07-13T10:03:00.000Z","message":{"model":"<synthetic>","role":"assistant","usage":{"input_tokens":0,"output_tokens":0}}}"#;

    #[test]
    fn parses_assistant_usage() {
        let t = parse_line(ASSISTANT_LINE).turn.expect("assistant line should parse");
        assert_eq!(t.session_id, "s-1");
        assert_eq!(t.tokens_in, 5733);
        assert_eq!(t.tokens_out, 475);
        assert_eq!(t.cache_read, 10319);
        assert_eq!(t.cache_write, 2590);
        assert_eq!(t.model.as_deref(), Some("claude-opus-4-8"));
        assert!(!t.is_sidechain);
    }

    #[test]
    fn parses_tool_use_and_result() {
        let p = parse_line(TOOL_USE_LINE);
        assert!(p.turn.is_some()); // same record carries usage AND the tool call
        assert_eq!(p.tool_uses.len(), 1);
        assert_eq!(p.tool_uses[0].tool, "Read");
        assert_eq!(p.tool_uses[0].target.as_deref(), Some("/repo/main.rs"));

        let r = parse_line(TOOL_RESULT_LINE);
        assert!(r.turn.is_none());
        assert_eq!(r.tool_results.len(), 1);
        assert_eq!(r.tool_results[0].chars, 10);
    }

    #[test]
    fn skips_non_assistant_synthetic_and_garbage() {
        let empty = |p: &Parsed| p.turn.is_none() && p.tool_uses.is_empty() && p.tool_results.is_empty();
        assert!(empty(&parse_line(USER_LINE)));
        assert!(empty(&parse_line(HOUSEKEEPING_LINE)));
        assert!(empty(&parse_line(SYNTHETIC_LINE)));
        assert!(empty(&parse_line("not json at all")));
        assert!(empty(&parse_line("")));
    }

    #[test]
    fn waste_queries_find_duplicate_reads() {
        let store = Store::open_in_memory().unwrap();
        // Same file read twice in one session, once in another.
        for (sess, id, chars) in [("s-1", "t1", 1000), ("s-1", "t2", 1000), ("s-2", "t3", 500)] {
            store
                .insert_tool_use(sess, "2026-07-13T10:00:00Z", "Read", Some("/repo/a.rs"), id)
                .unwrap();
            store.set_tool_result_chars(id, chars).unwrap();
        }
        let dups = store.duplicate_reads(None, 10, false).unwrap();
        assert_eq!(dups.len(), 1); // only s-1 has a duplicate
        assert_eq!(dups[0].target, "/repo/a.rs");
        assert_eq!(dups[0].reads, 2);
        assert_eq!(dups[0].extra, 1);
        assert_eq!(dups[0].wasted_chars, 1000);

        let sum = store.waste_summary(None, false).unwrap();
        assert_eq!(sum.tool_calls, 3);
        assert_eq!(sum.extra_reads, 1);
        assert_eq!(sum.wasted_chars, 1000);
        assert_eq!(sum.biggest_chars, 1000);

        let stats = store.tool_stats(None, false).unwrap();
        assert_eq!(stats[0].tool, "Read");
        assert_eq!(stats[0].calls, 3);
        assert_eq!(stats[0].chars, 2500);
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

        let ov = store.overview(None, false).unwrap();
        assert_eq!(ov.turns, 2);
        assert_eq!(ov.tokens_in, 5743);
        assert_eq!(ov.sessions, 1);

        // All-time rollups over the ingested turns.
        let series = store.series(None, crate::store::Bucket::Day).unwrap();
        assert_eq!(series.len(), 1); // both turns fall on the same (local) day
        assert_eq!(series[0].turns, 2);
        assert_eq!(series[0].tokens_out, 477);

        let models = store.by_model(None, false).unwrap();
        assert_eq!(models.len(), 2); // opus + the model-less second turn
        assert_eq!(models[0].model, "claude-opus-4-8");
        assert_eq!(models[1].model, "unknown");

        // Range filtering: the fixed 2026-07-13 timestamps are in the past,
        // so a 1-hour window is empty.
        let recent = store.overview(Some(1), false).unwrap();
        assert_eq!(recent.turns, 0);
        assert_eq!(recent.sessions, 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
