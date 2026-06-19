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

- **GPUI add-project-remote flow (done).** (a) Sidebar remote marker on the per-project icon
  (`ProjectInfo::host_device_id`). (b) Project editor "Device" field: This Mac + a chip per
  paired host + a "Pair device…" inline form (paste `codux://pair` ticket → `pair_remote_host`).
  (c) When a remote device is selected, the directory "Choose" opens an inline remote browser
  (navigate/create folders on the host via `remote_browse_directory`/`remote_create_directory`),
  and `host_device_id` is saved with the project. JSON→struct mapping is done in the runtime
  (`RemoteController::browse_directory → RemoteDirectoryListing`) so the UI carries no wire JSON.

- **Hosted-project domain routing (file + git status done).** The seam is
  `host_device_for_project_path` + `RemoteControllerManager`; each `RuntimeService` domain
  method branches on it. Done:
  - **file (full)** — list / open / write / create file+dir / delete / rename all route to the
    host (`remote_*` in `service_remote_controller.rs`, `RemoteController` file methods).
    Enriched the shared `file_list_payload` with `size/modifiedAt/isSymbolicLink`; the UI works
    in project-relative paths, `remote_absolute_path` resolves to the host's absolute path.
  - **git status** — `reload_project_git` maps the host `git.status` into `GitSummary`
    (`git_summary_from_payload`); aligned the agent's `changed_files` to the host's
    `GitFileStatus` shape. A remote project's files are fully usable and its git status shows.

- **Terminal transport + router (done, tested).** Correction to the earlier note: the desktop
  `TerminalManager` is concrete local-PTY, NOT trait-based, and mobile is Dart — so a
  `TerminalDriver` impl is the wrong abstraction. Instead the desktop mirrors mobile: the
  `RemoteController` got a `terminal.output` subscription (`set_terminal_sink`) +
  `create_terminal`/`terminal_input`/`terminal_resize`/`close_terminal`, and output is assembled
  by `codux-terminal-core`'s `RemoteTerminalOutputRouter` — the same engine mobile uses via FFI.
  A 3rd end-to-end iroh test proves it (pair → create → input → router assembles output).

