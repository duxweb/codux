---
name: dmux-project-architecture
description: Use when you need a fast mental model of the dmux codebase before making changes. Covers app entry, state containers, persistence, terminal/workspace layout, AI runtime pipeline, Git pipeline, pet subsystem, and where to place new work.
---

# Dmux Project Architecture

Use this skill before making non-trivial dmux changes that cross subsystem boundaries.

## What this repo is

`dmux` is a macOS SwiftUI desktop app with:

- multi-project workspace management
- split terminal sessions
- AI runtime/live usage aggregation
- SQLite-backed AI memory extraction and launch-context injection
- Git panel and remote actions
- app-level persistence and settings
- a pet UI layer bound to AI usage/activity

## Top-level layout

- `Sources/DmuxWorkspace/App`
  Main app stores and settings.
- `Sources/DmuxWorkspace/Models`
  Serializable app/domain models.
- `Sources/DmuxWorkspace/Services`
  Runtime, indexing, Git, updater, persistence, bridge services.
- `Sources/DmuxWorkspace/UI`
  SwiftUI views grouped by feature.
- `Sources/DmuxWorkspace/Terminal`
  Terminal embedding and bridge code.
- `Sources/DmuxWorkspace/Resources`
  Localizations, icons, pet assets.

## Primary state containers

- `AppModel`
  Main app coordinator. Owns project/workspace selection, settings mutation, panel toggles, terminal actions, Git actions, updater flow, runtime refresh orchestration.
- `AIStatsStore`
  AI panel state, timers, cached/indexed snapshots, live overlay merging.
- `AISessionStore`
  In-memory hook-driven live runtime/session binding model for AI tools.
- `MemoryCoordinator`
  Queues and runs memory extraction from completed AI sessions.
- `MemoryStore`
  SQLite-backed user, project, summary, and extraction-task memory storage.
- `GitStore`
  Git panel state and refresh/remote action orchestration.
- `PetStore`
  Pet claim/baseline state, persisted pet stats snapshot, XP baseline and daily damped stats cache.

## App entry and window structure

- `DmuxWorkspaceApp.swift`
  App entry, main window group, settings scene, window chrome helpers.
- `RootView.swift`
  Main shell.
  Titlebar overlay + sidebar + workspace split container.
- `WorkspaceView.swift`
  Actual terminal/workspace composition and split rendering.
- `SidebarView.swift`
  Project list and per-project activity badges.

## Persistence model

- `PersistenceService.swift`
  Reads/writes `state.json` under Application Support.
  Sanitizes projects/workspaces on load.
- `AppSnapshot` in `AppModels.swift`
  Persisted top-level app snapshot for projects, workspaces, selected project, app settings.
- `PetStore.swift`
  Separately persists encrypted pet state to `pet-state.dat` under the current Codux application support folder, with legacy dmux namespace migration.
- `MemoryStore.swift`
  Stores AI memory in `memory.sqlite3` under the current Codux application support folder.

## Terminal/workspace model

- `Project`
  User project entry.
- `TerminalSession`
  One terminal tab/pane definition.
- `ProjectWorkspace`
  Per-project layout containing top panes, bottom tabs, selected terminal, ratios.
- `SwiftTermBridge.swift`
  Terminal embedding and process bridge.

## AI architecture

Read `skills/dmux-ai-runtime-architecture/SKILL.md` for detailed runtime rules.
Read `skills/dmux-ai-memory/SKILL.md` before changing memory extraction, memory storage, provider selection, or launch prompt injection.

Quick map:

- `AIRuntimeBridgeService.swift`
  Managed shell/runtime environment setup.
- `AIRuntimeIngressService.swift`
  Imports runtime envelopes, socket events, file watches.
- `AIToolDriverFactory.swift`
  Tool-owned runtime behavior.
- `ClaudeRuntimeProbeService.swift`
- `GeminiRuntimeProbeService.swift`
- `OpenCodeRuntimeProbeService.swift`
- `CodexHookRuntimeService.swift`
  Tool-specific parsing/probe layers.
- `AIUsageService.swift` / `AIUsageStore.swift`
  Indexed usage snapshots and breakdowns.
- `MemoryCoordinator.swift`
  Session snapshot filtering, extraction queue orchestration, and provider invocation.
- `MemoryContextService.swift`
  Runtime launch artifacts that inject global prompt and memory context into supported tools.
- `MemoryStore.swift`
  SQLite persistence for memories, summaries, FTS search, and extraction tasks.
- `AIStatsPanelView.swift`
  AI panel UI.

## Git architecture

- `GitService.swift`
  Low-level git command execution.
- `GitStore.swift`
  Git panel orchestration, refresh polling, remote actions.
- `GitPanelView.swift`
  Git UI.

## Pet architecture

- `PetPanelView.swift`
  Current pet UI, claim flow, sprite view, popover, titlebar bubble behavior.
- `PetStore.swift`
  Claimed species, custom name, project-scoped token watermarks, persisted/damped pet stats snapshot.
- `AIStatsStore.normalizedTokenTotalsForPet(_:, claimedAt:)`
  Per-project pet token source filtered by claim time and current projects.

Current pet design in code:

- XP starts from claim-time per-project baselines, not from historical total tokens.
- Removed/reopened projects must not replay historical usage into hatch or XP progress.
- Pet stats are refreshed from AI usage but cached daily in `PetStore` with damping.
- Sleep/bubble behavior currently lives in `TitlebarPetButton`.
- Further pet work should continue extending this structure, not replace it with an unrelated parallel flow.

## Where to put new work

- New app-wide mutable state:
  `App/`
- New serializable models:
  `Models/`
- New integration/service logic:
  `Services/`
- New feature UI:
  feature folder under `UI/`
- New runtime/tool-specific AI behavior:
  tool-owned service/driver layer, not shared upper state

## Read next

- `skills/dmux-project-conventions/SKILL.md`
- `skills/dmux-ai-runtime-architecture/SKILL.md`
- `skills/dmux-ai-memory/SKILL.md`
- `skills/dmux-pet-system/SKILL.md`
- `skills/pet-sprite-pipeline/SKILL.md`
- `references/overview.md`
