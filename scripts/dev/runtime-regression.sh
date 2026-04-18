#!/bin/zsh
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
report_root="${1:-${TMPDIR:-/tmp}/dmux-runtime-regression-$(date +%Y%m%d-%H%M%S)}"

mkdir -p "${report_root}"

echo "[runtime-regression] report dir: ${report_root}"

run_step() {
  local name="$1"
  shift
  echo "[runtime-regression] >>> ${name}"
  if "$@" >"${report_root}/${name}.log" 2>&1; then
    echo "[runtime-regression] OK  ${name}"
  else
    echo "[runtime-regression] FAIL ${name}"
    echo "[runtime-regression] see ${report_root}/${name}.log"
  fi
}

cd "${root_dir}"

run_step "swift-test-drivers" \
  swift test --filter RuntimeDriverTests

run_step "swift-test-lifecycle" \
  swift test --filter RuntimeLifecycleScenarioTests

run_step "claude-interactive-flow" \
  python3 scripts/dev/runtime-scenario-runner.py \
    --tool claude \
    --scenario flow \
    --report-json "${report_root}/claude-interactive-flow.json"

run_step "claude-noninteractive-flow" \
  python3 scripts/dev/runtime-noninteractive-flow.py \
    --tool claude \
    --report-json "${report_root}/claude-noninteractive-flow.json"

run_step "codex-noninteractive-flow" \
  python3 scripts/dev/runtime-noninteractive-flow.py \
    --tool codex \
    --report-json "${report_root}/codex-noninteractive-flow.json"

run_step "gemini-noninteractive-flow" \
  python3 scripts/dev/runtime-noninteractive-flow.py \
    --tool gemini \
    --report-json "${report_root}/gemini-noninteractive-flow.json"

run_step "opencode-noninteractive-flow" \
  python3 scripts/dev/runtime-noninteractive-flow.py \
    --tool opencode \
    --report-json "${report_root}/opencode-noninteractive-flow.json"

echo "[runtime-regression] done"
