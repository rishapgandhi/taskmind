<script lang="ts">
  import { onMount } from "svelte";
  import { enable as enableAutostart } from "@tauri-apps/plugin-autostart";
  import {
    appInfo,
    getSetting,
    listModels,
    listProjects,
    requestTrackingPermission,
    setSetting,
    SETTINGS,
    trackingStatus,
  } from "$lib/api";

  let visible = $state(false);
  let shortcut = $state("");
  let hasModel = $state(false);
  let hasProject = $state(false);
  let problem = $state<string | null>(null);
  let autostartOn = $state(false);

  onMount(async () => {
    if ((await getSetting<boolean>(SETTINGS.onboardingDone).catch(() => true)) === true) return;
    visible = true;
    const [info, models, projects, status] = await Promise.all([appInfo(), listModels(), listProjects(), trackingStatus()]);
    shortcut = info.shortcuts.toggleRecording;
    hasModel = models.some((m) => m.downloaded);
    hasProject = projects.length > 0;
    problem = status.problem;
  });

  async function dismiss() {
    visible = false;
    await setSetting(SETTINGS.onboardingDone, true);
  }
</script>

{#if visible}
  <section class="card welcome">
    <div class="head">
      <h2>Welcome to TaskMind</h2>
      <button class="btn" onclick={dismiss}>Done</button>
    </div>
    <ol>
      <li class:ok={!problem}>
        <strong>Let TaskMind see window titles.</strong>
        {#if problem}
          {problem} <button class="btn" onclick={() => requestTrackingPermission()}>Grant access</button>
        {:else}
          Work tracking is running.
        {/if}
      </li>
      <li class:ok={hasModel}>
        <strong>Download a speech model</strong> for transcripts (about 150 MB, once).
        {#if !hasModel}<a href="/settings">Open Settings → Transcription</a>{/if}
      </li>
      <li>
        <strong>Record a meeting:</strong> press <kbd>{shortcut}</kbd> or ● in the small recorder window. The first time, allow
        the microphone and system audio. Use headphones for the cleanest transcript.
      </li>
      <li class:ok={hasProject}>
        <strong>Create projects</strong> so your time is grouped. <a href="/projects">Add a project</a>
      </li>
      <li class:ok={autostartOn}>
        <strong>Start at login</strong> so tracking never misses a day.
        {#if !autostartOn}
          <button class="btn" onclick={() => enableAutostart().then(() => (autostartOn = true))}>Turn on</button>
        {/if}
      </li>
    </ol>
    <p class="muted small">Everything stays on this computer. Tell participants before you record a meeting.</p>
  </section>
{/if}

<style>
  .welcome {
    margin-bottom: 16px;
    border-color: var(--accent);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  ol {
    padding-left: 20px;
  }
  li {
    margin: 8px 0;
  }
  li.ok::marker {
    content: "✓  ";
    color: #30a46c;
  }
  kbd {
    font-family: ui-monospace, monospace;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 1px 6px;
  }
  .small {
    font-size: 12px;
  }
</style>
