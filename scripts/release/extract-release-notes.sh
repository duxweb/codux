#!/bin/bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <version> [changelog_path]" >&2
  exit 1
fi

version="$1"
changelog_path="${2:-CHANGELOG.md}"

if [[ ! -f "${changelog_path}" ]]; then
  echo "Changelog not found: ${changelog_path}" >&2
  exit 1
fi

notes="$(
  awk -v version="${version}" '
    $0 ~ "^## \\[" version "\\]" { flag=1; next }
    /^## \[/ && flag { exit }
    flag { print }
  ' "${changelog_path}"
)"

notes="$(printf '%s\n' "${notes}" | sed -e '/./,$!d')"

if [[ -z "${notes//[[:space:]]/}" ]]; then
  echo "No changelog entry found for version ${version}" >&2
  exit 1
fi

printf '%s\n' "${notes}"
