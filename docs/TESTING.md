# Testing checklist (alpha)

Please work through this on your system and report anything odd with the bug template (GitHub → Issues → New issue → Bug report), or in a shared sheet with the same fields. One problem per report.

**Which system:** Windows ☐ · Ubuntu X11 ☐ · Ubuntu GNOME Wayland ☐ · macOS ☐

## 1. Install and start
- [ ] App installs (or `pnpm tauri dev` runs) without errors
- [ ] Main window and the small recorder window both appear. The tray icon is visible
- [ ] Closing the main window hides it. Tray → Open TaskMind brings it back
- [ ] Starting the app a second time just focuses the running one

## 2. Recording (most important)
- [ ] Press ● Record: the timer runs and **both** level bars move (green = your mic, purple = computer audio). Play a YouTube video and talk to check
- [ ] The "Tell participants you are recording" reminder shows
- [ ] Pause and Resume work. Paused time is not counted
- [ ] Stop: the meeting appears in **Meetings** with duration and size
- [ ] **Play** works and you hear both your voice and the computer audio
- [ ] A 30-minute real call (Meet/Zoom/Teams): file size about 6 MB, both sides audible. Note whether you used headphones or speakers
- [ ] Global shortcut starts and stops recording (note if it does nothing on your system)
- [ ] Plug in or remove headphones during a recording: note what happens (known limitation: a device switch is not followed yet)

## 3. Transcription
- [ ] Settings → Transcription: download the recommended model. Progress shows and the download finishes
- [ ] Earlier recordings move from "Waiting" to "Transcribing x%" to "Done"
- [ ] Meeting page: transcript lines are labelled **You** / **Others** correctly most of the time
- [ ] Clicking a line jumps the player there. The current line is highlighted while playing
- [ ] Search finds words from the transcript and opens the meeting at that moment
- [ ] Note how long a 30-minute meeting takes to transcribe, plus your CPU and RAM

## 4. AI notes (optional)
- [ ] With Ollama (`ollama pull llama3.2`) or an API key: Settings → AI meeting notes → provider
- [ ] "Write notes with AI" gives a summary, decisions and action items. Edit and tick items done
- [ ] Ticked action items also show on **Today**

## 5. Work tracking
- [ ] **Today** shows "Now: app · window title" and updates within about 10 seconds of switching apps
- [ ] Windows: titles show. Ubuntu Wayland: needs the Window Calls extension. macOS: needs Accessibility
- [ ] After 5+ minutes away, time counts as "Away"
- [ ] Create a project with a rule (Projects). Matching time turns that project's color on Today
- [ ] Block-list (Settings): a listed app shows as "Private" with no title
- [ ] Meeting detection: open a Google Meet / Zoom meeting. After about 20 seconds the recorder asks "Meeting detected. Record it?"

## 6. Reports and timesheet
- [ ] Reports → Week / Month show bars per day. Export CSV opens the file location
- [ ] Timesheet: edit rows, Save draft, Approve, Regenerate

## 7. Data
- [ ] Settings → Back up now creates a zip in Downloads
- [ ] Restore from that backup restarts the app with the data back
- [ ] Change the recordings folder: new recordings go there and still play
- [ ] (Ubuntu with TaskMind v1) "Import it" brings v1 projects, activity and recordings

## How to collect logs
Run TaskMind from a terminal to see its log:
- Source build: logs print in the terminal running `pnpm tauri dev`
- Windows installed: `"C:\Program Files\TaskMind\TaskMind.exe"` from PowerShell
- Ubuntu installed: run `taskmind` in a terminal
- macOS installed: `/Applications/TaskMind.app/Contents/MacOS/taskmind`

Copy lines containing `WARN` or `ERROR` into the report. For more detail, set `RUST_LOG=debug` first. Remove anything private (window titles) before sharing.
