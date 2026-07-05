---
created_at: 2026-07-04T00:00:00Z
updated_at: 2026-07-04T00:00:00Z
---

## Why

The terminal workspace shows model name and pane count but does not indicate whether a terminal's AI agent is actively working, waiting for input, or idle. Users cannot tell at a glance which panes need attention without switching to each one. Both sibling projects (cmux, daintree) solve this; codux already has the detection layer (`ScreenSignal`) but never surfaces it in the pane UI.

## What Changes

- **New `AgentLifecycleState` enum** (`Idle | Working | Waiting | Completed`) replacing the coarse mapping currently used for `AIActivityState` at the pane level, with a proper finite-state machine governing valid transitions; turn completion is derived from the `responding → idle` session-state edge (agent exit is handled by removing the pane's lifecycle entry, not by a state)
- **Hysteresis layer** (`PaneAgentLifecycle`) with minimum-hold timers, idle debounce, completed decay, and transition locks to prevent flicker on rapid agent state changes
- **Per-terminal-pane state tracking** stored on the desktop app state, updated on each runtime inventory refresh
- **Status dot on task-column terminal rows** showing the lifecycle state (spinning blue working / amber waiting / brief green check), visible only when an AI session is bound to that terminal — so the user can see which terminal needs attention without opening it. (v1 shipped a floating overlay chip on the panes; rejected after trying it — the open pane already shows the agent's status — and replaced by the sidebar row indicator.)
- No runtime crate changes; no new IPC messages; all data flows through the existing `AISessionSnapshot` → runtime inventory → desktop path

## Impact

- Affected specs: `agent-lifecycle` (new), `terminal-list-indicator` (new)
- Affected code:
  - `apps/desktop/src/app/ai_runtime_status.rs` — extend with new enum + FSM
  - `apps/desktop/src/app/agent_lifecycle.rs` — new file for hysteresis struct
  - `apps/desktop/src/app/agent_display.rs` — new file for display helpers
  - `apps/desktop/src/app/workspace_terminal.rs` — render overlay chip
  - `apps/desktop/src/app/app_state.rs` — add `pane_agent_lifecycle` state map
  - `apps/desktop/src/app/app_lifecycle.rs` — initialize new state field
  - `apps/desktop/src/app/tests.rs` — FSM + hysteresis unit tests
- Estimated: ~530 LOC, all in `apps/desktop`
