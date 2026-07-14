//! Per-model pricing and cost computation, provider-agnostic.
//!
//! Artificiency is a multi-provider tool; pricing is a per-model dimension, not
//! an Anthropic-specific one. Each provider contributes a block of entries to
//! `TABLE` below; the lookup matches on the model id alone, so adding a new
//! provider is just adding rows (and, if that provider prices its cache
//! differently, giving those rows their own cache multipliers). The store
//! already tags every session with its `provider`, so cost rolls up per
//! provider for free once the rows exist.
//!
//! Prices are USD per 1M tokens (table current as of 2026-07; DESIGN.md wants a
//! runtime-refreshable source later). `cache_read`/`cache_write` multipliers are
//! per-model so each provider's cache economics can differ; the defaults follow
//! the prompt-cache norm — a cache *read* ~0.1x the input rate, a cache *write*
//! ~1.25x (the short-TTL premium). Sources that don't split cache-write TTLs
//! (e.g. Claude Code lumps 5m + 1h creation) are priced at the short-TTL rate,
//! slightly under-counting long-TTL churn.
//!
//! Cost is always an estimate we own, never a bill: unknown models price to
//! zero and are surfaced as such, so a missing entry reads as "not yet priced"
//! rather than "free".

/// Per-1M-token pricing for one model, including its cache-rate multipliers
/// (relative to the input rate) so providers with different cache economics
/// coexist in one table.
#[derive(Debug, Clone, Copy)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read_mult: f64,
    pub cache_write_mult: f64,
}

/// The prompt-cache norm shared by current providers: read ~0.1x input, write
/// ~1.25x. A provider that departs from this sets its own multipliers per row.
const CACHE_READ: f64 = 0.1;
const CACHE_WRITE: f64 = 1.25;

/// Concise constructor for a row on the standard cache norm.
const fn p(input: f64, output: f64) -> ModelPrice {
    ModelPrice { input, output, cache_read_mult: CACHE_READ, cache_write_mult: CACHE_WRITE }
}

impl ModelPrice {
    /// Estimated USD cost of one usage record at this price.
    pub fn cost(self, tokens_in: i64, tokens_out: i64, cache_read: i64, cache_write: i64) -> f64 {
        let per = |tokens: i64, rate: f64| tokens as f64 / 1_000_000.0 * rate;
        per(tokens_in, self.input)
            + per(tokens_out, self.output)
            + per(cache_read, self.input * self.cache_read_mult)
            + per(cache_write, self.input * self.cache_write_mult)
    }
}

/// Model-id fragments mapped to prices, longest fragment winning so specific
/// versions override family fallbacks. Matched against the lowercased model id
/// as a substring — tolerant of the date suffixes and vendor prefixes that
/// appear in real transcripts (`claude-opus-4-8`, `gpt-4o-2024-…`). Grouped by
/// provider; add a new block to support a new provider's models.
const TABLE: &[(&str, ModelPrice)] = &[
    // ── Anthropic (Claude) ──────────────────────────────────────────────
    // Fable / most capable
    ("fable-5", p(10.0, 50.0)),
    ("mythos-5", p(10.0, 50.0)),
    // Opus 4.5+ — 1M-context pricing
    ("opus-4-8", p(5.0, 25.0)),
    ("opus-4-7", p(5.0, 25.0)),
    ("opus-4-6", p(5.0, 25.0)),
    ("opus-4-5", p(5.0, 25.0)),
    // Opus 4.1 / 4.0 / 3 — legacy $15/$75
    ("opus-4-1", p(15.0, 75.0)),
    ("opus-4", p(15.0, 75.0)),
    ("3-opus", p(15.0, 75.0)),
    // Sonnet — $3/$15 across the current line
    ("sonnet-5", p(3.0, 15.0)),
    ("sonnet-4-6", p(3.0, 15.0)),
    ("sonnet-4-5", p(3.0, 15.0)),
    ("sonnet-4", p(3.0, 15.0)),
    ("sonnet", p(3.0, 15.0)),
    // Haiku
    ("haiku-4-5", p(1.0, 5.0)),
    ("haiku", p(0.8, 4.0)),
    // Opus family fallback (current pricing) for any unversioned "opus"
    ("opus", p(5.0, 25.0)),
    // ── Future providers (OpenAI, Google, OpenRouter, …) slot in here, once
    //    a collector for them lands. Their cache economics may differ — set
    //    per-row multipliers rather than reusing p().
];

/// Price for a model id, or `None` when unrecognized (priced as $0 upstream,
/// and reported honestly as unpriced).
pub fn price_for(model: &str) -> Option<ModelPrice> {
    let m = model.to_ascii_lowercase();
    TABLE
        .iter()
        .filter(|(key, _)| m.contains(key))
        .max_by_key(|(key, _)| key.len())
        .map(|(_, price)| *price)
}

/// Estimated cost of a usage record, 0.0 when the model is unpriced.
pub fn cost_of(
    model: Option<&str>,
    tokens_in: i64,
    tokens_out: i64,
    cache_read: i64,
    cache_write: i64,
) -> f64 {
    model
        .and_then(price_for)
        .map(|p| p.cost(tokens_in, tokens_out, cache_read, cache_write))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_specific_version_over_family() {
        // "opus-4-8" must win over the bare "opus" fallback and the legacy
        // "opus-4" entry (both are substrings of the id).
        assert_eq!(price_for("claude-opus-4-8").unwrap().input, 5.0);
        // A genuine Opus 4.0 id lands on the legacy $15 rate, not the 4.6+ one.
        assert_eq!(price_for("claude-opus-4-20250514").unwrap().input, 15.0);
    }

    #[test]
    fn tolerates_prefixes_suffixes_and_case() {
        assert_eq!(price_for("claude-3-5-sonnet-20241022").unwrap().input, 3.0);
        assert_eq!(price_for("CLAUDE-HAIKU-4-5").unwrap().input, 1.0);
        assert!(price_for("claude-fable-5").is_some());
    }

    #[test]
    fn unknown_model_is_unpriced() {
        assert!(price_for("gpt-4o").is_none());
        assert_eq!(cost_of(Some("gpt-4o"), 1000, 1000, 0, 0), 0.0);
        assert_eq!(cost_of(None, 1000, 1000, 0, 0), 0.0);
    }

    #[test]
    fn cost_uses_cache_multipliers() {
        // 1M input, 1M output, 1M cache-read, 1M cache-write on Opus 4.8.
        // 5 + 25 + (5*0.1) + (5*1.25) = 5 + 25 + 0.5 + 6.25 = 36.75
        let c = cost_of(Some("claude-opus-4-8"), 1_000_000, 1_000_000, 1_000_000, 1_000_000);
        assert!((c - 36.75).abs() < 1e-9, "got {c}");
    }
}
