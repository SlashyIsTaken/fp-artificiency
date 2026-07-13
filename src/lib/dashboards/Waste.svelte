<script lang="ts">
  import { onMount } from "svelte";
  import {
    getWasteSummary,
    getDuplicateReads,
    getLargestResults,
    getToolStats,
  } from "../api";
  import type { WasteSummary, DupRead, BigResult, ToolStat } from "../api";
  import StatTile from "../components/StatTile.svelte";
  import RangeSelector from "../components/RangeSelector.svelte";
  import type { RangePreset } from "../components/RangeSelector.svelte";
  import { PRESETS, DEFAULT_PRESET, fmt, estTokens } from "../presets";

  let range = $state<RangePreset>(DEFAULT_PRESET);
  let summary = $state<WasteSummary | null>(null);
  let dups = $state<DupRead[]>([]);
  let biggest = $state<BigResult[]>([]);
  let tools = $state<ToolStat[]>([]);
  let errorMsg = $state("");

  // Sizes are chars of serialized tool output — honest label, token estimate
  // only as a hint (~4 chars/token).
  const vol = (chars: number) => `≈${fmt(estTokens(chars))} tok`;

  const when = (ts: string) => ts.slice(0, 16).replace("T", " ");

  const shorten = (t: string | null) => {
    if (!t) return "—";
    return t.length > 64 ? "…" + t.slice(-63) : t;
  };

  async function load() {
    try {
      summary = await getWasteSummary(range.hours);
      dups = await getDuplicateReads(range.hours);
      biggest = await getLargestResults(range.hours);
      tools = await getToolStats(range.hours);
      errorMsg = "";
    } catch (e) {
      errorMsg = String(e);
    }
  }

  onMount(load);
</script>

<header>
  <div>
    <h1>Waste diagnosis</h1>
    <span class="note-inline">sizes are measured output volume, shown as estimated tokens (~4 chars each)</span>
  </div>
  <RangeSelector presets={PRESETS} selected={range} onselect={(p) => { range = p; load(); }} />
</header>

{#if errorMsg}
  <p class="error">Collector error: {errorMsg}</p>
{:else if summary}
  <section class="tiles">
    <StatTile
      label="Tool calls"
      value={fmt(summary.tool_calls)}
      tip="Every tool invocation (file reads, shell commands, searches …) in the range. Each result is sent back into the model's context — tool output is input you pay for."
    />
    <StatTile
      label="Redundant re-reads"
      value={fmt(summary.extra_reads)}
      hint="same file, same session"
      tip="Times a file was read again in the same session. The model usually already had the content in context — the repeat is mostly waste."
    />
    <StatTile
      label="Re-read volume"
      value={vol(summary.wasted_chars)}
      hint="estimated"
      tip="Estimated volume of the redundant re-reads, valuing each repeat at that file's average result size. Estimate, not a bill."
    />
    <StatTile
      label="Biggest single output"
      value={vol(summary.biggest_chars)}
      tip="The largest single tool result in the range. One oversized dump (a huge log, a giant file) can dominate a session's input cost."
    />
  </section>

  {#if dups.length > 0}
    <section class="panel">
      <h2>Most re-read files</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>File</th>
              <th>Reads</th>
              <th>Redundant</th>
              <th>Sessions</th>
              <th>Est. waste</th>
            </tr>
          </thead>
          <tbody>
            {#each dups as d}
              <tr>
                <td class="target" title={d.target}>{shorten(d.target)}</td>
                <td>{fmt(d.reads)}</td>
                <td>{fmt(d.extra)}</td>
                <td>{fmt(d.sessions)}</td>
                <td>{vol(d.wasted_chars)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {:else}
    <section class="panel empty">No redundant re-reads in this range. Clean.</section>
  {/if}

  {#if biggest.length > 0}
    <section class="panel">
      <h2>Largest tool outputs</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Tool</th>
              <th class="l">Target</th>
              <th>Size</th>
              <th>When</th>
            </tr>
          </thead>
          <tbody>
            {#each biggest as b}
              <tr>
                <td class="mono">{b.tool}</td>
                <td class="target" title={b.target}>{shorten(b.target)}</td>
                <td>{vol(b.chars)}</td>
                <td class="mono">{when(b.ts)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}

  {#if tools.length > 0}
    <section class="panel">
      <h2>Output volume by tool</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Tool</th>
              <th>Calls</th>
              <th>Total output</th>
            </tr>
          </thead>
          <tbody>
            {#each tools as t}
              <tr>
                <td class="mono">{t.tool}</td>
                <td>{fmt(t.calls)}</td>
                <td>{vol(t.chars)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
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
  .note-inline {
    color: var(--text-muted);
    font-size: 0.75rem;
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
  .panel.empty {
    color: var(--text-muted);
    font-size: 0.85rem;
  }
  h2 {
    font-size: 0.9rem;
    font-weight: 600;
    margin-bottom: 0.75rem;
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
  td.target,
  td.mono {
    text-align: left;
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
  }
  td.target {
    max-width: 380px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error {
    color: #d03b3b;
  }
</style>
