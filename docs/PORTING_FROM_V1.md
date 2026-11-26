# Porting from v1

v1 source: `../../reference/taskmind-v1/` (read-only copy of https://github.com/rishapgandhi/taskmind, v1.0.3, last commit 2026-06-11).

Port behavior and ideas, not code. v1 calls shell tools (`pactl`, `parecord`, `xdotool`, `xprintidle`, `ffmpeg`). v2 must call OS APIs directly.

Status 2026-11-27: everything below is ported except the git watcher, the CLI, and Clockify/Toggl/Tempo push (see PLAN.md).

| v1 file | What it does | v2 target | Notes |
|---|---|---|---|
| `taskmind/capture/audio_recorder.py` | PulseAudio null sink mixing mic + monitor, `parecord` to WAV, ffmpeg to Opus | `audio/linux.rs`, `audio/mixer.rs`, `audio/encoder.rs` | Fix v1 bugs: picks the first `.monitor` source (may be the wrong device; use the default sink's monitor). ffmpeg missing means a silent fallback to huge WAV. Loopback modules change the user's audio setup (v2 must not load PulseAudio modules). |
| `taskmind/processing/transcriber.py` | Shells out to whisper.cpp, `tiny` model, no timestamps | `transcribe/` | Use `whisper-rs` in-process, keep timestamps, checksum the model downloads |
| `taskmind/capture/window_tracker.py` | xdotool on X11, Window Calls D-Bus on GNOME Wayland | `activity/linux_x11.rs`, `activity/linux_gnome.rs` | Keep the Window Calls approach and its install helper |
| `taskmind/capture/idle_detector.py` | `xprintidle` | `activity/idle.rs` | X11 XScreenSaver + Mutter IdleMonitor D-Bus |
| `taskmind/capture/browser_url.py` | Browser URL capture | `activity/` (url field) | Check how v1 reads URLs before choosing an approach per OS |
| `taskmind/capture/git_watcher.py` | Watches repos for commits | `gitwatch.rs` | Read `.git` with `gix` or shell `git log`. Decide in M3 |
| `taskmind/processing/classifier.py` | YAML rule matching | `classify.rs` + `project_rules` table | Same matcher kinds plus `url` |
| `taskmind/processing/timesheet.py` | Merges activities into project blocks | `timesheet.rs` | Port the merge rules and write table-driven tests from v1 behavior |
| `taskmind/processing/recap.py` | Daily recap text | `notes/recap` | Add meeting action items |
| `taskmind/processing/export.py` | CSV/JSON/Clockify/Toggl/Jira | `timesheet.rs` exports | Keep output formats byte-compatible where possible |
| `taskmind/daemon.py` | 10 s loop, meeting auto-detect, auto record | `tracker.rs`, `detect.rs` | Same 10 s cadence, same keyword list as default |
| `taskmind/database.py` | SQLite + FTS5 schema | `migrations/0001_init.sql` | New schema; write `sync/import_v1.rs` for upgrade |
| `taskmind/config.py`, `configs/*.yaml` | YAML config | `settings` table + Settings UI | Import v1 YAML on first run if found |
| `taskmind/ui/tray.py` | pystray tray | Tauri tray in `lib.rs` | |
| `taskmind/ui/dashboard/` | FastAPI + vanilla JS on localhost:7890 | Svelte `main` window | Reuse the tab structure: Today, Weekly, Monthly, Timesheet, Search, Recordings, Settings |
| `taskmind/cli.py` | Click CLI | Not in 2.0 | Optional later: small CLI that talks to the running app |
| `install.sh` | apt deps, venv, systemd | Installers + autostart plugin | No system packages |
| `tests/test_core.py` | Core tests | Rust unit tests | Port each case that still applies |

## v1 data locations (for the importer)

- DB: `~/.local/share/taskmind/taskmind.db`
- Recordings: `~/.local/share/taskmind/recordings/`
- Config: `~/.config/taskmind/config.yaml`, `projects.yaml`
