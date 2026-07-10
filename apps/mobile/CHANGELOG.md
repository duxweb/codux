# Changelog

Important changes to this project are documented here.

## [Unreleased]

## [2.0.0-rc.8] - 2026-07-10

### Changed

- Aligned Codux Mobile with the shared remote terminal lifecycle and ownership model used by Codux Desktop and the headless Agent.

### Fixed

- Fixed terminal creation getting stuck after send failures, host errors, immediate terminal exits, authorization changes, device removal, or transport disconnects.
- Fixed terminal and worktree selection after remote host restarts and reconnects, including automatically selecting a newly created terminal.
- Improved viewport takeover and session recovery consistency when switching projects, worktrees, terminals, and controller devices.

## [2.0.0-rc.6] - 2026-07-08

### Changed

- Aligned the Flutter mobile release with Codux 2.0.0-rc.6. No mobile pairing protocol changes are required for this release candidate.

### Fixed

- Improved compatibility with desktop and agent remote terminal status aggregation after host reconnects and restarts.

## [2.0.0-rc.5] - 2026-07-07

### Changed

- Aligned the Flutter mobile release with Codux 2.0.0-rc.5. No mobile pairing protocol changes are required for this release candidate.

### Fixed

- Improved remote terminal recovery compatibility with the desktop host's latest baseline keyframe and redraw stabilization changes.

## [2.0.0-rc.4] - 2026-07-06

### Added

- Added mobile terminal support for bundled Nerd Font symbols, vector powerline separators, broader legacy/braille glyph coverage, kitty keyboard disambiguation, and extended underline styling.

### Changed

- Aligned the Flutter mobile release with Codux 2.0.0-rc.4. No mobile pairing protocol changes are required for this release candidate.

## [2.0.0-rc.3] - 2026-07-05

### Changed

- Aligned the Flutter mobile release with Codux 2.0.0-rc.3. No mobile protocol changes are required for this release candidate.

## [2.0.0-rc.2] - 2026-07-04

### Changed

- Aligned the Flutter mobile release with Codux 2.0.0-rc.2. No mobile protocol changes are required for this release candidate.

## [2.0.0-rc.1] - 2026-07-04

### Changed

- Aligned the Flutter mobile release with Codux 2.0.0-rc.1. No mobile protocol changes are required for this release candidate.

## [2.0.0-beta.8] - 2026-07-02

### Fixed

- Fixed remote terminal restoration when switching between desktop, phone, and tablet viewers so active sessions recover from owner changes without blank or corrupted screens.
- Fixed stalled mobile terminal loading after host baseline failures by relying on explicit failed-baseline and stale-output recovery signals.

### Changed

- Added mobile parsing for v3.2 terminal recovery capabilities including baseline failures, stale output, and viewport keyframes.
- Reduced mobile terminal output ack traffic while keeping viewport leases and stale-output recovery responsive.

## [2.0.0-beta.5] - 2026-07-01

### Changed

- Aligned the mobile package with the Codux 2.0.0-beta.5 desktop and agent release. No mobile protocol changes are required for this beta.

## [2.0.0-beta.4] - 2026-06-30

### Changed

- Aligned the mobile package with the Codux 2.0.0-beta.4 desktop and agent release. No mobile protocol changes are required for this beta.

## [2.0.0-beta.3] - 2026-06-25

### Added

- Direct LAN (peer-to-peer) connection to a Codux host on the same network via iroh mDNS, with a direct-vs-relay indicator on the connection badge.
- Persistent remote terminals: returning to a project re-attaches the same host shell, and the phone can share an agent terminal with the desktop and hand off the viewport (the desktop reclaims it by tapping the phone badge).
- Import a pairing QR from a photo or screenshot as a fallback when the live camera scan struggles.

### Changed

- Clearer pairing scan: higher camera resolution and a loading state on recognition, plus a slimmer pairing QR (node id + relay url) with device-role badges.
- Faster reconnects by reusing a warm connection, and stopped cutting off slower connects too early.

### Fixed

