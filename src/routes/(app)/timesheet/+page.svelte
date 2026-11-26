<script lang="ts">
  import {
    errorMessage,
    exportTimesheet,
    getTimesheet,
    isoDate,
    listProjects,
    resetTimesheet,
    saveTimesheet,
    type Project,
    type Timesheet,
    type TimesheetEntry,
  } from "$lib/api";
  import DateNav from "$lib/components/DateNav.svelte";

  let date = $state(isoDate(new Date()));
  let sheet = $state<Timesheet | null>(null);
  let rows = $state<TimesheetEntry[]>([]);
  let projects = $state<Project[]>([]);
  let dirty = $state(false);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);

  async function load() {
    try {
      [sheet, projects] = await Promise.all([getTimesheet(date), listProjects()]);
      rows = sheet.entries.map((e) => ({ ...e }));
      dirty = false;
      error = null;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  $effect(() => {
    date;
    message = null;
    load();
  });

  const total = $derived(rows.reduce((s, r) => s + (Number(r.minutes) || 0), 0));
  const fmt = (m: number) => `${Math.floor(m / 60)}h ${(m % 60).toString().padStart(2, "0")}m`;

  function minutesBetween(start: string, end: string) {
    const [sh, sm] = start.split(":").map(Number);
    const [eh, em] = end.split(":").map(Number);
    const d = eh * 60 + em - (sh * 60 + sm);
    return Number.isFinite(d) && d > 0 ? d : 0;
  }

  function setProject(r: TimesheetEntry, value: string) {
    if (value === "meetings") {
      r.projectId = null;
      r.projectName = "Meetings";
    } else if (value === "none") {
      r.projectId = null;
      r.projectName = "Unassigned";
    } else {
      const p = projects.find((p) => p.id === Number(value));
      r.projectId = p?.id ?? null;
      r.projectName = p?.name ?? "Unassigned";
    }
    dirty = true;
  }

  function projectValue(r: TimesheetEntry) {
    if (r.projectId != null) return String(r.projectId);
    return r.projectName === "Meetings" ? "meetings" : "none";
  }

  function addRow() {
    rows = [...rows, { projectId: null, projectName: "Unassigned", start: "09:00", end: "10:00", minutes: 60, description: "" }];
    dirty = true;
  }

  async function save(approved: boolean) {
    try {
      await saveTimesheet(date, rows.map((r) => ({ ...r, minutes: Number(r.minutes) || 0 })), approved);
      message = approved ? "Approved." : "Draft saved.";
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function reset() {
    try {
      await resetTimesheet(date);
      message = "Regenerated from tracked activity.";
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function doExport() {
    try {
      message = `Saved to ${await exportTimesheet(date, date, "csv")}`;
    } catch (err) {
      error = errorMessage(err);
    }
  }
</script>

<h1>Timesheet</h1>
<DateNav bind:date />

{#if error}<p class="error">{error}</p>{/if}
{#if sheet}
  <p class="muted">
    {#if sheet.status === "generated"}Generated from today's tracked activity and meetings. Edit, then save or approve.
    {:else if sheet.status === "draft"}Saved draft.
    {:else}Approved.{/if}
    Total: <strong>{fmt(total)}</strong>
  </p>

  <table class="sheet">
    <thead>
      <tr><th>Project</th><th>Start</th><th>End</th><th>Minutes</th><th>Description</th><th></th></tr>
    </thead>
    <tbody>
      {#each rows as r, i}
        <tr>
          <td>
            <select value={projectValue(r)} onchange={(e) => setProject(r, e.currentTarget.value)}>
              <option value="none">Unassigned</option>
              <option value="meetings">Meetings</option>
              {#each projects as p}<option value={String(p.id)}>{p.name}</option>{/each}
            </select>
          </td>
          <td>
            <input
              type="time"
              bind:value={r.start}
              oninput={() => ((r.minutes = minutesBetween(r.start, r.end)), (dirty = true))}
            />
          </td>
          <td>
            <input
              type="time"
              bind:value={r.end}
              oninput={() => ((r.minutes = minutesBetween(r.start, r.end)), (dirty = true))}
            />
          </td>
          <td><input class="num" type="number" min="0" max="1440" bind:value={r.minutes} oninput={() => (dirty = true)} /></td>
          <td><input class="desc" bind:value={r.description} oninput={() => (dirty = true)} /></td>
          <td>
            <button
              class="x"
              aria-label="Remove row"
              onclick={() => {
                rows = rows.filter((_, j) => j !== i);
                dirty = true;
              }}>×</button
            >
          </td>
        </tr>
      {:else}
        <tr><td colspan="6" class="muted">No time tracked for this day.</td></tr>
      {/each}
    </tbody>
  </table>

  <div class="actions">
    <button class="btn" onclick={addRow}>+ Add row</button>
    <span class="spacer"></span>
    {#if message}<span class="muted small">{message}</span>{/if}
    {#if sheet.status !== "generated"}<button class="btn" onclick={reset}>Regenerate</button>{/if}
    <button class="btn" onclick={doExport}>Export CSV</button>
    <button class="btn" onclick={() => save(false)} disabled={!dirty && sheet.status !== "generated"}>Save draft</button>
    <button class="btn primary" onclick={() => save(true)}>Approve</button>
  </div>
{/if}

<style>
  .sheet {
    width: 100%;
    border-collapse: collapse;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  th {
    text-align: left;
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
    padding: 8px;
    border-bottom: 1px solid var(--border);
  }
  td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
  }
  input,
  select {
    font: inherit;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  .num {
    width: 70px;
  }
  .desc {
    width: 100%;
  }
  .x {
    border: none;
    background: none;
    color: var(--muted);
    font-size: 18px;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 12px;
  }
  .spacer {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--rec);
  }
  button:disabled {
    opacity: 0.4;
  }
</style>
