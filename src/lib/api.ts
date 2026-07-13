import { invoke } from "@tauri-apps/api/core";

export interface Overview {
  sessions: number;
  turns: number;
  tokens_in: number;
  tokens_out: number;
  cache_read: number;
  cache_write: number;
  first_ts: string | null;
  last_ts: string | null;
  db_path: string | null;
}

export interface IngestReport {
  files_seen: number;
  files_read: number;
  events_added: number;
  lines_skipped: number;
}

export interface UsageBucket {
  bucket: string;
  turns: number;
  tokens_in: number;
  tokens_out: number;
  cache_read: number;
  cache_write: number;
}

export interface ModelUsage {
  model: string;
  turns: number;
  tokens_in: number;
  tokens_out: number;
  cache_read: number;
  cache_write: number;
}

export function inTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

// hours = 0 means all time.
export const getOverview = (hours: number) => invoke<Overview>("overview", { hours });
export const runBackfill = () => invoke<IngestReport>("backfill");
export const getSeries = (hours: number, bucket: string) =>
  invoke<UsageBucket[]>("series", { hours, bucket });
export const getByModel = (hours: number) => invoke<ModelUsage[]>("by_model", { hours });