- Terminal: fixed viewport sizing and glyph rendering, the duplicate/ghost first prompt right after connecting, and switch/contention robustness; auto-recover stalled baselines to cut latency-driven jank.
- Fixed QR scanning on some MediaTek/OPLUS devices by dropping the forced camera resolution and autozoom that starved or mis-framed the scan.

## [2.0.0-beta.2] - 2026-06-22

### Added

- Manage SSH profiles on device — add, edit, and delete saved connections directly from mobile.
- Pull-to-refresh for the files, Git, and review lists.
- One-tap "commit & push" and "commit & merge" from the review footer.
- Card-style files list and a unified list-item style shared by the Git and review lists.

### Changed

- Reworked the mobile/pad workspace: live and historical AI session display, a dedicated "Files" center view with an editor and browser sidebar, a toggleable right column, and a redesigned Git panel.
- Hardware-keyboard-first terminal input; the terminal toolbar stays available when the right column is open.
- "+" now creates a new tab instead of splitting the terminal (split panes remain desktop-only).

### Fixed

- Fixed terminal cursor keys and tap-to-type behavior.
- Fixed current-directory "/"-rooted paths, file persistence, sync spinner timing, and review staleness.

## [2.0.0-beta.1] - 2026-06-20

### Added

- Connect to a remote Codux host (`codux-agent`) and drive its terminals, Git, and AI sessions from your phone over the end-to-end encrypted Iroh transport. (Beta)

### Fixed

- Fixed terminal cursor alignment in the self-drawn terminal: the cursor now centers on the glyph like desktop, and fully covers double-width (CJK) characters instead of half.

## [1.9.1] - 2026-06-18

### Changed

- Updated README copy and release links for the current mobile release flow.

### Fixed

- Fixed mobile terminal glyph measurement so self-drawn terminal text no longer drops or overlaps characters.
- Fixed terminal viewport handoff from desktop to mobile so controller-driven sizing stays consistent.

## [1.9.0] - 2026-06-18

### Changed

- Replaced the native Termux/SwiftTerm terminal views with a self-drawn mobile terminal that renders from the shared cell grid used by the remote terminal model.
- Removed the mobile native-terminal bridge, replay controller, platform plugins, and orphaned protocol FFI facades that were only needed by the previous native-rendering path.

### Fixed

- Fixed mobile terminal restore and scrolling around large sessions by using the shared raw-history/screen-keyframe model instead of native replay snapshots.
- Fixed remote viewport ownership so mobile can drive terminal columns without forcing stale host rows into alt-screen TUI sessions.
- Fixed terminal input delivery that could send escaped notation through IME commit text, keeping diagnostic channel tracing opt-in.

## [1.8.3] - 2026-06-17

### Fixed

- Fixed large-session terminal restore so the native terminal replays raw terminal history without appending screen snapshots that can clear the viewport or jump the cursor.
- Improved live terminal output performance by routing working output through append updates and reserving replace updates for baselines, restores, and screen-only refreshes.
- Kept mobile restore history aligned with the 500-line scrollback target while preserving a bounded transfer size for large terminal buffers.

## [1.8.2] - 2026-06-17

### Fixed

- Fixed terminal `Ctrl+C` from the mobile toolbar so it sends a raw ETX once and does not retry stale interrupt input.
- Fixed native terminal replay on Android and iOS so local emulator-generated OSC/DSR responses are not echoed back into the remote host input stream.
- Improved software-keyboard lifting by using native terminal cursor metrics, keeping the cursor visible without forcing a terminal viewport resize.
- Improved Android terminal text selection with Termux material handle assets and cell-aligned handle anchors.
- Added iOS bundle localization metadata so native terminal selection controls can follow the app/device language.

## [1.8.1] - 2026-06-16

### Changed

- Aligned mobile remote transport documentation and release metadata with the Iroh-only desktop/controller protocol.
- Updated the mobile release version to 1.8.1 for the shared desktop/mobile Iroh transport and terminal fixes.

### Fixed

- Fixed mobile pairing and reconnect expectations after the legacy relay/WebRTC path was removed from the shared protocol.
- Kept mobile release notes aligned with the shared runtime, protocol FFI, memory extraction, and terminal stability fixes in the 1.8.1 source tag.

## [1.8.0] - 2026-06-14

