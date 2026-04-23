import AppKit
import Darwin
import Foundation
import Observation
import SwiftUI

enum GitRemoteAction {
    case fetch
    case pull
    case push
    case forcePush
    case sync
}

enum GitRemoteOperation: Equatable {
    case fetch
    case pull
    case push
    case forcePush
}

enum RightPanelKind: String, Codable, Equatable {
    case git
    case aiStats
}

@MainActor
@Observable
final class AppModel {
    struct ProjectCompletionPresentation: Equatable {
        var token: String
        var tool: String
        var finishedAt: Date
        var exitCode: Int?
    }

    struct TerminalRecoveryIssue: Equatable {
        var message: String
        var detail: String
    }

    var projects: [Project]
    var workspaces: [ProjectWorkspace]
    var selectedProjectID: UUID? {
        didSet {
            guard oldValue != selectedProjectID else { return }
            debugLog.log(
                "project-selection",
                "change source=\(selectedProjectIDChangeSource) old=\(oldValue?.uuidString ?? "nil") new=\(selectedProjectID?.uuidString ?? "nil")"
            )
            selectedProjectIDChangeSource = "unspecified"
        }
    }
    var appSettings: AppSettings
    var activeTerminalBackgroundPreset: AppTerminalBackgroundPreset
    var activeBackgroundColorPreset: AppBackgroundColorPreset
    var rightPanel: RightPanelKind?
    var commitMessage = ""
    var statusMessage = ""
    var isSidebarExpanded = false
    var activityByProjectID: [UUID: ProjectActivityPhase] = [:]
    var activityRenderVersion: UInt64 = 0
    var rightPanelWidth: CGFloat = 360
    var isGeneratingCommitMessage = false
    var terminalFocusRequestID: UUID?
    var terminalFocusRenderVersion: UInt64 = 0
    let aiSessionStore = AISessionStore.shared
    let aiStatsStore = AIStatsStore()
    let petStore = PetStore.shared
    let petRefreshCoordinator = PetRefreshCoordinator(petStore: PetStore.shared)
    let gitStore = GitStore()
    let performanceMonitor = AppPerformanceMonitorStore()

    private let persistenceService: PersistenceService
    let gitService = GitService()
    let gitCredentialStore = GitCredentialStore()
    let activityService = ProjectActivityService()
    let diagnosticsExportService = AppDiagnosticsExportService()
    let toolPermissionSettingsService = AIToolPermissionSettingsService()
    let appUpdaterService = AppUpdaterService(isEnabled: AppUpdaterService.isSupportedConfiguration)
    private let runtimeBridgeService = AIRuntimeBridgeService()
    let runtimeIngressService = AIRuntimeIngressService.shared
    private let runtimePollingService = AIRuntimePollingService.shared
    private let terminalProcessInspector = TerminalProcessInspector()
    let toolDriverFactory = AIToolDriverFactory.shared
    let debugLog = AppDebugLog.shared
    var selectedProjectIDChangeSource = "init"
    var activityStatusWatcher: DispatchSourceFileSystemObject?
    var appActivationObservers: [NSObjectProtocol] = []
    var terminalFocusObserver: NSObjectProtocol?
    var runtimeBridgeObserver: NSObjectProtocol?
    var runtimeActivityObserver: NSObjectProtocol?
    var pendingActivityRefreshTask: Task<Void, Never>?
    var pendingActivityRefreshShouldNotify = false
    var pendingActivityRefreshShouldRefreshAIStats = false
    var pendingActivityRefreshRequiresStatusReload = false
    var cachedActivityPayloadByProjectID: [UUID: ProjectActivityPayload] = [:]
    var completionPresentationByProjectID: [UUID: ProjectCompletionPresentation] = [:]
    var lastCompletionTokenByProjectID: [UUID: String] = [:]
    var lastWaitingInputTokenByProjectID: [UUID: String] = [:]
    var lastActivityResolutionLogTokenByProjectID: [UUID: String] = [:]
    var isSystemUIReady = false
    private var isTerminalStartupUnlocked = false
    private var hasLoggedRootViewAppearance = false
    private var lastWorkspaceAppearanceToken: String?
    private var pendingTerminalStartupUnlockTask: Task<Void, Never>?
    private var terminalRecoveryIssueBySessionID: [UUID: TerminalRecoveryIssue] = [:]
    private var terminalRecoveryRetryTokenBySessionID: [UUID: Int] = [:]
    private var pendingStartupRecoveryDialog: ConfirmDialogState?
    private var hasPresentedStartupRecoveryDialog = false
    var hasScheduledLaunchUpdateCheck = false
    var detachedTerminalPlacementBySessionID: [UUID: DetachedTerminalPlacement] = [:]

