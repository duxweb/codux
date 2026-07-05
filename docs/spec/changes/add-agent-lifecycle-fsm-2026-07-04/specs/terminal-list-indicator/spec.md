## ADDED Requirements

### Requirement: Terminal Row Lifecycle Indicator
The system SHALL render an agent lifecycle status indicator on each terminal row in the task column sidebar (`terminal_compact_row` in `task_column.rs`) when an AI session is bound to that row's terminal. The indicator lets the user tell at a glance which terminals need attention without opening each pane. The terminal row already shows the agent tool name and model as its subtitle (via `terminal_ai_titles_by_terminal_id`); the indicator adds only the status dot. No floating overlay is rendered on the terminal panes themselves — the pane content already makes the agent's status visible when open.

#### Scenario: Indicator appears when agent session is bound
- **WHEN** a terminal row's terminal has a bound AI session and a lifecycle entry
- **THEN** a status dot SHALL be rendered on that row
- **AND** the terminal pane itself SHALL NOT render any floating status overlay

#### Scenario: No indicator without agent session
- **WHEN** a terminal row's terminal has no bound AI session (plain shell)
- **THEN** no status dot SHALL be rendered on that row

#### Scenario: Collapsed pane rows show the indicator
- **WHEN** a collapsed pane's terminal has a bound AI session with a non-`Idle` lifecycle state
- **THEN** the row SHALL render the lifecycle status dot in place of the static green collapsed marker
- **AND** the row's terminal icon SHALL be tinted with the lifecycle color
- **WHEN** the collapsed pane's lifecycle is `Idle` or no session is bound
- **THEN** the row SHALL keep the static green collapsed marker

### Requirement: Status Dot Visual States
The system SHALL render the row status dot according to the terminal's current `AgentLifecycleState`:
- `Working` → animated spinning dot in blue accent (`theme::ACCENT`)
- `Waiting` → static dot in amber (`theme::ORANGE`)
- `Completed` → green checkmark; it disappears when the state decays to `Idle` (3 seconds, per the agent-lifecycle Completed Decay requirement)
- `Idle` → no dot

#### Scenario: Working state shows spinning blue dot
- **WHEN** the row's `AgentLifecycleState` is `Working`
- **THEN** the status dot SHALL be blue (`theme::ACCENT`)
- **AND** the dot SHALL be animated (spinning or pulsing)

#### Scenario: Waiting state shows static amber dot
- **WHEN** the row's `AgentLifecycleState` is `Waiting`
- **THEN** the status dot SHALL be amber
- **AND** the dot SHALL be static (no animation)

#### Scenario: Completed state shows brief green checkmark
- **WHEN** the row's `AgentLifecycleState` transitions to `Completed`
- **THEN** a green checkmark SHALL appear on the row
- **AND** when the state decays to `Idle` after 3 seconds the checkmark SHALL disappear

### Requirement: Terminal Row Icon Tint
The system SHALL tint the terminal row's leading terminal icon (`HeroIconName::CommandLine`) with the lifecycle state color while the agent is not `Idle`: blue (`theme::ACCENT`) for `Working`, amber (`theme::ORANGE`) for `Waiting`, green (`theme::GREEN`) for `Completed`. When the state is `Idle` or no session is bound, the icon keeps its default muted color. This makes the row "light up" so the state is visible even at a glance from the icon alone.

#### Scenario: Icon lights up while agent works
- **WHEN** the row's `AgentLifecycleState` is `Working`
- **THEN** the row's terminal icon SHALL be tinted blue (`theme::ACCENT`)

#### Scenario: Icon returns to default when idle
- **WHEN** the row's lifecycle state is `Idle` or the session unbinds
- **THEN** the row's terminal icon SHALL render in its default muted color

### Requirement: Lifecycle-Driven Row Refresh
The system SHALL re-render the task column terminal rows whenever any pane's `AgentLifecycleState` changes — independent of whether the project-level activity summary (`ai_activity_project_states_changed`) changed. Lifecycle timer transitions (Completed decay, idle debounce) SHALL also be ticked and rendered on the periodic runtime tick even when no runtime supervisor events were drained.

#### Scenario: Row updates on pane-only state change
- **WHEN** a session's state changes (e.g. `responding` → `needsInput`) without changing the project-level activity summary
- **THEN** the task column SHALL be invalidated and the row indicator SHALL reflect the new state on the next render

#### Scenario: Completed checkmark decays without new events
- **WHEN** a pane is in `Completed` and no runtime events arrive for 3 seconds
- **THEN** the decay timer SHALL fire on a periodic tick and the checkmark SHALL disappear

### Requirement: Worktree Row Agent Indicator
The system SHALL render the same agent lifecycle status element (`agent_lifecycle_status_dot`) on each worktree row in the task column when any AI session attributed to that worktree has a non-`Idle` lifecycle state. Attribution: a session belongs to a worktree when `session.project_id == worktree.id`, or when the worktree is the default one and `session.project_id == worktree.project_id`. Aggregation across the worktree's sessions picks the highest-priority state: `Working` > `Waiting` > `Completed`. The element is separate from — and does not replace — the existing worktree activity dot (`worktree_activity_dot`), which is already used for coarse project activity and collapsed presentation.

#### Scenario: Worktree row shows working spinner
- **WHEN** any pane in a worktree has lifecycle state `Working`
- **THEN** the worktree row SHALL render the spinning working indicator
- **AND** the existing worktree activity dot SHALL remain unchanged

#### Scenario: No indicator when all panes idle
- **WHEN** no session attributed to the worktree has a non-`Idle` lifecycle state
- **THEN** the worktree row SHALL NOT render the lifecycle element

### Requirement: Live Git Counts While Agent Works
The system SHALL refresh the selected worktree's git summary (changes count, additions, deletions) on a fast throttled cadence (at most once per 5 seconds) while any pane's lifecycle state is `Working`, and once immediately when a pane transitions to `Completed`, so the task column's change counts track the agent's edits without waiting for the slow scheduled scan. These agent-driven refreshes SHALL NOT overwrite the status bar message.

#### Scenario: Counts update while agent edits
- **WHEN** an agent is `Working` in the selected worktree and modifies files
- **THEN** the worktree row's change counts SHALL update within ~5 seconds

#### Scenario: Final counts on completion
- **WHEN** a pane's lifecycle transitions to `Completed`
- **THEN** a git summary refresh SHALL run immediately (once)

### Requirement: Motion Reduction Support
The system SHALL disable the spinning/pulsing animation on the status dot when the operating system's "reduce motion" accessibility setting is enabled, showing a static dot instead.

#### Scenario: Animation disabled when reduce motion is on
- **WHEN** the system "reduce motion" setting is enabled and the row state is `Working`
- **THEN** the status dot SHALL be static blue (no animation)
