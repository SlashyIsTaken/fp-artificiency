# fp-artificiency — Design

*Flarepoint | Artificial Efficiency. Local-first AI usage analytics.*
*Status: design settled 2026-07-13, pre-implementation.*

## Positioning

Every existing usage tracker (ccusage, Usagebar, Claude-Usage-Tracker, ai-token-monitor, …)
is an **odometer**: totals, costs, rate-limit bars. fp-artificiency is the
**diagnostics layer**:

1. **Why** — waste attribution: re-read files, dumped tool outputs, cache misses,
   subagent overhead. Not just "you spent X" but "here is where X went".
2. **Did it help** — measure the effect of a change: installing a plugin, editing
   CLAUDE.md, switching workflow. Before/after comparisons keyed on session markers.
   No tool on the market does this.
3. **Any AI, one view** — provider is a dimension, not a fork. Claude Code first;
   Codex, Gemini, OpenRouter, … later via collectors. Multiple providers combine
   into one dashboard.

Local-first is a feature: transcripts are sensitive. **Data never leaves the
machine.** No hosted version.

## Architecture

Fully self-contained single app — no external plugin marketplace dependency.

```
┌─ Collectors (per provider, in-app, read-only) ─┐
│  claude-code: watch ~/.claude/projects/**/*.jsonl │
│  (later) codex, gemini, openrouter-billing, …     │
└──────────────┬─────────────────────────────────┘
               ▼  normalized events
┌─ Core (Rust) ───────────────────────────────────┐
│  incremental JSONL parser · SQLite store         │
│  stats engine · pricing table (refreshable)      │
└──────────────┬─────────────────────────────────┘
               ▼
┌─ UI (Tauri v2 + Svelte 5) ──────────────────────┐
│  dashboard-first window · tray = optional sugar  │
└─────────────────────────────────────────────────┘
```

### Collectors vs Enrichers

- **Collector**: read-only module *inside* the app; knows where a tool leaves its
  data and normalizes it. Zero install steps, nothing injected into the AI tool.
  Toggled in settings. Covers ~90% of value.
- **Enricher**: optional hook the app installs *into* the AI tool itself (one
  click, app-managed, removable), only for data the logs lack. For Claude Code:
  a SessionStart hook recording the active plugin roster + config → this is what
  unlocks before/after plugin measurement. Never distributed separately.

First-run experience: install → claude-code collector backfills existing
transcript history (~weeks of data already on disk) → dashboard is instantly
populated. Enrichment is an opt-in click afterwards.

## Data model

The schema must serve **two granularities** — rich turn-level sources (Claude
Code, Codex transcripts) and aggregate-only sources (billing APIs). Never force
aggregates into fake turns; the stats engine degrades gracefully (aggregate
sources get totals/trends only, turn sources also get distributions, waste
diagnosis, A/B).

```sql
sessions(id, provider, project, started_at, meta)
events(id, session_id, ts, granularity,      -- 'turn' | 'aggregate'
       model, tokens_in, tokens_out,
       cache_read, cache_write, cost, meta)  -- meta: JSON, provider-specific
markers(id, session_id, ts, kind, payload)   -- plugin_roster | config_change | user_tag
```

Cost is computed from a shipped per-model pricing table, refreshable at runtime
(pattern proven by ccusage via LiteLLM pricing data).

### Claude Code source (verified against real transcripts, CC v2.x, 2026-07)

`~/.claude/projects/<flattened-cwd>/<session-uuid>.jsonl`, one JSON object per
line. Relevant record types:

- `assistant` — carries `message.usage`: `input_tokens`, `output_tokens`,
  `cache_read_input_tokens`, `cache_creation_input_tokens` **with 5m/1h split**
  (`cache_creation.ephemeral_{5m,1h}_input_tokens`), `server_tool_use`
  (web search/fetch counts), plus `message.model`, `requestId`, `timestamp`,
  `cwd`, `gitBranch`, `sessionId`, `version`, and `isSidechain` (subagent
  traffic is separable!).
- `user` — prompts, `promptId`, `origin`, `timestamp`.
- `system` — hook activity (`hookInfos`, `hookErrors`).
- Housekeeping types to skip: `mode`, `permission-mode`, `file-history-snapshot`,
  `attachment`, `ai-title`, `last-prompt`.

Parsing must be tolerant: skip unknown types/malformed lines, never crash on
format drift between CC versions. Watch via inotify + periodic rescan; store a
per-file byte offset for incremental ingestion.

## Stats engine (v1 targets)

- Totals & trends: tokens/cost per day/week/project/model, input:output ratio,
  cache hit rate (read vs creation), subagent share (`isSidechain`).
- Waste diagnosis: duplicate file reads within a session, largest single tool
  outputs, cache-creation churn.
- Change measurement: distributions (tokens per turn, per tool call) before vs
  after a marker. **Honesty rule: observational data — show distributions, not
  single percentages; label confidence; task mix varies.** The app must not
  overstate its own numbers.

## Platforms & stack

| Decision | Choice | Why |
|---|---|---|
| Shell | Tauri v2 | ~10MB, Rust core for parsing/watching, tray on all 3 OS |
| Targets | Linux + macOS + Windows, CI builds all three from day one | dev machine is Linux; audience is heavily macOS |
| UI shape | **Dashboard-first**; tray is optional quick-glance sugar | Linux tray support is flaky (GNOME needs extension); core UX must not depend on it |
| Frontend | Svelte 5 | small, natural for data-dense reactive dashboards |
| Store | SQLite (XDG data dir) | single file, future CLI/headless mode reads the same DB |
| v1 scope | **Claude Code collector only**; schema designed on paper against Codex + a billing-style aggregate source | ship fast; revisit abstraction with a real second collector before declaring the interface stable |

## Open questions

- Enricher UX: exactly how the app writes/removes its SessionStart hook in
  `~/.claude/settings.json` safely (merge, don't clobber; fail open).
- Pricing table refresh source + cadence; offline behavior.
- Retention: transcripts get cleaned up by CC (`cleanupPeriodDays`) — our SQLite
  store becomes the durable history. Import-once semantics and dedup on rescan.
- Marker capture without the enricher: can plugin roster be inferred from
  `~/.claude/settings.json` mtime snapshots as a degraded fallback?
- Second collector choice when the time comes (Codex CLI logs vs OpenRouter
  billing — the latter exercises the `aggregate` granularity path better).
