//! Headless smoke run: backfill real Claude Code transcripts into a store and
//! print the overview. Usage: cargo run --example backfill [db-path]

use artificiency_core::collectors::claude_code;
use artificiency_core::Store;

fn main() {
    let db = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(Store::default_path);
    let store = Store::open(&db).expect("open store");
    let dir = claude_code::default_projects_dir().expect("home dir");
    let report = claude_code::backfill(&store, &dir).expect("backfill");
    println!("{report:#?}");
    println!("{:#?}", store.overview(None).expect("overview"));
}
