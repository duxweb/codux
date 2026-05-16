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
    case files
    case taskMemos
    case ssh
}

@MainActor
@Observable
final class AppModel {
    struct ProjectCompletionPresentation: Equatable {
        var tool: String
        var finishedAt: Date
        var exitCode: Int?
        var presentedAt: Date
    }

    struct ProjectActivityCache: Equatable {
        var completionPresentation: ProjectCompletionPresentation? = nil
        var lastCompletionToken: String? = nil
        var lastWaitingInputToken: String? = nil
        var lastResolutionLogToken: String? = nil
    }

    struct TerminalRecoveryIssue: Equatable {
        var message: String
        var detail: String
    }

    var projects: [Project]
    var worktrees: [ProjectWorktree]
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
    var selectedWorktreeID: UUID?
    var activeTerminalBackgroundPreset: AppTerminalBackgroundPreset
    var activeBackgroundColorPreset: AppBackgroundColorPreset
    var rightPanel: RightPanelKind?
    var taskMemos: [TaskMemoItem] = []
    var worktreeTasks: [WorktreeTask] = []
    var taskMemoFocusedSessionID: UUID?
    var sshProfiles: [SSHConnectionProfile] = []
    var commitMessage = ""
    var statusMessage = ""
    var isSidebarExpanded = false
    var isWorktreeSidebarExpanded = true
    var activityByProjectID: [UUID: ProjectActivityPhase] = [:]
    var activityRenderVersion: UInt64 = 0
    var rightPanelWidth: CGFloat = 360
    var isGeneratingCommitMessage = false
    var terminalFocusRequestID: UUID?
    var terminalFocusRenderVersion: UInt64 = 0
    var terminalLoadingSessionIDs: Set<UUID> = []
    var memoryExtractionStatus = MemoryExtractionStatusSnapshot(
        status: .idle,
        pendingCount: 0,
        runningCount: 0,
        lastError: nil,
        updatedAt: Date()
    )
    var aiProviderTestStates: [String: AIProviderTestState] = [:]
    var localLlamaModels: [LocalLlamaModelDescriptor] = LocalLlamaModelCatalog.models
    var localLlamaModelInstallStates: [String: LocalLlamaModelInstallState] = [:]
    let aiSessionStore = AISessionStore.shared
    let aiStatsStore = AIStatsStore()
    let petStore = PetStore.shared
    let petRefreshCoordinator = PetRefreshCoordinator(petStore: PetStore.shared)
    let petSpeechCoordinator = PetSpeechCoordinator()
    let gitStore = GitStore()
    let performanceMonitor = AppPerformanceMonitorStore()

