# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.8.1] - 2026-04-30

### Fixed

- Fixed AI API provider keys so they are stored in the app configuration instead of macOS Keychain, removing the `dmux.ai.providers` permission prompt for provider-backed memory extraction and pet LLM lines.
- Fixed pet sleep timing so titlebar and desktop pets stay awake while the current project terminal is still loading, then start the 30-second idle sleep timer after loading clears.

## [0.8.0] - 2026-04-30

### Added

- Added a project file sidebar that can browse the current project directory, including hidden directories, with file actions for opening, editing, deleting, copying paths, revealing in Finder, and inserting paths into the active terminal.
- Added a standalone file editor window with syntax highlighting, line numbers, edit mode, reload, copy path, reveal in Finder, and Save As actions.
- Added side-by-side Git file diff windows from the Git panel, showing new and old file content with localized fixed column headers, compact rows, and adaptive 1:1 column widths.

### Changed

- Refined file editor toolbar layering, borders, and dark/light appearance so line-number rulers no longer bleed into the toolbar.
- Improved Git diff preview layout by removing duplicate path headers, using solid column header colors, and keeping diff headers pinned while the file body scrolls.
- Optimized file editor scrolling by caching line-number offsets, reducing ruler invalidation during scroll, and reusing existing highlighted text when possible.

### Fixed

- Fixed file editor windows so Command-W closes the editor window instead of triggering the workspace split-close confirmation.
- Fixed desktop pet daily usage messages so Claude token-count changes no longer trigger repeated speech bubbles on every small token increase.
- Fixed pet sleep presentation so titlebar and desktop pets enter sleep after 30 seconds of idle time, with a fallback sleep indicator for stages without sleep sprites.
- Completed localization coverage for split-close confirmation text and Git diff new/old file labels.

## [0.7.0] - 2026-04-30

### Added

- Added the pet speech system for AI runtime events, milestones, reminders, and completion states, with messages shown through the desktop widget bubble instead of the old top pet speech surface.
- Added pet speech modes, hourly frequency controls with a 30-second global cooldown, temporary mute controls, and optional LLM line polishing through configured API providers.
- Added desktop pet widget controls and wired hydration, sedentary, late-night, and completed-turn reminders into the widget message flow.
- Added a General setting to prevent Mac idle sleep with Off, Always, and Power Adapter Only modes while still allowing the display to turn off.

### Changed

- Simplified pet settings by removing mixed-mode explanation, prompt audit preview, and daily LLM limit controls from the user-facing panel.
- Localized the new pet speech, desktop widget, LLM, reminder, and sleep-prevention UI strings across the app's supported languages.

### Fixed

- Improved Codex activity handling so completed turns and queued runtime updates remain visible long enough for widget messages instead of disappearing immediately.
- Hardened pet speech provider selection so disabled or incompatible providers fall back to automatic selection.

## [0.6.1] - 2026-04-29

### Changed

- Reworked AI memory launch context into layered workspace files so agents receive a compact index with separate user, project, recent, and search-oriented memory references instead of a bulky prompt dump.
- Added provider fallback for automatic memory extraction so another configured provider can retry when the preferred provider fails.

### Fixed

- Hardened memory extraction response parsing for fenced JSON, prompt-echo output, and balanced JSON embedded in stderr/stdout noise.
- Fixed misleading Codex extraction failures that reported echoed prompt text as the last useful error line.
- Fixed floating tooltip sizing so short labels stay compact while long multi-line tooltips expand vertically within the maximum width.

## [0.6.0] - 2026-04-28

### Added

- Added remote connection status in the title bar with an online/offline indicator and a device popover for paired mobile clients.
- Added one-time QR pairing with mobile confirmation details, matching codes, rejection flow, and cached paired-device restoration after restart.
- Added end-to-end encrypted remote payload transport between Codux macOS and Codux Mobile, including host/device key management and secure message forwarding.
- Added mobile-side remote file rename/delete handling from the macOS host.

