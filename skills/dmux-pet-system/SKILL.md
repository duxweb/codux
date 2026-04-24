---
name: dmux-pet-system
description: Use when editing the dmux electronic pet subsystem: claim flow, egg selection, hatch and XP rules, stage/evolution logic, sleep and bubble behavior, titlebar pet UI, dex/inheritance, or pet-specific tests and debug tools.
---

# Dmux Pet System

Use this skill before changing pet logic or pet UI.

## Core files

- `Sources/DmuxWorkspace/Models/PetModels.swift`
  Pet domain types: stats, species, claim options, legacy records.
- `Sources/DmuxWorkspace/App/PetStore.swift`
  Persisted pet ownership, project token watermarks, hatch/XP progress, locked evolution path, current stats, legacy records.
- `Sources/DmuxWorkspace/UI/Pet/PetPanelView.swift`
  Progress model, titlebar entry, popover, egg claim modal, bubble/sleep wiring.
- `Sources/DmuxWorkspace/UI/Pet/PetEvolutionEffect.swift`
  Evolution and max-level celebration overlays.
- `Sources/DmuxWorkspace/UI/Pet/PetDexWindow.swift`
  Dex and inheritance history window.

## Rules to preserve

- XP starts from the claim baseline, not historical total tokens before claim.
- XP is project-scoped: new projects seed a watermark first, removed projects clear or prune stale watermarks, and reopened projects must not replay historical tokens.
- Hatch threshold is `200_000_000` tokens.
- Current stage and level come from `PetProgressInfo`, not ad-hoc UI math.
- Evolution path locks once unlocked; do not keep recomputing it after lock.
- Sleep and bubble UI are titlebar behavior; persisted pet state stays in `PetStore`.
- Hidden species comes only from the random egg path.

## Dev debug workflow

In dev builds, the pet popover may expose debug-only controls for:

- bubble preview
- evolution effect preview
- max-level effect preview

Keep these controls dev-only. Do not ship them in the standard bundle.

## Tests

When changing pet rules, add or update Swift tests in `Tests/DmuxWorkspaceTests/`.

Prefer pure rule tests for:

- per-project token baselines and reopened-project XP stability
- pet refresh coordinator debounce and latest-snapshot behavior
- random egg species resolution
- hatch threshold and level math
- stage mapping by level/evolution path
- stat damping / persona derivation

## Read next

- `references/pet-architecture.md`
- `skills/pet-sprite-pipeline/SKILL.md`
