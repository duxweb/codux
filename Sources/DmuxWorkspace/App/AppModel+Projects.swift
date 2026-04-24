import AppKit
import Foundation

extension AppModel {
    func selectProject(_ projectID: UUID) {
        dismissCompletionPresentationIfNeeded(
            projectID: projectID,
            reason: "select-project"
        )
        updateSelectedProjectID(projectID, source: "selectProject")
        clearTerminalFocusOutsideSelectedProject()
        restoreSelectedTerminalFocusIfNeeded()
        DispatchQueue.main.async { [weak self] in
            guard self?.selectedProjectID == projectID else {
                return
            }
            self?.clearTerminalFocusOutsideSelectedProject()
            self?.restoreSelectedTerminalFocusIfNeeded()
        }
        restoreCachedGitPanelIfAvailable(for: projectID)
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

    func openRuntimeLog() {
        debugLog.log("app", "open runtime log")
        debugLog.openRuntimeLogInSystemViewer()
        statusMessage = String(localized: "app.runtime_log.opened", defaultValue: "Opened runtime log.", bundle: .module)
    }

    func openLiveLog() {
        debugLog.log("app", "open live log")
        debugLog.openLiveLogInSystemViewer()
        statusMessage = String(localized: "app.live_log.opened", defaultValue: "Opened live log.", bundle: .module)
    }

    func exportDiagnosticsArchive() {
        do {
            let destinationURL = try diagnosticsExportService.requestExportDestination(appDisplayName: appDisplayName)
            statusMessage = String(localized: "diagnostics.export.running", defaultValue: "Exporting diagnostics…", bundle: .module)

            let appDisplayName = self.appDisplayName
            let appVersionDescription = self.appVersionDescription
            Task.detached(priority: .userInitiated) {
                do {
                    let archiveURL = try AppDiagnosticsExportService().exportArchive(
                        to: destinationURL,
                        appDisplayName: appDisplayName,
                        appVersion: appVersionDescription
                    )
                    await MainActor.run {
                        NSWorkspace.shared.activateFileViewerSelecting([archiveURL])
                        self.statusMessage = String(
                            format: String(localized: "diagnostics.export.success_format", defaultValue: "Exported diagnostics to %@.", bundle: .module),
                            archiveURL.lastPathComponent
                        )
                    }
                } catch {
                    await MainActor.run {
                        self.debugLog.log("diagnostics-export", "failed error=\(error.localizedDescription)")
                        self.presentDiagnosticsExportError(error)
                    }
                }
            }
        } catch is CancellationError {
            return
        } catch {
            debugLog.log("diagnostics-export", "failed error=\(error.localizedDescription)")
            presentDiagnosticsExportError(error)
        }
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
            debugLog.log("project-create", "open-dialog failed reason=main-window-missing")
            return
        }
        debugLog.log("project-create", "open-dialog")

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
            guard let self else { return }
            guard let result else {
                self.debugLog.log("project-create", "dialog cancelled")
                return
            }
            self.debugLog.log(
                "project-create",
                "dialog confirmed name=\(result.name) path=\(result.path) symbol=\(result.badgeSymbol ?? "nil") color=\(result.badgeColorHex)"
            )
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
            guard let self else {
                return
            }
            guard response == .OK, let url = panel.url else {
                self.debugLog.log("project-create", "open-folder cancelled")
                return
            }
            self.debugLog.log("project-create", "open-folder selected path=\(url.path)")
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
            self.petStore.forgetProjectBaseline(projectID)
            self.projects.removeAll { $0.id == projectID }
            self.workspaces.removeAll { $0.projectID == projectID }
            if self.selectedProjectID == projectID {
                self.updateSelectedProjectID(self.projects.first?.id, source: "removeProject")
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

        petStore.forgetProjectBaseline(project.id)
        projects.removeAll { $0.id == project.id }
        workspaces.removeAll { $0.projectID == project.id }
        updateSelectedProjectID(projects.first?.id, source: "closeCurrentProject")
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
            self.petStore.forgetProjectBaselines(self.projects.map(\.id))
            self.projects.removeAll()
            self.workspaces.removeAll()
            self.updateSelectedProjectID(nil, source: "closeAllProjects")
            self.rightPanel = nil
            self.statusMessage = String(localized: "workspace.close_all_projects.success", defaultValue: "Closed all projects.", bundle: .module)
            self.persist()
            self.refreshGitState()
            self.updateGitRemoteSyncPolling()
            self.refreshAIStatsIfNeeded()
        }
    }

