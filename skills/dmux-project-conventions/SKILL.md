---
name: dmux-project-conventions
description: Use when making user-facing changes in dmux. Covers project-wide delivery rules such as localization completeness, changelog updates, scoped commits, and avoiding partial UI copy changes.
---

# Dmux Project Conventions

Use this skill for general dmux product work that is not limited to a single subsystem.

## Localization rule

This app is multilingual.

- Any newly added user-facing text must be added to `Sources/DmuxWorkspace/Resources/Localizable.xcstrings`.
- Do not leave new UI copy relying only on `defaultValue`.
- When adding or renaming a localization key, fill every supported locale already present in the project:
  - `en`
  - `zh-Hans`
  - `zh-Hant`
  - `de`
  - `es`
  - `fr`
  - `ja`
  - `ko`
  - `pt-BR`
  - `ru`
- If a view shows duplicated copy because both a container and a control render labels, fix the control usage instead of hiding the problem with layout hacks.

## Change discipline

- Any user-facing feature or bug fix should update both `CHANGELOG.md` and `CHANGELOG.zh-CN.md` under `## [Unreleased]`.
- Keep commits scoped to the finished change.
- Do not push half-verified UI changes as if they were complete.

## UI copy discipline

- Prefer short section titles and keep long explanation text in secondary helper copy.
- Reuse existing localization keys only when the meaning is actually the same.
- If a setting is visible in multiple places, keep the wording consistent across menu, settings, dialogs, and release notes.
