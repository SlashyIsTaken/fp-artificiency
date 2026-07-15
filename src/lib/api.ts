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

export interface PluginEvent {
  plugin: string;
  kind: string; // installed | removed
  ts: string;
}

export interface Distribution {
  count: number;
  sessions: number;
  p25: number;
  median: number;
  p75: number;
  mean: number;
}

export interface HookOverhead {
  script: string;
  plugin: string;
  calls: number;
  total_ms: number;
  avg_ms: number;
}

export function inTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

// hours = 0 means all time. dayAligned clamps a day+ frame's start to local
// midnight (see since_clause) so its totals don't slide with the wall clock;
// pass `range.bucket === "day"`. Ignored server-side when hours = 0.
export const getOverview = (hours: number, dayAligned: boolean) =>
  invoke<Overview>("overview", { hours, dayAligned });
export const runBackfill = () => invoke<IngestReport>("backfill");
export const getSeries = (hours: number, bucket: string) =>
  invoke<UsageBucket[]>("series", { hours, bucket });
export const getByModel = (hours: number, dayAligned: boolean) =>
  invoke<ModelUsage[]>("by_model", { hours, dayAligned });
export const getWasteSummary = (hours: number, dayAligned: boolean) =>
  invoke<WasteSummary>("waste_summary", { hours, dayAligned });
export const getDuplicateReads = (hours: number, dayAligned: boolean) =>
  invoke<DupRead[]>("duplicate_reads", { hours, dayAligned });
export const getLargestResults = (hours: number, dayAligned: boolean) =>
  invoke<BigResult[]>("largest_results", { hours, dayAligned });
export const getToolStats = (hours: number, dayAligned: boolean) =>
  invoke<ToolStat[]>("tool_stats", { hours, dayAligned });
export const getSeriesByModel = (hours: number, bucket: string) =>
  invoke<ModelBucket[]>("series_by_model", { hours, bucket });
export const getSeriesSessions = (hours: number, bucket: string) =>
  invoke<SessionBucket[]>("series_sessions", { hours, bucket });
export const getUsageLimits = () => invoke<UsageLimit[] | null>("usage_limits");
export const getSubscription = () => invoke<string | null>("subscription");
export const getConfigIntegrity = () => invoke<ConfigFile[]>("config_integrity");
export const reviewConfig = (path: string) => invoke<void>("review_config", { path });
export const getPluginEvents = () => invoke<PluginEvent[]>("plugin_events");
export const getMetricDistribution = (start: string, end: string, metric: string) =>
  invoke<Distribution>("metric_distribution", { start, end, metric });
export const getHookOverhead = (hours: number, dayAligned: boolean) =>
  invoke<HookOverhead[]>("hook_overhead", { hours, dayAligned });