    func toggleRightPanel(_ kind: RightPanelKind) {
        let wasShowingGitPanel = rightPanel == .git
        if rightPanel == kind {
            rightPanel = nil
        } else {
            rightPanel = kind
        }
        if rightPanel == .git && wasShowingGitPanel == false {
            refreshGitState(presentation: .preserveVisibleState)
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
            debugLog.log(
                "ai-session-open",
                "unsupported session=\(session.sessionID.uuidString) tool=\(session.lastTool ?? "nil") external=\(session.externalSessionID ?? "nil") reason=missing-command"
            )
            statusMessage = String(localized: "ai.session.open.unsupported", defaultValue: "This session does not support opening.", bundle: .module)
            return
        }
        guard let tool = session.lastTool?.trimmingCharacters(in: .whitespacesAndNewlines),
              !tool.isEmpty else {
            debugLog.log(
                "ai-session-open",
                "unsupported session=\(session.sessionID.uuidString) tool=nil external=\(session.externalSessionID ?? "nil") reason=missing-tool"
            )
            statusMessage = String(localized: "ai.session.open.unsupported", defaultValue: "This session does not support opening.", bundle: .module)
            return
        }
        guard let externalSessionID = session.externalSessionID, !externalSessionID.isEmpty else {
            debugLog.log(
                "ai-session-open",
                "unsupported session=\(session.sessionID.uuidString) tool=\(tool) external=nil reason=missing-external"
            )
            statusMessage = String(localized: "ai.session.identifier.missing", defaultValue: "Missing session identifier.", bundle: .module)
            return
        }
        debugLog.log(
            "ai-session-open",
            "request source=indexed session=\(session.sessionID.uuidString) selected=\(selectedSessionID?.uuidString ?? "nil") tool=\(tool) external=\(externalSessionID) command=\(command)"
        )

        if let selectedSessionID = selectedSessionID {
            aiSessionStore.registerExpectedLogicalSession(
                terminalID: selectedSessionID,
                tool: tool,
                aiSessionID: externalSessionID
            )
        }
        if tryReuseSelectedTopTerminalForCommand(command) {
            debugLog.log(
                "ai-session-open",
                "reuse-current selected=\(selectedSessionID?.uuidString ?? "nil") tool=\(tool) external=\(externalSessionID) command=\(command)"
            )
            statusMessage = String(localized: "ai.session.open.current_success", defaultValue: "Opened session in the current terminal.", bundle: .module)
            return
        }
        if let selectedSessionID = selectedSessionID {
            aiSessionStore.clearExpectedLogicalSession(terminalID: selectedSessionID)
        }

        guard let newSessionID = createSplitTerminal(command: command, axis: .horizontal) else {
            statusMessage = String(localized: "workspace.split.create_failed", defaultValue: "Unable to create a new split pane.", bundle: .module)
            return
        }
        aiSessionStore.registerExpectedLogicalSession(
            terminalID: newSessionID,
            tool: tool,
            aiSessionID: externalSessionID
        )
        debugLog.log(
            "ai-session-open",
            "create-split newSession=\(newSessionID.uuidString) tool=\(tool) external=\(externalSessionID) command=\(command)"
        )
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

    func invalidateAISessionCachesAndRefresh() {
        guard let project = selectedProject else {
            return
        }
        aiStatsStore.invalidateProjectCaches(project: project)
        refreshCurrentAIIndexing()
    }

    func updateSelectedProjectID(_ projectID: UUID?, source: String) {
        selectedProjectIDChangeSource = source
        selectedProjectID = projectID
        if selectedProjectID == projectID {
            selectedProjectIDChangeSource = "unspecified"
        }
    }

    private func openSelectedProjectInApplication(_ project: Project, bundleIdentifier: String, fallbackURL: URL? = nil, successMessage: String, failureMessage: String) {
        let projectURL = URL(fileURLWithPath: project.path, isDirectory: true)
        if NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) != nil {
            Task { [weak self, projectURL, bundleIdentifier, fallbackURL, successMessage, failureMessage] in
                let didOpen = await Task.detached(priority: .userInitiated) {
                    Self.openProjectURL(projectURL, withBundleIdentifier: bundleIdentifier)
                }.value

                guard let self else { return }
                if didOpen {
                    self.statusMessage = successMessage
                } else if let fallbackURL, NSWorkspace.shared.open(fallbackURL) {
                    self.statusMessage = successMessage
                } else {
                    self.statusMessage = failureMessage
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

    private nonisolated static func openProjectURL(_ projectURL: URL, withBundleIdentifier bundleIdentifier: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        process.arguments = ["-b", bundleIdentifier, projectURL.path]

        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
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

    private func importProject(name: String, path: String, badgeText: String, badgeSymbol: String?, badgeColorHex: String) {
        let rawPath = (path as NSString).expandingTildeInPath.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedPath = URL(fileURLWithPath: rawPath).standardizedFileURL.path
        debugLog.log(
            "project-create",
            "import start rawPath=\(path) normalizedPath=\(normalizedPath) name=\(name) symbol=\(badgeSymbol ?? "nil") color=\(badgeColorHex)"
        )
        guard !normalizedPath.isEmpty else {
            statusMessage = String(localized: "project.path.empty", defaultValue: "Project path cannot be empty.", bundle: .module)
            debugLog.log("project-create", "import failed reason=empty-path")
            return
        }

        var isDirectory: ObjCBool = false
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: normalizedPath, isDirectory: &isDirectory) {
            guard isDirectory.boolValue else {
                statusMessage = String(localized: "project.path.invalid_directory", defaultValue: "Project path must be a folder.", bundle: .module)
                debugLog.log("project-create", "import failed reason=path-is-file path=\(normalizedPath)")
                return
            }
            debugLog.log("project-create", "path exists path=\(normalizedPath)")
        } else {
            do {
                debugLog.log("project-create", "creating directory path=\(normalizedPath)")
                try fileManager.createDirectory(atPath: normalizedPath, withIntermediateDirectories: true, attributes: nil)
                debugLog.log("project-create", "created directory path=\(normalizedPath)")
            } catch {
                statusMessage = String(
                    format: String(localized: "project.path.create_failed", defaultValue: "Failed to create project folder: %@.", bundle: .module),
                    error.localizedDescription
                )
                debugLog.log("project-create", "import failed reason=create-directory error=\(error.localizedDescription) path=\(normalizedPath)")
                return
            }
        }

        if let existing = projects.first(where: { URL(fileURLWithPath: $0.path).standardizedFileURL.path == normalizedPath }) {
            updateSelectedProjectID(existing.id, source: "importProject.existing")
            statusMessage = String(localized: "project.exists.switched", defaultValue: "Project already exists. Switched to it.", bundle: .module)
            debugLog.log("project-create", "switched-to-existing projectID=\(existing.id.uuidString) path=\(normalizedPath)")
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
        debugLog.log("project-create", "project-created id=\(project.id.uuidString) name=\(project.name) path=\(project.path)")
        projects.append(project)
        workspaces.append(ProjectWorkspace.sample(projectID: project.id, path: project.path))
        updateSelectedProjectID(project.id, source: "importProject.created")
        statusMessage = String(
            format: String(localized: "project.add.success_format", defaultValue: "Added project %@.", bundle: .module),
            project.name
        )
        debugLog.log("project-create", "persist begin projectID=\(project.id.uuidString)")
        persist()
        debugLog.log("project-create", "persist complete projectID=\(project.id.uuidString)")
        refreshGitState()
        debugLog.log("project-create", "git refresh requested projectID=\(project.id.uuidString)")
        updateGitRemoteSyncPolling()
        refreshAIStatsIfNeeded()
        debugLog.log("project-create", "import complete projectID=\(project.id.uuidString)")
    }
}
