<script lang="ts">
  import { onMount } from "svelte";
  import {
    dayActionItems,
    dayReport,
    saveNotes,
    getNotes,
    deleteDayActivity,
    errorMessage,
    formatHours,
    isoDate,
    listProjects,
    onMeetingsChanged,
    onTrackingStatus,
    projectColor,
    requestTrackingPermission,
    trackingStatus,
    type DayActionItem,
    type DayReport,
    type Project,
    type TrackerStatus,
  } from "$lib/api";
  import DateNav from "$lib/components/DateNav.svelte";
  import Welcome from "$lib/components/Welcome.svelte";

  let date = $state(isoDate(new Date()));
  let report = $state<DayReport | null>(null);
  let actions = $state<DayActionItem[]>([]);
  let projects = $state<Project[]>([]);
  let status = $state<TrackerStatus | null>(null);
  let error = $state<string | null>(null);
  let confirmDelete = $state(false);

  const byId = $derived(new Map(projects.map((p) => [p.id, p])));
  const name = (id: number | null) => (id == null ? "Unassigned" : (byId.get(id)?.name ?? "Unassigned"));
  const isToday = $derived(date === isoDate(new Date()));

  async function load() {
    try {
      [report, projects, actions] = await Promise.all([dayReport(date), listProjects(), dayActionItems(date)]);
      error = null;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  $effect(() => {
    date;
    confirmDelete = false;
    load();
  });

  onMount(() => {
    trackingStatus().then((s) => (status = s)).catch(() => {});
    onTrackingStatus((s) => {
      status = s;
    });
    onMeetingsChanged(load);
    // Refresh today's numbers every minute while the page is open.
    const timer = setInterval(() => isToday && load(), 60_000);
    return () => clearInterval(timer);
  });

  const time = (iso: string) => new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  const total = $derived(report ? report.workSeconds + report.meetingSeconds : 0);

  async function toggleAction(a: DayActionItem) {
    try {
      const stored = await getNotes(a.meetingId);
      if (!stored) return;
      stored.notes.actionItems[a.index].done = !stored.notes.actionItems[a.index].done;
      await saveNotes(a.meetingId, stored.notes);
      actions = await dayActionItems(date);
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function removeDay() {
    try {
      await deleteDayActivity(date);
      confirmDelete = false;
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }
</script>

<Welcome />
<h1>What did I do?</h1>
<DateNav bind:date />

{#if error}<p class="error">{error}</p>{/if}

{#if status?.problem}
  <div class="notice">
    {status.problem}
    <button class="btn" onclick={() => requestTrackingPermission()}>Grant access</button>
  </div>
{/if}

{#if isToday && status}
  <p class="now muted">
    {#if !status.enabled}
      Work tracking is off (Settings).
    {:else if status.idle}
      Away (idle).
    {:else if status.appName}
      Now: <strong>{status.appName}</strong>{status.windowTitle ? ` · ${status.windowTitle}` : ""}
      {#if status.projectId != null}<span class="chip" style:--c={projectColor(byId.get(status.projectId))}>{name(status.projectId)}</span>{/if}
    {/if}
  </p>
{/if}

{#if report}
  <div class="stats">
    <div class="card stat"><span class="muted">Total</span><strong>{formatHours(total)}</strong></div>
    <div class="card stat"><span class="muted">Work</span><strong>{formatHours(report.workSeconds)}</strong></div>
    <div class="card stat"><span class="muted">Meetings</span><strong>{formatHours(report.meetingSeconds)}</strong></div>
    <div class="card stat"><span class="muted">Away</span><strong>{formatHours(report.idleSeconds)}</strong></div>
  </div>

  {#if report.projects.length}
    <div class="bar" aria-label="Time by project">
      {#each report.projects as p}
        <span
          title={`${name(p.projectId)}: ${formatHours(p.seconds)}`}
          style:width={`${(p.seconds / Math.max(1, report.workSeconds)) * 100}%`}
          style:background={projectColor(p.projectId == null ? null : byId.get(p.projectId))}
        ></span>
      {/each}
    </div>
    <ul class="legend">
      {#each report.projects as p}
        <li>
          <span class="dot-c" style:background={projectColor(p.projectId == null ? null : byId.get(p.projectId))}></span>
          {name(p.projectId)} <span class="muted">{formatHours(p.seconds)}</span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if actions.length}
    <h2>Action items from today's meetings</h2>
    <ul class="actions-list">
      {#each actions as a}
        <li>
          <label class:done={a.item.done}>
            <input type="checkbox" checked={a.item.done} onchange={() => toggleAction(a)} />
            {a.item.text}
            {#if a.item.owner}<span class="muted small">· {a.item.owner}</span>{/if}
            <a class="muted small" href={`/meetings/${a.meetingId}`}>{a.meetingTitle ?? "meeting"}</a>
          </label>
        </li>
      {/each}
    </ul>
  {/if}

  <h2>Timeline</h2>
  {#if report.blocks.length === 0}
    <div class="card">
      <p class="muted">
        Nothing recorded for this day yet. TaskMind notes the app and window you use every 10 seconds. Add
        <a href="/projects">projects</a> to group the time.
      </p>
    </div>
  {:else}
    <ol class="timeline">
      {#each report.blocks as b}
        <li class="item {b.kind}">
          <span class="when">{time(b.start)}–{time(b.end)}</span>
          <span
            class="rail"
            style:background={b.kind === "meeting" ? "var(--rec)" : projectColor(b.projectId == null ? null : byId.get(b.projectId))}
          ></span>
          <div class="body">
            {#if b.kind === "meeting"}
              <a href={`/meetings/${b.meetingId}`}><strong>📞 {b.details[0] ?? "Meeting"}</strong></a>
            {:else}
              <strong>{name(b.projectId)}</strong>
            {/if}
            <span class="muted"> · {formatHours(b.seconds)}</span>
            {#if b.kind === "work" && b.details.length}
              <div class="details muted">{b.details.join(" · ")}</div>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  {/if}

  {#if report.apps.length}
    <h2>Top apps</h2>
    <ul class="apps">
      {#each report.apps as a}
        <li><span>{a.key}</span><span class="muted">{formatHours(a.seconds)}</span></li>
      {/each}
    </ul>
  {/if}

  <p class="danger-zone">
    {#if confirmDelete}
      Delete all tracked activity for this day? Meetings are kept.
      <button class="btn danger" onclick={removeDay}>Delete</button>
      <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
    {:else}
      <button class="link" onclick={() => (confirmDelete = true)}>Delete this day's activity…</button>
    {/if}
  </p>
{/if}

<style>
  h2 {
    font-size: 15px;
    margin: 24px 0 8px;
  }
  .error {
    color: var(--rec);
  }
  .notice {
    background: var(--warn-bg);
    color: var(--warn-text);
    border-radius: 8px;
    padding: 8px 12px;
    margin-bottom: 12px;
    display: flex;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
  }
  .now {
    margin: 0 0 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip {
    font-size: 12px;
    border-radius: 999px;
    padding: 1px 8px;
    margin-left: 6px;
    color: #fff;
    background: var(--c);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    padding: 12px 16px;
  }
  .stat strong {
    font-size: 22px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    display: flex;
    height: 10px;
    border-radius: 5px;
    overflow: hidden;
    margin: 16px 0 8px;
    background: var(--border);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    list-style: none;
    padding: 0;
    margin: 0;
    font-size: 13px;
  }
  .dot-c {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    margin-right: 4px;
  }
  .timeline {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .item {
    display: grid;
    grid-template-columns: 110px 4px 1fr;
    gap: 12px;
    padding: 6px 0;
  }
  .when {
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    text-align: right;
    padding-top: 2px;
  }
  .rail {
    border-radius: 2px;
  }
  .body a {
    color: var(--text);
    text-decoration: none;
  }
  .details {
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .apps {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: 420px;
  }
  .apps li {
    display: flex;
    justify-content: space-between;
    padding: 3px 0;
    border-bottom: 1px solid var(--border);
  }
  .actions-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .actions-list label {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 2px 0;
  }
  .done {
    text-decoration: line-through;
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .danger-zone {
    margin-top: 32px;
    font-size: 13px;
    color: var(--muted);
  }
  .link {
    border: none;
    background: none;
    color: var(--muted);
    text-decoration: underline;
    padding: 0;
  }
</style>
