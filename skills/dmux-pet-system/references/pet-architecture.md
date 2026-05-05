# Pet Architecture

## Current behavior

- Claim flow:
  - User chooses one companion from the modal shown before first claim.
  - Name is optional; empty name falls back to the current stage species name.
  - Random claim resolves to one of the bundled species.
- Persistence:
  - Main pet state is stored as encrypted `pet-state.dat` under the current Codux application support folder.
  - Legacy dmux namespaces are migrated on load when available.
- XP:
  - Per-project token watermarks are captured as projects first appear after claim.
  - New or reopened project history seeds a watermark first and does not grant retroactive XP progress.
  - Removed project watermarks are forgotten and stale watermarks are pruned on snapshot refresh.
- Stats:
  - Pet token totals come from `AIStatsStore.normalizedTokenTotalsForPet(_:, claimedAt:)`.
  - `PetStore.refreshDerivedState` caches damped daily stats instead of replacing them continuously.
- Appearance:
  - Each species has one flat `spritesheet.png` atlas under `Sources/DmuxWorkspace/Resources/Pets/<species>/`.
  - `PetStage` is always `.companion`; legacy evolution path fields are read only for old state compatibility.
- Archive:
  - Available at `Lv.100+`.
  - Current pet is archived into `legacy` and claim state resets.

## UI map

- Titlebar entry:
  `TitlebarPetButton`
- Popover:
  `PetPopoverView`
- Claim flow:
  `PetClaimDialogView`
- Dex window:
  `PetDexWindowPresenter` + `PetDexWindowView`
- FX:
  `PetLevelUpEffectView`, `PetMaxLevelEffectView`

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
- No extra materials pipeline beyond the flat Codex atlas normalizer

## Test entrypoints

- Pure Swift tests:
  `Tests/DmuxWorkspaceTests/PetFeatureTests.swift`
  `Tests/DmuxWorkspaceTests/PetRefreshCoordinatorTests.swift`
  `Tests/DmuxWorkspaceTests/AIStatsStoreMetricsTests.swift`
- Dev-only visual checks:
  open the pet popover in the dev app and use the debug buttons for bubble / level-up / max-level effects
