---
name: dmux-ai-runtime-architecture
description: Use when editing dmux AI CLI runtime behavior, live token/loading/session restore logic, tool wrappers, runtime probes, or AI tool drivers. Covers the factory-based architecture, the shared runtime/live state pipeline, and the guardrail that tool-specific bugs must be fixed in the matching driver, probe, or wrapper instead of by patching upper shared layers.
---

# Dmux AI Runtime Architecture

Use this skill before changing dmux AI runtime behavior for `codex`, `claude`, `gemini`, or `opencode`.

## Core rule

Single-tool bugs must be fixed in that tool's own driver chain:

- wrapper or plugin
- tool driver in `AIToolDriverFactory.swift`
- tool-specific runtime probe service
- tool-specific source parsing or hook handling

Do not patch shared upper layers just to make one tool work.

## Shared layers you should preserve

- `Sources/DmuxWorkspace/Services/AIRuntimeIngressService.swift`
  Purpose: runtime socket listener, file watcher ingress, event dispatch, generic transport.
- `Sources/DmuxWorkspace/App/AIRuntimeStateStore.swift`
  Purpose: shared in-memory session/logical-session/terminal binding model and generic live token math.
- `Sources/DmuxWorkspace/App/AIStatsStore.swift`
  Purpose: panel assembly, refresh scheduling, current snapshot selection, UI-facing aggregation.

These files may change only for genuinely shared bugs or contract changes that affect multiple tools.

## Tool-owned layers

- `Sources/DmuxWorkspace/Services/AIToolDriverFactory.swift`
  Tool registration, capabilities, and driver-owned socket/file event handling.
- `Sources/DmuxWorkspace/Services/*RuntimeProbeService.swift`
  Tool-specific total/model/response-state snapshots.
- `scripts/wrappers/tool-wrapper.sh`
  Generic launcher envelope only.
- `scripts/wrappers/<tool>-config/...`
  Tool-specific plugin or hook extraction.

## Non-negotiable constraints

- No local keypress-driven AI state transitions.
- No "fixing" a single tool by adding upper-layer special cases.
- If `codex` and `claude` are correct but `opencode` is wrong, fix `opencode`.
- If a tool has an official hook, plugin, or runtime status source, use that source instead of guessing from UI behavior.

## Debug workflow

1. Check wrapper/plugin logs first.
2. Check whether the tool driver receives the event or snapshot it needs.
3. Check tool-specific probe resolution of `externalSessionID`, model, totals, and response state.
4. Only inspect shared layers if the same failure shape appears across multiple tools.

## Read next

For the detailed architecture and “what can move / what must not move” rules, read:

- `references/runtime-architecture.md`
