use artificiency_core::collectors::limits::{self, UsageLimit};
use artificiency_core::collectors::{claude_code, IngestReport};
use artificiency_core::store::{
    BigResult, Bucket, DupRead, ModelBucket, ModelUsage, SessionBucket, ToolStat, UsageBucket,
    WasteSummary,
};
use artificiency_core::{Overview, Store};

// SQLite opens are cheap; opening per command keeps startup infallible
// (fail open: a broken store surfaces as a command error, not a crash).
fn open_store() -> Result<Store, String> {
    Store::open(&Store::default_path()).map_err(|e| e.to_string())
}

// `hours = 0` means all time (Option doesn't cross the IPC boundary cleanly).
fn range(hours: i64) -> Option<i64> {
    (hours > 0).then_some(hours)
}

#[tauri::command]
fn overview(hours: i64) -> Result<Overview, String> {
    let store = open_store()?;
    let mut ov = store.overview(range(hours)).map_err(|e| e.to_string())?;
    ov.db_path = Some(Store::default_path().display().to_string());
    Ok(ov)
}

#[tauri::command]
fn series(hours: i64, bucket: String) -> Result<Vec<UsageBucket>, String> {
    open_store()?
        .series(range(hours), parse_bucket(&bucket))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn by_model(hours: i64) -> Result<Vec<ModelUsage>, String> {
    open_store()?
        .by_model(range(hours))
        .map_err(|e| e.to_string())
}

fn parse_bucket(bucket: &str) -> Bucket {
    match bucket {
        "minute" => Bucket::Minute,
        "hour" => Bucket::Hour,
        _ => Bucket::Day,
    }
}

#[tauri::command]
fn series_by_model(hours: i64, bucket: String) -> Result<Vec<ModelBucket>, String> {
    open_store()?
        .series_by_model(range(hours), parse_bucket(&bucket))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn series_sessions(hours: i64, bucket: String) -> Result<Vec<SessionBucket>, String> {
    open_store()?
        .series_sessions(range(hours), parse_bucket(&bucket))
        .map_err(|e| e.to_string())
}

/// Subscription limits; None hides the widget (no subscription / no creds).
/// Network call — async so the UI thread never blocks on it. A successful
/// fetch is cached; a failed one (e.g. a 429 on the shared OAuth token) falls
/// back to the last-good cache so a transient rate limit doesn't blank the
/// widget.
#[tauri::command]
async fn usage_limits() -> Option<Vec<UsageLimit>> {
    let fresh = tauri::async_runtime::spawn_blocking(limits::usage_limits)
        .await
        .ok()
        .flatten();
    let store = open_store().ok();
    match fresh {
        Some(l) => {
            if let Some(s) = &store {
                let _ = s.cache_usage_limits(&l);
            }
            Some(l)
        }
        None => store.and_then(|s| s.cached_usage_limits().ok().flatten()),
    }
}

#[tauri::command]
fn waste_summary(hours: i64) -> Result<WasteSummary, String> {
    open_store()?
        .waste_summary(range(hours))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn duplicate_reads(hours: i64) -> Result<Vec<DupRead>, String> {
    open_store()?
        .duplicate_reads(range(hours), 15)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn largest_results(hours: i64) -> Result<Vec<BigResult>, String> {
    open_store()?
        .largest_results(range(hours), 10)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn tool_stats(hours: i64) -> Result<Vec<ToolStat>, String> {
    open_store()?
        .tool_stats(range(hours))
        .map_err(|e| e.to_string())
}

/// Incremental ingest of all Claude Code transcripts. First run backfills
/// history; later runs only read bytes appended since the stored offsets.
#[tauri::command]
fn backfill() -> Result<IngestReport, String> {
    let store = open_store()?;
    let Some(dir) = claude_code::default_projects_dir() else {
        return Ok(IngestReport::default());
    };
    claude_code::backfill(&store, &dir).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            overview,
            backfill,
            series,
            series_by_model,
            series_sessions,
            by_model,
            usage_limits,
            waste_summary,
            duplicate_reads,
            largest_results,
            tool_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
