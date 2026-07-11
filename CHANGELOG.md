# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [2.0.0-rc.9] - 2026-07-11

### Changed

- Upgraded Iroh to 1.0.2 and aligned the mobile controller runtime with the process-wide shared endpoint lifecycle.
- Simplified remote resource synchronization across desktop, mobile, and headless Agent with consistent subscription ownership and cleanup.

### Fixed

- Significantly reduced mobile startup, foreground resume, and host-switch reconnect time by preserving an in-progress connection instead of restarting it.
- Fixed premature mobile connection failures caused by a legacy timeout expiring before the native Iroh connection budget.
- Hardened remote authorization changes, device disconnect cleanup, terminal synchronization, AI statistics, memory, project, worktree, file, and Git resource updates.

## [2.0.0-rc.8] - 2026-07-10

### Changed

- AI CLI management commands such as login now pass through directly, while resume and restore commands continue to receive Codux launch context.
- Unified terminal, worktree, and project lifecycle aggregation across local and remote sessions with deterministic status priority.

### Fixed

- Fixed remote terminal recovery after host restarts, device reconnects, worktree changes, and stale session cleanup so panes no longer remain bound to dead terminals.
- Fixed worktree terminal cleanup and rapid switching so existing terminal layouts are preserved without duplicate initialization or stale mappings.
- Fixed mobile terminal creation transactions so send failures, host errors, immediate process exits, authorization changes, and transport disconnects cannot leave the UI stuck in a creating state.
- Fixed headless Agent disconnect cleanup by removing stale terminal subscriptions, output acknowledgements, and AI statistics watchers for offline devices.
- Fixed terminal lifecycle summaries when working and error states coexist, removing nondeterministic project and worktree indicators.

## [2.0.0-rc.7] - 2026-07-09

### Fixed

- Fixed Git repository detection on Windows when trusted-directory prompts or stale repository caches prevented the Git sidebar from loading.
- Fixed rapid worktree switching so terminal panes keep their existing sessions instead of being reinitialized during layout changes.
- Improved terminal text contrast on light themes.
- Fixed release download links and update metadata so latest public assets remain stable across releases.

## [2.0.0-rc.6] - 2026-07-08

### Changed

- Switched terminal, worktree, and project agent lifecycle indicators to a single OSC-driven status path, with terminal events carrying explicit project and worktree scope for clean aggregation.

### Fixed

- Fixed remote terminal recovery after a host restart. When a Windows or remote host comes back online without the old PTY session, Codux now recreates the stable remote terminal from its saved launch configuration instead of leaving the pane bound to a dead session.
- Fixed remote worktree and project status indicators so they aggregate from the same terminal status events as the terminal list.

## [2.0.0-rc.5] - 2026-07-07

### Added

- Added a terminal right-click menu for copy, paste, select all, and prompt-preserving clear.

### Changed

- RC builds now default to the stable update channel, and stable release publishing also refreshes the beta manifest so beta-channel installs do not remain pinned to older beta builds.
- Added an opt-in raw terminal capture index for diagnosing terminal rendering regressions with the original PTY chunk boundaries.

### Fixed

