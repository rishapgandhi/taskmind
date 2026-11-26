<script lang="ts">
  import { onMount } from "svelte";
  import {
    errorMessage,
    generateNotes,
    getNotes,
    onNotesChanged,
    saveNotes,
    type Notes,
    type StoredNotes,
  } from "$lib/api";

  let {
    meetingId,
    hasTranscript,
    savedError = null,
  }: { meetingId: number; hasTranscript: boolean; savedError?: string | null } = $props();

  let stored = $state<StoredNotes | null>(null);
  let draft = $state<Notes | null>(null);
  let working = $state(false);
  let error = $state<string | null>(null);
  let editing = $state(false);

  async function load() {
    try {
      stored = await getNotes(meetingId);
    } catch (err) {
      error = errorMessage(err);
    }
  }

  let section = $state<HTMLElement | null>(null);

  onMount(() => {
    load().then(() => {
      if (location.hash === "#notes") section?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
    onNotesChanged((e) => {
      if (e.meetingId !== meetingId) return;
      working = e.state === "working";
      if (e.state === "failed") error = e.error ?? "failed";
      if (e.state === "done") {
        error = null;
        load();
      }
    });
  });

  async function generate() {
    working = true;
    error = null;
    try {
      await generateNotes(meetingId);
      await load();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      working = false;
    }
  }

  function startEdit() {
    if (!stored) return;
    draft = structuredClone($state.snapshot(stored.notes));
    editing = true;
  }

  async function save(notes: Notes) {
    try {
      await saveNotes(meetingId, notes);
      editing = false;
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function toggleDone(i: number) {
    if (!stored) return;
    const notes = structuredClone($state.snapshot(stored.notes));
    notes.actionItems[i].done = !notes.actionItems[i].done;
    await save(notes);
  }
</script>

<section class="card notes" id="notes" bind:this={section}>
  <div class="head">
    <h2>Notes</h2>
    <span class="spacer"></span>
    {#if stored && !editing}
      <span class="muted small">
        {stored.edited ? "Edited" : `By ${stored.provider} · ${stored.model}`}
      </span>
      <button class="btn" onclick={startEdit}>Edit</button>
    {/if}
    {#if hasTranscript && !editing}
      <button class="btn" onclick={generate} disabled={working}>
        {working ? "Writing notes…" : stored ? "Regenerate" : "Write notes with AI"}
      </button>
    {/if}
  </div>
  {#if error ?? (stored ? null : savedError)}
    <div class="fail">
      <strong>Notes could not be written.</strong>
      {error ?? savedError}
      <div class="fix">
        <a class="btn" href="/settings#notes">Open AI notes settings</a>
        {#if hasTranscript}<button class="btn" onclick={generate} disabled={working}>Try again</button>{/if}
      </div>
    </div>
  {/if}

  {#if editing && draft}
    <label class="col"><span class="muted small">Summary</span><textarea rows="5" bind:value={draft.summary}></textarea></label>
    <label class="col">
      <span class="muted small">Decisions (one per line)</span>
      <textarea
        rows="3"
        value={draft.decisions.join("\n")}
        oninput={(e) => draft && (draft.decisions = e.currentTarget.value.split("\n").filter((l) => l.trim()))}
      ></textarea>
    </label>
    <span class="muted small">Action items</span>
    {#each draft.actionItems as item, i}
      <div class="row">
        <input class="grow" bind:value={item.text} placeholder="Task" />
        <input bind:value={item.owner} placeholder="Owner" />
        <input bind:value={item.due} placeholder="Due" />
        <button class="x" aria-label="Remove" onclick={() => draft && (draft.actionItems = draft.actionItems.filter((_, j) => j !== i))}>×</button>
      </div>
    {/each}
    <button
      class="btn"
      onclick={() => draft && (draft.actionItems = [...draft.actionItems, { text: "", owner: null, due: null, done: false }])}
    >+ Add action item</button>
    <div class="actions">
      <button class="btn" onclick={() => (editing = false)}>Cancel</button>
      <button class="btn primary" onclick={() => draft && save({ ...draft, actionItems: draft.actionItems.filter((a) => a.text.trim()) })}>Save</button>
    </div>
  {:else if stored}
    <div class="summary">{stored.notes.summary}</div>
    {#if stored.notes.decisions.length}
      <h3>Decisions</h3>
      <ul>{#each stored.notes.decisions as d}<li>{d}</li>{/each}</ul>
    {/if}
    {#if stored.notes.actionItems.length}
      <h3>Action items</h3>
      <ul class="items">
        {#each stored.notes.actionItems as a, i}
          <li>
            <label class:done={a.done}>
              <input type="checkbox" checked={a.done} onchange={() => toggleDone(i)} />
              {a.text}
              {#if a.owner}<span class="tag">{a.owner}</span>{/if}
              {#if a.due}<span class="muted small">· {a.due}</span>{/if}
            </label>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if !hasTranscript}
    <p class="muted">Notes can be written once the transcript below is ready.</p>
  {:else if !(error ?? savedError)}
    <p class="muted">
      No notes yet. Press <strong>Write notes with AI</strong> above. They use the provider chosen in
      <a href="/settings#notes">Settings → AI meeting notes</a> (a local model keeps everything on this computer).
    </p>
  {/if}
</section>

<style>
  .notes {
    margin-bottom: 12px;
  }
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  h2 {
    font-size: 15px;
    margin: 0;
  }
  h3 {
    font-size: 13px;
    margin: 12px 0 4px;
  }
  .spacer {
    flex: 1;
  }
  .summary {
    white-space: pre-wrap;
    margin-top: 8px;
  }
  ul {
    margin: 0;
    padding-left: 20px;
  }
  .items {
    list-style: none;
    padding: 0;
  }
  .items label {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .done {
    text-decoration: line-through;
    color: var(--muted);
  }
  .tag {
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 999px;
    padding: 0 8px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 8px 0;
  }
  .row {
    display: flex;
    gap: 6px;
    margin: 4px 0;
  }
  .grow {
    flex: 1;
  }
  textarea,
  input {
    font: inherit;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
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
    margin-top: 8px;
  }
  .small {
    font-size: 12px;
  }
  .fail {
    background: var(--warn-bg);
    color: var(--warn-text);
    border-radius: 8px;
    padding: 10px 12px;
    margin-top: 8px;
  }
  .fix {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .fix a {
    text-decoration: none;
    color: var(--text);
  }
  button:disabled {
    opacity: 0.6;
  }
</style>
