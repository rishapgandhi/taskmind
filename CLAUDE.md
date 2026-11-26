# TaskMind v2

A free, open-source (MIT) desktop app for Mac, Windows and Linux that answers one question: **"What did I do today?"**

It combines two things in one local timeline:
1. **Work tracking:** active app and window, project classification, git commits, browser URLs, idle time, auto timesheets.
2. **Meeting capture without a bot:** records mic + system audio on the user's own machine, compresses it to Opus, transcribes it locally, and writes notes and action items.

v2 is a rewrite of TaskMind v1 (Python, Linux only). v1 source is in `../reference/taskmind-v1/` (read-only, never edit). Port the logic, not the code. See `docs/PORTING_FROM_V1.md`.

## Key documents

- `docs/PLAN.md`: milestones, scope and exit criteria. Check which milestone is active before starting work.
- `docs/ARCHITECTURE.md`: stack, modules, data model, per-OS strategy.
- `docs/PORTING_FROM_V1.md`: how each v1 module maps to v2.
- `docs/decisions/`: architecture decision records (ADRs). Add one for any decision that is hard to reverse.

## Stack

- Shell: Tauri 2 (Rust). Windows: `main` (history UI) and `widget` (small floating recorder, always on top, frameless).
- Frontend: Svelte 5 + TypeScript + Vite, in `src/`. Package manager: pnpm.
- Backend: Rust in `src-tauri/`. One crate with modules until there is a real reason to split into a workspace.
- Storage: SQLite (`rusqlite`, bundled, FTS5) in the Tauri app data dir. Audio files in a user-chosen folder (default: app data dir `recordings/`).
- Transcription: whisper.cpp through `whisper-rs`, models downloaded on first use.
- Audio encoding: Opus in Ogg, 32 kbps mono, done in-process. No ffmpeg.
- App identifier: `io.github.rishapgandhi.taskmind`.

## Commands

Rust is installed through rustup. If `cargo` is not found, run `source ~/.cargo/env` first.

```bash
pnpm install                     # install frontend deps
pnpm tauri dev                   # run the app in dev mode
pnpm check                       # svelte-check + tsc
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build                 # build installers for the current OS
```

Cross-platform checks from macOS (see the `dev-check` skill):

```bash
./scripts/check-windows.sh   # clippy for x86_64-pc-windows-gnu (needs mingw-w64)
./scripts/check-linux.sh     # clippy + tests in the Lima VM "tm" (Ubuntu)
```

Use the `dev-check` skill before saying any change is done.

## Source map (src-tauri/src)

| Module | What it does |
|---|---|
| `audio/` | cpal capture (mic + system), resample, mix, Ogg Opus encode/decode, recording session thread |
| `recorder/` | state machine + `dispatch` (meeting rows, events, tray, widget) |
| `transcribe/` | Whisper models (verified download), engine (chunks, speaker labels), background queue |
| `notes/` | AI notes providers (Ollama, OpenAI-compatible, Claude), keychain keys |
| `activity/` | active window + idle per OS, browser URL lookup |
| `tracker.rs`, `detect.rs` | 10 s sampling loop, block-list, meeting detection |
| `classify.rs`, `timesheet.rs` | project rules, day reports, blocks, timesheets, CSV |
| `storage.rs`, `import_v1.rs` | recordings folder, retention, backup/restore, v1 import |
| `db/` | SQLite access split by area. Migrations in `src-tauri/migrations/` |
| `commands/` | Tauri commands by area: `mod.rs` (recorder, settings), `meetings`, `transcribe`, `notes`, `work`, `data` |
| `settings.rs` | every setting key the UI may use (allow-list) |

Frontend: `src/lib/api.ts` (all backend calls and types), `src/routes/(app)/` (main window pages), `src/routes/widget/` (recorder).

## Product principles (non-negotiable, carried over from v1)

1. **Lightweight.** Idle RAM under 150 MB, CPU under 2% while tracking. Transcription is the only heavy job: run it in the background, at low priority, one meeting at a time, and let the user pause it. Must work on 4 GB RAM with no GPU.
2. **Local and offline by default.** No telemetry ever. The network is used only for: model downloads, and features the user turns on (cloud upload, cloud LLM notes, update checks). Every network feature is opt-in and visible in Settings.
3. **Privacy.** Never take screenshots. Store window titles and URLs only. Honor the app/URL blocklist before anything is written to disk. All data deletable from the UI.
4. **One-click install.** Signed installers (.dmg, .msi/.exe, .deb, AppImage). No terminal, no extra system packages, no Python, no ffmpeg.
5. **Consent.** Show a visible recording indicator at all times (widget + tray). Offer a configurable "remind me to tell participants" prompt when recording starts. Never record in hidden mode.
6. **Linux is not second class.** v1 users are on Ubuntu with GNOME Wayland. Do not break Linux to ship Mac or Windows.

## Code conventions

- Rust: `thiserror` for module errors, `anyhow` only at command boundaries. No `unwrap()`/`expect()` outside tests and startup. Use `tracing` for logs, never `println!`.
- OS-specific code lives in `<module>/macos.rs`, `windows.rs`, `linux.rs` behind `#[cfg(target_os = ...)]`, with a shared trait in `<module>/mod.rs`. Callers never use `cfg` themselves.
- Tauri commands live in `src-tauri/src/commands/` and stay thin: validate input, call a module, map errors to `CommandError` (`src-tauri/src/error.rs`). Every new command must also be added to `COMMANDS` in `src-tauri/build.rs` and granted in `src-tauri/capabilities/main.json` (and `widget.json` only if the widget needs it).
- Recorder actions from any source (command, tray, shortcut) go through `recorder::dispatch` so events, tray text and the widget stay in sync.
- Long work (recording, transcription, tracking loop) runs on dedicated threads or tokio tasks and reports progress through Tauri events, never by blocking a command.
- DB changes only through numbered migrations in `src-tauri/migrations/`. Never edit a shipped migration.
- Frontend: Svelte 5 runes, TypeScript strict, typed wrappers around `invoke()` in `src/lib/api.ts`. No UI framework heavier than what is in `package.json` without an ADR.
- Tests: unit tests next to the code. Platform code gets a fake implementation of its trait so logic can be tested on any OS.
- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`), as in v1.

## Reusing third-party code

Meetily (https://github.com/Zackriya-Solutions/meetily) is a Tauri + Rust meeting recorder. Its Community Edition is reported as MIT. Before copying any of its code, read its LICENSE file in the exact version you copy from, keep the copyright notice, and add an entry to `THIRD_PARTY_NOTICES.md`. Do not copy code from projects with GPL/AGPL licenses.

## Things to avoid

- Do not create, push to, or rename GitHub repos, or publish releases, without explicit approval from the user.
- Do not add a dependency that needs a system package on the user's machine.
- Do not commit `.kiro/`, personal notes, internal paths, tokens or Drive folder IDs. v1 leaked some of these in `.kiro/knowledge.md`.
