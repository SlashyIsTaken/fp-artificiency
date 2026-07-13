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
populated.

### Enrichment toggle

Enrichment is a **toggle, presented during onboarding, default ON**:

- **Onboarding**: one step, checkbox pre-checked, showing *exactly* what will be
  installed — the literal hook JSON and the one command it runs. Transparency is
  non-negotiable (see integrity guard below: we use the same mechanism malware
  abuses, so we must be auditably clean).
- **ON**: hook installs (merge into `~/.claude/settings.json`, never clobber),
  data gathers, and the enrichment-dependent panels (plugin A/B, roster history)
  appear in the dashboard.
- **OFF**: hook is removed. The dependent panels don't vanish — they render as
  **ghost panels**: greyed placeholder cards showing what the panel *would*
  answer ("Which plugin changes actually saved you tokens?") with an inline
  enable button. Passive, in-place, never notifications or nags — consistent
  with the Flarepoint inform-don't-enforce ethos.
- Toggleable at any time from settings; state changes are recorded as markers
  so stats honestly show coverage gaps.

### Config integrity guard

Motivated directly by the **Miasma worm** (TeamPCP/UNC6780, June 2026): a
self-spreading supply-chain worm that persisted by injecting into the
SessionStart hooks of 13 AI coding tools — including Claude Code via
repo-carried `.claude/settings.json` — executing a credential stealer the
moment a developer opened a compromised repo. fp-artificiency already watches
`~/.claude`; guarding the config is a natural extension.

- **Scope**: user-level `~/.claude/settings.json` (+ `.local`), and
  project-level `.claude/settings.json` for every project seen in
  `~/.claude/projects` — project-level is the actual Miasma vector.
  Highest-sensitivity keys: `hooks` (anything that executes), `mcpServers`,
  `apiKeyHelper`, `env`.
- **Mechanic**: baseline hash per config file; on change, diff and classify.
  New or modified hook commands trigger a warning with the literal diff.
  Heuristics flag known-bad shapes (base64 blobs, `curl … | sh`, writes to
  shell profiles / `.pth` files, unexpected network fetches) with a stated
  reason.
- **Self-attestation**: the app records the exact content of its own enricher
  hook. It never false-alarms on itself, and — the flip side — if *our* hook
  is ever modified by something else, that is itself a high-severity alert.
- **Surface, don't act**: warn with evidence and a suggested action (review
  diff, remove hook); never auto-modify the user's config beyond our own hook.
  Fail open: guard errors must never block the app or Claude Code.
- Positioning note: this is defense-in-depth alongside Claude Code's own trust
  prompts — Miasma proved people click through those. "The usage app that also
  noticed the worm" is an excellent story.

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

## UI backlog (Floran's feedback, 2026-07-13 — not yet scheduled)

- **Date-range selector** with predefined options scaling from hours to months
  (e.g. 1h · 24h · 7d · 30d · 3mo · all). Must be one-click, UX-friendly, and
  apply across the whole dashboard.
- **Sidebar navigation** between multiple dashboards; the current view becomes
  "General overview", sidebar entry #1. Future entries: waste diagnosis, plugin
  impact, config integrity, per-project deep dives.
- **Term tooltips**: hoverable explanations on jargon (sessions, turns, tokens,
  cache reads/writes) so non-expert users can read the dashboard.

## Open questions

- Enricher install mechanics: exact merge/remove strategy for our SessionStart
  hook in `~/.claude/settings.json` (merge, don't clobber; fail open; restore
  on uninstall).
- Integrity guard baseline bootstrapping: first run sees configs already in an
  unknown state — trust-on-first-use vs. heuristic scan of the initial state.
- Guard alert surface: dashboard-only, or also a system notification for
  high-severity hits (a worm in a hook arguably justifies breaking the
  no-notifications rule)?
- Pricing table refresh source + cadence; offline behavior.
- Retention: transcripts get cleaned up by CC (`cleanupPeriodDays`) — our SQLite
  store becomes the durable history. Import-once semantics and dedup on rescan.
- Marker capture without the enricher: can plugin roster be inferred from
  `~/.claude/settings.json` mtime snapshots as a degraded fallback?
- Second collector choice when the time comes (Codex CLI logs vs OpenRouter
  billing — the latter exercises the `aggregate` granularity path better).
