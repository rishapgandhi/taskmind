<script lang="ts">
  import { onMount } from "svelte";
  import {
    deleteProject,
    errorMessage,
    listProjects,
    projectColor,
    saveProject,
    type Project,
    type Rule,
    type RuleKind,
  } from "$lib/api";

  const KINDS: [RuleKind, string, string][] = [
    ["window_title", "Window title contains", "e.g. permit-api, PERMIT-"],
    ["url", "Web address contains", "e.g. jira.company.com/ESG"],
    ["app_name", "App name contains", "e.g. Figma, Slack"],
    ["directory", "Folder name in title", "e.g. esg-vault"],
    ["git_branch", "Git branch in title", "e.g. feature/ESG-"],
  ];

  type Draft = { id: number | null; name: string; color: string; rules: Rule[] };

  let projects = $state<Project[]>([]);
  let editing = $state<Draft | null>(null);
  let error = $state<string | null>(null);
  let confirmDelete = $state<number | null>(null);

  async function load() {
    try {
      projects = await listProjects();
    } catch (err) {
      error = errorMessage(err);
    }
  }
  onMount(load);

  function edit(p?: Project) {
    error = null;
    editing = p
      ? { id: p.id, name: p.name, color: projectColor(p), rules: p.rules.map((r) => ({ ...r })) }
      : { id: null, name: "", color: "#4f46e5", rules: [{ kind: "window_title", pattern: "", priority: 0 }] };
  }

  async function save() {
    if (!editing) return;
    try {
      await saveProject({ ...editing, rules: editing.rules.filter((r) => r.pattern.trim()) });
      editing = null;
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function remove(id: number) {
    try {
      await deleteProject(id);
      confirmDelete = null;
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  const kindLabel = (k: string) => KINDS.find((x) => x[0] === k)?.[1] ?? k;
</script>

<h1>Projects</h1>
<p class="muted">
  Projects group your tracked time. A rule assigns activity to a project when it matches (not case-sensitive). Changes also
  apply to the last 30 days.
</p>
{#if error}<p class="error">{error}</p>{/if}

{#if editing}
  <section class="card editor">
    <div class="row">
      <input class="name" placeholder="Project name, e.g. Permit Vault" bind:value={editing.name} maxlength="80" />
      <input type="color" bind:value={editing.color} aria-label="Color" />
    </div>
    <h3>Rules</h3>
    {#each editing.rules as r, i}
      <div class="row">
        <select bind:value={r.kind}>
          {#each KINDS as [k, label]}<option value={k}>{label}</option>{/each}
        </select>
        <input class="pattern" bind:value={r.pattern} placeholder={KINDS.find((x) => x[0] === r.kind)?.[2]} maxlength="200" />
        <button class="x" aria-label="Remove rule" onclick={() => editing && (editing.rules = editing.rules.filter((_, j) => j !== i))}>×</button>
      </div>
    {/each}
    <button class="btn" onclick={() => editing && (editing.rules = [...editing.rules, { kind: "window_title", pattern: "", priority: 0 }])}>
      + Add rule
    </button>
    <div class="actions">
      <button class="btn" onclick={() => (editing = null)}>Cancel</button>
      <button class="btn primary" onclick={save} disabled={!editing.name.trim()}>Save project</button>
    </div>
  </section>
{:else}
  <button class="btn primary" onclick={() => edit()}>+ New project</button>
{/if}

<ul class="list">
  {#each projects as p}
    <li class="card">
      <div class="head">
        <span class="dot-c" style:background={projectColor(p)}></span>
        <strong>{p.name}</strong>
        <span class="spacer"></span>
        {#if confirmDelete === p.id}
          <span class="small warn">Delete? Past activity becomes Unassigned.</span>
          <button class="btn danger" onclick={() => remove(p.id)}>Delete</button>
          <button class="btn" onclick={() => (confirmDelete = null)}>Cancel</button>
        {:else}
          <button class="btn" onclick={() => edit(p)}>Edit</button>
          <button class="btn" onclick={() => (confirmDelete = p.id)}>Delete…</button>
        {/if}
      </div>
      {#if p.rules.length}
        <ul class="rules muted">
          {#each p.rules as r}<li>{kindLabel(r.kind)} <code>{r.pattern}</code></li>{/each}
        </ul>
      {:else}
        <p class="muted small">No rules yet, so nothing is assigned automatically.</p>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .editor {
    margin-bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    font-size: 13px;
    margin: 8px 0 0;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  input,
  select {
    font: inherit;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  input[type="color"] {
    padding: 0;
    width: 40px;
    height: 34px;
  }
  .name {
    flex: 1;
    font-weight: 600;
  }
  .pattern {
    flex: 1;
  }
  .x {
    border: none;
    background: none;
    color: var(--muted);
    font-size: 18px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 16px 0 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .dot-c {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .rules {
    margin: 6px 0 0;
    padding-left: 18px;
    font-size: 13px;
  }
  code {
    background: var(--bg);
    border-radius: 4px;
    padding: 0 4px;
  }
  .small {
    font-size: 12px;
  }
  .warn {
    color: var(--rec);
  }
  .error {
    color: var(--rec);
  }
</style>
