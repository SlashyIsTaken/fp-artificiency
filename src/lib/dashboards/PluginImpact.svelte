<script lang="ts">
  import { onMount } from "svelte";
  import {
    getSeriesByModel,
    getPluginEvents,
    getMetricDistribution,
    getHookOverhead,
    inTauri,
  } from "../api";
  import type { ModelBucket, PluginEvent, Distribution, HookOverhead } from "../api";
  import PluginTimeline from "../components/PluginTimeline.svelte";
  import type { Point, Event } from "../components/PluginTimeline.svelte";
  import RangeSelector from "../components/RangeSelector.svelte";
  import type { RangePreset } from "../components/RangeSelector.svelte";
  import { PRESETS, fmt, money } from "../presets";

  type MetricKey = "output" | "cost" | "input" | "cache_read";
  interface MetricDef {
    label: string;
    unit: string;
    money?: boolean;
    field: (r: ModelBucket) => number;
  }
  const METRICS: Record<MetricKey, MetricDef> = {
    output: { label: "Output / turn", unit: "output tok/turn", field: (r) => r.tokens_out },
    cost: { label: "Cost / turn", unit: "$/turn", money: true, field: (r) => r.cost },
    input: { label: "Input / turn", unit: "input tok/turn", field: (r) => r.tokens_in },
    cache_read: { label: "Cache read / turn", unit: "cache tok/turn", field: (r) => r.cache_read },
  };
  const METRIC_KEYS = Object.keys(METRICS) as MetricKey[];

  // Default to the widest range so the full event timeline is visible.
  let range = $state<RangePreset>(PRESETS[PRESETS.length - 1]);
  let metric = $state<MetricKey>("output");
  let rows = $state<ModelBucket[]>([]);
  let events = $state<PluginEvent[]>([]);
  let hooks = $state<HookOverhead[]>([]);
  let selected = $state<number | null>(null); // index into `events` for before/after
  let before = $state<Distribution | null>(null);
  let after = $state<Distribution | null>(null);
  let status = $state<"loading" | "ready" | "browser" | "error">("loading");
  let errorMsg = $state("");

  // Local-time ms for a bucket key ("YYYY-MM-DD" or "YYYY-MM-DDTHH:MM").
  function keyToMs(key: string): number {
    return new Date(key.length === 10 ? `${key}T00:00` : key).getTime();
  }

  // Fold per-(bucket,model) rows into one efficiency point per bucket:
  // the chosen metric summed over models, divided by that bucket's turns.
  const points = $derived.by<Point[]>(() => {
    const field = METRICS[metric].field;
    const byBucket = new Map<string, { val: number; turns: number }>();
    for (const r of rows) {
      const b = byBucket.get(r.bucket) ?? { val: 0, turns: 0 };
      b.val += field(r);
      b.turns += r.turns;
      byBucket.set(r.bucket, b);
    }
    return [...byBucket.entries()]
      .map(([bucket, b]) => ({ t: keyToMs(bucket), value: b.turns ? b.val / b.turns : 0 }))
      .sort((a, b) => a.t - b.t);
  });

  const eventPoints = $derived<Event[]>(
    events.map((e) => ({
      t: new Date(e.ts).getTime(),
      kind: e.kind,
      label: e.plugin.split("@")[0], // drop the marketplace suffix
    })),
  );

  const fmtValue = $derived(METRICS[metric].money ? (n: number) => money(n) : fmt);

  // ── Before/after distribution around a selected event ────────────────────
  const WINDOW_MS = 14 * 86400 * 1000; // window each side of the event

  const eventOptions = $derived(
    events.map((e, i) => ({ i, e })).sort((a, b) => b.e.ts.localeCompare(a.e.ts)),
  );

  let seq = 0; // guards against a stale async result overwriting a newer one
  $effect(() => {
    const idx = selected;
    const m = metric; // re-run when the metric changes too
    if (idx === null || !events[idx]) {
      before = null;
      after = null;
      return;
    }
    const evT = new Date(events[idx].ts).getTime();
    const bStart = new Date(evT - WINDOW_MS).toISOString();
    const evIso = new Date(evT).toISOString();
    const aEnd = new Date(Math.min(Date.now(), evT + WINDOW_MS)).toISOString();
    const mine = ++seq;
    Promise.all([
      getMetricDistribution(bStart, evIso, m),
      getMetricDistribution(evIso, aEnd, m),
    ])
      .then(([b, a]) => {
        if (mine === seq) {
          before = b;
          after = a;
        }
      })
      .catch(() => {
        if (mine === seq) {
          before = null;
          after = null;
        }
      });
  });

  const conf = $derived.by(() => {
    if (before === null || after === null || selected === null) return null;
    const cov = Math.min(before.count, after.count);
    const evT = new Date(events[selected].ts).getTime();
    const confounds = events.filter(
      (e, i) => i !== selected && Math.abs(new Date(e.ts).getTime() - evT) <= WINDOW_MS,
    ).length;
    if (cov < 20) return { level: "low", note: `thin data (as few as ${cov} turns in a window)` };
    if (confounds > 0)
      return {
        level: "low",
        note: `${confounds} other plugin change${confounds > 1 ? "s" : ""} within the window`,
      };
    return { level: "moderate", note: "roster stable across both windows" };
  });

  const scaleMax = $derived(Math.max(before?.p75 ?? 0, after?.p75 ?? 0, 1));
  const pct = (v: number) => Math.min(100, (v / scaleMax) * 100);

  async function loadSeries() {
    [rows, hooks] = await Promise.all([
      getSeriesByModel(range.hours, range.bucket),
      getHookOverhead(range.hours),
    ]);
  }

  async function selectRange(p: RangePreset) {
    range = p;
    try {
      await loadSeries();
    } catch (e) {
      errorMsg = String(e);
      status = "error";
    }
  }

  onMount(async () => {
    if (!inTauri()) {
      status = "browser";
      return;
    }
    try {
      [rows, events, hooks] = await Promise.all([
        getSeriesByModel(range.hours, range.bucket),
        getPluginEvents(),
        getHookOverhead(range.hours),
      ]);
      if (events.length) selected = events.length - 1; // default to the newest event
      status = "ready";
    } catch (e) {
      errorMsg = String(e);
      status = "error";
    }
  });
