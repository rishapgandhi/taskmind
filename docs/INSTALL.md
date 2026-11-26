# Install and run TaskMind v2 (testers)

There are two ways to get TaskMind:

1. **Installer from GitHub (easiest).** When the code is on GitHub, every push builds installers for all three systems (Actions → CI → the run → Artifacts). Download the one for your system:
   - Windows: `.msi` or `-setup.exe`
   - Ubuntu/Debian: `.deb` (or `.AppImage` for other distributions)
   - macOS: `.dmg`
2. **Build from source.** Follow the section for your system below. Takes 15–30 minutes the first time.

The test builds are **not signed** yet, so the system warns you:
- **Windows SmartScreen:** "Windows protected your PC" → More info → Run anyway.
- **macOS:** right-click the app → Open → Open. If it still refuses: `xattr -dr com.apple.quarantine /Applications/TaskMind.app`
- **Ubuntu:** `sudo apt install ./TaskMind_2.0.0_amd64.deb`. For the AppImage, run `chmod +x` on it first.

---

## Build from source: Windows 10/11

1. Install the tools (PowerShell):
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   winget install Rustlang.Rustup Kitware.CMake LLVM.LLVM OpenJS.NodeJS.LTS Git.Git
   ```
   Close and reopen the terminal, then:
   ```powershell
   rustup default stable
   npm install -g pnpm
   ```
   WebView2 is already part of Windows 11. On Windows 10, install "WebView2 Runtime" from Microsoft if the app window stays blank.
2. Get the code and run it:
   ```powershell
   git clone <repo-url> taskmind
   cd taskmind        # the folder that contains package.json
   pnpm install
   pnpm tauri dev     # first build takes a while
   ```
3. Optional installer: `pnpm tauri build`. The files appear in `src-tauri\target\release\bundle\msi\` and `nsis\`.

## Build from source: Ubuntu 22.04 / 24.04 (and Debian-based)

1. Install the tools:
   ```bash
   sudo apt update
   sudo apt install -y build-essential curl wget file git cmake clang pkg-config patchelf \
     libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev \
     libasound2-dev libxcb1-dev libxcb-screensaver0-dev libxdo-dev
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   source ~/.cargo/env
   curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash - && sudo apt install -y nodejs
   sudo npm install -g pnpm
   ```
2. Get the code and run it:
   ```bash
   git clone <repo-url> taskmind && cd taskmind
   pnpm install
   pnpm tauri dev
   ```
3. Optional installer: `pnpm tauri build` produces `src-tauri/target/release/bundle/deb/*.deb` and `appimage/*.AppImage`.

**GNOME on Wayland (Ubuntu's default):** to record window titles, install the **Window Calls** extension (https://extensions.gnome.org/extension/4724/window-calls/), then log out and back in. Without it, TaskMind shows a hint and records meetings only. Global shortcuts may not work on Wayland. Use the tray icon or the small recorder window instead.

Recording needs PulseAudio or PipeWire (default on Ubuntu). Nothing else to install.

## Build from source: macOS 14.6+

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
brew install cmake node && npm install -g pnpm
git clone <repo-url> taskmind && cd taskmind
pnpm install && pnpm tauri dev
```

The first recording asks for **Microphone** and **System Audio Recording** permission. Work tracking asks for **Accessibility** (for window titles). In `pnpm tauri dev`, macOS shows these for your terminal app, not TaskMind.

---

## After it starts

- The small recorder window appears top-right. The main window shows **Today** with a welcome checklist.
- **Settings → Transcription → Download** the recommended model (one time, about 150 MB).
- **Record:** `Ctrl+Shift+Alt+R` (Windows/Linux), `Cmd+Shift+Option+R` (macOS), or ● in the recorder.
- Closing a window only hides it. Quit from the tray icon.

Data locations:
| OS | Database, recordings, models |
|---|---|
| Windows | `%APPDATA%\io.github.rishapgandhi.taskmind\` |
| Ubuntu | `~/.local/share/io.github.rishapgandhi.taskmind/` |
| macOS | `~/Library/Application Support/io.github.rishapgandhi.taskmind/` |
