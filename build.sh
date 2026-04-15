#!/bin/bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")" && pwd)"

if [[ -z "${DMUX_VERSION:-}" ]]; then
  if git -C "${root_dir}" describe --tags --abbrev=0 >/dev/null 2>&1; then
    export DMUX_VERSION="$(git -C "${root_dir}" describe --tags --abbrev=0 | sed 's/^v//')"
  else
    export DMUX_VERSION="0.1.2"
  fi
fi

export DMUX_BUILD_NUMBER="${DMUX_BUILD_NUMBER:-1}"
exec "${root_dir}/scripts/release/package-local-dmg.sh"
