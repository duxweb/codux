#!/bin/bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version> <sha256> <output-path>" >&2
  exit 1
fi

version="${1#v}"
sha256="$2"
output_path="$3"

if [[ -z "${version}" || -z "${sha256}" || -z "${output_path}" ]]; then
  echo "version, sha256, and output path are required." >&2
  exit 1
fi

mkdir -p "$(dirname "${output_path}")"

cat > "${output_path}" <<EOF
cask "codux" do
  version "${version}"
  sha256 "${sha256}"

  url "https://github.com/duxweb/codux/releases/download/v#{version}/Codux-#{version}-macos-universal.dmg"
  name "Codux"
  desc "Native terminal workspace for AI coding tools"
  homepage "https://github.com/duxweb/codux"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :sonoma"

  app "Codux.app"

  zap trash: [
    "~/Library/Application Support/Codux",
    "~/Library/Caches/com.duxweb.dmux",
    "~/Library/HTTPStorages/com.duxweb.dmux",
    "~/Library/Preferences/com.duxweb.dmux.plist",
    "~/Library/Saved Application State/com.duxweb.dmux.savedState",
  ]
end
EOF
