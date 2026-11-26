# TaskMind v2

**What did I do today?** TaskMind tracks your work and records your meetings, then shows both on one timeline. Free, open source (MIT), runs on Mac, Windows and Linux. No meeting bots, no subscription, and your data stays on your computer.

> **Looking for TaskMind v1** (Python, Linux only)? It lives on the [`v1` branch](https://github.com/rishapgandhi/taskmind/tree/v1) and [release v1.0.4](https://github.com/rishapgandhi/taskmind/releases/tag/v1.0.4). v2 can import your v1 data (Settings → Storage and backup).

> Status: **alpha**. All features have a first version; testing on Windows, Ubuntu and macOS is under way. See [docs/PLAN.md](docs/PLAN.md).

## Features

- Records meetings from any app (Zoom, Meet, Teams, Slack huddles) by capturing your mic and computer audio. Nobody sees a bot join.
- Small floating recorder and a global shortcut to start or stop.
- Compressed audio (about 14 MB per hour).
- Local transcription with Whisper. Optional meeting notes and action items with a local or cloud AI model.
- Automatic work tracking: active app, window, project, git commits, idle time.
- Timesheets with CSV, JSON, Jira, Clockify and Toggl export.
- Optional upload to Google Drive or OneDrive.

## Install and test

- [Install / build guide](docs/INSTALL.md) for Windows, Ubuntu and macOS
- [Testing checklist](docs/TESTING.md)

## Develop

Needs Rust (rustup), Node 20+, pnpm and CMake.

```bash
pnpm install
pnpm tauri dev
```

Shortcuts: `Cmd/Ctrl+Shift+Alt+R` start/stop recording, `Cmd/Ctrl+Shift+Alt+T` open TaskMind. Closing a window hides it. Quit from the tray icon.

## Docs

- [Plan](docs/PLAN.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Porting from v1](docs/PORTING_FROM_V1.md)
- [Changelog](CHANGELOG.md)

## License

MIT
