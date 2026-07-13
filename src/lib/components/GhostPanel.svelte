<script lang="ts">
  // Ghost panel: passive in-place placeholder for a panel whose data source is
  // not enabled or not built yet (DESIGN.md: inform, never nag).
  let {
    title,
    question,
    action,
  }: { title: string; question: string; action?: string } = $props();
</script>

<div class="ghost">
  <div class="title">{title}</div>
  <p class="question">{question}</p>
  {#if action}<span class="action">{action}</span>{/if}
</div>

<style>
  .ghost {
    border: 1.5px dashed var(--border);
    border-radius: 10px;
    padding: 1rem 1.15rem;
    color: var(--text-muted);
  }
  .title {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .question {
    font-size: 0.85rem;
    margin-top: 0.25rem;
  }
  .action {
    display: inline-block;
    margin-top: 0.6rem;
    font-size: 0.78rem;
    color: var(--accent);
  }
</style>
