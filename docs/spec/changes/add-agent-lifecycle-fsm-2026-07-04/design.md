## Context

Codux's runtime already detects whether a terminal's AI agent is working or waiting via `ScreenSignal` (`crates/codux-runtime-live/src/ai_runtime/screen_signal.rs`), which reads the rendered terminal tail and matches "esc to interrupt" / "Thinking…" / approval prompts. The supervisor applies this signal to session state in `AIRuntimeStateStore`, and `AISessionSnapshot.state` carries exactly three values: `"idle"`, `"responding"`, `"needsInput"` (`runtime_state_for_hook_kind` in `crates/codux-runtime-live/src/ai_runtime/state.rs`). **However, the desktop never sees those raw strings**: `summary_from_runtime_snapshot` re-maps them via `runtime_snapshot_session_state` (`crates/codux-runtime-live/src/ai_runtime_state.rs`) into the summary domain `"running"`, `"needs-input"`, `"completed"`, `"idle"` — and `AIRuntimeSessionSummary.state` is what the app consumes. The FSM input mapping keys on the summary domain (with the raw strings kept as aliases). This mismatch shipped in v1–v3 and made the FSM inert in the live app; the spec originally documented only the raw domain.

The desktop app has a coarse `AIActivityState { Idle, Running, Review, Done }` mapping (`apps/desktop/src/app/ai_runtime_status.rs`) that converts `session.state` to a per-project activity level — but this is project-scoped (not per-pane), has no hysteresis, and is never rendered in the terminal pane chrome.

Sibling projects solve this differently:
- **cmux** (Swift): agent CLI hooks emit `set_status` socket commands with icon+color; rendered as sidebar status-entry rows
- **daintree** (Electron): passive PTY observation fuses 6 signals (OSC 9;4, byte volume, regex, frame analysis, temperature, prompt detection) through a proper FSM with hysteresis; rendered as tab spinners + pane header chips

## Goals / Non-Goals

- Goals:
  - Per-terminal-pane agent lifecycle state with a proper FSM and hysteresis (no flicker)
  - Status dot on each task-column terminal row (agent name + model already shown as the row subtitle)
  - Zero runtime crate changes (desktop-only feature)
  - Works for all agents codux already supports (Claude Code, Codex, Kiro, etc.)

- Non-Goals:
  - OSC 9;4 shell-integration parsing (future enhancement)
  - Byte-volume / activity-temperature detection signals (future enhancement)
  - Project-level "N agents working" toolbar badge (future, but aggregator included)
  - Replacing the existing `AIActivityState` (it stays for project-level concerns)

## Decisions

### Decision: FSM on the desktop side, fed by existing `session.state`
- **Rationale**: The runtime already fuses hooks + screen signal into `session.state`. Duplicating that fusion in a desktop FSM would create two sources of truth. The desktop owns the render loop and hysteresis is a UI concern.
- **Alternative considered**: FSM in the runtime crate, streaming lifecycle events to the desktop. Rejected — would require new IPC messages and runtime changes, widening blast radius.

### Decision: New `AgentLifecycleState` enum (not reusing `AIActivityState`)
- **Rationale**: `AIActivityState` is per-project and lacks `Working`/`Exited` granularity. The pane-level indicator needs finer states and hysteresis. Keeping both avoids coupling two render targets.
- **Alternative considered**: Add hysteresis to `AIActivityState` and make it per-pane. Rejected — would break the existing project-level consumers (pet, project column).

### Decision: Status dot on task-column terminal rows (not a pane overlay)
- **Rationale**: The point of the feature is spotting which terminal needs attention *without* opening it — that is the sidebar terminal list, not the pane itself. An open pane already shows the agent's status in its content. The terminal rows already display agent name + model as subtitle (`terminal_ai_titles_by_terminal_id`), so the row only needs the status dot.
- **Alternatives considered**: (a) Full-width header strip — rejected, too heavy. (b) Floating overlay chip at `top_2().left_2()` on each pane — implemented in v1, rejected by user after trying it: redundant with visible pane content, and invisible exactly when you need it (pane not open). Removed in favor of the row indicator.

### Decision: Input from `session.state` string (not raw `ScreenSignal`)
- **Rationale**: `session.state` is already the fused output of hooks + screen signal + transcript monitoring. Tapping it directly avoids re-implementing fusion. Trade-off: inherits the supervisor's poll interval (`POLL_INTERVAL_SECONDS`) for responsiveness.

### Decision: Completion derived from the `responding → idle` edge; no `Completed` session state, no watchdog
- **Rationale**: The runtime never emits a `"completed"` session state — `turnCompleted` hooks map to `"idle"`. So the FSM treats a `Settle` input (state `"idle"`) arriving while `Working` as turn completion. Likewise there is no desktop watchdog: the runtime supervisor already owns turn staleness (it expires stale `responding` turns itself), and a desktop-side force-to-`Waiting` after N minutes would mislabel legitimately long agent turns.
- **Alternative considered**: Keying completion off `has_completed_turn` / `completed_phase`. Rejected — the edge is simpler, per-pane, and needs no extra fields.

### Decision: No `Exited` state
- **Rationale**: When an agent session unbinds (CLI exits, session pruned from inventory), the pane's lifecycle entry is removed and the chip hides — a dedicated `Exited` state would never render because the chip requires a bound session.

### Decision: Pane lifecycle changes drive task-column invalidation directly
- **Rationale**: The task column was previously invalidated only when the project-level activity summary changed (`ai_activity_project_states_changed` compares project phases/totals, not per-pane session states), so a pane's state change often never re-rendered the rows — the v2 indicator never lit up in practice. `sync_pane_agent_lifecycle` now returns whether any pane's state changed, and both runtime tick paths invalidate the task column on it. The fast tick's no-events early return also ticks the lifecycle map so timer transitions (Completed decay) render on time.
- **Alternative considered**: Widening `ai_activity_project_states_changed` to compare session states. Rejected — it feeds project-level consumers (pet, project column) that don't care about pane states and would re-render more than needed.

## Risks / Trade-offs

- **Poll-interval latency**: The FSM updates on each runtime inventory refresh, not on every screen change. Sub-second responsiveness would require tapping `ScreenSignal` directly (runtime change). Acceptable for v1 — the existing poll is already fast enough for `AIActivityState`.
- **GPUI animation**: GPUI lacks a built-in spinner primitive. Need to verify `Animation` transform rotation support or fall back to opacity pulse. This is the one technical unknown — will be resolved in task 6 before committing to the spinning-dot visual.
- **State explosion on rapid switching**: When the user switches projects, all pane lifecycle states reset. Need to ensure stale states from a previous project don't persist — handled by keying on `terminal_id` which is project-scoped.

## Migration Plan

No migration needed — this is a purely additive feature. No existing behavior changes. `AIActivityState` continues to work unchanged for its existing consumers.

## Open Questions

1. **Collapse persistence**: Should the "collapse overlay" toggle persist across app restarts per-project, or reset each session? (Default: reset each session for v1.)
2. **GPUI spinner**: Confirm `Animation::new(Duration).with_easing()` supports continuous rotation transform, or design fallback opacity-pulse. (Resolved in task 6.)
