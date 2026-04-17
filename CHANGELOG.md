# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- None yet.

### Changed

- None yet.

### Fixed

- None yet.

## [0.1.11] - 2026-04-17

### Changed

- Moved Git panel auto-refresh ownership fully into the Git store so the app layer now only controls panel lifecycle while repository watching and refresh coalescing stay inside the Git driver path.
- Updated the Git panel view to observe the Git store directly, keeping automatic file-status refreshes and remote sync state changes in the same render chain.

### Fixed

- Fixed Git file list auto-refresh so local file creates, deletes, and AI-generated changes now update the panel immediately while it is open, without requiring a manual refresh.
- Fixed Git panel refresh behavior to preserve the selected file and visible diff state across automatic repository refreshes instead of dropping back to stale or empty detail state.
- Fixed terminal focus restoration after project switches, window reactivation, and unminimizing so the shell can accept input again without an extra click.
- Fixed Git file row trailing actions so hover controls no longer change row height and the right-side status/action slot stays layout-stable.

## [0.1.10] - 2026-04-17

### Changed

- Split AI runtime probing into tool-specific services so Codex, Claude, Gemini, and OpenCode now own their own realtime probing and metadata lookup paths instead of keeping that logic in one shared probe file.
- Kept the runtime ingress and driver layers focused on routing only, with hook parsing, transcript probing, and external-session matching moved closer to each tool implementation.

### Fixed

- Fixed realtime loading/completion state recovery for Codex and Claude so prompt submit, interrupt, stop, and completed turns no longer bounce between stale `responding` and `idle` states.
- Fixed stale response payloads from reviving older realtime sessions after a newer snapshot had already moved the session back to idle.
- Fixed Claude hook session mapping so hook payload session IDs are captured more reliably, including stop-failure and resumed-session paths.
- Fixed project and today token overlays so live session tokens continue to merge correctly into the current project summary while avoiding duplicate indexed totals.

## [0.1.9] - 2026-04-16

### Changed

- Prioritized hook-driven runtime state for Codex and Claude so live hook events now own the sidebar responding/loading state while file probing only supplements metadata.
- Simplified terminal interaction and renderer tuning so focus, command-arrow routing, cursor behavior, and GPU mode updates stay closer to native terminal behavior without the extra temporary boost path.
- Reduced debug-log noise by de-duplicating repeated `startup-ui` and `activity-phase` lines during rapid window activation and workspace rebuilds.

### Fixed

- Fixed stale loading state after interrupt, app switching, or delayed probe refreshes by persisting interrupt timestamps and blocking older runtime snapshots from reviving `responding`.
- Fixed Claude and generic wrapper runtime completion reporting so wrapped tool exits emit a final completed state instead of leaving lingering running metadata behind.
- Fixed terminal focus/selection edge cases so split switching no longer re-triggers unnecessary stats refreshes and `Cmd+Left/Right` navigation works reliably with normalized modifier handling.

## [0.1.8] - 2026-04-16

### Added

- Added a three-part titlebar performance monitor that separates CPU, memory, and graphics usage so terminal rendering overhead is easier to read at a glance.
- Added terminal GPU mode controls in Settings with localized labels for high-performance, balanced, and memory-saver rendering profiles.

### Changed

- Rebalanced terminal rendering so the default balanced mode keeps the smoother low-jank GPU path while the memory-saver mode can trade idle graphics usage for lower footprint.

### Fixed

- Fixed terminal renderer churn by carrying pane focus, visibility, and reduced-memory hints through the workspace layout instead of treating every pane as fully active.
- Fixed memory-saver mode so a single focused terminal can temporarily promote back to Metal during interaction or live output, then fall back after idle without destabilizing the default experience.
- Fixed the performance monitor memory reading to split graphics footprint from general process memory, avoiding misleading single-number totals in the titlebar.

## [0.1.7] - 2026-04-16

### Changed

- Local packaging now always produces a verbose debug build by default, while the GitHub release workflow calls the release packager directly for formal release artifacts.
- Defaulted the manual test-build workflow and helper script to Debug so ad hoc verification runs keep the richer diagnostics profile unless explicitly overridden.

### Fixed

- Restored the macOS Settings toolbar tabs after the standard window chrome pass so the preferences header no longer disappears.
- Let the Settings window height follow the selected section instead of staying stuck at a single fixed height.

## [0.1.6] - 2026-04-16

### Added

- Added a dedicated Open Project action on the welcome screen so existing folders can be opened without going through project creation flow.
- Added configurable terminal GPU acceleration and performance-monitor settings in Preferences, including localized labels and adjustable sampling intervals.
- Added a helper release script plus release packaging updates so published builds include the signed zip, dmg, debug dmg, and SHA256 checksums together.

### Changed

- Refined the AI today-level presentation, welcome-screen buttons, and split-pane inactive overlay styling for more consistent macOS 14/15 appearance.
- Defaulted the Dock badge preference to enabled for new installs and for older snapshots that do not yet carry that setting.
- Split app logging into release-friendly compact mode and verbose debug packaging mode for easier user diagnostics.

### Fixed

- Fixed startup recovery and project-open fallback flow so failed terminal restoration no longer blocks entering the project shell.
- Fixed repeated terminal host/environment rebuild churn and improved diagnostics around terminal startup on macOS 14.
- Fixed the VS Code open action crash by avoiding main-actor state updates from LaunchServices completion callbacks.
- Fixed the Settings window standard-titlebar restoration on macOS 14.5 so the traffic-light controls no longer render offset into the content area.
- Fixed performance-monitor logging/session rollover behavior so each launch starts with a fresh rotating log set.

## [0.1.5] - 2026-04-16

### Changed

- Reduced real-time activity refresh pressure by coalescing runtime bridge and project activity updates instead of recomputing on every pulse.
- Smoothed Git file row hover actions so the action slot stays layout-stable and avoids needless view churn while moving the pointer.

### Fixed

- Fixed Claude session-end handling so stopping a run clears the responding state correctly instead of leaving the sidebar activity indicator spinning.
- Stopped the AI stats terminal-output path from repeatedly re-importing runtime state on every chunk of terminal output, reducing unnecessary CPU work during high-frequency responses.

## [0.1.4] - 2026-04-16

### Added

- Added in-app diagnostics export so users can collect logs and troubleshooting data from the Help menu more easily.
- Added a dedicated `test-build.sh` entrypoint and manual GitHub Actions test-build workflow for non-release verification builds.

### Changed

- Improved the README and Chinese README with clearer diagnostics and issue-reporting guidance.
- Updated the release/test packaging flow so test artifact labels are separated from the app's internal version number.

### Fixed

- Hardened project/settings persistence recovery so invalid saved data is less likely to block launch or project creation.
- Restored AI runtime hook setup more safely, including rebuilding user hook configuration when needed without clobbering unrelated content.
- Stopped workflow artifacts from distributing raw `.app` bundles, reducing broken-download cases caused by damaged app bundles after artifact transfer.

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