### Added

- Added shared Rust protocol FFI usage for the mobile controller path so mobile consumes the same protocol payloads, transport metadata, latency events, and terminal layout messages as desktop.
- Added mobile-side support for controller-created split, tab, worktree, and terminal layout actions through the host runtime model.
- Added separate mobile display controls for application text size and terminal text size, with terminal presets from small to extra large.

### Changed

- Reworked the mobile terminal screen to render from the shared headless terminal model instead of a separate Dart terminal history path.
- Reworked project, worktree, split, and tab selection so mobile keeps its own active UI selection while receiving the shared host project/worktree/terminal relationship model.
- Improved terminal scrolling, cursor drawing, character-width measurement, IME input forwarding, and TUI restore behavior on iOS and Android.
- Reduced duplicated mobile fallback logic now covered by the shared runtime, protocol, and terminal crates.

### Fixed

- Fixed blank terminal panes when switching projects, switching worktrees, returning from the background, or reconnecting to a restarted desktop host.
- Fixed mobile-created split/tab actions not syncing back to desktop and fixed desktop-created layout changes not consistently appearing on mobile.
- Fixed deleting split/tab entries leaving stale mobile layout state, including the invalid "delete last terminal" case.
- Fixed terminal-history loading overlays flashing repeatedly during normal live output and project switches.
- Fixed large restored TUI sessions showing only one screen, losing scrollback, or rendering cursor/input positions incorrectly.
- Fixed IME backspace/delete, terminal tap focus, terminal font fallback, and mobile cursor rendering regressions.
- Fixed latency and transport path labels not updating after direct/relay changes or desktop restart.

## [1.7.5] - 2026-06-09

### Added

- Added v3.1 host capability parsing, terminal-buffer chunk assembly, per-session terminal replicas, terminal subscription scopes, and bounded history rendering for large terminal output.
- Added reliable terminal input helpers, output sequencing/resync helpers, upload metadata helpers, and protocol payload codecs for future cross-device runtime domains.
- Added mobile update checking, refined debug-log export, and updated controller-oriented documentation for Codux Mobile.

### Changed

- Split the mobile remote workspace into dedicated runtime store, sync, project, terminal, device, file, logging, settings, and widget layers so UI only renders state and emits user intent.
- Reworked terminal screens so active project/session selection mounts data from local replicas instead of deciding whether protocol messages should be accepted.
- Reworked project, file, worktree, device action, debug-log, and update UI into focused widgets backed by service/controller tests.

### Fixed

- Fixed intermittent blank terminal panes during first entry, project switching, desktop host restarts, and rapid split selection.
- Fixed stale mobile terminal cache after host restart by separating host-confirmed runtime state from UI-mounted terminal panes.
- Fixed terminal-history recovery instability on slower networks with duplicate-chunk protection, out-of-order assembly, input retry, and output resync helpers.
- Fixed noisy sync/list refresh behavior so repeated project and terminal list responses do not repeatedly trigger conflicting UI state changes.

## [1.7.1] - 2026-06-08

### Changed

- Consolidated mobile remote state into dedicated device selection, connection sync, runtime store, and sync-state services so UI actions only trigger intent while the runtime store owns project and terminal decisions.
- Limited terminal full-buffer recovery to bounded windows so large Codex resume histories remain usable on slower mobile networks.
- Added v3.1 host capability parsing and terminal-buffer chunk assembly so mobile consumes the same protocol API as desktop.

### Fixed

- Fixed encrypted message replay detection so cross-channel packet reordering no longer drops valid `project.list`, `terminal.list`, `host.info`, or terminal output messages.
- Fixed rapid project switching so `project.selected`, refreshed project/terminal lists, session binding, terminal resize, and terminal buffer recovery close reliably without blank terminal panes.
- Fixed redundant missing-terminal recovery requests when a refreshed project list arrived before the matching terminal list.
- Fixed first-entry and reopen flows by remembering the last responsive device and synchronizing cached projects with host-confirmed selected project state.
- Fixed oversized terminal-history loading by assembling chunked buffers with progress, duplicate-chunk protection, out-of-order delivery support, and a mobile memory ceiling.

