#!/usr/bin/env bash
# Compile-check the Rust backend for Windows from macOS (needs: brew install mingw-w64; rustup target add x86_64-pc-windows-gnu).
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
INC="$(brew --prefix mingw-w64)/toolchain-x86_64/x86_64-w64-mingw32/include"
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnu="--target=x86_64-w64-mingw32 -I$INC"
cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
