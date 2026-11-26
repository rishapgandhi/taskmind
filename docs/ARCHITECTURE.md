# TaskMind v2 Architecture

## Overview

```
┌──────────────────────── Tauri app ────────────────────────┐
│  Frontend (Svelte 5, src/)                                 │
│   main window: Timeline · Meetings · Timesheet · Search ·  │
│                Projects · Settings                         │
│   widget window: ● 00:12:31  ▮▮▯  ⏸ ⏹                     │
│        ▲ invoke() / events                                 │
│  Backend (Rust, src-tauri/)                                │
│   commands/ ── recorder ── audio/{macos,windows,linux}     │
│            │            └─ encoder (Opus/Ogg)              │
│            ├─ transcribe/ (whisper-rs, job queue)          │
│            ├─ notes/ (Ollama / OpenAI-compatible / Claude) │
│            ├─ tracker/ ── activity/{macos,windows,linux}   │
│            │           └─ classify/ ── timesheet/          │
│            ├─ detect/ (meeting detection)                  │
│            ├─ sync/ (Drive, OneDrive, backup)              │
│            └─ db/ (SQLite + FTS5, migrations)              │
└────────────────────────────────────────────────────────────┘
```

## Planned source layout

```
v2/
├── src/                        # Svelte frontend
│   ├── lib/api.ts              # typed invoke() wrappers + event types
│   ├── lib/components/
│   ├── routes/ (or views/)     # timeline, meetings, meeting/[id], timesheet, search, projects, settings
│   └── widget/                 # floating recorder window entry
├── src-tauri/
│   ├── src/
│   │   ├── main.rs / lib.rs    # app setup, tray, shortcuts, windows
│   │   ├── commands/           # thin Tauri command handlers
│   │   ├── audio/              # mod.rs (traits), mixer.rs, encoder.rs, macos.rs, windows.rs, linux.rs
│   │   ├── recorder.rs         # state machine: Idle → Recording ⇄ Paused → Finalizing
│   │   ├── transcribe/         # models.rs, queue.rs, whisper.rs
│   │   ├── notes/              # provider trait + implementations, prompts
│   │   ├── activity/           # mod.rs (traits), macos.rs, windows.rs, linux_x11.rs, linux_gnome.rs, idle.rs
│   │   ├── tracker.rs          # 10 s sampling loop, idle handling, blocklist
│   │   ├── classify.rs         # project rules
│   │   ├── timesheet.rs        # block merging, exports
│   │   ├── detect.rs           # meeting detection
│   │   ├── gitwatch.rs
│   │   ├── sync/               # drive.rs, onedrive.rs, backup.rs, import_v1.rs
│   │   └── db/                 # mod.rs, queries
│   ├── migrations/             # 0001_init.sql, ...
│   ├── capabilities/           # Tauri permission files per window
│   └── tauri.conf.json
└── docs/
```

## Key crates (check latest versions when adding)

| Need | Crate |
|---|---|
| Tray, windows, events | `tauri` 2 |
| Global shortcut, autostart, updater, dialog, opener | `tauri-plugin-global-shortcut`, `-autostart`, `-updater`, `-dialog`, `-opener` |
| Mic capture, WASAPI loopback | `cpal` (Windows loopback via WASAPI), `windows` crate for anything cpal lacks |
| macOS system audio | `objc2` + `objc2-core-audio` / `objc2-screen-capture-kit` (or `screencapturekit` crate) |
| Linux audio | `libpulse-binding` + `libpulse-simple-binding` (PulseAudio and PipeWire-pulse) |
| Resampling | `rubato` |
| Opus + Ogg | `opus` (or `audiopus`) + `ogg` |
| Echo cancellation | WebRTC AEC3 binding (evaluate `webrtc-audio-processing` or `aec3`) |
| Transcription | `whisper-rs` |
| DB | `rusqlite` with `bundled` (FTS5 on) |
| Linux window tracking | `x11rb`, `zbus` |
| macOS window tracking | `objc2-app-kit`, `accessibility-sys` or `objc2-application-services` |
| Windows window tracking | `windows` crate (`Win32_UI_WindowsAndMessaging`, `Win32_System_ProcessStatus`) |
| HTTP (models, LLM, Drive) | `reqwest` with rustls |
| Secrets | `keyring` |
| Errors / logs | `thiserror`, `anyhow`, `tracing`, `tracing-appender` |

## Audio pipeline (as built in M1)