## [1.7.0] - 2026-06-08

### Added

- Added the official v3 remote protocol for Codux Mobile, sharing the same relay/WebRTC model as Codux Desktop.
- Added stateless QR ticket pairing so the desktop can publish a short-lived pairing payload through the relay service.
- Added WebRTC DataChannel direct transport with WebSocket relay fallback, plus latency reporting based on protocol ping/pong.
- Added in-app debug logs with copy/export support and configurable log verbosity for connection and terminal diagnostics.

### Changed

- Standardized project list, terminal list, host info, transport status, and reconnect behavior around one remote protocol state machine.
- Reworked first-load synchronization so connection, hello, and transport path events actively request host info, project list, terminal list, and terminal buffers.
- Replayed native terminal buffers when the controller is created or resized so restored sessions do not open as a blank terminal.

### Fixed

- Fixed first entry after pairing or app restart showing an empty terminal until the user manually switched projects.
- Fixed background/foreground reconnect status flicker, missing latency display, and stale relay/P2P labels during transport changes.
- Fixed duplicate terminal buffer requests and replay ordering around project switching and split selection.

### Notes

- Codux Mobile 1.7.0 should be paired with Codux Desktop 1.7.0. Existing paired devices should be paired again after upgrading.

## [1.7.0-beta.1] - 2026-06-07

### Changed

- Replaced the mobile remote transport with the unified Iroh protocol model.
- Prefer Iroh direct paths when available and use configured Iroh relays when direct paths cannot connect.
- Store Iroh transport candidates from pairing payloads so reconnects use the same protocol model as desktop.

### Notes

- This beta requires the updated Codux desktop app and updated Codux relay service. Existing remote devices need to be paired again.

## [1.6.8] - 2026-06-07

### Changed

- Published a higher Android build number for release-channel P2P verification without changing transport behavior.

## [1.6.7] - 2026-06-07

### Fixed

- Added the Android `INTERNET` permission to the main release manifest so packaged builds can use the Iroh network transport like debug builds.

## [1.6.6] - 2026-06-07

### Fixed

- Ignore stored Iroh direct addresses during normal reconnects and dial with stable node identity plus relay only.
- Use fresh `host.info` direct addresses only for controlled relay-to-direct upgrade reconnects.
- Prevent repeated relay upgrade reconnects after a session has already reached a direct path.

## [1.6.4] - 2026-06-06

### Fixed

- Fixed Iroh direct-address updates so host-provided addresses are added to the native lookup table instead of being ignored.
- Prevented normal terminal navigation, project switching, and host info refreshes from triggering reconnects unless the native path is confirmed as relay or mixed.
- Added a cooldown for relay-to-direct upgrade reconnects to avoid reconnect loops when direct paths are unavailable.

## [1.6.3] - 2026-06-06

### Fixed

- Fixed Android Iroh endpoint startup after the 1.0.0-rc.1 upgrade by initializing the Android native context required by the Iroh DNS resolver.
- Added native Iroh endpoint bind progress states and timeout reporting so connection startup failures no longer remain stuck at connecting.

## [1.6.2] - 2026-06-05

### Changed

- Upgraded the mobile Iroh transport bridge to `iroh` 1.0.0-rc.1 and forwards the configured relay URL into the native transport.
- Prefer the first split terminal when opening a project so the default terminal matches the desktop layout.

### Fixed

- Fixed iOS bridge exports so `codux_iroh_add_node_addr` is available to the Swift plugin.
- Linked the iOS Iroh bridge against `Network.framework` and re-signed embedded Flutter native asset frameworks with the active build identity.

## [1.6.0] - 2026-06-05

### Added

- Added the native Iroh remote transport bridge for mobile pairing, reconnect, terminal traffic, and upload delivery.
- Added the terminal switcher screen for split terminals, tab terminals, and worktree switching.
- Added worktree create, merge, delete, and refresh actions from the mobile terminal switcher.

### Changed

- Replaced the previous terminal transport with the unified Iroh QUIC protocol path.
- Standardized pairing and reconnect around encrypted Dart protocol envelopes, keeping native transport code limited to connection and frame delivery.
- Restricted terminal file and image uploads to direct Iroh connections so large transfers never run over relay paths.
- Updated the iOS TestFlight workflow to build the Iroh bridge before archiving.

