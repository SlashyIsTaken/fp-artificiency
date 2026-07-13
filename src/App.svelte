<script lang="ts">
  import { onMount } from "svelte";
  import { getOverview, runBackfill, inTauri } from "./lib/api";
  import type { Overview, IngestReport } from "./lib/api";
  import StatTile from "./lib/components/StatTile.svelte";
  import GhostPanel from "./lib/components/GhostPanel.svelte";

  let overview = $state<Overview | null>(null);
  let ingest = $state<IngestReport | null>(null);
  let status = $state<"loading" | "ready" | "error" | "browser">("loading");
  let errorMsg = $state("");

  const compact = new Intl.NumberFormat("en", {
    notation: "compact",
    maximumFractionDigits: 1,
  });
  const fmt = (n: number) => compact.format(n);

  const day = (ts: string | null) => (ts ? ts.slice(0, 10) : "—");

  onMount(async () => {
    if (!inTauri()) {
      status = "browser";
      return;
    }
    try {
      ingest = await runBackfill();
      overview = await getOverview();
      status = "ready";
    } catch (e) {
      errorMsg = String(e);
      status = "error";
    }
  });
</script>

<main>
  <header>
    <div>
      <h1>fp-artificiency</h1>
      <p class="tagline">Artificial Efficiency — where your tokens actually went</p>
    </div>
    {#if overview?.first_ts}
      <span class="range">{day(overview.first_ts)} → {day(overview.last_ts)}</span>
    {/if}
  </header>

  {#if status === "loading"}
    <p class="note">Reading Claude Code transcripts…</p>
  {:else if status === "browser"}
    <p class="note">
      Running in a plain browser — start the desktop app with <code>npm run tauri dev</code>
      to connect to the collector backend.
    </p>
  {:else if status === "error"}
    <p class="note error">Collector error: {errorMsg}</p>
  {:else if overview}
    <section class="tiles">
      <StatTile label="Sessions" value={fmt(overview.sessions)} />
      <StatTile label="Turns" value={fmt(overview.turns)} />
      <StatTile label="Input tokens" value={fmt(overview.tokens_in)} hint="uncached" />
      <StatTile label="Output tokens" value={fmt(overview.tokens_out)} />
      <StatTile label="Cache reads" value={fmt(overview.cache_read)} hint="tokens served from cache" />
      <StatTile label="Cache writes" value={fmt(overview.cache_write)} hint="tokens written to cache" />
    </section>

    <section class="ghosts">
      <GhostPanel
        title="Plugin impact"
        question="Which plugin or config change actually saved you tokens?"
        action="Requires enrichment — coming soon"
      />
      <GhostPanel
        title="Waste diagnosis"
        question="Duplicate file reads, oversized tool outputs, cache churn — where is the leak?"
        action="Coming soon"
      />
      <GhostPanel
        title="Config integrity"
        question="Have your Claude Code hooks been tampered with?"
        action="Coming soon"
      />
    </section>

    {#if ingest && overview.db_path}
      <footer>
        Ingested {ingest.events_added} new turns from {ingest.files_seen} transcripts ·
        store: <code>{overview.db_path}</code>
      </footer>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: 1080px;
    margin: 0 auto;
    padding: 2rem 1.5rem;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 1.5rem;
  }
  h1 {
    font-size: 1.3rem;
    font-weight: 700;
  }
  .tagline {
    color: var(--text-secondary);
    font-size: 0.85rem;
  }
  .range {
    color: var(--text-muted);
    font-size: 0.8rem;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 0.75rem;
  }
  .ghosts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 0.75rem;
    margin-top: 0.75rem;
  }
  .note {
    color: var(--text-secondary);
  }
  .note.error {
    color: #d03b3b;
  }
  footer {
    margin-top: 1.5rem;
    font-size: 0.78rem;
    color: var(--text-muted);
  }
  code {
    font-size: 0.95em;
  }
</style>
