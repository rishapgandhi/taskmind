<script lang="ts">
  import { onMount, tick } from "svelte";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import {
    deleteMeeting,
    errorMessage,
    formatBytes,
    formatClock,
    formatDuration,
    getMeeting,
    onMeetingsChanged,
    renameMeeting,
    retranscribeMeeting,
    revealMeeting,
    SPEAKER_LABEL,
    type MeetingDetail,
  } from "$lib/api";
  import { transcription } from "$lib/transcription.svelte";
  import NotesPanel from "$lib/components/NotesPanel.svelte";

  const id = $derived(Number(page.params.id));
  let detail = $state<MeetingDetail | null>(null);
  let error = $state<string | null>(null);
  let title = $state("");
  let editing = $state(false);
  let audio = $state<HTMLAudioElement | null>(null);
  let currentMs = $state(0);
  let confirmDelete = $state(false);

  async function load() {
    try {
      detail = await getMeeting(id);
      title = detail.meeting.title ?? "";
      error = null;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  onMount(() => {
    transcription.init();
    load().then(async () => {
      // Search results link here with ?t=<ms> to jump to the hit.
      const t = Number(page.url.searchParams.get("t"));
      if (t > 0) {
        await tick();
        seek(t);
      }
    });
    onMeetingsChanged(load);
  });

  const active = $derived(
    detail?.segments.findIndex((s, i, all) => currentMs >= s.startMs && (i === all.length - 1 || currentMs < all[i + 1].startMs)) ?? -1,
  );
  const progress = $derived(transcription.progressFor(id));

  function seek(ms: number) {
    if (!audio) return;
    audio.currentTime = ms / 1000;
    audio.play().catch(() => {});
  }

  async function saveTitle() {
    editing = false;
    try {
      await renameMeeting(id, title);
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function remove() {
    try {
      await deleteMeeting(id);
      goto("/meetings");
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function act(fn: () => Promise<void>) {
    try {
      await fn();
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  function when(iso: string) {
    return new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  }
</script>

<a class="back" href="/meetings">← Meetings</a>
{#if error}<p class="error">{error}</p>{/if}

{#if detail}
  {@const m = detail.meeting}
  <div class="head">
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="title-input"
        bind:value={title}
        autofocus
        maxlength="200"
        onkeydown={(e) => e.key === "Enter" && saveTitle()}
        onblur={saveTitle}
      />
    {:else}
      <h1>
        <button class="title-btn" title="Rename" onclick={() => (editing = true)}>
          {m.title ?? `Meeting on ${when(m.startedAt)}`} <span class="muted pencil">✎</span>
        </button>
      </h1>
    {/if}
    <p class="muted">
      {when(m.startedAt)} · {formatDuration(m.durationS)} · {formatBytes(m.audioBytes)}
      {#if m.language}· language: {m.language}{/if}
    </p>
    <div class="actions">
      <button class="btn" onclick={() => act(() => revealMeeting(id))}>Show file</button>
      {#if m.status === "done" || m.status === "failed"}
        <button class="btn" onclick={() => act(() => retranscribeMeeting(id))}>Transcribe again</button>
      {/if}
      {#if confirmDelete}
        <span class="confirm">Delete audio and transcript?</span>
        <button class="btn danger" onclick={remove}>Delete</button>
        <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
      {:else}
        <button class="btn" onclick={() => (confirmDelete = true)}>Delete…</button>
      {/if}
    </div>
  </div>

  {#if m.audioPath && m.status !== "recording"}
    <audio
      bind:this={audio}
      controls
      src={convertFileSrc(m.audioPath)}
      ontimeupdate={() => (currentMs = (audio?.currentTime ?? 0) * 1000)}
    ></audio>
  {/if}

  {#key id}
    <NotesPanel meetingId={id} hasTranscript={m.status === "done" && detail.segments.length > 0} savedError={m.notesError} />
  {/key}

  <section class="card transcript">
    {#if m.status === "recording"}
      <p class="muted">Recording in progress.</p>
    {:else if m.status === "queued"}
      <p class="muted">
        Waiting for transcription.
        {#if transcription.status.state === "needs_model"}
          <a href="/settings">Download a speech model</a> to start.
        {:else if transcription.status.state === "paused"}
          Transcription is paused in <a href="/settings">Settings</a>.
        {/if}
      </p>
    {:else if m.status === "transcribing"}
      <p class="muted">Transcribing… {progress != null ? `${Math.round(progress * 100)}%` : ""}</p>
      <div class="bar"><span style:width={`${Math.round((progress ?? 0) * 100)}%`}></span></div>
    {:else if m.status === "failed"}
      <p class="error">Transcription failed: {m.error ?? "unknown error"}</p>
    {:else if detail.segments.length === 0}
      <p class="muted">No speech was found in this recording.</p>
    {:else}
      {#each detail.segments as s, i}
        <button class="seg" class:active={i === active} onclick={() => seek(s.startMs)}>
          <span class="time">{formatClock(s.startMs)}</span>
          <span class="who {s.speaker}">{SPEAKER_LABEL[s.speaker] ?? s.speaker}</span>
          <span class="text">{s.text}</span>
        </button>
      {/each}
    {/if}
  </section>
{/if}

<style>
  .back {
    color: var(--muted);
    text-decoration: none;
    font-size: 13px;
  }
  .head {
    margin: 8px 0 12px;
  }
  .title-btn {
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    text-align: left;
  }
  .pencil {
    font-size: 14px;
  }
  .title-input {
    font-size: 20px;
    font-weight: 700;
    width: 100%;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .confirm {
    color: var(--rec);
    font-size: 13px;
  }
  audio {
    width: 100%;
    margin: 4px 0 12px;
  }
  .transcript {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .seg {
    display: grid;
    grid-template-columns: 52px 64px 1fr;
    gap: 8px;
    text-align: left;
    border: none;
    background: none;
    padding: 6px 8px;
    border-radius: 6px;
  }
  .seg:hover {
    background: var(--bg);
  }
  .seg.active {
    background: var(--accent-soft);
  }
  .time {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    padding-top: 2px;
  }
  .who {
    font-size: 12px;
    font-weight: 600;
    padding-top: 2px;
  }
  .who.you {
    color: #30a46c;
  }
  .who.others {
    color: var(--accent);
  }
  .bar {
    height: 6px;
    background: var(--border);
    border-radius: 3px;
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .error {
    color: var(--rec);
  }
</style>
