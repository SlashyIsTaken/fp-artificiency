-- fp-artificiency normalized store. Two granularities by design (see DESIGN.md):
-- 'turn' for rich transcript sources, 'aggregate' for billing-style sources.

CREATE TABLE IF NOT EXISTS sessions (
  id          TEXT PRIMARY KEY,
  provider    TEXT NOT NULL,
  project     TEXT,
  started_at  TEXT,
  meta        TEXT
);

CREATE TABLE IF NOT EXISTS events (
  id          INTEGER PRIMARY KEY,
  session_id  TEXT NOT NULL,
  ts          TEXT NOT NULL,
  granularity TEXT NOT NULL CHECK (granularity IN ('turn', 'aggregate')),
  model       TEXT,
  tokens_in   INTEGER NOT NULL DEFAULT 0,
  tokens_out  INTEGER NOT NULL DEFAULT 0,
  cache_read  INTEGER NOT NULL DEFAULT 0,
  cache_write INTEGER NOT NULL DEFAULT 0,
  cost        REAL,
  dedup_key   TEXT UNIQUE,
  meta        TEXT
);

CREATE TABLE IF NOT EXISTS markers (
  id          INTEGER PRIMARY KEY,
  session_id  TEXT,
  ts          TEXT NOT NULL,
  kind        TEXT NOT NULL,
  payload     TEXT
);

-- One row per tool invocation, for waste diagnosis. `result_chars` is the
-- serialized size of the tool result (filled in when the result record is
-- seen); a rough proxy for context volume (~4 chars per token).
CREATE TABLE IF NOT EXISTS tool_calls (
  id           INTEGER PRIMARY KEY,
  session_id   TEXT NOT NULL,
  ts           TEXT NOT NULL,
  tool         TEXT NOT NULL,
  target       TEXT,
  result_chars INTEGER,
  dedup_key    TEXT UNIQUE,
  meta         TEXT
);

CREATE INDEX IF NOT EXISTS idx_tool_calls_ts ON tool_calls (ts);
CREATE INDEX IF NOT EXISTS idx_tool_calls_lookup ON tool_calls (session_id, tool, target);

-- One row per plugin hook invocation (from transcript `hookInfos`), for the
-- plugin-impact hook-overhead view. `script` is the hook script basename;
-- `${CLAUDE_PLUGIN_ROOT}` in the command is unexpanded, so a shared script name
-- can't always be attributed to a single plugin.
CREATE TABLE IF NOT EXISTS hook_calls (
  id          INTEGER PRIMARY KEY,
  session_id  TEXT NOT NULL,
  ts          TEXT NOT NULL,
  script      TEXT NOT NULL,
  duration_ms INTEGER,
  dedup_key   TEXT UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_hook_calls_ts ON hook_calls (ts);

-- Small key/value cache. Used for last-good subscription limits so a transient
-- fetch failure (e.g. a 429 on the shared OAuth token) doesn't blank the widget.
CREATE TABLE IF NOT EXISTS kv (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT
);

-- Incremental ingestion bookkeeping: byte offset per source file.
CREATE TABLE IF NOT EXISTS ingest_files (
  path        TEXT PRIMARY KEY,
  byte_offset INTEGER NOT NULL DEFAULT 0,
  updated_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_events_session ON events (session_id);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events (ts);
CREATE INDEX IF NOT EXISTS idx_markers_ts ON markers (ts);