- Improved Windows terminal startup by preserving core system environment variables such as `SystemDrive` and `ProgramData`, preventing literal `%SystemDrive%` directories from being created in project folders.
- Stabilized Windows ConPTY and remote terminal redraws, including cursor jumps at synchronized output frame boundaries, primary-buffer TUI recovery, baseline keyframes, and scroll restore after shorter history resyncs.
- Normalized Windows file paths before file operations so Explorer and recycle-bin helpers do not receive `\\?\`-prefixed paths.
- Moved worktree removal off the UI thread so deleting larger worktrees no longer freezes the interface.

## [2.0.0-rc.4] - 2026-07-06

### Added

- Added broader terminal protocol support, including OSC 8 hyperlinks, OSC 52 clipboard writes, OSC 133 prompt marks, OSC 1337 inline images, kitty keyboard disambiguation, bell notifications, extended underline colors, Nerd Font symbols, powerline separators, and legacy/braille glyph rendering.
- Added terminal settings for shell selection, line height, padding, and smaller font sizes down to 8px.

### Changed

- Split large desktop and runtime modules into focused submodules without behavior changes, improving reviewability and follow-up maintenance.
- Moved paste-images-as-paths into the terminal settings card and switched desktop dependencies back to the official GPUI component source.

### Fixed

- Improved Git action reliability for worktrees and remote projects, including amend-last-message, existing-repo init, branch switching, Quick Pick/Input interactions, and visible failure dialogs.
- Improved terminal stability on Windows and Linux, including paste shortcuts, PowerShell wrapper path handling, hidden console windows, conhost light-theme detection, and Windows file-preview close behavior.
- Fixed terminal rendering for non-ASCII combining marks, Nerd Font icons, powerline separators, and IME cursor bounds during reflow.
- Improved saved SSH auto-connect/reconnect behavior, `codux-ssh` non-interactive execution, SCP support, and agent relay configuration flags.

## [2.0.0-rc.3] - 2026-07-05

### Added

- Added a terminal-pane sidebar in the task column, including collapsed pane rows and per-pane agent lifecycle indicators.
- Added Japanese and Korean README translations, plus latest mobile download links and LINUX DO community support links.

### Changed

- Refined agent status indicators with lightweight pulse/spinner rendering for task, project, and terminal rows without full-window animation churn.
- Improved terminal and session list copy, selection behavior, wheel scrolling, and collapsed-session docking in the task column.

### Fixed

- Improved terminal stability under file-descriptor pressure by raising the open-file limit, killing PTY process groups, avoiding one-off shell path failure caching, and logging terminal/git/file access failures.
- Fixed agent lifecycle refresh paths so pane/worktree indicators update from the correct state domain and repaint when lifecycle transitions occur.
- Fixed stale desktop pet plan bubbles so abandoned or old plan snapshots no longer pin outdated progress text.
- Fixed collapsed task-column rerender behavior, startup repaint paths, and related agent lifecycle review findings.

## [2.0.0-rc.2] - 2026-07-04

### Added

- Added a terminal list with search and select-all support for quickly finding and managing active terminal sessions.
- Added folder context-menu actions for creating files and folders directly inside the selected directory without switching the file view.

### Fixed

- Reduced idle desktop CPU usage by avoiding redundant terminal repaint and UI invalidation work while sessions are inactive or hidden.
- Fixed remote controller cleanup so replaced or disconnected controllers shut down their stale transport tasks instead of leaving duplicate readers alive.
- Fixed agent auto-update manifest lookup so the updater selects the latest matching channel release reliably.
- Fixed Claude runtime probing so stale pre-launch transcript activity no longer keeps a project marked as actively responding.

## [2.0.0-rc.1] - 2026-07-04

### Added

- Added bottom time axes to the AI usage trend and heatmap charts, including localized month labels for the visible heatmap range.
- Added drag-and-drop restore from AI session history into terminal panes, pasting the restore command for user confirmation.

### Fixed

- Reduced Server panel host metrics polling and cached disk metrics between refreshes to avoid noisy macOS volume-space queries.
- Preserved rotated desktop runtime logs across restarts, included recent rotations in runtime log previews, and wrote Rust panic payloads, source locations, and backtraces to `runtime-rust.log`.
- Changed the default Codex reasoning effort to Default/none so Codux follows Codex's own configuration unless the user selects an explicit effort.
- Made Codex wrappers reload tool permission settings on each launch and avoid passing `model_reasoning_effort` when the effort is Default.
- Fixed PowerShell wrapper reasoning-effort validation and config-key detection to match the shell wrapper.

## [2.0.0-beta.11] - 2026-07-04

### Added

- Added local host metrics to the Server panel so local projects can inspect the same system overview as remote projects.

### Fixed

- Fixed light-theme terminal color detection for wrapped AI tools by seeding OSC 10/11 foreground and background colors before launch.

### Changed

- Refined workspace chrome with a fixed worktree/session split, clearer Server button availability, and stronger dark-theme card contrast.

## [2.0.0-beta.10] - 2026-07-03

### Added

- Added a remote host metrics panel with host capability detection, system details, CPU, memory, network, disk, and top process snapshots for remote projects.

### Fixed

- Fixed terminal split layout behavior for grid-based panes, drag placement, pane closing, floating pane restore, and split header controls.
- Fixed IME candidate placement after fast project switches so empty or newly attached terminals fall back to an in-window cursor rectangle instead of the screen corner.
- Fixed Codex and Claude resume probes so stale interrupted transcripts from previous launches no longer leave projects stuck in a running state.
- Fixed terminal font family persistence so typed settings no longer remove newer raw settings keys.
- Fixed remote host metrics edge cases for zero network totals and platforms without load averages.
- Fixed agent beta update and install lookup so beta-tagged normal GitHub releases are still selected correctly.

### Changed

- Improved the server metrics sidebar visuals with compact cards, OS icons, CPU dot grids, ring indicators, and smaller metric typography.
- Published beta GitHub releases as normal releases while keeping the beta updater channel manifest separate.

## [2.0.0-beta.9] - 2026-07-02

### Fixed

- Reduced long-running Claude runtime CPU and disk usage by parsing transcript logs incrementally and reusing the same probe cache for hook events.
- Fixed hidden terminal panes forcing full-window repaints while they are in background tabs, inactive workspace views, or closed bottom tabs.
- Reduced terminal memory growth by lowering default scrollback limits, shrinking idle remote sessions, and restoring full history only while a remote viewer is attached.
- Reduced AI history indexing memory usage by bounding oversized JSON transcript reads.

### Changed

- Kept visible terminal panes at the original frame cadence while suppressing repaint notifications only for panes that are not currently rendered.

## [2.0.0-beta.8] - 2026-07-02

### Added

- Added the remote protocol v3.2 compatibility reference in `docs/protocol.md`, covering envelope semantics, terminal subscriptions, viewport ownership, baseline/live output modes, recovery, and keepalive roles.

### Fixed

- Fixed remote terminal viewport handoff recovery across desktop, mobile, and headless agent hosts so stale baselines, owner changes, and multi-device viewing no longer leave terminals blank or corrupted.
- Fixed Iroh transport writer/read failures so dead peer senders are removed and controllers reconnect instead of silently dropping subsequent frames.
- Fixed legacy `terminal.resize` handling on desktop and agent hosts so missing or invalid dimensions are rejected instead of resizing PTYs to fallback dimensions.

### Changed

- Added host capability flags for terminal baseline failures, stale-output recovery, and viewport keyframes so clients can detect v3.2 terminal recovery features.
- Reduced mobile terminal output ack traffic while staying within the host stale-output tolerance.
- Removed unused remote resource message constants and documented deprecated terminal subscription/resize compatibility paths.

## [2.0.0-beta.7] - 2026-07-02

### Added

- Added a project-scoped database connection manager and `codux-db` runtime command so AI sessions can discover saved databases without seeing credentials.
- Added a README AI CLI support matrix that documents live status, token usage, settings support, and which tools receive non-invasive environment directives.

### Fixed

- Fixed Codux environment directive delivery so `codux-ssh` and `codux-db` instructions are injected independently of memory settings for supported AI CLIs.
- Fixed Kimi Code environment directive injection through a managed `--agent-file` and kept CodeWhale marked as non-injected for interactive sessions because it has no confirmed non-invasive prompt channel.
- Fixed memory extraction preflight gating, memory relevance ranking, secret redaction, completion-state races, and launch-artifact test isolation.
- Fixed mobile takeover of active remote terminal history by closing empty baseline loading states and moving host baseline generation out of the synchronous receive path.

### Changed

- Clarified unsupported AI CLI injection behavior for Kiro CLI, CodeWhale, and Agy instead of forcing project or user-level prompt configuration.
- Refined database and SSH launch guidance so agents discover current profiles at runtime with `codux-db list` and `codux-ssh list`.

## [2.0.0-beta.6] - 2026-07-01

### Fixed

- Fixed local and remote terminal attach rendering so startup, project switches, and new panes no longer leave bottom tabs or split panes stuck in the mounting state.
- Fixed AI CLI wrapper injection in integrated terminals so permission mode, model, reasoning effort, and other runtime settings survive third-party shell integration changes.
- Fixed Web Tunnel Browser localization coverage and related error titles across supported languages.

### Changed

- Refined the AI usage dashboard layout: responsive trend and heatmap charts, cleaner chart titles, tighter chart cards, adaptive project table widths, and consistent 12px range controls.
- Improved remote controller and host terminal lifecycle handling to reduce stale terminal state during reconnects and project switches.

## [2.0.0-beta.5] - 2026-07-01

### Fixed

- Fixed remote terminals duplicating pasted text and Ctrl-C output after repeated reconnects or project switches by replacing leaked host-side terminal event viewers instead of appending duplicate sinks.
- Fixed remote terminals sometimes rendering blank until the window was resized or input was sent by registering viewers before PTY output and replaying the baseline on reattach.
- Fixed Codex/Claude wrapper settings in the integrated terminal so permission mode, model, reasoning effort, memory injection, and SSH profile environment are applied reliably even when third-party shell integrations modify `ZDOTDIR` or `PATH`.
- Fixed SSH profile private-key selection, file-picker scrolling and operations, remote Git/file operations, and remote terminal attach behavior across the desktop app and headless agent.
- Fixed Windows release builds by removing unstable file-identity metadata APIs from rename conflict checks.

### Changed

- Restored terminal layouts immediately during project switches so moving between local and remote projects no longer waits on the asynchronous terminal-load worker.
- Centralized the remote terminal create lifecycle into shared runtime helpers used by both the desktop host and headless agent, removing duplicate host-specific wrapper paths.
- Improved `codux-ssh` one-off command performance with safer SSH connection reuse and stdin forwarding while keeping credentials inside the saved profile/helper path.

## [2.0.0-beta.4] - 2026-06-30

### Added

- Added a full-page AI usage dashboard with global usage KPIs, cached/uncached token views, charts, heatmap, and project summaries.
- Added a built-in agent diagnostics web page at `http://127.0.0.1:8765/` with health and latency checks for connection testing.

### Changed

- Refined desktop UI consistency by migrating more buttons, tabs, settings cards, and statistics controls onto shared GPUI components and shared text sizing.
- Simplified agent pairing and task-column empty states so worktrees, sessions, and empty repositories present cleaner first-run UI.

### Fixed

- Fixed AI runtime helper dispatch on headless agent hosts, keeping wrapper-based settings, memory injection, SSH profiles, and hook parsing available remotely.
- Fixed CodeWhale/Kiro runtime lifecycle and token accounting edge cases, including interrupted loading state and restored-session live usage.
- Fixed terminal focus routing, file-sidebar keyboard behavior, and responsive workspace sizing for statistics/review/sidebar layouts.

## [2.0.0-beta.3] - 2026-06-25

### Added

- Direct LAN (peer-to-peer) remote connections: the desktop or phone discovers a Codux host on the same network via iroh mDNS and links directly instead of through a relay, with a direct-vs-relay indicator on the connection badge.
- Persistent, multi-client remote terminals: switching projects or devices re-attaches the same host shell, several clients can share one agent terminal with viewport hand-off (click the phone badge to reclaim the viewport), and the headless host now forwards Ctrl-C and viewport scroll.
- Git sidebar folders now have a right-click menu — stage, unstage, discard, or add a whole directory to .gitignore — matching the per-file actions.
- Slimmer pairing QR (node id + relay url only) with device-role badges, plus a mobile fallback to import a pairing QR from a photo or screenshot when the live camera scan struggles.

### Changed

- Faster remote reconnects: a pooled controller endpoint and warm re-dial skip the cold start, and the now-unused per-output screen keyframe is no longer sent on the desktop or agent live path.
- Unified the cross-device internals: one shared remote-terminal dispatch router, a shared terminal-stream lane classifier, shared pairing-QR parsing over FFI, and worktree create/merge/remove sunk into the shared codux-git engine.
- Simplified desktop pairing into a match-then-confirm flow and dropped the separate operator dialog.

