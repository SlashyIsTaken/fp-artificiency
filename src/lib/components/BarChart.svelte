<script lang="ts">
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";

  export interface Segment {
    /// Index into the parent's series list; color = var(--series-{slot+1}).
    slot: number;
    name: string;
    value: number;
  }

  export interface Bar {
    label: string; // full label for the tooltip (e.g. "2026-07-13")
    tick?: string; // short axis tick; only some bars carry one
    segments: Segment[]; // stacked bottom-up in the given order
  }

  export interface Series {
    name: string;
    slot: number;
  }

  let {
    bars,
    series,
    unit,
    money = false,
    animate,
  }: {
    bars: Bar[];
    series: Series[];
    unit: string;
    money?: boolean;
    // Changing this token replays the grow-in transition — the parent passes
    // the active metric, so swapping data source animates the bars.
    animate?: string;
  } = $props();

  const W = 720;
  const H = 190;
  const PAD_L = 44;
  const PAD_B = 20;
  const PAD_T = 22; // headroom so the peak's direct label never clips

  // Money metrics (spend) format as compact/full USD; everything else as plain
  // numbers. Axis + peak use the compact form; the tooltip uses the full form.
  const numCompact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
  const usdCompact = new Intl.NumberFormat("en", {
    style: "currency",
    currency: "USD",
    notation: "compact",
    maximumFractionDigits: 1,
  });
  const usdFull = new Intl.NumberFormat("en", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  });
  const numFull = new Intl.NumberFormat("en");
  const compact = $derived(money ? usdCompact : numCompact);
  const full = $derived(money ? usdFull : numFull);

  let hovered = $state<number | null>(null);

  const total = (b: Bar) => b.segments.reduce((s, x) => s + x.value, 0);
  const max = $derived(Math.max(1, ...bars.map(total)));
  const peak = $derived(bars.findIndex((b) => total(b) === max && max > 0));
  const innerW = W - PAD_L - 6;
  const innerH = H - PAD_T - PAD_B;
  const base = PAD_T + innerH;
  const slotW = $derived(innerW / Math.max(1, bars.length));
  const barW = $derived(Math.max(2, slotW - 2)); // 2px surface gap between bars
  const x = (i: number) => PAD_L + i * slotW + (slotW - barW) / 2;
  const h = (v: number) => (innerH * v) / max;

  // Rounded top corners for the topmost segment; square base at the baseline.
  function topPath(bx: number, y: number, height: number): string {
    const r = Math.min(2, height, barW / 2);
    return `M${bx},${y + height} L${bx},${y + r} Q${bx},${y} ${bx + r},${y}
            L${bx + barW - r},${y} Q${bx + barW},${y} ${bx + barW},${y + r}
            L${bx + barW},${y + height} Z`;
  }

  // Stack geometry, bottom-up, with a 2px surface gap between segments
  // (skipped for slivers that would disappear).
  function stack(b: Bar, i: number) {
    const out: { seg: Segment; y: number; height: number; top: boolean }[] = [];
    let cum = 0;
    const drawn = b.segments.filter((s) => s.value > 0);
    drawn.forEach((seg, j) => {
      const y0 = base - h(cum);
      cum += seg.value;
      const y1 = base - h(cum);
      let y = y1;
      let height = y0 - y1;
      if (j > 0 && height > 3) {
        height -= 2; // gap eats into this segment's bottom
      }
      out.push({ seg, y, height, top: j === drawn.length - 1 });
    });
    return out.map((s) => ({ ...s, bx: x(i) }));
  }

  const gridLines = $derived(
    [0.5, 1].map((f) => ({ v: max * f, gy: base - h(max * f) })),
  );
</script>