### Changed

- Remote terminal sessions created by mobile clients are now isolated from the visible macOS split workspace while still running on the Mac host.
- Remote settings now hide device-management details until a relay server is configured and enabled.
- Remote reconnect handling now uses localized status messages and backs off automatically when the relay disconnects.

### Fixed

- Fixed paired device removal and status refresh behavior so removed devices disappear from the list instead of staying as stale entries.
- Fixed pairing dialog flow so closing or rejecting an active pairing cancels the mobile waiting state instead of leaving it pending.

## [0.5.11] - 2026-04-24

### Changed

- Simplified project activity tracking to rely on live runtime sessions plus UI completion presentation, removing stale cached status fallbacks from sidebar loading and completion handling.
- Removed legacy dmux state auto-merge compatibility and old memory extraction response schema compatibility that are no longer used by current Codux releases.

### Fixed

- Hardened pet progression around project add, remove, and reopen flows so stale project baselines are pruned automatically and historical tokens cannot be replayed into hatch or XP progress.
- Tightened runtime hook and polling coordination by ignoring tool-use/internal Codex memory sessions more precisely and matching managed hook cleanup by tool, reducing stale activity updates and duplicate hook state.
- Fixed memory extraction queue recovery for missing projects so abandoned extraction tasks are dropped cleanly instead of surfacing a persistent failure state.

## [0.5.10] - 2026-04-24

### Added

- Added per-provider test buttons in AI settings so configured memory extraction models and API credentials can be verified directly.
- Added per-tool runtime configuration groups for full-access mode, terminal launch default models, and global prompt injection across supported tools.

### Fixed

- Restored legacy dmux project/workspace configuration by merging old project state into the new Codux app support storage without overwriting current settings.
- Fixed terminal launch model overrides so Codex receives `--model=...`, Claude/Gemini/OpenCode receive `--model ...`, and blank model fields leave each CLI default untouched.

## [0.5.9] - 2026-04-24

### Changed

- Left built-in CLI memory extraction models blank by default for new settings, so Claude, Codex, Gemini, and OpenCode use their own CLI-configured default models unless the user explicitly enters one.

## [0.5.8] - 2026-04-24

### Fixed

- Rebuilt floating tooltips on a borderless AppKit panel anchored to each hovered control, keeping release-build hover labels positioned correctly without SwiftUI overlay clipping or system popover chrome.

## [0.5.7] - 2026-04-24

### Fixed

- Fixed release-build floating tooltips being stretched by their SwiftUI overlay container, keeping hover labels compact and anchored to the hovered control.

## [0.5.6] - 2026-04-24

### Fixed

- Fixed project loading stability so active AI responses stay visible until an explicit completion, interruption, or runtime idle event instead of expiring from a timer.
- Fixed Codex stale Stop hook handling so completion from an older interrupted turn cannot clear the loading state of a newer prompt.

## [0.5.5] - 2026-04-24

### Fixed

- Fixed Codex memory extraction model overrides by passing the configured model with the current Codex CLI `--model=...` form and updating the built-in Codex default to `gpt-5.3-codex`.

## [0.5.4] - 2026-04-24

### Fixed

- Replaced global floating tooltip windows with local SwiftUI overlays so sidebar and title-bar hover labels stay anchored to their controls in release builds.
- Fixed Codex memory extraction so Codux no longer forces the built-in default model over the user's local Codex provider configuration.

## [0.5.3] - 2026-04-24

### Fixed

- Fixed release-build AI memory extraction by resolving CLI paths from the user's login shell environment, so background Codex, Claude, Gemini, and OpenCode workers can find user-installed binaries even when Codux is launched from Finder.
- Fixed title-bar floating tooltips in release builds by resolving the anchor from the control's real AppKit frame, keeping hover labels attached to the correct button after packaging.

## [0.5.2] - 2026-04-24

