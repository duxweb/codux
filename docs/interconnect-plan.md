# Multi-device interconnect (incl. headless Linux) — plan & progress

## Context

Codux should let one desktop connect to other Codux devices — another desktop, or a
**headless agent (including Linux)** — and run a project's terminals/files/Git/AI against
that remote device's data, driven from the local desktop UI. The remote stack (Iroh
transport, host capability surface, client consumer model, generic device↔device pairing)
was already built toward this. This is the "land it" milestone.

## Decisions (set by the product owner)

1. **Order: headless Linux agent first.** Prioritize a real shared host runtime the
   headless agent can run, so a Linux/headless box can host. (Desktop-as-client is still
   required to *use* it, and is built alongside.)
2. **Add-project can create a new directory on the host** (not just select existing) →
   the host protocol gains a `mkdir` (directory create) message.
3. **Desktop pairing = paste a ticket/code** (desktops have no camera).
4. **Full parity: AI stats + memory also run on the remote/headless host** — the heaviest
   extraction (AI history indexer + memory pipeline are currently desktop-wired).

## Architecture

Peer model: every device (Mac / Windows / Linux-headless / mobile) is a peer. A **host**
publishes runtime resources (project/terminal/file/Git/worktree/AI); a **controller**
(client) consumes them. The desktop must be **both** (host for its own projects + serving
the phone, AND a controller of remote hosts). The transport, the wire protocol, the host
capability surface, the client consumer model (`FfiRemoteRuntimeModel`), and pairing are
all already in shared Rust — no new protocol is needed for the core flow.

The one catch: the stateful host runtime (`RemoteHostRuntime` + `ProjectStore` /
`TerminalManager` / `WorktreeService` / `AIHistoryIndexer` / memory) lives in
`apps/desktop/runtime`, not a shared crate. It is GPUI-free but desktop-crate-bound, so the
headless agent cannot link it as-is.

## Host serve-loop pattern (template for every domain)

`apps/agent/src/host.rs` establishes the pattern, already working for `host.info` /
`file.list` / `file.read`:

- `connect_host(config, handler, …)` returns `Arc<dyn RemoteTransport>` (Clone; has
  `send(bytes, device_id)` / `iroh_candidate` / `iroh_endpoint_ticket` / `shutdown`).
- The handler is stored a transport slot `Arc<Mutex<Option<Arc<dyn RemoteTransport>>>>`,
  populated *after* connect (solves the chicken-and-egg of handler-needs-transport).
- On each envelope `{type, deviceId, requestId, payload}`: dispatch on `type`, build the
  reply payload via the **stateless `codux-runtime-core` builders** (`file_list_payload`,
  `host_info_payload`, …), echo the request `type` with `{deviceId, requestId, payload}`,
  and `transport.send(bytes, device_id)`.

Every remaining domain (terminal/Git/worktree/project/AI) plugs into this same dispatch —
the only new work per domain is the *stateful service* behind the payload builder.

## Phases

### P1 — Shared host runtime (critical path; heaviest)
Move `RemoteHostRuntime` + its services into shared crates so the agent can run the full
host. Recommended **leaner extraction** (not big-bang): define host-facing traits in
`codux-runtime-core` (`ProjectStore` / `TerminalManager` / `Worktree` / `AiStats` /
`Memory`), make `host.rs` depend on traits instead of the `crate::` concretes, migrate
UI-free services one domain at a time, keep the desktop green after each. Domain order:
file/git (done, pure) → ProjectStore/Worktree → terminal (reuse `codux-terminal-pty`) →
**AI history indexer** → **memory** (needs an LLM provider configured on the agent). Then
re-home `RemoteHostRuntime` into a shared `codux-host` crate; desktop supplies its adapters
(AI runtime, pet-XP), agent supplies plain impls.

### P2 — Headless agent host + Linux  ← STARTED
`apps/agent` instantiates the shared host runtime, persists host identity (`host_secret_key`)
+ a pairing allowlist, and prints a pairing ticket. Linux: shared crates already compile
(portable_pty / iroh / Unix shell paths handled, no `compile_error`); the work is CI + a
release artifact.

