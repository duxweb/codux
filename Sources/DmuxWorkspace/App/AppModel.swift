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

    func i18n(_ key: String, fallback: String) -> String {
        appSettings.language.i18n(key, fallback: fallback)
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
        statusMessage = i18n("app.debug_log.opened", fallback: "Opened debug log.")
    }

    func openSelectedProjectInVSCode() {
        guard let project = selectedProject else {
            statusMessage = i18n("project.none_selected", fallback: "No project selected.")
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.microsoft.VSCode",
            fallbackURL: vscodeOpenURL(for: project.path),
            successMessage: i18n("project.open.vscode.success", fallback: "Opened project in VS Code."),
            failureMessage: i18n("project.open.vscode.failure", fallback: "Unable to find VS Code for this directory.")
        )
    }

    func revealSelectedProjectInFinder() {
        guard let project = selectedProject else {
            statusMessage = i18n("project.none_selected", fallback: "No project selected.")
            return
        }

        revealProjectInFinder(project.id)
    }

    func revealProjectInFinder(_ projectID: UUID) {
        guard let project = projects.first(where: { $0.id == projectID }) else {
            statusMessage = i18n("project.not_found", fallback: "Project not found.")
            return
        }

        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: project.path, isDirectory: true)])
        statusMessage = i18n("project.reveal.finder.success", fallback: "Revealed project in Finder.")
    }

    func openProjectDirectory(_ projectID: UUID) {
        guard let project = projects.first(where: { $0.id == projectID }) else {
            statusMessage = i18n("project.not_found", fallback: "Project not found.")
            return
        }

        NSWorkspace.shared.open(URL(fileURLWithPath: project.path, isDirectory: true))
        statusMessage = i18n("project.open.folder.success", fallback: "Opened project folder.")
    }

    func openSelectedProjectInTerminal() {
        guard let project = selectedProject else {
            statusMessage = i18n("project.none_selected", fallback: "No project selected.")
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.apple.Terminal",
            successMessage: i18n("project.open.terminal.success", fallback: "Opened project in Terminal."),
            failureMessage: i18n("project.open.terminal.failure", fallback: "Terminal not found.")
        )
    }

    func openSelectedProjectInITerm2() {
        guard let project = selectedProject else {
            statusMessage = i18n("project.none_selected", fallback: "No project selected.")
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.googlecode.iterm2",
            successMessage: i18n("project.open.iterm2.success", fallback: "Opened project in iTerm2."),
            failureMessage: i18n("project.open.iterm2.failure", fallback: "iTerm2 not found.")
        )
    }

    func openSelectedProjectInGhostty() {
        guard let project = selectedProject else {
            statusMessage = i18n("project.none_selected", fallback: "No project selected.")
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.mitchellh.ghostty",
            successMessage: i18n("project.open.ghostty.success", fallback: "Opened project in Ghostty."),
            failureMessage: i18n("project.open.ghostty.failure", fallback: "Ghostty not found.")
        )
    }

    func openSelectedProjectInXcode() {
        guard let project = selectedProject else {
            statusMessage = i18n("project.none_selected", fallback: "No project selected.")
            return
        }
        openSelectedProjectInApplication(
            project,
            bundleIdentifier: "com.apple.dt.Xcode",
            successMessage: i18n("project.open.xcode.success", fallback: "Opened project in Xcode."),
            failureMessage: i18n("project.open.xcode.failure", fallback: "Xcode not found.")
        )
    }

    func addProject() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let dialog = ProjectEditorDialogState(
            title: i18n("project.create.title", fallback: "Create Project"),
            message: i18n("project.create.message", fallback: "Fill in the project name, directory, color, and icon."),
            confirmTitle: i18n("common.create", fallback: "Create"),
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
        panel.title = i18n("project.open_folder.title", fallback: "Open Folder")
        panel.prompt = i18n("project.open_folder.prompt", fallback: "Open")
        panel.message = i18n("project.open_folder.message", fallback: "Choose a project folder to import.")

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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let project = projects[index]
        let dialog = ProjectEditorDialogState(
            title: i18n("project.edit.title", fallback: "Edit Project"),
            message: i18n("project.edit.message", fallback: "Update the project name, directory, color, and icon."),
            confirmTitle: i18n("common.save", fallback: "Save"),
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

            self.statusMessage = self.i18n("project.update.success_format", fallback: "Updated project \(updatedProject.name).")
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        let dialog = ConfirmDialogState(
            title: i18n("project.remove.title", fallback: "Remove Project"),
            message: String(
                format: i18n(
                    "project.remove.confirm_format",
                    fallback: "Are you sure you want to remove project %@? Files on disk will not be deleted."
                ),
                project.name
            ),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("common.remove", fallback: "Remove"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.projects.removeAll { $0.id == projectID }
            self.workspaces.removeAll { $0.projectID == projectID }
            if self.selectedProjectID == projectID {
                self.selectedProjectID = self.projects.first?.id
            }
            self.statusMessage = self.i18n("project.remove.success_format", fallback: "Removed project \(project.name).")
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
            format: i18n("project.close.success_format", fallback: "Closed project %@."),
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let dialog = ConfirmDialogState(
            title: i18n("workspace.close_all_projects.title", fallback: "Close All Projects"),
            message: i18n("workspace.close_all_projects.message", fallback: "Are you sure you want to close all projects in the current workspace? Files on disk will not be deleted."),
            icon: "xmark.rectangle.stack",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("workspace.close_all_projects.confirm", fallback: "Close All"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.projects.removeAll()
            self.workspaces.removeAll()
            self.selectedProjectID = nil
            self.rightPanel = nil
            self.statusMessage = self.i18n("workspace.close_all_projects.success", fallback: "Closed all projects.")
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
            statusMessage = i18n("ai.session.open.unsupported", fallback: "This session does not support opening.")
            return
        }

        if tryReuseSelectedTopTerminalForCommand(command) {
            statusMessage = i18n("ai.session.open.current_success", fallback: "Opened session in the current terminal.")
            return
        }

        guard let newSessionID = createSplitTerminal(command: command, axis: .horizontal) else {
            statusMessage = i18n("workspace.split.create_failed", fallback: "Unable to create a new split pane.")
            return
        }
        terminalFocusRequestID = newSessionID
        statusMessage = i18n("ai.session.open.split_success", fallback: "Opened session in a new split pane.")
    }

    func renameAISession(_ session: AISessionSummary) {
        let capabilities = aiSessionCapabilities(for: session)
        guard capabilities.canRename else {
            statusMessage = i18n("ai.session.rename.unsupported", fallback: "This session does not support renaming.")
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let dialog = GitInputDialogState(
            kind: .renameAISession,
            title: i18n("ai.session.rename.title", fallback: "Rename Session"),
            message: i18n("ai.session.rename.message", fallback: "Directly update the local session title for the corresponding AI CLI."),
            placeholder: i18n("ai.session.rename.placeholder", fallback: "Enter a New Session Title"),
            confirmTitle: i18n("common.save", fallback: "Save"),
            value: session.sessionTitle,
            isMultiline: false
        )
        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] value in
            guard let self, let value else {
                return
            }
            let title = value.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !title.isEmpty else {
                self.statusMessage = self.i18n("ai.session.rename.empty", fallback: "Session title cannot be empty.")
                return
            }

            do {
                try self.toolDriverFactory.renameSession(session, to: title)
                self.aiStatsStore.renameSessionOptimistically(projectID: session.projectID, sessionID: session.sessionID, title: title)
                self.invalidateAISessionCachesAndRefresh()
                self.statusMessage = self.i18n("ai.session.rename.success", fallback: "Renamed session.")
            } catch {
                self.statusMessage = error.localizedDescription
            }
        }
    }

    func removeAISession(_ session: AISessionSummary) {
        let capabilities = aiSessionCapabilities(for: session)
        guard capabilities.canRemove else {
            statusMessage = i18n("ai.session.remove.unsupported", fallback: "This session does not support removal.")
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let dialog = ConfirmDialogState(
            title: i18n("ai.session.remove.title", fallback: "Remove Session"),
            message: i18n("ai.session.remove.confirm", fallback: "This will directly modify the local session storage for the corresponding AI CLI. Continue?"),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("common.remove", fallback: "Remove"),
            primaryTint: AppTheme.warning,
            secondaryTitle: nil,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else {
                return
            }

            do {
                try self.toolDriverFactory.removeSession(session)
                self.aiStatsStore.removeSessionOptimistically(projectID: session.projectID, sessionID: session.sessionID)
                self.invalidateAISessionCachesAndRefresh()
                self.statusMessage = self.i18n("ai.session.remove.success", fallback: "Removed session.")
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
                ? i18n("workspace.bottom_split.created", fallback: "Created the bottom split area.")
                : i18n("workspace.bottom_tab.created", fallback: "Added a new bottom tab.")
        }
    }

    func splitSelectedPane(axis: PaneAxis) {
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            workspace.sessions.append(session)

            switch axis {
            case .horizontal:
                if workspace.addTopSession(session.id) {
                    statusMessage = i18n("workspace.top_pane.horizontal_created", fallback: "Added a horizontal pane.")
                } else {
                    workspace.sessions.removeAll(where: { $0.id == session.id })
                    statusMessage = String(
                        format: i18n("workspace.top_pane.limit_format", fallback: "The top row supports up to %@ panes."),
                        "\(ProjectWorkspace.maxTopPanes)"
                    )
                }
            case .vertical:
                workspace.addBottomTab(session.id)
                statusMessage = workspace.bottomTabSessionIDs.count == 1
                    ? i18n("workspace.bottom_split.created", fallback: "Created the bottom split area.")
                    : i18n("workspace.bottom_tab.additional", fallback: "Added a tab to the bottom split area.")
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
        statusMessage = entries.count > 1 ? self.i18n("git.files.copy_selected_paths.success", fallback: "Copied selected file paths.") : self.i18n("git.files.copy_path.success", fallback: "Copied file path.")
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
                    self.statusMessage = paths.count > 1 ? self.i18n("git.ignore.added", fallback: "Added to .gitignore.") : self.i18n("git.ignore.added", fallback: "Added to .gitignore.")
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
        let currentBranch = gitState?.branch ?? i18n("git.branch.current_label", fallback: "Current Branch")
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let dialog = ConfirmDialogState(
            title: i18n("git.branch.merge.title", fallback: "Merge Branch"),
            message: String(
                format: i18n("git.branch.merge.message_format", fallback: "Merge %@ into %@."),
                branch,
                currentBranch
            ),
            icon: "arrow.merge",
            iconColor: AppTheme.focus,
            primaryTitle: i18n("common.merge", fallback: "Merge"),
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
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
                        self.statusMessage = self.i18n("git.branch.merge.success_format", fallback: "Merged branch \(branch).")
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
            statusMessage = i18n("git.main_branch.not_found", fallback: "Main branch not found.")
            return
        }

        if gitState?.branch == mainBranch {
            statusMessage = i18n("git.main_branch.already_on", fallback: "Already on the main branch.")
            return
        }

        let currentBranch = gitState?.branch ?? i18n("git.branch.current_label", fallback: "Current Branch")
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        let dialog = ConfirmDialogState(
            title: i18n("git.main_branch.sync.title", fallback: "Sync Main Branch"),
            message: String(
                format: i18n("git.main_branch.sync.message_format", fallback: "Merge the latest changes from %@ into %@."),
                mainBranch,
                currentBranch
            ),
            icon: "arrow.triangle.merge",
            iconColor: AppTheme.focus,
            primaryTitle: i18n("common.sync", fallback: "Sync"),
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
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
                statusMessage = i18n("terminal.keep_one_open", fallback: "At least one terminal must remain open.")
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
            statusMessage = i18n("terminal.closed", fallback: "Closed terminal.")
        }
    }

    func confirmCloseSelectedSession() {
        guard let sessionID = selectedSessionID else {
            statusMessage = i18n("terminal.none_selected", fallback: "No terminal selected.")
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        guard let workspace = selectedWorkspace else {
            statusMessage = i18n("workspace.not_found", fallback: "Current workspace not found.")
            return
        }
        guard workspace.sessions.count > 1 else {
            statusMessage = i18n("terminal.keep_one_open", fallback: "At least one terminal must remain open.")
            return
        }

        let dialog = ConfirmDialogState(
            title: i18n("workspace.close_current_split.title", fallback: "Close Current Split"),
            message: i18n("workspace.close_current_split.message", fallback: "Are you sure you want to close the current split or tab?"),
            icon: "xmark.rectangle.portrait",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("common.close", fallback: "Close"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
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
            ? i18n("project.default_command.cleared", fallback: "Cleared default startup command.")
            : i18n("project.default_command.updated", fallback: "Updated default startup command.")
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        let dialog = GitInputDialogState(
            kind: .cloneRepository,
            title: i18n("git.empty.clone_remote_repository", fallback: "Clone Remote Repository"),
            message: i18n("git.clone.message", fallback: "Enter a remote repository URL to clone it into the current project directory."),
            placeholder: "https://github.com/foo/bar.git",
            confirmTitle: i18n("git.clone.start", fallback: "Start Clone"),
            value: "",
            isMultiline: false
        )
        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] value in
            guard let self, let value else { return }
            let remoteURL = value.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !remoteURL.isEmpty else {
                self.statusMessage = self.i18n("git.remote.url_required", fallback: "Please enter a remote repository URL.")
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
        statusMessage = i18n("git.commit.hash.copied", fallback: "Copied commit hash.")
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        let dialog = GitInputDialogState(
            kind: .createBranchFromCommit(commit.hash),
            title: i18n("git.branch.create_from_commit.title", fallback: "Create Branch from Commit"),
            message: i18n("git.branch.new.message", fallback: "Enter a new branch name."),
            placeholder: "feature/from-commit",
            confirmTitle: i18n("common.create", fallback: "Create"),
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
            title: forceRemote ? i18n("git.history.restore_remote", fallback: "Restore This Revision Remotely") : i18n("git.history.restore_local", fallback: "Restore This Revision Locally"),
            message: forceRemote
                ? i18n("git.history.restore_remote.message", fallback: "Reset the current branch to this revision and overwrite the remote branch.")
                : i18n("git.history.restore_local.message", fallback: "Reset the current branch to this revision locally only."),
            icon: forceRemote ? "arrow.uturn.backward.circle.fill" : "clock.arrow.circlepath",
            iconColor: AppTheme.warning,
            primaryTitle: forceRemote ? i18n("git.history.restore_remote.action", fallback: "Remote Restore") : i18n("git.history.restore_local.action", fallback: "Local Restore"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        guard let project = selectedProject else { return }

        let nameDialog = GitInputDialogState(
            kind: .cloneRepository,
            title: i18n("git.remote.add", fallback: "Add Remote"),
            message: i18n("git.remote.add.name_message", fallback: "Enter the remote name first, such as origin or upstream."),
            placeholder: i18n("git.remote.name", fallback: "Remote Name"),
            confirmTitle: i18n("common.next", fallback: "Next"),
            value: "",
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: nameDialog, parentWindow: parentWindow) { [weak self] nameValue in
            guard let self else { return }
            let name = nameValue?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !name.isEmpty else { return }

            let urlDialog = GitInputDialogState(
                kind: .cloneRepository,
                title: i18n("git.remote.add", fallback: "Add Remote"),
                message: i18n("git.remote.add.url_message", fallback: "Enter the remote URL."),
                placeholder: "https://github.com/org/repo.git",
                confirmTitle: i18n("common.add", fallback: "Add"),
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let dialog = ConfirmDialogState(
            title: i18n("git.remote.remove", fallback: "Remove Remote"),
            message: String(format: i18n("git.remote.remove.confirm_format", fallback: "Are you sure you want to remove remote %@?"), remote.name),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("common.remove", fallback: "Remove"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
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
        statusMessage = String(format: i18n("git.remote.default_push.set_format", fallback: "Set %@ as the default push remote."), remote.name)
    }

    func clearDefaultPushRemote() {
        guard let project = selectedProject,
              let index = projects.firstIndex(where: { $0.id == project.id }) else { return }
        projects[index].gitDefaultPushRemoteName = nil
        persist()
        statusMessage = i18n("git.remote.default_push.cleared", fallback: "Restored the Git default push target.")
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
            statusMessage = i18n("git.remote.matching_not_found", fallback: "Matching remote was not found.")
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
            title: i18n("git.remote.force_push", fallback: "Force Push"),
            message: i18n("git.remote.force_push.message", fallback: "Overwrite the current remote branch. Only use this when you intentionally want to rewrite remote history."),
            icon: "exclamationmark.arrow.trianglehead.2.clockwise.rotate.90",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("git.remote.force_push", fallback: "Force Push"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
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
            title: i18n("git.history.undo_last_commit", fallback: "Undo Last Commit"),
            message: headCommitPushed
                ? i18n("git.history.undo_last_commit.remote_notice", fallback: "Undo the last commit but keep the file changes and staging state. This commit may already be on the remote, so you might need to force push later.")
                : i18n("git.history.undo_last_commit.local_notice", fallback: "Undo the last commit but keep the file changes and staging state."),
            icon: "arrow.uturn.backward",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("common.undo", fallback: "Undo"),
            primaryTint: AppTheme.warning,
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.gitStore.undoLastCommit(headCommitPushed: headCommitPushed, project: project, onStatus: { self.statusMessage = $0 }, onRewriteWarning: {
                self.presentPostCommitRewriteAlert(actionTitle: self.i18n("git.history.undo_last_commit", fallback: "Undo Last Commit"), repositoryPath: path)
            })
        }
    }

    func editLastGitCommitMessage() {
        guard let project = selectedProject else { return }
        let path = project.path
        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        let currentMessage = (try? gitService.lastCommitMessage(at: path)) ?? ""
        let headCommitPushed = (try? gitService.isHeadCommitPushed(at: path)) ?? false

        let dialog = GitInputDialogState(
            kind: .editLastCommitMessage(headCommitPushed: headCommitPushed),
            title: i18n("git.history.edit_last_commit_message", fallback: "Edit Last Commit Message"),
            message: headCommitPushed
                ? i18n("git.commit.edit_last_message.remote_notice", fallback: "Only change the latest commit message. This commit may already be on the remote, so you might need to force push after editing it.")
                : i18n("git.commit.edit_last_message.notice", fallback: "Only change the latest commit message."),
            placeholder: i18n("git.commit.edit_last_message.placeholder", fallback: "Enter a new commit message"),
            confirmTitle: i18n("common.save", fallback: "Save"),
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }
        let dialog = GitInputDialogState(
            kind: .createBranch,
            title: i18n("git.branch.new", fallback: "New Branch"),
            message: i18n("git.branch.new.message", fallback: "Enter a new branch name."),
            placeholder: "feature/xxx",
            confirmTitle: i18n("common.create", fallback: "Create"),
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
            self.presentPostCommitRewriteAlert(actionTitle: self.i18n("git.history.edit_last_commit_message", fallback: "Edit Last Commit Message"), repositoryPath: path)
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
        gitStore.discardEntries(entries, project: project, successMessage: i18n("git.discard.selected.success", fallback: "Discarded selected changes.")) { self.statusMessage = $0 }
    }

    func stageEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.filter { $0.kind != .staged }.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.stagePaths(paths, project: project, successMessage: i18n("git.stage.section.success", fallback: "Staged files in this section.")) { self.statusMessage = $0 }
    }

    func unstageEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.filter { $0.kind == .staged }.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.unstagePaths(paths, project: project, successMessage: i18n("git.unstage.section.success", fallback: "Unstaged files in this section.")) { self.statusMessage = $0 }
    }

    func discardEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        guard !entries.isEmpty else { return }
        gitStore.discardEntries(entries, project: project, successMessage: i18n("git.discard.section.success", fallback: "Discarded changes in this section.")) { self.statusMessage = $0 }
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
        gitStore.stagePaths(paths, project: project, successMessage: i18n("git.stage.selected.success", fallback: "Staged selected files.")) { self.statusMessage = $0 }
    }

    func unstageSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind == .staged }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.unstagePaths(paths, project: project, successMessage: i18n("git.unstage.selected.success", fallback: "Unstaged selected files.")) { self.statusMessage = $0 }
    }

    func stage(_ entry: GitFileEntry) {
        guard let project = selectedProject else {
            return
        }
        gitStore.stagePaths([entry.path], project: project, successMessage: String(format: i18n("git.stage.file.success_format", fallback: "Staged %@."), entry.path)) { self.statusMessage = $0 }
    }

    func unstage(_ entry: GitFileEntry) {
        guard let project = selectedProject else {
            return
        }
        gitStore.unstagePaths([entry.path], project: project, successMessage: String(format: i18n("git.unstage.file.success_format", fallback: "Unstaged %@."), entry.path)) { self.statusMessage = $0 }
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
            statusMessage = i18n("git.commit.message.empty", fallback: "Commit message cannot be empty.")
            return
        }

        gitStore.commit(message: trimmed, action: action, project: project, onStatus: { self.statusMessage = $0 }, onSuccess: {
            self.commitMessage = ""
            self.selectedGitEntry = nil
            self.selectedGitEntryIDs.removeAll()
            self.gitDiffText = self.i18n("git.commit.completed", fallback: "Commit completed.")
        })
    }

    func generateCommitMessage() {
        guard let gitState else {
            statusMessage = i18n("git.commit.generate.unavailable", fallback: "There are no changes available to generate a commit message.")
            return
        }

        let fileNames = Array(Set((gitState.staged + gitState.changes + gitState.untracked).map(\.path))).sorted()
        guard !fileNames.isEmpty else {
            statusMessage = i18n("git.commit.generate.unavailable", fallback: "There are no changes available to generate a commit message.")
            return
        }

        isGeneratingCommitMessage = true
        let summary = fileNames.prefix(3).joined(separator: ", ")
        let suffix = fileNames.count > 3
            ? String(format: i18n("git.commit.generate.more_files_format", fallback: " and %@ more files"), "\(fileNames.count)")
            : ""
        commitMessage = String(format: i18n("git.commit.generate.summary_format", fallback: "Update %@%@ Git workspace interaction"), summary, suffix)
        isGeneratingCommitMessage = false
        statusMessage = i18n("git.commit.generate.success", fallback: "Generated commit message.")
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
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            completion(nil)
            return
        }

        let dialog = GitCredentialDialogState(
            title: i18n("git.credentials.title", fallback: "Git Credentials Required"),
            message: i18n("git.credentials.message", fallback: "Remote access requires authentication. Enter your username and password or token to retry."),
            confirmTitle: i18n("common.continue", fallback: "Continue"),
            cancelTitle: i18n("common.cancel", fallback: "Cancel")
        )

        GitCredentialDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] credential in
            guard let self else {
                completion(nil)
                return
            }
            guard let credential else {
                self.statusMessage = self.i18n("git.auth.cancelled", fallback: "Git authentication was cancelled.")
                completion(nil)
                return
            }

            guard !credential.username.isEmpty, !credential.password.isEmpty else {
                self.statusMessage = self.i18n("git.auth.credentials_required", fallback: "Username and password or token cannot be empty.")
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
            message: i18n("git.history.rewrite_warning.message", fallback: "Local history has changed. If the remote still contains the old commits, you may need to force push to sync it."),
            icon: "arrow.trianglehead.2.clockwise.rotate.90",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("git.history.rewrite_warning.defer", fallback: "Handle Later"),
            secondaryTitle: i18n("git.history.rewrite_warning.force_push", fallback: "Force Push Now")
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
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
            title: i18n("git.sync.conflict.title", fallback: "Sync Conflict"),
            message: i18n("git.sync.conflict.message", fallback: "A conflict occurred while pulling. You can resolve it manually first, or force push the current branch directly."),
            icon: "exclamationmark.triangle.fill",
            iconColor: AppTheme.warning,
            primaryTitle: i18n("git.sync.conflict.resolve_manually", fallback: "Resolve Manually"),
            secondaryTitle: i18n("git.remote.force_push", fallback: "Force Push")
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = i18n("app.window.main_missing", fallback: "Unable to find the main window.")
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else { return }
            if result == .secondary {
                self.forcePushAfterConflict(at: repositoryPath)
            } else {
                self.statusMessage = self.i18n("git.remote.pull_conflicts", fallback: "Pull resulted in conflicts. Resolve them manually first.")
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
        appSettings.themeMode = mode
        applyThemeMode()
        persist()
    }

    func updateLanguage(_ language: AppLanguage) {
        appSettings.language = language
        persist()
        aiStatsStore.refreshLocalizedStatusTexts()
    }

    func updateAppIconStyle(_ style: AppIconStyle) {
        appSettings.iconStyle = style
        applyAppIcon()
        persist()
    }

    func updateTerminalBackgroundPreset(_ preset: AppTerminalBackgroundPreset) {
        appSettings.terminalBackgroundPreset = preset
        persist()
    }

    func updateDefaultTerminal(_ terminal: AppTerminalProfile) {
        let previousShell = appSettings.defaultTerminal.shellPath
        appSettings.defaultTerminal = terminal
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
        appSettings.showsDockBadge = enabled
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
        appSettings.gitAutoRefreshInterval = interval
        gitStore.configureRemoteSyncInterval(interval)
        persist()
    }

    func updateAIAutomaticRefreshInterval(_ interval: TimeInterval) {
        appSettings.aiAutoRefreshInterval = interval
        aiStatsStore.configureIntervals(
            automatic: interval,
            background: appSettings.aiBackgroundRefreshInterval
        )
        persist()
    }

    func updateAIBackgroundRefreshInterval(_ interval: TimeInterval) {
        appSettings.aiBackgroundRefreshInterval = interval
        aiStatsStore.configureIntervals(
            automatic: appSettings.aiAutoRefreshInterval,
            background: interval
        )
        persist()
    }

    func updateDeveloperNotificationTestButtonEnabled(_ enabled: Bool) {
        appSettings.developer.showsNotificationTestButton = enabled
        persist()
    }

    func updateDeveloperDebugLogButtonEnabled(_ enabled: Bool) {
        appSettings.developer.showsDebugLogButton = enabled
        persist()
    }

    func openURL(_ url: URL) {
        NSWorkspace.shared.open(url)
    }

    func checkForUpdates() {
        openURL(AppSupportLinks.releases)
    }

    var appDisplayName: String {
        if let name = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String, !name.isEmpty {
            return name
        }
        if let name = Bundle.main.object(forInfoDictionaryKey: kCFBundleNameKey as String) as? String, !name.isEmpty {
            return name
        }
        return "dmux"
    }

    var appVersionDescription: String {
        let version = (Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String) ?? "0.1.0"
        let build = (Bundle.main.object(forInfoDictionaryKey: kCFBundleVersionKey as String) as? String) ?? "dev"
        return "v\(version) (\(build))"
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
        NSApplication.shared.applicationIconImage = AppIconRenderer.image(for: appSettings.iconStyle)
        updateDockBadge()
    }

    private func updateDockBadge() {
        guard isSystemUIReady else {
            return
        }
        NSApp.dockTile.contentView = nil
        guard appSettings.showsDockBadge else {
            NSApp.dockTile.badgeLabel = nil
            NSApp.dockTile.display()
            return
        }

        let completedCount = activityByProjectID.values.reduce(into: 0) { partial, phase in
            if case .completed = phase {
                partial += 1
            }
        }
        NSApp.dockTile.badgeLabel = completedCount > 0 ? "\(completedCount)" : nil
        NSApp.dockTile.display()
    }

    private func importProject(name: String, path: String, badgeText: String, badgeSymbol: String?, badgeColorHex: String) {
        let normalizedPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedPath.isEmpty else {
            statusMessage = i18n("project.path.empty", fallback: "Project path cannot be empty.")
            return
        }

        if let existing = projects.first(where: { $0.path == normalizedPath }) {
            selectedProjectID = existing.id
            statusMessage = i18n("project.exists.switched", fallback: "Project already exists. Switched to it.")
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
            format: i18n("project.add.success_format", fallback: "Added project %@."),
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
            return appI18n("git.commit.action", fallback: "Commit")
        case .commitAndPush:
            return appI18n("git.commit.action_push", fallback: "Commit and Push")
        case .commitAndSync:
            return appI18n("git.commit.action_sync", fallback: "Commit and Sync")
        }
    }

    var successMessage: String {
        switch self {
        case .commit:
            return appI18n("git.commit.success", fallback: "Committed staged changes.")
        case .commitAndPush:
            return appI18n("git.commit.push_success", fallback: "Committed and pushed changes.")
        case .commitAndSync:
            return appI18n("git.commit.sync_success", fallback: "Committed and synced changes.")
        }
    }
}
