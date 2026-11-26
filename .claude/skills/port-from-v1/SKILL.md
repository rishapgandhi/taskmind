---
name: port-from-v1
description: Use when porting a feature, rule, export format or data from TaskMind v1 (Python, ../reference/taskmind-v1/) to v2 (Rust/Svelte), or when writing the v1 data importer.
---

# Porting a v1 feature to v2

1. Find the row for the feature in `docs/PORTING_FROM_V1.md` and read the v1 file in `../reference/taskmind-v1/`. Do not edit anything under `../reference/`.
2. Write down v1's observable behavior (inputs, outputs, edge cases) as Rust test cases first. Include v1's tests from `../reference/taskmind-v1/tests/test_core.py` that still apply.
3. Note v1 bugs you find and decide per bug: keep for compatibility (exports, import) or fix (everything else). List fixes in the PR description.
4. Implement in the v2 module named in the table, behind the module's trait if it touches the OS.
5. Replace every v1 shell-out (`pactl`, `parecord`, `xdotool`, `xprintidle`, `wmctrl`, `ffmpeg`, `notify-send`, `wget`) with an API call or a crate. A shell-out to a tool that may not be installed is never acceptable in v2.
6. Update the table row in `docs/PORTING_FROM_V1.md` with the status, and tick the item in `docs/PLAN.md`.

## Importer (`sync/import_v1.rs`)

- Open the v1 DB read-only. Never modify v1 files.
- Map: `activities` → `activities` (merge consecutive 10 s rows), `recordings` → `meetings` + one segment per transcript (no timestamps in v1), `git_events` → `git_events`, `projects.yaml` → `projects` + `project_rules`, `config.yaml` → `settings`.
- Be idempotent: record the import in `settings` (`import.v1.done_at`) and skip rows already imported.
- Leave audio files where they are and point `audio_path` at them. Offer a "copy into v2 folder" option.
