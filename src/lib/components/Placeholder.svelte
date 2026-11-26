<script lang="ts">
  let { title, milestone, items }: { title: string; milestone: string; items: string[] } = $props();
</script>

<h1>{title}</h1>
<p class="muted">Arrives in {milestone}. See docs/PLAN.md.</p>
<div class="card">
  <ul>
    {#each items as item}
      <li>{item}</li>
    {/each}
  </ul>
</div>

<style>
  ul {
    margin: 0;
    padding-left: 18px;
  }
  li {
    margin: 4px 0;
  }
</style>