### Fixed

- Remote terminals: brought them fully online (blank panes, dropped first output, and attach-by-id root causes), stopped the screen duplicating on window resize and the duplicate/ghost first prompt on re-attach, reaped the host PTY when a terminal is closed, and guarded against double-attaching one terminal to two host PTYs.
- Connection robustness: stopped cutting off slow iroh connects at 15s, stopped a late eviction from wedging a concurrent reconnect, closed the desktop pairing sheet on success, and stopped transport churn while pairing.
- Remote projects and hosts: a host now advertises only its local projects, the status-bar badge counts saved outbound hosts correctly, and remote add-project / new-folder works across platforms with correct path handling.
- Desktop resilience: auto-recover the file tree, Git, and watchers when a project drive remounts, stop caching a failed shell-env capture so codex self-heals after a drive blip, suppress the Windows console flash on file open, and hold the pet activity line to stop multi-agent flicker.

## [2.0.0-beta.2] - 2026-06-22

### Added

- Mobile/pad workspace overhaul: live and historical AI session display, add/edit/delete SSH profiles on device, pull-to-refresh for the files/Git/review lists, one-tap "commit & push" / "commit & merge" in the review footer, and a card-style files list.
- Desktop "Star us on GitHub" nudge: a Help-menu entry plus a one-time prompt after a few days of use, localized across all 10 languages.

### Changed

- Reworked desktop transparency into a single app-opacity control with a layered frosted-glass depth scheme; refreshed cards, section titles, sidebar headers, the Git toolbar icon, and gave popup/context menus roomier rows.
- AI runtime is faster and lighter: lower-latency hook delivery (filesystem watch instead of a 3s poll), single-pass state snapshots, and no wasted rebuilds while idle.
- Unified desktop and headless-host Git, session, and worktree logic; surfaced real-time remote AI stats.

### Fixed

- AI runtime stability: stopped false "task interrupted" notifications, kept the loading/running state accurate through long model turns, and stopped closed terminals from lingering in the current-session totals.
- Localized the file context menu's "Save As…", fixed the opacity slider not filling its row, and a range of mobile terminal input, path, persistence, and review-staleness issues.

## [2.0.0-beta.1] - 2026-06-20

### Added

- Cross-device interconnect: connect the desktop (or phone) to a headless **Codux host** (`codux-agent`) running on a server, spare Mac, or Linux box, and drive its terminals, Git, AI sessions, and memory remotely over the end-to-end encrypted Iroh transport. (Beta)
- Headless Codux host app (`codux-agent`) for macOS, Linux, and Windows (x86_64 and arm64), installable as a startup service with QR-code or ticket pairing; dropped clients reconnect to the same sessions.
- Direct, always-latest per-platform download links for both the desktop app and the headless host.

### Changed

- Restructured the README around seven core pillars and positioned 2.0 as a high-performance, Rust-native, cross-device terminal built for AI coding.

## [1.9.1] - 2026-06-18

### Changed

- Refreshed the desktop and mobile README positioning and linked downloads to the latest release pages.

### Fixed

- Fixed desktop/mobile terminal viewport handoff so remote viewer ownership does not leave stale sizing or history state behind.
- Fixed missing or overlapping glyphs in the self-drawn mobile terminal renderer.
- Fixed several AI runtime probes so Claude, CodeWhale, Kiro, and related wrappers report running/loading state reliably.

## [1.9.0] - 2026-06-18

### Changed

- Reworked mobile terminal rendering to use the shared self-drawn cell-grid renderer instead of the native Termux/SwiftTerm platform views, keeping desktop and mobile on the same terminal state model.
- Migrated the shared terminal VT engine from the vendored Ghostty bindings to `alacritty_terminal`, and removed the old Ghostty/Zig release build remnants.
- Simplified mobile terminal, protocol FFI, and remote output layers by removing dead native-terminal, host-served-scroll, and orphaned input APIs.

### Fixed

- Fixed remote terminal viewport ownership across desktop and mobile so remote viewers drive columns while the host preserves normal-screen rows, with alt-screen sessions restored from keyframes instead of stale host rows.
- Fixed terminal input handling that could deliver escape-notation text through the IME commit path, and kept diagnostic tracing behind `CODUX_TERMINAL_TRACE`.
- Fixed memory extraction queue lock contention by broadening retry classification, reducing hot-path database work, and applying extracted memory in one connection/transaction.
- Fixed AI usage accounting bugs that undercounted Claude cache tokens or inflated Codex totals, then re-indexed usage history after the parser fixes.
- Fixed desktop pet LLM response races and JSON-shaped bubble text so pet messages show the intended plain text.

## [1.8.3] - 2026-06-17

### Fixed

- Fixed mobile terminal scrollback recovery for large sessions by sending native replay data from raw terminal history without mixing in screen snapshots that can clear or reposition the terminal viewport.
- Improved mobile terminal output performance by separating live append output from baseline replace output and keeping restored history bounded to the mobile scrollback window.
- Stabilized desktop terminal viewport ownership so remote resize and scrollback state do not fight the active terminal renderer.

## [1.8.2] - 2026-06-17

### Fixed

- Added right-click copy for selected desktop terminal text, clearing the selection after copying so Windows users can copy terminal output without sending `Ctrl+C` to the running process.
- Fixed mobile terminal `Ctrl+C` delivery so interrupt input is sent once immediately and is not retried as stale terminal input.
- Fixed mobile native terminal replay so Android and iOS no longer forward emulator-generated OSC/DSR replies back into the remote host input stream.
- Improved mobile keyboard lift behavior by using native cursor metrics instead of resizing the terminal viewport blindly when the software keyboard is shown.
- Improved Android terminal text selection handles by reusing the Termux material handle assets and aligning the handle anchors with selected terminal cells.
- Added iOS bundle localization declarations so native terminal selection menus can follow the app/device language instead of defaulting to English-only metadata.

## [1.8.1] - 2026-06-16

### Changed

- Switched the remote stack to the Iroh-only transport model and removed the in-repository `apps/server` relay/pairing service from the Cargo workspace.
- Updated desktop and mobile release documentation so the monorepo remains the source tag for desktop/mobile code while service deployment is owned by the separate service repository.
- Upgraded the memory extraction provider SDK to `genai` 0.6.5 and added structured-output schema support for providers that advertise compatible JSON schema response formats.

### Fixed

- Fixed memory extraction failures caused by sending JSON schema response formats to OpenAI-compatible providers that reject that mode, including DeepSeek-compatible endpoints; those providers now stay on JSON mode while OpenAI, Anthropic, and Gemini can use structured schema output.
- Improved memory extraction retries and malformed JSON diagnostics so transient empty responses or malformed provider output do not immediately fail queued extraction work.
- Cleaned stale remote architecture documents and README references that still described the removed legacy relay/WebRTC/server path.

## [1.8.0] - 2026-06-14

### Added

- Added MiMo Code as a supported AI runtime, including managed wrapper commands, runtime detection, hook integration, settings coverage, and history indexing alongside Codex, Claude Code, Gemini CLI, OpenCode, Kiro CLI, Kimi Code, CodeWhale, and Agy.
- Added shared runtime and terminal crates for the cross-device architecture: protocol, transport, terminal core, terminal PTY, runtime core, and protocol FFI now provide the common foundation for desktop, mobile, server, and future headless hosts.
- Added remote host runtime events for terminal layout changes so controller-created splits, tabs, and closes can be reflected by desktop without polling.
- Added v1.8.0 server release packaging for Linux deployments so the relay/service binary can be published from the same tag as the desktop release.

### Changed

