---
name: release-build
description: Use when building TaskMind v2 installers, setting up or fixing the GitHub Actions build matrix, code signing, macOS notarization, the Tauri updater, or preparing a release. Never publishes on its own.
---

# Building and releasing TaskMind v2

## Targets

| OS | Artifacts | Runner |
|---|---|---|
| macOS | `.dmg` + `.app.tar.gz` (updater) for `aarch64-apple-darwin` and `x86_64-apple-darwin` | `macos-latest` |
| Windows | `.msi` and NSIS `.exe` for `x86_64-pc-windows-msvc` | `windows-latest` |
| Linux | `.deb` + `.AppImage` for x86_64 | `ubuntu-22.04` (glibc 2.35 baseline, do not use a newer image) |

Use `tauri-apps/tauri-action` in `.github/workflows/release.yml`. CI on pull requests builds unsigned artifacts only.

## Linux build deps (CI only, never required from users)

`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev libxcb1-dev libxcb-screensaver0-dev cmake patchelf`. Whisper and Opus build from source through their crates (needs `cmake` and `clang` on runners).

## Signing (secrets set by the user in GitHub, never in the repo)

- macOS: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`. Hardened runtime on. Entitlements: `com.apple.security.device.audio-input`.
- Windows: Azure Trusted Signing or a certificate, configured in `bundle.windows`. Without signing, SmartScreen warns users. Note this in release notes.
- Updater: `TAURI_SIGNING_PRIVATE_KEY` + password. The public key goes in `tauri.conf.json`. Losing the private key breaks updates for every installed copy, so tell the user to back it up.

## Release steps (each push/tag step needs explicit user approval)

1. Bump version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` (keep all three equal).
2. Update `CHANGELOG.md`.
3. Run the `dev-check` skill.
4. Ask the user before tagging `vX.Y.Z` and pushing. The workflow creates a **draft** GitHub release.
5. The user tests the draft installers on each OS and publishes it.

## Install size budget

Installer under 20 MB, without whisper models (models are downloaded on first use). Fail the build review if it grows past 30 MB.
