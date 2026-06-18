#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PLUGIN_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
find_repo_root() {
  local dir="$PLUGIN_DIR"
  while [[ "$dir" != "/" ]]; do
    if [[ -f "$dir/Cargo.toml" ]] && grep -q '^\[workspace\]' "$dir/Cargo.toml"; then
      printf '%s\n' "$dir"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}
REPO_ROOT="$(find_repo_root)"
SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"

if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  if [[ -d "$SDK_ROOT/ndk" ]]; then
    ANDROID_NDK_HOME="$(find "$SDK_ROOT/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
    export ANDROID_NDK_HOME
  fi
fi

if [[ -z "${ANDROID_NDK_HOME:-}" || ! -d "$ANDROID_NDK_HOME" ]]; then
  echo "ANDROID_NDK_HOME is not set and no NDK was found under $SDK_ROOT/ndk" >&2
  exit 2
fi

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "cargo-ndk is required. Install with: cargo install cargo-ndk" >&2
  exit 2
fi

cd "$REPO_ROOT"
# The engine is alacritty_terminal (pure Rust), linked straight into the FFI
# .so, so no separate native VT library is bundled. Drop any stale runtime lib
# left by the previous native engine.
rm -f \
  "$PLUGIN_DIR/android/src/main/jniLibs/arm64-v8a/libc++_shared.so"

cargo ndk \
  -t arm64-v8a \
  -o "$PLUGIN_DIR/android/src/main/jniLibs" \
  build -p codux-protocol-ffi --release
