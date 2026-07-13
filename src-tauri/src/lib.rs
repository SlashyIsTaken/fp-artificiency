use artificiency_core::collectors::{claude_code, IngestReport};
use artificiency_core::{Overview, Store};

// SQLite opens are cheap; opening per command keeps startup infallible
// (fail open: a broken store surfaces as a command error, not a crash).
fn open_store() -> Result<Store, String> {
    Store::open(&Store::default_path()).map_err(|e| e.to_string())
}

#[tauri::command]
fn overview() -> Result<Overview, String> {
    let store = open_store()?;
    let mut ov = store.overview().map_err(|e| e.to_string())?;
    ov.db_path = Some(Store::default_path().display().to_string());
    Ok(ov)
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
        .invoke_handler(tauri::generate_handler![overview, backfill])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
