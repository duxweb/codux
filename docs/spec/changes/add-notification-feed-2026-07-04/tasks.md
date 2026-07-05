---
created_at: 2026-07-04T00:00:00Z
updated_at: 2026-07-04T00:00:00Z
completed_at:
---

## 1. Data Model + Classifier

- [ ] 1.1 Create `apps/desktop/src/app/notifications/mod.rs` module file (re-export public types)
- [ ] 1.2 Define `NotificationSemantic` enum (`Actionable | Completed | Active`) with `Clone, Copy, Debug, PartialEq, Eq, Hash` derives in `notifications/types.rs`
- [ ] 1.3 Define `NotificationStatus` enum (`Unread | Read | Resolved`) with derives in `notifications/types.rs`
- [ ] 1.4 Define `NotificationItem` struct (`id, terminal_id, tool, kind, semantic, title, project_name, created_at, status`) in `notifications/types.rs`
- [ ] 1.5 Implement `NotificationItem::from_runtime_event(event: &RuntimeEventItem) -> Self` in `notifications/types.rs` (maps fields, calls `classify`, sets title fallback, status = `Unread`)
- [ ] 1.6 Implement `classify(event: &RuntimeEventItem) -> NotificationSemantic` in `notifications/classify.rs` (exhaustive match on `event.state`)
- [ ] 1.7 Write unit tests for `classify` covering all four state-string cases (`needs-input`, `completed`, `running`, unknown)
- [ ] 1.8 Register `mod notifications;` in `apps/desktop/src/app.rs`

## 2. Feed Store

- [ ] 2.1 Define `NotificationFeed` struct (`VecDeque<NotificationItem>` cap 500, `HashSet<String>` seen) in `notifications/feed.rs`
- [ ] 2.2 Implement `NotificationFeed::default()` (empty store)
- [ ] 2.3 Implement `ingest(&mut self, item: NotificationItem) -> bool` (dedupe by id, prepend newest-first, evict oldest on overflow)
- [ ] 2.4 Implement `items() -> &[NotificationItem]`, `unread_actionable_count() -> usize`
- [ ] 2.5 Implement `mark_read(&mut self, id: &str)` (find by id, set status `Read`)
- [ ] 2.6 Implement `resolve_for_terminal(&mut self, terminal_id: &str)` (set all `Actionable`/`Completed` + `Unread`/`Read` items for that terminal to `Resolved`)
- [ ] 2.7 Implement `seed_from_events(&mut self, events: &[RuntimeEventItem])` (ingest each, ignoring duplicates)
- [ ] 2.8 Write unit tests: dedupe, prepend-order, capacity eviction, unread count, mark_read, resolve_for_terminal

## 3. GPUI Event Bus Wiring

- [ ] 3.1 Define `NotificationFeedUpdateEvent { revision: u64 }` in `app_events.rs`
- [ ] 3.2 Add `static NOTIFICATION_FEED_UPDATE_EVENT: OnceLock<Mutex<NotificationFeedUpdateEvent>>` + `notification_feed_update_event()` accessor
- [ ] 3.3 Implement `current_notification_feed_update_event() -> NotificationFeedUpdateEvent`
- [ ] 3.4 Implement `publish_notification_feed_update() -> u64` (increment + return revision)
- [ ] 3.5 Add `notification_feed: NotificationFeed` field to `AppState` in `app_state.rs`
- [ ] 3.6 Initialize `notification_feed: NotificationFeed::default()` in `app_lifecycle.rs`

## 4. Ingestion From Runtime Event Loop

- [ ] 4.1 Locate the event-decoding site inside `start_runtime_events_loop` (in `runtime_actions.rs`) where `RuntimeEventService::summary()` is called
- [ ] 4.2 After summary is obtained, for each `recent_event`: construct `NotificationItem::from_runtime_event`, call `self.state.notification_feed.ingest(item)`, collect a bool indicating any new ingestion
- [ ] 4.3 If any new ingestion occurred, call `publish_notification_feed_update()`
- [ ] 4.4 Implement startup seeding: in app init (after `AppState` is built), call `feed.seed_from_events(&summary.recent_events)` before the polling loop starts
- [ ] 4.5 Implement auto-resolve: after ingesting a new event, if its semantic is `Active` and a prior `Actionable`/`Completed` item exists for the same `terminal_id`, call `feed.resolve_for_terminal(&terminal_id)`

## 5. Toolbar Bell Button

- [ ] 5.1 Define `NotificationBellSnapshot { notification_revision: u64, unread_count: usize }` (Clone + PartialEq) in `workspace_toolbar.rs` (or a new `notifications/view.rs`)
- [ ] 5.2 Implement `CoduxApp::notification_bell_snapshot(&self) -> NotificationBellSnapshot` (reads `current_notification_feed_update_event().revision` + `feed.unread_actionable_count()`)
- [ ] 5.3 Add the snapshot to the workspace toolbar view's diff check; call `cx.notify()` on change
- [ ] 5.4 Implement `workspace_notification_bell_button(...)` returning an element: `Button::new("workspace-notification-bell").ghost()` + `Icon::new(HeroIconName::Bell)` + conditional badge overlay div when `unread_count > 0`
- [ ] 5.5 Wire the bell into `workspace_toolbar()` right cluster, immediately before the settings access point
- [ ] 5.6 Add a localized tooltip via `with_codux_tooltip()`

## 6. Popup + Jump

- [ ] 6.1 Implement the popup as `.dropdown_menu(move |menu, window, cx| { ... })` on the bell button
- [ ] 6.2 Inside the menu builder: read `feed.items()`, filter out `Active` (unless show-all toggle), take up to 20, map each to a `PopupMenuItem`
- [ ] 6.3 Each `PopupMenuItem`: label = truncated title + relative time; icon = semantic indicator (`BellAlert` for Actionable, `CheckCircle` for Completed); `on_click` closure captures `app_entity` + `item.id` + `item.terminal_id`
- [ ] 6.4 In the `on_click`: call `app.set_active_terminal_runtime_id(Some(&terminal_id))`, then `app.state.notification_feed.mark_read(&id)`, then `publish_notification_feed_update()`
- [ ] 6.5 Add empty-state: if no `Actionable`/`Completed` items, emit one disabled `PopupMenuItem` with localized "No notifications"
- [ ] 6.6 Add "N more…" disabled footer item when total qualifying items exceed 20
- [ ] 6.7 Implement relative-time label helper (e.g., "just now", "5m ago", "2h ago") reusing existing time-formatting helpers if present

## 7. Localization

- [ ] 7.1 Add translation keys for: bell tooltip ("Notifications"), empty state ("No notifications"), "N more…" footer, "Show all" toggle
- [ ] 7.2 Wire all user-visible strings through `translate()` / `workspace_i18n()` with the current `settings.language`

## 8. Validation

- [ ] 8.1 `cargo check` passes
- [ ] 8.2 `cargo clippy` passes (no new warnings)
- [ ] 8.3 Unit tests pass: classifier, feed store (dedupe, eviction, counts, mark_read, resolve), snapshot diff
- [ ] 8.4 Manual verification: trigger an agent event (e.g., a `needs-input` state), confirm the bell badge appears, open popup, click row, confirm the terminal focuses and badge decrements
