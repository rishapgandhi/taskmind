<script lang="ts">
  import {
    addDays,
    errorMessage,
    exportTimesheet,
    formatHours,
    isoDate,
    listProjects,
    projectColor,
    rangeReport,
    type DaySummary,
    type Project,
  } from "$lib/api";

  type Range = "week" | "month";
  let range = $state<Range>("week");
  let offset = $state(0);
  let days = $state<DaySummary[]>([]);
  let projects = $state<Project[]>([]);
  let error = $state<string | null>(null);
  let exported = $state<string | null>(null);

  const byId = $derived(new Map(projects.map((p) => [p.id, p])));
  const name = (id: number | null) => (id == null ? "Unassigned" : (byId.get(id)?.name ?? "Unassigned"));

  // Weeks start on Monday.
  const bounds = $derived.by(() => {
    const now = new Date();
    if (range === "week") {
      const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - ((now.getDay() + 6) % 7) + offset * 7);
      const from = isoDate(monday);
      return { from, to: addDays(from, 6) };
    }
    const first = new Date(now.getFullYear(), now.getMonth() + offset, 1);
    const last = new Date(first.getFullYear(), first.getMonth() + 1, 0);
    return { from: isoDate(first), to: isoDate(last) };
  });

  const title = $derived.by(() => {
    const f = new Date(`${bounds.from}T12:00:00`);
    if (range === "month") return f.toLocaleDateString(undefined, { month: "long", year: "numeric" });
    const t = new Date(`${bounds.to}T12:00:00`);
    return `${f.toLocaleDateString(undefined, { day: "numeric", month: "short" })} – ${t.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}`;
  });

  $effect(() => {
    const { from, to } = bounds;
    exported = null;
    Promise.all([rangeReport(from, to), listProjects()])
      .then(([d, p]) => {
        days = d;
        projects = p;
        error = null;
      })
      .catch((err) => (error = errorMessage(err)));
  });

  const max = $derived(Math.max(3600, ...days.map((d) => d.workSeconds + d.meetingSeconds)));
  const totals = $derived.by(() => {
    const m = new Map<number | null, number>();
    for (const d of days) for (const p of d.projects) m.set(p.projectId, (m.get(p.projectId) ?? 0) + p.seconds);
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  });
  const workTotal = $derived(days.reduce((s, d) => s + d.workSeconds, 0));
  const meetingTotal = $derived(days.reduce((s, d) => s + d.meetingSeconds, 0));

  async function doExport(format: "csv" | "json") {
    try {
      exported = await exportTimesheet(bounds.from, bounds.to, format);
    } catch (err) {
      error = errorMessage(err);
    }
  }
</script>

<h1>Reports</h1>
<div class="toolbar">
  <div class="seg">
    <button class:on={range === "week"} onclick={() => ((range = "week"), (offset = 0))}>Week</button>
    <button class:on={range === "month"} onclick={() => ((range = "month"), (offset = 0))}>Month</button>
  </div>
  <button class="btn" aria-label="Previous" onclick={() => offset--}>‹</button>
  <strong class="title">{title}</strong>
  <button class="btn" aria-label="Next" disabled={offset >= 0} onclick={() => offset++}>›</button>
  <span class="spacer"></span>
  <button class="btn" onclick={() => doExport("csv")}>Export CSV</button>
  <button class="btn" onclick={() => doExport("json")}>Export JSON</button>
</div>
{#if exported}<p class="muted small">Saved to {exported}</p>{/if}
{#if error}<p class="error">{error}</p>{/if}

<p class="muted">Work {formatHours(workTotal)} · Meetings {formatHours(meetingTotal)}</p>

<div class="chart" class:month={range === "month"}>
  {#each days as d}
    <a class="col" href="/" title={`${d.date}: ${formatHours(d.workSeconds + d.meetingSeconds)}`}>
      <div class="stack">
        <span class="seg-m" style:height={`${(d.meetingSeconds / max) * 100}%`}></span>
        {#each d.projects as p}
          <span
            style:height={`${(p.seconds / max) * 100}%`}
            style:background={projectColor(p.projectId == null ? null : byId.get(p.projectId))}
          ></span>
        {/each}
      </div>
      <span class="day muted">
        {range === "week"
          ? new Date(`${d.date}T12:00:00`).toLocaleDateString(undefined, { weekday: "short" })
          : Number(d.date.slice(8))}
      </span>
    </a>
  {/each}
</div>

<h2>By project</h2>
<table class="totals">
  <tbody>
    {#each totals as [id, secs]}
      <tr>
        <td><span class="dot-c" style:background={projectColor(id == null ? null : byId.get(id))}></span>{name(id)}</td>
        <td class="num">{formatHours(secs)}</td>
      </tr>
    {/each}
    <tr>
      <td><span class="dot-c" style:background="var(--rec)"></span>Meetings</td>
      <td class="num">{formatHours(meetingTotal)}</td>
    </tr>
  </tbody>
</table>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
    flex-wrap: wrap;
  }
  .title {
    min-width: 200px;
    text-align: center;
  }
  .spacer {
    flex: 1;
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
  button:disabled {
    opacity: 0.4;
  }
  .chart {
    display: flex;
    gap: 8px;
    align-items: flex-end;
    height: 220px;
    margin: 16px 0;
  }
  .chart.month {
    gap: 3px;
  }
  .col {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    height: 100%;
    text-decoration: none;
  }
  .stack {
    flex: 1;
    width: 100%;
    display: flex;
    flex-direction: column-reverse;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg-m {
    background: var(--rec);
    opacity: 0.8;
  }
  .day {
    font-size: 11px;
    margin-top: 4px;
  }
  h2 {
    font-size: 15px;
    margin: 16px 0 8px;
  }
  .totals {
    border-collapse: collapse;
    min-width: 360px;
  }
  .totals td {
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .dot-c {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    margin-right: 6px;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--rec);
  }
</style>
