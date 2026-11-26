<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import {
    errorMessage,
    formatBytes,
    formatDuration,
    generateNotes,
    isoDate,
    listMeetings,
    onMeetingsChanged,
    onNotesChanged,
    revealMeeting,
    type Meeting,
  } from "$lib/api";
  import DateNav from "$lib/components/DateNav.svelte";
  import { transcription } from "$lib/transcription.svelte";

  type View = "day" | "all";
  let view = $state<View>("day");
  let date = $state(isoDate(new Date()));
  let meetings = $state<Meeting[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let playing = $state<number | null>(null);
  let writing = $state<Set<number>>(new Set());

  const statusLabel: Record<Meeting["status"], string> = {
    recording: "Recording",
    finalizing: "Saving",
    queued: "Waiting for transcript",
    transcribing: "Transcribing",
    done: "Transcript ready",
    failed: "Transcript failed",
  };

  async function load() {
    try {
      meetings = await listMeetings(view === "day" ? { date } : {});
      error = null;
    } catch (err) {
      error = errorMessage(err);
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    view;
    date;
    load();
  });

  onMount(() => {
    transcription.init();
    onMeetingsChanged(load);
    onNotesChanged((e) => {
      const next = new Set(writing);
      if (e.state === "working") next.add(e.meetingId);
      else next.delete(e.meetingId);
      writing = next;
    });
  });

  // "All" view: group by local day, newest day first.
  const groups = $derived.by(() => {
    const map = new Map<string, Meeting[]>();
    for (const m of meetings) {
      const key = isoDate(new Date(m.startedAt));
      map.set(key, [...(map.get(key) ?? []), m]);
    }
    return [...map.entries()];
  });

  const dayTitle = (key: string) =>
    new Date(`${key}T12:00:00`).toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  const time = (iso: string | null) =>
    iso ? new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" }) : "…";

  async function writeNotes(id: number) {
    writing = new Set(writing).add(id);
    try {
      await generateNotes(id);
    } catch {
      // The reason is saved on the meeting and shown in the row.
    } finally {
      const next = new Set(writing);
      next.delete(id);
      writing = next;
      await load();
    }
  }

  async function reveal(id: number) {
    try {
      await revealMeeting(id);
    } catch (err) {
      error = errorMessage(err);
    }
  }
</script>

<h1>Meetings</h1>

<div class="toolbar">
  <div class="seg">
    <button class:on={view === "day"} onclick={() => (view = "day")}>By day</button>
    <button class:on={view === "all"} onclick={() => (view = "all")}>All meetings</button>
  </div>
  {#if view === "day"}
    <input type="date" bind:value={date} max={isoDate(new Date())} aria-label="Pick a day" />
  {/if}
</div>
{#if view === "day"}<DateNav bind:date />{/if}

{#if transcription.status.state === "needs_model"}
  <p class="notice">Transcripts need a speech model. <a href="/settings">Download one in Settings</a> (about 150 MB, once).</p>
{:else if transcription.status.state === "paused"}
  <p class="notice">Transcription is paused. <a href="/settings">Resume in Settings</a>.</p>
{/if}
{#if error}<p class="error">{error}</p>{/if}

{#if loaded && meetings.length === 0}
  <div class="card">
    <p><strong>{view === "day" ? "No meetings on this day." : "No recordings yet."}</strong></p>
    <p class="muted">Press ● Record (or the shortcut shown in Settings) when a meeting starts.</p>
  </div>
{/if}

{#snippet row(m: Meeting)}
  <li class="card">
    <div class="row">
      <div class="main">
        <a class="title" href={`/meetings/${m.id}`}>{m.title ?? `Meeting at ${time(m.startedAt)}`}</a>
        <div class="meta muted">
          {time(m.startedAt)}–{time(m.endedAt)} · {formatDuration(m.durationS)} · {formatBytes(m.audioBytes)}
        </div>
      </div>
      <span class="badge {m.status}">
        {statusLabel[m.status]}{#if transcription.progressFor(m.id) != null}
          {Math.round((transcription.progressFor(m.id) ?? 0) * 100)}%{/if}
      </span>
      {#if m.audioPath && m.status !== "recording"}
        <button class="btn" onclick={() => (playing = playing === m.id ? null : m.id)}>
          {playing === m.id ? "Close" : "Play"}
        </button>
        <button class="btn" onclick={() => reveal(m.id)}>Show file</button>
      {/if}
    </div>

    <div class="notes-line">
      {#if writing.has(m.id)}
        <span class="muted">📝 Writing notes…</span>
      {:else if m.hasNotes}
        <a class="notes-link" href={`/meetings/${m.id}#notes`}>📝 Open notes</a>
        <button class="link" onclick={() => writeNotes(m.id)}>Regenerate</button>
      {:else if m.status === "done"}
        {#if m.notesError}
          <span class="warn">Notes failed: {m.notesError}</span>
          <button class="btn" onclick={() => writeNotes(m.id)}>Try again</button>
          <a href="/settings#notes">Settings</a>
        {:else}
          <span class="muted">No notes yet.</span>
          <button class="btn primary" onclick={() => writeNotes(m.id)}>Write notes</button>
        {/if}
      {:else}
        <span class="muted small">Notes can be written once the transcript is ready.</span>
      {/if}
    </div>

    {#if playing === m.id && m.audioPath}
      <!-- svelte-ignore a11y_media_has_caption -->
      <audio controls autoplay src={convertFileSrc(m.audioPath)}></audio>
    {/if}
  </li>
{/snippet}

{#if view === "day"}
  <ul class="list">{#each meetings as m (m.id)}{@render row(m)}{/each}</ul>
{:else}
  {#each groups as [day, items]}
    <h2>{dayTitle(day)} <span class="muted count">{items.length} meeting{items.length === 1 ? "" : "s"}</span></h2>
    <ul class="list">{#each items as m (m.id)}{@render row(m)}{/each}</ul>
  {/each}
{/if}

<style>
  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin: 8px 0 4px;
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .seg button {
    border: none;
    background: var(--surface);
    padding: 6px 12px;
  }
  .seg button.on {
    background: var(--accent);
    color: #fff;
  }
  input[type="date"] {
    font: inherit;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
  }
  h2 {
    font-size: 15px;
    margin: 20px 0 8px;
  }
  .count {
    font-weight: 400;
    font-size: 13px;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    color: var(--text);
    text-decoration: none;
  }
  .title:hover {
    color: var(--accent);
  }
  .meta {
    font-size: 13px;
  }
  .badge {
    font-size: 12px;
    border-radius: 999px;
    padding: 2px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    white-space: nowrap;
  }
  .badge.recording {
    color: var(--rec);
  }
  .badge.failed {
    color: #fff;
    background: var(--rec);
    border-color: var(--rec);
  }
  .notes-line {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
    font-size: 13px;
  }
  .notes-link {
    font-weight: 600;
    color: var(--accent);
    text-decoration: none;
  }
  .link {
    border: none;
    background: none;
    color: var(--muted);
    text-decoration: underline;
    padding: 0;
    font-size: 13px;
  }
  .warn {
    color: var(--rec);
  }
  .small {
    font-size: 12px;
  }
  .notice {
    background: var(--warn-bg);
    color: var(--warn-text);
    border-radius: 8px;
    padding: 8px 12px;
  }
  audio {
    width: 100%;
    margin-top: 12px;
  }
  .error {
    color: var(--rec);
  }
  .card p {
    margin: 4px 0;
  }
</style>