### P3 — Desktop-as-client
`remote/controller.rs` using `connect_controller` + reusing `FfiRemoteRuntimeModel`. Promote
the **client pairing** logic (currently Dart-only in `remote_protocol_service.dart`) into
Rust; **paste-ticket** entry. Client-side saved-host store (Rust mirror of mobile
`StoredDevice`, distinct from the host-side `cached_devices` allowlist). GPUI device/connect
UI; render remote projects/terminals/files/Git via the P4 routing fork.

### P4 — Device-aware project model + routing
Add `host_id: Option<String>` (`None` = local) to `ProjectRecord`
(`project_store/types.rs`), `ProjectInfo` (`runtime_state/types.rs`), and the
`ProjectListItem` DTO (`codux-runtime-core/src/project.rs`), all `#[serde(default)]` so
existing state/mobile payloads are unchanged. Branch `RuntimeService`'s file/git/terminal/AI
paths on it (local impl vs controller transport).

### P5 — Add-project UX
Device chooser before `open_project_folder_from_dialog` / `choose_project_editor_directory`
(`apps/desktop/src/app/project_actions.rs`). Local → native dialog. Remote → directory
browser over `file.list`, a **new mkdir** message for "create new folder on host", then
`project.add`. Reference: mobile `remote_file_picker.dart` / `remote_project_controller.dart`.

(Linux *GPUI desktop* is a separate, later track: UI shims `dialog/app_icon/dock_badge/
project_open/app_info/update` have no Linux arm, `rfd` is macOS/Windows-only, GPUI
x11/wayland unproven.)

## Status

- **Done — agent serves every mirror-able domain over Iroh** (`apps/agent/src/host.rs`,
  all proven by `--serve-smoke` controller→host round trips):
  - `host.info`
  - `file.*` — list / read / write / rename / delete / **createDirectory** (mkdir)
  - `project.*` — list / add / remove (`AgentProjectStore`, JSON at the agent data dir)
  - `terminal.*` — create / input / resize / close + async `terminal.output` streaming
    (real PTYs via `codux-terminal-pty`)
  - `git.status` — lean git2 reader (`apps/agent/src/git.rs`); git2 on the agent binary
    only, never the shared crates, so the mobile FFI stays clear of libgit2
  - `ai.stats` — the AI usage engine was extracted into the shared **`codux-ai-history`**
    crate (normalized parsers + SQLite usage store + background indexer, GPUI-free); the
    desktop re-exports it under the old module paths, the agent opens it against its data
    dir and serves a single-reply usage snapshot (`apps/agent/src/ai_stats.rs`).

  These cover every domain the **desktop host actually serves remotely today** — the agent
  controlled-end is at parity with the desktop host.

- **Memory is deliberately NOT a controlled-end mirror.** There is *no remote memory
  protocol* anywhere (not in `codux-protocol`, not desktop↔desktop, not desktop↔mobile), so
  there is nothing to mirror. And the memory engine's core flow — extraction/apply/profile —
  selects an **AI provider with an API key** from `crate::settings::AISettings` to run LLM
  completions. Serving memory on the headless host therefore requires two things that don't
  exist yet: (a) a net-new remote memory protocol *and a consumer*, and (b) **model routing
  to the host (P4)** so extraction has a provider. Building it before those lands would be
  speculative scaffolding with no consumer. → **Memory moves into P4**, sequenced after
  model routing, not into the controlled-end grind. (The host's stored memory *files* —
  `memory-workspaces/MEMORY.md` and entries — are already reachable today via the `file.*`
  domain.)

