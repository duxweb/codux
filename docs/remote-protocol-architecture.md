# Codux Remote Protocol Architecture

Codux remote is organized as a layered runtime protocol, not as UI-specific terminal forwarding.

## Roles

- **Desktop app (macOS / Windows)**: can act as a controller and a controlled host.
- **Mobile app (Android / iOS)**: controller only. It does not own local projects, PTYs, Git state, or file state.
- **Linux controlled agent**: planned headless host that exposes the same host-side runtime domains without a GUI.
- **Relay service**: discovery, pairing-ticket exchange, signaling, and WebSocket fallback transport. It does not own runtime state.

## Target Layers

```text
UI renderer
  Reads runtime models and emits user intent. It never consumes transport
  messages directly and never owns terminal history, sequence, or resync logic.

Runtime models / buffer pools
  Own project, terminal session, file, Git, worktree, and AI-stat state.
  Every baseline and live delta enters these models before UI rendering.
  Terminal data enters a local or remote PTY session model; UI attaches to
  that model the same way for local and remote terminals.

Bidirectional subscription layer
  Owns resource.subscribe, resource.unsubscribe, baseline, delta, ack, and
  resync semantics. Any peer may publish resources and subscribe to resources
  exposed by the other peer.

Protocol router
  Defines versioning, capabilities, secure envelopes, message domains,
  sequence handling, requestId, error, and schema compatibility.

Transport drivers
  Move protocol envelopes over local memory, WebSocket relay, WebRTC
  DataChannel, or future transports such as QUIC/WebTransport.
```

The UI must not branch on transport type. Git, file, terminal, worktree, and AI-stat features consume the same runtime API whether the active transport is local, WebRTC, WebSocket relay, or a future driver.

## Bidirectional Resource Model

Codux remote should converge on a peer-to-peer resource model instead of a controller-only request model. Each peer can:

- publish resources it owns, such as terminal sessions, file trees, Git state, projects, or worktrees;
- subscribe to resources exposed by the other peer;
- receive a baseline for the subscribed resource;
- receive ordered deltas after the baseline;
- acknowledge processed sequence ranges;
- request resync when sequence gaps or incompatible state are detected.

Terminal sessions are one resource type in this model. A controller subscribes to a terminal session or project terminal scope. The host sends a baseline buffer window, then streams `terminal.output` deltas. While the controller is hydrating the baseline, newer deltas are held in the remote PTY session buffer and replayed after the baseline sequence. UI code only attaches to the resulting runtime model.

## v3.1 Capabilities

`host.info` advertises the protocol version and host capabilities:

- `protocolVersion`: currently `v3.1`.
- `capabilities.domains`: supported runtime domains such as `project`, `terminal`, `worktree`, `file`, and `aiStats`.
- `capabilities.terminalBuffer`: terminal history limits and chunking support.

Terminal history is sent as bounded buffer windows. Large snapshots can be split into `chunked` payloads identified by `snapshotId`, `chunkIndex`, and `chunkCount`. Controllers assemble chunks by session and snapshot before rendering. This keeps large Codex resume histories from becoming one oversized transport message and gives mobile a real progress value.

## Runtime Domains

The protocol is domain-oriented:

- `project.*`: project list, selection, add/edit/remove.
- `terminal.*`: terminal list, create/close, resize, input, output, buffer, upload.
- `worktree.*`: list/select/create/merge/remove.
- `file.*`: list/read/write/rename/delete.
- `ai.stats`: project-scoped AI usage summary.

Future Git-specific controller messages should follow the same pattern instead of binding Git logic to any transport or UI widget.

## Current Terminal Alignment

The current Mac host and Flutter controller are being aligned to the target model:

- Mac host owns the real local PTY session.
- Flutter owns a `RemotePtySession` model for each subscribed remote session.
- `terminal.subscribe` is the subscription entry point.
- `terminal.buffer` remains the baseline/hydration payload while the protocol migrates toward generic `resource.baseline`.
- Live `terminal.output` deltas are written into `RemotePtySession`, not directly into UI.
- UI/native terminal rendering only replays the model for the active session.

## Shared Crate Boundary

The desktop repository now starts the shared Rust boundary inside the workspace:

- `crates/codux-protocol`: protocol version, capabilities, envelope-adjacent payload helpers, chunking, and baseline payload construction.
- `crates/codux-terminal-core`: platform-neutral terminal session semantics such as sequence, snapshot/page hydration, retained live output, and cache limits.

Protocol and terminal core are intentionally separate. Protocol describes what is sent between peers. Terminal core describes how a controller or host stores terminal state after messages are decoded. This keeps future local PTY, remote PTY, Linux headless, and mobile rendering paths aligned without coupling transport schemas to terminal storage internals.

Flutter keeps its Dart implementation while the API stabilizes. After Mac, Windows, Linux headless, and Flutter agree on the same behavior, the shared Rust crates can be exposed to mobile through FFI if the Android NDK and iOS framework build cost is justified.
