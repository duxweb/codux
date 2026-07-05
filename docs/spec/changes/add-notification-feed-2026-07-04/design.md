## Context

Codux already decodes AI agent hook events into `RuntimeEventItem` (tool, kind, state ∈ {running, needs-input, completed}, project_name, terminal_id, session_title, updated_at, modified_at) via `RuntimeEventService` in `apps/desktop/runtime/src/runtime_event.rs`. The active `add-agent-lifecycle-fsm` change adds per-pane lifecycle state for overlay rendering. What is missing is an **aggregate, reviewable, jump-from** surface — a notification feed.

The sibling project cmux implements this as a Feed panel (`Sources/Feed/`). Its transferable architectural decisions (informed by their `FeedEventClassifier`, `WorkstreamStore`, `FeedRowActions`, and `FeedJumpResolver`) are adapted below to Codux's GPUI + Rust stack. The key simplification: cmux needs a sidecar JSON file to resolve a `workstreamId` to a workspace/surface because AppKit decouples the Feed from the TerminalController. Codux does not — GPUI has direct access to `set_active_terminal_runtime_id(terminal_id)`, so the jump is a single method call with no indirection.

The user has specified the UX: a bell button in the toolbar (above/adjacent to the settings gear) that opens a popup, NOT a sidebar panel.

## Goals / Non-Goals

- Goals:
  - Collective review of all agent events across terminals in one popup
  - Unread badge on a toolbar bell drawing attention to actionable items
  - One-click jump from a notification row to the owning terminal pane
  - Typed classifier as single source of truth (prevents "is this noise?" bugs)
  - Follows existing Codux patterns: `app_events.rs` revision counters, `dropdown_menu` popup, snapshot-diff re-render
- Non-Goals:
  - OS-level notification banners (the existing `notification/` module handles outbound push; native banners are out of scope)
  - Blocking/interactive approval cards inside the popup (cmux's permission/plan/question cards) — Codux agents are non-blocking in this layer; the popup is review + jump only
  - JSONL persistence / paged history (in-memory only for v1; the event dir already persists raw events and the store rebuilds on startup)
  - Custom rich popover panel (v1 uses `PopupMenu` / `dropdown_menu`; a bespoke panel is a future enhancement if the menu proves too constrained)

## Decisions

### D1: Semantic classifier as single source of truth
Port cmux's `FeedEventClassifier` idea: one pure function `classify(event: &RuntimeEventItem) -> NotificationSemantic` maps `(state, kind)` to `Actionable` (needs-input), `Completed` (turn done), or `Active` (running / telemetry). Every consumer (badge count, popup filter, auto-resolve) reads this enum, never the raw strings.

- Decision: typed enum + exhaustive match.
- Alternatives considered: string matching on `state` scattered across call sites (rejected — fragile, bug-prone as seen in cmux #4985).

### D2: In-memory ring buffer, no separate persistence layer
The store is a `VecDeque<NotificationItem>` with capacity 500. On startup it seeds from `RuntimeEventService::summary().recent_events` (already decoded, top 10 by recency). During the session, the existing `start_runtime_events_loop` polls the event dir; the feed ingests newly-seen file names (tracked in a `HashSet<String>`).

- Decision: in-memory only; rebuild from event dir on launch.
- Alternatives considered: JSONL append log like cmux's `WorkstreamPersistence` (rejected for v1 — adds a file-format surface and paging UI that the popup doesn't need yet; the event dir is already the durable source).

### D3: Jump = direct method call, no sidecar indirection
Each `NotificationItem` carries `terminal_id`. Row click calls `set_active_terminal_runtime_id(Some(item.terminal_id))` (existing method on `CoduxApp` at `terminal_worktree_actions.rs:2052`), then marks the item `Read`.

- Decision: direct pane focus via existing primitive.
- Alternatives considered: cmux's sidecar-JSON resolver pattern (rejected — that pattern exists to bridge AppKit/Controller isolation; GPUI has no such gap).

### D4: Popup via `dropdown_menu` / `PopupMenu` for v1
The app already uses `Button::new(...).dropdown_menu(|menu, ...| ...)` (see `workspace_open_button` in `workspace_toolbar.rs`). Each notification becomes a `PopupMenuItem` with an icon, a label (project + session title), and an `on_click` that jumps + marks read.

- Decision: reuse `PopupMenu` for v1.
- Alternatives considered: a bespoke floating panel element (rejected for v1 — heavier implementation; revisit if the menu's single-line-per-item constraint limits readability for long session titles).

### D5: GPUI re-render via new `NotificationFeedUpdateEvent`
Add `NotificationFeedUpdateEvent { revision: u64 }` to `app_events.rs` using the established `&'static Mutex<...>` + `publish_*()` / `current_*_event()` pattern. The bell button's snapshot includes `notification_revision`; the toolbar view diffs and calls `cx.notify()` on change.

- Decision: extend the existing event-bus pattern.
- Alternatives considered: GPUI entity observation directly on the feed store (rejected — inconsistent with the 6 existing subsystems that all use the revision-counter pattern; matching conventions reduces cognitive load).

### D6: Auto-resolve on state transition
When the polling loop decodes an event whose `state` has moved past `needs-input` or `completed` for a given `terminal_id` (i.e., a later event shows `running` again, or the session disappears from the summary), existing `Actionable`/`Completed` items for that terminal are marked `Resolved`. Resolved items stay in the popup list (for review) but drop out of the unread badge.

- Decision: derive resolve from subsequent events, no timers.
- Alternatives considered: timer-based expiry like cmux's kqueue PID watcher (rejected — Codux doesn't fork agent processes the same way; the event stream itself signals state changes).

## Risks / Trade-offs

- **Popup menu readability**: `PopupMenu` rows are single-line; long session titles may truncate. Mitigation: v1 truncates with ellipsis + tooltip; if insufficient, escalate to a bespoke panel in a follow-up change.
- **Polling cadence**: the feed depends on `start_runtime_events_loop`'s poll interval. If it's too slow, notifications lag. Mitigation: verify the existing interval during implementation; the feed adds no polling of its own.
- **Event dir churn**: on busy sessions with many tool calls, the feed could fill with `Active` telemetry. Mitigation: the classifier + popup default filter shows `Actionable` first; `Active` items are suppressed from the default view (matching cmux's `.actionable` filter).
- **Store capacity eviction**: at 500 items, oldest are evicted. This is acceptable for a review surface (not an audit log); the raw events remain on disk in the event dir.

## Migration Plan

No migration — this is a purely additive feature. No existing data formats, IPC messages, or settings keys change. The `notification/` outbound-dispatch module is untouched and unrelated.

## Open Questions

1. **Exact toolbar placement**: "above the gear" — the settings gear appears via `settings_icon_button_state` and the `Cog6Tooth` entry points. Implementation will confirm the precise anchor element in the current chrome and place the bell immediately before it in the toolbar's right cluster. (Non-blocking; resolved at implementation time.)
2. **Popup max rows**: `PopupMenu` may not paginate. If the feed exceeds ~20 visible rows, v1 truncates to the most recent 20 with a "N more…" disabled footer item. Acceptable?
