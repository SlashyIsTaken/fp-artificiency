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

-- Incremental ingestion bookkeeping: byte offset per source file.
CREATE TABLE IF NOT EXISTS ingest_files (
  path        TEXT PRIMARY KEY,
  byte_offset INTEGER NOT NULL DEFAULT 0,
  updated_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_events_session ON events (session_id);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events (ts);
CREATE INDEX IF NOT EXISTS idx_markers_ts ON markers (ts);
