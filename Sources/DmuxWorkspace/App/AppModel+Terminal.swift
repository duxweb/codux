import AppKit
import Foundation

extension AppModel {
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
        var newSessionID: UUID?
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            newSessionID = session.id
            workspace.sessions.append(session)

            if workspace.addTopSession(session.id) {
                terminalFocusRequestID = session.id
                statusMessage = "Created a new terminal pane."
            } else {
                workspace.addBottomTab(session.id)
                terminalFocusRequestID = session.id
                statusMessage = "Top row is full, added a bottom tab instead."
            }
        }
        if newSessionID != nil {
            refreshAIStatsIfNeeded()
        }
    }

    func createBottomTab() {
        var newSessionID: UUID?
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            newSessionID = session.id
            workspace.sessions.append(session)
            workspace.addBottomTab(session.id)
            terminalFocusRequestID = session.id
            statusMessage = workspace.bottomTabSessionIDs.count == 1
                ? String(localized: "workspace.bottom_split.created", defaultValue: "Created the bottom split area.", bundle: .module)
                : String(localized: "workspace.bottom_tab.created", defaultValue: "Added a new bottom tab.", bundle: .module)
        }
        if newSessionID != nil {
            refreshAIStatsIfNeeded()
        }
    }

    func splitSelectedPane(axis: PaneAxis) {
        var newSessionID: UUID?
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            newSessionID = session.id
            workspace.sessions.append(session)

            switch axis {
            case .horizontal:
                if workspace.addTopSession(session.id) {
                    terminalFocusRequestID = session.id
                    statusMessage = String(localized: "workspace.top_pane.horizontal_created", defaultValue: "Added a horizontal pane.", bundle: .module)
                } else {
                    workspace.sessions.removeAll(where: { $0.id == session.id })
                    newSessionID = nil
                    statusMessage = String(
                        format: String(localized: "workspace.top_pane.limit_format", defaultValue: "The top row supports up to %@ panes.", bundle: .module),
                        "\(ProjectWorkspace.maxTopPanes)"
                    )
                }
            case .vertical:
                workspace.addBottomTab(session.id)
                terminalFocusRequestID = session.id
                statusMessage = workspace.bottomTabSessionIDs.count == 1
                    ? String(localized: "workspace.bottom_split.created", defaultValue: "Created the bottom split area.", bundle: .module)
                    : String(localized: "workspace.bottom_tab.additional", defaultValue: "Added a tab to the bottom split area.", bundle: .module)
            }
        }
        if newSessionID != nil {
            refreshAIStatsIfNeeded()
        }
    }

    func detachSession(_ sessionID: UUID) {
        if isDetachedTerminal(sessionID) {
            DetachedTerminalWindowPresenter.show(model: self, sessionID: sessionID)
            return
        }

        guard let workspaceIndex = workspaces.firstIndex(where: { $0.containsVisibleSession(sessionID) }),
              let session = workspaces[workspaceIndex].session(for: sessionID),
              let placement = workspaces[workspaceIndex].detachVisibleSession(sessionID) else {
            statusMessage = String(localized: "terminal.detach.unavailable", defaultValue: "Unable to detach this terminal.", bundle: .module)
            return
        }

        detachedTerminalPlacementBySessionID[sessionID] = placement
        let projectID = workspaces[workspaceIndex].projectID
        persist()
        if selectedProjectID == projectID {
            terminalFocusRequestID = workspaces[workspaceIndex].selectedSessionID
        }
        refreshAIStatsIfNeeded()
        DetachedTerminalWindowPresenter.show(model: self, sessionID: sessionID)
        statusMessage = String(
            format: String(localized: "terminal.detach.success_format", defaultValue: "Detached %@ into a separate window.", bundle: .module),
            session.title
        )
    }

    func restoreDetachedSession(_ sessionID: UUID, shouldFocus: Bool = true) {
        guard let placement = detachedTerminalPlacementBySessionID.removeValue(forKey: sessionID),
              let workspaceIndex = workspaces.firstIndex(where: { $0.projectID == placement.projectID }) else {
            return
        }

        workspaces[workspaceIndex].restoreDetachedSession(sessionID, placement: placement)
        persist()
        if shouldFocus && selectedProjectID == placement.projectID {
            terminalFocusRequestID = sessionID
            terminalFocusRenderVersion &+= 1
        }
        refreshAIStatsIfNeeded()
    }

    func selectSession(_ sessionID: UUID) {
        guard Self.shouldRefreshSelectionFocus(
            requestedSessionID: sessionID,
            selectedSessionID: selectedSessionID,
            pendingFocusRequestID: terminalFocusRequestID,
            registryFocusedSessionID: DmuxTerminalBackend.shared.registry.focusedSessionID()
        ) else {
            return
        }
        mutateSelectedWorkspace { workspace, _ in
            workspace.selectedSessionID = sessionID
            terminalFocusRequestID = sessionID
        }
    }

    func requestTerminalFocus(_ sessionID: UUID) {
        terminalFocusRequestID = sessionID
        terminalFocusRenderVersion &+= 1
        _ = DmuxTerminalBackend.shared.registry.focus(sessionID: sessionID)
    }

    func selectBottomTabSession(_ sessionID: UUID) {
        guard let workspace = selectedWorkspace else { return }
        guard workspace.bottomTabSessionIDs.contains(sessionID) else { return }
        guard Self.shouldRefreshBottomTabSelection(
            requestedSessionID: sessionID,
            selectedSessionID: workspace.selectedSessionID,
            selectedBottomTabSessionID: workspace.selectedBottomTabSessionID,
            pendingFocusRequestID: terminalFocusRequestID,
            registryFocusedSessionID: DmuxTerminalBackend.shared.registry.focusedSessionID()
        ) else {
            return
        }
        mutateSelectedWorkspace { workspace, _ in
            workspace.selectedBottomTabSessionID = sessionID
            workspace.selectedSessionID = sessionID
            terminalFocusRequestID = sessionID
        }
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

    func clearTerminalFocusOutsideSelectedProject() {
        let sessionIDs = Set(selectedWorkspace?.sessions.map(\.id) ?? [])
        DmuxTerminalBackend.shared.registry.clearFocusedSessionIfOutside(sessionIDs, in: NSApp.keyWindow ?? NSApp.mainWindow)
    }

    func restoreSelectedTerminalFocusIfNeeded() {
        guard let sessionID = selectedSessionID else {
            return
        }
        terminalFocusRequestID = sessionID
        terminalFocusRenderVersion &+= 1
        _ = DmuxTerminalBackend.shared.registry.focus(sessionID: sessionID)
        DispatchQueue.main.async {
            _ = DmuxTerminalBackend.shared.registry.focus(sessionID: sessionID)
        }
    }

    func sendInterruptToSelectedSession() -> Bool {
        guard let sessionID = DmuxTerminalBackend.shared.registry.focusedSessionID() ?? selectedSessionID else {
            return false
        }
        terminalFocusRequestID = sessionID
        let didSend = DmuxTerminalBackend.shared.registry.sendInterrupt(to: sessionID)
        return didSend
    }

    func sendEscapeToSelectedSessionIfInterruptingAI() -> Bool {
        guard let sessionID = DmuxTerminalBackend.shared.registry.focusedSessionID() ?? selectedSessionID,
              let tool = aiSessionStore.tool(for: sessionID),
              toolDriverFactory.isRealtimeTool(tool),
              aiSessionStore.isRunning(terminalID: sessionID) else {
            return false
        }

        terminalFocusRequestID = sessionID
        let didSend = DmuxTerminalBackend.shared.registry.sendEscape(to: sessionID)
        return didSend
    }

    func insertPathIntoCurrentTerminal(_ url: URL) {
        guard let sessionID = DmuxTerminalBackend.shared.registry.focusedSessionID() ?? selectedSessionID else {
            statusMessage = String(localized: "files.panel.insert_path_terminal.failure", defaultValue: "No active terminal to insert the path.", bundle: .module)
            return
        }

        let text = shellQuoted(url.standardizedFileURL.path) + " "
        terminalFocusRequestID = sessionID
        let didSend = DmuxTerminalBackend.shared.registry.sendText(text, to: sessionID)
        if didSend {
            _ = DmuxTerminalBackend.shared.registry.focus(sessionID: sessionID)
            statusMessage = String(localized: "files.panel.insert_path_terminal.success", defaultValue: "Inserted path into terminal.", bundle: .module)
        } else {
            statusMessage = String(localized: "files.panel.insert_path_terminal.failure", defaultValue: "No active terminal to insert the path.", bundle: .module)
        }
        debugLog.log(
            "terminal-command",
            "insert-path session=\(sessionID.uuidString) sent=\(didSend) path=\(url.path)"
        )
    }

    func session(for sessionID: UUID) -> TerminalSession? {
        selectedWorkspace?.sessions.first(where: { $0.id == sessionID })
    }

    func closeSession(_ sessionID: UUID) {
        noteTerminalLoadingState(sessionID, isLoading: false)
        if let placement = detachedTerminalPlacementBySessionID.removeValue(forKey: sessionID) {
            debugLog.log(
                "terminal-lifecycle",
                "release-request session=\(sessionID.uuidString) reason=close-detached project=\(placement.projectID.uuidString)"
            )
            mutateWorkspace(projectID: placement.projectID) { workspace in
                workspace.removeSession(sessionID)
            }
            runtimeIngressService.clearLiveState(sessionID: sessionID)
            aiStatsStore.handleTerminalSessionClosed(
                sessionID: sessionID,
                project: selectedProject,
                projects: projects,
                selectedSessionID: selectedSessionID
            )
            DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
            clearTerminalRecoveryState(for: sessionID)
            DetachedTerminalWindowPresenter.dismiss(sessionID: sessionID, restoreOnClose: false)
            statusMessage = String(localized: "terminal.closed", defaultValue: "Closed terminal.", bundle: .module)
            return
        }

        debugLog.log(
            "terminal-lifecycle",
            "release-request session=\(sessionID.uuidString) reason=close-workspace selected=\(selectedSessionID?.uuidString ?? "nil")"
        )
        var didCloseSession = false
        var didCreateReplacementSession = false
        mutateSelectedWorkspace { workspace, project in
            guard workspace.containsSession(sessionID) || workspace.containsVisibleSession(sessionID) else {
                return
            }
            let shouldRebuildLastTerminal = workspace.visibleSessionCount <= 1
            let replacementSession = shouldRebuildLastTerminal
                ? TerminalSession.make(project: project, command: project.defaultCommand)
                : nil

            workspace.removeSession(sessionID)
            if let replacementSession {
                didCreateReplacementSession = true
                workspace.sessions.append(replacementSession)
                _ = workspace.addTopSession(replacementSession.id)
                terminalFocusRequestID = replacementSession.id
                terminalFocusRenderVersion &+= 1
            }

            didCloseSession = true
            statusMessage = replacementSession == nil
                ? String(localized: "terminal.closed", defaultValue: "Closed terminal.", bundle: .module)
                : String(localized: "terminal.rebuilt", defaultValue: "Restarted terminal.", bundle: .module)
        }
        guard didCloseSession else {
            return
        }
        runtimeIngressService.clearLiveState(sessionID: sessionID)
        aiStatsStore.handleTerminalSessionClosed(
            sessionID: sessionID,
            project: selectedProject,
            projects: projects,
            selectedSessionID: selectedSessionID
        )
        DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
        clearTerminalRecoveryState(for: sessionID)
        if didCreateReplacementSession == false {
            terminalFocusRenderVersion &+= 1
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
        let isLastTerminal = workspace.visibleSessionCount <= 1

        let dialog = ConfirmDialogState(
            title: isLastTerminal
                ? String(localized: "workspace.restart_current_terminal.title", defaultValue: "Restart Current Terminal", bundle: .module)
                : String(localized: "workspace.close_current_split.title", defaultValue: "Close Current Split", bundle: .module),
            message: isLastTerminal
                ? String(localized: "workspace.restart_current_terminal.message", defaultValue: "This is the last terminal for the project. Close it and start a fresh terminal?", bundle: .module)
                : String(localized: "workspace.close_current_split.message", defaultValue: "Are you sure you want to close the current split or tab?", bundle: .module),
            icon: isLastTerminal ? "arrow.clockwise" : "xmark.rectangle.portrait",
            iconColor: AppTheme.warning,
            primaryTitle: isLastTerminal
                ? String(localized: "common.restart_now", defaultValue: "Restart Now", bundle: .module)
                : String(localized: "common.close", defaultValue: "Close", bundle: .module),
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

    private func mutateWorkspace(projectID: UUID, _ transform: (inout ProjectWorkspace) -> Void) {
        guard let index = workspaces.firstIndex(where: { $0.projectID == projectID }) else {
            return
        }

        var updatedWorkspaces = workspaces
        transform(&updatedWorkspaces[index])
        workspaces = updatedWorkspaces
        persist()
    }

    func createSplitTerminal(command: String, axis: PaneAxis) -> UUID? {
        createSplitTerminal(command: command, axis: axis, runCommandInsideShell: false)
    }

    func createSplitTerminalRunningCommandInShell(command: String, axis: PaneAxis) -> UUID? {
        createSplitTerminal(command: command, axis: axis, runCommandInsideShell: true)
    }

    private func createSplitTerminal(command: String, axis: PaneAxis, runCommandInsideShell: Bool) -> UUID? {
        guard let selectedProjectID,
              let project = projects.first(where: { $0.id == selectedProjectID }),
              let index = workspaces.firstIndex(where: { $0.projectID == selectedProjectID }) else {
            debugLog.log(
                "terminal-command",
                "split-failed axis=\(axis == .horizontal ? "horizontal" : "vertical") reason=missing-selected-project command=\(command)"
            )
            return nil
        }

        var updatedWorkspaces = workspaces
        let session = TerminalSession.make(project: project, command: runCommandInsideShell ? "" : command)
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
        debugLog.log(
            "terminal-command",
            "split-created session=\(session.id.uuidString) axis=\(axis == .horizontal ? "horizontal" : "vertical") mode=\(runCommandInsideShell ? "shell-send" : "direct") command=\(command)"
        )
        if runCommandInsideShell {
            sendTerminalCommandWhenShellReady(command, sessionID: session.id)
        }
        return session.id
    }

    private func sendTerminalCommandWhenShellReady(_ command: String, sessionID: UUID) {
        Task { @MainActor [weak self] in
            guard let self else { return }
            for attempt in 1...30 {
                if let shellPID = DmuxTerminalBackend.shared.registry.shellPID(for: sessionID), shellPID > 0 {
                    let didSend = DmuxTerminalBackend.shared.registry.sendText(command + "\n", to: sessionID)
                    self.debugLog.log(
                        "terminal-command",
                        "shell-send session=\(sessionID.uuidString) shellPID=\(shellPID) attempt=\(attempt) sent=\(didSend) command=\(command)"
                    )
                    return
                }
                try? await Task.sleep(for: .milliseconds(100))
            }
            self.debugLog.log(
                "terminal-command",
                "shell-send-failed session=\(sessionID.uuidString) reason=shell-timeout command=\(command)"
            )
        }
    }

    func tryReuseSelectedTopTerminalForCommand(_ command: String) -> Bool {
        guard let workspace = selectedWorkspace,
              let selectedSessionID,
              workspace.topSessionIDs.contains(selectedSessionID) else {
            debugLog.log(
                "terminal-command",
                "reuse-skip reason=selection-not-top selected=\(selectedSessionID?.uuidString ?? "nil") command=\(command)"
            )
            return false
        }

        if let session = aiSessionStore.session(for: selectedSessionID),
           session.state != .idle,
           !session.tool.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            debugLog.log(
                "terminal-command",
                "reuse-failed session=\(selectedSessionID.uuidString) reason=runtime-live tool=\(session.tool) running=\(aiSessionStore.isRunning(terminalID: selectedSessionID)) command=\(command)"
            )
            return false
        }

        return tryReuseTerminalCommand(command, sessionID: selectedSessionID)
    }

    private func tryReuseTerminalCommand(_ command: String, sessionID: UUID) -> Bool {
        guard let selectedSessionID,
              selectedSessionID == sessionID,
              let shellPID = DmuxTerminalBackend.shared.registry.shellPID(for: sessionID),
              shellPID > 0 else {
            debugLog.log(
                "terminal-command",
                "reuse-failed session=\(sessionID.uuidString) selected=\(selectedSessionID?.uuidString ?? "nil") shellPID=\(DmuxTerminalBackend.shared.registry.shellPID(for: sessionID).map(String.init) ?? "nil") reason=missing-shell command=\(command)"
            )
            return false
        }

        let inspector = TerminalProcessInspector()
        guard inspector.activeTool(forShellPID: shellPID) == nil,
              inspector.hasActiveCommand(forShellPID: shellPID) == false else {
            debugLog.log(
                "terminal-command",
                "reuse-failed session=\(sessionID.uuidString) shellPID=\(shellPID) reason=busy command=\(command)"
            )
            return false
        }

        terminalFocusRequestID = sessionID
        let didSend = DmuxTerminalBackend.shared.registry.sendText(command + "\n", to: sessionID)
        debugLog.log(
            "terminal-command",
            "reuse-send session=\(sessionID.uuidString) shellPID=\(shellPID) sent=\(didSend) command=\(command)"
        )
        return didSend
    }
}