    private let persistenceService: PersistenceService
    let gitService = GitService()
    let gitCredentialStore = GitCredentialStore()
    let activityService = ProjectActivityService()
    let diagnosticsExportService = AppDiagnosticsExportService()
    let toolPermissionSettingsService = AIToolPermissionSettingsService()
    let appUpdaterService = AppUpdaterService(isEnabled: AppUpdaterService.isSupportedConfiguration)
    @ObservationIgnored lazy var remoteHostService = RemoteHostService(model: self)
    private let runtimeBridgeService = AIRuntimeBridgeService()
    let memoryCoordinator = MemoryCoordinator()
    let localLlamaModelStore = LocalLlamaModelStore()
    let sleepPreventionService = SleepPreventionService.shared
    let runtimeIngressService = AIRuntimeIngressService.shared
    private let runtimePollingService = AIRuntimePollingService.shared
    private let terminalProcessInspector = TerminalProcessInspector()
    let toolDriverFactory = AIToolDriverFactory.shared
    let agentDriverFactory = AgentDriverFactory.shared
    let debugLog = AppDebugLog.shared
    var selectedProjectIDChangeSource = "init"
    var appActivationObservers: [NSObjectProtocol] = []
    var terminalFocusObserver: NSObjectProtocol?
    var runtimeBridgeObserver: NSObjectProtocol?
    var memoryExtractionStatusObserver: NSObjectProtocol?
    var pendingActivityRefreshTask: Task<Void, Never>?
    var pendingActivityRefreshShouldNotify = false
    var pendingMemorySessionSnapshotTask: Task<Void, Never>?
    private var pendingDragReorderPersistTask: Task<Void, Never>?
    var activityCacheByProjectID: [UUID: ProjectActivityCache] = [:]
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
    var pendingWorktreeGitSummaryRefreshTask: Task<Void, Never>?
    var detachedTerminalPlacementBySessionID: [UUID: DetachedTerminalPlacement] = [:]
    var workspaceFileTabsByWorktreeID: [UUID: [WorkspaceFileTab]] = [:]
    var selectedWorkspaceContentByWorktreeID: [UUID: WorkspaceContentSelection] = [:]
    var workspacePrimaryViewModeByWorktreeID: [UUID: WorkspacePrimaryViewMode] = [:]
    var dirtyWorkspaceFileTabIDs: Set<String> = []
    var workspaceFileEditorSaveRequestTokensByTabID: [String: Int] = [:]
    var workspaceFileEditorSaveAndCloseRequestTokensByTabID: [String: Int] = [:]
    var worktreeGitSummaries: [UUID: ProjectWorktreeGitSummary] = [:]
    var selectedWorktreeReviewID: UUID?
    var selectedWorktreeReviewFileID: String?
    var worktreeReviewSnapshot: WorktreeReviewSnapshot?
    var isLoadingWorktreeReview = false
    var agentSessionStates: [UUID: AgentSessionState] = [:]
    var agentRunTasks: [UUID: Task<Void, Never>] = [:]
    var agentInputDrafts: [UUID: String] = [:]
    @ObservationIgnored var pendingAgentDriverEvents: [UUID: [AgentDriverEvent]] = [:]
    @ObservationIgnored var pendingAgentDriverEventFlushTasks: [UUID: Task<Void, Never>] = [:]

