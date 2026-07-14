<script lang="ts">
  // Subscription usage meters (5h session / weekly limits). Renders nothing
  // for accounts without limits — the backend returns null and we stay quiet.
  import { onMount } from "svelte";
  import { getUsageLimits, inTauri } from "../api";
  import type { UsageLimit } from "../api";

  let limits = $state<UsageLimit[] | null>(null);

  const fill = (l: UsageLimit) =>
    l.severity === "critical" || l.percent >= 90
      ? "var(--status-critical)"
      : l.severity === "warning"
        ? "var(--status-warning)"
        : "var(--accent)";

  const resetText = (l: UsageLimit) => {
    if (!l.resets_at) return "";
    const d = new Date(l.resets_at);
    const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    const sameDay = d.toDateString() === new Date().toDateString();
    return sameDay
      ? `resets ${hm}`
      : `resets ${d.toLocaleDateString("en", { weekday: "short" })} ${hm}`;
  };

  async function refresh() {
    try {
      const next = await getUsageLimits();
      // Keep the last-good values on a transient miss (rate limit / offline) —
      // the backend already falls back to its cache, and retaining here means
      // the widget never flickers away. It only appears once we've seen data.
      if (next && next.length > 0) limits = next;
    } catch {
      /* keep whatever we last had */
    }
  }

  onMount(() => {
    if (!inTauri()) return;
    refresh();
    // Usage percentages move slowly and the endpoint is rate-limited (shared
    // with Claude Code), so poll gently — every 5 minutes.
    const t = setInterval(refresh, 300_000);
    return () => clearInterval(t);
  });
</script>

{#if limits && limits.length > 0}
  <div class="meters">
    <div class="heading">Current usage</div>
    {#each limits as l}
      <div class="meter">
        <div class="row">
          <span class="label">{l.label}</span>
          <span class="pct" style="color: {fill(l)}">{l.percent}%</span>
        </div>
        <div class="track" style="background: color-mix(in srgb, {fill(l)} 22%, transparent)">
          <div class="fill" style="width: {Math.min(100, l.percent)}%; background: {fill(l)}"></div>
        </div>
        <div class="reset">{resetText(l)}</div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .meters {
    margin-top: auto;
    padding: 0.9rem 0.6rem 0;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .heading {
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 0.25rem;
  }
  .label {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }
  .pct {
    font-size: 0.75rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .track {
    height: 5px;
    border-radius: 99px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 99px;
  }
  .reset {
    font-size: 0.68rem;
    color: var(--text-muted);
    margin-top: 0.2rem;
  }
</style>