</script>

<header>
  <div>
    <h1>Plugin impact</h1>
    <span class="note-inline">
      Efficiency over time, normalized per turn so the line reflects how much each turn cost,
      not how much you worked. Vertical lines mark when a plugin was installed or removed.
    </span>
  </div>
  <RangeSelector presets={PRESETS} selected={range} onselect={selectRange} />
</header>

{#if status === "loading"}
  <p class="note">Loading…</p>
{:else if status === "browser"}
  <p class="note">
    Running in a plain browser. Start the desktop app with <code>npm run tauri dev</code>.
  </p>
{:else if status === "error"}
  <p class="note error">Error: {errorMsg}</p>
{:else}
  <div class="metric-row">
    {#each METRIC_KEYS as k}
      <button class="metric" class:active={metric === k} onclick={() => (metric = k)}>
        {METRICS[k].label}
      </button>
    {/each}
  </div>

  <section class="panel">
    {#if points.length === 0}
      <p class="empty">No activity in this range.</p>
    {:else}
      <PluginTimeline {points} events={eventPoints} unit={METRICS[metric].unit} format={fmtValue} />
    {/if}
  </section>

  {#if events.length > 0}
    <section class="panel">
      <div class="ba-head">
        <h2>Before / after an event</h2>
        <select onchange={(e) => (selected = Number(e.currentTarget.value))}>
          {#each eventOptions as o}
            <option value={o.i} selected={o.i === selected}>
              {o.e.kind} {o.e.plugin.split("@")[0]} · {o.e.ts.slice(0, 10)}
            </option>
          {/each}
        </select>
      </div>
      {#if before && after}
        {@const deltaMedian = after.median - before.median}
        <div class="ba-rows">
          {#each [{ label: "Before", d: before }, { label: "After", d: after }] as row}
            <div class="ba-row">
              <div class="ba-label">{row.label}</div>
              <div class="track">
                <div
                  class="iqr"
                  style="left: {pct(row.d.p25)}%; width: {pct(row.d.p75) - pct(row.d.p25)}%"
                ></div>
                <div class="median" style="left: {pct(row.d.median)}%"></div>
              </div>
              <div class="ba-stat">
                median {fmtValue(row.d.median)}
                <span>· n={row.d.count}, {row.d.sessions} sess</span>
              </div>
            </div>
          {/each}
        </div>
        <div class="ba-delta">
          Median {METRICS[metric].label}: {deltaMedian >= 0 ? "+" : ""}{fmtValue(deltaMedian)}
          {#if conf}
            <span class="conf {conf.level}">{conf.level} confidence</span>
            <span class="conf-note">({conf.note})</span>
          {/if}
        </div>
      {:else}
        <p class="empty">No turns in the 14-day windows around this event.</p>
      {/if}
      <p class="sub">
        Per-turn distribution 14 days before vs after. The bar spans the middle 50% (p25 to p75),
        the tick is the median.
      </p>
    </section>
  {/if}

  {#if hooks.length > 0}
    <section class="panel">
      <h2>Hook overhead</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Hook script</th>
              <th class="l">Plugin</th>
              <th>Calls</th>
              <th>Avg</th>
              <th>Total</th>
            </tr>
          </thead>
          <tbody>
            {#each hooks as h}
              <tr>
                <td class="mono">{h.script}</td>
                <td class="l plugin">{h.plugin.split("@")[0]}</td>
                <td>{h.calls}</td>
                <td>{h.avg_ms.toFixed(0)} ms</td>
                <td>{(h.total_ms / 1000).toFixed(1)} s</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <p class="sub">
        Time each plugin hook added, from the transcript hook records. Only hooks Claude Code logs
        there appear, so some (like the every-prompt injectors) are not counted.
      </p>
    </section>
  {/if}

  <p class="caveat">
    Impact here is correlational, not causal. Task mix varies day to day, and several plugins
    can change at once, so read a shift near an event as a hint to investigate, not proof.
  </p>
{/if}

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 1.25rem;
    gap: 1rem;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 1.15rem;
    font-weight: 700;
  }
  .note-inline {
    display: block;
    max-width: 60ch;
    margin-top: 0.2rem;
    color: var(--text-muted);
    font-size: 0.75rem;
  }
  .metric-row {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    margin-bottom: 0.75rem;
  }
  .metric {
    border: 1px solid var(--border);
    background: var(--surface-raised);
    color: var(--text-secondary);
    font: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.7rem;
    border-radius: 999px;
    cursor: pointer;
  }
  .metric:hover {
    color: var(--text-primary);
  }
  .metric.active {
    color: var(--text-primary);
    border-color: var(--series-1);
    box-shadow: inset 0 0 0 1px var(--series-1);
  }
  .panel {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1rem 1.15rem;
    margin-top: 0.85rem;
  }
  .empty {
    color: var(--text-muted);
    font-size: 0.85rem;
  }
  h2 {
    font-size: 0.9rem;
    font-weight: 600;
    margin-bottom: 0.6rem;
  }
  .ba-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 0.8rem;
  }
  .ba-head h2 {
    margin-bottom: 0;
  }
  select {
    /* strip the native control face so our themed background actually applies
       (WebKitGTK otherwise renders it light, hiding light-theme white text) */
    appearance: none;
    -webkit-appearance: none;
    background-color: var(--surface-raised);
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%238a887f' stroke-width='1.5'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 0.6rem center;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 7px;
    font: inherit;
    font-size: 0.8rem;
    padding: 0.3rem 1.7rem 0.3rem 0.55rem;
    cursor: pointer;
  }
  select option {
    background: var(--surface-raised);
    color: var(--text-primary);
  }
  .ba-rows {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .ba-row {
    display: grid;
    grid-template-columns: 54px 1fr auto;
    align-items: center;
    gap: 0.7rem;
  }
  .ba-label {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
  .track {
    position: relative;
    height: 12px;
    background: color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 4px;
  }
  .iqr {
    position: absolute;
    top: 0;
    height: 100%;
    background: color-mix(in srgb, var(--series-1) 35%, transparent);
    border-radius: 4px;
  }
  .median {
    position: absolute;
    top: -2px;
    width: 2px;
    height: 16px;
    background: var(--series-1);
    border-radius: 2px;
  }
  .ba-stat {
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .ba-stat span {
    color: var(--text-muted);
  }
  .ba-delta {
    margin-top: 0.8rem;
    font-size: 0.85rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .conf {
    margin-left: 0.5rem;
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
  }
  .conf.low {
    color: var(--status-warning);
    border: 1px solid var(--status-warning);
  }
  .conf.moderate {
    color: var(--series-2);
    border: 1px solid var(--series-2);
  }
  .conf-note {
    font-weight: 400;
    color: var(--text-muted);
    font-size: 0.75rem;
    margin-left: 0.3rem;
  }
  .sub {
    margin-top: 0.6rem;
    color: var(--text-muted);
    font-size: 0.72rem;
    max-width: 70ch;
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
  th:first-child,
  th.l {
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
  td.l {
    text-align: left;
  }
  td.mono {
    text-align: left;
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
  }
  td.plugin {
    color: var(--text-secondary);
  }
  .caveat {
    margin-top: 0.9rem;
    color: var(--text-muted);
    font-size: 0.75rem;
    max-width: 70ch;
  }
  .note {
    color: var(--text-secondary);
  }
  .note.error {
    color: var(--status-critical);
  }
</style>
