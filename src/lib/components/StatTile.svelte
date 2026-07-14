<script lang="ts">
  import { untrack } from "svelte";
  import { Tween, prefersReducedMotion } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import InfoTip from "./InfoTip.svelte";

  let {
    label,
    value,
    num,
    format,
    hint,
    tip,
    onselect,
    active = false,
  }: {
    label: string;
    value?: string; // static display; use when the value isn't a live number
    num?: number; // animated numeric value; pair with `format`
    format?: (n: number) => string;
    hint?: string;
    tip?: string;
    // When set, the tile becomes a chart-metric selector: clicking it drives
    // the trend chart, and `active` marks the one currently shown.
    onselect?: () => void;
    active?: boolean;
  } = $props();

  // Odometer: roll up to a growing value; snap on shrink (range change) or
  // when the user prefers reduced motion. Seed from the initial value (untrack:
  // capturing the initial is intended; the $effect below tracks later changes).
  const rolled = new Tween(untrack(() => num) ?? 0, { easing: cubicOut });
  let prev = untrack(() => num) ?? 0;
  $effect(() => {
    const v = num ?? 0;
    const grew = v > prev;
    prev = v;
    rolled.set(v, { duration: grew && !prefersReducedMotion.current ? 600 : 0 });
  });

  const shown = $derived(
    num !== undefined && format ? format(rolled.current) : (value ?? ""),
  );
</script>

<!-- A real <button> when selectable (native focus + Enter/Space), a plain
     <div> otherwise. -->
<svelte:element
  this={onselect ? "button" : "div"}
  class="tile"
  class:selectable={onselect}
  class:active
  type={onselect ? "button" : undefined}
  role={onselect ? "button" : undefined}
  aria-pressed={onselect ? active : undefined}
  onclick={onselect}
>
  <div class="label">
    {#if tip}<InfoTip text={label} {tip} />{:else}{label}{/if}
  </div>
  <div class="value">{shown}</div>
  {#if hint}<div class="hint">{hint}</div>{/if}
</svelte:element>

<style>
  .tile {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1rem 1.15rem;
    /* reset <button>'s UA styles so the selectable variant matches the div */
    display: block;
    width: 100%;
    text-align: left;
    font: inherit;
    color: inherit;
    /* animate the selector affordances */
    transition: border-color 0.12s, box-shadow 0.12s, transform 0.06s;
  }
  .selectable {
    cursor: pointer;
  }
  .selectable:hover {
    border-color: var(--series-1);
  }
  .selectable:active {
    transform: translateY(1px);
  }
  .tile.active {
    border-color: var(--series-1);
    box-shadow: inset 0 0 0 1px var(--series-1);
  }
  .selectable:focus-visible {
    outline: 2px solid var(--series-1);
    outline-offset: 2px;
  }
  .label {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
  .value {
    /* Stat-tile value: semibold, proportional figures (not tabular) at display size */
    font-size: 1.7rem;
    font-weight: 600;
    margin-top: 0.15rem;
  }
  .hint {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin-top: 0.15rem;
  }
</style>
