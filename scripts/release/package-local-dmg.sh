#!/bin/bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
project_path="${root_dir}/dmux.xcodeproj"
scheme="dmux"
app_name="Codux"
version="${DMUX_VERSION:-0.1.3}"
package_version="${DMUX_PACKAGE_VERSION:-${version}}"
build_number="${DMUX_BUILD_NUMBER:-1}"
configuration="${DMUX_CONFIGURATION:-Release}"
arch_list="${DMUX_ARCHS:-arm64 x86_64}"
codesign_identity="${APPLE_CODESIGN_IDENTITY:--}"
enable_notarization="${DMUX_NOTARIZE:-0}"
apple_team_id="${APPLE_TEAM_ID:-}"
notary_profile="${APPLE_NOTARY_PROFILE:-}"
notary_api_key_path="${APPLE_API_PRIVATE_KEY_PATH:-}"
notary_api_key_id="${APPLE_API_KEY_ID:-}"
notary_api_issuer_id="${APPLE_API_ISSUER_ID:-}"
dist_dir="${root_dir}/dist"
build_dir="${root_dir}/.xcode-release"
build_products_dir="${build_dir}/Build/Products/${configuration}"
built_app_dir="${build_products_dir}/${app_name}.app"
app_dir="${dist_dir}/${app_name}.app"
package_basename="${app_name}-${package_version}-macos-universal"
dmg_staging_dir="${dist_dir}/dmg"
dmg_path="${dist_dir}/${package_basename}.dmg"
zip_path="${dist_dir}/${package_basename}.zip"
checksums_path="${dist_dir}/SHA256SUMS.txt"
iconset_dir="${dist_dir}/AppIcon.iconset"
main_icns_path="${app_dir}/Contents/Resources/AppIcon.icns"
icon_generator_bin="$(mktemp "${TMPDIR:-/tmp}/dmux-generate-app-icon.XXXXXX")"

cleanup() {
  rm -f "${icon_generator_bin}"
}
trap cleanup EXIT

require_command() {
  local command_name="$1"
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "[package] missing required command: ${command_name}" >&2
    exit 1
  fi
}

ensure_metal_toolchain() {
  if ! xcrun metal -help >/dev/null 2>&1; then
    echo "[package] Metal Toolchain is unavailable." >&2
    echo "[package] Install it with: xcodebuild -downloadComponent MetalToolchain" >&2
    exit 1
  fi
}

generate_app_icons() {
  rm -rf "${iconset_dir}"
  mkdir -p "$(dirname "${main_icns_path}")"
  swiftc -framework AppKit "${root_dir}/scripts/release/generate-app-icon.swift" -o "${icon_generator_bin}"
  "${icon_generator_bin}" "${iconset_dir}" >/dev/null
  iconutil -c icns "${iconset_dir}" -o "${main_icns_path}"
  rm -rf "${iconset_dir}"
}

build_app() {
  rm -rf "${build_dir}" "${app_dir}" "${dmg_staging_dir}" "${dmg_path}" "${zip_path}" "${checksums_path}"
  mkdir -p "${dist_dir}"

  echo "[package] building ${scheme} (${configuration}) for ${arch_list}"
  xcodebuild \
    -project "${project_path}" \
    -scheme "${scheme}" \
    -configuration "${configuration}" \
    -derivedDataPath "${build_dir}" \
    -destination "generic/platform=macOS" \
    ARCHS="${arch_list}" \
    ONLY_ACTIVE_ARCH=NO \
    MARKETING_VERSION="${version}" \
    CURRENT_PROJECT_VERSION="${build_number}" \
    CONFIGURATION_BUILD_DIR="${build_products_dir}" \
    CODE_SIGNING_ALLOWED=NO \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGN_IDENTITY="" \
    build

  if [[ ! -d "${built_app_dir}" ]]; then
    echo "[package] expected built app not found: ${built_app_dir}" >&2
    exit 1
  fi

  cp -R "${built_app_dir}" "${app_dir}"
}

sign_app() {
  if [[ "${codesign_identity}" == "-" ]]; then
    echo "[package] applying ad-hoc signature"
    codesign --force --deep --sign - --timestamp=none "${app_dir}"
  else
    echo "[package] signing app with identity: ${codesign_identity}"
    codesign --force --deep --options runtime --sign "${codesign_identity}" "${app_dir}"
  fi
}

verify_app_signature() {
  echo "[package] verifying bundle signature"
  codesign --verify --deep --strict --verbose=2 "${app_dir}"
}

notarize_app_if_needed() {
  if [[ "${enable_notarization}" != "1" ]]; then
    return
  fi

  if [[ "${codesign_identity}" == "-" ]]; then
    echo "[package] notarization requested but APPLE_CODESIGN_IDENTITY is not set" >&2
    exit 1
  fi

  local submit_args=()
  if [[ -n "${notary_profile}" ]]; then
    submit_args=(--keychain-profile "${notary_profile}")
  elif [[ -n "${notary_api_key_path}" && -n "${notary_api_key_id}" && -n "${notary_api_issuer_id}" ]]; then
    submit_args=(--key "${notary_api_key_path}" --key-id "${notary_api_key_id}" --issuer "${notary_api_issuer_id}")
  else
    echo "[package] notarization requested but no notary credentials were configured" >&2
    exit 1
  fi

  if [[ -n "${apple_team_id}" ]]; then
    submit_args+=(--team-id "${apple_team_id}")
  fi

  echo "[package] submitting app for notarization"
  xcrun notarytool submit "${app_dir}" --wait "${submit_args[@]}"

  echo "[package] stapling notarization ticket"
  xcrun stapler staple "${app_dir}"
}

create_release_artifacts() {
  echo "[package] preparing dmg staging"
  mkdir -p "${dmg_staging_dir}"
  cp -R "${app_dir}" "${dmg_staging_dir}/"
  ln -s /Applications "${dmg_staging_dir}/Applications"

  echo "[package] creating dmg at ${dmg_path}"
  hdiutil create \
    -volname "${app_name}" \
    -srcfolder "${dmg_staging_dir}" \
    -ov \
    -format UDZO \
    "${dmg_path}" >/dev/null
  rm -rf "${dmg_staging_dir}"

  echo "[package] creating zip at ${zip_path}"
  ditto -c -k --sequesterRsrc --keepParent "${app_dir}" "${zip_path}"

  echo "[package] writing checksums to ${checksums_path}"
  (
    cd "${dist_dir}"
    shasum -a 256 "$(basename "${dmg_path}")" "$(basename "${zip_path}")" > "$(basename "${checksums_path}")"
  )
}

require_command xcodebuild
require_command xcrun
require_command swiftc
require_command iconutil
require_command codesign
require_command ditto
require_command hdiutil
require_command shasum

if [[ ! -d "${project_path}" ]]; then
  echo "[package] missing Xcode project: ${project_path}" >&2
  exit 1
fi

ensure_metal_toolchain
build_app
generate_app_icons
sign_app
verify_app_signature
notarize_app_if_needed
create_release_artifacts

echo "[package] done"
echo "APP=${app_dir}"
echo "DMG=${dmg_path}"
echo "ZIP=${zip_path}"
echo "CHECKSUMS=${checksums_path}"
