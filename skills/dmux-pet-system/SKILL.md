---
name: dmux-pet-system
description: Use when editing the dmux electronic pet subsystem: claim flow, single-form XP rules, sleep and bubble behavior, titlebar pet UI, dex/archive history, or pet-specific tests and debug tools.
---

# Dmux Pet System

Use this skill before changing pet logic or pet UI.

## Core files

- `Sources/DmuxWorkspace/Models/PetModels.swift`
  Pet domain types: stats, species, claim options, legacy records.
- `Sources/DmuxWorkspace/App/PetStore.swift`
  Persisted pet ownership, project token watermarks, XP progress, current stats, legacy archive records.
- `Sources/DmuxWorkspace/UI/Pet/PetPanelView.swift`
  Progress model, titlebar entry, popover, claim modal, bubble/sleep wiring.
- `Sources/DmuxWorkspace/UI/Pet/PetMilestoneEffect.swift`
  Level-up and max-level celebration overlays.
- `Sources/DmuxWorkspace/UI/Pet/PetDexWindow.swift`
  Dex and archive history window.

## Rules to preserve

- XP starts from the claim baseline, not historical total tokens before claim.
- XP is project-scoped: new projects seed a watermark first, removed projects clear or prune stale watermarks, and reopened projects must not replay historical tokens.
- Pets are single-form companions. There is no egg, hatch, evolution, or stage-specific sprite path.
- Current level comes from `PetProgressInfo`, not ad-hoc UI math.
- Sleep and bubble UI are titlebar behavior; persisted pet state stays in `PetStore`.
- Random claim resolves across all bundled species.

## Dev debug workflow

In dev builds, the pet popover may expose debug-only controls for:

- bubble preview
- level-up effect preview
- max-level effect preview

Keep these controls dev-only. Do not ship them in the standard bundle.

## Tests

When changing pet rules, add or update Swift tests in `Tests/DmuxWorkspaceTests/`.

Prefer pure rule tests for:

- per-project token baselines and reopened-project XP stability
- pet refresh coordinator debounce and latest-snapshot behavior
- random claim species resolution
- level math
- single companion stage behavior
- stat damping / persona derivation

## Read next

- `references/pet-architecture.md`
- `skills/pet-sprite-pipeline/SKILL.md`
