<script lang="ts">
  export interface Point {
    t: number; // epoch ms
    value: number;
  }
  export interface Event {
    t: number;
    label: string;
    kind: string; // installed | removed
  }

  let {
    points,
    events,
    unit,
    format,
  }: {
    points: Point[];
    events: Event[];
    unit: string;
    format: (n: number) => string;
  } = $props();

  const W = 720;
  const H = 230;
  const PAD_L = 50;
  const PAD_R = 12;
  const PAD_T = 16;
  const PAD_B = 52; // room for date ticks and slanted event labels

  const plotW = W - PAD_L - PAD_R;
  const plotH = H - PAD_T - PAD_B;
  const baseY = PAD_T + plotH;

  const tMin = $derived(points.length ? points[0].t : 0);
  const tMax = $derived(points.length ? points[points.length - 1].t : 1);
  const yMax = $derived(Math.max(1, ...points.map((p) => p.value)));

  const span = $derived(Math.max(1, tMax - tMin));
  const x = (t: number) => PAD_L + (plotW * (t - tMin)) / span;
  const y = (v: number) => baseY - (plotH * v) / yMax;

  const line = $derived(points.map((p) => `${x(p.t)},${y(p.value)}`).join(" "));
  const inRange = $derived(events.filter((e) => e.t >= tMin && e.t <= tMax));

  const dateLabel = (t: number) =>
    new Date(t).toLocaleDateString("en", { month: "short", day: "numeric" });

  // ~5 evenly spaced x ticks across the time span.
  const xTicks = $derived(
    Array.from({ length: 5 }, (_, i) => tMin + (span * i) / 4),
  );
  const gridY = $derived([0.5, 1].map((f) => ({ v: yMax * f, gy: y(yMax * f) })));

  let hovered = $state<number | null>(null);
  let hoveredEvent = $state<number | null>(null);

  // Collapse event markers closer than this many px into one cluster, so a
  // burst of installs/removals doesn't stack lines and labels into a smear.
  const MIN_GAP = 36;
  interface Cluster {
    x: number;
    events: Event[];
    kind: string; // shared kind of every event, or "mixed"
  }
  const clusters = $derived.by<Cluster[]>(() => {
    const sorted = [...inRange].sort((a, b) => a.t - b.t);
    const out: Cluster[] = [];
    for (const e of sorted) {
      const ex = x(e.t);
      const last = out[out.length - 1];
      // compare to the cluster's first marker so a cluster stays ≤ MIN_GAP wide
      if (last && ex - last.x <= MIN_GAP) last.events.push(e);
      else out.push({ x: ex, events: [e], kind: e.kind });
    }
    for (const c of out) {
      c.kind = c.events.every((e) => e.kind === c.events[0].kind) ? c.events[0].kind : "mixed";
    }
    return out;
  });

  const eventDateHeader = (c: Cluster) => {
    const first = dateLabel(c.events[0].t);
    const last = dateLabel(c.events[c.events.length - 1].t);
    return first === last ? first : `${first} – ${last}`;
  };
</script>

