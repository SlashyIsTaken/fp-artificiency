<script lang="ts">
  export interface Bar {
    label: string; // full label for the tooltip (e.g. "2026-07-13")
    tick?: string; // short axis tick; only some bars carry one
    value: number;
  }

  let { bars, unit }: { bars: Bar[]; unit: string } = $props();

  const W = 720;
  const H = 180;
  const PAD_L = 44;
  const PAD_B = 20;
  const PAD_T = 8;

  const compact = new Intl.NumberFormat("en", {
    notation: "compact",
    maximumFractionDigits: 1,
  });
  const full = new Intl.NumberFormat("en");

  let hovered = $state<number | null>(null);

  const max = $derived(Math.max(1, ...bars.map((b) => b.value)));
  const peak = $derived(bars.findIndex((b) => b.value === max && max > 0));
  const innerW = W - PAD_L - 6;
  const innerH = H - PAD_T - PAD_B;
  const slot = $derived(innerW / Math.max(1, bars.length));
  const barW = $derived(Math.max(2, slot - 2)); // 2px surface gap between bars
  const x = (i: number) => PAD_L + i * slot + (slot - barW) / 2;
  const y = (v: number) => PAD_T + innerH * (1 - v / max);
  const h = (v: number) => (innerH * v) / max;

  // Bar with rounded top corners, square base anchored to the baseline.
  function barPath(i: number, v: number): string {
    const bx = x(i);
    const by = y(v);
    const bh = h(v);
    const r = Math.min(2, bh, barW / 2);
    const base = PAD_T + innerH;
    return `M${bx},${base} L${bx},${by + r} Q${bx},${by} ${bx + r},${by}
            L${bx + barW - r},${by} Q${bx + barW},${by} ${bx + barW},${by + r}
            L${bx + barW},${base} Z`;
  }

  const gridLines = $derived([0.5, 1].map((f) => ({ v: max * f, gy: y(max * f) })));
</script>

<div class="chart">
  <svg viewBox="0 0 {W} {H}" role="img" aria-label="Bar chart, {unit} per day">
    {#each gridLines as g}
      <line x1={PAD_L} x2={W - 6} y1={g.gy} y2={g.gy} class="grid" />
      <text x={PAD_L - 6} y={g.gy + 3.5} class="axis" text-anchor="end">
        {compact.format(g.v)}
      </text>
    {/each}
    <line x1={PAD_L} x2={W - 6} y1={PAD_T + innerH} y2={PAD_T + innerH} class="baseline" />

    {#each bars as b, i}
      {#if b.value > 0}
        <path d={barPath(i, b.value)} class="bar" class:dim={hovered !== null && hovered !== i} />
      {/if}
      {#if b.tick}
        <text x={x(i) + barW / 2} y={H - 5} class="axis" text-anchor="middle">{b.tick}</text>
      {/if}
      <!-- full-height hit target, wider than the mark -->
      <rect
        x={PAD_L + i * slot}
        y={PAD_T}
        width={slot}
        height={innerH}
        fill="transparent"
        role="presentation"
        onmouseenter={() => (hovered = i)}
        onmouseleave={() => (hovered = null)}
      />
      {#if i === peak && hovered === null}
        <text x={x(i) + barW / 2} y={y(b.value) - 4} class="peak" text-anchor="middle">
          {compact.format(b.value)}
        </text>
      {/if}
    {/each}
  </svg>

  {#if hovered !== null && bars[hovered]}
    <div
      class="tooltip"
      style="left: {((x(hovered) + barW / 2) / W) * 100}%; top: 0;"
    >
      <span class="tt-label">{bars[hovered].label}</span>
      <span class="tt-value">{full.format(bars[hovered].value)} {unit}</span>
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
  .bar {
    fill: var(--accent);
  }
  .bar.dim {
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
  .tooltip {
    position: absolute;
    transform: translateX(-50%);
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.3rem 0.55rem;
    font-size: 0.75rem;
    pointer-events: none;
    white-space: nowrap;
    display: flex;
    gap: 0.5rem;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
  }
  .tt-label {
    color: var(--text-secondary);
  }
  .tt-value {
    color: var(--text-primary);
    font-weight: 600;
  }
</style>
