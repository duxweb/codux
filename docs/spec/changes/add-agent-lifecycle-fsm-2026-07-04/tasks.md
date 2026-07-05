---
created_at: 2026-07-04T00:00:00Z
updated_at: 2026-07-05T01:55:00Z
completed_at:
---

## 1. FSM Core

- [x] 1.1 Define `AgentLifecycleState` enum (`Idle | Working | Waiting | Completed`) in `apps/desktop/src/app/ai_runtime_status.rs`
- [x] 1.2 Define `AgentLifecycleInput` enum (`Busy | Prompt | Settle`)
- [x] 1.3 Implement `AgentLifecycleState::allowed_transitions()` returning valid next states
- [x] 1.4 Implement `AgentLifecycleState::transition(input)` applying an input event, returning `Option<AgentLifecycleState>` (`None` = no-op)
- [x] 1.5 Implement `AgentLifecycleState::from_session_state(state: &str)` mapping the runtime `session.state` string (`"responding"` → `Busy`, `"needsInput"` → `Prompt`, `"idle"` → `Settle`, other → `None`)
- [x] 1.6 Write unit tests for every valid transition + no-op pair in `tests.rs`, including `Working + Settle → Completed` (turn-completion edge) and `Idle + Prompt → Waiting` (prompt at startup)

## 2. Hysteresis Layer

- [x] 2.1 Create `apps/desktop/src/app/agent_lifecycle.rs` module
- [x] 2.2 Define `PaneAgentLifecycle { state, last_transition_at, last_input_at }` struct
- [x] 2.3 Implement `PaneAgentLifecycle::tick(input, now)` applying hysteresis rules:
  - [x] 2.3a Minimum hold: 1500ms in `Working` before `Waiting`/`Completed` allowed
  - [x] 2.3b Idle debounce: 8000ms with no input of any kind before `Working → Idle` (stall safety net)
  - [x] 2.3c Completed decay: 3000ms in `Completed` with no `Busy`/`Prompt` → `Idle` (drives checkmark auto-hide)
  - [x] 2.3d Transition lock: 500ms after `Working ↔ Waiting`, suppress the reverse transition
- [x] 2.4 Write unit tests for each hysteresis rule (hold, debounce, decay, lock)
- [x] 2.5 Register module in `apps/desktop/src/app.rs`

## 3. State Store + Wiring

- [x] 3.1 Add `pane_agent_lifecycle: HashMap<String, PaneAgentLifecycle>` to `AppState` in `app_state.rs`
- [x] 3.2 Initialize empty map in `app_lifecycle.rs`
- [x] 3.3 On each runtime inventory refresh, for each pane with a bound `AISessionSnapshot`:
  - [x] 3.3a Look up `session.state` by `terminal_id`
  - [x] 3.3b Map to `AgentLifecycleInput` via `from_session_state`
  - [x] 3.3c Feed into `PaneAgentLifecycle::tick()`
  - [x] 3.3d Write updated state back to the map
- [x] 3.4 Clean up stale entries when the session unbinds (gone from inventory), panes close, or projects switch

## 4. Display Helpers

- [x] 4.1 Create `apps/desktop/src/app/agent_display.rs` module
- [x] 4.2 Implement `humanize_tool_name(tool: &str) -> String` over canonical tool ids (e.g. `"claude"` → `"Claude Code"`, `"codex"` → `"Codex"`; fallback title-cases with `_`/`-` → space)
- [x] 4.3 Implement `shorten_model_name(model: &str) -> String` (e.g. `"claude-sonnet-4-5-20250514"` → `"Sonnet 4.5"`, `"gpt-4o"` → `"GPT-4o"`; unknown → truncate to 20 chars)
- [x] 4.4 Write unit tests for known agent names + model families
- [x] 4.5 Register module in `apps/desktop/src/app.rs`

## 5. Pane Overlay Render

- [x] 5.1 In `workspace_terminal.rs` `terminal_panes()`, for each pane with a bound AI session, render a floating chip at `absolute().top_2().left_2()`:
  - [x] 5.1a Left: status dot (colored circle, animated for `Working`)
  - [x] 5.1b Right of dot: `humanize_tool_name(tool)` text, plus ` · shorten_model_name(model)` when `model` is `Some`
