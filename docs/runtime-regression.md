# Runtime Regression Guide

This document records the current regression tooling for dmux AI runtime behavior.

Use it when verifying:

- loading / responding transitions
- interrupt handling
- completion handling
- restored session token baselines
- resume / reopen session behavior

## Test Layers

### 1. Unit tests

Fastest feedback. These run entirely inside Swift tests.

Commands:

```bash
swift test
swift test --filter RuntimeDriverTests
swift test --filter RuntimeLifecycleScenarioTests
```

Single examples:

```bash
swift test --filter RuntimeDriverTests/testCodexStopWithoutDefinitiveCompletionDoesNotReassertResponding
swift test --filter RuntimeLifecycleScenarioTests/testRestoredSessionStartsFromZeroThenGrowsAcrossMessages
```

Coverage:

- tool driver hook/socket state transitions
- shared runtime store merge / preserve rules
- new session token growth
- interrupt -> idle
- restored session baseline behavior
- reset / reopen / mixed fresh+restored flows

### 2. Real interactive hook scenarios

Runs the real `claude` / `codex` wrappers in a PTY and captures dmux hook/runtime events over a temporary unix socket.

Script:

- `scripts/dev/runtime-scenario-runner.py`

Commands:

```bash
python3 scripts/dev/runtime-scenario-runner.py --tool claude --scenario interrupt
python3 scripts/dev/runtime-scenario-runner.py --tool claude --scenario flow --report-json /tmp/claude-flow.json
python3 scripts/dev/runtime-scenario-runner.py --tool codex --scenario interrupt
python3 scripts/dev/runtime-scenario-runner.py --tool codex --scenario flow --report-json /tmp/codex-flow.json
```

Notes:

- `claude` currently supports full interactive flow verification.
- `codex` interactive flow can still be blocked by upstream TUI crashes; use the non-interactive runner below for reliable resume/reopen validation.

### 3. Real non-interactive flow scenarios

Runs real models through non-interactive CLI paths.

Script:

- `scripts/dev/runtime-noninteractive-flow.py`

Commands:

```bash
python3 scripts/dev/runtime-noninteractive-flow.py --tool claude
python3 scripts/dev/runtime-noninteractive-flow.py --tool codex
```

Current behavior:

- `claude` uses `--print --output-format stream-json --verbose --resume`
- `codex` uses `exec --json` and `exec resume --json`

For `codex`, the script automatically falls back across candidate models until one works.

Current default candidate order:

1. `gpt-5.1-codex-mini`
2. `gpt-5.2-codex`
3. `gpt-5.4-mini`
4. `gpt-5.4`

## Real Model Defaults

Current low-cost defaults:

- `codex`: `gpt-5.1-codex-mini`
- `claude`: `claude-haiku-4-5`

You can override them:

```bash
python3 scripts/dev/runtime-scenario-runner.py --tool codex --model gpt-5.4-mini --scenario flow
python3 scripts/dev/runtime-noninteractive-flow.py --tool claude --model claude-haiku-4-5
```

## Expected Outputs

### Interactive runner

- terminal transcript tail
- summarized dmux hook/response/usage events
- optional structured JSON report via `--report-json`

The JSON report includes:

- `tool`
- `model`
- `external_session_id`
- `steps`
- `events`
- `output_tail`

### Non-interactive runner

Outputs structured JSON to stdout:

- per-step `exit_code`
- session / thread id
- condensed event list

This is the preferred path for codex resume/reopen regression checks.

## Known Limitations

- `codex` interactive TUI can still panic before hooks complete on some prompt/input paths.
- `claude` interactive flow is the most reliable real-time regression target right now.
- These scripts are for developer regression work, not CI defaults.

## When To Use Which Tool

- If you are changing shared merge / lifecycle logic: start with `swift test`.
- If you are changing Claude hook behavior: run `RuntimeDriverTests` and `runtime-scenario-runner.py --tool claude`.
- If you are changing Codex stop / resume logic: run `RuntimeDriverTests`, `RuntimeLifecycleScenarioTests`, and `runtime-noninteractive-flow.py --tool codex`.
