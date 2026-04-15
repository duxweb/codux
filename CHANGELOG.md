# Changelog

All notable changes to this project will be documented in this file.

## [0.1.3] - 2026-04-16

### Changed

- Refined the terminal and right-side assistant chrome so the top and split dividers use a unified separator treatment.
- Softened the AI Assistant card backgrounds and increased the panel title spacing for a calmer visual rhythm.

### Fixed

- Corrected the terminal top-left border rendering so only the intended top and left edges are drawn, without broken joins or stray rounded corners.
- Fixed the right sidebar top divider gap when opening the panel.
- Removed the Git commit split menu checkmark state so action items no longer look like persistent selections.

## [0.1.2] - 2026-04-16

### Changed

- Refined the main app chrome so the selected sidebar project and top-right titlebar controls read more clearly with stronger, more consistent background emphasis.
- Increased the About window action button sizing for better visibility and click comfort.
- Rebalanced the daily work-intensity ladder to `5M / 10M / 30M / 70M / 100M / 200M / 300M`.
- Renamed the daily intensity tiers to short, shareable work-state labels.

### Fixed

- Adjusted the Git commit split dropdown width so the action layout is centered and visually stable.
- Localized the daily intensity tier names across all bundled languages instead of mixing the new Chinese labels with legacy rank names.

## [0.1.1] - 2026-04-15

### Changed

- Refreshed the product screenshot used in the repository and release materials.
- Polished the Git panel interaction details and visual states.

### Fixed

- Hid the branch/header toolbar when the current directory is not a Git repository.
- Fixed the commit split button layout so the primary action and dropdown no longer render with mismatched widths or background fills.
- Tuned the split button divider visibility so it remains visible without looking too heavy.
- Improved Git file/history hover action readability by using self-contained button backgrounds that no longer let underlying text show through.

## [0.1.0] - 2026-04-15

### Added

- First public Codux release.
- Native macOS terminal workspace for AI coding tools with project workspaces, split terminals, integrated Git panel, AI usage tracking, localization, update checks, and universal macOS release packaging.
