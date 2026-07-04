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

### Requirement: Motion Reduction Support
The system SHALL disable the spinning/pulsing animation on the status dot when the operating system's "reduce motion" accessibility setting is enabled, showing a static dot instead.

#### Scenario: Animation disabled when reduce motion is on
- **WHEN** the system "reduce motion" setting is enabled and the row state is `Working`
- **THEN** the status dot SHALL be static blue (no animation)
