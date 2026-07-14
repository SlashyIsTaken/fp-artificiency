<script lang="ts">
  import Sidebar from "./lib/components/Sidebar.svelte";
  import type { NavItem } from "./lib/components/Sidebar.svelte";
  import Overview from "./lib/dashboards/Overview.svelte";
  import Waste from "./lib/dashboards/Waste.svelte";

  const NAV: NavItem[] = [
    { id: "overview", label: "Overview", enabled: true },
    { id: "waste", label: "Waste diagnosis", enabled: true },
    { id: "plugins", label: "Plugin impact", enabled: false },
    { id: "integrity", label: "Config integrity", enabled: false },
  ];

  let active = $state("overview");
</script>

<div class="shell">
  <Sidebar items={NAV} {active} onselect={(id) => (active = id)} />
  <main>
    {#if active === "overview"}
      <Overview />
    {:else if active === "waste"}
      <Waste />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: flex;
    min-height: 100vh;
  }
  main {
    flex: 1;
    min-width: 0;
    padding: 1.5rem;
    /* Cap the content width so lines/tiles don't stretch absurdly on very wide
       displays, but keep it wide enough that the stat tiles can collapse to a
       single row; center the leftover space so ultrawide screens stay balanced
       rather than piling emptiness on the right. */
    max-width: 1600px;
    margin-inline: auto;
  }
</style>