```
cpal mic stream ───► ring buffer ─┐                      ┌─► .tracks/<stamp>-mic.ogg    (Opus 24 kbps)
                                   ├─► resample 48 kHz ───┤
cpal system stream ► ring buffer ─┘   align to clock     ├─► .tracks/<stamp>-system.ogg (Opus 24 kbps)
                                       (150 ms buffer)    └─► mix ─► <stamp>.ogg          (Opus 32 kbps mono)
```

- Code: `src-tauri/src/audio/` (`capture.rs`, `loopback.rs` for macOS/Windows, `linux.rs`, `resample.rs`, `mixer.rs`, `encoder.rs`, `session.rs`).
- One recorder thread owns the cpal streams and runs every 20 ms. The wall clock is the master: each tick takes exactly the samples due from each source, pads silence if a source is behind (Windows loopback sends nothing during silence) and drops backlog beyond 250 ms (device clock drift).
- Files live in `<app data>/recordings/YYYY-MM/`. Ogg pages are flushed every second.
- Measured size: about 11 MB per hour for the mixed file plus about 10 MB per hour for the two tracks. M2 deletes the tracks after transcription.
- Not built yet: echo cancellation, device-change restart, band-limited resampling.

## Data model (SQLite, first migration)

```sql
projects(id, name, color, created_at)
project_rules(id, project_id, kind, pattern, priority)   -- kind: window_title|app_name|git_branch|directory|url
activities(id, started_at, ended_at, app_name, window_title, url, project_id, is_idle)
git_events(id, at, repo_path, kind, branch, message, files_changed, project_id)
meetings(id, title, started_at, ended_at, duration_s, audio_path, audio_bytes, source_app,
         status,            -- recording|finalizing|queued|transcribing|done|failed
         project_id, created_at)
transcript_segments(id, meeting_id, start_ms, end_ms, speaker, text)   -- speaker: you|others|<name>
meeting_notes(meeting_id PK, summary, decisions_json, action_items_json, provider, model, generated_at, edited)
timesheet_entries(id, date, project_id, start_at, end_at, minutes, description, status)
daily_recaps(date PK, summary, data_json, generated_at)
uploads(id, meeting_id, target, remote_id, status, uploaded_at)
settings(key PK, value_json)
-- FTS5: activities_fts(window_title, app_name, url), segments_fts(text)
```

Timestamps are stored as UTC ISO 8601. The UI converts to local time. v1 had a UTC vs local date bug, so day boundaries must use the user's local timezone.

## Per-OS strategy

| Feature | macOS | Windows | Linux |
|---|---|---|---|
| System audio | Core Audio process tap via cpal (14.6+). ScreenCaptureKit fallback planned for 13.x | WASAPI loopback via cpal | Default sink's monitor source via cpal's pure-Rust PulseAudio client (PipeWire via pipewire-pulse) |
| Mic | cpal (Core Audio) | cpal (WASAPI) | cpal (PulseAudio) |
| Active window | NSWorkspace + Accessibility | GetForegroundWindow | X11: `_NET_ACTIVE_WINDOW`. GNOME Wayland: Window Calls extension D-Bus |
| Idle | IOKit HIDIdleTime | GetLastInputInfo | X11: XScreenSaver. GNOME Wayland: Mutter IdleMonitor D-Bus |
| Permissions | Mic, Screen & System Audio Recording, Accessibility | Mic privacy toggle | None (extension install on GNOME Wayland) |
| Installer | .dmg (signed + notarized) | .msi / NSIS .exe (signed) | .deb, AppImage |
| Min version | macOS 13 | Windows 10 21H2 | Ubuntu 22.04 / glibc 2.35 |

## Threads and state

- `Recorder` owns audio threads. UI gets `recorder://state` and `recorder://level` events (levels at 10 Hz max).
- `Tracker` runs a 10 s loop on its own thread. It pauses when the screen is locked.
- `TranscribeQueue` is a single worker thread at low OS priority. Jobs survive restarts (status in `meetings`).
- Shared state goes in `tauri::State` behind `Mutex`/`RwLock`. Never hold a lock across an await or a blocking call.

## Security

- Tauri capabilities: the `widget` window can only call recorder commands. The `main` window gets the rest.
- Strict CSP, no remote content in webviews.
- API keys and OAuth tokens in the OS keychain (`keyring`), never in SQLite or settings files.
- LLM prompts include transcript text only, never file paths or other meetings.
