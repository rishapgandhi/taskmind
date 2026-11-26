# ADR 0001: Rewrite v2 on Tauri + Rust

Date: 2026-11-27
Status: Accepted

## Context

TaskMind v1 is Python and works only on Linux. It calls shell tools (`pactl`, `parecord`, `xdotool`, `ffmpeg`). v2 must install with one click on Mac, Windows and Linux and capture system audio on each.

## Decision

Build v2 as a Tauri 2 app with a Rust backend and a Svelte 5 + TypeScript frontend. Develop it in `v2/` and publish it later as a `v2` branch of the existing `taskmind` repo (final placement pending user confirmation).

## Reasons

- System audio needs native APIs (Core Audio taps / ScreenCaptureKit, WASAPI, PulseAudio). Rust reaches all of them directly. Python cannot do macOS cleanly.
- Tauri installers are small (about 10 MB before models) and come with tray, global shortcut, autostart and updater plugins.
- whisper.cpp and Opus can run in-process, so users install nothing else.
- Meetily, a working MIT-licensed Tauri + Rust meeting recorder, shows the approach works and may provide reusable audio code.

## Alternatives considered

- **Keep Python + PyInstaller:** weak macOS audio, large installers, hard signing. Rejected.
- **Electron:** large installers and RAM use, against the lightweight principle. Native audio would still need addons. Rejected.
- **Contribute to Meetily only:** fastest for meetings, but does not cover activity tracking and timesheets, which is TaskMind's core. Reuse its code instead.

## Consequences

- v1 code under `../reference/` is never edited. All v1 code is rewritten. v1 stays maintained for bug fixes until v2.0 ships.
- Contributors need Rust. Frontend work stays in TypeScript.
