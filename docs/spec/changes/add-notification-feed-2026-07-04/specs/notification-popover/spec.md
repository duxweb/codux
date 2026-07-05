## ADDED Requirements

### Requirement: Notification Bell Toolbar Button
The system SHALL render a notification bell button (`HeroIconName::Bell`) in the workspace toolbar's right cluster, positioned immediately before the settings access point. The button SHALL be always visible (not gated on project selection). When the unread actionable count is greater than zero, the button SHALL display a numeric badge overlay with the count; when the count is zero, no badge SHALL be shown. Clicking the button SHALL open the notification popup.

#### Scenario: Bell rendered before settings gear
- **WHEN** the workspace toolbar is rendered
- **THEN** a bell button SHALL appear in the right cluster, immediately before the settings access point element

#### Scenario: Badge shows unread actionable count
- **WHEN** the notification feed's `unread_actionable_count()` returns a value greater than zero
- **THEN** the bell button SHALL display a badge overlay showing that number

#### Scenario: No badge when count is zero
- **WHEN** the notification feed's `unread_actionable_count()` returns zero
- **THEN** the bell button SHALL NOT display any badge overlay

#### Scenario: Click opens popup
- **WHEN** the user clicks the bell button
- **THEN** the notification popup SHALL open

### Requirement: Notification Popup Content
The system SHALL render the notification popup using the established `Button::dropdown_menu` / `PopupMenu` pattern. The popup SHALL list up to 20 most-recent notification items, newest-first. Each row SHALL show: a tool-derived icon, the item title (project + session title, truncated with ellipsis if needed), a semantic indicator (distinct icon or color for `Actionable` vs `Completed` vs `Active`), and a relative-time label. `Active` items SHALL be suppressed by default; only `Actionable` and `Completed` items appear unless the user toggles a "show all" affordance.

#### Scenario: Popup lists actionable and completed items newest-first
- **WHEN** the popup is opened and the feed contains 5 `Actionable`, 3 `Completed`, and 10 `Active` items
- **THEN** the popup SHALL list the 8 `Actionable` + `Completed` items ordered newest-first
- **AND** the 10 `Active` items SHALL NOT appear

#### Scenario: Popup truncates to 20 rows
- **WHEN** the popup is opened and the feed contains 30 `Actionable` + `Completed` items
- **THEN** the popup SHALL list the 20 most recent items
- **AND** a disabled footer item SHALL indicate that more items exist

#### Scenario: Empty state
- **WHEN** the popup is opened and the feed has zero `Actionable` or `Completed` items
- **THEN** the popup SHALL show a single disabled item with a localized "No notifications" message

#### Scenario: Row displays semantic indicator
- **WHEN** a notification row is rendered for an `Actionable` item
- **THEN** the row SHALL display a distinct indicator (e.g., a warning icon or accent color) differentiating it from `Completed` items

### Requirement: Jump To Terminal Pane
The system SHALL make each notification row in the popup clickable. Clicking a row SHALL call `set_active_terminal_runtime_id(Some(item.terminal_id))` on the `CoduxApp` entity to focus the owning terminal pane, SHALL mark the item `Read` via `feed.mark_read(&item.id)`, and SHALL publish a `NotificationFeedUpdateEvent` revision bump so the badge updates.

#### Scenario: Click focuses the owning terminal
- **WHEN** the user clicks a notification row whose item has `terminal_id` equal to `"term-42"`
- **THEN** `set_active_terminal_runtime_id(Some("term-42"))` SHALL be invoked
- **AND** the item's `status` SHALL transition to `Read`

#### Scenario: Click on item with unknown terminal is a no-op focus
- **WHEN** the user clicks a notification row whose `terminal_id` no longer maps to any live terminal pane
- **THEN** `set_active_terminal_runtime_id` SHALL be invoked but SHALL return `false` (no pane focused)
- **AND** the item's `status` SHALL still transition to `Read`

#### Scenario: Badge decrements after click
- **WHEN** the user clicks an `Actionable` + `Unread` row and the unread actionable count was 3
- **THEN** a `NotificationFeedUpdateEvent` SHALL be published
- **AND** the next render SHALL show the badge count as 2

### Requirement: Notification Feed Snapshot For Toolbar Re-render
The system SHALL compute a `NotificationBellSnapshot` (deriving `Clone`, `PartialEq`) carrying the current `notification_revision: u64` and `unread_actionable_count: usize`. The workspace toolbar view SHALL include this snapshot in its diff check; when the snapshot changes, the view SHALL call `cx.notify()` to trigger re-render of the bell button and badge.

#### Scenario: Revision bump triggers re-render
- **WHEN** `publish_notification_feed_update()` bumps the revision from 5 to 6
- **AND** the toolbar view recomputes its snapshot
- **THEN** the snapshot SHALL differ from the previous one
- **AND** `cx.notify()` SHALL be called

#### Scenario: Unchanged revision skips re-render
- **WHEN** the toolbar view recomputes its snapshot and both `notification_revision` and `unread_actionable_count` are unchanged
- **THEN** the snapshot SHALL equal the previous one
- **AND** `cx.notify()` SHALL NOT be called

### Requirement: Localization
The system SHALL localize all user-visible strings in the notification popup (row labels, empty-state message, "show all" toggle, "N more" footer, tooltip on the bell button) using the existing `translate()` / `workspace_i18n()` helpers, keyed by the current `settings.language`.

#### Scenario: Empty state message is localized
- **WHEN** the popup renders the empty state with `settings.language` set to `"zh-CN"`
- **THEN** the "No notifications" message SHALL be retrieved via `translate()` with a Chinese locale and a translation key

#### Scenario: Bell tooltip is localized
- **WHEN** the bell button renders its tooltip
- **THEN** the tooltip text SHALL come from `workspace_i18n()` with the current language setting