- Standardized the remote terminal flow around a single model: transport/protocol messages enter the runtime/terminal session model first, then desktop and mobile render from that state instead of owning separate terminal history paths.
- Reworked mobile terminal rendering around the shared headless terminal screen model, including scrollback, cursor placement, input handling, terminal font sizing, and mobile-safe viewport behavior.
- Reworked remote layout synchronization so project, worktree, split, tab, and terminal identifiers are resolved through the same runtime relationship model used by the desktop host.
- Improved WebRTC/WebSocket transport status handling so latency, direct/relay path changes, and reconnect state are surfaced through the transport layer instead of scattered UI fallbacks.
- Improved mobile settings typography with separate application and terminal text-size controls and platform-appropriate defaults.
- Updated release automation so desktop and server assets can be produced from the main repository tag while the mobile release repository can reuse the same source tag for Android and iOS builds.

### Fixed

- Fixed mobile-created terminal splits and tabs not appearing on desktop, including the reverse close path and stale layout reconciliation.
- Fixed deletion of the last terminal in a project/worktree scope by preventing invalid empty layouts and keeping host/controller state aligned.
- Fixed mobile worktree switching losing the selected task, showing empty terminal panes, or requiring manual split selection before the terminal changed.
- Fixed duplicate terminal output and blank lower viewport regions caused by resize/layout messages being replayed as terminal content changes.
- Fixed restored TUI sessions on mobile showing only a partial screen or losing scrollback after large history recovery.
- Fixed cursor placement, terminal character width, IME backspace/delete handling, and mobile terminal tap behavior after switching to the shared terminal model.
- Fixed remote layout updates relying on tick/poll behavior; terminal layout changes now publish explicit runtime events.
- Fixed desktop path/cwd display for worktree terminals so terminal metadata follows the associated worktree instead of falling back to `~`.
- Fixed mobile project/worktree selection state being reset by unrelated project switches; each controller keeps its own active view while consuming the shared host model.
- Fixed terminal list/listener noise that could trigger unnecessary history reload overlays, duplicate layout refreshes, or delayed mobile rendering.

## [1.7.6] - 2026-06-09

### Added

- Added Kimi Code CLI as a first-class AI runtime driver with `kimi` and `kimi-code` wrapper aliases, managed hooks, runtime status, settings, history indexing, and session fork support.
- Added Kimi Code settings for permission mode and model selection, with model arguments applied through the runtime wrapper without injecting unverified permission flags.
- Added Kimi Code history indexing for `~/.kimi-code/sessions/**/state.json` and `agents/*/wire.jsonl` session data.

### Changed

- Updated the README feature overview to include Kimi Code alongside Codex, Claude Code, Gemini CLI, OpenCode, Kiro CLI, CodeWhale, and Agy.

### Fixed

- Fixed runtime tool configuration counts and permission summaries so newly supported tools are written into the managed wrapper settings file.

## [1.7.5] - 2026-06-09

### Added

- Added the v3.1 remote protocol layer with shared protocol constants, host capabilities, terminal-buffer chunk payloads, and protocol documentation for future cross-device runtimes.
- Added host-side terminal subscriptions so remote clients only receive terminal streams they are subscribed to, keeping project switching and multi-pane sessions bounded.
- Added terminal-buffer chunking for remote history recovery, including bounded payload windows for large Codex resume histories.
- Added an overlay scrollbar to the desktop terminal while keeping mouse wheel scrolling behavior unchanged.

### Changed

- Reworked the desktop remote host path so project, terminal, and terminal-buffer messages are produced through the v3.1 runtime protocol instead of ad hoc payload construction.
- Improved remote PTY snapshot and viewport handling so desktop and mobile clients can mount terminal state without fighting over active UI resize.
- Improved the desktop terminal selection model so drag selection is tracked by the terminal grid and remains stable while new output rotates scrollback.
- Standardized memory extraction transcript boundaries so extraction uses the configured recent transcript window, compacts low-signal logs, and avoids sending oversized noisy prompts.

### Fixed

- Fixed remote terminal history recovery for oversized output by chunking buffers and keeping output assembly compatible with weak network ordering.
- Fixed remote protocol replay handling around channel sequence checks so valid cross-channel project, terminal, and host messages are not dropped during rapid switching.
- Fixed remote terminal subscription cleanup so stale sessions do not keep receiving unnecessary output after project or pane changes.
- Fixed terminal drag selection being displaced when new output arrives during selection.
- Reduced terminal drag-selection repaint churn by coalescing selection updates and skipping unchanged cell updates.
- Fixed high memory extraction failure rates caused by hard-coded transcript limits and provider empty-response bursts; transient provider failures are retried before the task is marked failed.

## [1.7.1] - 2026-06-08

### Changed

- Improved mobile remote synchronization by keeping project, terminal, selected project, active session, and pending project selection state inside a dedicated runtime store.
- Limited mobile terminal full-buffer requests to bounded windows so large Codex resume histories do not overwhelm low-bandwidth mobile sessions.
- Moved v3.1 remote protocol constants, host capabilities, and terminal-buffer chunk payload generation into a dedicated Rust protocol module.

### Fixed

- Fixed encrypted remote message replay checks so small cross-channel packet reordering no longer drops valid project, terminal, or host-info messages during rapid project switching.
- Fixed rapid mobile project switching so `project.selected`, `project.list`, `terminal.list`, terminal binding, and `terminal.buffer` follow one stable confirmation path without blank terminal states.
- Fixed redundant project-select recovery requests that could be triggered when `project.list` arrived before the refreshed `terminal.list`.
- Fixed desktop host sequence validation to use the same per-channel sliding-window replay guard as mobile while still rejecting duplicate and stale encrypted messages.
- Fixed mobile terminal-history recovery for oversized output by assembling v3.1 chunked buffers with progress, duplicate-chunk handling, out-of-order delivery support, and bounded mobile memory limits.

## [1.7.0] - 2026-06-08

### Added

- Added the official v3 remote stack for Codux Desktop and Codux Mobile, with WebRTC DataChannel as the preferred direct path and WebSocket relay as the fallback path.
- Added stateless relay ticket pairing so QR codes stay small while the relay only exchanges short-lived pairing payloads.
- Added global, China, and custom relay presets backed by `https://codux-node.dux.plus` and `https://codux-service.dux.plus`.
- Added CodeWhale to the unified AI runtime driver architecture, including runtime status, hooks, history indexing, model/session probes, and memory injection.
- Added child-window file editing and preview flows, Markdown source/preview layout, media preview, clipboard image paste, terminal URL interactions, project/tab drag ordering, and terminal tab rename dialogs.

### Changed

- Replaced the Iroh desktop remote host path with the v3 relay/WebRTC transport factory so protocol differences stay behind transport drivers.
- Standardized remote pairing, host info, status probes, latency reporting, project selection, and terminal synchronization around one desktop/mobile protocol model.
- Reworked AI runtime integration so Codex, Claude, Gemini, Kiro, Agy, OpenCode, and CodeWhale provide hooks, probes, history sources, and memory behavior through per-tool drivers.
- Updated Windows packaging to use the current installer flow without an extra console window and reduced public release assets to the installer/updater files users actually need.
- Preserved pet progress and token watermarks across older state files and project removal/re-addition.

### Fixed

- Fixed mobile project selection so the desktop host creates or restores the project terminal before sending terminal snapshots.
- Fixed mobile first-load terminal blank states by making host info, project list, terminal list, buffer replay, and native terminal controller replay part of the standard startup path.
- Fixed remote status, reconnect, latency, device removal, relay preset switching, and re-pairing flows around the new v3 protocol.
- Fixed settings, child-window, Markdown preview, Git review, clipboard paste, file tree restore, split/tab drag, terminal color/link, and bottom-pane restore regressions found during the 1.7.0 validation cycle.

### Notes

- Existing mobile pairings should be paired again after upgrading to 1.7.0 because the remote protocol and relay ticket payload are now v3.

## [1.7.0-beta.1] - 2026-06-07

### Changed

- Replaced the remote transport stack with the v3 relay/WebRTC protocol, using WebRTC DataChannel for direct paths and WebSocket relay as the fallback path.
- Standardized remote pairing and host info payloads around transport candidates so desktop and mobile share the same protocol model.
- Removed the Iroh runtime transport from the desktop remote host path and moved relay/WebRTC differences behind the transport factory.

