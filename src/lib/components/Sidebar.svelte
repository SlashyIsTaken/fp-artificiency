<script lang="ts">
  export interface NavItem {
    id: string;
    label: string;
    enabled: boolean;
  }

  let {
    items,
    active,
    onselect,
  }: {
    items: NavItem[];
    active: string;
    onselect: (id: string) => void;
  } = $props();
</script>

<nav>
  <div class="brand">
    <div class="name">fp-artificiency</div>
    <div class="tag">Artificial Efficiency</div>
  </div>
  {#each items as item}
    <button
      class:active={item.id === active}
      disabled={!item.enabled}
      onclick={() => onselect(item.id)}
    >
      {item.label}
      {#if !item.enabled}<span class="soon">soon</span>{/if}
    </button>
  {/each}
</nav>

<style>
  nav {
    width: 190px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    padding: 1.25rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .brand {
    padding: 0 0.6rem 1rem;
  }
  .name {
    font-weight: 700;
    font-size: 0.95rem;
  }
  .tag {
    font-size: 0.7rem;
    color: var(--text-muted);
  }
  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
    padding: 0.45rem 0.6rem;
    border-radius: 7px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--surface-raised);
    color: var(--text-primary);
  }
  button.active {
    background: var(--surface-raised);
    color: var(--text-primary);
    font-weight: 600;
    box-shadow: inset 2px 0 0 var(--accent);
  }
  button:disabled {
    color: var(--text-muted);
    cursor: default;
  }
  .soon {
    font-size: 0.65rem;
    border: 1px solid var(--border);
    border-radius: 99px;
    padding: 0 0.4rem;
    color: var(--text-muted);
  }
</style>