### Fixed

- Fixed mobile reconnect after desktop restarts by using Iroh n0 discovery for node address resolution.
- Fixed project switching, terminal history recovery, connection status, latency display, and host responsiveness handling on the new transport.
- Fixed iOS navigation transitions, edge-swipe back handling, terminal padding, toolbar layout, pairing overflow copy, and upload copy.

## [1.5.0] - 2026-06-04

### Changed

- Aligned the mobile client with the Codux 1.5.0 desktop protocol and GPUI terminal host.
- Improved shared terminal restore, resize, split ordering, and mobile terminal rendering across Android and iOS.
- Added a remote protocol version check during `host.info` so incompatible desktop builds ask users to update instead of connecting silently.

### Fixed

- Removed the Ghostty iOS keyboard accessory from the embedded terminal.
- Fixed Android terminal background, sizing, stale split list behavior, and project switching with the new desktop runtime.

### Notes

- Includes the 1.5.0-beta.1 mobile validation cycle for the Codux 1.5.0 desktop protocol, shared terminal restore, split ordering, Android and iOS terminal rendering, and host protocol compatibility checks.

## [1.5.0-beta.1] - 2026-06-03

### Changed

- Aligned the mobile client with the Codux 1.5.0 desktop protocol and GPUI terminal host.
- Improved shared terminal restore, resize, split ordering, and mobile terminal rendering across Android and iOS.
- Added a remote protocol version check during `host.info` so incompatible desktop builds ask users to update instead of connecting silently.

### Fixed

- Removed the Ghostty iOS keyboard accessory from the embedded terminal.
- Fixed Android terminal background, sizing, and stale split list behavior for the new desktop runtime.

## [0.1.11] - 2026-05-24

### Changed

- Tuned the Android adaptive icon foreground scale to better match the iOS and macOS app icon proportions.

### Fixed

- Added the iOS location permission purpose string required by App Store Connect static analysis for nearby connectivity APIs.

## [0.1.10] - 2026-05-23

### Added

- Added file upload from the terminal toolbar while keeping image upload on the same path-insertion flow.
- Added P2P health probing so the app only reports P2P when the DataChannel is open and responding.

### Changed

- Routed uploads through the dedicated WebRTC upload DataChannel only, preventing large uploads from falling back to relay or blocking terminal traffic.
- Blocked file and image uploads unless a healthy direct P2P upload channel is available.
- Improved inserted upload paths with platform-aware quoting for paths that contain spaces.

### Fixed

- Stabilized terminal history recovery after switching projects by preferring the direct P2P buffer request path and avoiding relay duplication.
- Fixed upload status copy for file uploads and added coverage for refused upload transports.

## [0.1.9] - 2026-05-22

### Fixed

- Pinned `device_info_plus` and `package_info_plus` to a compatible pair so iOS archives build on the current GitHub macOS runner SDK.

## [0.1.8] - 2026-05-22

### Fixed

- Pinned `device_info_plus` to an iOS SDK-compatible version so GitHub Actions can archive the iOS TestFlight build.

## [0.1.7] - 2026-05-22

### Added

- Added iOS TestFlight release automation with App Store Connect upload support.
- Added the native iOS terminal adapter backed by Ghostty so iOS uses the same Dart terminal session flow as Android.
- Added connection latency display to the device list and terminal header.

### Changed

- Updated iOS release signing, bundle metadata, app icons, launch images, and App Store update checks for TestFlight distribution.
- Refined scanner pairing and paste labels to avoid overflow in compact mobile layouts.

## [0.1.6] - 2026-05-16

### Changed

- Changed Android release and manual release builds to package only the `arm64-v8a` APK artifact.
- Improved terminal transport stability with a reliable host-response probe so relay-only connections no longer appear as a live Mac session.
- Improved WebRTC terminal transport backpressure, reconnect handling, and relay fallback behavior for low-traffic sessions.
- Improved image upload UX with chunked terminal uploads, progress feedback, and a persistent loading state until the image is inserted.

