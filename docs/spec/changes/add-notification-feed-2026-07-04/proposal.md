---
created_at: 2026-07-04T00:00:00Z
updated_at: 2026-07-04T00:00:00Z
---

## Why

Codux surfaces agent activity only at the per-pane level (the `add-agent-lifecycle-fsm` overlay chip) and in the truncated 10-item `recent_events` inside `RuntimeEventSummary`. There is no collective review surface where a user can see all agent events across every terminal at once and jump directly to the pane that needs attention. The sibling project cmux solves this with its Feed panel; Codux should offer the equivalent, adapted to its GPUI toolbar chrome as a bell-button popup.

## What Changes

- **New `NotificationItem` model** with a typed semantic classifier (`Actionable` / `Completed` / `Active`) derived from the existing `RuntimeEventItem`, carrying `terminal_id` for jump-to-pane routing
- **In-memory notification store** (`VecDeque`, capacity 500) fed by the existing runtime event polling loop (`start_runtime_events_loop`), diffing newly-decoded event files against already-seen file names to avoid duplicates
- **New `NotificationFeedUpdateEvent`** in `app_events.rs` following the established `&'static Mutex<XxxUpdateEvent>` revision-counter pattern, published whenever the store mutates
- **Toolbar bell button** placed in the workspace toolbar right cluster, immediately before the settings access point, showing an unread badge count of `Actionable + Unread` items
- **Dropdown popup** (using the established `Button::dropdown_menu()` / `PopupMenu` pattern) listing notification rows; each row shows tool icon + project/session title + semantic indicator + relative time, and clicking a row calls `set_active_terminal_runtime_id(Some(terminal_id))` to jump to the pane and marks the item `Read`
- **Auto-resolve**: when a terminal's runtime session state transitions out of `needs-input` / `completed`, the corresponding `Actionable` / `Completed` notifications for that terminal are marked `Resolved` (suppressed from the unread badge)
- No runtime crate changes; no new IPC messages; all data flows through the existing `RuntimeEventService` → `RuntimeEventItem` path and the `set_active_terminal_runtime_id` jump primitive

## Impact

- Affected specs: `notification-feed` (new), `notification-popover` (new)
- Depends on: `add-agent-lifecycle-fsm` (active) for per-pane state semantics — the feed consumes the same runtime event stream
- Affected code:
  - `apps/desktop/src/app/notifications/mod.rs` — new module (types, classifier, store)
  - `apps/desktop/src/app/notifications/types.rs` — `NotificationItem`, `NotificationSemantic`, `NotificationStatus`
  - `apps/desktop/src/app/notifications/classify.rs` — `classify(event: &RuntimeEventItem) -> NotificationSemantic`
  - `apps/desktop/src/app/notifications/feed.rs` — `NotificationFeed` store + `NotificationFeedUpdateEvent` wiring
  - `apps/desktop/src/app/app_events.rs` — add `NotificationFeedUpdateEvent` + accessors
  - `apps/desktop/src/app/workspace_toolbar.rs` — add bell button before the settings access point
  - `apps/desktop/src/app/runtime_actions.rs` — tap the event loop to ingest into the feed
  - `apps/desktop/src/app.rs` — register `notifications` module
  - `apps/desktop/src/app/app_state.rs` — add `notification_feed` state field
  - `apps/desktop/src/app/app_lifecycle.rs` — initialize new state field
- Estimated: ~450 LOC, all in `apps/desktop`
