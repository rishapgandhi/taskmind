<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { formatElapsed, onConsentReminder, onMeetingDetected, showMainWindow } from "$lib/api";
  import { rec } from "$lib/recorder.svelte";

  type Notice = { text: string; kind: "warn" | "error" };
  let notice = $state<Notice | null>(null);
  let detected = $state<string | null>(null);
  let detectedTimer: ReturnType<typeof setTimeout> | undefined;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  function show(text: string, kind: Notice["kind"], ms: number) {
    notice = { text, kind };
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), ms);
  }

  onMount(() => {
    rec.init();
    onConsentReminder(() => show("Tell participants you are recording", "warn", 6000));
    onMeetingDetected((title) => {
      if (rec.state.status !== "idle") return;
      detected = title;
      clearTimeout(detectedTimer);
      detectedTimer = setTimeout(() => (detected = null), 60_000);
    });
  });

  // Errors from shortcuts or the tray only show up here.
  $effect(() => {
    if (rec.error) show(rec.error, "error", 8000);
  });

  // A missing source (for example no system-audio permission) is shown once per recording.
  $effect(() => {
    const warnings = rec.state.sources?.warnings ?? [];
    if (warnings.length) show(warnings.join(" · "), "warn", 8000);
  });

  const hide = () => getCurrentWindow().hide();
  const pct = (v: number) => `${Math.round(v * 100)}%`;
</script>

<div class="pill" data-tauri-drag-region>
  {#if detected && rec.state.status === "idle"}
    <span class="notice warn" title={detected} data-tauri-drag-region>Meeting detected. Record it?</span>
    <div class="controls">
      <button class="yes" onclick={() => ((detected = null), rec.start())}>Record</button>
      <button title="Dismiss" aria-label="Dismiss" onclick={() => (detected = null)}>✕</button>
    </div>
  {:else}
  {#if notice}
    <span class="notice {notice.kind}" title={notice.text} data-tauri-drag-region>{notice.text}</span>
  {:else}
    <span class="dot {rec.state.status}" data-tauri-drag-region></span>
    <div class="info" data-tauri-drag-region>
      <span class="time" data-tauri-drag-region>
        {rec.state.status === "idle" ? "Ready" : formatElapsed(rec.displayMs)}
      </span>
      {#if rec.state.status !== "idle"}
        <div class="meters" data-tauri-drag-region>
          <span class="meter" title="You (microphone)" class:off={!rec.state.sources?.mic}>
            <span style:width={pct(rec.levels.mic)}></span>
          </span>
          <span class="meter sys" title="Others (system audio)" class:off={!rec.state.sources?.system}>
            <span style:width={pct(rec.levels.system)}></span>
          </span>
        </div>
      {/if}
    </div>
  {/if}

  <div class="controls">
    {#if rec.state.status === "idle"}
      <button class="rec" title="Start recording" aria-label="Start recording" onclick={rec.start}>●</button>
    {:else}
      {#if rec.state.status === "recording"}
        <button title="Pause" aria-label="Pause" onclick={rec.pause}>❚❚</button>
      {:else}
        <button title="Resume" aria-label="Resume" onclick={rec.resume}>▶</button>
      {/if}
      <button title="Stop" aria-label="Stop" onclick={rec.stop}>■</button>
    {/if}
    <button title="Open TaskMind" aria-label="Open TaskMind" onclick={showMainWindow}>⤢</button>
    <button title="Hide recorder" aria-label="Hide recorder" onclick={hide}>–</button>
  </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: var(--surface);
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
  }
  .pill {
    height: 100vh;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px 0 14px;
    border: 1px solid var(--border);
    cursor: grab;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .time {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .meters {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 90px;
  }
  .meter {
    height: 3px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    background: #30a46c;
    transition: width 90ms linear;
  }
  .meter.sys span {
    background: var(--accent);
  }
  .meter.off {
    opacity: 0.3;
  }
  .notice {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    line-height: 1.3;
    border-radius: 6px;
    padding: 3px 7px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .notice.warn {
    color: var(--warn-text);
    background: var(--warn-bg);
  }
  .notice.error {
    color: #fff;
    background: var(--rec);
  }
  .controls {
    display: flex;
    gap: 1px;
  }
  button {
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 8px;
    background: transparent;
    font-size: 13px;
  }
  button:hover {
    background: var(--bg);
  }
  button.yes {
    width: auto;
    padding: 0 10px;
    background: var(--rec);
    color: #fff;
    font-weight: 600;
  }
  button.rec {
    color: var(--rec);
    font-size: 16px;
  }
</style>