    init(snapshot: AppSnapshot?, persistenceService: PersistenceService, startupIssues: [PersistenceLoadIssue] = []) {
        self.persistenceService = persistenceService
        debugLog.reset()

        let resolvedSettings: AppSettings
        if let snapshot {
            self.projects = snapshot.projects
            self.workspaces = snapshot.workspaces
            self.selectedProjectID = snapshot.selectedProjectID ?? snapshot.projects.first?.id
            resolvedSettings = snapshot.appSettings ?? AppSettings()
            self.appSettings = resolvedSettings
        } else {
            self.projects = []
            self.workspaces = []
            self.selectedProjectID = nil
            resolvedSettings = AppSettings()
            self.appSettings = resolvedSettings
        }
        self.activeTerminalBackgroundPreset = resolvedSettings.terminalBackgroundPreset
        self.activeBackgroundColorPreset = resolvedSettings.backgroundColorPreset

        DmuxTerminalBackend.shared.configure(using: appSettings)

        refreshGitState()
        resetActivityState()
        activityService.clearAllStatuses()
        runtimeIngressService.resetEphemeralState()
        runtimeBridgeService.prepareManagedRuntimeSupportIfNeeded()
        runtimePollingService.start()
        reconcileManagedAIProcessState(reason: "launch")
        refreshProjectActivity(sendNotifications: false)
        activityService.requestNotificationPermission()
        observeApplicationActivation()
        observeTerminalFocusChanges()
        startActivityWatchers()
        debugLog.log("app", "launch selectedProject=\(selectedProject?.name ?? "nil")")
        aiStatsStore.configureIntervals(
            automatic: appSettings.aiAutoRefreshInterval,
            background: appSettings.aiBackgroundRefreshInterval
        )
        petRefreshCoordinator.configure(
            totalNormalizedTokensByProject: { [weak self] in
                guard let self else {
                    return [:]
                }
                return self.aiStatsStore.normalizedTokenTotalsForPet(
                    self.projects,
                    claimedAt: self.petStore.claimedAt
                )
            },
            computedStats: { [weak self] in
                guard let self else {
                    return .neutral
                }
                return self.aiStatsStore.petStatsSinceClaimedAt(
                    self.petStore.claimedAt,
                    projects: self.projects
                )
            }
        )
        aiSessionStore.onRenderVersionChange = { [weak self] in
            self?.petRefreshCoordinator.scheduleRefresh(reason: .aiSession)
        }
        petRefreshCoordinator.start()
        aiStatsStore.startTimers(
            isPanelVisible: { self.rightPanel == .aiStats },
            selectedProject: { self.selectedProject },
            selectedSessionID: { self.selectedSessionID },
            projects: { self.projects }
        )
        gitStore.configureRemoteSyncInterval(appSettings.gitAutoRefreshInterval)
        gitStore.startRemoteSyncPolling(
            selectedProject: { self.selectedProject },
            isEnabled: { NSApplication.shared.isActive && self.rightPanel == .git }
        )
        updateGitRemoteSyncPolling()
        performanceMonitor.setContextProvider { [weak self] in
            self?.performanceMonitorContextSnapshot() ?? .empty
        }
        performanceMonitor.setApplicationActive(NSApplication.shared.isActive)
        performanceMonitor.configure(
            isEnabled: appSettings.developer.showsPerformanceMonitor,
            sampleInterval: appSettings.developer.performanceMonitorSamplingInterval
        )
        toolPermissionSettingsService.sync(appSettings.toolPermissions)

        pendingStartupRecoveryDialog = startupRecoveryDialog(for: startupIssues)
        if pendingStartupRecoveryDialog != nil {
            statusMessage = String(localized: "startup.recovery.status", defaultValue: "Recovered invalid saved app data.", bundle: .module)
        }

        appUpdaterService.onCanCheckForUpdatesChanged = { [weak self] canCheck in
            guard let self else {
                return
            }
            if canCheck {
                self.isCheckingForUpdates = false
            }
        }

        Task { @MainActor in
            self.isSystemUIReady = true
            self.applyThemeMode()
            self.applyAppIcon()
            self.updateDockBadge()
        }
    }

