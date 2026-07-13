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

export interface DailyUsage {
  day: string;
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

export const getOverview = () => invoke<Overview>("overview");
export const runBackfill = () => invoke<IngestReport>("backfill");
export const getDaily = (days: number) => invoke<DailyUsage[]>("daily", { days });
export const getByModel = () => invoke<ModelUsage[]>("by_model");
