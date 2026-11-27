<script lang="ts">
  import { onMount } from "svelte";
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    appInfo,
    deleteModel,
    downloadModel,
    errorMessage,
    formatBytes,
    backupNow,
    checkNotesProvider,
    importV1Status,
    recordingsFolder,
    restoreBackup,
    runImportV1,
    getSetting,
    hasNotesKey,
    setNotesKey,
    listModels,
    onModelProgress,
    setSetting,
    SETTINGS,
    type AppInfo,
    type ModelStatus,
  } from "$lib/api";
  import { transcription } from "$lib/transcription.svelte";

  const LANGUAGES: [string, string][] = [
    ["translate", "Mixed / Hinglish → English text (recommended for Hindi + English meetings)"],
    ["auto", "Detect automatically (one language per meeting)"],
    ["en", "English"],
    ["hi", "Hindi"],
    ["es", "Spanish"],
    ["fr", "French"],
    ["de", "German"],
    ["pt", "Portuguese"],
    ["ar", "Arabic"],
    ["ja", "Japanese"],
    ["zh", "Chinese"],
  ];

  let trackingEnabled = $state(true);
  let idleMinutes = $state(5);
  let browserUrls = $state(false);
  let blocklist = $state("");
  let detection = $state("ask");
  let keywords = $state("");
  const DEFAULT_KEYWORDS = ["Meet - ", "meet.google.com", "Zoom Meeting", "Zoom Webinar", "Meeting in ", "Huddle", "Webex Meeting"];
  const lines = (s: string) => s.split("\n").map((l) => l.trim()).filter(Boolean);

  let notesProvider = $state("off");
  let notesModel = $state("");
  let notesBaseUrl = $state("");
  let notesAuto = $state(true);
  let notesKey = $state("");
  let notesKeySaved = $state(false);
  const PROVIDERS: Record<string, { label: string; model: string; url: string; key: boolean; note: string }> = {
    off: { label: "Off", model: "", url: "", key: false, note: "" },
    ollama: {
      label: "Local model (Ollama)",
      model: "llama3.2",
      url: "http://localhost:11434",
      key: false,
      note: "Install Ollama from ollama.com and run `ollama pull llama3.2`. Nothing leaves this computer.",
    },
    openai: {
      label: "OpenAI-compatible server",
      model: "gpt-4o-mini",
      url: "https://api.openai.com/v1",
      key: true,
      note: "Works with OpenAI, LM Studio, vLLM and others. The transcript is sent to this server.",
    },
    anthropic: {
      label: "Claude (Anthropic)",
      model: "claude-opus-5-5",
      url: "",
      key: true,
      note: "The transcript is sent to Anthropic's API using your own API key.",
    },
  };

  async function refreshKey() {
    notesKeySaved = PROVIDERS[notesProvider]?.key ? await hasNotesKey(notesProvider).catch(() => false) : false;
  }

  async function chooseProvider() {
    await save(SETTINGS.notesProvider, notesProvider === "off" ? null : notesProvider);
    notesModel = "";
    notesBaseUrl = "";
    await save(SETTINGS.notesModel, null);
    await save(SETTINGS.notesBaseUrl, null);
    await refreshKey();
  }

  let checkResult = $state<{ ok: boolean; text: string } | null>(null);
  async function testProvider() {
    checkResult = null;
    try {
      checkResult = { ok: true, text: await checkNotesProvider() };
    } catch (err) {
      checkResult = { ok: false, text: errorMessage(err) };
    }
  }

  async function storeKey() {
    try {
      await setNotesKey(notesProvider, notesKey);
      notesKey = "";
      await refreshKey();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  let folder = $state("");
  let deleteDays = $state(0);
  let dataMessage = $state<string | null>(null);
  let v1 = $state<{ available: boolean; doneAt: string | null } | null>(null);
  let confirmRestore = $state<string | null>(null);

  async function chooseFolder() {
    const picked = await openDialog({ directory: true, multiple: false, title: "Folder for recordings" });
    if (typeof picked === "string") {
      await save(SETTINGS.recordingsDir, picked);
      folder = await recordingsFolder();
      dataMessage = "New recordings will be saved there. Existing recordings stay where they are.";
    }
  }

  async function resetFolder() {
    await save(SETTINGS.recordingsDir, null);
    folder = await recordingsFolder();
  }

  async function doBackup() {
    try {
      dataMessage = `Backup saved to ${await backupNow()}`;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function pickBackup() {
    const picked = await openDialog({ multiple: false, filters: [{ name: "TaskMind backup", extensions: ["zip"] }] });
    if (typeof picked === "string") confirmRestore = picked;
  }

  async function doRestore() {
    if (!confirmRestore) return;
    try {
      await restoreBackup(confirmRestore);
    } catch (err) {
      error = errorMessage(err);
      confirmRestore = null;
    }
  }

  async function doImport() {
    try {
      const r = await runImportV1();
      dataMessage = r.found
        ? `Imported ${r.projects} projects, ${r.activities} activity blocks and ${r.meetings} recordings from TaskMind v1.`
        : "No TaskMind v1 data found.";
      v1 = await importV1Status();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  let models = $state<ModelStatus[]>([]);
  let activeModel = $state<string | null>(null);
  let language = $state("auto");
  let paused = $state(false);
  let progress = $state<Record<string, number>>({});

  let info = $state<AppInfo | null>(null);
  let autostart = $state(false);
  let consentReminder = $state(true);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      info = await appInfo();
      autostart = await isEnabled();
      consentReminder = (await getSetting<boolean>(SETTINGS.consentReminder)) ?? true;
      models = await listModels();
      activeModel = (await getSetting<string>(SETTINGS.transcribeModel)) ?? models.find((m) => m.recommended)?.id ?? null;
      language = (await getSetting<string>(SETTINGS.transcribeLanguage)) ?? "auto";
      paused = (await getSetting<boolean>(SETTINGS.transcribePaused)) ?? false;
      trackingEnabled = (await getSetting<boolean>(SETTINGS.trackingEnabled)) ?? true;
      idleMinutes = (await getSetting<number>(SETTINGS.trackingIdleMinutes)) ?? 5;
      browserUrls = (await getSetting<boolean>(SETTINGS.trackingBrowserUrls)) ?? false;
      blocklist = ((await getSetting<string[]>(SETTINGS.trackingBlocklist)) ?? []).join("\n");
      detection = (await getSetting<string>(SETTINGS.meetingDetection)) ?? "ask";
      keywords = ((await getSetting<string[]>(SETTINGS.meetingKeywords)) ?? DEFAULT_KEYWORDS).join("\n");
      notesProvider = (await getSetting<string>(SETTINGS.notesProvider)) ?? "off";
      notesModel = (await getSetting<string>(SETTINGS.notesModel)) ?? "";
      notesBaseUrl = (await getSetting<string>(SETTINGS.notesBaseUrl)) ?? "";
      notesAuto = (await getSetting<boolean>(SETTINGS.notesAuto)) ?? true;
      await refreshKey();
      folder = await recordingsFolder();
      deleteDays = (await getSetting<number>(SETTINGS.deleteAudioAfterDays)) ?? 0;
      v1 = await importV1Status();
      transcription.init();
      onModelProgress(async (p) => {
        if (p.done) {
          const { [p.id]: _, ...rest } = progress;
          progress = rest;
          if (p.error) error = p.error;
          models = await listModels();
        } else {
          progress = { ...progress, [p.id]: p.downloaded / p.total };
        }
      });
    } catch (err) {
      error = errorMessage(err);
    }
  });

  async function toggleAutostart() {
    try {
      if (autostart) await disable();
      else await enable();
      autostart = await isEnabled();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function save(key: string, value: unknown) {
    try {
      await setSetting(key, value);
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function download(id: string) {
    progress = { ...progress, [id]: 0 };
    try {
      await downloadModel(id);
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function removeModel(id: string) {
    try {
      await deleteModel(id);
      models = await listModels();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function chooseModel(id: string) {
    activeModel = id;
    await save(SETTINGS.transcribeModel, id);
  }

  const statusText = $derived.by(() => {
    const s = transcription.status;
    if (s.state === "working") return `Transcribing a meeting… ${Math.round(s.progress * 100)}%`;
    if (s.state === "paused") return "Paused";
    if (s.state === "needs_model") return "Waiting: download the selected model below";
    return "Idle";
  });

  async function toggleConsent() {
    try {
      await setSetting(SETTINGS.consentReminder, !consentReminder);
      consentReminder = !consentReminder;
    } catch (err) {
      error = errorMessage(err);
    }
  }
</script>

<h1>Settings</h1>
{#if error}<p class="error">{error}</p>{/if}

<section class="card">
  <h2>General</h2>
  <label class="row">
    <input type="checkbox" checked={autostart} onchange={toggleAutostart} />
    <span>Start TaskMind when I log in</span>
  </label>
  <label class="row">
    <input type="checkbox" checked={consentReminder} onchange={toggleConsent} />
    <span>Remind me to tell participants when I start recording</span>
  </label>
</section>

<section class="card">
  <h2>Work tracking</h2>
  <p class="muted small">Records the app and window title you use every 10 seconds. Never screenshots. Stays on this computer.</p>
  <label class="row">
    <input
      type="checkbox"
      checked={trackingEnabled}
      onchange={() => {
        trackingEnabled = !trackingEnabled;
        save(SETTINGS.trackingEnabled, trackingEnabled);
      }}
    />
    <span>Track my work</span>
  </label>
  <label class="row">
    <span>Count me as away after</span>
    <input
      class="num"
      type="number"
      min="1"
      max="120"
      bind:value={idleMinutes}
      onchange={() => save(SETTINGS.trackingIdleMinutes, Math.min(120, Math.max(1, Number(idleMinutes) || 5)))}
    />
    <span>minutes without keyboard or mouse</span>
  </label>
  <label class="row">
    <input
      type="checkbox"
      checked={browserUrls}
      onchange={() => {
        browserUrls = !browserUrls;
        save(SETTINGS.trackingBrowserUrls, browserUrls);
      }}
    />
    <span>Record web addresses (reads a copy of Chrome/Edge/Brave/Firefox history to match tab titles)</span>
  </label>
  <label class="col">
    <span>Never record (one per line: app names, words in titles, or website addresses)</span>
    <textarea
      rows="3"
      placeholder={"1password\nbank\nmail.google.com"}
      bind:value={blocklist}
      onchange={() => save(SETTINGS.trackingBlocklist, lines(blocklist))}
    ></textarea>
  </label>
</section>

<section class="card">
  <h2>Meeting detection</h2>
  <label class="row">
    <span>When a meeting window opens</span>
    <select bind:value={detection} onchange={() => save(SETTINGS.meetingDetection, detection)}>
      <option value="ask">Ask me to record</option>
      <option value="auto">Record automatically (and stop when it closes)</option>
      <option value="off">Do nothing</option>
    </select>
  </label>
  <label class="col">
    <span>Window titles that mean a meeting (one per line)</span>
    <textarea rows="4" bind:value={keywords} onchange={() => save(SETTINGS.meetingKeywords, lines(keywords))}></textarea>
  </label>
</section>

<section class="card">
  <h2>Transcription</h2>
  <p class="muted">Runs on this computer only. Status: {statusText}</p>
  <label class="row">
    <input
      type="checkbox"
      checked={paused}
      onchange={() => {
        paused = !paused;
        save(SETTINGS.transcribePaused, paused);
      }}
    />
    <span>Pause transcription (for example on battery)</span>
  </label>
  <label class="row">
    <span>Language</span>
    <select bind:value={language} onchange={() => save(SETTINGS.transcribeLanguage, language)}>
      {#each LANGUAGES as [code, name]}<option value={code}>{name}</option>{/each}
    </select>
  </label>
  <table class="models">
    <tbody>
      {#each models as m}
        <tr>
          <td>
            <label class="row">
              <input
                type="radio"
                name="model"
                checked={activeModel === m.id}
                disabled={!m.downloaded}
                onchange={() => chooseModel(m.id)}
              />
              <span>
                <strong>{m.label}</strong>
                {#if m.recommended}<span class="tag">recommended</span>{/if}
                <br /><span class="muted small">{m.description} {formatBytes(m.sizeBytes)}</span>
              </span>
            </label>
          </td>
          <td class="model-action">
            {#if progress[m.id] != null}
              <span class="small">Downloading {Math.round(progress[m.id] * 100)}%</span>
            {:else if m.downloaded}
              <button class="btn" onclick={() => removeModel(m.id)}>Delete</button>
            {:else}
              <button class="btn primary" onclick={() => download(m.id)}>Download</button>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="muted small">Models download once from Hugging Face and are checked before use. Pick a downloaded model to use it.</p>
</section>

<section class="card" id="notes">
  <h2>AI meeting notes</h2>
  <p class="muted small">Summary, decisions and action items written from the transcript. Off by default.</p>
  <label class="row">
    <span>Provider</span>
    <select bind:value={notesProvider} onchange={chooseProvider}>
      {#each Object.entries(PROVIDERS) as [id, p]}<option value={id}>{p.label}</option>{/each}
    </select>
  </label>
  {#if notesProvider !== "off"}
    <p class="muted small">{PROVIDERS[notesProvider].note}</p>
    <label class="row">
      <span>Model</span>
      <input
        class="text"
        placeholder={PROVIDERS[notesProvider].model}
        bind:value={notesModel}
        onchange={() => save(SETTINGS.notesModel, notesModel.trim() || null)}
      />
    </label>
    {#if notesProvider !== "anthropic"}
      <label class="row">
        <span>Server address</span>
        <input
          class="text"
          placeholder={PROVIDERS[notesProvider].url}
          bind:value={notesBaseUrl}
          onchange={() => save(SETTINGS.notesBaseUrl, notesBaseUrl.trim() || null)}
        />
      </label>
    {/if}
    {#if PROVIDERS[notesProvider].key}
      <label class="row">
        <span>API key</span>
        <input class="text" type="password" placeholder={notesKeySaved ? "Saved in your keychain" : "Paste key"} bind:value={notesKey} />
        <button class="btn" onclick={storeKey} disabled={!notesKey.trim()}>Save</button>
        {#if notesKeySaved}
          <button class="btn" onclick={() => ((notesKey = ""), setNotesKey(notesProvider, "").then(refreshKey))}>Remove</button>
        {/if}
      </label>
    {/if}
    <label class="row">
      <input
        type="checkbox"
        checked={notesAuto}
        onchange={() => {
          notesAuto = !notesAuto;
          save(SETTINGS.notesAuto, notesAuto);
        }}
      />
      <span>Write notes automatically when a transcript is ready</span>
    </label>
    <div class="row">
      <button class="btn" onclick={testProvider}>Test connection</button>
      {#if checkResult}<span class={checkResult.ok ? "ok" : "warn"}>{checkResult.text}</span>{/if}
    </div>
  {/if}
</section>

<section class="card">
  <h2>Storage and backup</h2>
  {#if dataMessage}<p class="muted small">{dataMessage}</p>{/if}
  <div class="row">
    <span>Recordings folder</span>
    <code class="path grow">{folder}</code>
    <button class="btn" onclick={chooseFolder}>Change…</button>
    <button class="btn" onclick={resetFolder}>Default</button>
  </div>
  <p class="muted small">
    Tip: choose a folder synced by the Google Drive, OneDrive or Dropbox desktop app to keep recordings in the cloud.
  </p>
  <label class="row">
    <span>Delete meeting audio after</span>
    <select bind:value={deleteDays} onchange={() => save(SETTINGS.deleteAudioAfterDays, Number(deleteDays))}>
      <option value={0}>Never</option>
      <option value={7}>7 days</option>
      <option value={30}>30 days</option>
      <option value={90}>90 days</option>
      <option value={365}>1 year</option>
    </select>
    <span class="muted small">Transcripts and notes are kept.</span>
  </label>
  <div class="row">
    <button class="btn" onclick={doBackup}>Back up now</button>
    {#if confirmRestore}
      <span class="small warn">Replace all current data with this backup? The app will restart.</span>
      <button class="btn danger" onclick={doRestore}>Restore</button>
      <button class="btn" onclick={() => (confirmRestore = null)}>Cancel</button>
    {:else}
      <button class="btn" onclick={pickBackup}>Restore from backup…</button>
    {/if}
  </div>
  <p class="muted small">Backups contain settings, projects, activity, transcripts and notes (not audio files).</p>
  {#if v1?.available}
    <div class="row">
      {#if v1.doneAt}
        <span class="muted small">TaskMind v1 data imported on {new Date(v1.doneAt).toLocaleDateString()}.</span>
      {:else}
        <span>TaskMind v1 data found on this computer.</span>
        <button class="btn primary" onclick={doImport}>Import it</button>
      {/if}
    </div>
  {/if}
</section>

<section class="card">
  <h2>Keyboard shortcuts</h2>
  {#if info}
    <dl>
      <dt>Start / stop recording</dt>
      <dd><kbd>{info.shortcuts.toggleRecording}</kbd></dd>
      <dt>Open TaskMind</dt>
      <dd><kbd>{info.shortcuts.showApp}</kbd></dd>
    </dl>
    <p class="muted">Custom shortcuts are coming. On some Linux desktops (GNOME Wayland) global shortcuts may be blocked. Use the tray or the recorder widget instead.</p>
  {/if}
</section>

<section class="card">
  <h2>About</h2>
  {#if info}
    <dl>
      <dt>Version</dt>
      <dd>{info.version}</dd>
      <dt>Database</dt>
      <dd class="path">{info.dbPath ?? "in memory"}</dd>
      <dt>Schema version</dt>
      <dd>{info.schemaVersion}</dd>
      <dt>Stored meetings</dt>
      <dd>{info.meetingCount}</dd>
      <dt>Stored activities</dt>
      <dd>{info.activityCount}</dd>
    </dl>
  {/if}
</section>

<style>
  section {
    margin-top: 16px;
  }
  h2 {
    font-size: 15px;
    margin: 0 0 10px;
  }
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 4px 0;
  }
  dl {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 6px 12px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  .path {
    font-family: ui-monospace, monospace;
    font-size: 12px;
    word-break: break-all;
  }
  kbd {
    font-family: ui-monospace, monospace;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 1px 6px;
    background: var(--bg);
  }
  .error {
    color: var(--rec);
  }
  .models {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
  }
  .models td {
    padding: 6px 0;
    border-top: 1px solid var(--border);
  }
  .model-action {
    text-align: right;
    white-space: nowrap;
  }
  .small {
    font-size: 12px;
  }
  .tag {
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 999px;
    padding: 1px 8px;
    margin-left: 4px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 0;
  }
  textarea {
    font: inherit;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .path {
    font-size: 12px;
    background: var(--bg);
    border-radius: 4px;
    padding: 2px 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .warn {
    color: var(--rec);
  }
  .ok {
    color: #30a46c;
  }
  .text {
    flex: 1;
    max-width: 380px;
    font: inherit;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  .num {
    width: 64px;
    font: inherit;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  select {
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
  }
</style>
