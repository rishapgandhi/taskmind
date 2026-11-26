---
name: dev-check
description: Use before saying any TaskMind v2 change is done, before committing, or when asked to verify, lint or test the v2 app. Runs formatting, lints, type checks, tests and (when UI or native code changed) a dev launch.
---

# Verify a TaskMind v2 change

Run from `v2/`. Stop at the first failure, fix it, and rerun from that step.

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm check
pnpm test --run        # only if a frontend test runner is configured
```

If the change touches Rust code that differs per OS (`audio/`, `activity/`, `cfg(...)`), also run:

```bash
./scripts/check-windows.sh   # Windows compile + clippy from macOS
./scripts/check-linux.sh     # Linux clippy + tests in the Lima VM "tm" (start it with `limactl start tm`)
```

If the change touches UI, Tauri config, capabilities, tray, shortcuts or OS-specific code, also run `pnpm tauri dev` and confirm the app starts without errors in the terminal. For UI changes, open the affected view and check it. For audio, record 30 seconds with music playing and your voice, then play the file back.

## Reporting

- Say which commands ran and their result. If something was skipped (for example, a Windows-only path tested on macOS), say so and name what still needs testing on which OS.
- Never report "tests pass" for code that only compiled.

## Before committing

- Conventional Commit message (`feat:`, `fix:`, `docs:` ...).
- `docs/PLAN.md` checkboxes updated if a plan item is complete.
- No secrets, tokens, personal paths, or `.kiro/` files staged.
- Commit only when the user asked. Never push or open PRs without approval.
