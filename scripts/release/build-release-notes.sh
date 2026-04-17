#!/bin/bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <version> <output_path> [english_changelog] [chinese_changelog]" >&2
  exit 1
fi

version="$1"
output_path="$2"
english_changelog="${3:-CHANGELOG.md}"
chinese_changelog="${4:-CHANGELOG.zh-CN.md}"

script_dir="$(cd "$(dirname "$0")" && pwd)"
extract_script="${script_dir}/extract-release-notes.sh"

english_notes="$(
  bash "${extract_script}" "${version}" "${english_changelog}"
)"

chinese_notes=""
if [[ -f "${chinese_changelog}" ]]; then
  if chinese_notes="$(bash "${extract_script}" "${version}" "${chinese_changelog}" 2>/dev/null)"; then
    :
  else
    chinese_notes=""
  fi
fi

mkdir -p "$(dirname "${output_path}")"

if [[ -n "${chinese_notes//[[:space:]]/}" ]]; then
  {
    printf '## 中文\n\n'
    printf '%s\n' "${chinese_notes}"
    printf '\n---\n\n'
    printf '## English\n\n'
    printf '%s\n' "${english_notes}"
  } > "${output_path}"
else
  printf '%s\n' "${english_notes}" > "${output_path}"
fi