### Changed

- Removed the debug DMG from the formal GitHub Release asset workflow; debug packages remain available through the manual test-build workflow.

### Fixed

- Fixed release-build floating tooltips by anchoring them to the real control overlay and presenting them with stable screen coordinates.
- Fixed AI memory extraction workers so Claude, Codex, Gemini, and OpenCode provider runs skip Codux terminal wrappers and resolve the real user-installed CLI instead.

## [0.5.1] - 2026-04-24

### Added

- Added an Automatic memory extraction provider mode that uses the current terminal tool first, then falls back to provider priority.

### Fixed

- Fixed release-build floating tooltips so title-bar hover labels stay anchored to their buttons instead of rendering lower in the window.
- Fixed AI memory extraction in release builds by giving background provider workers the same CLI search paths used by managed terminals.
- Fixed memory extraction failures so the title-bar memory indicator stays red and shows the latest concrete failure reason instead of falling back to idle.
- Fixed a crash when Codex exits before reading memory-extraction stdin by converting the broken pipe into a recoverable extraction failure.
- Fixed Codex interrupted-turn activity handling so a stale stop hook no longer clears the left-sidebar loading indicator while a follow-up response is already running.
- Clarified missing CLI errors so memory extraction reports that the Claude/Codex/Gemini/OpenCode CLI is missing from the application PATH instead of surfacing raw `/usr/bin/env` output.

## [0.5.0] - 2026-04-24

### Added

- Added the first AI memory system with SQLite-backed user memory, project memory, extraction queueing, compact merged project summaries, and limited working-memory injection for supported AI tools.
- Added AI settings for built-in and custom providers, including Claude, Codex, Gemini, OpenCode, and OpenAI-compatible extraction providers with model, base URL, API key, and memory-extraction controls.
- Added a lightweight memory status indicator in the title bar so extraction activity and queue state are visible without opening settings.
- Added terminal environment loading for project `.env` files when present, making configured AI CLI credentials and proxy variables available consistently inside Codux-managed terminals.

### Changed

- Renamed the settings Tools section to AI and moved runtime permissions, provider setup, and memory controls into one AI-focused settings surface.
- Kept appearance theme/background changes on the stable restart-required path instead of live-applying them to existing terminal surfaces.
- Updated README troubleshooting paths to the current Codux support directory and runtime log filenames.

### Fixed

- Fixed project terminal focus drift after long sessions by ignoring stale focused terminals from other projects and clearing hidden terminal responders when switching projects.
- Fixed closing the last visible terminal split so Codux now terminates the old session and starts a fresh project terminal instead of leaving the workspace blocked or refusing the action.
- Fixed long-running AI activity state renewal so hook-driven loading indicators stay tied to the active runtime session instead of expiring or reviving from stale state.
- Fixed Gemini/OpenCode/Codex runtime environment handling across managed terminals and memory extraction workers, including compatibility with custom API base URLs and credentials.

## [0.4.5] - 2026-04-24

### Changed

- Updated the bundled Ghostty package to the latest AppKit input-fix revision so Codux inherits the upstream terminal input handling fixes without carrying local compatibility shims.
- Reduced runtime log noise by suppressing repetitive activity-resolution, unchanged history-index, socket receive, and no-op hook ingress entries while keeping state transitions, failures, and actionable notification diagnostics visible.

### Fixed

- Fixed project AI activity state handling so left-sidebar loading and completed indicators now stay driven by real hook/runtime session state instead of being revived by tool-use hook noise, stale project activation recalculation, or unrelated realtime session probes.
- Fixed Codex and Claude hook ingestion so queued turns, interrupted turns, and runtime backfill edge cases resolve more consistently across prompt submission, completion, and follow-up turn start boundaries.
- Fixed managed hook installation cleanup so obsolete Codex and Claude tool-use hook registrations are stripped from app-managed config, preventing redundant hook traffic from older generated entries after runtime support refresh.

