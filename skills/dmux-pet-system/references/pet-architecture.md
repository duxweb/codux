# Pet Architecture

## Current behavior

- Claim flow:
  - User chooses one egg from the modal shown before first claim.
  - Name is optional; empty name falls back to the current stage species name.
  - Random egg resolves to one of `voidcat`, `rusthound`, `goose`, or hidden `chaossprite`.
- Persistence:
  - Main pet state is stored as encrypted `pet-state.dat` under the current Codux application support folder.
  - Legacy dmux namespaces are migrated on load when available.
- XP:
  - Per-project token watermarks are captured as projects first appear after claim.
  - New or reopened project history seeds a watermark first and does not grant retroactive hatch or XP progress.
  - Removed project watermarks are forgotten and stale watermarks are pruned on snapshot refresh.
- Stats:
  - Pet token totals come from `AIStatsStore.normalizedTokenTotalsForPet(_:, claimedAt:)`.
  - `PetStore.refreshDerivedState` caches damped daily stats instead of replacing them continuously.
- Evolution:
  - `lockedEvoPath` is set once the pet reaches `PetProgressInfo.evoUnlockLevel`.
  - From that point, stage naming and sprites use the locked path.
- Inheritance:
  - Available at `Lv.100+`.
  - Current pet is archived into `legacy` and claim state resets.

## UI map

- Titlebar entry:
  `TitlebarPetButton`
- Popover:
  `PetPopoverView`
- Egg claim flow:
  `PetEggSelectionDialogView`
- Dex window:
  `PetDexWindowPresenter` + `PetDexWindowView`
- FX:
  `PetEvolutionEffectView`, `PetMaxLevelEffectView`

## Sleep and bubble rules

- Sleep:
  - if app inactive: sleeping
  - if any project activity is running: awake
  - otherwise sleep after 5 minutes without fresh activity ticks
- Bubble triggers:
  - first open
  - running
  - completion / error
  - big session
  - long active session
  - late night
  - level up

These are ephemeral UI events, not persisted gameplay history.

## Known non-goals right now

- No pet click / petting interaction
- No post-100 accessory system
- No complex dex filters
- No extra materials pipeline beyond the existing sprite processors

## Test entrypoints

- Pure Swift tests:
  `Tests/DmuxWorkspaceTests/PetFeatureTests.swift`
  `Tests/DmuxWorkspaceTests/PetRefreshCoordinatorTests.swift`
  `Tests/DmuxWorkspaceTests/AIStatsStoreMetricsTests.swift`
- Dev-only visual checks:
  open the pet popover in the dev app and use the debug buttons for bubble / evolution / max-level effects
