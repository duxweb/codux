## ADDED Requirements

### Requirement: Agent Lifecycle State Enum
The system SHALL define a `AgentLifecycleState` enum with four variants — `Idle`, `Working`, `Waiting`, `Completed` — representing the lifecycle of a single AI agent session bound to a terminal pane. This enum is distinct from the existing `AIActivityState` (which remains for project-level aggregation) and provides finer granularity for per-pane rendering. Agent exit is not a state: when the session unbinds from a terminal, the pane's lifecycle entry is removed and the overlay hides (see Per-Pane Lifecycle State Store).

#### Scenario: Enum variants cover all agent lifecycle phases
- **WHEN** the `AgentLifecycleState` enum is defined
- **THEN** it SHALL include exactly: `Idle`, `Working`, `Waiting`, `Completed`
- **AND** each variant SHALL derive `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `Hash`

### Requirement: Finite-State Machine Transitions
The system SHALL enforce a finite-state machine on `AgentLifecycleState` where only valid transitions are permitted. An input that does not match an allowed transition SHALL be rejected (return `None`), leaving the current state unchanged.

Input-driven transitions (inputs: `Busy`, `Prompt`, `Settle`):
- `Idle` + `Busy` → `Working`
- `Idle` + `Prompt` → `Waiting` — an agent can be blocked on the user before it was ever observed working (trust prompt at startup, or the app launching while an agent is already waiting)
- `Working` + `Prompt` → `Waiting`
- `Working` + `Settle` → `Completed` — the `responding` → `idle` edge is the turn-completed signal
- `Waiting` + `Busy` → `Working`
- `Waiting` + `Settle` → `Completed`
- `Completed` + `Busy` → `Working`
- `Completed` + `Prompt` → `Waiting`

Timer-driven transitions (applied by the hysteresis layer on `tick`, not by input):
- `Working` → `Idle` (idle debounce)
- `Completed` → `Idle` (completed decay)

All other (state, input) pairs SHALL be no-ops.

#### Scenario: Valid transition applied
- **WHEN** the current state is `Idle` and a `Busy` input is received
- **THEN** the state SHALL transition to `Working`

#### Scenario: Prompt while idle is allowed
- **WHEN** the current state is `Idle` and a `Prompt` input is received
- **THEN** the state SHALL transition to `Waiting`

#### Scenario: No-op input leaves state unchanged
- **WHEN** the current state is `Idle` and a `Settle` input is received
- **THEN** the transition SHALL be rejected
- **AND** the state SHALL remain `Idle`

#### Scenario: Working to waiting is allowed
- **WHEN** the current state is `Working` and a `Prompt` input is received
- **THEN** the state SHALL transition to `Waiting`

#### Scenario: Turn completion via settle edge
- **WHEN** the current state is `Working` (past the minimum hold) and a `Settle` input is received
- **THEN** the state SHALL transition to `Completed`

### Requirement: Session State Mapping
The system SHALL map the desktop session summary `AIRuntimeSessionSummary.state` string to an `AgentLifecycleInput` event so the FSM can consume existing runtime data without new IPC. The desktop does NOT see the raw runtime states: `summary_from_runtime_snapshot` re-maps them via `runtime_snapshot_session_state` (`crates/codux-runtime-live/src/ai_runtime_state.rs`) to the summary domain `"running"`, `"needs-input"`, `"completed"`, `"idle"` (raw `"responding"` → `"running"`, raw `"needsInput"` or a pending notification → `"needs-input"`, completed turn while not running → `"completed"`). The mapping SHALL cover both domains for robustness:
- `"running"` or raw `"responding"` → `Busy`
- `"needs-input"` or raw `"needsInput"` → `Prompt`
- `"idle"` → `Settle`
- `"completed"` → `Settle` (the summary reports a finished turn; the FSM's `Working + Settle → Completed` edge derives the checkmark, and a long-completed session feeding repeated `Settle` from `Idle` stays `Idle`)
- Any other value → no input (defensive; state unchanged)

The mapping is applied on every runtime inventory refresh, not edge-triggered; the FSM's no-op rules absorb repeated inputs.

#### Scenario: Running state maps to Busy
- **WHEN** `session.state` is `"running"` (or raw `"responding"`)
- **THEN** the mapped input SHALL be `Busy`

#### Scenario: Needs-input state maps to Prompt
- **WHEN** `session.state` is `"needs-input"` (or raw `"needsInput"`)
- **THEN** the mapped input SHALL be `Prompt`

#### Scenario: Idle and completed states map to Settle
- **WHEN** `session.state` is `"idle"` or `"completed"`
- **THEN** the mapped input SHALL be `Settle`

#### Scenario: Unknown state produces no input
- **WHEN** `session.state` is any unmapped value
- **THEN** no input SHALL be produced
- **AND** the current lifecycle state SHALL remain unchanged

### Requirement: Hysteresis Minimum Hold
The system SHALL enforce a minimum hold time of 1500 milliseconds in the `Working` state before a transition to `Waiting` or `Completed` is permitted, to prevent flicker on rapid tool-call boundaries.

#### Scenario: Prompt blocked within hold window
- **WHEN** the state entered `Working` 500ms ago and a `Prompt` input arrives
- **THEN** the transition to `Waiting` SHALL be suppressed
- **AND** the state SHALL remain `Working`

#### Scenario: Settle blocked within hold window
- **WHEN** the state entered `Working` 500ms ago and a `Settle` input arrives
- **THEN** the transition to `Completed` SHALL be suppressed
- **AND** the state SHALL remain `Working`

#### Scenario: Transition allowed after hold window
- **WHEN** the state entered `Working` 2000ms ago and a `Prompt` input arrives
- **THEN** the transition to `Waiting` SHALL be permitted

### Requirement: Hysteresis Idle Debounce
The system SHALL debounce a stalled `Working` state: if no input of any kind is received for 8000 milliseconds while in `Working`, the state SHALL transition to `Idle`. This is a safety net for stalled inventory refreshes or a session missing from a refresh — during normal operation every refresh produces an input, so turn completion flows through the `Settle` edge, not this rule.

#### Scenario: Short gap does not trigger idle
- **WHEN** the state is `Working` and no input arrives for 5000ms
- **THEN** the state SHALL remain `Working`

#### Scenario: Long silence triggers idle
- **WHEN** the state is `Working` and no input arrives for 8000ms or more
- **THEN** the state SHALL transition to `Idle`

### Requirement: Completed Decay
The system SHALL decay the `Completed` state to `Idle` after 3000 milliseconds with no `Busy` or `Prompt` input. This drives the overlay checkmark's auto-hide.

#### Scenario: Completed decays to idle
- **WHEN** the state entered `Completed` 3000ms ago and only `Settle` inputs (or none) have arrived since
- **THEN** the state SHALL transition to `Idle`

#### Scenario: New activity interrupts decay
- **WHEN** the state entered `Completed` 1000ms ago and a `Busy` input arrives
- **THEN** the state SHALL transition to `Working`

### Requirement: Hysteresis Transition Lock
The system SHALL apply a 500-millisecond transition lock after any `Working` ↔ `Waiting` transition, during which the reverse-direction transition is suppressed, to prevent oscillation between working and waiting on rapid output gaps.

#### Scenario: Reverse transition suppressed within lock window
- **WHEN** the state transitioned from `Working` to `Waiting` 200ms ago and a `Busy` input arrives
- **THEN** the transition back to `Working` SHALL be suppressed
- **AND** the state SHALL remain `Waiting`

#### Scenario: Reverse transition allowed after lock window
- **WHEN** the state transitioned from `Working` to `Waiting` 600ms ago and a `Busy` input arrives
- **THEN** the transition back to `Working` SHALL be permitted

### Requirement: Per-Pane Lifecycle State Store
The system SHALL maintain a `HashMap<String, PaneAgentLifecycle>` keyed by `terminal_id` on the desktop app state, storing the current lifecycle state and hysteresis metadata for each terminal pane. Entries SHALL be created when an AI session binds to a terminal and removed when the session unbinds, the pane closes, or the project switches.

#### Scenario: New pane gets lifecycle tracking
- **WHEN** an AI session binds to a terminal pane with `terminal_id` `"gpui-term-proj-1-abc"`
- **THEN** an entry SHALL be created in the lifecycle map for that `terminal_id`

#### Scenario: Closed pane removes lifecycle tracking
- **WHEN** a terminal pane with `terminal_id` `"gpui-term-proj-1-abc"` is closed
- **THEN** the corresponding entry SHALL be removed from the lifecycle map

#### Scenario: Unbound session removes lifecycle tracking
- **WHEN** the AI session bound to a terminal disappears from the runtime inventory while the pane stays open
- **THEN** the corresponding entry SHALL be removed from the lifecycle map
- **AND** the overlay chip SHALL be hidden for that pane

#### Scenario: Project switch clears stale entries
- **WHEN** the user switches from project A to project B
- **THEN** all lifecycle entries from project A's terminal ids SHALL be removed
- **AND** only project B's terminal ids SHALL remain