    init(snapshot: AppSnapshot?, persistenceService: PersistenceService, startupIssues: [PersistenceLoadIssue] = []) {
        self.persistenceService = persistenceService
        debugLog.reset()

        let resolvedSettings: AppSettings
        if let snapshot {
            let resolvedProjects = snapshot.projects
            let resolvedWorktrees = Self.resolvedWorktrees(snapshot.worktrees, projects: resolvedProjects)
            let resolvedWorkspaces = Self.resolvedWorkspaces(snapshot.workspaces, projects: resolvedProjects, worktrees: resolvedWorktrees)
            let resolvedSelectedProjectID = snapshot.selectedProjectID ?? resolvedProjects.first?.id
            self.projects = resolvedProjects
            self.worktrees = resolvedWorktrees
            self.workspaces = resolvedWorkspaces
            self.selectedProjectID = resolvedSelectedProjectID
            self.selectedWorktreeID = Self.resolvedSelectedWorktreeID(
                snapshot.selectedWorktreeID,
                selectedProjectID: resolvedSelectedProjectID,
                worktrees: resolvedWorktrees
            )
            let resolvedWorkspaceContent = Self.resolvedWorkspaceContentStates(
                snapshot.workspaceContentStates,
                worktrees: resolvedWorktrees
            )
            self.workspaceFileTabsByWorktreeID = resolvedWorkspaceContent.fileTabs
            self.selectedWorkspaceContentByWorktreeID = resolvedWorkspaceContent.selections
            self.workspacePrimaryViewModeByWorktreeID = resolvedWorkspaceContent.primaryModes
            resolvedSettings = snapshot.appSettings ?? AppSettings()
            self.appSettings = resolvedSettings
            self.taskMemos = snapshot.taskMemos ?? []
            self.worktreeTasks = Self.resolvedWorktreeTasks(
                snapshot.worktreeTasks,
                worktrees: resolvedWorktrees
            )
            self.sshProfiles = snapshot.sshProfiles ?? []
        } else {
            self.projects = []
            self.worktrees = []
            self.workspaces = []
            self.selectedProjectID = nil
            self.selectedWorktreeID = nil
            resolvedSettings = AppSettings()
            self.appSettings = resolvedSettings
            self.taskMemos = []
            self.worktreeTasks = []
            self.sshProfiles = []
        }
        self.activeTerminalBackgroundPreset = resolvedSettings.terminalBackgroundPreset
        self.activeBackgroundColorPreset = resolvedSettings.backgroundColorPreset

        DmuxTerminalBackend.shared.configure(using: appSettings)
        remoteHostService.applySettings()
        sleepPreventionService.configure(mode: appSettings.sleepPreventionMode)
        refreshLocalLlamaModelCatalog()

        refreshGitState()
        resetActivityState()
        runtimeIngressService.resetEphemeralState()
        runtimeBridgeService.prepareManagedRuntimeSupportIfNeeded()
        runtimePollingService.start()
        reconcileManagedAIProcessState(reason: "launch")
        refreshProjectActivity(sendNotifications: false)
        activityService.requestNotificationPermission()
        observeApplicationActivation()
        observeTerminalFocusChanges()
        observeMemoryExtractionStatusChanges()
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
                let currentProjects = self.projects
                var totals = self.aiStatsStore.normalizedTokenTotalsForPet(
                    currentProjects,
                    claimedAt: self.petStore.claimedAt
                )
                for project in currentProjects where totals[project.id] == nil {
                    totals[project.id] = 0
                }
                return totals
            },
            computedStats: { [weak self] now in
                guard let self else {
                    return .neutral
                }
                return self.aiStatsStore.petStatsRolling(self.projects, now: now)
            },
            dailyTotalTokens: { [weak self] in
                guard let self else {
                    return 0
                }
                return self.aiStatsStore.totalTodayNormalizedTokensAcrossProjects(self.projects)
            }
        )
        petSpeechCoordinator.configure(
            settings: { [weak self] in
                self?.appSettings.ai.pet ?? AppAIPetSettings()
            },
            aiSettings: { [weak self] in
                self?.appSettings.ai ?? AppAISettings()
            },
            petSettings: { [weak self] in
                self?.appSettings.pet ?? AppPetSettings()
            },
            petName: { [weak self] in
                self?.resolvedPetSpeechName() ?? petSpeechL("pet.speech.payload.pet_name", "Little One")
            },
            activitySnapshots: { [weak self] in
                self?.petSpeechActivitySnapshots() ?? []
            },
            llmLineProvider: { event, mode, aiSettings in
                guard aiSettings.pet.speechLLMEnabled else {
                    return nil
                }
                return await PetSpeechLLMService()
                    .generateLine(event: event, mode: mode, settings: aiSettings)
            }
        )
        aiSessionStore.onSpeechEvent = { [weak self] event in
            self?.handleAISessionSpeechEvent(event)
        }
        petStore.onSpeechEvent = { [weak self] event in
            self?.petSpeechCoordinator.notify(event)
        }
        petRefreshCoordinator.onSpeechEvent = { [weak self] event in
            self?.petSpeechCoordinator.notify(event)
        }
        aiSessionStore.onRenderVersionChange = { [weak self] in
            guard let self else {
                return
            }
            self.petRefreshCoordinator.scheduleRefresh(reason: .aiSession)
            self.scheduleMemorySessionSnapshotHandling()
        }
        petSpeechCoordinator.start()
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
        Task {
            await self.memoryCoordinator.recoverInterruptedExtractions()
            await self.memoryCoordinator.handleSessionSnapshots([], settings: self.appSettings.ai, projects: self.projects)
        }

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
        activityCacheByProjectID.removeAll()
    }

    private func resolvedPetSpeechName() -> String {
        let trimmedName = petStore.customName.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmedName.isEmpty {
            return trimmedName
        }
        return petStore.currentIdentity.displayName
    }

    private func petSpeechActivitySnapshots() -> [PetSpeechActivitySnapshot] {
        aiSessionStore.terminalSessionsByID.values.map { session in
            PetSpeechActivitySnapshot(
                tool: session.tool,
                model: session.model,
                projectName: session.projectName,
                state: session.state.rawValue,
                updatedAt: Date(timeIntervalSince1970: session.updatedAt),
                activeStartedAt: session.activeTurnStartedAt.map { Date(timeIntervalSince1970: $0) },
                totalTokens: session.committedTotalTokens
            )
        }
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

    private static func resolvedWorktrees(_ persisted: [ProjectWorktree]?, projects: [Project]) -> [ProjectWorktree] {
        var seenIDs = Set<UUID>()
        var seenPaths = Set<String>()
        let projectIDs = Set(projects.map(\.id))
        var resolved: [ProjectWorktree] = []

        for worktree in persisted ?? [] {
            guard projectIDs.contains(worktree.projectID),
                  seenIDs.insert(worktree.id).inserted else {
                continue
            }
            var sanitized = worktree
            sanitized.name = sanitized.name.trimmingCharacters(in: .whitespacesAndNewlines)
            if sanitized.name.isEmpty {
                sanitized.name = sanitized.isDefault
                    ? String(localized: "worktree.default.name", defaultValue: "Default", bundle: .module)
                    : URL(fileURLWithPath: sanitized.path, isDirectory: true).lastPathComponent
            }
            sanitized.path = URL(fileURLWithPath: sanitized.path).standardizedFileURL.path
            guard !sanitized.path.isEmpty,
                  seenPaths.insert(sanitized.path).inserted else {
                continue
            }
            resolved.append(sanitized)
        }

        for project in projects where resolved.contains(where: { $0.projectID == project.id && $0.isDefault }) == false {
            let defaultWorktree = ProjectWorktree.defaultWorktree(for: project)
            if seenIDs.insert(defaultWorktree.id).inserted,
               seenPaths.insert(URL(fileURLWithPath: defaultWorktree.path).standardizedFileURL.path).inserted {
                resolved.append(defaultWorktree)
            }
        }

        let projectOrder = Dictionary(uniqueKeysWithValues: projects.enumerated().map { ($0.element.id, $0.offset) })
        return resolved.sorted { lhs, rhs in
            let lhsProjectOrder = projectOrder[lhs.projectID] ?? Int.max
            let rhsProjectOrder = projectOrder[rhs.projectID] ?? Int.max
            if lhsProjectOrder != rhsProjectOrder {
                return lhsProjectOrder < rhsProjectOrder
            }
            if lhs.isDefault != rhs.isDefault {
                return lhs.isDefault
            }
            return lhs.updatedAt > rhs.updatedAt
        }
    }

    private static func resolvedWorkspaces(
        _ persisted: [ProjectWorkspace],
        projects: [Project],
        worktrees: [ProjectWorktree]
    ) -> [ProjectWorkspace] {
        var workspaceByID = Dictionary(uniqueKeysWithValues: persisted.map { ($0.projectID, $0) })
        return worktrees.compactMap { worktree in
            guard let rootProject = projects.first(where: { $0.id == worktree.projectID }) else {
                return nil
            }
            let effectiveProject = Project(
                id: worktree.id,
                name: worktree.isDefault ? rootProject.name : "\(rootProject.name) · \(worktree.name)",
                path: worktree.path,
                shell: rootProject.shell,
                defaultCommand: rootProject.defaultCommand,
                badgeText: rootProject.badgeText,
                badgeSymbol: rootProject.badgeSymbol,
                badgeColorHex: rootProject.badgeColorHex,
                gitDefaultPushRemoteName: rootProject.gitDefaultPushRemoteName
            )
            if var workspace = workspaceByID.removeValue(forKey: worktree.id) {
                workspace.projectID = worktree.id
                workspace.topPaneRatios = []
                workspace.bottomPaneHeight = ProjectWorkspace.defaultBottomPaneHeight
                workspace.sessions = workspace.sessions.map { session in
                    var updated = session
                    updated.projectID = effectiveProject.id
                    updated.projectName = effectiveProject.name
                    updated.cwd = effectiveProject.path
                    updated.shell = effectiveProject.shell
                    return updated
                }
                workspace.topPaneRatios = workspace.resolvedTopPaneRatios()
                return workspace
            }
            return ProjectWorkspace.sample(projectID: worktree.id, path: worktree.path)
        }
    }

    private static func resolvedWorktreeTasks(
        _ persisted: [WorktreeTask]?,
        worktrees: [ProjectWorktree]
    ) -> [WorktreeTask] {
        let validWorktreeIDs = Set(worktrees.map(\.id))
        var seenWorktreeIDs = Set<UUID>()
        return (persisted ?? []).compactMap { task in
            guard validWorktreeIDs.contains(task.worktreeID),
                  seenWorktreeIDs.insert(task.worktreeID).inserted else {
                return nil
            }
            var sanitized = task
            sanitized.title = sanitized.title.trimmingCharacters(in: .whitespacesAndNewlines)
            sanitized.baseBranch = sanitized.baseBranch.trimmingCharacters(in: .whitespacesAndNewlines)
            if sanitized.title.isEmpty {
                sanitized.title = worktrees.first(where: { $0.id == task.worktreeID })?.name ?? String(localized: "worktree.task.untitled", defaultValue: "Untitled Task", bundle: .module)
            }
            return sanitized
        }
    }

    private static func resolvedSelectedWorktreeID(
        _ persisted: UUID?,
        selectedProjectID: UUID?,
        worktrees: [ProjectWorktree]
    ) -> UUID? {
        if let persisted,
           worktrees.contains(where: { $0.id == persisted }) {
            return persisted
        }
        guard let selectedProjectID else {
            return nil
        }
        return worktrees.first(where: { $0.projectID == selectedProjectID && $0.isDefault })?.id
            ?? worktrees.first(where: { $0.projectID == selectedProjectID })?.id
    }

    var selectedProject: Project? {
        selectedWorktreeProject ?? selectedRootProject
    }

    var selectedRootProject: Project? {
        guard let selectedProjectID else {
            return nil
        }

        return projects.first(where: { $0.id == selectedProjectID })
    }

    var selectedWorktree: ProjectWorktree? {
        guard let selectedWorktreeID else {
            return nil
        }
        return worktrees.first(where: { $0.id == selectedWorktreeID })
    }

    var selectedProjectWorktrees: [ProjectWorktree] {
        guard let selectedProjectID else {
            return []
        }
        return worktrees
            .filter { $0.projectID == selectedProjectID }
            .sorted { lhs, rhs in
                if lhs.isDefault != rhs.isDefault {
                    return lhs.isDefault
                }
                return lhs.updatedAt > rhs.updatedAt
            }
    }

    var selectedWorktreeProject: Project? {
        guard let worktree = selectedWorktree,
              let rootProject = projects.first(where: { $0.id == worktree.projectID }) else {
            return nil
        }
        return Project(
            id: worktree.id,
            name: worktree.isDefault ? rootProject.name : "\(rootProject.name) · \(worktree.name)",
            path: worktree.path,
            shell: rootProject.shell,
            defaultCommand: rootProject.defaultCommand,
            badgeText: rootProject.badgeText,
            badgeSymbol: rootProject.badgeSymbol,
            badgeColorHex: rootProject.badgeColorHex,
            gitDefaultPushRemoteName: rootProject.gitDefaultPushRemoteName
        )
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
        noteTerminalLoadingState(sessionID, isLoading: false)
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

    func noteTerminalLoadingState(_ sessionID: UUID, isLoading: Bool) {
        let didChange: Bool
        if isLoading {
            didChange = terminalLoadingSessionIDs.insert(sessionID).inserted
        } else {
            didChange = terminalLoadingSessionIDs.remove(sessionID) != nil
        }
        guard didChange else {
            return
        }

        if let context = terminalRecoveryContext(for: sessionID) {
            activityByProjectID[context.projectID] = resolvedProjectActivityPhase(projectID: context.projectID)
            debugLog.log(
                "terminal-loading",
                "session=\(sessionID.uuidString) project=\(context.projectID.uuidString) loading=\(isLoading)"
            )
        } else {
            debugLog.log(
                "terminal-loading",
                "session=\(sessionID.uuidString) loading=\(isLoading)"
            )
        }
        markActivityStateChanged()
    }

    func retryTerminalRecovery(_ sessionID: UUID) {
        terminalRecoveryIssueBySessionID[sessionID] = nil
        terminalRecoveryRetryTokenBySessionID[sessionID, default: 0] += 1
        debugLog.log(
            "terminal-lifecycle",
            "release-request session=\(sessionID.uuidString) reason=recovery-retry token=\(terminalRecoveryRetryTokenBySessionID[sessionID] ?? 0)"
        )
        noteTerminalLoadingState(sessionID, isLoading: false)
        DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
        terminalFocusRequestID = sessionID
        debugLog.log(
            "terminal-recovery",
            "retry session=\(sessionID.uuidString) token=\(terminalRecoveryRetryTokenBySessionID[sessionID] ?? 0)"
        )
        statusMessage = String(localized: "terminal.recovery.retrying", defaultValue: "Retrying terminal recovery.", bundle: .module)
    }

    var selectedWorkspace: ProjectWorkspace? {
        guard let selectedWorktreeID else {
            return nil
        }

        return workspaces.first(where: { $0.projectID == selectedWorktreeID })
    }

    var selectedSessionID: UUID? {
        selectedWorkspace?.selectedSessionID
    }

    var displayedFocusedTerminalSessionID: UUID? {
        let workspaceSessionIDs = Set(selectedWorkspace?.sessions.map(\.id) ?? [])
        let registryFocusedSessionID = DmuxTerminalBackend.shared.registry.focusedSessionID()
        return Self.resolveDisplayedFocusedTerminalSessionID(
            focusRequestID: terminalFocusRequestID,
            registryFocusedSessionID: registryFocusedSessionID,
            selectedSessionID: selectedSessionID,
            visibleSessionIDs: workspaceSessionIDs
        )
    }

    static func resolveDisplayedFocusedTerminalSessionID(
        focusRequestID: UUID?,
        registryFocusedSessionID: UUID?,
        selectedSessionID: UUID?,
        visibleSessionIDs: Set<UUID>? = nil
    ) -> UUID? {
        if let focusRequestID {
            return focusRequestID
        }
        if let registryFocusedSessionID,
           visibleSessionIDs?.contains(registryFocusedSessionID) ?? true {
            return registryFocusedSessionID
        }
        return selectedSessionID
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
        terminalLoadingSessionIDs.remove(sessionID)
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

    private func persistenceSnapshot() -> AppSnapshot {
        AppSnapshot(
            projects: projects,
            worktrees: worktrees,
            worktreeTasks: worktreeTasks,
            workspaces: workspaces,
            selectedProjectID: selectedProjectID,
            selectedWorktreeID: selectedWorktreeID,
            workspaceContentStates: workspaceContentStatesSnapshot(),
            appSettings: appSettings,
            taskMemos: taskMemos,
            sshProfiles: sshProfiles
        )
    }

    func persist() {
        pendingDragReorderPersistTask?.cancel()
        pendingDragReorderPersistTask = nil
        persistenceService.save(persistenceSnapshot())
    }

    func prepareForApplicationTermination() {
        pendingDragReorderPersistTask?.cancel()
        pendingDragReorderPersistTask = nil
        pendingTerminalStartupUnlockTask?.cancel()
        pendingTerminalStartupUnlockTask = nil
        pendingActivityRefreshTask?.cancel()
        pendingActivityRefreshTask = nil
        pendingMemorySessionSnapshotTask?.cancel()
        pendingMemorySessionSnapshotTask = nil
        pendingWorktreeGitSummaryRefreshTask?.cancel()
        pendingWorktreeGitSummaryRefreshTask = nil
        pendingAgentDriverEventFlushTasks.values.forEach { $0.cancel() }
        pendingAgentDriverEventFlushTasks.removeAll()
        pendingAgentDriverEvents.removeAll()
        petSpeechCoordinator.stop()
    }

    func scheduleDragReorderPersist(refreshAIStats: Bool = false) {
        pendingDragReorderPersistTask?.cancel()
        pendingDragReorderPersistTask = Task { @MainActor [weak self] in
            guard let self, !Task.isCancelled else {
                return
            }

            let snapshot = self.persistenceSnapshot()
            self.pendingDragReorderPersistTask = nil
            self.persistenceService.saveInBackground(snapshot)
            if refreshAIStats {
                Task { @MainActor [weak self] in
                    try? await Task.sleep(for: .milliseconds(220))
                    guard let self, !Task.isCancelled else {
                        return
                    }
                    self.refreshAIStatsIfNeeded()
                }
            }
        }
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
