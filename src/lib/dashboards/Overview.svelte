<script lang="ts">
  import { onMount } from "svelte";
  import {
    getOverview,
    runBackfill,
    getSeriesByModel,
    getSeriesSessions,
    getByModel,
    getSubscription,
    inTauri,
  } from "../api";
  import type { Overview, IngestReport, ModelBucket, SessionBucket, ModelUsage } from "../api";
  import StatTile from "../components/StatTile.svelte";
  import BarChart from "../components/BarChart.svelte";
  import type { Bar, Series } from "../components/BarChart.svelte";
  import RangeSelector from "../components/RangeSelector.svelte";
  import type { RangePreset } from "../components/RangeSelector.svelte";
  import { PRESETS, DEFAULT_PRESET, money } from "../presets";

  const TIPS: Record<string, string> = {
    Spend:
      "Estimated cost of this range at each provider's pay-as-you-go API token rates (input, output, and cache read/write). If you are on a subscription, this is what the usage would cost on the API, not what you paid. Computed locally. Models we don't price count as $0.",
    Sessions:
      "One Claude Code conversation, from open to close. Reopening a project or /clear starts a new session.",
    Turns:
      "One assistant reply within a session. Each turn reports its own token usage. Turns are the atoms of this dashboard.",
    "Input tokens":
      "Fresh (uncached) prompt tokens sent to the model, billed at the full input rate. Most prompt volume is usually served from cache instead.",
    "Output tokens":
      "Tokens the model generated: prose, code, and tool calls. Usually the most expensive per token.",
    "Cache reads":
      "Prompt tokens served from the provider's prompt cache instead of being resent, far cheaper than fresh input. High is good.",
    "Cache writes":
      "Tokens written into the prompt cache at a small premium so that later turns can read them back cheaply.",
  };

  // Which metric the trend chart shows. Every stat tile is a selector for one
  // of these: clicking a tile drills its total into a per-bucket time series.
  type MetricKey =
    | "spend"
    | "sessions"
    | "turns"
    | "input"
    | "output"
    | "cache_read"
    | "cache_write";
  interface MetricDef {
    label: string;
    unit: string;
    money?: boolean;
    // Stacked metrics extract a value from each per-model bucket row; Sessions
    // has no field and renders as a single (non-model-attributable) series.
    field?: (r: ModelBucket) => number;
  }
  const METRICS: Record<MetricKey, MetricDef> = {
    spend: { label: "Spend", unit: "USD", money: true, field: (r) => r.cost },
    sessions: { label: "Sessions", unit: "sessions" },
    turns: { label: "Turns", unit: "turns", field: (r) => r.turns },
    input: { label: "Input tokens", unit: "input tokens", field: (r) => r.tokens_in },
    output: { label: "Output tokens", unit: "output tokens", field: (r) => r.tokens_out },
    cache_read: { label: "Cache reads", unit: "cache-read tokens", field: (r) => r.cache_read },
    cache_write: { label: "Cache writes", unit: "cache-write tokens", field: (r) => r.cache_write },
  };

  let metric = $state<MetricKey>("output");
  let range = $state<RangePreset>(DEFAULT_PRESET);
  let overview = $state<Overview | null>(null);
  let ingest = $state<IngestReport | null>(null);
  let subscription = $state<string | null>(null);
  let models = $state<ModelUsage[]>([]);
  let modelRows = $state<ModelBucket[]>([]);
  let sessionRows = $state<SessionBucket[]>([]);
  // Per-model series for stacked metrics. Colors are fixed from the all-time
  // output ranking, assigned once, so switching range or metric never repaints
  // a model.
  let modelSeries = $state<Series[]>([]);
  let modelSlot = new Map<string, number>();

  async function initSeries() {
    const all = await getByModel(0, false); // all-time: alignment is a no-op
    const top = all.slice(0, 3);
    modelSlot = new Map(top.map((m, i) => [m.model, i]));
    modelSeries = top.map((m, i) => ({ name: m.model, slot: i }));
    if (all.length > 3) {
      modelSeries = [...modelSeries, { name: "Other", slot: 3 }];
    }
  }
  let status = $state<"loading" | "ready" | "error" | "browser">("loading");
  let errorMsg = $state("");

  const compact = new Intl.NumberFormat("en", {
    notation: "compact",
    maximumFractionDigits: 1,
  });
  const fmt = (n: number) => compact.format(n);
  const day = (ts: string | null) => (ts ? ts.slice(0, 10) : "—");

  const pad = (n: number) => String(n).padStart(2, "0");

  // Local-time bucket key, matching the SQL 'localtime' bucket expressions.
  function keyOf(d: Date, bucket: RangePreset["bucket"]): string {
    const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
    if (bucket === "day") return date;
    const min = bucket === "hour" ? "00" : pad(d.getMinutes());
    return `${date}T${pad(d.getHours())}:${min}`;
  }

  // Bucket → slot → value for the active metric: stacked per model, or a single
  // slot-0 series for Sessions (which isn't model-attributable).
  function bucketMap(): Map<string, Map<number, number>> {
    const def = METRICS[metric];
    const byKey = new Map<string, Map<number, number>>();
    if (def.field) {
      for (const r of modelRows) {
        const slot = modelSlot.get(r.model) ?? 3; // unmapped models fold into Other
        const m = byKey.get(r.bucket) ?? new Map<number, number>();
        m.set(slot, (m.get(slot) ?? 0) + def.field(r));
        byKey.set(r.bucket, m);
      }
    } else {
      for (const r of sessionRows) byKey.set(r.bucket, new Map([[0, r.sessions]]));
    }
    return byKey;
  }

  // The store omits empty buckets; rebuild the full expected axis and stack.
  function buildAxis(
    byKey: Map<string, Map<number, number>>,
    series: Series[],
    preset: RangePreset,
    firstTs: string | null,
  ): Bar[] {
    const stepMs = preset.bucket === "minute" ? 60_000 : preset.bucket === "hour" ? 3_600_000 : 86_400_000;
    const now = new Date();
    let start: Date;
    if (preset.hours > 0) {
      const raw = now.getTime() - preset.hours * 3_600_000;
      if (preset.bucket === "day") {
        // Match the backend's day-aligned window: snap the first bucket to
        // local midnight so the axis left edge doesn't slide with the clock.
        const d = new Date(raw);
        d.setHours(0, 0, 0, 0);
        start = d;
      } else {
        start = new Date(raw + stepMs);
      }
    } else {
      start = firstTs ? new Date(firstTs) : now;
    }
    const out: Bar[] = [];
    for (let t = start.getTime(); t <= now.getTime(); t += stepMs) {
      const key = keyOf(new Date(t), preset.bucket);
      const m = byKey.get(key);
      out.push({
        label: key.replace("T", " "),
        segments: series.map((s) => ({ slot: s.slot, name: s.name, value: m?.get(s.slot) ?? 0 })),
      });
    }
    // Sparse ticks: ~6 across, last bucket labeled unless a regular tick crowds it.
    const step = Math.max(1, Math.ceil(out.length / 6));
    const short = (label: string) =>
      preset.bucket === "day" ? label.slice(5, 10) : label.slice(11);
    out.forEach((b, i) => {
      if ((i % step === 0 && out.length - 1 - i >= step / 2) || i === out.length - 1) {
        b.tick = short(b.label);
      }
    });
    return out;
  }

  const activeMetric = $derived(METRICS[metric]);
  const chartSeries: Series[] = $derived(
    activeMetric.field ? modelSeries : [{ name: "Sessions", slot: 0 }],
  );
  // Recomputes on metric change with no refetch — switching tiles is instant.
  const bars: Bar[] = $derived(
    overview ? buildAxis(bucketMap(), chartSeries, range, overview.first_ts) : [],
  );

  async function load() {
    const dayAligned = range.bucket === "day";
    overview = await getOverview(range.hours, dayAligned);
    [modelRows, sessionRows, models] = await Promise.all([
      getSeriesByModel(range.hours, range.bucket),
      getSeriesSessions(range.hours, range.bucket),
      getByModel(range.hours, dayAligned),
    ]);
  }

  async function selectRange(p: RangePreset) {
    range = p;
    try {
      await load();
    } catch (e) {
      errorMsg = String(e);
      status = "error";
    }
  }

  const POLL_MS = 3000;
  let polling = false;

  // Live refresh: re-scan for new activity, then reload. Cheap — the ingest is
  // incremental (only appended bytes). Skipped while hidden; a transient error
  // keeps the last-good view rather than flipping to an error state.
  async function poll() {
    if (polling || document.hidden) return;
    polling = true;
    try {
      await runBackfill();
      await load();
    } catch {
      /* transient; keep the last good data */
    } finally {
      polling = false;
    }
  }

  onMount(() => {
    if (!inTauri()) {
      status = "browser";
      return;
    }
    let timer: ReturnType<typeof setInterval> | undefined;
    const onVisible = () => {
      if (!document.hidden) poll(); // catch up the instant the window is shown
    };
    // Kick off async init, then start the visible-only poll loop. onMount's
    // cleanup only registers when returned synchronously, so init runs detached.
    (async () => {
      try {
        ingest = await runBackfill();
        await initSeries();
        await load();
        subscription = await getSubscription().catch(() => null);
        status = "ready";
      } catch (e) {
        errorMsg = String(e);
        status = "error";
        return;
      }
      timer = setInterval(poll, POLL_MS);
      document.addEventListener("visibilitychange", onVisible);
    })();
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  });

  const bucketName = $derived(
    range.bucket === "day" ? "day" : range.bucket === "hour" ? "hour" : "minute",
  );
