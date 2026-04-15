#!/bin/bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
app_name="dmux"
binary_name="dmux"
bundle_id="com.duxweb.dmux"
version="${DMUX_VERSION:-0.1.0}"
build_number="${DMUX_BUILD_NUMBER:-1}"
configuration="${DMUX_CONFIGURATION:-release}"
arch_list="${DMUX_ARCHS:-arm64 x86_64}"
read -r -a target_archs <<< "${arch_list}"
codesign_identity="${APPLE_CODESIGN_IDENTITY:--}"
enable_notarization="${DMUX_NOTARIZE:-0}"
apple_team_id="${APPLE_TEAM_ID:-}"
notary_profile="${APPLE_NOTARY_PROFILE:-}"
notary_api_key_path="${APPLE_API_PRIVATE_KEY_PATH:-}"
notary_api_key_id="${APPLE_API_KEY_ID:-}"
notary_api_issuer_id="${APPLE_API_ISSUER_ID:-}"
dist_dir="${root_dir}/dist"
app_dir="${dist_dir}/${app_name}.app"
contents_dir="${app_dir}/Contents"
macos_dir="${contents_dir}/MacOS"
resources_dir="${contents_dir}/Resources"
helpers_dir="${resources_dir}/Helpers"
runtime_root="${resources_dir}/runtime-root"
launcher_path="${macos_dir}/${binary_name}"
binary_path="${macos_dir}/${binary_name}-bin"
plist_path="${contents_dir}/Info.plist"
pkginfo_path="${contents_dir}/PkgInfo"
dmg_staging_dir="${dist_dir}/dmg"
dmg_path="${dist_dir}/${app_name}-${version}.dmg"
zip_path="${dist_dir}/${app_name}-${version}.zip"
checksums_path="${dist_dir}/SHA256SUMS.txt"
iconset_dir="${dist_dir}/AppIcon.iconset"
icns_path="${resources_dir}/AppIcon.icns"
localizations=(en zh-Hans zh-Hant de es fr ja ko pt-BR ru)

function build_dir_for_arch() {
  local arch="$1"
  swift build -c "${configuration}" --arch "${arch}" --show-bin-path
}

function sign_app() {
  if [[ "${codesign_identity}" == "-" ]]; then
    echo "[package] applying ad-hoc signature"
    codesign --force --deep --sign - --timestamp=none "${app_dir}"
  else
    echo "[package] signing app with identity: ${codesign_identity}"
    codesign --force --deep --options runtime --sign "${codesign_identity}" "${app_dir}"
  fi
}

function verify_app_signature() {
  echo "[package] verifying bundle signature"
  codesign --verify --deep --strict --verbose=2 "${app_dir}"
}

function notarize_app_if_needed() {
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

echo "[package] building universal swift package (${configuration}) for: ${target_archs[*]}"

build_product_dirs=()
build_bins=()
notify_helper_bins=()

for arch in "${target_archs[@]}"; do
  echo "[package] building architecture ${arch}"
  swift build -c "${configuration}" --arch "${arch}" >/dev/null
  swift build -c "${configuration}" --arch "${arch}" --product dmux-notify-helper >/dev/null

  build_products_dir="$(build_dir_for_arch "${arch}")"
  build_bin="${build_products_dir}/${binary_name}"
  notify_helper_bin="${build_products_dir}/dmux-notify-helper"

  if [[ ! -x "${build_bin}" || ! -x "${notify_helper_bin}" ]]; then
    echo "[package] missing built binary for ${arch}: ${build_bin}" >&2
    exit 1
  fi

  build_product_dirs+=("${build_products_dir}")
  build_bins+=("${build_bin}")
  notify_helper_bins+=("${notify_helper_bin}")
done

echo "[package] assembling app bundle at ${app_dir}"
rm -rf "${app_dir}" "${dmg_staging_dir}" "${dmg_path}" "${zip_path}" "${checksums_path}" "${iconset_dir}"
mkdir -p "${macos_dir}" "${resources_dir}" "${helpers_dir}" "${runtime_root}" "${dmg_staging_dir}"

if (( ${#build_bins[@]} == 1 )); then
  cp -f "${build_bins[0]}" "${binary_path}"
  cp -f "${notify_helper_bins[0]}" "${helpers_dir}/dmux-notify-helper"
else
  lipo -create "${build_bins[@]}" -output "${binary_path}"
  lipo -create "${notify_helper_bins[@]}" -output "${helpers_dir}/dmux-notify-helper"
fi
chmod +x "${binary_path}" "${helpers_dir}/dmux-notify-helper"

cat > "${launcher_path}" <<'EOF'
#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
contents_dir="$(cd "${script_dir}/.." && pwd)"
resources_dir="${contents_dir}/Resources"
runtime_root="${resources_dir}/runtime-root"

export DMUX_WORKSPACE_ROOT="${runtime_root}"
exec "${script_dir}/dmux-bin" "$@"
EOF
chmod +x "${launcher_path}"

for bundle_path in "${build_product_dirs[0]}"/*.bundle; do
  if [[ -d "${bundle_path}" ]]; then
    cp -R "${bundle_path}" "${resources_dir}/"
  fi
done

echo "[package] app binary: $(lipo -info "${binary_path}")"
echo "[package] notify helper: $(lipo -info "${helpers_dir}/dmux-notify-helper")"

swift "${root_dir}/scripts/release/generate-app-icon.swift" "${iconset_dir}" >/dev/null
iconutil -c icns "${iconset_dir}" -o "${icns_path}"
rm -rf "${iconset_dir}"

cp -f "${root_dir}/Package.swift" "${runtime_root}/Package.swift"
cp -R "${root_dir}/scripts" "${runtime_root}/scripts"

for localization in "${localizations[@]}"; do
  mkdir -p "${resources_dir}/${localization}.lproj"
  cat > "${resources_dir}/${localization}.lproj/InfoPlist.strings" <<'EOF'
"CFBundleDisplayName" = "dmux";
"CFBundleName" = "dmux";
EOF
done

cat > "${plist_path}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleAllowMixedLocalizations</key>
  <true/>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${app_name}</string>
  <key>CFBundleExecutable</key>
  <string>${binary_name}</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleLocalizations</key>
  <array>
    <string>en</string>
    <string>zh-Hans</string>
    <string>zh-Hant</string>
    <string>de</string>
    <string>es</string>
    <string>fr</string>
    <string>ja</string>
    <string>ko</string>
    <string>pt-BR</string>
    <string>ru</string>
  </array>
  <key>CFBundleName</key>
  <string>${app_name}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${build_number}</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
EOF

printf 'APPL????' > "${pkginfo_path}"

sign_app
verify_app_signature
notarize_app_if_needed

echo "[package] preparing dmg staging"
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

echo "[package] done"
echo "APP=${app_dir}"
echo "DMG=${dmg_path}"
echo "ZIP=${zip_path}"
echo "CHECKSUMS=${checksums_path}"
