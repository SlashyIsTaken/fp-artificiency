//! Plugin roster timeline for the Plugin impact view's event lines. Installs
//! come from Claude Code's `installed_plugins.json` (`installedAt`, retroactive);
//! removals are detected prospectively by diffing the current roster against the
//! last-seen snapshot and recorded as markers.

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::store::Store;
use crate::Result;

#[derive(Debug, Serialize, PartialEq)]
pub struct PluginEvent {
    pub plugin: String,
    pub kind: String, // installed | removed
    pub ts: String,   // ISO 8601
}

const ROSTER_KEY: &str = "plugin_roster_seen";
const REMOVED_KIND: &str = "plugin_removed";

fn installed_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("plugins").join("installed_plugins.json"))
}

/// Current roster as (plugin id, earliest installedAt). `installed_plugins.json`
/// maps "name@marketplace" to an array of install records (one per scope/version).
fn current_roster() -> Vec<(String, String)> {
    let Some(path) = installed_path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(plugins) = v.get("plugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (id, recs) in plugins {
        let earliest = recs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.get("installedAt").and_then(Value::as_str))
            .min()
            .map(String::from);
        if let Some(ts) = earliest {
            out.push((id.clone(), ts));
        }
    }
    out
}

/// Install and removal events, oldest first. Installs read from `installedAt`;
/// removals are recorded once, when a plugin first drops out of the roster
/// between calls, timestamped at detection.
pub fn events(store: &Store) -> Vec<PluginEvent> {
    let roster = current_roster();
    let current_ids: Vec<String> = roster.iter().map(|(id, _)| id.clone()).collect();

    if let Ok(Some(prev_json)) = store.kv_get(ROSTER_KEY) {
        if let Ok(prev) = serde_json::from_str::<Vec<String>>(&prev_json) {
            for gone in prev.iter().filter(|id| !current_ids.contains(id)) {
                let _ = store.record_marker(REMOVED_KIND, gone);
            }
        }
    }
    if let Ok(json) = serde_json::to_string(&current_ids) {
        let _ = store.kv_set(ROSTER_KEY, &json);
    }

    let mut events: Vec<PluginEvent> = roster
        .into_iter()
        .map(|(plugin, ts)| PluginEvent { plugin, kind: "installed".into(), ts })
        .collect();
    if let Ok(removed) = store.markers(REMOVED_KIND) {
        events.extend(removed.into_iter().map(|(ts, plugin)| PluginEvent {
            plugin,
            kind: "removed".into(),
            ts,
        }));
    }
    events.sort_by(|a, b| a.ts.cmp(&b.ts));
    events
}

// ── Stage 3: before/after distributions ────────────────────────────────────

/// Per-turn distribution of one metric over a time window.
#[derive(Debug, Serialize, PartialEq, Default)]
pub struct Distribution {
    pub count: i64,    // turns
    pub sessions: i64, // distinct sessions (coverage)
    pub p25: f64,
    pub median: f64,
    pub p75: f64,
    pub mean: f64,
}

/// Distribution of a per-turn metric in [start, end). `cost` is priced per turn;
/// the token metrics are the raw column.
pub fn metric_distribution(store: &Store, start: &str, end: &str, metric: &str) -> Result<Distribution> {
    let turns = store.turns_in_window(start, end)?;
    let mut sessions = HashSet::new();
    let mut vals: Vec<f64> = Vec::with_capacity(turns.len());
    for t in &turns {
        sessions.insert(t.session_id.as_str());
        vals.push(match metric {
            "cost" => crate::pricing::cost_of(
                t.model.as_deref(),
                t.tokens_in,
                t.tokens_out,
                t.cache_read,
                t.cache_write,
            ),
            "input" => t.tokens_in as f64,
            "cache_read" => t.cache_read as f64,
            _ => t.tokens_out as f64,
        });
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = if vals.is_empty() {
        0.0
    } else {
        vals.iter().sum::<f64>() / vals.len() as f64
    };
    Ok(Distribution {
        count: vals.len() as i64,
        sessions: sessions.len() as i64,
        p25: quantile(&vals, 0.25),
        median: quantile(&vals, 0.5),
        p75: quantile(&vals, 0.75),
        mean,
    })
}

/// Linear-interpolated quantile of a pre-sorted slice.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
}

// ── Stage 4: hook overhead ──────────────────────────────────────────────────

#[derive(Debug, Serialize, PartialEq)]
pub struct HookOverhead {
    pub script: String,
    pub plugin: String, // owning plugin id, "shared", or "unknown"
    pub calls: i64,
    pub total_ms: i64,
    pub avg_ms: f64,
}

/// Hook cost per script within the range, attributed to a plugin where the
/// script name is unique to one installed plugin.
pub fn hook_overhead(store: &Store, hours: Option<i64>) -> Result<Vec<HookOverhead>> {
    let owners = script_owners();
    Ok(store
        .hook_stats(hours)?
        .into_iter()
        .map(|(script, calls, total_ms)| {
            let plugin = match owners.get(&script) {
                Some(ids) if ids.len() == 1 => ids.iter().next().unwrap().clone(),
                Some(_) => "shared".into(),
                None => "unknown".into(),
            };
            HookOverhead {
                script,
                plugin,
                calls,
                total_ms,
                avg_ms: if calls > 0 { total_ms as f64 / calls as f64 } else { 0.0 },
            }
        })
        .collect())
}

/// Map hook script basename to the plugin ids that ship it, by scanning
/// `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/hooks/`.
fn script_owners() -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    let Some(home) = dirs::home_dir() else {
        return map;
    };
    let cache = home.join(".claude").join("plugins").join("cache");
    let read = |p: &std::path::Path| std::fs::read_dir(p).ok().into_iter().flatten().flatten();
    for market in read(&cache) {
        let market_name = market.file_name().to_string_lossy().to_string();
        for plugin in read(&market.path()) {
            let id = format!("{}@{}", plugin.file_name().to_string_lossy(), market_name);
            for version in read(&plugin.path()) {
                for entry in read(&version.path().join("hooks")) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    map.entry(name).or_default().insert(id.clone());
                }
            }
        }
    }
    map
}