- [x] 5.2 Implement status dot render:
  - [x] 5.2a `Working` → spinning blue dot (`theme::ACCENT` / `#4C8DFF`)
  - [x] 5.2b `Waiting` → static amber dot
  - [x] 5.2c `Completed` → green checkmark (hides when the state decays to `Idle` after 3s)
  - [x] 5.2d `Idle` → no dot
- [x] 5.3 Implement collapse toggle: click chip to hide; reappear on `Waiting` transition
- [x] 5.4 Hide chip entirely when no AI session bound to the pane
- [x] 5.5 Wire the chip into the live render path: extend `TerminalPaneViewSnapshot` (+ its `PartialEq`) with agent chip data built in `terminal_workspace_snapshot()`, render it in `workspace_views.rs` `terminal_pane()` reusing a shared chip helper (the `workspace_terminal.rs` path from 5.1 is a non-live fallback; `TerminalWorkspaceView` is the real UI)

## 6. GPUI Animation

- [x] 6.1 Verify GPUI `Animation` API supports continuous rotation transform
- [x] 6.2 If yes: implement spinning dot via rotating `div()` with `Animation::new().with_easing()`
- [x] 6.3 If no: fall back to opacity-pulse animation (breathing effect)
- [x] 6.4 Add `motion-reduce` equivalent: disable animation when system "reduce motion" is on

## 7. Integration + Verification

- [x] 7.1 Write integration test in `tests.rs`: simulate `session.state` sequence, assert `PaneAgentLifecycle` reaches expected final state
- [ ] 7.2 Manual test: launch Claude Code in a pane → dot spins during "Thinking…" → turns amber on approval prompt → green checkmark on completion, fading to name-only after 3s
- [ ] 7.3 Manual test: repeat with Codex, Kiro
- [x] 7.4 Run `cargo check` on `apps/desktop`
- [x] 7.5 Run `cargo test` on `apps/desktop`
- [x] 7.6 Verify no regressions in existing `AIActivityState` consumers (pet, project column)

## 8. Rework: sidebar row indicator instead of pane chip (user feedback on v1)

- [x] 8.1 Remove the floating chip: `terminal_pane_agent_chip_element`, `AgentPaneChipSnapshot`, the chip render in `workspace_terminal.rs` `terminal_panes()` and in `workspace_views.rs` `terminal_pane()`, the `agent_chip` field on `TerminalPaneViewSnapshot` (+ its `PartialEq` line), and the chip build in `terminal_workspace_snapshot()`
- [x] 8.2 Remove the collapse state: `pane_agent_chip_collapsed` field (+ inits in `app_lifecycle.rs` / `window_actions.rs`), `prune_pane_agent_chip_collapsed`, and its call in `sync_pane_agent_lifecycle`
- [x] 8.3 Remove now-unused display helpers `humanize_tool_name` / `shorten_model_name` (+ their tests); keep `reduce_motion_enabled` (used by the row dot)
- [x] 8.4 Add `lifecycle: Option<AgentLifecycleState>` to `TaskTerminalRow`, built in `task_terminal_list_snapshot()` from `pane_agent_lifecycle` (None for collapsed rows / no session)
- [x] 8.5 Render the status dot in `terminal_compact_row()` (spinning blue `Working` with reduce-motion fallback, static amber `Waiting`, green check `Completed`, nothing for `Idle`/None), placed before the subtitle
- [ ] 8.6 Re-run `cargo check` + `cargo test -p codux`; manual test replaces 7.2: row dot spins while agent works, turns amber on prompt, brief green check on completion

## 9. Fix: rows never refresh on lifecycle change (v2 field report: indicator never lights up)