### Fixed

- Added WebRTC signaling handling inside the transport driver so business runtime messages stay independent from the active transport path.
- Added public STUN candidates for WebRTC ICE using Xiaomi first and Google as backup.

## [1.6.7] - 2026-06-07

### Fixed

- Fixed file sidebar expanded folders being overwritten by empty async load state after switching projects, tasks, or worktrees.
- Fixed Git clone refresh clearing the file tree expansion state instead of preserving expanded folders that still exist.

## [1.6.6] - 2026-06-07

### Fixed

- Fixed Codux child windows being forced back above the main window during normal in-app focus changes, while still restoring them when returning from another app.

## [1.6.5] - 2026-06-07

### Added

- Added child-window file editing and preview flows for non-file views, including Markdown source/preview layout and media previews.
- Added clipboard image paste support for the file sidebar and optional terminal clipboard image path insertion.
- Added foreground restoration for Codux child windows when the app is reactivated.

### Changed

- Improved AI session clone cleanup so oversized tool output, screenshots, injected memory, and cloned-session boilerplate are filtered from restored context.
- Improved remote relay configuration copy and default handling for Iroh pairing.
- Refined file preview/editor sizing, localization, and unsupported-file fallback behavior.

### Fixed

- Fixed Markdown preview visibility, scrolling, and selectable text behavior in preview windows.
- Fixed Git review file opening for added files and simplified Git diff preview actions.
- Fixed file sidebar clipboard paste crashes and added runtime coverage for copied image payloads.
- Fixed child file windows staying behind the main window after switching away from Codux and back.

## [1.6.4] - 2026-06-06

### Added

- Added drag reordering for projects, file tabs, and bottom terminal tabs.
- Added a terminal tab rename child window with localized labels across all supported languages.

### Changed

- Preserved existing pet progress and token watermarks when loading older pet state files.

### Fixed

- Fixed AI history titles so injected Codux memory, global prompts, and launch context are not used as session titles.
- Fixed pet state compatibility for older custom pet progress fields.
- Fixed terminal tab rename copy so it uses the shared 10-language localization bundle.

## [1.6.3] - 2026-06-06

### Added

- Added terminal URL interactions with OSC-8 links, web-link detection, hover underline, and Cmd/Ctrl-click opening.
- Added xterm-style terminal color scheme query and update reporting so supporting TUIs can react to light/dark theme changes.

### Changed

- Refreshed reused terminal pane renderer settings after project and worktree switches so restored panes use the current terminal theme configuration.

### Fixed

- Fixed stale default terminal foreground, background, and cursor colors after switching between light and dark themes.
- Fixed terminal panes keeping an old color palette after project or worktree restoration.

## [1.6.2] - 2026-06-06

### Added

- Added CodeWhale support across AI runtime detection, hook ingestion, history indexing, model/session probing, wrapper scripts, and permission settings.
- Added per-tool AI runtime drivers for Codex, Claude, Gemini, Kiro, Agy, OpenCode, and CodeWhale so hooks, probes, history sources, and memory injection share one extension path.
- Added SSH launch context injection for AI tools so `codux-ssh` is available in managed CLI sessions without exposing saved credentials.

### Changed

- Reworked normalized AI history indexing around source drivers and added CodeWhale transcript parsing.
- Centralized AI memory injection behavior into tool drivers instead of duplicating wrapper-specific logic.
- Improved runtime hook installation so terminal creation is not blocked by hook setup.
- Improved remote host info and Iroh address handling for Node ID pairing and direct-address upgrades.
- Refined terminal PTY restoration, viewport/model state handling, focus recovery, and programmatic command injection across platforms.
- Updated Windows packaging so release builds no longer open with an extra console window.
- Expanded release packaging tests for Windows installer assets and release artifact filtering.

### Fixed

- Fixed programmatic terminal commands on Windows by sending the platform enter sequence instead of `\n`.
- Fixed `codux-ssh` non-interactive commands so long-running or password-based commands exit cleanly.
- Fixed SSH profile context menus to use the shared localized Edit label instead of the longer SSH-specific label.
- Fixed SSH profile testing feedback so the editor footer shows an inline status indicator with the connection test result.
- Fixed unsupported async hook config warnings by keeping runtime hooks on supported synchronous paths.

## [1.6.1] - 2026-06-06

### Added

- Added Iroh remote connection settings and Node ID pairing, with host info and direct addresses flowing through the runtime model protocol.
- Added per-worktree terminal layout ratio persistence so project and worktree switches keep top/bottom split and bottom panel heights.
- Wired pet voice, reminder, proactive speech, and LLM persona settings into the actual pet behavior path.

### Changed

- Reworked the remote connection path around Iroh discovery/relay and host info, removing the old HTTP relay implementation.
- Improved startup readiness, status bar refreshes, system services, history scans, memory queues, and menu actions to reduce startup and interaction stalls.
- Refined settings UI behavior, including shared selects, form alignment, separators, terminal font settings, mobile guidance, and localization copy.
- Reworked memory management views for user memories, queued work, and failures, with queue clearing plus failure refresh and deletion actions.
- Improved AI history indexing and pet experience calculations through runtime-owned daily usage, experience baselines, and project history refreshes.
- Refined terminal focus, input stability, shortcuts, file-tab close behavior, and project/worktree restoration.
- Improved Git worktree merge feedback with explicit success or failure dialogs after confirmation.
- Reduced public GitHub Release assets to the macOS DMG, macOS updater bundle, Windows installer, and `latest.json`.

### Fixed

- Fixed the macOS DMG installer presentation so it uses the standard drag-to-Applications layout.
- Fixed settings dropdowns closing unexpectedly, failing to scroll, and overflowing long option text.
- Fixed crashes and blocking behavior in settings, child windows, update windows, and pet claiming flows.
- Fixed daily rank, pet experience, project removal/re-addition baselines, and cached usage totals.
- Fixed AI session history, Git state, terminal layout, and bottom panel height refresh issues after project or worktree switches.
- Fixed edge cases in Claude/Codex runtime status, permission-wait states, completion notifications, and auto-compaction state detection.
- Fixed update checking, download progress, completion window sizing, and release note display regressions.
- Fixed release publishing so checksum files, signatures, zip files, and intermediate executables are no longer uploaded while updater signatures still feed `latest.json`.

## [1.6.0] - 2026-06-05

### Added

- Added the Iroh-based remote transport foundation and aligned the remote protocol around shared runtime models for desktop and mobile clients.
- Added terminal font family selection backed by runtime system-font discovery.
- Added startup readiness and runtime busy status reporting in the status bar.

### Changed

- Reworked settings selects, dialog actions, update windows, memory management views, pet controls, and remote mobile guidance for a more consistent GPUI interaction path.
- Moved startup scanning, system services, memory queue work, menu actions, project dialogs, and other blocking operations further into runtime-managed async flows.
- Improved AI memory extraction scheduling, queue visibility, failure handling, and locale-aware prompts.
- Refined terminal rendering, focus recovery, input stability, shortcuts, font handling, and project/worktree switching.

### Fixed

- Fixed settings dropdowns closing unexpectedly or failing to scroll.
- Fixed settings-window crashes caused by reading app state while it was already being updated.
- Fixed project removal, worktree switching, Git status updates, AI history refresh, and desktop pet chat/layout edge cases.
- Fixed update download/progress window sizing and release-note display regressions.

## [1.5.2] - 2026-06-05

### Fixed

- Fixed daily level token totals so the workspace rank uses today's normalized global AI usage instead of historical project totals.
- Fixed update signature verification for Tauri-style base64-wrapped minisign signatures.
- Fixed update dialog release notes rendering and supported additional manifest note fields.
- Hid technical download paths from the update completion message.
- Refined child dialogs, settings, project and worktree forms, Git status indicators, and desktop pet chat rendering.

## [1.5.1] - 2026-06-04

