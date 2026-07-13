//! Subscription usage limits for Claude Code accounts, from Anthropic's OAuth
//! usage endpoint — the same source the official /usage view reads. Uses the
//! access token Claude Code already stores locally; nothing beyond that token
//! ever leaves the machine, and we never refresh it (Claude Code owns the
//! refresh cycle).
//!
//! Response shape verified live 2026-07-13: a `limits[]` array with kind,
//! group, percent, severity, resets_at, is_active, and optional model scope;
//! legacy `five_hour`/`seven_day` objects as fallback. API-key accounts
//! without a subscription have no limits — everything here fails open to
//! `None` and the UI hides the widget.

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Serialize, PartialEq)]
pub struct UsageLimit {
    pub kind: String,
    pub label: String,
    pub percent: i64,
    /// "normal" | "warning" | ... as reported; UI maps unknowns to normal.
    pub severity: String,
    pub resets_at: Option<String>,
}

fn credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join(".credentials.json"))
}

/// The stored access token, unless missing or already expired.
fn access_token() -> Option<String> {
    let raw = std::fs::read_to_string(credentials_path()?).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let oauth = v.get("claudeAiOauth")?;
    let expires_at = oauth.get("expiresAt").and_then(Value::as_f64).unwrap_or(0.0);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as f64;
    if now_ms >= expires_at {
        return None; // stale token; Claude Code will refresh it on next use
    }
    oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .map(String::from)
}

/// Fetch current usage limits. `None` (never an error) when there are no
/// credentials, no subscription, or any fetch problem — fail open by contract.
pub fn usage_limits() -> Option<Vec<UsageLimit>> {
    let token = access_token()?;
    let resp = ureq::get(USAGE_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .ok()?;
    let body: Value = resp.into_json().ok()?;
    let limits = parse_limits(&body);
    (!limits.is_empty()).then_some(limits)
}

fn label_for(kind: &str, scope_model: Option<&str>) -> String {
    match (kind, scope_model) {
        ("session", _) => "Session".to_string(),
        ("weekly_all", _) => "Weekly (all models)".to_string(),
        (_, Some(m)) => format!("Weekly · {m}"),
        (other, None) => other.replace('_', " "),
    }
}

/// Normalize the response into display-ready limits (session first).
pub fn parse_limits(body: &Value) -> Vec<UsageLimit> {
    let mut out = Vec::new();
    if let Some(arr) = body.get("limits").and_then(Value::as_array) {
        for l in arr {
            let Some(kind) = l.get("kind").and_then(Value::as_str) else {
                continue;
            };
            let scope_model = l
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str);
            out.push(UsageLimit {
                kind: kind.to_string(),
                label: label_for(kind, scope_model),
                percent: l.get("percent").and_then(Value::as_i64).unwrap_or(0),
                severity: l
                    .get("severity")
                    .and_then(Value::as_str)
                    .unwrap_or("normal")
                    .to_string(),
                resets_at: l.get("resets_at").and_then(Value::as_str).map(String::from),
            });
        }
    }
    if out.is_empty() {
        // Legacy shape fallback.
        for (key, kind, label) in [
            ("five_hour", "session", "Session"),
            ("seven_day", "weekly_all", "Weekly (all models)"),
        ] {
            if let Some(pct) = body.pointer(&format!("/{key}/utilization")).and_then(Value::as_f64) {
                out.push(UsageLimit {
                    kind: kind.to_string(),
                    label: label.to_string(),
                    percent: pct.round() as i64,
                    severity: if pct >= 90.0 { "warning" } else { "normal" }.to_string(),
                    resets_at: body
                        .pointer(&format!("/{key}/resets_at"))
                        .and_then(Value::as_str)
                        .map(String::from),
                });
            }
        }
    }
    out.sort_by_key(|l| (l.kind != "session", l.kind.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_limits_array() {
        let body: Value = serde_json::from_str(
            r#"{"limits":[
              {"kind":"weekly_scoped","group":"weekly","percent":14,"severity":"normal","resets_at":"2026-07-19T16:00:00Z","scope":{"model":{"id":null,"display_name":"Fable"}},"is_active":false},
              {"kind":"session","group":"session","percent":77,"severity":"warning","resets_at":"2026-07-13T14:20:00Z","scope":null,"is_active":true},
              {"kind":"weekly_all","group":"weekly","percent":8,"severity":"normal","resets_at":"2026-07-19T16:00:00Z","scope":null,"is_active":false}
            ]}"#,
        )
        .unwrap();
        let limits = parse_limits(&body);
        assert_eq!(limits.len(), 3);
        assert_eq!(limits[0].kind, "session"); // session sorts first
        assert_eq!(limits[0].percent, 77);
        assert_eq!(limits[0].severity, "warning");
        assert_eq!(limits[1].label, "Weekly (all models)");
        assert_eq!(limits[2].label, "Weekly · Fable");
    }

    #[test]
    fn falls_back_to_legacy_shape() {
        let body: Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":91.0,"resets_at":"2026-07-13T14:20:00Z"},
                "seven_day":{"utilization":8.0,"resets_at":null}}"#,
        )
        .unwrap();
        let limits = parse_limits(&body);
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0].percent, 91);
        assert_eq!(limits[0].severity, "warning");
        assert_eq!(limits[1].percent, 8);
    }

    #[test]
    fn empty_for_accounts_without_limits() {
        let body: Value = serde_json::from_str(r#"{"limits":[]}"#).unwrap();
        assert!(parse_limits(&body).is_empty());
    }
}
