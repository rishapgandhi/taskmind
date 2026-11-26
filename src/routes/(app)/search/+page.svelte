<script lang="ts">
  import { errorMessage, formatClock, searchTranscripts, snippetParts, SPEAKER_LABEL, type SearchHit } from "$lib/api";

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let searched = $state(false);
  let error = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function onInput() {
    clearTimeout(timer);
    timer = setTimeout(run, 200);
  }

  async function run() {
    const q = query.trim();
    if (!q) {
      hits = [];
      searched = false;
      return;
    }
    try {
      hits = await searchTranscripts(q);
      searched = true;
      error = null;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  function day(iso: string) {
    return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }
</script>

<h1>Search</h1>
<p class="muted">Search everything said in your meetings. Work activity search arrives in M4.</p>

<!-- svelte-ignore a11y_autofocus -->
<input class="search" type="search" placeholder="e.g. invoice, budget, Friday" bind:value={query} oninput={onInput} autofocus />
{#if error}<p class="error">{error}</p>{/if}

{#if searched && hits.length === 0}
  <p class="muted">No matches.</p>
{/if}

<ul class="hits">
  {#each hits as h}
    <li>
      <a class="card hit" href={`/meetings/${h.meetingId}?t=${h.startMs}`}>
        <div class="meta muted">
          {h.title ?? "Meeting"} · {day(h.startedAt)} · {formatClock(h.startMs)} · {SPEAKER_LABEL[h.speaker] ?? h.speaker}
        </div>
        <div>
          {#each snippetParts(h.snippet) as p}
            {#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}
          {/each}
        </div>
      </a>
    </li>
  {/each}
</ul>

<style>
  .search {
    width: 100%;
    font-size: 16px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    margin: 8px 0 16px;
  }
  .hits {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hit {
    display: block;
    color: var(--text);
    text-decoration: none;
  }
  .hit:hover {
    border-color: var(--accent);
  }
  .meta {
    font-size: 12px;
    margin-bottom: 4px;
  }
  mark {
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 3px;
    padding: 0 2px;
  }
  .error {
    color: var(--rec);
  }
</style>