    private func resetActivityState() {
        aiSessionStore.reset()
        activityByProjectID = [:]
        activityRenderVersion = 0
        cachedActivityPayloadByProjectID.removeAll()
        completionPresentationByProjectID.removeAll()
        lastCompletionTokenByProjectID.removeAll()
        lastWaitingInputTokenByProjectID.removeAll()
        lastActivityResolutionLogTokenByProjectID.removeAll()
    }

    var allowsDeferredTerminalStartup: Bool {
        isTerminalStartupUnlocked
    }

    static func bootstrap() -> AppModel {
        let persistenceService = PersistenceService()
        let loadResult = persistenceService.loadWithRecovery()
        return AppModel(
            snapshot: loadResult.snapshot,
            persistenceService: persistenceService,
            startupIssues: loadResult.issues
        )
    }

    var selectedProject: Project? {
        guard let selectedProjectID else {
            return nil
        }

        return projects.first(where: { $0.id == selectedProjectID })
    }

    func terminalSession(for sessionID: UUID) -> TerminalSession? {
        workspaces.lazy.compactMap { $0.session(for: sessionID) }.first
    }

    func isDetachedTerminal(_ sessionID: UUID) -> Bool {
        detachedTerminalPlacementBySessionID[sessionID] != nil
    }

    func presentStartupRecoveryIfNeeded() {
        guard hasPresentedStartupRecoveryDialog == false,
              let dialog = pendingStartupRecoveryDialog,
              let parentWindow = presentationWindow() else {
            return
        }

        hasPresentedStartupRecoveryDialog = true
        pendingStartupRecoveryDialog = nil
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }
    }

    func reconcileManagedAIProcessState(reason: String) {
        let activeInstanceIDs = DmuxTerminalBackend.shared.registry.activeSessionInstanceIDs()
        let orphanGroups = terminalProcessInspector.orphanedManagedAIProcessGroups(
            activeSessionInstanceIDs: activeInstanceIDs
        )
        let orphanInstanceIDs = Set(orphanGroups.map(\.sessionInstanceID))

        for group in orphanGroups {
            _ = kill(-group.pgid, SIGTERM)
            _ = kill(group.pgid, SIGTERM)
            debugLog.log(
                "ghostty-lifecycle",
                "reap-orphan reason=\(reason) tool=\(group.tool) pgid=\(group.pgid) instance=\(group.sessionInstanceID) projectPath=\(group.projectPath ?? "nil")"
            )
        }

        let observation = terminalProcessInspector.managedSessionObservation()
        let liveInstanceIDs = observation.liveInstanceIDs.subtracting(orphanInstanceIDs)
        let shouldSkipSessionPrune = observation.hasManagedProcessCandidates && liveInstanceIDs.isEmpty
        let removedTerminalIDs: [UUID]
        if shouldSkipSessionPrune {
            removedTerminalIDs = []
            debugLog.log(
                "ghostty-lifecycle",
                "skip-prune reason=\(reason) activeInstances=\(activeInstanceIDs.count) managedCandidates=1 observedLiveInstances=0"
            )
        } else {
            removedTerminalIDs = aiSessionStore.removeMissingManagedTerminalSessions(
                liveInstanceIDs: liveInstanceIDs
            )
        }

        if orphanGroups.isEmpty == false || removedTerminalIDs.isEmpty == false {
            debugLog.log(
                "ghostty-lifecycle",
                "reconcile-managed-ai reason=\(reason) orphans=\(orphanGroups.count) removedSessions=\(removedTerminalIDs.count) activeInstances=\(activeInstanceIDs.count) liveInstances=\(liveInstanceIDs.count)"
            )
        }
    }

    func noteRootViewAppeared() {
        guard hasLoggedRootViewAppearance == false else {
            return
        }
        hasLoggedRootViewAppearance = true
        debugLog.log("startup-ui", "root-view appeared selectedProject=\(selectedProject?.name ?? "nil")")
        unlockDeferredTerminalStartupAfterInitialPresentation()
        scheduleLaunchUpdateCheckIfNeeded()
    }

    func noteWorkspaceViewAppeared() {
        let token: String
        if let workspace = selectedWorkspace {
            token = "project:\(workspace.projectID.uuidString):top=\(workspace.topSessionIDs.count):bottom=\(workspace.bottomTabSessionIDs.count):selected=\(workspace.selectedSessionID.uuidString)"
        } else {
            token = "empty"
        }

        guard lastWorkspaceAppearanceToken != token else {
            return
        }
        lastWorkspaceAppearanceToken = token

        if let workspace = selectedWorkspace {
            debugLog.log(
                "startup-ui",
                "workspace-view project=\(workspace.projectID.uuidString) topSessions=\(workspace.topSessionIDs.count) bottomTabs=\(workspace.bottomTabSessionIDs.count) selectedSession=\(workspace.selectedSessionID.uuidString)"
            )
        } else {
            debugLog.log("startup-ui", "workspace-view empty")
        }
    }

    func terminalRecoveryIssue(for sessionID: UUID) -> TerminalRecoveryIssue? {
        terminalRecoveryIssueBySessionID[sessionID]
    }

    func terminalRecoveryRetryToken(for sessionID: UUID) -> Int {
        terminalRecoveryRetryTokenBySessionID[sessionID] ?? 0
    }

    func noteTerminalStartupFailure(_ sessionID: UUID, detail: String) {
        let message = String(
            localized: "terminal.recovery.failed",
            defaultValue: "Terminal recovery failed. Showing the project shell instead.",
            bundle: .module
        )
        let issue = TerminalRecoveryIssue(message: message, detail: detail)
        guard terminalRecoveryIssueBySessionID[sessionID] != issue else {
            return
        }

        terminalRecoveryIssueBySessionID[sessionID] = issue
        if let context = terminalRecoveryContext(for: sessionID) {
            debugLog.log(
                "terminal-recovery",
                "failed session=\(sessionID.uuidString) project=\(context.projectID.uuidString) title=\(context.title) detail=\(detail)"
            )
        } else {
            debugLog.log(
                "terminal-recovery",
                "failed session=\(sessionID.uuidString) detail=\(detail)"
            )
        }
        debugLog.log(
            "terminal-lifecycle",
            "release-request session=\(sessionID.uuidString) reason=startup-failure"
        )
        DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)

        if selectedSessionID == sessionID {
            statusMessage = message
        }
    }

    func noteTerminalStartupSucceeded(_ sessionID: UUID) {
        guard terminalRecoveryIssueBySessionID.removeValue(forKey: sessionID) != nil else {
            return
        }
        debugLog.log("terminal-recovery", "recovered session=\(sessionID.uuidString)")
    }

    func retryTerminalRecovery(_ sessionID: UUID) {
        terminalRecoveryIssueBySessionID[sessionID] = nil
        terminalRecoveryRetryTokenBySessionID[sessionID, default: 0] += 1
        debugLog.log(
            "terminal-lifecycle",
            "release-request session=\(sessionID.uuidString) reason=recovery-retry token=\(terminalRecoveryRetryTokenBySessionID[sessionID] ?? 0)"
        )
        DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
        terminalFocusRequestID = sessionID
        debugLog.log(
            "terminal-recovery",
            "retry session=\(sessionID.uuidString) token=\(terminalRecoveryRetryTokenBySessionID[sessionID] ?? 0)"
        )
        statusMessage = String(localized: "terminal.recovery.retrying", defaultValue: "Retrying terminal recovery.", bundle: .module)
    }

    var selectedWorkspace: ProjectWorkspace? {
        guard let selectedProjectID else {
            return nil
        }

        return workspaces.first(where: { $0.projectID == selectedProjectID })
    }

    var selectedSessionID: UUID? {
        selectedWorkspace?.selectedSessionID
    }

    var displayedFocusedTerminalSessionID: UUID? {
        Self.resolveDisplayedFocusedTerminalSessionID(
            focusRequestID: terminalFocusRequestID,
            registryFocusedSessionID: DmuxTerminalBackend.shared.registry.focusedSessionID(),
            selectedSessionID: selectedSessionID
        )
    }

    static func resolveDisplayedFocusedTerminalSessionID(
        focusRequestID: UUID?,
        registryFocusedSessionID: UUID?,
        selectedSessionID: UUID?
    ) -> UUID? {
        focusRequestID ?? registryFocusedSessionID ?? selectedSessionID
    }

    static func shouldRefreshSelectionFocus(
        requestedSessionID: UUID,
        selectedSessionID: UUID?,
        pendingFocusRequestID: UUID?,
        registryFocusedSessionID: UUID?
    ) -> Bool {
        if selectedSessionID != requestedSessionID {
            return true
        }
        if let pendingFocusRequestID {
            return pendingFocusRequestID != requestedSessionID
        }
        return registryFocusedSessionID != requestedSessionID
    }

    static func shouldRefreshBottomTabSelection(
        requestedSessionID: UUID,
        selectedSessionID: UUID?,
        selectedBottomTabSessionID: UUID?,
        pendingFocusRequestID: UUID?,
        registryFocusedSessionID: UUID?
    ) -> Bool {
        if selectedBottomTabSessionID != requestedSessionID {
            return true
        }
        return shouldRefreshSelectionFocus(
            requestedSessionID: requestedSessionID,
            selectedSessionID: selectedSessionID,
            pendingFocusRequestID: pendingFocusRequestID,
            registryFocusedSessionID: registryFocusedSessionID
        )
    }

    private func unlockDeferredTerminalStartupAfterInitialPresentation() {
        guard isTerminalStartupUnlocked == false else {
            return
        }
        pendingTerminalStartupUnlockTask?.cancel()
        pendingTerminalStartupUnlockTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(260))
            guard let self, !Task.isCancelled else {
                return
            }
            self.isTerminalStartupUnlocked = true
            self.pendingTerminalStartupUnlockTask = nil
            self.debugLog.log("startup-ui", "terminal-startup unlocked")
            self.refreshAIStatsIfNeeded()
        }
    }

    func clearTerminalRecoveryState(for sessionID: UUID) {
        terminalRecoveryIssueBySessionID[sessionID] = nil
        terminalRecoveryRetryTokenBySessionID[sessionID] = nil
    }

    private func terminalRecoveryContext(for sessionID: UUID) -> (projectID: UUID, title: String)? {
        for workspace in workspaces {
            if let session = workspace.sessions.first(where: { $0.id == sessionID }) {
                return (workspace.projectID, session.title)
            }
        }
        return nil
    }

    var aiStatsState: AIStatsPanelState {
        aiStatsStore.state
    }

    var displayLanguage: AppLanguage {
        AppLanguageBootstrap.languageAtLaunch.resolved
    }

    var aiPanelRefreshState: PanelRefreshState {
        aiStatsStore.refreshState
    }

    var terminalBackgroundPreset: AppTerminalBackgroundPreset {
        activeTerminalBackgroundPreset
    }

    var backgroundColorPreset: AppBackgroundColorPreset {
        activeBackgroundColorPreset
    }

    var automaticTerminalAppearance: AppEffectiveTerminalAppearance {
        GhosttyEmbeddedConfig.resolvedAutomaticTerminalAppearance(
            prefersDarkAppearance: systemPrefersDarkAppearance
        )
    }

    var terminalAppearance: AppEffectiveTerminalAppearance {
        terminalBackgroundPreset.effectiveAppearance(
            backgroundColorPreset: backgroundColorPreset,
            automaticAppearance: automaticTerminalAppearance
        )
    }

    var effectiveThemeMode: AppThemeMode {
        terminalAppearance.isLight ? .light : .dark
    }

    var terminalChromeColor: Color {
        Color(nsColor: terminalAppearance.backgroundColor)
    }

    var windowGlassTintColor: Color {
        Color(
            nsColor: NSColor(name: nil) { appearance in
                let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                return self.terminalAppearance.windowGlassTintColor(forDarkAppearance: isDark)
            }
        )
    }

    var terminalDividerColor: Color {
        Color(nsColor: terminalAppearance.dividerColor)
    }

    var terminalDividerNSColor: NSColor {
        terminalAppearance.dividerColor
    }

    var terminalTextColor: Color {
        Color(nsColor: terminalAppearance.foregroundColor)
    }

    var terminalMutedTextColor: Color {
        Color(nsColor: terminalAppearance.mutedForegroundColor)
    }

    var terminalUsesLightBackground: Bool {
        terminalAppearance.isLight
    }

    var terminalInactiveDimColor: NSColor {
        terminalAppearance.inactiveDimColor
    }

    private var systemPrefersDarkAppearance: Bool {
        let appearance = NSApplication.shared.effectiveAppearance
        return appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }

    var gitPanelState: GitPanelState {
        gitStore.panelState
    }

    var gitState: GitRepositoryState? {
        get { gitStore.panelState.gitState }
        set { gitStore.panelState.gitState = newValue }
    }

    var selectedGitEntry: GitFileEntry? {
        get { gitStore.panelState.selectedGitEntry }
        set { gitStore.panelState.selectedGitEntry = newValue }
    }

    var selectedGitEntryIDs: Set<String> {
        get { gitStore.panelState.selectedGitEntryIDs }
        set { gitStore.panelState.selectedGitEntryIDs = newValue }
    }

    var gitHistory: [GitCommitEntry] {
        get { gitStore.panelState.gitHistory }
        set { gitStore.panelState.gitHistory = newValue }
    }

    var selectedGitCommitHash: String? {
        get { gitStore.panelState.selectedGitCommitHash }
        set { gitStore.panelState.selectedGitCommitHash = newValue }
    }

    var gitDiffText: String {
        get { gitStore.panelState.gitDiffText }
        set { gitStore.panelState.gitDiffText = newValue }
    }

    var isGitLoading: Bool {
        get { gitStore.panelState.isGitLoading }
        set { gitStore.panelState.isGitLoading = newValue }
    }

    var isCheckingForUpdates = false

    var isGitDiffLoading: Bool {
        get { gitStore.panelState.isGitDiffLoading }
        set { gitStore.panelState.isGitDiffLoading = newValue }
    }

    var gitSelectionAnchorID: String? {
        get { gitStore.panelState.gitSelectionAnchorID }
        set { gitStore.panelState.gitSelectionAnchorID = newValue }
    }

    var gitBranches: [String] {
        get { gitStore.panelState.gitBranches }
        set { gitStore.panelState.gitBranches = newValue }
    }

    var gitBranchUpstreams: [String: String] {
        get { gitStore.panelState.gitBranchUpstreams }
        set { gitStore.panelState.gitBranchUpstreams = newValue }
    }

    var gitRemoteBranches: [String] {
        get { gitStore.panelState.gitRemoteBranches }
        set { gitStore.panelState.gitRemoteBranches = newValue }
    }

    var gitRemotes: [GitRemoteEntry] {
        get { gitStore.panelState.gitRemotes }
        set { gitStore.panelState.gitRemotes = newValue }
    }

    var gitRemoteSyncState: GitRemoteSyncState {
        get { gitStore.panelState.gitRemoteSyncState }
        set { gitStore.panelState.gitRemoteSyncState = newValue }
    }

    var activeGitRemoteOperation: GitRemoteOperation? {
        get { gitStore.panelState.activeGitRemoteOperation }
        set { gitStore.panelState.activeGitRemoteOperation = newValue }
    }

    func persist() {
        let snapshot = AppSnapshot(
            projects: projects,
            workspaces: workspaces,
            selectedProjectID: selectedProjectID,
            appSettings: appSettings
        )
        persistenceService.save(snapshot)
    }

    func presentationWindow() -> NSWindow? {
        if let keyWindow = NSApp.keyWindow, !(keyWindow is NSPanel) {
            return keyWindow
        }
        if let mainWindow = NSApp.mainWindow, !(mainWindow is NSPanel) {
            return mainWindow
        }
        return NSApp.windows.first(where: { !($0 is NSPanel) && $0.isVisible })
    }

    func presentDiagnosticsExportError(_ error: Error) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = error.localizedDescription
            return
        }

        statusMessage = error.localizedDescription
        let dialog = ConfirmDialogState(
            title: String(localized: "diagnostics.export.error.title", defaultValue: "Unable to Export Diagnostics", bundle: .module),
            message: error.localizedDescription,
            icon: "tray.and.arrow.down.fill",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
        )
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }
    }

    private func startupRecoveryDialog(for issues: [PersistenceLoadIssue]) -> ConfirmDialogState? {
        guard !issues.isEmpty else {
            return nil
        }

        var lines: [String] = []
        for issue in issues {
            switch issue {
            case let .invalidStateFile(backupFileName):
                if let backupFileName, !backupFileName.isEmpty {
                    lines.append(
                        String(
                            format: String(
                                localized: "startup.recovery.invalid_state.backup_format",
                                defaultValue: "The saved app configuration was invalid and has been reset to defaults. A backup was saved as %@.",
                                bundle: .module
                            ),
                            backupFileName
                        )
                    )
                } else {
                    lines.append(
                        String(
                            localized: "startup.recovery.invalid_state",
                            defaultValue: "The saved app configuration was invalid and has been reset to defaults.",
                            bundle: .module
                        )
                    )
                }
            case .sanitizedState:
                lines.append(
                    String(
                        localized: "startup.recovery.sanitized",
                        defaultValue: "Some saved projects or terminal layout data were invalid and were repaired automatically.",
                        bundle: .module
                    )
                )
            }
        }

        return ConfirmDialogState(
            title: String(localized: "startup.recovery.title", defaultValue: "Recovered App Data", bundle: .module),
            message: lines.joined(separator: "\n\n"),
            icon: "exclamationmark.triangle.fill",
            iconColor: .orange,
            primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
        )
    }

}

enum GitCommitAction: CaseIterable, Hashable {
    case commit
    case commitAndPush
    case commitAndSync

    var title: String {
        switch self {
        case .commit:
            return String(localized: "git.commit.action", defaultValue: "Commit", bundle: .module)
        case .commitAndPush:
            return String(localized: "git.commit.action_push", defaultValue: "Commit and Push", bundle: .module)
        case .commitAndSync:
            return String(localized: "git.commit.action_sync", defaultValue: "Commit and Sync", bundle: .module)
        }
    }

    var successMessage: String {
        switch self {
        case .commit:
            return String(localized: "git.commit.success", defaultValue: "Committed staged changes.", bundle: .module)
        case .commitAndPush:
            return String(localized: "git.commit.push_success", defaultValue: "Committed and pushed changes.", bundle: .module)
        case .commitAndSync:
            return String(localized: "git.commit.sync_success", defaultValue: "Committed and synced changes.", bundle: .module)
        }
    }
}