### Fixed

- Fixed Mac-offline detection so the mobile app moves from connecting to connection failed instead of looping through syncing and relay states.
- Fixed foreground resume recovery by refreshing host baselines and replaying cached terminal output after the app returns from the background.
- Fixed duplicate terminal input/output handling with input acknowledgements and output sequence acknowledgements.

## [0.1.5] - 2026-05-05

### Added

- Added shared Mac terminal split support, including mobile split switching, creation, deletion, history replay, and mobile-driven resize.
- Added WebRTC DataChannel P2P transport for remote terminal traffic with STUN direct connection first and WebSocket relay fallback.
- Added local sherpa_onnx voice input with an in-app waveform overlay and editable recognition preview.
- Added terminal input payload normalization so paste, voice, typed text, and control keys use stable insertion paths.

### Changed

- Replaced Android native speech recognition with the local voice model flow.
- Updated P2P STUN ordering to prefer a domestic STUN server for Chinese language environments while retaining global fallbacks.
- Improved connection-state grace handling so transient reconnects keep the paired-device list observable instead of flashing disconnected state.

### Fixed

- Fixed repeated terminal input from IME composition, paste, and voice-send paths.
- Fixed extra blank lines from remote Enter handling when using shared Mac terminal sessions.
- Fixed terminal history loading and background project/session synchronization after Mac restarts or mobile project switching.
- Fixed native terminal event subscriptions and duplicate resize events after Android platform-view recreation.

## [0.1.4] - 2026-04-28

### Changed

- Moved project-list syncing feedback into the right-side project action button spinner, keeping the horizontal project list free of transient status text.
- Kept the project list empty state focused on the final host response, showing "No projects" only after syncing completes with no projects.

## [0.1.3] - 2026-04-28

### Fixed

- Kept encrypted message sequence numbers monotonic across mobile reconnects so the Mac host no longer drops fresh `project.list` and `terminal.list` requests as replayed messages after app restart or foreground resume.
- Split relay connection and host baseline readiness in the UI, showing a syncing state instead of reporting fully connected while project and terminal baselines are still pending.
- Reconnected when the app returns to the foreground, avoiding stale WebSocket state after Android pauses or resumes the process.
- Ignored stale native terminal platform-view calls after Android recreates the terminal view, preventing `MissingPluginException` during terminal resize races.

## [0.1.2] - 2026-04-28

### Fixed

- Retried initial `project.list` and `terminal.list` baseline requests when the host response is not received, so the mobile project list and terminal session lookup recover from transient dropped messages.
- Restored the cached project list on app startup and refreshed it after the host returns the latest list.
- Limited the terminal history loading overlay to active `terminal.buffer` requests for the current session, avoiding a stuck loading state before projects or sessions are available.
- Added regression coverage for project-list retry, project-list cache storage, and opening the terminal before the project list returns.

## [0.1.1] - 2026-04-28

### Added

- Added a terminal history loading state so the terminal screen no longer appears as an empty cursor-only view while the remote buffer is being restored.

### Fixed

- Retried `terminal.buffer` requests when the remote history buffer is not acknowledged, improving recovery after relay reconnects or transient dropped messages.
- Added regression coverage for terminal buffer retry, acknowledgement, and readiness behavior.

## [0.1.0] - 2026-04-28

### Added

- Initial Codux Mobile Flutter client for connecting to Codux on macOS through the relay service.
- Added QR pairing, device management, project switching, terminal split switching, file browsing, image upload, and AI usage panels.
- Added native Android terminal rendering through a Termux TerminalView based Flutter platform view, including remote output, user input, scrollback, text selection, quick keys, and IME avoidance.
- Added GitHub update checking against the latest `duxweb/codux-flutter` release.

### Changed

- Replaced the earlier WebView / xterm rendering direction with the native Android terminal plugin.
- Added release logging control through `CODUX_LOG_LEVEL`, shared by Flutter and the native terminal plugin.

### Fixed

- Stabilized Android keyboard avoidance for terminal TUI apps by shifting the terminal surface without forcing remote terminal resize.
- Fixed remote terminal input duplication and emulator response forwarding by separating user input from local terminal responses.
