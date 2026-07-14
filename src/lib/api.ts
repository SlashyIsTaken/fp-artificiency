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
  cost: number;
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
  cost: number;
}

export interface WasteSummary {
  tool_calls: number;
  extra_reads: number;
  wasted_chars: number;
  biggest_chars: number;
}

export interface DupRead {
  target: string;
  reads: number;
  extra: number;
  wasted_chars: number;
  sessions: number;
}

export interface BigResult {
  tool: string;
  target: string | null;
  chars: number;
  ts: string;
}

export interface ToolStat {
  tool: string;
  calls: number;
  chars: number;
}

export interface ModelBucket {
  bucket: string;
  model: string;
  turns: number;
  tokens_in: number;
  tokens_out: number;
  cache_read: number;
  cache_write: number;
  cost: number;
}

export interface SessionBucket {
  bucket: string;
  sessions: number;
}

export interface UsageLimit {
  kind: string;
  label: string;
  percent: number;
  severity: string;
  resets_at: string | null;
}

export interface ConfigFinding {
  key: string;
  change: string; // added | modified | removed
  severity: string; // info | warning | critical
  detail: string;
}

export interface ConfigFile {
  path: string;
  status: string; // watching | clean | changed
  findings: ConfigFinding[];
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
export const getWasteSummary = (hours: number) => invoke<WasteSummary>("waste_summary", { hours });
export const getDuplicateReads = (hours: number) => invoke<DupRead[]>("duplicate_reads", { hours });
export const getLargestResults = (hours: number) => invoke<BigResult[]>("largest_results", { hours });
export const getToolStats = (hours: number) => invoke<ToolStat[]>("tool_stats", { hours });
export const getSeriesByModel = (hours: number, bucket: string) =>
  invoke<ModelBucket[]>("series_by_model", { hours, bucket });
export const getSeriesSessions = (hours: number, bucket: string) =>
  invoke<SessionBucket[]>("series_sessions", { hours, bucket });
export const getUsageLimits = () => invoke<UsageLimit[] | null>("usage_limits");
export const getConfigIntegrity = () => invoke<ConfigFile[]>("config_integrity");
export const reviewConfig = (path: string) => invoke<void>("review_config", { path });
