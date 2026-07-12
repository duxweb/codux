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
PLATFORM_NAME="${PLATFORM_NAME:-macosx}"
ARCHS="${ARCHS:-arm64}"
CURRENT_ARCH="${CURRENT_ARCH:-}"
NATIVE_ARCH_ACTUAL="${NATIVE_ARCH_ACTUAL:-}"
CONFIGURATION="${CONFIGURATION:-Release}"

PROFILE_FLAG="--release"
TARGET_DIR="release"
if [[ "$CONFIGURATION" == "Debug" ]]; then
  PROFILE_FLAG=""
  TARGET_DIR="debug"
fi

select_apple_arch() {
  if [[ -n "$CURRENT_ARCH" && "$CURRENT_ARCH" != "undefined_arch" ]]; then
    printf '%s\n' "$CURRENT_ARCH"
    return 0
  fi
  if [[ -n "$NATIVE_ARCH_ACTUAL" && "$NATIVE_ARCH_ACTUAL" != "undefined_arch" ]]; then
    printf '%s\n' "$NATIVE_ARCH_ACTUAL"
    return 0
  fi
  case " $ARCHS " in
    *" arm64 "*) printf '%s\n' "arm64" ;;
    *" x86_64 "*) printf '%s\n' "x86_64" ;;
    *) printf '%s\n' "$(uname -m)" ;;
  esac
}

APPLE_ARCH="$(select_apple_arch)"

case "$PLATFORM_NAME" in
  iphoneos)
    TARGET="aarch64-apple-ios"
    OUT_DIR="$PLUGIN_DIR/ios/Frameworks"
    export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
    ;;
  iphonesimulator)
    if [[ "$APPLE_ARCH" == "x86_64" ]]; then
      TARGET="x86_64-apple-ios"
    else
      TARGET="aarch64-apple-ios-sim"
    fi
    OUT_DIR="$PLUGIN_DIR/ios/Frameworks"
    export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
    ;;
  macosx)
    if [[ "$APPLE_ARCH" == "x86_64" ]]; then
      TARGET="x86_64-apple-darwin"
    else
      TARGET="aarch64-apple-darwin"
    fi
    OUT_DIR="$PLUGIN_DIR/macos/Frameworks"
    export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
    ;;
  *)
    echo "Unsupported Apple platform: $PLATFORM_NAME" >&2
    exit 2
    ;;
esac

cd "$REPO_ROOT"
rustup target add "$TARGET" >/dev/null
cargo build -p codux-protocol-ffi --target "$TARGET" $PROFILE_FLAG
mkdir -p "$OUT_DIR"
cp "$REPO_ROOT/target/$TARGET/$TARGET_DIR/libcodux_protocol_ffi.a" \
  "$OUT_DIR/libcodux_protocol_ffi.a"