## [0.4.4] - 2026-04-23

### Fixed

- Fixed pet progression so newly indexed history from a project no longer grants retroactive pet XP the first time that project enters tracking; each project now establishes its own baseline before future growth is counted.
- Fixed project removal semantics for pet progression so removing or closing a project also clears that project's pet baseline, preventing large delayed XP jumps if the same project is re-added and re-indexed much later.

## [0.4.2] - 2026-04-23

### Changed

- Reworked app-owned runtime path resolution so logs, pet state, runtime support files, and tool-permission state now live under the active app's own Application Support directory, while transient runtime sockets and status files live under an owner-scoped temp root.
- Simplified debug/runtime log naming so release and development builds no longer rely on extra `.dev` or `.release` filename suffixes for separation; build identity now comes entirely from the app container path.

### Fixed

- Fixed multi-build hook coexistence for Codex, Claude, and Gemini by making injected dmux hook commands owner-aware, preserving other active app owners, and aggressively removing legacy ownerless hook entries from older helper paths.
- Fixed Codex config installation so `suppress_unstable_features_warning = true` is enforced as a real top-level TOML key instead of being written into nested notice tables, preventing startup warnings and invalid config structure after app bootstrap.
- Fixed runtime bootstrap path partitioning so `claude-session-map`, runtime socket files, and agent status state are now treated as temporary runtime artifacts instead of leaking into persistent support storage.
- Fixed pet storage migration for existing installs by auto-moving legacy `Application Support/dmux*/pet-state.dat` files into the new app-owned container and re-encrypting them under the current runtime namespace on first load.
- Fixed release cleanup metadata so the generated Homebrew cask zap path now matches the real Application Support directory used by current builds.

## [0.4.1] - 2026-04-23

### Fixed

- Fixed Codex runtime config installation so `suppress_unstable_features_warning` is now written as a top-level config key instead of corrupting `[notice.model_migrations]`, which previously caused Codex startup failures on updated user configs.
- Fixed live AI session presentation and aggregation edge cases so completed sessions remain visible in the realtime panel, current-session token cards stay bound to raw live totals, and overlay-only math no longer leaks into the per-session display path.
- Fixed runtime and historical AI accounting edge cases across completed-turn baselines, post-cutoff indexed session buckets, corrupted active-duration history rows, and stale managed-session cleanup so project totals, pet progression inputs, and live overlays stay aligned more reliably.
- Fixed runtime hook/bootstrap support for release builds by tightening socket/config handling and adding regression coverage around Codex config generation, runtime socket reconnectability, and live stats/session retention behavior.

## [0.4.0] - 2026-04-23

### Changed

- Replaced the terminal backend with the Ghostty stack and completed the follow-up workspace integration work so split panes, detached terminal windows, project switching, and restored terminal sessions now run on one consistent rendering path.
- Split several oversized app, terminal, AI stats, Git, settings, history-indexing, and pet modules into smaller focused units to keep the codebase easier to maintain and reduce future regression risk.
- Tuned AI history indexing profiles and Ghostty appearance handling, including curated bundled Ghostty themes and lower-overhead background indexing behavior.
- Refined pet progression, trait scoring, localized trait tooltips, and realtime refresh flow so pet state follows post-claim activity more consistently and remains easier to reason about.

### Fixed

- Fixed Ghostty terminal lifecycle issues across project switching, floating windows, restored terminals, bridge refreshes, and detached terminal handoff so terminal content, input, and scrolling stay stable during workspace transitions.
- Fixed live AI runtime accounting so tool switching, completed turns, indexed-history overlays, and realtime project totals no longer leak old session totals, double-count overlays, or drift between live and indexed views.
- Fixed historical AI session queries used by pet progression and statistics so post-cutoff session buckets are counted with the correct time boundary semantics instead of dropping ongoing sessions or mixing in the wrong totals.
- Fixed bundled Ghostty theme color parsing so selected built-in themes now resolve and apply correctly after relaunch.
- Fixed pet progression bookkeeping, stale session-watermark cleanup, and trait refresh edge cases so egg, XP, and trait state no longer drift or get inflated by orphaned realtime session state.

