# Changelog

## 2.0.0-alpha.3

- Transcription: new "Mixed / Hinglish → English text" language option; language detected per 5-minute part instead of once; repetition loops and punctuation-only lines removed; `small` model recommended on 12 GB+ RAM.
- AI notes: due dates are kept only if they were said in the meeting; clearer summary instructions; notes status, retry and failure reason on every meeting; Test connection button in Settings.
- Meetings: filter by day or show all meetings grouped by day.
- Release: macOS builds no longer fail when signing secrets are not set.

## 2.0.0-alpha.1 / alpha.2

First testable build of TaskMind v2, a rewrite of v1 for macOS, Windows and Linux.

- Meeting recording without a bot: microphone and system audio, compressed to Opus (about 11 MB per hour), floating recorder with level meters, tray and global shortcuts.
- Local transcription with Whisper (model downloaded once, verified), "You" vs "Others" labels, synced playback, full-text search.
- Optional AI notes (summary, decisions, action items) with a local model (Ollama), any OpenAI-compatible server, or Claude.
- Work tracking: active app and window every 10 seconds, idle detection, block-list, optional browser URLs, project rules.
- Today timeline mixing work and meetings, week and month reports, editable timesheets, CSV and JSON export.
- Meeting detection from window titles (ask, auto-record, or off).
- Recordings folder choice, audio retention, backup and restore, import from TaskMind v1.