<div class="chart">
  <svg viewBox="0 0 {W} {H}" role="img" aria-label="Efficiency over time, {unit}">
    {#each gridY as g}
      <line x1={PAD_L} x2={W - PAD_R} y1={g.gy} y2={g.gy} class="grid" />
      <text x={PAD_L - 6} y={g.gy + 3.5} class="axis" text-anchor="end">{format(g.v)}</text>
    {/each}
    <line x1={PAD_L} x2={W - PAD_R} y1={baseY} y2={baseY} class="baseline" />

    {#each xTicks as t}
      <text x={x(t)} y={baseY + 14} class="axis" text-anchor="middle">{dateLabel(t)}</text>
    {/each}

    {#each clusters as c, i}
      <line
        x1={c.x}
        x2={c.x}
        y1={PAD_T}
        y2={baseY}
        class="event {c.kind}"
        class:active={hoveredEvent === i}
      />
      {#if c.events.length === 1}
        <text
          x={c.x}
          y={baseY + 26}
          class="event-label"
          text-anchor="end"
          transform="rotate(-30 {c.x} {baseY + 26})"
        >
          {c.events[0].label}
        </text>
      {:else}
        <circle
          cx={c.x}
          cy={PAD_T}
          r="7.5"
          class="cluster-badge {c.kind}"
          class:active={hoveredEvent === i}
        />
        <text x={c.x} y={PAD_T} dominant-baseline="central" text-anchor="middle" class="cluster-count">
          {c.events.length}
        </text>
      {/if}
    {/each}

    {#if points.length > 1}
      <polyline points={line} class="line" fill="none" />
    {/if}
    {#each points as p, i}
      <circle cx={x(p.t)} cy={y(p.value)} r={hovered === i ? 3.5 : 2} class="dot" />
    {/each}

    {#each points as p, i}
      <rect
        x={x(p.t) - Math.max(4, plotW / Math.max(1, points.length) / 2)}
        y={PAD_T}
        width={Math.max(8, plotW / Math.max(1, points.length))}
        height={plotH}
        fill="transparent"
        role="presentation"
        onmouseenter={() => (hovered = i)}
        onmouseleave={() => (hovered = null)}
      />
    {/each}

    {#each clusters as c, i}
      <rect
        x={c.x - 8}
        y={PAD_T}
        width="16"
        height={plotH}
        fill="transparent"
        role="presentation"
        onmouseenter={() => {
          hoveredEvent = i;
          hovered = null;
        }}
        onmouseleave={() => (hoveredEvent = null)}
      />
    {/each}
  </svg>

  {#if hovered !== null && points[hovered]}
    {@const p = points[hovered]}
    <div class="tooltip" style="left: {(x(p.t) / W) * 100}%;">
      <div class="tt-date">{dateLabel(p.t)}</div>
      <div class="tt-value">{format(p.value)} <span>{unit}</span></div>
    </div>
  {/if}

  {#if hoveredEvent !== null && clusters[hoveredEvent]}
    {@const c = clusters[hoveredEvent]}
    <div class="tooltip event-tt" style="left: {(c.x / W) * 100}%;">
      <div class="tt-date">{eventDateHeader(c)}</div>
      <ul class="tt-events">
        {#each c.events as e}
          <li class="ev-item {e.kind}">
            <span class="ev-sign">{e.kind === "removed" ? "−" : "+"}</span>{e.label}
          </li>
        {/each}
      </ul>
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
  .line {
    stroke: var(--series-1);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .dot {
    fill: var(--series-1);
  }
  .event {
    stroke-width: 1.5;
    stroke-dasharray: 3 3;
    opacity: 0.7;
  }
  .event.installed {
    stroke: var(--series-2);
  }
  .event.removed {
    stroke: var(--status-critical);
  }
  .event.mixed {
    stroke: var(--text-muted);
  }
  .event.active {
    opacity: 1;
    stroke-width: 2;
  }
  .event-label {
    font-size: 9px;
    fill: var(--text-muted);
  }
  .cluster-badge {
    stroke: var(--surface-raised);
    stroke-width: 1.5;
  }
  .cluster-badge.installed {
    fill: var(--series-2);
  }
  .cluster-badge.removed {
    fill: var(--status-critical);
  }
  .cluster-badge.mixed {
    fill: var(--text-muted);
  }
  .cluster-badge.active {
    stroke: var(--text-primary);
  }
  .cluster-count {
    font-size: 9px;
    font-weight: 700;
    fill: #fff;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }
  .event-tt .tt-events {
    list-style: none;
    margin: 0.25rem 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }
  .event-tt .ev-item {
    font-size: 0.72rem;
    font-weight: 500;
    color: var(--text-primary);
  }
  .event-tt .ev-sign {
    display: inline-block;
    width: 0.9em;
    font-weight: 700;
  }
  .event-tt .ev-item.installed .ev-sign {
    color: var(--series-2);
  }
  .event-tt .ev-item.removed .ev-sign {
    color: var(--status-critical);
  }
  .tooltip {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.35rem 0.55rem;
    font-size: 0.75rem;
    pointer-events: none;
    white-space: nowrap;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
  }
  .tt-date {
    color: var(--text-secondary);
  }
  .tt-value {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .tt-value span {
    font-weight: 400;
    color: var(--text-muted);
  }
</style>
