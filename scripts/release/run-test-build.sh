#!/bin/bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
cd "${root_dir}"

require_command() {
  local command_name="$1"
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "[test-build] missing required command: ${command_name}" >&2
    exit 1
  fi
}

require_command gh
require_command git

ref="main"
configuration="Release"
build_number=""
watch_run=0
version=""
package_version=""

usage() {
  cat <<'EOF'
Usage:
  bash scripts/release/run-test-build.sh [options]

Options:
  -v, --version <version>        App version. Default: latest tag version
  -p, --package-version <label>  Artifact label. Default: <version>-test-<date>
  -r, --ref <branch>             Git ref to run on. Default: main
  -c, --configuration <config>   Release or Debug. Default: Release
  -b, --build-number <number>    Optional build number override
  -w, --watch                    Watch the workflow after triggering
  -h, --help                     Show this help

Examples:
  bash scripts/release/run-test-build.sh
  bash scripts/release/run-test-build.sh -v 0.1.3 -p 0.1.3-test-20260416 -r main -c Release
  bash scripts/release/run-test-build.sh -w
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -v|--version)
      version="${2:-}"
      shift 2
      ;;
    -r|--ref)
      ref="${2:-}"
      shift 2
      ;;
    -p|--package-version)
      package_version="${2:-}"
      shift 2
      ;;
    -c|--configuration)
      configuration="${2:-}"
      shift 2
      ;;
    -b|--build-number)
      build_number="${2:-}"
      shift 2
      ;;
    -w|--watch)
      watch_run=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[test-build] unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

case "${configuration}" in
  Release|Debug)
    ;;
  *)
    echo "[test-build] configuration must be Release or Debug" >&2
    exit 1
    ;;
esac

if [[ -z "${version}" ]]; then
  if git describe --tags --abbrev=0 >/dev/null 2>&1; then
    version="$(git describe --tags --abbrev=0 | sed 's/^v//')"
  else
    version="0.1.0"
  fi
fi

if [[ -z "${package_version}" ]]; then
  date_suffix="$(date +%Y%m%d)"
  package_version="${version}-test-${date_suffix}"
fi

echo "[test-build] triggering Test Build"
echo "[test-build] ref=${ref}"
echo "[test-build] version=${version}"
echo "[test-build] package_version=${package_version}"
echo "[test-build] configuration=${configuration}"
if [[ -n "${build_number}" ]]; then
  echo "[test-build] build_number=${build_number}"
fi

args=(
  workflow run test-build.yml
  --ref "${ref}"
  -f "version=${version}"
  -f "package_version=${package_version}"
  -f "configuration=${configuration}"
)

if [[ -n "${build_number}" ]]; then
  args+=(-f "build_number=${build_number}")
fi

gh "${args[@]}"

sleep 3

run_id="$(gh run list --workflow test-build.yml --branch "${ref}" --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
run_url="$(gh run list --workflow test-build.yml --branch "${ref}" --limit 1 --json url --jq '.[0].url // empty')"
run_status="$(gh run list --workflow test-build.yml --branch "${ref}" --limit 1 --json status --jq '.[0].status // empty')"

if [[ -n "${run_id}" ]]; then
  echo "[test-build] run_id=${run_id}"
fi
if [[ -n "${run_status}" ]]; then
  echo "[test-build] status=${run_status}"
fi
if [[ -n "${run_url}" ]]; then
  echo "[test-build] url=${run_url}"
fi

if [[ "${watch_run}" == "1" && -n "${run_id}" ]]; then
  gh run watch "${run_id}"
fi
