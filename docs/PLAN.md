# TaskMind v2 Plan

Status (2026-11-27): **all milestones have a first implementation. Alpha testing on Windows, Ubuntu and macOS is next.** Unchecked items below are known gaps, not forgotten ones.

How things were verified:
- **macOS (arm64):** builds, runs, recording verified on real devices, transcription verified end to end (tiny model).
- **Windows:** Rust code compile- and lint-checked from macOS (`scripts/check-windows.sh`). Never run on a real Windows machine yet.
- **Linux:** Rust code built, linted and unit-tested in an Ubuntu 24.04 arm64 VM (`scripts/check-linux.sh`). Never run with a desktop session yet.
- Unit tests: 49 (plus 2 hardware tests run by hand). `pnpm check`: 0 errors.

## Goal

One installable, open-source app on Mac, Windows and Linux that shows a person's whole workday on one timeline: what they worked on, and every meeting they were in, with audio, transcript and notes. No bots, no subscription, data stays on the machine.

## Out of scope for v2.0

Joining meetings as a bot, any TaskMind-hosted backend or accounts, team dashboards, mobile apps, screen or video recording.

## M0: Scaffold and CI ✅
- [x] Tauri 2 + Svelte 5 + TypeScript app in `v2/`, identifier `io.github.rishapgandhi.taskmind`
- [x] Two windows: `main` and `widget` (frameless, always on top)
- [x] Tray icon: Open, Show/Hide recorder, Start/Stop recording, Quit
- [x] Global shortcuts `Cmd/Ctrl+Shift+Alt+R` (record) and `+T` (open)
- [x] Autostart toggle, first-run welcome checklist
- [x] SQLite with migrations (3 so far), FTS5
- [x] GitHub Actions CI (lint, tests, unsigned builds for 4 targets). Not run yet: the code is not on GitHub
- [ ] Let users change the global shortcuts

## M1: Recording core ✅ (macOS verified)
- [x] Capture through cpal on all three OS (ADR 0002): Core Audio tap (macOS 14.6+), WASAPI loopback, PulseAudio/PipeWire monitor
- [x] Mixed mono Opus 32 kbps (about 11 MB/hour measured) + separate mic and system tracks
- [x] Crash safety (pages flushed every second, recovery on start), clean stop on quit
- [x] Widget: timer, level meters, pause, stop, warnings, consent reminder
- [ ] macOS 13.0–14.5: ScreenCaptureKit fallback (mic-only today, with a warning)
- [ ] macOS: output devices that are also inputs (some headsets) are refused for system audio
- [ ] Echo cancellation (recommend headphones until then)
- [ ] Follow the default device when it changes mid-call
- [ ] 60-minute real-call test on each OS

## M2: Transcription and meeting history ✅
- [x] Model catalog (tiny, base, small, large-v3-turbo), download with SHA-256 check, delete. Default by RAM
- [x] Background queue: one at a time, half the cores, resumable, pausable, survives restarts
- [x] 5-minute chunks (bounded memory), language auto-detect or fixed, noise markers dropped
- [x] "You" / "Others" from comparing the two tracks
- [x] Meeting page: synced transcript, click to seek, rename, delete, transcribe again
- [x] Full-text search with highlighted snippets
- [ ] Benchmark: 60-minute meeting with `base` on a 4-core laptop

## M3: Activity tracking ✅ (macOS verified, Windows/Linux untested at runtime)
- [x] macOS (NSWorkspace + Accessibility), Windows (Win32), Linux X11 (x11rb), GNOME Wayland (Window Calls D-Bus) + idle on each
- [x] 10-second sampling merged into rows, idle trimmed back to the last input
- [x] Block-list applied before storage, optional browser URLs from history (v1 approach, opt-in)
- [x] Project rules (title, URL, app, folder, branch) with reclassification of the last 30 days
- [x] Projects page
- [ ] Git commit watcher (v1 had one)
- [ ] Accuracy check: a full workday within 5% of hand-measured time

## M4: Timeline and reports ✅
- [x] Today: totals, project bar, timeline mixing work blocks and meetings, top apps, live "Now"
- [x] Reports: week and month bars, totals per project
- [x] Timesheet: generated drafts, edit, save, approve, regenerate
- [x] Export CSV (v1 columns) and JSON to Downloads
- [x] Delete a day's activity
- [ ] Search across window titles and URLs (transcripts only today)

## M5: Meeting detection and AI notes ✅
- [x] Detection from window titles with v1 keywords (editable): ask / auto-record and auto-stop / off
- [x] Notes providers: Ollama (local), OpenAI-compatible, Claude (`claude-opus-5-5`, refusal fallback on). Keys in the OS keychain
- [x] Summary, decisions, action items. Editable, tick items done, chunked for long meetings on small models
- [x] Auto-generate after transcription (on by default once a provider is chosen)
- [x] Today shows action items from that day's meetings
- [ ] "Mic in use" signal for detection (titles only today)

## M6: Storage, sync and migration ✅ (partly)
- [x] Choose the recordings folder (works with Google Drive / OneDrive / Dropbox desktop sync)
- [x] Retention: delete audio after N days, keep transcripts and notes
- [x] Backup (zip in Downloads) and restore (applied on restart, old database kept)
- [x] Import from v1 (database + projects.yaml), once
- [ ] Direct Google Drive / OneDrive upload: needs OAuth apps registered by the project owner (see ADR 0003)
- [ ] Push timesheets to Clockify / Toggl / Jira Tempo (v1 had this): planned with keychain-stored tokens

## M7: Release ✅ (pipeline only)
- [x] `release.yml`: on a `v*` tag, builds all installers into a **draft** GitHub release. Signing secrets optional
- [x] CHANGELOG, install guide, testing checklist, bug report template
- [ ] Code signing (Apple Developer account, Windows certificate)
- [ ] Auto-updater (needs an update signing key the owner generates and keeps; see release-build skill)
- [ ] Docs website (reuse v1 GitHub Pages)

## Open decisions (owner: user)
1. Push `v2/` as a `v2` branch of the `taskmind` repo (recommended) or a new repo. CI builds installers for testers once it is on GitHub.
2. Code-signing budget (Apple about USD 99/year; Windows certificate or Azure Trusted Signing).
3. Whether to register Google/Microsoft OAuth apps for direct Drive/OneDrive upload.
4. Name: keep "TaskMind".

## Risks
| Risk | Mitigation |
|---|---|
| Windows/Linux code never run on real hardware | Alpha testers on both, `docs/TESTING.md` checklist |
| Echo on speakers | Separate tracks keep labels right. AEC planned. Recommend headphones |
| GNOME Wayland blocks titles and shortcuts | Window Calls extension hint, tray/widget controls |
| Slow transcription on low-end PCs | `tiny` model by RAM, pause option, background queue |
| Recording consent laws | Visible indicator, consent reminder |
