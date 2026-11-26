<script lang="ts">
  import { page } from "$app/state";
  import { onMount } from "svelte";
  import { formatElapsed } from "$lib/api";
  import { rec } from "$lib/recorder.svelte";

  let { children } = $props();

  const nav = [
    { href: "/", label: "Today" },
    { href: "/meetings", label: "Meetings" },
    { href: "/reports", label: "Reports" },
    { href: "/timesheet", label: "Timesheet" },
    { href: "/search", label: "Search" },
    { href: "/projects", label: "Projects" },
    { href: "/settings", label: "Settings" },
  ];

  onMount(() => {
    rec.init();
  });
</script>

<div class="shell">
  <aside>
    <div class="brand">TaskMind</div>
    <nav>
      {#each nav as item}
        <a href={item.href} class:active={item.href === "/" ? page.url.pathname === "/" : page.url.pathname.startsWith(item.href)}>{item.label}</a>
      {/each}
    </nav>
  </aside>

  <div class="content">
    <header>
      <div class="status">
        <span class="dot {rec.state.status}"></span>
        {#if rec.state.status === "idle"}
          <span class="muted">Not recording</span>
        {:else}
          <span>{rec.state.status === "paused" ? "Paused" : "Recording"} · {formatElapsed(rec.displayMs)}</span>
        {/if}
      </div>
      <div class="actions">
        {#if rec.state.status === "idle"}
          <button class="btn danger" onclick={rec.start}>● Record</button>
        {:else}
          {#if rec.state.status === "recording"}
            <button class="btn" onclick={rec.pause}>Pause</button>
          {:else}
            <button class="btn" onclick={rec.resume}>Resume</button>
          {/if}
          <button class="btn" onclick={rec.stop}>Stop</button>
        {/if}
      </div>
    </header>
    {#if rec.error}
      <p class="error">{rec.error}</p>
    {/if}
    {#if rec.state.status !== "idle" && rec.state.sources}
      <p class="sources muted">
        You: {rec.state.sources.mic ?? "not recording"} · Others: {rec.state.sources.system ?? "not recording"}
        {#each rec.state.sources.warnings as w}<br /><span class="warn">{w}</span>{/each}
      </p>
    {/if}
    <main>
      {@render children()}
    </main>
  </div>
</div>

<style>
  .shell {
    display: flex;
    height: 100vh;
  }
  aside {
    width: 200px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    background: var(--surface);
    padding: 18px 12px;
  }
  .brand {
    font-weight: 700;
    font-size: 16px;
    padding: 0 10px 16px;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav a {
    padding: 7px 10px;
    border-radius: 8px;
    color: var(--text);
    text-decoration: none;
  }
  nav a:hover {
    background: var(--bg);
  }
  nav a.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 24px;
  }
  .error {
    margin: 8px 24px 0;
    color: var(--rec);
  }
  .sources {
    margin: 8px 24px 0;
    font-size: 12px;
  }
  .warn {
    color: var(--warn-text);
    background: var(--warn-bg);
    border-radius: 4px;
    padding: 0 4px;
  }
</style>