### Fixed

- Fixed settings and child-window theming so auxiliary windows consistently inherit the current light or dark appearance.
- Improved terminal rendering, project/worktree switching, split restoration, keyboard shortcuts, and long-session redraw stability.
- Fixed AI runtime status ingestion for Codex and Claude sessions, including startup cleanup and completion notifications.
- Reduced memory extraction queue pressure and memory growth by limiting automatic batches and reading transcript content only while processing.
- Refined desktop pet rendering, claim dialogs, notifications, and project baseline filtering.
- Improved remote mobile terminal synchronization, protocol compatibility checks, and split ordering.

## [1.5.0] - 2026-06-04

### Changed

- Rebuilt Codux Desktop as a Rust-native GPUI application, replacing the previous Tauri/WebView desktop shell while keeping the existing product scope and user-facing workflows as the release baseline.
- Moved the main window, terminal workspace, project and worktree navigation, file browser, Git review, AI runtime status, settings, update dialogs, and desktop pet surfaces onto the native GPUI rendering path.
- Consolidated runtime state, project/worktree UI state, terminal layout, file editor layout, Git review state, and desktop pet baseline handling around the Rust runtime and local cache stores.
- Updated the release and updater pipeline for the Rust-native desktop build, including GitHub Releases metadata, Windows installer packaging, macOS app packaging, and the separate notarized macOS formal package.

### Notes

- This is the first stable 1.5.0 GPUI baseline for Codux. It focuses on the desktop architecture migration from Tauri to Rust + GPUI and does not introduce a separate feature expansion over the existing Codux workflows.
- Includes the 1.5.0-beta.1 validation cycle and fixes for GPUI terminal restore, project/worktree state recovery, AI runtime status, desktop pet behavior, remote mobile terminal compatibility, and the Tauri-compatible updater artifact format.

## [1.5.0-beta.1] - 2026-06-03

### Changed

- Rebuilt Codux Desktop as a Rust-native GPUI application, replacing the previous Tauri/WebView desktop shell while keeping the existing product scope and user-facing workflows as the release baseline.
- Moved the main window, terminal workspace, project and worktree navigation, file browser, Git review, AI runtime status, settings, update dialogs, and desktop pet surfaces onto the native GPUI rendering path.
- Consolidated runtime state, project/worktree UI state, terminal layout, file editor layout, Git review state, and desktop pet baseline handling around the Rust runtime and local cache stores.
- Updated the release and updater pipeline for the Rust-native desktop build, including GitHub Releases metadata, Windows installer packaging, macOS app packaging, and the separate notarized macOS formal package.

### Notes

- This is the first 1.5.0 GPUI baseline beta for Codux. It focuses on the desktop architecture migration from Tauri to Rust + GPUI and does not introduce a separate feature expansion over the existing Codux workflows.

## [1.0.8] - 2026-05-27

### Includes 1.0.7

#### Changed

- Removed HeroUI from the desktop interaction layer and replaced the affected buttons, menus, dialogs, popovers, forms, and tooltips with local components built on React Aria and Floating UI.
- Reduced the default terminal history from 2000 lines to 500 lines to lower long-session memory and restore overhead.
- Disabled the xterm WebGL renderer on macOS to avoid intermittent WKWebView idle CPU spikes after terminal split, resize, and close operations.

#### Fixed

- Fixed terminal menu and text-input handling so shifted punctuation and menu selections work consistently after terminal focus.
- Fixed terminal scroll position preservation when creating tabs, creating splits, resizing panes, and restoring long sessions.
- Aligned the titlebar performance HUD, memory status button, and remote connection pill to the same visual height.

### 1.0.8 Updates

#### Fixed

- Fixed desktop submenu switching so sibling and nested branch menus no longer leave stale flyouts behind while hovering.
- Fixed Git branch submenus to consistently open from the left side of the Git panel menu.
- Fixed AI session history context menus after terminal focus by removing the broken rename action and making menu item activation work on pointer down.
- Fixed file-sidebar delete confirmation actions and shortened the confirmation bar buttons to Confirm and Cancel.
- Added missing 10-locale translations for terminal history settings and the shared Confirm action.

## [1.0.6] - 2026-05-26

### Changed

- Restored terminal resizing to the standard xterm fit addon path, removing manual `proposeDimensions()` sizing and extra resize debouncing.

### Fixed

- Fixed terminal scrollbar drift and layout jitter after repeated window resizing.
- Fixed Claude interactive hook blocking risk and Kiro managed agent config validation errors.
- Fixed terminal launches not automatically inheriting Codex `OPENAI_API_KEY` and `OPENAI_BASE_URL`, and improved long-session terminal history cleanup and memory use.
- Fixed intermittent Windows settings-window close button clicks.
- Fixed terminal project switching so stale backend snapshots no longer overwrite the restored terminal view.
- Reduced startup and performance-monitor CPU usage by removing shell-based macOS process CPU sampling.
- Improved automatic memory extraction by pacing queued background tasks and extending long memory LLM requests to 120 seconds.

## [1.0.5] - 2026-05-26

### Changed

- Moved the terminal stack back to the stable xterm.js 5.5 line used by comparable Tauri terminal apps, keeping the existing Codux input and selection compatibility patches.

### Fixed

- Fixed macOS packaged builds where Claude CLI could enter a TUI state that stopped accepting terminal input while the same flow worked in `pnpm tauri dev`.
- Fixed terminal packaging regressions caused by the xterm 6.1 beta keyboard path, while preserving existing macOS text-input, selection-release, and shortcut behavior.

## [1.0.4] - 2026-05-26

### Added

- Added source-signal sampling to project profile generation so the LLM can refine project summaries from representative entry points, routes, Tauri commands, and module declarations without reading the full repository.
- Added real desktop pet LLM speech calls through the configured AI provider, with JSON-mode responses and UI-locale-aware language selection.

### Changed

- Improved memory LLM parsing by using JSON mode and `llm-json-repair` for both project profiles and regular memory extraction.
- Refined project memory management with three-layer memory, SQLite-backed profiles, module-scoped project memories, token estimates, manual project profile refresh, and clearer local-scan fallback diagnostics.

### Fixed

- Fixed project profile refresh failures caused by providers returning fenced, repaired, or structured JSON instead of the exact original `content` shape.
- Fixed desktop pet speech not reaching the configured AI provider from the desktop pet window.
- Fixed titlebar and session-list click handling so focused terminal panes no longer require extra clicks for memory, view switching, or session restore actions.

## [1.0.3] - 2026-05-25

### Added

- Added Antigravity `agy` CLI support across the managed runtime wrappers, hook installer, session restore command generation, AI history parsing, and memory scanning.

### Fixed

- Removed duplicate available-update copy from the update dialog so release notes appear directly under the version summary.
- Fixed file sidebar double-click from another main view so the file opens immediately after switching into the file editor.
- Fixed Worktree sidebar diff statistics so changed file count, additions, and deletions stay aligned on one row.

## [1.0.2] - 2026-05-25

### Changed

- Refined the update check dialog so the latest-version state uses clearer copy and avoids repeating the same message in both the title and body.

### Fixed

- Fixed updater metadata for the stable channel so v1.0.2 is published with the correct signed platform download entries.

## [1.0.1] - 2026-05-25

### Fixed

- Fixed Windows dev startup and desktop pet visibility handling so the app no longer depends on stale window state when settings change.
- Fixed session restore double-click behavior by adding a pending state that blocks duplicate launches while a restore is starting.

## [1.0.0] - 2026-05-24

### Added

- Added the first stable cross-platform Codux release for macOS and Windows.
- Added Git commit message provider selection with Automatic, Off, and explicit AI provider modes.
- Added runtime telemetry and log rotation to make CPU, memory, GPU, AI runtime, Git, and memory extraction issues easier to diagnose.

### Changed

