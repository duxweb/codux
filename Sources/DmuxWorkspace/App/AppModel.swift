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
    struct RealtimeCompletionState {
        var tool: String
        var finishedAt: Date
    }

    var projects: [Project]
    var workspaces: [ProjectWorkspace]
    var selectedProjectID: UUID?
    var appSettings: AppSettings
    var rightPanel: RightPanelKind?
    var commitMessage = ""
    var statusMessage = ""
    var isSidebarExpanded = false
    var activityByProjectID: [UUID: ProjectActivityPhase] = [:]
    var activityRenderVersion: UInt64 = 0
    var rightPanelWidth: CGFloat = 360
    var isGeneratingCommitMessage = false
    var terminalFocusRequestID: UUID?
    let runtimeStore = AIRuntimeStateStore.shared
    let aiStatsStore = AIStatsStore()
    let gitStore = GitStore()

    private let persistenceService: PersistenceService
    private let gitService = GitService()
    private let gitCredentialStore = GitCredentialStore()
    private let activityService = ProjectActivityService()
    private let runtimeIngressService = AIRuntimeIngressService.shared
    private let toolDriverFactory = AIToolDriverFactory.shared
    private let debugLog = AppDebugLog.shared
    private var activityStatusWatcher: DispatchSourceFileSystemObject?
    private var appActivationObservers: [NSObjectProtocol] = []
    private var runtimeBridgeObserver: NSObjectProtocol?
    private var runtimeActivityObserver: NSObjectProtocol?
    private var lastCompletionTokenByProjectID: [UUID: String] = [:]
    private var clearedCompletionTokenByProjectID: [UUID: String] = [:]
    private var realtimeCompletionByProjectID: [UUID: RealtimeCompletionState] = [:]
    private var lastRealtimeResponseBySessionID: [UUID: AIResponseState?] = [:]
    private var realtimeProjectIDBySessionID: [UUID: UUID] = [:]
    private var isSystemUIReady = false

    init(snapshot: AppSnapshot?, persistenceService: PersistenceService) {
        self.persistenceService = persistenceService
        debugLog.reset()

        if let snapshot {
            self.projects = snapshot.projects
            self.workspaces = snapshot.workspaces
            self.selectedProjectID = snapshot.selectedProjectID ?? snapshot.projects.first?.id
            self.appSettings = snapshot.appSettings ?? AppSettings()
        } else {
            self.projects = []
            self.workspaces = []
            self.selectedProjectID = nil
            self.appSettings = AppSettings()
        }

        refreshGitState()
        resetActivityState()
        activityService.clearAllStatuses()
        runtimeIngressService.resetEphemeralState()
        refreshProjectActivity(sendNotifications: false)
        activityService.requestNotificationPermission()
        observeApplicationActivation()
        startActivityWatchers()
        debugLog.log("app", "launch selectedProject=\(selectedProject?.name ?? "nil")")
        aiStatsStore.configureIntervals(
            automatic: appSettings.aiAutoRefreshInterval,
            background: appSettings.aiBackgroundRefreshInterval
        )
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
        refreshAIStatsIfNeeded()

        Task { @MainActor in
            self.isSystemUIReady = true
            self.applyThemeMode()
            self.applyAppIcon()
            self.updateDockBadge()
        }
    }

    private func resetActivityState() {
        runtimeStore.reset()
        activityByProjectID = [:]
        activityRenderVersion = 0
        lastCompletionTokenByProjectID.removeAll()
        clearedCompletionTokenByProjectID.removeAll()
        realtimeCompletionByProjectID.removeAll()
        lastRealtimeResponseBySessionID.removeAll()
        realtimeProjectIDBySessionID.removeAll()
    }

    static func bootstrap() -> AppModel {
        let persistenceService = PersistenceService()
        let snapshot = persistenceService.load()
        return AppModel(snapshot: snapshot, persistenceService: persistenceService)
    }

    var selectedProject: Project? {
        guard let selectedProjectID else {
            return nil
        }

        return projects.first(where: { $0.id == selectedProjectID })
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
        appSettings.terminalBackgroundPreset
    }

    var terminalChromeColor: Color {
        Color(nsColor: terminalBackgroundPreset.backgroundColor)
    }

    var terminalDividerColor: Color {
        Color(nsColor: terminalBackgroundPreset.dividerColor)
    }

    var terminalDividerNSColor: NSColor {
        terminalBackgroundPreset.dividerColor
    }

    var terminalTextColor: Color {
        Color(nsColor: terminalBackgroundPreset.foregroundColor)
    }

    var terminalMutedTextColor: Color {
        Color(nsColor: terminalBackgroundPreset.mutedForegroundColor)
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

    func selectProject(_ projectID: UUID) {
        selectedProjectID = projectID
        restoreCachedGitPanelIfAvailable(for: projectID)
        clearCompletedActivityIfNeeded(for: projectID)
        persist()
        refreshGitState()
        updateGitRemoteSyncPolling()
        refreshAIStatsIfNeeded()
    }

    func selectProject(atSidebarIndex index: Int) {
        guard projects.indices.contains(index) else {
            return
        }
        selectProject(projects[index].id)
    }

    func openDebugLog() {
        debugLog.log("app", "open debug log")
        debugLog.openInSystemViewer()
        statusMessage = String(localized: "app.debug_log.opened", defaultValue: "Opened debug log.", bundle: .module)
    }

    func openSelectedProjectInVSCode() {
        guard let project = selectedProject else {
            statusMessage = String(localized: "project.none_selected", defaultValue: "No project selected.", bundle: .module)
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.microsoft.VSCode",
            fallbackURL: vscodeOpenURL(for: project.path),
            successMessage: String(localized: "project.open.vscode.success", defaultValue: "Opened project in VS Code.", bundle: .module),
            failureMessage: String(localized: "project.open.vscode.failure", defaultValue: "Unable to find VS Code for this directory.", bundle: .module)
        )
    }

    func revealSelectedProjectInFinder() {
        guard let project = selectedProject else {
            statusMessage = String(localized: "project.none_selected", defaultValue: "No project selected.", bundle: .module)
            return
        }

        revealProjectInFinder(project.id)
    }

    func revealProjectInFinder(_ projectID: UUID) {
        guard let project = projects.first(where: { $0.id == projectID }) else {
            statusMessage = String(localized: "project.not_found", defaultValue: "Project not found.", bundle: .module)
            return
        }

        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: project.path, isDirectory: true)])
        statusMessage = String(localized: "project.reveal.finder.success", defaultValue: "Revealed project in Finder.", bundle: .module)
    }

    func openProjectDirectory(_ projectID: UUID) {
        guard let project = projects.first(where: { $0.id == projectID }) else {
            statusMessage = String(localized: "project.not_found", defaultValue: "Project not found.", bundle: .module)
            return
        }

        NSWorkspace.shared.open(URL(fileURLWithPath: project.path, isDirectory: true))
        statusMessage = String(localized: "project.open.folder.success", defaultValue: "Opened project folder.", bundle: .module)
    }

    func openSelectedProjectInTerminal() {
        guard let project = selectedProject else {
            statusMessage = String(localized: "project.none_selected", defaultValue: "No project selected.", bundle: .module)
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.apple.Terminal",
            successMessage: String(localized: "project.open.terminal.success", defaultValue: "Opened project in Terminal.", bundle: .module),
            failureMessage: String(localized: "project.open.terminal.failure", defaultValue: "Terminal not found.", bundle: .module)
        )
    }

    func openSelectedProjectInITerm2() {
        guard let project = selectedProject else {
            statusMessage = String(localized: "project.none_selected", defaultValue: "No project selected.", bundle: .module)
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.googlecode.iterm2",
            successMessage: String(localized: "project.open.iterm2.success", defaultValue: "Opened project in iTerm2.", bundle: .module),
            failureMessage: String(localized: "project.open.iterm2.failure", defaultValue: "iTerm2 not found.", bundle: .module)
        )
    }

    func openSelectedProjectInGhostty() {
        guard let project = selectedProject else {
            statusMessage = String(localized: "project.none_selected", defaultValue: "No project selected.", bundle: .module)
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.mitchellh.ghostty",
            successMessage: String(localized: "project.open.ghostty.success", defaultValue: "Opened project in Ghostty.", bundle: .module),
            failureMessage: String(localized: "project.open.ghostty.failure", defaultValue: "Ghostty not found.", bundle: .module)
        )
    }

    func openSelectedProjectInXcode() {
        guard let project = selectedProject else {
            statusMessage = String(localized: "project.none_selected", defaultValue: "No project selected.", bundle: .module)
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.apple.dt.Xcode",
            successMessage: String(localized: "project.open.xcode.success", defaultValue: "Opened project in Xcode.", bundle: .module),
            failureMessage: String(localized: "project.open.xcode.failure", defaultValue: "Xcode not found.", bundle: .module)
        )
    }

    func addProject() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ProjectEditorDialogState(
            title: String(localized: "project.create.title", defaultValue: "Create Project", bundle: .module),
            message: String(localized: "project.create.message", defaultValue: "Fill in the project name, directory, color, and icon.", bundle: .module),
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            name: "",
            path: "",
            badgeText: "",
            badgeSymbol: nil,
            badgeColorHex: systemAccentHexString()
        )

        ProjectEditorPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, let result else { return }
            self.importProject(
                name: result.name.trimmingCharacters(in: .whitespacesAndNewlines),
                path: result.path.trimmingCharacters(in: .whitespacesAndNewlines),
                badgeText: result.badgeText,
                badgeSymbol: result.badgeSymbol,
                badgeColorHex: result.badgeColorHex
            )
        }
    }

    func openProjectFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.title = String(localized: "project.open_folder.title", defaultValue: "Open Folder", bundle: .module)
        panel.prompt = String(localized: "project.open_folder.prompt", defaultValue: "Open", bundle: .module)
        panel.message = String(localized: "project.open_folder.message", defaultValue: "Choose a project folder to import.", bundle: .module)

        let completion: (NSApplication.ModalResponse) -> Void = { [weak self] response in
            guard let self, response == .OK, let url = panel.url else {
                return
            }
            self.importProject(
                name: url.lastPathComponent,
                path: url.path,
                badgeText: "",
                badgeSymbol: nil,
                badgeColorHex: systemAccentHexString()
            )
        }

        if let parentWindow = presentationWindow() {
            panel.beginSheetModal(for: parentWindow, completionHandler: completion)
        } else {
            completion(panel.runModal())
        }
    }

    func editProject(_ projectID: UUID) {
        guard let index = projects.firstIndex(where: { $0.id == projectID }) else { return }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let project = projects[index]
        let dialog = ProjectEditorDialogState(
            title: String(localized: "project.edit.title", defaultValue: "Edit Project", bundle: .module),
            message: String(localized: "project.edit.message", defaultValue: "Update the project name, directory, color, and icon.", bundle: .module),
            confirmTitle: String(localized: "common.save", defaultValue: "Save", bundle: .module),
            name: project.name,
            path: project.path,
            badgeText: project.badgeText ?? "",
            badgeSymbol: project.badgeSymbol,
            badgeColorHex: project.badgeColorHex ?? systemAccentHexString()
        )

        ProjectEditorPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, let result else { return }
            appendProjectEditLog("[EditProjectResult] id=\(projectID.uuidString) name=\(result.name) symbol=\(result.badgeSymbol ?? "nil") color=\(result.badgeColorHex)")

            let updatedProject = Project(
                id: project.id,
                name: result.name.trimmingCharacters(in: .whitespacesAndNewlines),
                path: result.path.trimmingCharacters(in: .whitespacesAndNewlines),
                shell: project.shell,
                defaultCommand: project.defaultCommand,
                badgeText: result.badgeText,
                badgeSymbol: result.badgeSymbol,
                badgeColorHex: result.badgeColorHex,
                gitDefaultPushRemoteName: project.gitDefaultPushRemoteName
            )
            var updatedProjects = self.projects
            updatedProjects[index] = updatedProject
            self.projects = updatedProjects
            NSLog("[ProjectEdit] save id=%@ symbol=%@ color=%@ text=%@", project.id.uuidString, updatedProject.badgeSymbol ?? "nil", updatedProject.badgeColorHex ?? "nil", updatedProject.badgeText ?? "nil")

            if let workspaceIndex = self.workspaces.firstIndex(where: { $0.projectID == projectID }) {
                var updatedWorkspaces = self.workspaces
                for sessionIndex in updatedWorkspaces[workspaceIndex].sessions.indices {
                    updatedWorkspaces[workspaceIndex].sessions[sessionIndex].projectName = updatedProject.name
                    updatedWorkspaces[workspaceIndex].sessions[sessionIndex].cwd = updatedProject.path
                }
                self.workspaces = updatedWorkspaces
            }

            self.statusMessage = String(
                format: String(localized: "project.update.success_format", defaultValue: "Updated project %@.", bundle: .module),
                updatedProject.name
            )
            self.persist()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            if let data = try? encoder.encode(AppSnapshot(projects: self.projects, workspaces: self.workspaces, selectedProjectID: self.selectedProjectID)),
               let encoded = String(data: data, encoding: .utf8) {
                appendProjectEditLog("[PersistSnapshot] \(encoded)")
            }
            self.refreshGitState()
            self.refreshAIStatsIfNeeded()
        }
    }

    func removeProject(_ projectID: UUID) {
        guard let project = projects.first(where: { $0.id == projectID }) else { return }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = ConfirmDialogState(
            title: String(localized: "project.remove.title", defaultValue: "Remove Project", bundle: .module),
            message: String(
                format: String(localized: "project.remove.confirm_format", defaultValue: "Are you sure you want to remove project %@? Files on disk will not be deleted.", bundle: .module),
                project.name
            ),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.remove", defaultValue: "Remove", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.projects.removeAll { $0.id == projectID }
            self.workspaces.removeAll { $0.projectID == projectID }
            if self.selectedProjectID == projectID {
                self.selectedProjectID = self.projects.first?.id
            }
            self.statusMessage = String(
                format: String(localized: "project.remove.success_format", defaultValue: "Removed project %@.", bundle: .module),
                project.name
            )
            self.persist()
            self.refreshGitState()
            self.updateGitRemoteSyncPolling()
            self.refreshAIStatsIfNeeded()
        }
    }

    func closeCurrentProject() {
        guard let project = selectedProject else {
            return
        }

        projects.removeAll { $0.id == project.id }
        workspaces.removeAll { $0.projectID == project.id }
        selectedProjectID = projects.first?.id
        statusMessage = String(
            format: String(localized: "project.close.success_format", defaultValue: "Closed project %@.", bundle: .module),
            project.name
        )
        persist()
        refreshGitState()
        updateGitRemoteSyncPolling()
        refreshAIStatsIfNeeded()
    }

    func closeAllProjects() {
        guard !projects.isEmpty else {
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "workspace.close_all_projects.title", defaultValue: "Close All Projects", bundle: .module),
            message: String(localized: "workspace.close_all_projects.message", defaultValue: "Are you sure you want to close all projects in the current workspace? Files on disk will not be deleted.", bundle: .module),
            icon: "xmark.rectangle.stack",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "workspace.close_all_projects.confirm", defaultValue: "Close All", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.projects.removeAll()
            self.workspaces.removeAll()
            self.selectedProjectID = nil
            self.rightPanel = nil
            self.statusMessage = String(localized: "workspace.close_all_projects.success", defaultValue: "Closed all projects.", bundle: .module)
            self.persist()
            self.refreshGitState()
            self.updateGitRemoteSyncPolling()
            self.refreshAIStatsIfNeeded()
        }
    }

    func toggleRightPanel(_ kind: RightPanelKind) {
        if rightPanel == kind {
            rightPanel = nil
        } else {
            rightPanel = kind
        }
        updateGitRemoteSyncPolling()
        refreshAIStatsIfNeeded()
    }

    func cancelCurrentAIIndexing() {
        aiStatsStore.cancelCurrent(project: selectedProject, projects: projects)
    }

    func refreshCurrentAIIndexing() {
        aiStatsStore.refreshCurrent(project: selectedProject, projects: projects, selectedSessionID: selectedSessionID)
    }

    func aiSessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        toolDriverFactory.sessionCapabilities(for: session)
    }

    func openAISession(_ session: AISessionSummary) {
        guard let command = toolDriverFactory.resumeCommand(for: session) else {
            statusMessage = String(localized: "ai.session.open.unsupported", defaultValue: "This session does not support opening.", bundle: .module)
            return
        }

        if tryReuseSelectedTopTerminalForCommand(command) {
            statusMessage = String(localized: "ai.session.open.current_success", defaultValue: "Opened session in the current terminal.", bundle: .module)
            return
        }

        guard let newSessionID = createSplitTerminal(command: command, axis: .horizontal) else {
            statusMessage = String(localized: "workspace.split.create_failed", defaultValue: "Unable to create a new split pane.", bundle: .module)
            return
        }
        terminalFocusRequestID = newSessionID
        statusMessage = String(localized: "ai.session.open.split_success", defaultValue: "Opened session in a new split pane.", bundle: .module)
    }

    func renameAISession(_ session: AISessionSummary) {
        let capabilities = aiSessionCapabilities(for: session)
        guard capabilities.canRename else {
            statusMessage = String(localized: "ai.session.rename.unsupported", defaultValue: "This session does not support renaming.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = GitInputDialogState(
            kind: .renameAISession,
            title: String(localized: "ai.session.rename.title", defaultValue: "Rename Session", bundle: .module),
            message: String(localized: "ai.session.rename.message", defaultValue: "Directly update the local session title for the corresponding AI CLI.", bundle: .module),
            placeholder: String(localized: "ai.session.rename.placeholder", defaultValue: "Enter a New Session Title", bundle: .module),
            confirmTitle: String(localized: "common.save", defaultValue: "Save", bundle: .module),
            value: session.sessionTitle,
            isMultiline: false
        )
        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] value in
            guard let self, let value else {
                return
            }
            let title = value.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !title.isEmpty else {
                self.statusMessage = String(localized: "ai.session.rename.empty", defaultValue: "Session title cannot be empty.", bundle: .module)
                return
            }

            do {
                try self.toolDriverFactory.renameSession(session, to: title)
                self.aiStatsStore.renameSessionOptimistically(projectID: session.projectID, sessionID: session.sessionID, title: title)
                self.invalidateAISessionCachesAndRefresh()
                self.statusMessage = String(localized: "ai.session.rename.success", defaultValue: "Renamed session.", bundle: .module)
            } catch {
                self.statusMessage = error.localizedDescription
            }
        }
    }

    func removeAISession(_ session: AISessionSummary) {
        let capabilities = aiSessionCapabilities(for: session)
        guard capabilities.canRemove else {
            statusMessage = String(localized: "ai.session.remove.unsupported", defaultValue: "This session does not support removal.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "ai.session.remove.title", defaultValue: "Remove Session", bundle: .module),
            message: String(localized: "ai.session.remove.confirm", defaultValue: "This will directly modify the local session storage for the corresponding AI CLI. Continue?", bundle: .module),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.remove", defaultValue: "Remove", bundle: .module),
            primaryTint: AppTheme.warning,
            secondaryTitle: nil,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else {
                return
            }

            do {
                try self.toolDriverFactory.removeSession(session)
                self.aiStatsStore.removeSessionOptimistically(projectID: session.projectID, sessionID: session.sessionID)
                self.invalidateAISessionCachesAndRefresh()
                self.statusMessage = String(localized: "ai.session.remove.success", defaultValue: "Removed session.", bundle: .module)
            } catch {
                self.statusMessage = error.localizedDescription
            }
        }
    }

    func toggleSidebarExpansion() {
        isSidebarExpanded.toggle()
    }

    func updateRightPanelWidth(_ width: CGFloat) {
        rightPanelWidth = min(max(width, 280), 560)
    }

    func updateBottomPaneHeight(_ height: CGFloat, for projectID: UUID, availableHeight: CGFloat) {
        let maxHeight = max(180, availableHeight - 180)
        let clamped = min(max(height, 160), maxHeight)

        guard let index = workspaces.firstIndex(where: { $0.projectID == projectID }) else {
            return
        }

        workspaces[index].bottomPaneHeight = clamped
        persist()
    }

    func createNewTerminal() {
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            workspace.sessions.append(session)

            if workspace.addTopSession(session.id) {
                statusMessage = "Created a new terminal pane."
            } else {
                workspace.addBottomTab(session.id)
                statusMessage = "Top row is full, added a bottom tab instead."
            }
        }
    }

    func createBottomTab() {
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            workspace.sessions.append(session)
            workspace.addBottomTab(session.id)
            statusMessage = workspace.bottomTabSessionIDs.count == 1
                ? String(localized: "workspace.bottom_split.created", defaultValue: "Created the bottom split area.", bundle: .module)
                : String(localized: "workspace.bottom_tab.created", defaultValue: "Added a new bottom tab.", bundle: .module)
        }
    }

    func splitSelectedPane(axis: PaneAxis) {
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            workspace.sessions.append(session)

            switch axis {
            case .horizontal:
                if workspace.addTopSession(session.id) {
                    statusMessage = String(localized: "workspace.top_pane.horizontal_created", defaultValue: "Added a horizontal pane.", bundle: .module)
                } else {
                    workspace.sessions.removeAll(where: { $0.id == session.id })
                    statusMessage = String(
                        format: String(localized: "workspace.top_pane.limit_format", defaultValue: "The top row supports up to %@ panes.", bundle: .module),
                        "\(ProjectWorkspace.maxTopPanes)"
                    )
                }
            case .vertical:
                workspace.addBottomTab(session.id)
                statusMessage = workspace.bottomTabSessionIDs.count == 1
                    ? String(localized: "workspace.bottom_split.created", defaultValue: "Created the bottom split area.", bundle: .module)
                    : String(localized: "workspace.bottom_tab.additional", defaultValue: "Added a tab to the bottom split area.", bundle: .module)
            }
        }
    }

    func selectSession(_ sessionID: UUID) {
        mutateSelectedWorkspace { workspace, _ in
            workspace.selectedSessionID = sessionID
        }
        refreshAIStatsIfNeeded()
    }

    func selectBottomTabSession(_ sessionID: UUID) {
        mutateSelectedWorkspace { workspace, _ in
            guard workspace.bottomTabSessionIDs.contains(sessionID) else { return }
            workspace.selectedBottomTabSessionID = sessionID
            workspace.selectedSessionID = sessionID
            terminalFocusRequestID = sessionID
        }
        refreshAIStatsIfNeeded()
    }

    func updateTopPaneRatios(_ ratios: [CGFloat]) {
        mutateSelectedWorkspace { workspace, _ in
            guard ratios.count == workspace.topSessionIDs.count, !ratios.isEmpty else { return }
            workspace.topPaneRatios = ratios
        }
    }

    func consumeTerminalFocusRequest(_ sessionID: UUID) {
        guard terminalFocusRequestID == sessionID else { return }
        terminalFocusRequestID = nil
    }

    func session(for sessionID: UUID) -> TerminalSession? {
        selectedWorkspace?.sessions.first(where: { $0.id == sessionID })
    }

    func activityPhase(for projectID: UUID) -> ProjectActivityPhase {
        let runtimePhase = runtimeStore.projectPhase(projectID: projectID)
        if runtimePhase != .idle {
            return runtimePhase
        }
        if let phase = activityByProjectID[projectID], phase != .idle {
            if case .running(let tool) = phase, isRealtimeAITool(tool) {
                return .idle
            }
            return phase
        }
        return .idle
    }

    func isGitEntrySelected(_ entry: GitFileEntry) -> Bool {
        gitStore.isEntrySelected(entry)
    }

    func toggleGitEntrySelection(_ entry: GitFileEntry) {
        gitStore.toggleEntrySelection(entry)
    }

    func selectGitEntry(_ entry: GitFileEntry, extendingRange: Bool) {
        gitStore.selectEntry(entry, in: gitState, extendingRange: extendingRange)
    }

    func setAllGitEntrySelection(for kind: GitFileKind, selected: Bool) {
        gitStore.setAllEntrySelection(for: kind, in: gitState, selected: selected)
    }

    func prepareGitEntryContextMenu(_ entry: GitFileEntry) {
        if selectedGitEntryIDs.contains(entry.id) {
            gitStore.prepareEntryContextMenu(entry)
            loadDiff(for: entry)
            return
        }

        selectGitEntry(entry, extendingRange: false)
        loadDiff(for: entry)
    }

    func prepareGitCommitContextMenu(_ commit: GitCommitEntry) {
        gitStore.selectCommit(commit)
    }

    var selectedGitEntriesForContextMenu: [GitFileEntry] {
        guard let gitState else { return [] }
        let all = flatGitEntries(from: gitState)
        return all.filter { selectedGitEntryIDs.contains($0.id) }
    }

    func copyGitPaths(_ entries: [GitFileEntry]) {
        let text = entries.map(\.path).joined(separator: "\n")
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        statusMessage = entries.count > 1 ? String(localized: "git.files.copy_selected_paths.success", defaultValue: "Copied selected file paths.", bundle: .module) : String(localized: "git.files.copy_path.success", defaultValue: "Copied file path.", bundle: .module)
    }

    func addGitEntriesToIgnore(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }

        let repositoryPath = project.path
        isGitLoading = true

        Task.detached {
            let service = GitService()
            do {
                try service.appendToGitignore(paths, at: repositoryPath)
                await MainActor.run {
                    self.statusMessage = paths.count > 1 ? String(localized: "git.ignore.added", defaultValue: "Added to .gitignore.", bundle: .module) : String(localized: "git.ignore.added", defaultValue: "Added to .gitignore.", bundle: .module)
                    self.refreshGitState()
                }
            } catch {
                await MainActor.run {
                    self.isGitLoading = false
                    self.statusMessage = error.localizedDescription
                }
            }
        }
    }

    func revealGitEntriesInFinder(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let urls = entries.map { URL(fileURLWithPath: project.path).appendingPathComponent($0.path) }
        guard !urls.isEmpty else { return }
        NSWorkspace.shared.activateFileViewerSelecting(urls)
    }

    func mergeBranchIntoCurrent(_ branch: String) {
        guard let project = selectedProject else { return }
        let currentBranch = gitState?.branch ?? String(localized: "git.branch.current_label", defaultValue: "Current Branch", bundle: .module)
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "git.branch.merge.title", defaultValue: "Merge Branch", bundle: .module),
            message: String(
                format: String(localized: "git.branch.merge.message_format", defaultValue: "Merge %@ into %@.", bundle: .module),
                branch,
                currentBranch
            ),
            icon: "arrow.merge",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.merge", defaultValue: "Merge", bundle: .module),
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            let path = project.path
            self.isGitLoading = true

            Task.detached {
                let service = GitService()
                do {
                    try service.merge(branch: branch, intoCurrentBranchAt: path)
                    await MainActor.run {
                        self.statusMessage = String(
                            format: String(localized: "git.branch.merge.success_format", defaultValue: "Merged branch %@.", bundle: .module),
                            branch
                        )
                        self.refreshGitState()
                    }
                } catch {
                    await MainActor.run {
                        self.isGitLoading = false
                        self.statusMessage = error.localizedDescription
                    }
                }
            }
        }
    }

    func syncMainBranchIntoCurrent() {
        let mainBranch = gitBranches.contains("main") ? "main" : (gitBranches.contains("master") ? "master" : nil)
        guard let mainBranch else {
            statusMessage = String(localized: "git.main_branch.not_found", defaultValue: "Main branch not found.", bundle: .module)
            return
        }

        if gitState?.branch == mainBranch {
            statusMessage = String(localized: "git.main_branch.already_on", defaultValue: "Already on the main branch.", bundle: .module)
            return
        }

        let currentBranch = gitState?.branch ?? String(localized: "git.branch.current_label", defaultValue: "Current Branch", bundle: .module)
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = ConfirmDialogState(
            title: String(localized: "git.main_branch.sync.title", defaultValue: "Sync Main Branch", bundle: .module),
            message: String(
                format: String(localized: "git.main_branch.sync.message_format", defaultValue: "Merge the latest changes from %@ into %@.", bundle: .module),
                mainBranch,
                currentBranch
            ),
            icon: "arrow.triangle.merge",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.sync", defaultValue: "Sync", bundle: .module),
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.mergeBranchIntoCurrent(mainBranch)
        }
    }

    func closeSession(_ sessionID: UUID) {
        mutateSelectedWorkspace { workspace, _ in
            let totalCount = workspace.sessions.count
            guard totalCount > 1 else {
                statusMessage = String(localized: "terminal.keep_one_open", defaultValue: "At least one terminal must remain open.", bundle: .module)
                return
            }

            workspace.removeSession(sessionID)
            runtimeIngressService.clearLiveState(sessionID: sessionID)
            runtimeIngressService.clearResponseState(sessionID: sessionID)
            aiStatsStore.handleTerminalSessionClosed(
                sessionID: sessionID,
                project: selectedProject,
                projects: projects,
                selectedSessionID: selectedSessionID
            )
            SwiftTermTerminalRegistry.shared.release(sessionID: sessionID)
            statusMessage = String(localized: "terminal.closed", defaultValue: "Closed terminal.", bundle: .module)
        }
    }

    func confirmCloseSelectedSession() {
        guard let sessionID = selectedSessionID else {
            statusMessage = String(localized: "terminal.none_selected", defaultValue: "No terminal selected.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        guard let workspace = selectedWorkspace else {
            statusMessage = String(localized: "workspace.not_found", defaultValue: "Current workspace not found.", bundle: .module)
            return
        }
        guard workspace.sessions.count > 1 else {
            statusMessage = String(localized: "terminal.keep_one_open", defaultValue: "At least one terminal must remain open.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "workspace.close_current_split.title", defaultValue: "Close Current Split", bundle: .module),
            message: String(localized: "workspace.close_current_split.message", defaultValue: "Are you sure you want to close the current split or tab?", bundle: .module),
            icon: "xmark.rectangle.portrait",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.close", defaultValue: "Close", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else {
                return
            }
            self.closeSession(sessionID)
        }
    }

    func updateDefaultCommand(_ command: String) {
        guard let selectedProjectID, let index = projects.firstIndex(where: { $0.id == selectedProjectID }) else {
            return
        }

        projects[index].defaultCommand = command
        statusMessage = command.isEmpty
            ? String(localized: "project.default_command.cleared", defaultValue: "Cleared default startup command.", bundle: .module)
            : String(localized: "project.default_command.updated", defaultValue: "Updated default startup command.", bundle: .module)
        persist()
    }

    func refreshGitState() {
        gitStore.refresh(project: selectedProject)
    }

    func initializeGitRepository() {
        guard let project = selectedProject else { return }
        gitStore.initializeRepository(project: project) { self.statusMessage = $0 }
    }

    func cloneGitRepository() {
        guard let project = selectedProject else { return }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = GitInputDialogState(
            kind: .cloneRepository,
            title: String(localized: "git.empty.clone_remote_repository", defaultValue: "Clone Remote Repository", bundle: .module),
            message: String(localized: "git.clone.message", defaultValue: "Enter a remote repository URL to clone it into the current project directory.", bundle: .module),
            placeholder: "https://github.com/foo/bar.git",
            confirmTitle: String(localized: "git.clone.start", defaultValue: "Start Clone", bundle: .module),
            value: "",
            isMultiline: false
        )
        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] value in
            guard let self, let value else { return }
            let remoteURL = value.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !remoteURL.isEmpty else {
                self.statusMessage = String(localized: "git.remote.url_required", defaultValue: "Please enter a remote repository URL.", bundle: .module)
                return
            }

            let savedCredential = self.gitCredentialStore.credential(for: remoteURL)
            self.gitStore.cloneRepository(project: project, remoteURL: remoteURL, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in
                self.promptForGitCredential(completion: completion)
            }, onAuthSucceeded: { credential in
                self.gitCredentialStore.save(credential, for: remoteURL)
            })
        }
    }

    private func restoreCachedGitPanelIfAvailable(for projectID: UUID) {
        gitStore.restoreCachedState(for: projectID)
    }

    func checkoutGitBranch(_ branch: String) {
        guard let project = selectedProject else { return }
        gitStore.checkoutBranch(branch, project: project) { self.statusMessage = $0 }
    }

    func checkoutRemoteGitBranch(_ remoteBranch: String) {
        guard let project = selectedProject else { return }
        gitStore.checkoutRemoteBranch(remoteBranch, project: project) { self.statusMessage = $0 }
    }

    func selectGitCommit(_ commit: GitCommitEntry) {
        gitStore.selectCommit(commit)
    }

    func copyGitCommitHash(_ commit: GitCommitEntry) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(commit.hash, forType: .string)
        statusMessage = String(localized: "git.commit.hash.copied", defaultValue: "Copied commit hash.", bundle: .module)
    }

    func checkoutGitCommit(_ commit: GitCommitEntry) {
        guard let project = selectedProject else { return }
        gitStore.checkoutCommit(commit, project: project) { self.statusMessage = $0 }
    }

    func revertGitCommit(_ commit: GitCommitEntry) {
        guard let project = selectedProject else { return }
        gitStore.revertCommit(commit, project: project) { self.statusMessage = $0 }
    }

    func createBranch(from commit: GitCommitEntry) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = GitInputDialogState(
            kind: .createBranchFromCommit(commit.hash),
            title: String(localized: "git.branch.create_from_commit.title", defaultValue: "Create Branch from Commit", bundle: .module),
            message: String(localized: "git.branch.new.message", defaultValue: "Enter a new branch name.", bundle: .module),
            placeholder: "feature/from-commit",
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            value: "",
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] branchName in
            guard let self, let branchName else { return }
            self.createGitBranch(named: branchName, from: commit.hash)
        }
    }

    func restoreGitCommit(_ commit: GitCommitEntry, forceRemote: Bool) {
        guard let project = selectedProject else { return }

        let dialog = ConfirmDialogState(
            title: forceRemote ? String(localized: "git.history.restore_remote", defaultValue: "Restore This Revision Remotely", bundle: .module) : String(localized: "git.history.restore_local", defaultValue: "Restore This Revision Locally", bundle: .module),
            message: forceRemote
                ? String(localized: "git.history.restore_remote.message", defaultValue: "Reset the current branch to this revision and overwrite the remote branch.", bundle: .module)
                : String(localized: "git.history.restore_local.message", defaultValue: "Reset the current branch to this revision locally only.", bundle: .module),
            icon: forceRemote ? "arrow.uturn.backward.circle.fill" : "clock.arrow.circlepath",
            iconColor: AppTheme.warning,
            primaryTitle: forceRemote ? String(localized: "git.history.restore_remote.action", defaultValue: "Remote Restore", bundle: .module) : String(localized: "git.history.restore_local.action", defaultValue: "Local Restore", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.gitStore.restoreCommit(commit, forceRemote: forceRemote, project: project) { self.statusMessage = $0 }
        }
    }

    func pullGitBranch() {
        guard let project = selectedProject else { return }
        let savedCredential = credentialForSelectedProjectRemote()
        gitStore.performRemoteAction(GitRemoteAction.pull, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
            if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                self.gitCredentialStore.save(credential, for: remoteURL)
            }
        }, onConflict: {
            self.presentRemoteSyncConflictAlert(repositoryPath: project.path)
        })
    }

    func fetchGitBranch() {
        guard let project = selectedProject else { return }
        let savedCredential = credentialForSelectedProjectRemote()
        gitStore.performRemoteAction(GitRemoteAction.fetch, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
            if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                self.gitCredentialStore.save(credential, for: remoteURL)
            }
        }, onConflict: {
            self.presentRemoteSyncConflictAlert(repositoryPath: project.path)
        })
    }

    func refreshRemoteBranches() {
        guard let project = selectedProject else { return }
        gitStore.refreshRemoteBranches(project: project) { self.statusMessage = $0 }
    }

    func addGitRemote() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        guard let project = selectedProject else { return }

        let nameDialog = GitInputDialogState(
            kind: .cloneRepository,
            title: String(localized: "git.remote.add", defaultValue: "Add Remote", bundle: .module),
            message: String(localized: "git.remote.add.name_message", defaultValue: "Enter the remote name first, such as origin or upstream.", bundle: .module),
            placeholder: String(localized: "git.remote.name", defaultValue: "Remote Name", bundle: .module),
            confirmTitle: String(localized: "common.next", defaultValue: "Next", bundle: .module),
            value: "",
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: nameDialog, parentWindow: parentWindow) { [weak self] nameValue in
            guard let self else { return }
            let name = nameValue?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !name.isEmpty else { return }

            let urlDialog = GitInputDialogState(
                kind: .cloneRepository,
                title: String(localized: "git.remote.add", defaultValue: "Add Remote", bundle: .module),
                message: String(localized: "git.remote.add.url_message", defaultValue: "Enter the remote URL.", bundle: .module),
                placeholder: "https://github.com/org/repo.git",
                confirmTitle: String(localized: "common.add", defaultValue: "Add", bundle: .module),
                value: "",
                isMultiline: false
            )

            GitInputPanelPresenter.present(dialog: urlDialog, parentWindow: parentWindow) { urlValue in
                let url = urlValue?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                guard !url.isEmpty else { return }
                self.gitStore.addRemote(name: name, url: url, project: project) { self.statusMessage = $0 }
            }
        }
    }

    func removeGitRemote(_ remote: GitRemoteEntry) {
        guard let project = selectedProject else { return }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "git.remote.remove", defaultValue: "Remove Remote", bundle: .module),
            message: String(format: String(localized: "git.remote.remove.confirm_format", defaultValue: "Are you sure you want to remove remote %@?", bundle: .module), remote.name),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.remove", defaultValue: "Remove", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            if project.gitDefaultPushRemoteName == remote.name,
               let index = self.projects.firstIndex(where: { $0.id == project.id }) {
                self.projects[index].gitDefaultPushRemoteName = nil
                self.persist()
            }
            self.gitStore.removeRemote(remote, project: project) { self.statusMessage = $0 }
        }
    }

    func setDefaultPushRemote(_ remote: GitRemoteEntry) {
        guard let project = selectedProject,
              let index = projects.firstIndex(where: { $0.id == project.id }) else { return }
        projects[index].gitDefaultPushRemoteName = remote.name
        persist()
        statusMessage = String(format: String(localized: "git.remote.default_push.set_format", defaultValue: "Set %@ as the default push remote.", bundle: .module), remote.name)
    }

    func clearDefaultPushRemote() {
        guard let project = selectedProject,
              let index = projects.firstIndex(where: { $0.id == project.id }) else { return }
        projects[index].gitDefaultPushRemoteName = nil
        persist()
        statusMessage = String(localized: "git.remote.default_push.cleared", defaultValue: "Restored the Git default push target.", bundle: .module)
    }

    func pushGitBranch() {
        guard let project = selectedProject else { return }

        if let defaultRemote = defaultPushRemote(for: project) {
            pushGitBranch(to: defaultRemote)
            return
        }

        let savedCredential = credentialForSelectedProjectRemote()
        gitStore.performRemoteAction(GitRemoteAction.push, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
            if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                self.gitCredentialStore.save(credential, for: remoteURL)
            }
        }, onConflict: {
            self.presentRemoteSyncConflictAlert(repositoryPath: project.path)
        })
    }

    func pushGitBranch(_ branch: String? = nil, to remote: GitRemoteEntry) {
        guard let project = selectedProject else { return }
        let savedCredential = credentialForRemote(remote)
        let branchName = branch ?? gitState?.branch ?? "HEAD"
        gitStore.pushBranch(branchName, to: remote, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in
            self.promptForGitCredential(completion: completion)
        }, onAuthSucceeded: { credential in
            self.gitCredentialStore.save(credential, for: remote.url)
        })
    }

    func pushCurrentLocalBranch(to remoteBranch: String) {
        guard let project = selectedProject,
              let localBranch = gitState?.branch else { return }

        let parts = remoteBranch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).map(String.init)
        guard parts.count == 2,
              let remote = gitRemotes.first(where: { $0.name == parts[0] }) else {
            statusMessage = String(localized: "git.remote.matching_not_found", defaultValue: "Matching remote was not found.", bundle: .module)
            return
        }

        let savedCredential = credentialForRemote(remote)
        gitStore.pushLocalBranch(localBranch, to: remote, remoteBranch: parts[1], project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in
            self.promptForGitCredential(completion: completion)
        }, onAuthSucceeded: { credential in
            self.gitCredentialStore.save(credential, for: remote.url)
        })
    }

    func forcePushGitBranch() {
        guard let project = selectedProject else { return }
        let dialog = ConfirmDialogState(
            title: String(localized: "git.remote.force_push", defaultValue: "Force Push", bundle: .module),
            message: String(localized: "git.remote.force_push.message", defaultValue: "Overwrite the current remote branch. Only use this when you intentionally want to rewrite remote history.", bundle: .module),
            icon: "exclamationmark.arrow.trianglehead.2.clockwise.rotate.90",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.remote.force_push", defaultValue: "Force Push", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            let savedCredential = self.credentialForSelectedProjectRemote()
            self.gitStore.performRemoteAction(GitRemoteAction.forcePush, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
                if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                    self.gitCredentialStore.save(credential, for: remoteURL)
                }
            }, onConflict: {
                self.presentRemoteSyncConflictAlert(repositoryPath: project.path)
            })
        }
    }

    func revealRepositoryInFinder() {
        guard let project = selectedProject else { return }
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: project.path)])
    }

    func undoLastGitCommit() {
        guard let project = selectedProject else { return }
        let path = project.path
        let headCommitPushed = (try? gitService.isHeadCommitPushed(at: path)) ?? false

        let dialog = ConfirmDialogState(
            title: String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module),
            message: headCommitPushed
                ? String(localized: "git.history.undo_last_commit.remote_notice", defaultValue: "Undo the last commit but keep the file changes and staging state. This commit may already be on the remote, so you might need to force push later.", bundle: .module)
                : String(localized: "git.history.undo_last_commit.local_notice", defaultValue: "Undo the last commit but keep the file changes and staging state.", bundle: .module),
            icon: "arrow.uturn.backward",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.undo", defaultValue: "Undo", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.gitStore.undoLastCommit(headCommitPushed: headCommitPushed, project: project, onStatus: { self.statusMessage = $0 }, onRewriteWarning: {
                self.presentPostCommitRewriteAlert(actionTitle: String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module), repositoryPath: path)
            })
        }
    }

    func editLastGitCommitMessage() {
        guard let project = selectedProject else { return }
        let path = project.path
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let currentMessage = (try? gitService.lastCommitMessage(at: path)) ?? ""
        let headCommitPushed = (try? gitService.isHeadCommitPushed(at: path)) ?? false

        let dialog = GitInputDialogState(
            kind: .editLastCommitMessage(headCommitPushed: headCommitPushed),
            title: String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module),
            message: headCommitPushed
                ? String(localized: "git.commit.edit_last_message.remote_notice", defaultValue: "Only change the latest commit message. This commit may already be on the remote, so you might need to force push after editing it.", bundle: .module)
                : String(localized: "git.commit.edit_last_message.notice", defaultValue: "Only change the latest commit message.", bundle: .module),
            placeholder: String(localized: "git.commit.edit_last_message.placeholder", defaultValue: "Enter a new commit message", bundle: .module),
            confirmTitle: String(localized: "common.save", defaultValue: "Save", bundle: .module),
            value: currentMessage,
            isMultiline: true
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] message in
            guard let self, let message else { return }
            self.amendLastGitCommitMessage(message, headCommitPushed: headCommitPushed)
        }
    }

    func syncGitRemote() {
        guard let project = selectedProject else { return }
        let savedCredential = credentialForSelectedProjectRemote()
        gitStore.performRemoteAction(GitRemoteAction.sync, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
            if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                self.gitCredentialStore.save(credential, for: remoteURL)
            }
        }, onConflict: {
            self.presentRemoteSyncConflictAlert(repositoryPath: project.path)
        })
    }

    func createGitBranch() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = GitInputDialogState(
            kind: .createBranch,
            title: String(localized: "git.branch.new", defaultValue: "New Branch", bundle: .module),
            message: String(localized: "git.branch.new.message", defaultValue: "Enter a new branch name.", bundle: .module),
            placeholder: "feature/xxx",
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            value: "",
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] branchName in
            guard let self, let branchName else { return }
            self.createGitBranch(named: branchName)
        }
    }

    private func createGitBranch(named branchName: String, from commitHash: String? = nil) {
        guard let project = selectedProject else { return }
        gitStore.createBranch(branchName, from: commitHash, project: project) { self.statusMessage = $0 }
    }

    private func amendLastGitCommitMessage(_ message: String, headCommitPushed: Bool) {
        guard let project = selectedProject else { return }
        let path = project.path
        gitStore.amendLastCommitMessage(message, headCommitPushed: headCommitPushed, project: project, onStatus: { self.statusMessage = $0 }, onRewriteWarning: {
            self.presentPostCommitRewriteAlert(actionTitle: String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module), repositoryPath: path)
        })
    }

    func discard(_ entry: GitFileEntry) {
        guard let project = selectedProject else { return }
        gitStore.discardEntry(entry, project: project) { self.statusMessage = $0 }
    }

    func discardSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind != .staged }
        guard !entries.isEmpty else { return }
        gitStore.discardEntries(entries, project: project, successMessage: String(localized: "git.discard.selected.success", defaultValue: "Discarded selected changes.", bundle: .module)) { self.statusMessage = $0 }
    }

    func stageEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.filter { $0.kind != .staged }.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.stagePaths(paths, project: project, successMessage: String(localized: "git.stage.section.success", defaultValue: "Staged files in this section.", bundle: .module)) { self.statusMessage = $0 }
    }

    func unstageEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.filter { $0.kind == .staged }.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.unstagePaths(paths, project: project, successMessage: String(localized: "git.unstage.section.success", defaultValue: "Unstaged files in this section.", bundle: .module)) { self.statusMessage = $0 }
    }

    func discardEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        guard !entries.isEmpty else { return }
        gitStore.discardEntries(entries, project: project, successMessage: String(localized: "git.discard.section.success", defaultValue: "Discarded changes in this section.", bundle: .module)) { self.statusMessage = $0 }
    }

    func loadDiff(for entry: GitFileEntry) {
        gitStore.loadDiff(for: entry, project: selectedProject)
    }

    func stageAllChanges() {
        guard let project = selectedProject else {
            return
        }
        gitStore.stageAll(project: project) { self.statusMessage = $0 }
    }

    func unstageAllChanges() {
        guard let project = selectedProject else {
            return
        }
        gitStore.unstageAll(project: project) { self.statusMessage = $0 }
    }

    func stageSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind == .changed || $0.kind == .untracked }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.stagePaths(paths, project: project, successMessage: String(localized: "git.stage.selected.success", defaultValue: "Staged selected files.", bundle: .module)) { self.statusMessage = $0 }
    }

    func unstageSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind == .staged }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.unstagePaths(paths, project: project, successMessage: String(localized: "git.unstage.selected.success", defaultValue: "Unstaged selected files.", bundle: .module)) { self.statusMessage = $0 }
    }

    func stage(_ entry: GitFileEntry) {
        guard let project = selectedProject else {
            return
        }
        gitStore.stagePaths([entry.path], project: project, successMessage: String(format: String(localized: "git.stage.file.success_format", defaultValue: "Staged %@.", bundle: .module), entry.path)) { self.statusMessage = $0 }
    }

    func unstage(_ entry: GitFileEntry) {
        guard let project = selectedProject else {
            return
        }
        gitStore.unstagePaths([entry.path], project: project, successMessage: String(format: String(localized: "git.unstage.file.success_format", defaultValue: "Unstaged %@.", bundle: .module), entry.path)) { self.statusMessage = $0 }
    }

    func commitChanges() {
        performCommitAction(.commit)
    }

    func performCommitAction(_ action: GitCommitAction) {
        guard let project = selectedProject else {
            return
        }

        let trimmed = commitMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            statusMessage = String(localized: "git.commit.message.empty", defaultValue: "Commit message cannot be empty.", bundle: .module)
            return
        }

        gitStore.commit(message: trimmed, action: action, project: project, onStatus: { self.statusMessage = $0 }, onSuccess: {
            self.commitMessage = ""
            self.selectedGitEntry = nil
            self.selectedGitEntryIDs.removeAll()
            self.gitDiffText = String(localized: "git.commit.completed", defaultValue: "Commit completed.", bundle: .module)
        })
    }

    func generateCommitMessage() {
        guard let gitState else {
            statusMessage = String(localized: "git.commit.generate.unavailable", defaultValue: "There are no changes available to generate a commit message.", bundle: .module)
            return
        }

        let fileNames = Array(Set((gitState.staged + gitState.changes + gitState.untracked).map(\.path))).sorted()
        guard !fileNames.isEmpty else {
            statusMessage = String(localized: "git.commit.generate.unavailable", defaultValue: "There are no changes available to generate a commit message.", bundle: .module)
            return
        }

        isGeneratingCommitMessage = true
        let summary = fileNames.prefix(3).joined(separator: ", ")
        let suffix = fileNames.count > 3
            ? String(format: String(localized: "git.commit.generate.more_files_format", defaultValue: " and %@ more files", bundle: .module), "\(fileNames.count)")
            : ""
        commitMessage = String(format: String(localized: "git.commit.generate.summary_format", defaultValue: "Update %@%@ Git workspace interaction", bundle: .module), summary, suffix)
        isGeneratingCommitMessage = false
        statusMessage = String(localized: "git.commit.generate.success", defaultValue: "Generated commit message.", bundle: .module)
    }

    private func entries(for kind: GitFileKind, in state: GitRepositoryState) -> [GitFileEntry] {
        switch kind {
        case .staged:
            return state.staged
        case .changed:
            return state.changes
        case .untracked:
            return state.untracked
        }
    }

    private func selectedEntries(in state: GitRepositoryState) -> [GitFileEntry] {
        let all = state.staged + state.changes + state.untracked
        return all.filter { selectedGitEntryIDs.contains($0.id) }
    }

    private func flatGitEntries(from state: GitRepositoryState) -> [GitFileEntry] {
        state.staged + state.changes + state.untracked
    }

    private func promptForGitCredential(completion: @escaping (GitCredential?) -> Void) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            completion(nil)
            return
        }

        let dialog = GitCredentialDialogState(
            title: String(localized: "git.credentials.title", defaultValue: "Git Credentials Required", bundle: .module),
            message: String(localized: "git.credentials.message", defaultValue: "Remote access requires authentication. Enter your username and password or token to retry.", bundle: .module),
            confirmTitle: String(localized: "common.continue", defaultValue: "Continue", bundle: .module),
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        GitCredentialDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] credential in
            guard let self else {
                completion(nil)
                return
            }
            guard let credential else {
                self.statusMessage = String(localized: "git.auth.cancelled", defaultValue: "Git authentication was cancelled.", bundle: .module)
                completion(nil)
                return
            }

            guard !credential.username.isEmpty, !credential.password.isEmpty else {
                self.statusMessage = String(localized: "git.auth.credentials_required", defaultValue: "Username and password or token cannot be empty.", bundle: .module)
                completion(nil)
                return
            }

            completion(credential)
        }
    }

    private func credentialForSelectedProjectRemote() -> GitCredential? {
        guard let project = selectedProject,
              let remoteURL = try? gitService.originURL(at: project.path),
              !remoteURL.isEmpty else {
            return nil
        }
        return gitCredentialStore.credential(for: remoteURL)
    }

    private func defaultPushRemote(for project: Project) -> GitRemoteEntry? {
        guard let name = project.gitDefaultPushRemoteName else { return nil }
        return gitRemotes.first(where: { $0.name == name })
    }

    private func credentialForRemote(_ remote: GitRemoteEntry) -> GitCredential? {
        gitCredentialStore.credential(for: remote.url)
    }

    private func presentPostCommitRewriteAlert(actionTitle: String, repositoryPath: String) {
        let dialog = ConfirmDialogState(
            title: actionTitle,
            message: String(localized: "git.history.rewrite_warning.message", defaultValue: "Local history has changed. If the remote still contains the old commits, you may need to force push to sync it.", bundle: .module),
            icon: "arrow.trianglehead.2.clockwise.rotate.90",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.history.rewrite_warning.defer", defaultValue: "Handle Later", bundle: .module),
            secondaryTitle: String(localized: "git.history.rewrite_warning.force_push", defaultValue: "Force Push Now", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else { return }
            if result == .secondary {
                guard let project = self.selectedProject else { return }
                let savedCredential = self.credentialForSelectedProjectRemote()
                self.gitStore.performRemoteAction(GitRemoteAction.forcePush, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
                    if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                        self.gitCredentialStore.save(credential, for: remoteURL)
                    }
                }, onConflict: {
                    self.presentRemoteSyncConflictAlert(repositoryPath: repositoryPath)
                })
            } else {
                self.refreshGitState()
            }
        }
    }

    private func refreshGitRemoteSyncState(projectPath: String, projectID: UUID) {
        gitStore.refreshRemoteSyncState(projectPath: projectPath, projectID: projectID)
    }

    private func startGitRemoteSyncPolling() {
        gitStore.startRemoteSyncPolling(
            selectedProject: { self.selectedProject },
            isEnabled: { NSApp.isActive && self.rightPanel == .git }
        )
    }

    private func stopGitRemoteSyncPolling() {
        gitStore.stopRemoteSyncPolling()
    }

    private func updateGitRemoteSyncPolling() {
        guard NSApplication.shared.isActive, rightPanel == .git, selectedProject != nil else {
            stopGitRemoteSyncPolling()
            return
        }
        startGitRemoteSyncPolling()
    }

    private func observeApplicationActivation() {
        let center = NotificationCenter.default

        let becameActive = center.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.updateGitRemoteSyncPolling()
                self?.refreshAIStatsIfNeeded()
            }
        }

        let resignedActive = center.addObserver(
            forName: NSApplication.didResignActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.updateGitRemoteSyncPolling()
            }
        }

        appActivationObservers = [becameActive, resignedActive]
    }

    private func presentRemoteSyncConflictAlert(repositoryPath: String) {
        let dialog = ConfirmDialogState(
            title: String(localized: "git.sync.conflict.title", defaultValue: "Sync Conflict", bundle: .module),
            message: String(localized: "git.sync.conflict.message", defaultValue: "A conflict occurred while pulling. You can resolve it manually first, or force push the current branch directly.", bundle: .module),
            icon: "exclamationmark.triangle.fill",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.sync.conflict.resolve_manually", defaultValue: "Resolve Manually", bundle: .module),
            secondaryTitle: String(localized: "git.remote.force_push", defaultValue: "Force Push", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else { return }
            if result == .secondary {
                self.forcePushAfterConflict(at: repositoryPath)
            } else {
                self.statusMessage = String(localized: "git.remote.pull_conflicts", defaultValue: "Pull resulted in conflicts. Resolve them manually first.", bundle: .module)
                self.refreshGitState()
            }
        }
    }

    private func forcePushAfterConflict(at path: String) {
        guard let project = selectedProject else { return }
        let savedCredential = credentialForSelectedProjectRemote()
        gitStore.performRemoteAction(GitRemoteAction.forcePush, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
            if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                self.gitCredentialStore.save(credential, for: remoteURL)
            }
        }, onConflict: {
            self.presentRemoteSyncConflictAlert(repositoryPath: path)
        })
    }

    private func mutateSelectedWorkspace(_ transform: (inout ProjectWorkspace, Project) -> Void) {
        guard let selectedProjectID,
              let project = projects.first(where: { $0.id == selectedProjectID }),
              let index = workspaces.firstIndex(where: { $0.projectID == selectedProjectID }) else {
            return
        }

        var updatedWorkspaces = workspaces
        transform(&updatedWorkspaces[index], project)
        workspaces = updatedWorkspaces
        persist()
    }

    private func createSplitTerminal(command: String, axis: PaneAxis) -> UUID? {
        guard let selectedProjectID,
              let project = projects.first(where: { $0.id == selectedProjectID }),
              let index = workspaces.firstIndex(where: { $0.projectID == selectedProjectID }) else {
            return nil
        }

        var updatedWorkspaces = workspaces
        let session = TerminalSession.make(project: project, command: command)
        updatedWorkspaces[index].sessions.append(session)

        switch axis {
        case .horizontal:
            guard updatedWorkspaces[index].addTopSession(session.id) else {
                return nil
            }
        case .vertical:
            updatedWorkspaces[index].addBottomTab(session.id)
        }

        workspaces = updatedWorkspaces
        persist()
        refreshAIStatsIfNeeded()
        return session.id
    }

    private func tryReuseSelectedTopTerminalForCommand(_ command: String) -> Bool {
        guard let workspace = selectedWorkspace,
              let selectedSessionID,
              workspace.topSessionIDs.contains(selectedSessionID) else {
            return false
        }

        return tryReuseTerminalCommand(command, sessionID: selectedSessionID)
    }

    private func tryReuseTerminalCommand(_ command: String, sessionID: UUID) -> Bool {
        guard let selectedSessionID,
              selectedSessionID == sessionID,
              let shellPID = SwiftTermTerminalRegistry.shared.shellPID(for: sessionID),
              shellPID > 0 else {
            return false
        }

        let inspector = TerminalProcessInspector()
        guard inspector.activeTool(forShellPID: shellPID) == nil,
              inspector.hasActiveCommand(forShellPID: shellPID) == false else {
            return false
        }

        terminalFocusRequestID = sessionID
        return SwiftTermTerminalRegistry.shared.sendText(command + "\n", to: sessionID)
    }

    private func invalidateAISessionCachesAndRefresh() {
        guard let project = selectedProject else {
            return
        }
        aiStatsStore.invalidateProjectCaches(project: project)
        Task {
            await AIProjectSummaryCache.shared.invalidate(projectPath: project.path)
        }
        refreshCurrentAIIndexing()
    }

    private func openSelectedProjectInApplication(_ project: Project, bundleIdentifier: String, fallbackURL: URL? = nil, successMessage: String, failureMessage: String) {
        let projectURL = URL(fileURLWithPath: project.path, isDirectory: true)

        if let appURL = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) {
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            NSWorkspace.shared.open([projectURL], withApplicationAt: appURL, configuration: configuration) { [weak self] _, error in
                Task { @MainActor in
                    guard let self else { return }
                    self.statusMessage = error == nil ? successMessage : failureMessage
                }
            }
            return
        }

        if let fallbackURL, NSWorkspace.shared.open(fallbackURL) {
            statusMessage = successMessage
            return
        }

        statusMessage = failureMessage
    }

    private func vscodeOpenURL(for path: String) -> URL? {
        guard var components = URLComponents(string: "vscode://file") else {
            return nil
        }

        var normalizedPath = URL(fileURLWithPath: path, isDirectory: true).path
        if !normalizedPath.hasSuffix("/") {
            normalizedPath += "/"
        }
        components.percentEncodedPath = normalizedPath.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? normalizedPath
        return components.url
    }

    private func persist() {
        let snapshot = AppSnapshot(
            projects: projects,
            workspaces: workspaces,
            selectedProjectID: selectedProjectID,
            appSettings: appSettings
        )
        persistenceService.save(snapshot)
    }

    private func presentationWindow() -> NSWindow? {
        if let keyWindow = NSApp.keyWindow, !(keyWindow is NSPanel) {
            return keyWindow
        }
        if let mainWindow = NSApp.mainWindow, !(mainWindow is NSPanel) {
            return mainWindow
        }
        return NSApp.windows.first(where: { !($0 is NSPanel) && $0.isVisible })
    }

    private func startActivityWatchers() {
        activityStatusWatcher?.cancel()

        activityStatusWatcher = makeDirectoryWatcher(for: activityService.statusDirectoryURL())
        runtimeIngressService.startWatching()
        if let runtimeBridgeObserver {
            NotificationCenter.default.removeObserver(runtimeBridgeObserver)
        }
        if let runtimeActivityObserver {
            NotificationCenter.default.removeObserver(runtimeActivityObserver)
        }
        runtimeBridgeObserver = NotificationCenter.default.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.refreshProjectActivity(sendNotifications: true)
                if self?.rightPanel == .aiStats {
                    self?.refreshAIStatsIfNeeded()
                }
            }
        }
        runtimeActivityObserver = NotificationCenter.default.addObserver(
            forName: .dmuxAIRuntimeActivityPulse,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.refreshProjectActivity(sendNotifications: false)
            }
        }
    }

    private func makeDirectoryWatcher(for directoryURL: URL) -> DispatchSourceFileSystemObject? {
        let fd = open(directoryURL.path, O_EVTONLY)
        guard fd >= 0 else {
            return nil
        }

        let watcher = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .delete, .rename, .attrib, .link, .extend],
            queue: .main
        )
        watcher.setEventHandler { [weak self] in
            Task { @MainActor in
                self?.refreshProjectActivity(sendNotifications: true)
                if self?.rightPanel == .aiStats {
                    self?.refreshAIStatsIfNeeded()
                }
            }
        }
        watcher.setCancelHandler {
            close(fd)
        }
        watcher.resume()
        return watcher
    }

    private func refreshAIStatsIfNeeded() {
        aiStatsStore.refreshIfNeeded(project: selectedProject, projects: projects, selectedSessionID: selectedSessionID)
    }

    private func refreshProjectActivity(sendNotifications: Bool) {
        let payloads = activityService.loadStatuses(projects: projects)
        let liveEnvelopes = runtimeIngressService.importRuntime(projects: projects)
        let currentProjects = projects
        runtimeIngressService.refreshRuntimeSources(projects: currentProjects, liveEnvelopes: liveEnvelopes)

        var phases: [UUID: ProjectActivityPhase] = [:]
        let previousPhases = activityByProjectID

        for project in projects {
            let payload = payloads[project.id]
            var phase = activityService.phase(for: payload)
            let runtimeSnapshots = runtimeStore.liveSnapshots(projectID: project.id)
            updateRealtimeCompletionTracking(project: project, runtimeSnapshots: runtimeSnapshots)

            if let payload, isRealtimeAITool(payload.tool) {
                activityService.clearStatus(for: project.id)
                phase = .idle
            }

            if case .running(let tool) = phase, isRealtimeAITool(tool) {
                phase = .idle
            }

            if let payload,
               case .completed = phase,
               clearedCompletionTokenByProjectID[project.id] == activityService.completionToken(for: payload) {
                phase = .idle
            }

            let runtimePhase = runtimeStore.projectPhase(projectID: project.id)
            if runtimePhase != .idle {
                phase = runtimePhase
            } else if let completion = recentRealtimeCompletion(for: project.id) {
                phase = .completed(tool: completion.tool, finishedAt: completion.finishedAt, exitCode: nil)
            }

            phases[project.id] = phase

            if previousPhases[project.id] != phase {
                debugLog.log(
                    "activity",
                    "project=\(project.name) phase=\(debugActivityDescription(phase))"
                )
            }

            guard sendNotifications,
                  case .completed(let tool, let finishedAt, let exitCode) = phase else {
                continue
            }

            let token: String
            if let payload {
                token = activityService.completionToken(for: payload)
            } else {
                token = "realtime-\(tool)-\(Int(finishedAt.timeIntervalSince1970 * 1000))"
            }
            if lastCompletionTokenByProjectID[project.id] != token {
                lastCompletionTokenByProjectID[project.id] = token
                clearedCompletionTokenByProjectID[project.id] = nil
                activityService.notifyCompletion(projectName: project.name, tool: tool, exitCode: exitCode)
            }
        }

        activityByProjectID = phases
        activityRenderVersion &+= 1
        updateDockBadge()
    }

    private func debugActivityDescription(_ phase: ProjectActivityPhase) -> String {
        switch phase {
        case .idle:
            return "idle"
        case .running(let tool):
            return "running:\(tool)"
        case .completed(let tool, _, let exitCode):
            return "completed:\(tool):\(exitCode.map(String.init) ?? "nil")"
        }
    }

    private func isRealtimeAITool(_ tool: String) -> Bool {
        runtimeIngressService.isRealtimeTool(tool)
    }

    private func updateRealtimeCompletionTracking(project: Project, runtimeSnapshots: [AITerminalSessionSnapshot]) {
        let liveSessionIDs = Set(runtimeSnapshots.map(\.sessionID))

        for snapshot in runtimeSnapshots {
            guard let tool = snapshot.tool, isRealtimeAITool(tool) else {
                continue
            }

            realtimeProjectIDBySessionID[snapshot.sessionID] = project.id
            let previousState = lastRealtimeResponseBySessionID[snapshot.sessionID] ?? nil
            let currentState = snapshot.responseState

            if previousState == .responding, currentState == .idle, snapshot.status == "running" {
                realtimeCompletionByProjectID[project.id] = RealtimeCompletionState(
                    tool: tool,
                    finishedAt: snapshot.updatedAt
                )
            }

            lastRealtimeResponseBySessionID[snapshot.sessionID] = currentState
        }

        for sessionID in Array(lastRealtimeResponseBySessionID.keys) {
            guard realtimeProjectIDBySessionID[sessionID] == project.id,
                  !liveSessionIDs.contains(sessionID) else {
                continue
            }
            lastRealtimeResponseBySessionID[sessionID] = nil
            realtimeProjectIDBySessionID[sessionID] = nil
        }

        if let completion = realtimeCompletionByProjectID[project.id],
           Date().timeIntervalSince(completion.finishedAt) > 6 {
            realtimeCompletionByProjectID[project.id] = nil
        }
    }

    private func recentRealtimeCompletion(for projectID: UUID) -> RealtimeCompletionState? {
        guard let completion = realtimeCompletionByProjectID[projectID] else {
            return nil
        }
        guard Date().timeIntervalSince(completion.finishedAt) <= 6 else {
            realtimeCompletionByProjectID[projectID] = nil
            return nil
        }
        return completion
    }

    private func clearCompletedActivityIfNeeded(for projectID: UUID) {
        var didClear = false

        if realtimeCompletionByProjectID[projectID] != nil {
            realtimeCompletionByProjectID[projectID] = nil
            didClear = true
        }

        if case .completed = activityByProjectID[projectID] {
            activityByProjectID[projectID] = .idle
            didClear = true
        }

        if let payload = activityService.loadStatuses(projects: projects)[projectID],
           case .completed = activityService.phase(for: payload) {
            let token = activityService.completionToken(for: payload)
            clearedCompletionTokenByProjectID[projectID] = token
            activityByProjectID[projectID] = .idle
            activityService.clearStatus(for: projectID)
            didClear = true
        }

        if didClear {
            activityRenderVersion &+= 1
            updateDockBadge()
        }
    }

    func triggerActivityTest() {
        guard let project = selectedProject else {
            return
        }

        let tool = "dmux-test"
        activityService.writeTestStatus(project: project, tool: tool, phase: "running")
        refreshProjectActivity(sendNotifications: false)

        Task { @MainActor in
            try? await Task.sleep(for: .seconds(2))
            activityService.writeTestStatus(project: project, tool: tool, phase: "completed", exitCode: 0)
            if let payload = activityService.loadStatuses(projects: [project])[project.id] {
                let token = activityService.completionToken(for: payload)
                lastCompletionTokenByProjectID[project.id] = token
                clearedCompletionTokenByProjectID[project.id] = nil
            }
            activityService.notifyCompletion(projectName: project.name, tool: tool, exitCode: 0)
            refreshProjectActivity(sendNotifications: false)
        }
    }

    func updateThemeMode(_ mode: AppThemeMode) {
        var settings = appSettings
        settings.themeMode = mode
        appSettings = settings
        applyThemeMode()
        persist()
    }

    func updateLanguage(_ language: AppLanguage) {
        var settings = appSettings
        settings.language = language
        appSettings = settings
        persist()
        AppLanguageBootstrap.apply(language: language)
        aiStatsStore.refreshLocalizedStatusTexts()
        presentLanguageRestartPrompt()
    }

    func updateAppIconStyle(_ style: AppIconStyle) {
        var settings = appSettings
        settings.iconStyle = style
        appSettings = settings
        applyAppIcon()
        persist()
    }

    func updateTerminalBackgroundPreset(_ preset: AppTerminalBackgroundPreset) {
        var settings = appSettings
        settings.terminalBackgroundPreset = preset
        appSettings = settings
        persist()
    }

    func updateDefaultTerminal(_ terminal: AppTerminalProfile) {
        let previousShell = appSettings.defaultTerminal.shellPath
        var settings = appSettings
        settings.defaultTerminal = terminal
        appSettings = settings
        let nextShell = terminal.shellPath
        for index in projects.indices where projects[index].shell == previousShell || AppTerminalProfile.allShellPaths.contains(projects[index].shell) {
            projects[index].shell = nextShell
        }
        for workspaceIndex in workspaces.indices {
            for sessionIndex in workspaces[workspaceIndex].sessions.indices where workspaces[workspaceIndex].sessions[sessionIndex].shell == previousShell || AppTerminalProfile.allShellPaths.contains(workspaces[workspaceIndex].sessions[sessionIndex].shell) {
                workspaces[workspaceIndex].sessions[sessionIndex].shell = nextShell
            }
        }
        persist()
    }

    func updateDockBadgeEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.showsDockBadge = enabled
        appSettings = settings
        updateDockBadge()
        persist()
    }

    func updateShortcut(_ shortcut: AppKeyboardShortcut?, for target: AppShortcutTarget) {
        switch target {
        case .splitPane:
            appSettings.shortcuts.splitPane = shortcut
        case .createTab:
            appSettings.shortcuts.createTab = shortcut
        case .toggleGitPanel:
            appSettings.shortcuts.toggleGitPanel = shortcut
        case .toggleAIPanel:
            appSettings.shortcuts.toggleAIPanel = shortcut
        }
        persist()
    }

    func updateGitAutoRefreshInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.gitAutoRefreshInterval = interval
        appSettings = settings
        gitStore.configureRemoteSyncInterval(interval)
        persist()
    }

    func updateAIAutomaticRefreshInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.aiAutoRefreshInterval = interval
        appSettings = settings
        aiStatsStore.configureIntervals(
            automatic: interval,
            background: appSettings.aiBackgroundRefreshInterval
        )
        persist()
    }

    func updateAIBackgroundRefreshInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.aiBackgroundRefreshInterval = interval
        appSettings = settings
        aiStatsStore.configureIntervals(
            automatic: appSettings.aiAutoRefreshInterval,
            background: interval
        )
        persist()
    }

    func updateDeveloperNotificationTestButtonEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.developer.showsNotificationTestButton = enabled
        appSettings = settings
        persist()
    }

    func updateDeveloperDebugLogButtonEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.developer.showsDebugLogButton = enabled
        appSettings = settings
        persist()
    }

    func openURL(_ url: URL) {
        NSWorkspace.shared.open(url)
    }

    func checkForUpdates() {
        guard !isCheckingForUpdates else {
            return
        }
        isCheckingForUpdates = true
        statusMessage = String(localized: "update.checking", defaultValue: "Checking for updates...", bundle: .module)

        Task { @MainActor in
            defer { isCheckingForUpdates = false }

            do {
                let result = try await AppReleaseService.checkForUpdates(currentVersion: currentAppVersion)
                presentUpdateCheckResult(result)
            } catch {
                presentUpdateCheckError(error)
            }
        }
    }

    var appDisplayName: String {
        if let name = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String, !name.isEmpty {
            return name
        }
        if let name = Bundle.main.object(forInfoDictionaryKey: kCFBundleNameKey as String) as? String, !name.isEmpty {
            return name
        }
        return "Codux"
    }

    var appVersionDescription: String {
        let version = currentAppVersion
        let build = (Bundle.main.object(forInfoDictionaryKey: kCFBundleVersionKey as String) as? String) ?? "dev"
        return "v\(version) (\(build))"
    }

    var localizedUserAgreementDocument: String {
        [
            String(localized: "about.user_agreement_body", defaultValue: "This app is currently a development preview. By using it, you understand that terminal, Git, and AI activity features read local project metadata and runtime state, but do not proactively upload your project contents.", bundle: .module),
            String(localized: "about.user_agreement_data", defaultValue: "Codux only reads the local state needed to display terminal sessions, Git repository status, AI tool activity, and local statistics. You are responsible for reviewing any third-party CLI behavior and network activity triggered by those tools.", bundle: .module),
            String(localized: "about.user_agreement_responsibility", defaultValue: "You are responsible for your local environment, file permissions, repository credentials, notification permissions, and any commands executed inside the terminal.", bundle: .module),
            String(localized: "about.user_agreement_license", defaultValue: "Codux is distributed as open-source software under the GPL-3.0 license. Continued use means you accept that this experimental software may change behavior, interface, and compatibility over time.", bundle: .module)
        ].joined(separator: "\n\n")
    }

    var appIconImage: NSImage {
        AppIconRenderer.image(for: appSettings.iconStyle, size: 160)
    }

    private func applyThemeMode() {
        guard isSystemUIReady else {
            return
        }
        NSApp.appearance = appSettings.themeMode.appearance
        for window in NSApp.windows {
            guard !(window is NSPanel) else { continue }
            window.appearance = appSettings.themeMode.appearance
        }
    }

    private func applyAppIcon() {
        guard isSystemUIReady else {
            return
        }
        persistApplicationBundleIcon()
        updateDockBadge()
        applyRuntimeDockIcon()
    }

    private func persistApplicationBundleIcon() {
        let bundlePath = Bundle.main.bundleURL.path
        let iconImage: NSImage? = appSettings.iconStyle == .default
            ? nil
            : AppIconRenderer.image(for: appSettings.iconStyle, size: 1024)

        let didUpdate = NSWorkspace.shared.setIcon(iconImage, forFile: bundlePath, options: [])
        debugLog.log(
            "app",
            "bundle icon update style=\(appSettings.iconStyle.rawValue) success=\(didUpdate) path=\(bundlePath)"
        )

        guard didUpdate else {
            return
        }

        // Nudge Finder and LaunchServices to pick up the custom icon change immediately.
        NSWorkspace.shared.noteFileSystemChanged(bundlePath)
        let parentPath = Bundle.main.bundleURL.deletingLastPathComponent().path
        NSWorkspace.shared.noteFileSystemChanged(parentPath)
    }

    private func updateDockBadge() {
        guard isSystemUIReady else {
            return
        }
        NSApp.dockTile.contentView = nil
        guard appSettings.showsDockBadge else {
            NSApp.dockTile.badgeLabel = nil
            NSApp.dockTile.display()
            applyRuntimeDockIcon()
            return
        }

        let completedCount = activityByProjectID.values.reduce(into: 0) { partial, phase in
            if case .completed = phase {
                partial += 1
            }
        }
        NSApp.dockTile.badgeLabel = completedCount > 0 ? "\(completedCount)" : nil
        NSApp.dockTile.display()
        applyRuntimeDockIcon()
    }

    private func applyRuntimeDockIcon() {
        NSApplication.shared.applicationIconImage = AppIconRenderer.image(for: appSettings.iconStyle)
    }

    private var currentAppVersion: String {
        ((Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String) ?? "0.1.0")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func presentUpdateCheckResult(_ result: AppReleaseCheckResult) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        switch result {
        case .upToDate(let currentVersion, let latestVersion):
            statusMessage = String(localized: "update.latest.title", defaultValue: "You're up to date.", bundle: .module)
            let dialog = ConfirmDialogState(
                title: String(localized: "update.latest.title", defaultValue: "You're up to date.", bundle: .module),
                message: String(
                    format: String(localized: "update.latest.message_format", defaultValue: "Current version: v%@\nLatest release: v%@", bundle: .module),
                    currentVersion,
                    latestVersion
                ),
                icon: "checkmark.circle.fill",
                iconColor: AppTheme.success,
                primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
            )
            ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }

        case .updateAvailable(let currentVersion, let latest):
            let notes = AppReleaseService.releaseNotesExcerpt(from: latest.body)
            var message = String(
                format: String(localized: "update.available.message_format", defaultValue: "A new version v%@ is available. You are currently using v%@.", bundle: .module),
                latest.version,
                currentVersion
            )
            if let notes, !notes.isEmpty {
                message += "\n\n" + notes
            }

            let dialog = ConfirmDialogState(
                title: String(localized: "update.available.title", defaultValue: "Update Available", bundle: .module),
                message: message,
                icon: "arrow.down.circle.fill",
                iconColor: AppTheme.focus,
                primaryTitle: String(localized: "update.available.open", defaultValue: "Download", bundle: .module),
                secondaryTitle: String(localized: "update.available.later", defaultValue: "Later", bundle: .module)
            )
            ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
                guard let self, result == .primary else {
                    return
                }
                self.openURL(AppReleaseService.preferredDownloadURL(for: latest))
            }
        }
    }

    private func presentUpdateCheckError(_ error: Error) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = error.localizedDescription
            return
        }

        statusMessage = error.localizedDescription
        let dialog = ConfirmDialogState(
            title: String(localized: "update.error.title", defaultValue: "Unable to Check for Updates", bundle: .module),
            message: String(localized: "update.error.message", defaultValue: "Please check your network connection and try again.", bundle: .module),
            icon: "wifi.exclamationmark",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
        )
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }
    }

    private func presentLanguageRestartPrompt() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "settings.language.restart_required", defaultValue: "Restart the app to apply the selected language.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "settings.language.restart_title", defaultValue: "Restart Required", bundle: .module),
            message: String(localized: "settings.language.restart_message", defaultValue: "Restart Codux to apply the selected language.", bundle: .module),
            icon: "globe",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.restart_now", defaultValue: "Restart Now", bundle: .module),
            secondaryTitle: String(localized: "common.later", defaultValue: "Later", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else {
                return
            }
            if result == .primary {
                self.relaunchApplication()
            } else {
                self.statusMessage = String(localized: "settings.language.restart_pending", defaultValue: "Language changes will apply after restart.", bundle: .module)
            }
        }
    }

    private func relaunchApplication() {
        AppDelegate.scheduleRelaunch(at: Bundle.main.bundleURL)
    }

    private func importProject(name: String, path: String, badgeText: String, badgeSymbol: String?, badgeColorHex: String) {
        let normalizedPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedPath.isEmpty else {
            statusMessage = String(localized: "project.path.empty", defaultValue: "Project path cannot be empty.", bundle: .module)
            return
        }

        if let existing = projects.first(where: { $0.path == normalizedPath }) {
            selectedProjectID = existing.id
            statusMessage = String(localized: "project.exists.switched", defaultValue: "Project already exists. Switched to it.", bundle: .module)
            refreshGitState()
            updateGitRemoteSyncPolling()
            refreshAIStatsIfNeeded()
            return
        }

        let project = Project(
            id: UUID(),
            name: name.isEmpty ? URL(fileURLWithPath: normalizedPath).lastPathComponent : name,
            path: normalizedPath,
            shell: appSettings.defaultTerminal.shellPath,
            defaultCommand: "",
            badgeText: badgeText,
            badgeSymbol: badgeSymbol,
            badgeColorHex: badgeColorHex,
            gitDefaultPushRemoteName: nil
        )
        projects.append(project)
        workspaces.append(ProjectWorkspace.sample(projectID: project.id, path: project.path))
        selectedProjectID = project.id
        statusMessage = String(
            format: String(localized: "project.add.success_format", defaultValue: "Added project %@.", bundle: .module),
            project.name
        )
        persist()
        refreshGitState()
        updateGitRemoteSyncPolling()
        refreshAIStatsIfNeeded()
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
