<script lang="ts">
  import { onMount } from "svelte";
  import { getConfigIntegrity, reviewConfig, inTauri } from "../api";
  import type { ConfigFile } from "../api";

  let files = $state<ConfigFile[]>([]);
  let status = $state<"loading" | "ready" | "browser" | "error">("loading");
  let errorMsg = $state("");

  const base = (p: string) => p.split("/").pop() ?? p;
  const sev = (s: string) =>
    s === "critical"
      ? "var(--status-critical)"
      : s === "warning"
        ? "var(--status-warning)"
        : "var(--text-muted)";

  async function load() {
    try {
      files = await getConfigIntegrity();
      status = "ready";
    } catch (e) {
      errorMsg = String(e);
      status = "error";
    }
  }

  async function markReviewed(path: string) {
    try {
      await reviewConfig(path);
      await load();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  onMount(() => {
    if (!inTauri()) {
      status = "browser";
      return;
    }
    load();
    const onVisible = () => {
      if (!document.hidden) load(); // re-check when the window is shown again
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  });

  const changed = $derived(files.filter((f) => f.status === "changed").length);
</script>

<header>
  <div>
    <h1>Config integrity</h1>
    <span class="note-inline">
      Watches your user-level Claude config for changes to sensitive keys (hooks, MCP
      servers, plugins, and more). Read-only, and it never edits your config.
    </span>
  </div>
  <button class="refresh" onclick={load}>Re-check</button>
</header>

{#if status === "loading"}
  <p class="note">Reading config…</p>
{:else if status === "browser"}
  <p class="note">
    Running in a plain browser. Start the desktop app with <code>npm run tauri dev</code>.
  </p>
{:else if status === "error"}
  <p class="note error">Guard error: {errorMsg}</p>
{:else if files.length === 0}
  <section class="panel empty">No Claude config files found under ~/.claude.</section>
{:else}
  {#if changed === 0}
    <section class="panel ok">All watched configs match their baseline.</section>
  {/if}
  {#each files as f}
    <section class="panel">
      <div class="file-head">
        <code class="path" title={f.path}>{base(f.path)}</code>
        <span class="status {f.status}">{f.status}</span>
      </div>
      {#if f.status === "watching"}
        <p class="sub">Baseline recorded. Watching for future changes.</p>
      {:else if f.status === "clean"}
        <p class="sub">Matches its baseline.</p>
      {:else}
        <ul class="findings">
          {#each f.findings as fi}
            <li>
              <span class="dot" style="background: {sev(fi.severity)}"></span>
              <span class="fkey">{fi.key}</span>
              <span class="change">{fi.change}</span>
              <span class="detail">{fi.detail}</span>
            </li>
          {/each}
        </ul>
        <button class="review" onclick={() => markReviewed(f.path)}>Mark reviewed</button>
      {/if}
    </section>
  {/each}
{/if}

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 1.25rem;
    gap: 1rem;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 1.15rem;
    font-weight: 700;
  }
  .note-inline {
    display: block;
    max-width: 46ch;
    margin-top: 0.2rem;
    color: var(--text-muted);
    font-size: 0.75rem;
  }
  .refresh {
    border: 1px solid var(--border);
    background: var(--surface-raised);
    color: var(--text-secondary);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.35rem 0.7rem;
    border-radius: 7px;
    cursor: pointer;
  }
  .refresh:hover {
    color: var(--text-primary);
    border-color: var(--text-muted);
  }
  .panel {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1rem 1.15rem;
    margin-top: 0.75rem;
  }
  .panel.empty,
  .panel.ok {
    color: var(--text-secondary);
    font-size: 0.85rem;
  }
  .panel.ok {
    border-left: 3px solid var(--series-2);
  }
  .file-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
  }
  .path {
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .status {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
    color: var(--text-muted);
  }
  .status.changed {
    color: var(--status-warning);
  }
  .sub {
    margin-top: 0.35rem;
    color: var(--text-muted);
    font-size: 0.8rem;
  }
  .findings {
    list-style: none;
    margin: 0.6rem 0 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .findings li {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 0.82rem;
    flex-wrap: wrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 99px;
    flex-shrink: 0;
    align-self: center;
  }
  .fkey {
    font-family: ui-monospace, monospace;
    font-weight: 600;
  }
  .change {
    color: var(--text-muted);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .detail {
    color: var(--text-secondary);
  }
  .review {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.65rem;
    border-radius: 7px;
    cursor: pointer;
  }
  .review:hover {
    color: var(--text-primary);
    border-color: var(--text-muted);
  }
  .note {
    color: var(--text-secondary);
  }
  .note.error {
    color: var(--status-critical);
  }
  code {
    font-size: 0.95em;
  }
</style>