- Improved terminal rendering, restore behavior, WebGL defaults, project switching, and process HUD reporting for smoother long-running AI sessions.
- Refined app theming, settings surfaces, Git panel controls, SSH profile management, memory management, and desktop pet behavior.
- Moved long-running Git pull, push, fetch, sync, and remote push operations onto cancellable background work so the UI and cancel action remain responsive.

### Fixed

- Fixed AI runtime queued-task state recovery, memory extraction limits, provider error handling, and project/worktree memory sharing.
- Fixed Git action status feedback, cancellation behavior, pull/push errors, and updater release metadata handling.
- Fixed terminal CJK redraw issues, project switching terminal restoration, excessive startup refresh work, and stale frontend state writes.

## [1.0.0-beta.4] - 2026-05-22

### Changed

- Split macOS release packaging into fast unsigned builds and separately triggered Developer ID notarized formal builds, with clear asset names for each package type.
- Pinned macOS CI builds to the macOS 26 runner while keeping the deployment target at macOS 14.0 to reduce system traffic-light layout drift in packaged builds.
- Added release download guidance to the English and Chinese READMEs so users can distinguish DMG installers, updater packages, Windows installers, and updater metadata.

### Fixed

- Prevented formal macOS release jobs from overwriting existing release assets by publishing formal packages under distinct names and merging updater metadata with existing Windows entries.
- Stopped uploading updater signature files as visible GitHub Release assets while still using them to generate `latest.json`.

## [1.0.0-beta.3] - 2026-05-22

### Added

- Added a saved SSH profile modal with credential testing, double-click connect, context-menu actions, and a `codux-ssh` runtime command that AI tools can discover through the shared injected context.
- Added a dedicated update download/install progress flow after the update notes confirmation dialog.

### Changed

- Improved terminal output delivery by preserving raw PTY bytes through the React terminal runtime, reducing garbled CJK output when AI tools redraw previous terminal content.
- Tuned idle performance and memory reporting for the dev build, including lower terminal replay/scrollback limits and macOS physical-footprint based process memory sampling.
- Refined SSH panel row typography, title-bar drag behavior, update UI copy, user-agreement handling, and theme border contrast.

### Fixed

- Fixed terminal cursor blinking and removed stale terminal-input debug logging from the active runtime path.
- Fixed project/worktree activity and memory extraction background errors so they are written to the runtime live log instead of leaking noisy stderr output.
- Fixed update installation progress events so the UI can show download, install, and completion states before restart.

## [1.0.0-beta.2] - 2026-05-22

### Fixed

- Fixed AI memory extraction so completed sessions from project worktrees are indexed into the root project memory while still resolving transcripts from the active worktree path.
- Fixed the memory extraction queue to run provider calls asynchronously and publish live memory status snapshots to the React store.
- Fixed worktree removal so deleted Git worktrees are pruned from the sidebar state immediately after confirmation.

### Changed

- Moved the Memory Manager “Index Now” action into the memory target sidebar header and added visible loading/error feedback.
- Kept Memory Manager state, title-bar memory state, and Rust extraction events synchronized through `memory:status` and `memory:manager` events.

## [1.0.0-beta.1] - 2026-05-21

### Added

- Added the first cross-platform Tauri beta of Codux, covering macOS and Windows builds from the `tauri` branch.
- Added Tauri updater support backed by GitHub Release channels, with separate stable and beta `latest.json` endpoints.
- Added Windows desktop support with self-drawn chrome, Mica-compatible window effects, WebView2 terminal rendering, and PowerShell terminal defaults.

### Changed

- Ported the main workspace shell, project/worktree sidebars, Git panel, file editor, AI statistics, memory status, desktop pet, settings, and remote pairing surfaces to the Tauri application.
- Reworked project, Git, worktree, AI runtime, remote, and pet state so Rust owns durable/runtime state while the React UI subscribes to snapshots and events.
- Aligned the Tauri app identity, visible product name, updater channel naming, and release assets with the Codux replacement path.

### Fixed

- Fixed the welcome screen so project-dependent side panels and actions are hidden until a project is selected.
- Fixed Windows terminal startup, AI runtime hook ingestion, remote terminal hydration, and desktop pet CPU usage regressions found during cross-platform testing.
- Fixed the update configuration warning by wiring the signed Tauri updater key and managed GitHub channel endpoints.

## [0.10.2] - 2026-05-17

### Changed

- Updated the bundled GhosttyKit package to the latest upstream main revision for terminal rendering fixes.
- Reduced terminal and AI runtime background refresh work while sessions are active, including avoiding redundant remote terminal starts and memory artifact preparation on cached launches.
- Reworked project activity loading badges to use a lower-overhead native animation and a clearer orange count badge when multiple tasks are running.

### Fixed

- Fixed Codex config migration so legacy `codex_hooks` entries are rewritten to the current `hooks` feature flag, removing the deprecated-feature warning in managed launches.
- Fixed worktree sidebar row spacing so main tasks and subtasks keep matching height, radius, and activity-dot alignment.

## [0.10.1] - 2026-05-16

### Added

- Added multi-worktree task workspaces with clearer main-task and subtask activity indicators, worktree review mode, merge/review actions, and safe discard controls for changed files.
- Added an immersive file workspace backed by the native source editor, with top-level Terminal, Files, and Review view switching instead of opening edited files in separate windows.
- Added an experimental structured Agent split option, disabled by default, for Codex, Claude Code, and OpenCode protocol-driven chat panes.

### Changed

- Refined split-pane layout behavior so launch defaults are stable, bottom pane sizing resets cleanly on startup, and user drags are kept in memory during the session.
- Improved review comparison layout with three equal columns, editor clipping, clearer typography, and full-container sizing.
- Reduced runtime polling and session refresh work while AI tasks are running, lowering repeated background updates during Codex sessions.

### Fixed

- Fixed shortcut and focus routing so file editor shortcuts stay with the editor, terminal shortcuts stay with terminals, and Agent input focus releases correctly when another split is selected.
- Fixed divider hit areas and cursor behavior around horizontal, vertical, and hidden side panels.
- Fixed Codex hook ownership handling so formal and dev builds do not overwrite each other's managed hooks while still updating the current owner.

## [0.9.10] - 2026-05-08

### Fixed

- Updated Codex hook feature flag handling to prefer `--enable hooks`, falling back to `codex_hooks` only when older CLIs still expose the legacy flag, migrate startup-managed Codex config to `[features].hooks = true`, and trust only the Codux-managed hook hashes written by the app.
- Removed Codux-managed legacy Codex tool-use and unsupported session-end hooks, plus duplicate cross-owner managed hooks, to reduce redundant entries in the new Codex `/hooks` review flow.

## [0.9.9] - 2026-05-07

### Added

- Added an IDE launcher to the project open menu and sidebar project context menu, supporting IntelliJ IDEA, WebStorm, PhpStorm, PyCharm, GoLand, CLion, Rider, Android Studio, Cursor, Zed, Sublime Text, and Windsurf.

## [0.9.8] - 2026-05-07

### Added

- Added desktop pet context-menu controls to make the pet larger, smaller, or reset it to the default size, with the scale persisted in app settings.

### Changed

- Updated the bundled `code` and `voidcat` pet assets and regenerated their standard Codex atlases; `voidcat` now uses the new high-resolution source while the black-background `sheep` source remains validated.
- Kept desktop pet bubble text visually fixed at 14pt while the pet is scaled, preventing oversized speech text after enlarging the pet.

## [0.9.7] - 2026-05-07

### Added

- Added pet archive and restore flows in the Petdex, allowing the current companion to be archived at any level, restored later, or swapped while keeping only one active pet.
- Added custom pet installation from Codex-style pet market pages, with page URL resolution, metadata preview, editable pet names, package validation, install progress, and adoption support.

### Changed

- Refined pet speech so task status always has priority, running Codex turns can show sanitized assistant text, and random pet monologues only appear during idle time.
- Simplified Petdex and claim surfaces by removing archive history from the titlebar popover and moving custom pet entry points into adoption/Petdex flows.
- Updated pet bubble sizing and animation timing so speech is easier to read and full-frame animations play slightly faster while short-frame animations keep their calmer cadence.

