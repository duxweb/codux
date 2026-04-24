# Dmux Overview

## Main code paths

- App entry: `Sources/DmuxWorkspace/DmuxWorkspaceApp.swift`
- Main coordinator: `Sources/DmuxWorkspace/App/AppModel.swift`
- Main shell: `Sources/DmuxWorkspace/UI/RootView.swift`
- Workspace composition: `Sources/DmuxWorkspace/UI/Workspace/WorkspaceView.swift`
- Sidebar: `Sources/DmuxWorkspace/UI/Sidebar/SidebarView.swift`

## Main stores

- `AppModel`
- `AIStatsStore`
- `AISessionStore`
- `MemoryCoordinator`
- `MemoryStore`
- `GitStore`
- `PetStore`

## Data boundaries

- `PersistenceService` owns persisted app snapshot
- `PetStore` owns persisted pet-specific state
- `MemoryStore` owns SQLite-backed AI memory and extraction task state
- `MemoryContextService` owns generated launch-context artifacts for memory/global prompt injection
- `AIStatsStore` merges indexed usage and live runtime state
- `AISessionStore` owns ephemeral hook-driven runtime/session live state only

## Current AI memory status

Implemented:

- SQLite-backed memory storage in `memory.sqlite3`
- queued extraction from AI session snapshots
- automatic memory extraction provider selection with per-provider testing
- app-private generated context files for supported AI tools
- global prompt injection through runtime launch artifacts
- titlebar memory extraction status indicator

## Current pet status

Implemented:

- egg selection and random egg hidden-species routing
- claim baseline XP
- custom naming on claim
- per-project token baselines and hatch threshold flow
- stage / evolution / Lv.100 FX overlays
- species persistence + inheritance history
- sleep detection
- bubble triggers
- daily damped pet stats cache

## Test map

- `RuntimeDriverTests`
- `AIRuntimeIngressHookEventTests`
- `AIRuntimeIngressSocketTests`
- `AISessionStoreTests`
- `MemoryStoreTests`
- `MemoryContextServiceTests`
- `MemoryCoordinatorTests`
- `PetFeatureTests`
- `PetRefreshCoordinatorTests`
- `scripts/dev/runtime-hook-smoke.py`
