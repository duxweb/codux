# Spec Instructions

## TL;DR
- Stage 1: write `docs/spec/changes/<id>/` (proposal + tasks + deltas), validate, get approval
- Stage 2: implement tasks sequentially (no scope creep)
- Stage 3: archive after shipping; update truth specs

## Delta rules
- Delta files live under `docs/spec/changes/<id>/specs/<capability>/spec.md`
- First non-empty line MUST be: `## ADDED Requirements` / `## MODIFIED Requirements` / `## REMOVED Requirements` / `## RENAMED Requirements`
- Each `### Requirement:` MUST include descriptive text before scenarios
- Each requirement MUST include ≥1 scenario using `#### Scenario:` (4 hashes)
