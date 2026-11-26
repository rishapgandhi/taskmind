---
name: activity-tracking
description: Use when implementing or debugging active-window tracking, idle detection, browser URL capture, project classification, meeting auto-detection or timesheet generation in TaskMind v2 (src-tauri/src/activity/, tracker.rs, classify.rs, timesheet.rs, detect.rs).
---

# Activity tracking in TaskMind v2

## Contract

- `activity/mod.rs`: `trait ActivitySource { fn active(&self) -> Result<Option<WindowInfo>>; fn all_windows(&self) -> Result<Vec<WindowInfo>>; }` and `trait IdleSource { fn idle_seconds(&self) -> Result<u64>; }`.
- `WindowInfo { app_name, window_title, process_path, url: Option<String> }`.
- `tracker.rs` samples every 10 s (v1 cadence). It extends the current row when app + title + url are unchanged instead of inserting a new one. Idle threshold default 5 minutes, configurable.
- Apply the blocklist (apps, title patterns, URL domains) **before** writing. Blocked samples are stored as "Private" with no title or URL.
- `all_windows()` is only for meeting detection (`detect.rs`), never stored.

## Per OS

- **macOS:** frontmost app from `NSWorkspace`. Window title needs the Accessibility permission (`AXUIElementCopyAttributeValue` on the focused window). Without permission, store the app name only and show a banner. Idle from IOKit `HIDIdleTime`.
- **Windows:** `GetForegroundWindow`, `GetWindowTextW`, `GetWindowThreadProcessId` → process image name. Idle from `GetLastInputInfo`.
- **Linux X11:** `x11rb`, root `_NET_ACTIVE_WINDOW`, `_NET_WM_NAME`, `WM_CLASS`. Idle via XScreenSaver extension.
- **Linux GNOME Wayland:** Window Calls extension (`org.gnome.Shell.Extensions.Windows`, methods `List`, `GetTitle`) via `zbus`, same as v1 (`../reference/taskmind-v1/taskmind/capture/window_tracker.py`). Idle via `org.gnome.Mutter.IdleMonitor`. If the extension is missing, guide the user to install it and log app-less samples.
- Other Wayland compositors: return `Ok(None)` and show "window tracking unavailable" in Settings. Do not crash.

## Classification and timesheets

- Port rules and merge behavior from `../reference/taskmind-v1/taskmind/processing/classifier.py` and `timesheet.py`. Write table-driven tests from v1 behavior before changing anything.
- Rule priority: explicit priority, then rule kind order url > window_title > git_branch > directory > app_name.

## Meeting detection

- Default keywords from v1: `Meet -`, `Zoom Meeting`, `Microsoft Teams`, `meet.google.com`, `Huddle`. Make them editable.
- Require the match to persist for 2 samples before acting, and to be gone for 3 samples before auto-stop, so a brief tab switch does not split a meeting.
- Default mode is "ask" (notification with Record button). Never auto-record unless the user chose it.

## Day boundaries

Store UTC. Group by the user's local date. v1 had a UTC/local bug in the dashboard, so add a test across midnight and across a DST change.
