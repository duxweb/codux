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

- **Done:** `apps/agent` serves a real host domain over Iroh — `host.info`, `file.list`,
  `file.read` — via `apps/agent/src/host.rs`. `--serve` runs it; `--serve-smoke` (in
  `just smoke`) proves a controller→host `file.list` round trip end to end. This is the
  first real slice of P2 and establishes the serve-loop pattern for all domains.
- **Next:** P1 trait seam + migrate ProjectStore/Worktree → terminal → AI → memory into the
  shared host runtime, then point `apps/agent --serve` at it; in parallel, P3 desktop client
  + P4 routing so the desktop can connect and use it.

## Verification

- Headless host today: `just smoke` (includes `--serve-smoke`), or run `cargo run -p
  codux-agent -- --serve` and connect a controller.
- Per domain: extend the serve round-trip to that message (controller issues e.g.
  `terminal.create` / `git.status`, assert the reply), then a manual desktop-client run.