- **P3 started — desktop controller runtime (done, tested).** The desktop was host-only; it
  now has the dial-out side: `RemoteTransportFactory::connect_controller` + a `RemoteController`
  (`remote/controller.rs`) that drives a remote host's domains over Iroh, the inverse of
  `RemoteHostRuntime`. Replies correlate by message type via a FIFO waiter list (the proven
  mobile/agent scheme); the request path is synchronous so it composes with the synchronous
  `RuntimeService` domain methods. Typed helpers for host.info / file.list / createDirectory /
  git.status / ai.stats / project.list; correlation unit-tested.

- **P4 started — device-aware project model (done, tested).** `host_device_id: Option<String>`
  now threads through the persisted `ProjectRecord`, the create/update DTOs, the raw record
  builder + mutations, the runtime `ProjectInfo`, and the state loader. `None` = local (today);
  `Some(id)` will route the project's domains remotely and mark it remote in the sidebar.
  Backward-compatible; round-trips through `state.json` (tested).

- **P3 pairing + saved-host store (done, tested).** Pairing reuses the host's existing
  handshake (`pairing.request` → operator `pairing.confirmed`/`pairing.rejected`), driven from
  the controller side exactly like the mobile flow: `parse_pairing_ticket` decodes a pasted
  `codux://pair` ticket, `RemoteController::pair` connects unpaired (self-minted device id,
  empty token, ticket-only iroh candidate) and awaits confirmation, `RemoteControllerStore`
  persists the resulting `SavedRemoteHost`, and `RemoteController::connect_saved` reconnects
  without re-pairing (the host caches the device id). Ticket parsing, the confirmed-payload
  mapping, and the store round trip are unit-tested.

- **Agent pairing (done, tested).** The headless agent speaks the host pairing handshake and
  auto-confirms (reaching it means the controller holds the iroh ticket — the real gate), so a
  desktop can pair with an agent, not only another desktop. `--serve` prints a pasteable
  `codux://pair` ticket; the serve-smoke drives the pairing round trip.

- **Controller manager + remote browse (done, tested).** `RemoteControllerManager` pools live
  `RemoteController` connections (keyed by device id), lazy-connects from the saved store, and
  bridges the async API into sync `RuntimeService` methods via `block_on`. First real routing
  consumers on `RuntimeService`: `pair_remote_host`, `saved_remote_hosts`, `forget_remote_host`,
  `remote_browse_directory`, `remote_create_directory`, `remote_host_info` — these back the
  add-project remote flow. **Two end-to-end tests over real in-process iroh** prove the whole
  loop: pair the controller against a real host and drive `host.info`; and paste a ticket →
  pair (persist + cache) → browse a remote directory through the manager. (Run with
  `cargo test -p codux-runtime -- --ignored controller`.)

- **Next (ordered), each its own green commit:**
  1. **GPUI UI** — the backend for this is done and validated. (a) Paste-ticket pairing dialog
     → `pair_remote_host`. (b) Add-project: a "device" picker (`saved_remote_hosts`) + a remote
     directory browser over `remote_browse_directory` / `remote_create_directory`, setting the
     new project's `host_device_id`. (c) Sidebar remote marker on the per-project `.relative()`
     icon wrapper in `project_column.rs` (driven by `ProjectInfo::host_device_id`). This is
     where product/UX decisions live, so it's the natural next checkpoint with the owner.
  2. **Hosted-project domain routing** — for a project whose `host_device_id` is set, route its
     file/git/terminal/ai through the manager's controller. Per domain needs a payload→struct
     mapping (host returns runtime-core JSON; desktop UI wants `GitSummary`/`FileEntry`), so
     it's real work per domain, not a blanket wrap. The manager + `controller_for` make each a
     thin addition; validate each against the agent (`cargo run -p codux-agent -- --serve`).
  3. **Memory over remote (P4 tail)** — net-new memory protocol + host serving, riding on the
     model routing from step 2.

## Verification

- Headless host today: `just smoke` (includes `--serve-smoke`), or run `cargo run -p
  codux-agent -- --serve` and connect a controller.
- Per domain: extend the serve round-trip to that message (controller issues e.g.
  `terminal.create` / `git.status`, assert the reply), then a manual desktop-client run.
