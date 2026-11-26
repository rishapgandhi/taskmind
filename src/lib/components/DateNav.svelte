<script lang="ts">
  import { addDays, isoDate } from "$lib/api";

  let { date = $bindable(), step = 1 }: { date: string; step?: number } = $props();
  const today = isoDate(new Date());

  const label = $derived(
    new Date(`${date}T12:00:00`).toLocaleDateString(undefined, {
      weekday: "long",
      day: "numeric",
      month: "long",
      year: "numeric",
    }),
  );
</script>

<div class="nav">
  <button class="btn" aria-label="Previous" onclick={() => (date = addDays(date, -step))}>‹</button>
  <span class="label">{label}</span>
  <button class="btn" aria-label="Next" disabled={date >= today} onclick={() => (date = addDays(date, step))}>›</button>
  {#if date !== today}
    <button class="btn" onclick={() => (date = today)}>Today</button>
  {/if}
</div>

<style>
  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 16px;
  }
  .label {
    font-weight: 600;
    min-width: 260px;
    text-align: center;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
