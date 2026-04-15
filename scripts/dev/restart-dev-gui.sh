#!/bin/zsh
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
project_path="${root_dir}/dmux.xcodeproj"
scheme="dmux"
configuration="Debug"
native_arch="${DMUX_ARCHS:-$(uname -m)}"
build_dir="${root_dir}/.xcode-dev"
build_products_dir="${build_dir}/Build/Products/${configuration}"
built_app_dir="${build_products_dir}/Codux.app"
dev_apps_dir="${HOME}/Applications"
app_dir="${dev_apps_dir}/Codux-dev.app"
main_plist_path="${app_dir}/Contents/Info.plist"
main_icns_path="${app_dir}/Contents/Resources/AppIcon.icns"
iconset_dir="${build_dir}/AppIcon.iconset"
icon_generator_bin="$(mktemp "${TMPDIR:-/tmp}/codux-dev-generate-app-icon.XXXXXX")"

cleanup() {
  rm -f "${icon_generator_bin}"
}
trap cleanup EXIT

require_command() {
  local command_name="$1"
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    print -u2 -- "[dev] missing required command: ${command_name}"
    exit 1
  fi
}

ensure_metal_toolchain() {
  if ! xcrun metal -help >/dev/null 2>&1; then
    print -u2 -- "[dev] Metal Toolchain is unavailable."
    print -u2 -- "[dev] Install it with: xcodebuild -downloadComponent MetalToolchain"
    exit 1
  fi
}

build_app() {
  rm -rf "${build_dir}"
  mkdir -p "${build_products_dir}"

  xcodebuild \
    -project "${project_path}" \
    -scheme "${scheme}" \
    -configuration "${configuration}" \
    -derivedDataPath "${build_dir}" \
    -destination "platform=macOS" \
    ARCHS="${native_arch}" \
    ONLY_ACTIVE_ARCH=YES \
    CONFIGURATION_BUILD_DIR="${build_products_dir}" \
    CODE_SIGNING_ALLOWED=NO \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGN_IDENTITY="" \
    build

  if [[ ! -d "${built_app_dir}" ]]; then
    print -u2 -- "[dev] expected built app not found: ${built_app_dir}"
    exit 1
  fi
}

install_dev_bundle() {
  pkill -x Codux >/dev/null 2>&1 || true
  mkdir -p "${dev_apps_dir}"
  rm -rf "${app_dir}"
  cp -R "${built_app_dir}" "${app_dir}"
}

update_dev_metadata() {
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier com.duxweb.codux.dev" "${main_plist_path}"
  /usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName Codux-dev" "${main_plist_path}" >/dev/null 2>&1 || true
  /usr/libexec/PlistBuddy -c "Set :CFBundleName Codux-dev" "${main_plist_path}" >/dev/null 2>&1 || true
}

generate_app_icons() {
  rm -rf "${iconset_dir}"
  mkdir -p "$(dirname "${main_icns_path}")"
  swiftc -framework AppKit "${root_dir}/scripts/release/generate-app-icon.swift" -o "${icon_generator_bin}"
  "${icon_generator_bin}" "${iconset_dir}" >/dev/null
  iconutil -c icns "${iconset_dir}" -o "${main_icns_path}"
  rm -rf "${iconset_dir}"
}

sign_and_launch() {
  codesign --force --deep --sign - --timestamp=none "${app_dir}" >/dev/null
  open -n "${app_dir}"
}

require_command xcodebuild
require_command xcrun
require_command swiftc
require_command iconutil
require_command codesign
require_command open

if [[ ! -d "${project_path}" ]]; then
  print -u2 -- "[dev] missing Xcode project: ${project_path}"
  exit 1
fi

ensure_metal_toolchain
build_app
install_dev_bundle
update_dev_metadata
generate_app_icons
sign_and_launch