## [0.3.2] - 2026-04-20

### Fixed

- Fixed terminal paste and other standard Command-key editing shortcuts after the terminal key passthrough change by keeping Command-based shortcuts in AppKit instead of forwarding them as raw terminal key events.
- Fixed Git sidebar auto-refresh after terminal-driven Git operations so commits and other `.git` metadata updates now invalidate the changed-file list immediately instead of leaving stale entries behind until a manual refresh.

## [0.3.1] - 2026-04-20

### Changed

- Refined the AI stats panel so project switching now shows a lightweight summary first, defers heavier detail sections, and limits session history to the most recent 20 entries for smoother navigation.
- Adjusted the default pet reminder cadence to healthier starter values: sedentary reminders every 30 minutes, hydration reminders every 2 hours, and late-night reminders every 1 hour.

### Fixed

- Fixed queued-turn loading state handling for Codex and Claude so follow-up prompts in the same session stay in `loading` until the final queued response really settles, instead of clearing too early or getting stuck.
- Fixed terminal key passthrough so unreserved shortcuts such as `Shift+Tab` reach the underlying AI terminal correctly without being swallowed by the app shell.
- Fixed top/bottom terminal split persistence so resized tab-region height no longer resets when switching projects and returning.
- Fixed AI panel project switching stutter by synchronizing live runtime state immediately and postponing heavy detail rendering until after the lightweight panel state is visible.
- Removed the remaining pet debug controls and unused pet debug localization entries from the shipping app so release builds no longer expose internal testing affordances.

## [0.3.0] - 2026-04-19

### Added

- Added the desktop pet system to Codux, including egg claim flow, hatching, level and evolution progression, inheritance, per-stage dex, and dedicated sprite/effect resources.
- Added a dedicated `Settings > Pet` tab with pet enable/static mode controls plus configurable hydration, sedentary, and late-night reminder intervals.
- Added localized user-facing pet documentation to both READMEs and integrated feature screenshots for split workspace, Git, AI stats, daily level, and pet views.

### Changed

- Refined pet growth so trait values start at `0` when the egg is claimed, accumulate from post-claim AI activity, and refresh on an hourly cadence.
- Reworked pet personality scoring to remove tool-brand bias from wisdom, distribute long-term token growth across all attributes, and avoid collapsing into a single persona when scores stay close together.
- Reworked empathy scoring to favor real iterative repair behavior, including multi-turn debugging loops and sustained correction-heavy coding sessions, instead of only very short prompt bursts.
- Moved pet controls out of General settings into a dedicated Pet tab and tightened dex overlay interaction and copy for a more consistent user-facing experience.

### Fixed

- Fixed Claude completion handling so `Stop` now marks a finished turn directly from hook semantics, while `Idle` and `SessionEnd` still clear loading without losing the distinction between cleanup and completion.
- Fixed Codex loading stalls after non-definitive `Stop` hooks by treating settled idle probe state as a real completion signal and stopping deferred stop hooks from reasserting stale `responding` state.
- Fixed pet storage so release and development builds now use separate encrypted local `.dat` files without triggering Keychain access prompts.
- Fixed pet spotlight overlay dismissal so clicking anywhere on the dimmed background closes it reliably, with a stronger backdrop for better focus.
- Fixed late-night pet reminders to use the `23:00-06:00` window and made reminder timing follow the configured pet reminder intervals.

## [0.2.2] - 2026-04-18

### Changed

- Hardened wrapped Claude launches so Codux now resolves the real Claude binary more reliably and prefers system tool paths when starting managed Claude sessions.

### Fixed

