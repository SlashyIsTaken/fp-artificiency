use artificiency_core::collectors::limits::{self, UsageLimit};
use artificiency_core::collectors::{claude_code, IngestReport};
use artificiency_core::integrity::{self, ConfigFile};
use artificiency_core::plugins::{self, Distribution, HookOverhead, PluginEvent};
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
fn overview(hours: i64, day_aligned: bool) -> Result<Overview, String> {
    let store = open_store()?;
    let mut ov = store
        .overview(range(hours), day_aligned)
        .map_err(|e| e.to_string())?;
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
fn by_model(hours: i64, day_aligned: bool) -> Result<Vec<ModelUsage>, String> {
    open_store()?
        .by_model(range(hours), day_aligned)
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

/// Subscription plan (e.g. "pro"), or None for API-key accounts. Drives the
/// "Spend is an API-rate reference, not your bill" note.
#[tauri::command]
fn subscription() -> Option<String> {
    limits::subscription_type()
}

#[tauri::command]
fn waste_summary(hours: i64, day_aligned: bool) -> Result<WasteSummary, String> {
    open_store()?
        .waste_summary(range(hours), day_aligned)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn duplicate_reads(hours: i64, day_aligned: bool) -> Result<Vec<DupRead>, String> {
    open_store()?
        .duplicate_reads(range(hours), 15, day_aligned)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn largest_results(hours: i64, day_aligned: bool) -> Result<Vec<BigResult>, String> {
    open_store()?
        .largest_results(range(hours), 10, day_aligned)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn tool_stats(hours: i64, day_aligned: bool) -> Result<Vec<ToolStat>, String> {
    open_store()?
        .tool_stats(range(hours), day_aligned)
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

/// Config integrity findings for the watched user-level config files.
#[tauri::command]
fn config_integrity() -> Result<Vec<ConfigFile>, String> {
    Ok(integrity::check(&open_store()?))
}

/// Accept a config file's current content as its new baseline.
#[tauri::command]
fn review_config(path: String) -> Result<(), String> {
    integrity::review(&open_store()?, &path).map_err(|e| e.to_string())
}

/// Plugin install/remove events for the impact timeline's event lines.
#[tauri::command]
fn plugin_events() -> Result<Vec<PluginEvent>, String> {
    Ok(plugins::events(&open_store()?))
}

/// Per-turn distribution of `metric` over [start, end) (ISO). Called twice per
/// event for a before/after comparison.
#[tauri::command]
fn metric_distribution(start: String, end: String, metric: String) -> Result<Distribution, String> {
    plugins::metric_distribution(&open_store()?, &start, &end, &metric).map_err(|e| e.to_string())
}

/// Per-plugin hook overhead within the range.
#[tauri::command]
fn hook_overhead(hours: i64, day_aligned: bool) -> Result<Vec<HookOverhead>, String> {
    plugins::hook_overhead(&open_store()?, range(hours), day_aligned).map_err(|e| e.to_string())
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
            subscription,
            waste_summary,
            duplicate_reads,
            largest_results,
            tool_stats,
            config_integrity,
            review_config,
            plugin_events,
            metric_distribution,
            hook_overhead
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