<div class="chart">
  {#key animate}
  <svg
    viewBox="0 0 {W} {H}"
    role="img"
    aria-label="Stacked bar chart, {unit}"
    in:fly={{ y: 12, duration: 320, easing: cubicOut }}
  >
    {#each gridLines as g}
      <line x1={PAD_L} x2={W - 6} y1={g.gy} y2={g.gy} class="grid" />
      <text x={PAD_L - 6} y={g.gy + 3.5} class="axis" text-anchor="end">
        {compact.format(g.v)}
      </text>
    {/each}
    <line x1={PAD_L} x2={W - 6} y1={base} y2={base} class="baseline" />

    {#each bars as b, i}
      {#each stack(b, i) as s}
        {#if s.top}
          <path
            d={topPath(s.bx, s.y, s.height)}
            style="fill: var(--series-{s.seg.slot + 1})"
            class:dim={hovered !== null && hovered !== i}
          />
        {:else}
          <rect
            x={s.bx}
            y={s.y}
            width={barW}
            height={s.height}
            style="fill: var(--series-{s.seg.slot + 1})"
            class:dim={hovered !== null && hovered !== i}
          />
        {/if}
      {/each}
      {#if b.tick}
        <text x={x(i) + barW / 2} y={H - 5} class="axis" text-anchor="middle">{b.tick}</text>
      {/if}
      <rect
        x={PAD_L + i * slotW}
        y={PAD_T}
        width={slotW}
        height={innerH}
        fill="transparent"
        role="presentation"
        onmouseenter={() => (hovered = i)}
        onmouseleave={() => (hovered = null)}
      />
      {#if i === peak && hovered === null}
        <text x={x(i) + barW / 2} y={base - h(total(b)) - 4} class="peak" text-anchor="middle">
          {compact.format(total(b))}
        </text>
      {/if}
    {/each}
  </svg>
  {/key}

  {#if series.length > 1}
    <div class="legend">
      {#each series as s}
        <span class="key">
          <span class="chip" style="background: var(--series-{s.slot + 1})"></span>{s.name}
        </span>
      {/each}
    </div>
  {/if}

  {#if hovered !== null && bars[hovered]}
    {@const b = bars[hovered]}
    <div class="tooltip" style="left: {((x(hovered) + barW / 2) / W) * 100}%; top: 0;">
      <div class="tt-label">{b.label}</div>
      {#each b.segments.filter((s) => s.value > 0) as s}
        <div class="tt-row">
          <span class="chip" style="background: var(--series-{s.slot + 1})"></span>
          <span>{s.name}</span>
          <span class="tt-value">{full.format(s.value)}</span>
        </div>
      {/each}
      <div class="tt-row tt-total">
        <span>total</span>
        <span class="tt-value">{full.format(total(b))}{money ? "" : ` ${unit}`}</span>
      </div>
    </div>
  {/if}
</div>

<style>
  .chart {
    position: relative;
  }
  svg {
    width: 100%;
    height: auto;
    display: block;
  }
  .dim {
    opacity: 0.45;
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }
  .baseline {
    stroke: var(--border);
    stroke-width: 1;
  }
  .axis {
    font-size: 10px;
    fill: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .peak {
    font-size: 10px;
    fill: var(--text-secondary);
  }
  .legend {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
    margin-top: 0.4rem;
    font-size: 0.75rem;
    color: var(--text-secondary);
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
  .chip {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    display: inline-block;
    flex-shrink: 0;
  }
  .tooltip {
    position: absolute;
    transform: translateX(-50%);
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font-size: 0.75rem;
    pointer-events: none;
    white-space: nowrap;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
    min-width: 150px;
  }
  .tt-label {
    color: var(--text-secondary);
    margin-bottom: 0.25rem;
  }
  .tt-row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    justify-content: space-between;
  }
  .tt-row span:nth-child(2) {
    margin-right: auto;
  }
  .tt-value {
    color: var(--text-primary);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .tt-total {
    border-top: 1px solid var(--border);
    margin-top: 0.25rem;
    padding-top: 0.25rem;
    color: var(--text-secondary);
  }
</style>
