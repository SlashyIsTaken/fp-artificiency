use artificiency_core::collectors::{claude_code, IngestReport};
use artificiency_core::store::{Bucket, ModelUsage, UsageBucket};
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
    let bucket = match bucket.as_str() {
        "minute" => Bucket::Minute,
        "hour" => Bucket::Hour,
        _ => Bucket::Day,
    };
    open_store()?
        .series(range(hours), bucket)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn by_model(hours: i64) -> Result<Vec<ModelUsage>, String> {
    open_store()?
        .by_model(range(hours))
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
        .invoke_handler(tauri::generate_handler![overview, backfill, series, by_model])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
