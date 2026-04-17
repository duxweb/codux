# Runtime Architecture

This document records the dmux AI runtime architecture for future edits.

## Intent

The runtime system is factory-based and driver-based:

- shared layers manage transport, generic state, and UI aggregation
- each AI CLI owns its own event extraction and session semantics

That split is deliberate. Regressions happened when a single tool bug was "fixed" by patching shared layers.

## Layer map

### 1. Tool wrappers and plugins

Files:

- `scripts/wrappers/tool-wrapper.sh`
- `scripts/wrappers/dmux-ai-state.sh`
- tool-specific plugin folders such as `scripts/wrappers/opencode-config/`

Responsibilities:

- launch the real CLI
- inject dmux environment
- emit raw runtime events from the tool's own hooks or plugin APIs

Must not:

- implement shared token math
- infer panel state from local terminal keypresses
- patch another tool's behavior

### 2. Tool drivers

File:

- `Sources/DmuxWorkspace/Services/AIToolDriverFactory.swift`

Responsibilities:

- register each tool driver
- declare driver capabilities such as:
  - `prefersHookDrivenResponseState`
  - `allowsRuntimeExternalSessionSwitch`
  - `usesHistoricalExternalSessionHintForRuntimeProbe`
  - `appliesGenericResponsePayloads`
- parse tool-specific socket or file events
- produce tool-owned runtime snapshots and response payloads

This is the first place to fix a single-tool runtime bug.

Examples:

- `codex` hook event handling belongs here
- `opencode` plugin event handling belongs here
- if a tool can switch external sessions inside one running process, that behavior must be owned here or in its probe/plugin chain, not forced by generic layers

### 3. Tool-specific runtime probes

Files:

- `Sources/DmuxWorkspace/Services/CodexRuntimeProbeService.swift`
- `Sources/DmuxWorkspace/Services/ClaudeRuntimeProbeService.swift`
- `Sources/DmuxWorkspace/Services/GeminiRuntimeProbeService.swift`
- `Sources/DmuxWorkspace/Services/OpenCodeRuntimeProbeService.swift`
- `Sources/DmuxWorkspace/Services/OpenCodeGlobalEventService.swift`

Responsibilities:

- resolve external session IDs
- load model and total token state
- load official response/loading state where available
- decide whether a snapshot is fresh or restored

Rules:

- prefer official tool sources
- for `opencode`, use official plugin/runtime/global-event sources before inventing generic behavior
- if a response/loading bug is only for one tool, fix the probe for that tool

### 4. Shared ingress

File:

- `Sources/DmuxWorkspace/Services/AIRuntimeIngressService.swift`

Responsibilities:

- own the runtime unix socket listener
- receive raw events from wrappers/plugins
- decode payloads
- dispatch to drivers
- maintain in-memory live envelope and response payload caches
- watch shared runtime source descriptors

Must stay generic.

Do not:

- add `if tool == ...` behavior for one broken CLI
- rewrite an incoming tool event just because one driver is wrong
- change shared normalization rules to fix a single tool unless the contract is truly shared

If a change here would alter `codex`, `claude`, or `gemini` behavior while you are only fixing `opencode`, the change is probably in the wrong layer.

### 5. Shared runtime state

File:

- `Sources/DmuxWorkspace/App/AIRuntimeStateStore.swift`

Responsibilities:

- shared in-memory model for:
  - `sessions`
  - logical tool sessions
  - terminal-to-session bindings
- generic baseline and live token calculations
- generic current snapshot assembly
- generic terminal binding lifecycle

The shared model is conceptually:

- terminal state
- logical session state keyed by `tool + externalSessionID`
- attachment state between terminals and logical sessions

This file is not where single-tool semantics should be invented.

Allowed changes:

- bugs that affect multiple tools
- generic baseline math bugs
- generic binding lifecycle bugs
- generic snapshot merge contract fixes

Disallowed changes for single-tool issues:

- "keep previous external session id" hacks
- "ignore new external session id" hacks
- one-off special casing because one plugin or probe is emitting the wrong data

### 6. Shared panel assembly

File:

- `Sources/DmuxWorkspace/App/AIStatsStore.swift`

Responsibilities:

- refresh scheduling
- indexing + runtime composition
- current selected terminal snapshot
- live list vs aggregation list
- panel state shown to the UI

Must stay presentation-focused.

Do not fix tool extraction problems here.

## Contract summary

### Shared contract

Shared layers expect tool drivers and probes to provide:

- external session identity
- model
- totals
- response/loading state

Shared layers then:

- bind terminals
- compute live usage from baseline and totals
- aggregate snapshots for display

### What "live" means

Live state is in-memory runtime state, not historical indexing.

At minimum it tracks:

- current running tool per terminal
- current external session identity
- response/loading state
- latest model
- latest totals
- logical-session attachment and baseline state

### What "historical" means

Historical indexing is the persisted session list and aggregate totals loaded from each tool's storage.

Historical data seeds restored sessions, but it must not replace tool-owned live semantics.

## Repair policy

When a runtime bug is reported:

1. Confirm whether it affects one tool or multiple tools.
2. If one tool only, start in that tool's wrapper, plugin, driver, and probe.
3. Use shared layers only after proving the contract is wrong for all tools.

## Concrete guardrails

- If `codex` and `claude` are correct, do not touch shared runtime layers to fix `opencode`.
- If a tool's loading disappears after `Esc` or `Ctrl+C`, do not infer AI state from local key handling. Use the tool's own events.
- If a tool restores the wrong token total, fix its external session resolution or probe logic.
- If a tool changes sessions in-process, its own event chain must communicate that clearly. Do not force shared layers to guess.

## Log-first workflow

Always inspect the log chain in this order:

1. wrapper or plugin emitted event
2. driver accepted event
3. probe resolved external session / totals / response state
4. runtime store bound the correct logical session
5. AI stats panel rendered the expected snapshot

If step 1 or step 2 is wrong, stop there and fix the tool-owned chain first.
