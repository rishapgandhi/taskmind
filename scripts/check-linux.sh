#!/usr/bin/env bash
# Lint and test the Rust backend on Linux inside the Lima VM "tm" (see docs/DEVELOPMENT.md).
# The source is copied into the VM first, because the shared folder is read-only there.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
limactl shell tm -- bash -lc "set -e; . ~/.cargo/env
  rsync -a --delete --exclude target --exclude node_modules --exclude .svelte-kit --exclude build '$ROOT/' \$HOME/taskmind/
  cd \$HOME/taskmind/src-tauri
  cargo clippy --all-targets -- -D warnings
  cargo test"
