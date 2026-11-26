# ADR 0003: AI notes providers, keychain, and cloud sync through synced folders

Date: 2026-11-27
Status: Accepted

## Decisions

1. **AI notes are off by default** and need an explicit provider choice: Ollama (local, nothing leaves the machine), any OpenAI-compatible server, or Claude through the Anthropic Messages API (`claude-opus-5-5`, structured JSON output, server-side refusal fallback). Raw HTTP with `reqwest` is used because there is no official Rust SDK.
2. **API keys live in the OS keychain** (`keyring` crate: macOS Keychain, Windows Credential Manager, Secret Service on Linux), never in SQLite, settings or logs.
3. **Cloud storage uses a synced folder** (Google Drive, OneDrive or Dropbox desktop app) chosen in Settings, instead of direct API upload in 2.0.

## Why synced folders first

Direct upload needs OAuth client registrations with Google and Microsoft (verification, consent screens, client IDs) that only the project owner can create, and secrets that cannot ship in an open-source binary safely without PKCE plus app verification. Synced folders work today on all three OS with no login inside TaskMind.

## Consequences

- Direct Drive/OneDrive upload stays on the plan (M6) until the owner registers OAuth apps.
- Testers on Linux need a Secret Service provider (GNOME Keyring, default on Ubuntu) to save API keys.
