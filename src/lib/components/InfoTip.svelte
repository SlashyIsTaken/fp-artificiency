<script lang="ts">
  // Term with an explanatory tooltip for non-expert users (hover; CSS-only).
  let { text, tip }: { text: string; tip: string } = $props();
</script>

<span class="term" aria-label={tip}>
  {text}<span class="tip" role="tooltip">{tip}</span>
</span>

<style>
  .term {
    position: relative;
    border-bottom: 1px dotted var(--text-muted);
    cursor: help;
  }
  .tip {
    display: none;
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 10;
    width: 210px;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.45rem 0.6rem;
    font-size: 0.75rem;
    line-height: 1.4;
    color: var(--text-secondary);
    box-shadow: 0 3px 12px rgba(0, 0, 0, 0.15);
    white-space: normal;
  }
  .term:hover .tip {
    display: block;
  }
</style>