</script>

<header>
  <div>
    <h1>General overview</h1>
    {#if overview?.first_ts}
      <span class="range-note">{day(overview.first_ts)} → {day(overview.last_ts)}</span>
    {/if}
  </div>
  <RangeSelector presets={PRESETS} selected={range} onselect={selectRange} />
</header>

{#if status === "loading"}
  <p class="note">Reading Claude Code transcripts…</p>
{:else if status === "browser"}
  <p class="note">
    Running in a plain browser — start the desktop app with <code>npm run tauri dev</code>
    to connect to the collector backend.
  </p>
{:else if status === "error"}
  <p class="note error">Collector error: {errorMsg}</p>
{:else if overview}
  <section class="tiles">
    <StatTile label="Spend" num={overview.cost} format={money} hint="at API rates" tip={TIPS["Spend"]}
      onselect={() => (metric = "spend")} active={metric === "spend"} />
    <StatTile label="Sessions" num={overview.sessions} format={fmt} tip={TIPS["Sessions"]}
      onselect={() => (metric = "sessions")} active={metric === "sessions"} />
    <StatTile label="Turns" num={overview.turns} format={fmt} tip={TIPS["Turns"]}
      onselect={() => (metric = "turns")} active={metric === "turns"} />
    <StatTile label="Input tokens" num={overview.tokens_in} format={fmt} hint="uncached" tip={TIPS["Input tokens"]}
      onselect={() => (metric = "input")} active={metric === "input"} />
    <StatTile label="Output tokens" num={overview.tokens_out} format={fmt} tip={TIPS["Output tokens"]}
      onselect={() => (metric = "output")} active={metric === "output"} />
    <StatTile label="Cache reads" num={overview.cache_read} format={fmt} hint="tokens served from cache" tip={TIPS["Cache reads"]}
      onselect={() => (metric = "cache_read")} active={metric === "cache_read"} />
    <StatTile label="Cache writes" num={overview.cache_write} format={fmt} hint="tokens written to cache" tip={TIPS["Cache writes"]}
      onselect={() => (metric = "cache_write")} active={metric === "cache_write"} />
  </section>

  {#if subscription}
    <p class="api-note">
      You are on a Claude subscription ({subscription}), so Spend is what this usage would cost
      on the pay-as-you-go API, not what you paid.
    </p>
  {/if}

  <section class="panel">
    <h2>
      {activeMetric.label} per {bucketName}
      <span class="sub">
        {range.label === "All" ? "all time" : `last ${range.label}`}
        {activeMetric.field ? "· by model" : "· distinct sessions active"}
      </span>
    </h2>
    <p class="pickhint">Click any tile above to change this chart.</p>
    <svelte:boundary onerror={(e) => console.error("BarChart error:", e)}>
      <BarChart {bars} series={chartSeries} unit={activeMetric.unit} money={activeMetric.money} animate={metric} />
      {#snippet failed(error)}
        <p class="note error">Chart couldn't render: {error instanceof Error ? error.message : String(error)}</p>
      {/snippet}
    </svelte:boundary>
  </section>

  {#if models.length > 0}
    <section class="panel">
      <h2>By model</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Model</th>
              <th>Turns</th>
              <th>Input</th>
              <th>Output</th>
              <th>Cache read</th>
              <th>Cache write</th>
              <th>Cost</th>
            </tr>
          </thead>
          <tbody>
            {#each models as m}
              <tr>
                <td class="model">{m.model}</td>
                <td>{fmt(m.turns)}</td>
                <td>{fmt(m.tokens_in)}</td>
                <td>{fmt(m.tokens_out)}</td>
                <td>{fmt(m.cache_read)}</td>
                <td>{fmt(m.cache_write)}</td>
                <td class="cost">{m.cost > 0 ? money(m.cost) : "—"}</td>
              </tr>
            {/each}
          </tbody>
          <tfoot>
            <tr>
              <td class="model">Total</td>
              <td colspan="5"></td>
              <td class="cost">{money(overview.cost)}</td>
            </tr>
          </tfoot>
        </table>
      </div>
    </section>
  {/if}


  {#if ingest && overview.db_path}
    <footer>
      Ingested {ingest.events_added} new turns from {ingest.files_seen} transcripts ·
      store: <code>{overview.db_path}</code>
    </footer>
  {/if}
{/if}

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.25rem;
    gap: 1rem;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 1.15rem;
    font-weight: 700;
  }
  .range-note {
    color: var(--text-muted);
    font-size: 0.78rem;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 0.75rem;
  }
  .panel {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1rem 1.15rem;
    margin-top: 0.75rem;
  }
  h2 {
    font-size: 0.9rem;
    font-weight: 600;
    margin-bottom: 0.75rem;
  }
  h2 .sub {
    font-weight: 400;
    color: var(--text-muted);
    font-size: 0.8rem;
  }
  .pickhint {
    color: var(--text-muted);
    font-size: 0.72rem;
    margin: -0.4rem 0 0.6rem;
  }
  .table-wrap {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
  }
  th {
    text-align: right;
    font-weight: 500;
    color: var(--text-secondary);
    padding: 0.35rem 0.6rem;
    border-bottom: 1px solid var(--border);
  }
  th:first-child {
    text-align: left;
  }
  td {
    text-align: right;
    padding: 0.35rem 0.6rem;
    font-variant-numeric: tabular-nums;
    border-bottom: 1px solid var(--border);
  }
  tr:last-child td {
    border-bottom: none;
  }
  td.model {
    text-align: left;
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
  }
  td.cost {
    font-variant-numeric: tabular-nums;
  }
  tfoot td {
    border-top: 1px solid var(--border);
    border-bottom: none;
    font-weight: 600;
    padding-top: 0.5rem;
  }
  tfoot td.model {
    font-family: inherit;
    color: var(--text-secondary);
  }
  .api-note {
    margin-top: 0.7rem;
    color: var(--text-muted);
    font-size: 0.78rem;
    max-width: 70ch;
  }
  .note {
    color: var(--text-secondary);
  }
  .note.error {
    color: #d03b3b;
  }
  footer {
    margin-top: 1.5rem;
    font-size: 0.78rem;
    color: var(--text-muted);
  }
  code {
    font-size: 0.95em;
  }
</style>