- Reduced the risk of terminal process exhaustion by delaying hidden pane PTY startup and capping managed Claude process trees before they can exhaust the user's process budget.

## [0.2.1] - 2026-04-18

### Added

- Added terminal font-size controls in Settings > Appearance so terminal text size can be adjusted with direct numeric input.
- Added a dedicated Tools settings tab for configuring default permission mode for Codex, Claude Code, Gemini, and OpenCode launches inside Codux terminals.
- Added a Notifications settings tab with per-channel enable switches plus address/token fields for Bark, ntfy, WxPusher, Feishu, DingTalk, WeCom, Telegram, Discord, Slack, and generic webhooks.
- Added background external notification delivery for the configured notification channels so completion events can fan out without blocking the UI, with silent failure handling recorded in debug logs.
- Simplified the WxPusher notification channel to the SPT quick-send flow, removing the unused token field and aligning the setup UI with the one-parameter mode.

### Changed

- Hardened the Codex, Claude, Gemini, and OpenCode runtime drivers so loading, interrupt, resume, and per-turn live token display now follow tool-driven session events instead of unstable cross-session carryover.
- Tightened tool binary resolution inside Codux terminals so Claude now follows the exact executable path resolved by the user's current shell environment rather than guessing install locations.
- Refined the AI stats status bar so the refresh action is hidden while a stats refresh is actively running, keeping the update state focused on progress and stop controls.
- Updated the app menu's About and Updates actions to use icons and appear as one grouped app-info section.
- Refined the Notifications settings cards with channel-specific labels, localized setup copy, cleaner field alignment, and direct links to each provider's documentation.
- Hardened external notification delivery with unified request timeouts, disabled request caching, and richer debug logs for request start, latency, status codes, and sanitized response summaries.

### Fixed

- Fixed live AI usage tracking across Codex, Claude, Gemini, and OpenCode so both new sessions and restored historical sessions now start from `0` live tokens and only show per-turn token deltas after each completed response.
- Fixed tool-session rebinding across reopen, resume, interrupt, and multi-terminal paths so restored sessions no longer inherit totals, models, or loading state from the previous live session.
- Reduced live runtime log noise to keep only actionable tracing around hook/socket events, logical session lifecycle, response transitions, and token commits.
- Localized the new Tools settings copy across the app's supported languages and removed the duplicate tool-name label shown beside each permission picker.
- Fixed the Sparkle update prompt background so it no longer turns transparent after the window loses focus.
- Fixed split-pane terminal relayout so creating or resizing splits no longer compresses terminal content into broken multi-column text layouts.

## [0.2.0] - 2026-04-17

### Added

- Added Sparkle-based in-app updates backed by GitHub Releases, including automatic background checks on launch, an app-menu update action, signed `appcast.xml` generation in CI, and bundled release-update documentation.
- Added Homebrew tap publishing in the release workflow so tagged releases can update the maintained cask automatically.
- Added bilingual release-notes generation for GitHub Releases and Sparkle appcasts by combining `CHANGELOG.md` and `CHANGELOG.zh-CN.md` when both version entries exist.

### Changed

- Refined AI runtime session tracking around tool session state, terminal-to-session association, and live usage aggregation so Codex, Claude, Gemini, and OpenCode can rebuild live state more consistently across reopen, resume, and multi-terminal paths.
- Refined terminal split rendering and AI stats panel interaction behavior to reduce layout instability, improve hover handling, and keep panel interactions smoother under frequent updates.
- Documented the release/update flow, Homebrew install path, and changelog maintenance process so ongoing development notes stay under `Unreleased` until a version is cut.

### Fixed

- Fixed updater packaging so release builds embed the Sparkle public key, ship a signed `appcast.xml`, and can surface embedded release notes directly inside the update dialog.
- Fixed release-note publishing so the generated notes can fall back to English when a matching Chinese changelog entry is missing instead of blocking the release flow.

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
