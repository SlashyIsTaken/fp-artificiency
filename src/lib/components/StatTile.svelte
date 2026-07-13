<script lang="ts">
  let { label, value, hint }: { label: string; value: string; hint?: string } = $props();
</script>

<div class="tile">
  <div class="label">{label}</div>
  <div class="value">{value}</div>
  {#if hint}<div class="hint">{hint}</div>{/if}
</div>

<style>
  .tile {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1rem 1.15rem;
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