- **AI stats (done, tested).** Added a distinct `ai.state` message carrying the full
  `AIHistoryProjectState{snapshot}` (the desktop AI panel's shape), served by the agent and the
  desktop host (indexing the path the controller sends); the controller deserializes it (added
  `Deserialize` across the snapshot types) and `indexed_project_ai_history_summary/state` route
  to it. `ai.stats` (mobile's baseline view) is unchanged.

- **Terminal UI (done).** A remote-hosted project's terminals run on the host. `host_device_id`
  threads through the launch config; the attach chokepoint branches to
  `attach_pending_session_remote`, which opens the terminal on the host
  (`RemoteController::open_terminal`), forwards `terminal.output` bytes into the pane's existing
  byte channel (the model parses them like a local PTY — zero render changes), and routes
  input/resize through a Remote variant of `TerminalSessionBinding`. The controller demuxes
  output per session. (Two immediate-constructor paths — boot/float restore — remain local-only;
  small follow-up.) The transport/router path is e2e-tested; GUI pixels want a human glance.

- **Git operations (done).** Added the git operation protocol (stage/unstage/commit/discard/diff)
  and implemented it on **both** hosts (agent via `git2`; desktop host via `GitService`). The
  controller + `RuntimeService` git methods route remote projects to the host and map the
  refreshed `git.status` back into `GitSummary`. Agent serve-smoke drives stage → commit → diff.
  (Push/pull/fetch/checkout/branch follow the same pattern — a further extension.)

- **File domain complete (done).** copy / move / move-overwrite / import / writeBytes route to
  the host (`file.copy`/`file.move`/`file.writeBytes` in codux-runtime-core, served by both
  hosts; the desktop host also gained the missing `createDirectory`). Only reveal/open-external
  stay local (inherently local).

- **Git domain complete (done).** Generic `git.invoke`/`git.read` dispatch; ~30 RuntimeService
  git methods route to the host. Agent: git2 (local) + `git` CLI (branch/network, host auth);
  desktop host: GitService.

- **Worktree domain complete (done).** The full mutation set routes to the host:
  `reload_worktrees`/`reload_worktrees_from_state` map the host `worktree.list` payload →
  `WorktreeSummary`; `create`/`remove`/`merge` route via `remote_worktree_mutation` →
  `worktree.create`/`remove`/`merge` (host replies `worktree.updated`). Agent worktree support is
  a new `apps/agent/src/worktree.rs` over the `git worktree` CLI (list-porcelain → `WorktreeInfo`
  shape; add/remove/merge via `git worktree`). Added `Deserialize` to the worktree types. Only
  `select` stays controller-local (a UI preference, not a host op). Agent serve-smoke drives
  list → create.

- **Memory engine extracted + read path routed (done).** The 3.4k-line memory engine now lives
  in `crates/codux-memory` (and a small shared `crates/codux-llm` for the genai provider core),
  so the host can run it. The crate owns narrow config types (`MemoryConfig`/`MemorySettings`/
  `MemoryProvider`/`MemoryProjectInfo`/`MemoryProjectRecord`/`MemorySessionSnapshot`) that mirror
  the desktop's richer settings; the desktop is a thin re-export shim converting at the boundary
  (40 engine tests pass in the crate). The host serves `memory.read` (summary / manager /
  management / status) against its own store, resolving its project id from the controller-sent
  path (like `ai.state`). `reload_memory` / `memory_management_snapshot` / `memory_manager_snapshot`
  route to the host for remote-hosted projects; agent serve-smoke drives `memory.read` e2e.
  Added `Deserialize` to the memory reply types.

- **Memory extraction routed (done).** The LLM write path completes the memory domain.
  `memory.extract {config, outputLocale}` on the host enqueues candidates from its own indexed
  AI sessions and runs the engine's `process_memory_extraction_queue` with the
  controller-forwarded `MemoryConfig` (incl. the chosen provider's API key) — used for the run,
  never persisted (the **owner decision**). Extraction runs an LLM (async/slow), so the agent
  handles it imperatively on its own runtime thread and sends its own reply, like terminals. The
  controller has `memory_extract` (300s timeout); RuntimeService
  `extract_remote_project_memory(project_id)` forwards the desktop's selected provider to the
  project's host. Agent serve-smoke drives memory.extract e2e (empty config → empty-queue
  status, no LLM). Memory domain is now complete: engine in codux-memory + codux-llm, desktop
  shim, read routing, host extraction.

- **Terminal project-switch restore (done).** Switching to a remote-hosted project now
  reconnects its terminals on the host. `spawn_terminal_tabs`/`mount_terminal_tab_panes` take an
  optional `pending_out`: remote panes are built pending and deferred into the existing async
  attach chokepoint (branches `host_device_id` → `attach_pending_session_remote`), so the UI
  thread never blocks on the network. Local terminals are unchanged.

- **Remaining (each a real chunk, best done with fresh context):**
  - **Terminal boot-time remote restore** — the boot path runs during App construction (no
    `Context<Self>` for the async chokepoint yet), so it still spawns local PTYs and a remote
    project's terminals attach on the next project-switch restore. Needs a post-construction
    attach hook. Small.
  - **AI session ops** — the AI *stats* panel already routes via `ai.state`. The session-level
    ops (detail / fork / rename / remove) would route to the host's AIHistoryService. Secondary.

## Verification

- Headless host today: `just smoke` (includes `--serve-smoke`), or run `cargo run -p
  codux-agent -- --serve` and connect a controller.
- Per domain: extend the serve round-trip to that message (controller issues e.g.
  `terminal.create` / `git.status`, assert the reply), then a manual desktop-client run.