- [x] 9.1 Make `sync_pane_agent_lifecycle` return `bool` — true when any pane's `state` changed during the tick or a non-`Idle` entry was pruned (`agent_lifecycle.rs`)
- [x] 9.2 In `apply_runtime_activity_tick` (`ssh_remote_actions.rs`): always tick the lifecycle map when it is non-empty (even when `should_refresh_ai_state` is false), invalidate the task column when the sync reports a change, and include that change in the tick result's `changed`
- [x] 9.3 In `apply_ai_runtime_activity_tick` (`ssh_remote_actions.rs`): on the no-events early return, still tick lifecycle timers when the map is non-empty; when the sync reports a change, invalidate the task column and return `changed: true`; same capture + invalidate on the main path
- [x] 9.4 Adjust remaining `sync_pane_agent_lifecycle` call sites (`runtime_actions.rs`, `pet_actions.rs`, other `ssh_remote_actions.rs` sites) for the new return type
- [x] 9.5 Tint the row's `CommandLine` icon with the lifecycle color when non-idle (`task_column.rs` `terminal_compact_row`): `Working` → `theme::ACCENT`, `Waiting` → `theme::ORANGE`, `Completed` → `theme::GREEN`; default muted color when `Idle`/no session (add an `agent_lifecycle_color` helper in `agent_display.rs`)
- [x] 9.6 Unit test: sync change detection (state change → true, steady state → false, prune of non-idle entry → true)
- [ ] 9.7 Re-run `cargo check -p codux` + `cargo test -p codux`; manual test: row icon + dot light up while agent works, amber on prompt, green check decays after ~3s without further events

## 10. Worktree row indicator + live git counts (user request)

- [x] 10.1 Add `aggregate_agent_lifecycle` free function (priority `Working` > `Waiting` > `Completed`, `Idle`/empty → None) + `CoduxApp::worktree_agent_lifecycle(worktree)` attributing sessions by `project_id == worktree.id` or default-worktree `project_id` match (`agent_lifecycle.rs`), with unit tests
- [x] 10.2 Add `lifecycle: Option<AgentLifecycleState>` to `TaskWorktreeRow`, built in `task_worktree_list_snapshot()` (`task_column.rs`)
- [x] 10.3 Render `agent_lifecycle_status_dot` on the worktree row between the text column and the +/− counts; existing `worktree_activity_dot` untouched (`task_column.rs`)
- [x] 10.4 Add `quiet` variant of `refresh_git_panel_state_async` that skips the `status_message` update (`project_actions.rs`)
- [x] 10.5 Trigger the quiet git refresh from the runtime ticks: while any pane lifecycle is `Working`, at most once per 5s (new `agent_git_refresh_after: Option<Instant>` throttle field on `CoduxApp`); plus once when a pane transitions into `Completed` (`ssh_remote_actions.rs`, `app_state.rs`, inits in `app_lifecycle.rs`/`window_actions.rs`)
- [ ] 10.6 Re-run `cargo check -p codux` + `cargo test -p codux`; manual test: worktree row spinner while agent works, +/− counts tick up during the turn, final counts right after completion

## 11. Fix: collapsed pane rows never show the indicator (field report: agent pane is collapsed, row stays static green)

- [x] 11.1 Build `lifecycle` for collapsed rows in `task_terminal_list_snapshot()` from `slot.terminal_id` (`task_column.rs`), replacing the hardcoded `lifecycle: None`
- [x] 11.2 In `terminal_compact_row()`: when the row is collapsed and lifecycle is non-`Idle`, render the lifecycle status dot instead of the static green collapsed dot; keep the green dot for idle/no-session collapsed rows; ensure the lifecycle dot is not rendered twice
- [ ] 11.3 Re-run `cargo check -p codux` + `cargo test -p codux`; manual test with a collapsed agent pane: row icon tints and dot spins/turns amber while the agent works

## 12. Fix: FSM never receives inputs — summary state domain mismatch (root cause of all "nothing lights up" reports)

- [x] 12.1 Fix `AgentLifecycleState::from_session_state` (`ai_runtime_status.rs`): the desktop consumes `AIRuntimeSessionSummary.state`, which `runtime_snapshot_session_state` re-maps to `"running" | "needs-input" | "completed" | "idle"` — the raw `"responding"`/`"needsInput"` strings never reach the app. Map `"running"`/`"responding"` → `Busy`, `"needs-input"`/`"needsInput"` → `Prompt`, `"idle"`/`"completed"` → `Settle`, other → `None`
- [x] 12.2 Update the mapping unit tests + the integration test to feed the summary domain strings (keep raw-string alias assertions)
- [ ] 12.3 Re-run `cargo check -p codux` + `cargo test -p codux`; manual test: rebuild, relaunch, trigger agent — `agent-lifecycle` lines appear in the runtime log and the terminal row lights up
