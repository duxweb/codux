#!/bin/zsh
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
configuration="debug"
app_name="dmux"
binary_name="dmux"
bundle_id="com.dmux.dev"
app_dir="/tmp/${app_name}-dev.app"
contents_dir="${app_dir}/Contents"
macos_dir="${contents_dir}/MacOS"
resources_dir="${contents_dir}/Resources"
helpers_dir="${resources_dir}/Helpers"
plist_path="${contents_dir}/Info.plist"
launcher_path="${macos_dir}/${binary_name}"
pkginfo_path="${contents_dir}/PkgInfo"
iconset_dir="${app_dir}.iconset"
icns_path="${resources_dir}/AppIcon.icns"
localizations=(en zh-Hans zh-Hant de es fr ja ko pt-BR ru)

swift build -c "${configuration}" >/dev/null
swift build -c "${configuration}" --product dmux-notify-helper >/dev/null
build_products_dir="$(swift build -c "${configuration}" --show-bin-path)"
build_bin="${build_products_dir}/${binary_name}"
notify_helper_bin="${build_products_dir}/dmux-notify-helper"

if [[ ! -x "${build_bin}" || ! -x "${notify_helper_bin}" ]]; then
  print -u2 -- "missing built binary: ${build_bin}"
  exit 1
fi

pkill -x "${binary_name}" >/dev/null 2>&1 || true
mkdir -p "${macos_dir}"
mkdir -p "${resources_dir}"
rm -rf "${resources_dir}"/*
mkdir -p "${helpers_dir}"
rm -f "${launcher_path}"
rm -rf "${iconset_dir}"
cp -f "${build_bin}" "${launcher_path}"
chmod +x "${launcher_path}"
cp -f "${notify_helper_bin}" "${helpers_dir}/dmux-notify-helper"
chmod +x "${helpers_dir}/dmux-notify-helper"

for bundle_path in "${build_products_dir}"/*.bundle; do
  if [[ -d "${bundle_path}" ]]; then
    cp -R "${bundle_path}" "${resources_dir}/"
  fi
done

swift "${root_dir}/scripts/release/generate-app-icon.swift" "${iconset_dir}" >/dev/null
iconutil -c icns "${iconset_dir}" -o "${icns_path}"
rm -rf "${iconset_dir}"

for localization in "${localizations[@]}"; do
  mkdir -p "${resources_dir}/${localization}.lproj"
  cat > "${resources_dir}/${localization}.lproj/InfoPlist.strings" <<'EOF'
"CFBundleDisplayName" = "dmux";
"CFBundleName" = "dmux";
EOF
done

cat > "${plist_path}" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleAllowMixedLocalizations</key>
  <true/>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>dmux</string>
  <key>CFBundleExecutable</key>
  <string>dmux</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>com.dmux.dev</string>
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
  <string>dmux</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>LSEnvironment</key>
  <dict>
    <key>DMUX_WORKSPACE_ROOT</key>
    <string>__DMUX_WORKSPACE_ROOT__</string>
  </dict>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

perl -0pi -e "s#__DMUX_WORKSPACE_ROOT__#${root_dir//\#/\\#}#g" "${plist_path}"
printf 'APPL????' > "${pkginfo_path}"
codesign --force --deep --sign - --timestamp=none "${app_dir}" >/dev/null

open -n "${app_dir}"