### Fixed

- Fixed interrupted AI turns so pet status treats them as failures and completion/failure animations remain short-lived status reactions.
- Fixed pet progress preservation when loading newer identity state, avoiding level rollback while keeping the existing XP algorithm version unchanged.
- Fixed leftover idle-entry speech templates that made pets appear to interject at a fixed time instead of speaking randomly while idle.

## [0.9.6] - 2026-05-07

### Added

- Added an editable CodeMirror-based file preview window with syntax highlighting, save/copy/paste/undo/redo/find controls, unsaved-change protection, and theme-aware chrome.
- Added virtual read-only preview support for very large text files so large files can be opened without rendering the entire file into SwiftUI.

### Changed

- Refined the memory manager tabs into Summary, Memories, and History, with memory entries grouped by type so compacted memories remain visible instead of making core/working views look empty.
- Improved Git side-by-side diff previews so long lines scroll inside each column, without whole-window horizontal scrolling or line-number drift.
- Shared the terminal minimum bottom-pane height through one model constant to keep split layout restoration consistent.
- Isolated AI hook configuration writes during tests so test runs no longer touch the user's real tool configuration files.

### Fixed

- Fixed file preview editing so files open directly in editable mode and save back to the original project file.
- Fixed diff preview windows so `Command-W` closes them like normal app windows.
- Removed the obsolete file preview "Edit Mode" localization after the editor became always editable.

## [0.9.5] - 2026-05-06

### Added

- Added a manual memory indexing action in the memory manager, allowing completed AI sessions to be scanned on demand even when automatic extraction is disabled.
- Added manual editing for summary memories, saving edits as new summary versions for future memory injection.
- Added local Llama support to the pet LLM channel picker.

### Changed

- Renamed the built-in local Llama provider from legacy "Local Llama Memory" wording to localized "Llama Model" naming across AI settings, memory extraction, and pet LLM selection.
- Improved memory manager spacing in the sidebar and main header.

### Fixed

- Fixed local Llama provider names in memory extraction failures so built-in providers use the localized display name.

## [0.9.4] - 2026-05-06

### Added

- Added local Llama memory extraction with a bundled llama.cpp XCFramework package, a remote-refreshable model catalog, and one-click model install/remove controls.
- Added China and international model download routes, preferring ModelScope in China and Hugging Face internationally.
- Added a curated local model list covering low-end through 128 GB Macs, with multilingual descriptions and recommended memory/runtime configurations.

### Changed

- Made memory extraction prefer compact prompts for local models, with smaller transcript windows and safer queue recovery when provider configuration changes.
- Reduced terminal UI churn during resize and high-output sessions by coalescing pane ratio updates, Ghostty surface refreshes, and terminal output delivery.

### Fixed

- Fixed pet permission prompts so they use the orange attention bubble, stay visible briefly, and then restore the running status when appropriate.
- Fixed pet hydration, sedentary, and late-night reminders so they use a red warning bubble instead of the default speech bubble.
- Fixed AI waiting-input notifications so Codex permission requests are treated consistently across notification and pet bubble paths.

## [0.9.3] - 2026-05-06

### Added

- Added a global SSH side panel with create, edit, delete, and double-click connect flows for saved server profiles.
- Added a bottom terminal status bar that remains visible when all bottom tabs are closed, with a one-click new terminal action.

### Changed

- Switched the Ghostty package dependency back to the official `Lakr233/libghostty-spm` main branch.
- Simplified saved SSH launches so Codux sends a single `codux-ssh <profile>` command instead of pasting an expect script into the terminal.
- Further reduced terminal resize work by deferring Ghostty frame and viewport refreshes during live window resizing.

### Fixed

- Fixed large paste handling in Codex, Claude, and other TUI sessions by queueing PTY writes with backpressure instead of dropping or stalling input.
- Fixed saved SSH connections so local locale variables are not forwarded to remote hosts that do not have the same locale installed.
- Fixed AI loading recovery when Codex runtime polling briefly reports idle but token growth shows the turn is still running.
- Redacted saved SSH passwords and key passphrases from diagnostics exports.

## [0.9.2] - 2026-05-06

### Added

- Added project-scoped task memos for terminal sessions, including queued, waiting, and completed states, duplicate memo support, and manual send controls.
- Added automatic queued memo dispatch after an AI turn completes for the same project and terminal session.

### Changed

- Reduced AI memory token pressure with smaller default injection budgets, summary truncation, transcript extraction limits, and per-session extraction cooldowns.
- Moved split-pane terminal controls into the terminal overlay layer so they no longer reserve a separate layout column.
- Refined task memo status controls with full-width capsule hit areas, localized labels, and reusable focused multiline inputs.

### Fixed

- Reduced short CPU spikes and main-thread stalls while resizing Codex terminal windows by coalescing Ghostty viewport refreshes and ignoring transient tiny layout sizes.
- Fixed AI memory compaction so newly extracted working memories stay browseable, stable items can be promoted to core memory, and only stale working entries are automatically merged into summaries.
- Cleaned invalid version-only memory summaries so broken extraction responses no longer leave empty summary panels.
- Fixed split-pane task memo button hit testing and queued-state styling so it remains clickable and no longer clips a corner badge.

## [0.9.1] - 2026-05-06

### Changed

- Updated the bundled single-form pet spritesheets with the latest generated assets while preserving the flat `Pets/<species>/pet.json` package layout.
- Improved pet atlas normalization so frames share a consistent scale and alignment across each spritesheet, reducing size jitter between animation frames.
- Made pet animation playback adapt to the actual non-empty frame count in each atlas row, so short and long actions keep a calm cycle speed instead of being rushed or truncated.

### Fixed

- Unified titlebar and desktop pet animation selection so both surfaces now map AI activity states to the same running, review, success, failure, idle, and sleep animations.

## [0.9.0] - 2026-05-06

### Added

- Added the flat single-form Codex-style pet atlas pipeline, bundled pet packages, conversion tooling, and developer guidance for producing new white-background pet spritesheets.
- Added new built-in companion species using one `pet.json` and one `spritesheet.png` per species.
- Added Markdown split preview support in the file preview window.

### Changed

- Reworked pet presentation to use single-form atlas assets instead of egg, stage, evolution, and mega-form sprite chains while keeping existing pet progress data compatible.
- Tuned pet animation playback so short-frame actions hold longer, idle and waiting states feel calmer, and all bundled pet animations play more slowly.
- Updated pet naming and pet UI strings across supported localizations.
- Improved file-browser keyboard focus, inline rename behavior, delete confirmation flow, and Finder/file action handling.

### Fixed

- Fixed project activity loading state handling for interrupted Codex turns and missing stop hooks with a focused runtime polling fallback.
- Fixed terminal and file-browser shortcut routing so file actions only trigger while the file panel has focus.
- Fixed memory and runtime state cleanup paths that could leave stale provider or session data after project changes.

## [0.8.2] - 2026-05-05

### Added

- Added shared remote terminal splits for Codux Mobile, so mobile clients can browse, create, switch, resize, and close the same split sessions shown on macOS.
- Added WebRTC DataChannel P2P transport for remote terminal traffic, using STUN direct connection first with encrypted WebSocket relay fallback.
- Added host-side support for mobile file-manager paste, drag/drop moves, inline rename, external opening for media and office files, and terminal file-drop insertion.

### Changed

- Improved remote terminal resize ownership so mobile terminal grids can drive the shared Mac session without forcing visible macOS focus changes.
- Updated P2P ICE server ordering to prefer domestic STUN for Chinese language environments while retaining global STUN fallbacks.
- Refined the README screenshots and remote-terminal documentation for the current shared-terminal workflow.

### Fixed

- Fixed remote-created terminals so they can start in background projects and replay their history when the Mac later opens that project.
- Fixed Codex runtime snapshots so a completed turn after a 502 error clears loading instead of leaving the session stuck responding.
- Fixed duplicate remote terminal input handling by adding input IDs and host-side duplicate suppression.

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
