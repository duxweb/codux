## ADDED Requirements

### Requirement: Notification Semantic Enum
The system SHALL define a `NotificationSemantic` enum with three variants — `Actionable`, `Completed`, `Active` — classifying the urgency of a notification derived from a `RuntimeEventItem`. `Actionable` means the agent needs user input (`needs-input` state); `Completed` means a turn finished (`completed` state); `Active` means the agent is working (`running` state) and is telemetry-only. This enum is the single source of truth for urgency and SHALL be used by the unread badge, the popup filter, and the auto-resolve logic — never the raw `state` string.

#### Scenario: Enum variants cover all semantic urgencies
- **WHEN** the `NotificationSemantic` enum is defined
- **THEN** it SHALL include exactly: `Actionable`, `Completed`, `Active`
- **AND** each variant SHALL derive `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `Hash`

### Requirement: Notification Semantic Classifier
The system SHALL provide a pure function `classify(event: &RuntimeEventItem) -> NotificationSemantic` that maps the event's `state` field to a semantic variant via an exhaustive match. The mapping SHALL be: `"needs-input"` → `Actionable`, `"completed"` → `Completed`, `"running"` (or any other value) → `Active`. This function is the only place raw state strings are interpreted.

#### Scenario: Needs-input event is actionable
- **WHEN** `classify` receives a `RuntimeEventItem` whose `state` is `"needs-input"`
- **THEN** it SHALL return `NotificationSemantic::Actionable`

#### Scenario: Completed event signals turn done
- **WHEN** `classify` receives a `RuntimeEventItem` whose `state` is `"completed"`
- **THEN** it SHALL return `NotificationSemantic::Completed`

#### Scenario: Running event is telemetry
- **WHEN** `classify` receives a `RuntimeEventItem` whose `state` is `"running"`
- **THEN** it SHALL return `NotificationSemantic::Active`

#### Scenario: Unknown state defaults to active telemetry
- **WHEN** `classify` receives a `RuntimeEventItem` whose `state` is any value not listed above
- **THEN** it SHALL return `NotificationSemantic::Active`

### Requirement: Notification Item Model
The system SHALL define a `NotificationItem` struct representing a single decoded agent event for the feed. It SHALL carry: `id: String` (derived from the source event file name, stable across polls), `terminal_id: String`, `tool: String`, `kind: String`, `semantic: NotificationSemantic`, `title: String` (the session title, falling back to the project name), `project_name: String`, `created_at: f64` (the event's `modified_at` timestamp), and `status: NotificationStatus`.

#### Scenario: Item fields are populated from a runtime event
- **WHEN** a `NotificationItem` is constructed from a `RuntimeEventItem`
- **THEN** its `id` SHALL equal the event's `file_name`
- **AND** its `terminal_id`, `tool`, `kind`, `project_name` SHALL be copied from the event
- **AND** its `semantic` SHALL be the result of `classify(&event)`
- **AND** its `title` SHALL be the event's `session_title`, or the `project_name` if the session title is empty
- **AND** its `created_at` SHALL equal the event's `modified_at`
- **AND** its `status` SHALL be `NotificationStatus::Unread`

### Requirement: Notification Status Lifecycle
The system SHALL define a `NotificationStatus` enum with three variants — `Unread`, `Read`, `Resolved` — tracking the lifecycle of a notification item. New items are `Unread`. Clicking a row in the popup transitions the item to `Read`. A subsequent runtime event for the same `terminal_id` whose semantic differs from an existing `Actionable` or `Completed` item transitions that item to `Resolved`. Only `Unread` items with semantic `Actionable` contribute to the unread badge count.

#### Scenario: New item starts unread
- **WHEN** a `NotificationItem` is created
- **THEN** its `status` SHALL be `Unread`

#### Scenario: Row click marks read
- **WHEN** the user clicks a notification row in the popup whose item is `Unread`
- **THEN** the item's `status` SHALL transition to `Read`
- **AND** the unread badge count SHALL decrease by one if the item's semantic was `Actionable`

#### Scenario: Later event resolves an actionable item
- **WHEN** a new `NotificationItem` is ingested for a `terminal_id` that already has an `Unread` or `Read` item with semantic `Actionable`
- **AND** the new item's semantic is not `Actionable`
- **THEN** the existing item's `status` SHALL transition to `Resolved`

#### Scenario: Resolved items excluded from badge
- **WHEN** the unread badge count is computed
- **THEN** only items with `status == Unread` AND `semantic == Actionable` SHALL be counted

### Requirement: Notification Feed Store
The system SHALL provide a `NotificationFeed` struct holding an in-memory `VecDeque<NotificationItem>` with a maximum capacity of 500 items and a `HashSet<String>` of seen event file names. When capacity is exceeded, the oldest item SHALL be evicted. The store SHALL expose: `ingest(item: NotificationItem) -> bool` (returns true if the item was newly added, false if its `id` was already in the seen set), `items() -> &[NotificationItem]` (newest-first), `unread_actionable_count() -> usize`, `mark_read(id: &str)`, and `resolve_for_terminal(terminal_id: &str)`.

#### Scenario: Ingest deduplicates by event file name
- **WHEN** `ingest` is called with an item whose `id` is already in the seen set
- **THEN** the store SHALL return `false` and not append the item

#### Scenario: Ingest appends new items newest-first
- **WHEN** `ingest` is called with an item whose `id` is not in the seen set
- **THEN** the store SHALL add the `id` to the seen set, prepend the item to the front of the `VecDeque`, and return `true`

#### Scenario: Capacity eviction drops oldest
- **WHEN** the `VecDeque` contains 500 items and a new item is ingested
- **THEN** the oldest item (back of the `VecDeque`) SHALL be removed before the new item is prepended

#### Scenario: Unread actionable count excludes read and resolved
- **WHEN** `unread_actionable_count()` is called on a store containing 3 `Actionable` items (2 `Unread`, 1 `Read`) and 2 `Completed` items (both `Unread`)
- **THEN** it SHALL return `2`

### Requirement: Feed Ingestion From Runtime Event Loop
The system SHALL tap the existing runtime event polling loop (`start_runtime_events_loop`) to ingest newly-decoded events into the `NotificationFeed`. For each `RuntimeEventItem` produced by `RuntimeEventService::summary()`, the system SHALL construct a `NotificationItem`, call `feed.ingest(item)`, and if ingestion returned `true`, publish a `NotificationFeedUpdateEvent` revision bump so the toolbar re-renders. On application startup, the store SHALL seed from `summary().recent_events` before the polling loop begins.

#### Scenario: Startup seeds from recent events
- **WHEN** the application launches and the notification feed is initialized
- **THEN** the store SHALL ingest every item from `RuntimeEventService::summary().recent_events`
- **AND** each successfully-ingested item SHALL NOT trigger a duplicate on the first poll

#### Scenario: Poll ingests new events and publishes revision
- **WHEN** the runtime event loop decodes a new event file not yet in the feed's seen set
- **THEN** a `NotificationItem` SHALL be constructed and ingested
- **AND** if ingestion succeeds, `publish_notification_feed_update()` SHALL be called to bump the revision counter

#### Scenario: Already-seen events are skipped
- **WHEN** the runtime event loop decodes an event file whose name is already in the feed's seen set
- **THEN** no `NotificationItem` SHALL be constructed and no revision bump SHALL occur
