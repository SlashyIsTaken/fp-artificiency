//! Collectors: read-only modules that know where a given AI tool leaves its
//! usage data and normalize it into the store. Zero install steps; nothing is
//! injected into the tool (that's an *enricher*, see DESIGN.md).

pub mod claude_code;

use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct IngestReport {
    pub files_seen: usize,
    pub files_read: usize,
    pub events_added: usize,
    pub lines_skipped: usize,
}
