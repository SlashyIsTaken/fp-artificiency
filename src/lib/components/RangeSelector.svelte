<script lang="ts">
  export interface RangePreset {
    label: string;
    hours: number; // 0 = all time
    bucket: "minute" | "hour" | "day";
  }

  let {
    presets,
    selected,
    onselect,
  }: {
    presets: RangePreset[];
    selected: RangePreset;
    onselect: (p: RangePreset) => void;
  } = $props();
</script>

<div class="range" role="group" aria-label="Date range">
  {#each presets as p}
    <button
      class:active={p.label === selected.label}
      onclick={() => onselect(p)}
    >
      {p.label}
    </button>
  {/each}
</div>

<style>
  .range {
    display: inline-flex;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 2px;
    gap: 2px;
  }
  button {
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.65rem;
    border-radius: 6px;
    cursor: pointer;
  }
  button:hover {
    color: var(--text-primary);
  }
  button.active {
    background: var(--accent);
    color: #ffffff;
  }
</style>
