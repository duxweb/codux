import AppKit
import Foundation

extension AppModel {
    func toggleSidebarExpansion() {
        isSidebarExpanded.toggle()
    }

    func toggleWorktreeSidebarExpansion() {
        isWorktreeSidebarExpanded.toggle()
    }

    func updateRightPanelWidth(_ width: CGFloat) {
        let clamped = min(max(width, 280), 560)
        guard abs(rightPanelWidth - clamped) > 0.5 else {
            return
        }
        rightPanelWidth = clamped
    }

    func updateBottomPaneHeight(_ height: CGFloat, for projectID: UUID, availableHeight: CGFloat) {
        let maxHeight = max(ProjectWorkspace.minimumBottomPaneHeight, availableHeight - ProjectWorkspace.minimumTopPaneHeight)
        let clamped = min(max(height, ProjectWorkspace.minimumBottomPaneHeight), maxHeight)

        guard let index = workspaces.firstIndex(where: { $0.projectID == projectID }) else {
            return
        }

        guard abs(workspaces[index].bottomPaneHeight - clamped) > 0.5 else {
            return
        }
        workspaces[index].bottomPaneHeight = clamped
    }

    func createNewTerminal() {
        var newSessionID: UUID?
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.make(project: project, command: project.defaultCommand)
            newSessionID = session.id
            workspace.sessions.append(session)

            if workspace.addTopSession(session.id) {
                terminalFocusRequestID = session.id
                FileBrowserKeyboardFocusState.activateTerminal()
                statusMessage = String(localized: "workspace.top_pane.created", defaultValue: "Created a new terminal pane.", bundle: .module)
            } else {
                workspace.addBottomTab(session.id)
                terminalFocusRequestID = session.id
                FileBrowserKeyboardFocusState.activateTerminal()
                statusMessage = String(localized: "workspace.top_pane.full_added_bottom_tab", defaultValue: "Top row is full, added a bottom tab instead.", bundle: .module)
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
            FileBrowserKeyboardFocusState.activateTerminal()
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
                    FileBrowserKeyboardFocusState.activateTerminal()
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
                FileBrowserKeyboardFocusState.activateTerminal()
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
        if selectedWorktreeID == projectID {
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
        if shouldFocus && selectedWorktreeID == placement.projectID {
            terminalFocusRequestID = sessionID
            terminalFocusRenderVersion &+= 1
        }
        refreshAIStatsIfNeeded()
    }

    func selectSession(_ sessionID: UUID) {
        let isAgentSession = selectedWorkspace?.session(for: sessionID)?.launchMode == .agent
        guard Self.shouldRefreshSelectionFocus(
            requestedSessionID: sessionID,
            selectedSessionID: selectedSessionID,
            pendingFocusRequestID: terminalFocusRequestID,
            registryFocusedSessionID: DmuxTerminalBackend.shared.registry.focusedSessionID()
        ) else {
            return
        }
        if isAgentSession {
            selectAgentSession(sessionID)
            return
        }
        mutateSelectedWorkspace { workspace, _ in
            workspace.selectedSessionID = sessionID
            resignTextResponderIfNeeded()
            terminalFocusRequestID = sessionID
            FileBrowserKeyboardFocusState.activateTerminal()
        }
    }

    func selectAgentSession(_ sessionID: UUID) {
        guard let workspace = selectedWorkspace,
              workspace.containsVisibleSession(sessionID),
              workspace.session(for: sessionID)?.launchMode == .agent else {
            return
        }

        let isBottomTab = workspace.bottomTabSessionIDs.contains(sessionID)
        let isAlreadySelected = workspace.selectedSessionID == sessionID
            && (!isBottomTab || workspace.selectedBottomTabSessionID == sessionID)
            && terminalFocusRequestID == nil
            && DmuxTerminalBackend.shared.registry.focusedSessionID() == nil
        if isAlreadySelected {
            FileBrowserKeyboardFocusState.clearFileBrowserIfNeeded()
            FileBrowserKeyboardFocusState.clearWorkspaceFileEditor(tabID: nil)
            DmuxTerminalBackend.shared.registry.resignFocusedTerminal(in: NSApp.keyWindow ?? NSApp.mainWindow)
            return
        }

        if workspace.selectedSessionID != sessionID || (isBottomTab && workspace.selectedBottomTabSessionID != sessionID) {
            mutateSelectedWorkspace { workspace, _ in
                workspace.selectedSessionID = sessionID
                if isBottomTab {
                    workspace.selectedBottomTabSessionID = sessionID
                }
            }
        }

        terminalFocusRequestID = nil
        FileBrowserKeyboardFocusState.clearFileBrowserIfNeeded()
        FileBrowserKeyboardFocusState.clearWorkspaceFileEditor(tabID: nil)
        DmuxTerminalBackend.shared.registry.resignFocusedTerminal(in: NSApp.keyWindow ?? NSApp.mainWindow)
        terminalFocusRenderVersion &+= 1
    }

    func requestTerminalFocus(_ sessionID: UUID) {
        FileBrowserKeyboardFocusState.activateTerminal()
        terminalFocusRequestID = sessionID
        terminalFocusRenderVersion &+= 1
        _ = DmuxTerminalBackend.shared.registry.focus(sessionID: sessionID)
    }

    func selectBottomTabSession(_ sessionID: UUID) {
        guard let workspace = selectedWorkspace else { return }
        guard workspace.bottomTabSessionIDs.contains(sessionID) else { return }
        let isAgentSession = workspace.session(for: sessionID)?.launchMode == .agent
        guard Self.shouldRefreshBottomTabSelection(
            requestedSessionID: sessionID,
            selectedSessionID: workspace.selectedSessionID,
            selectedBottomTabSessionID: workspace.selectedBottomTabSessionID,
            pendingFocusRequestID: terminalFocusRequestID,
            registryFocusedSessionID: DmuxTerminalBackend.shared.registry.focusedSessionID()
        ) else {
            return
        }
        if isAgentSession {
            selectAgentSession(sessionID)
            return
        }
        mutateSelectedWorkspace { workspace, _ in
            workspace.selectedBottomTabSessionID = sessionID
            workspace.selectedSessionID = sessionID
            resignTextResponderIfNeeded()
            terminalFocusRequestID = sessionID
            FileBrowserKeyboardFocusState.activateTerminal()
        }
    }

    func bottomTabDisplayTitle(sessionID: UUID, fallbackIndex: Int) -> String {
        if let title = selectedWorkspace?.session(for: sessionID)?.tabTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty {
            return title
        }
        return ProjectWorkspace.defaultBottomTabTitle(index: fallbackIndex)
    }

    func renameBottomTabSession(_ sessionID: UUID) {
        guard let workspace = selectedWorkspace,
              workspace.bottomTabSessionIDs.contains(sessionID),
              let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let fallbackIndex = workspace.bottomTabSessionIDs.firstIndex(of: sessionID) ?? 0
        let dialog = GitInputDialogState(
            kind: .renameTerminalTab,
            title: String(localized: "terminal.tab.rename.title", defaultValue: "Rename Tab", bundle: .module),
            message: String(localized: "terminal.tab.rename.message", defaultValue: "Set a short label for this bottom terminal tab.", bundle: .module),
            placeholder: String(localized: "terminal.tab.rename.placeholder", defaultValue: "Tab Name", bundle: .module),
            confirmTitle: String(localized: "common.save", defaultValue: "Save", bundle: .module),
            value: bottomTabDisplayTitle(sessionID: sessionID, fallbackIndex: fallbackIndex),
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] value in
            guard let self, let value else {
                return
            }
            self.updateBottomTabTitle(sessionID, title: value)
        }
    }

    func updateBottomTabTitle(_ sessionID: UUID, title: String) {
        guard let selectedWorktreeID,
              let index = workspaces.firstIndex(where: { $0.projectID == selectedWorktreeID }) else {
            return
        }

        var updatedWorkspaces = workspaces
        guard updatedWorkspaces[index].renameBottomTab(sessionID, to: title) else {
            return
        }
        updatedWorkspaces[index].selectedBottomTabSessionID = sessionID
        updatedWorkspaces[index].selectedSessionID = sessionID
        workspaces = updatedWorkspaces
        terminalFocusRequestID = sessionID
        statusMessage = String(localized: "terminal.tab.rename.success", defaultValue: "Renamed tab.", bundle: .module)
        persist()
    }

    func moveBottomTabSession(_ sessionID: UUID, to targetSessionID: UUID, persists: Bool = true) {
        guard let selectedWorktreeID,
              let index = workspaces.firstIndex(where: { $0.projectID == selectedWorktreeID }) else {
            return
        }

        var updatedWorkspaces = workspaces
        guard updatedWorkspaces[index].moveBottomTab(sessionID, to: targetSessionID) else {
            return
        }
        workspaces = updatedWorkspaces
        if persists {
            persist()
        }
    }

    func scheduleBottomTabOrderPersist() {
        scheduleDragReorderPersist()
    }

    func updateTopPaneRatios(_ ratios: [CGFloat]) {
        guard let selectedWorktreeID,
              let index = workspaces.firstIndex(where: { $0.projectID == selectedWorktreeID }) else {
            return
        }
        guard let normalizedRatios = Self.normalizedTopPaneRatios(
            ratios,
            expectedCount: workspaces[index].topSessionIDs.count
        ) else {
            return
        }
        guard Self.topPaneRatiosDiffer(
            normalizedRatios,
            workspaces[index].resolvedTopPaneRatios()
        ) else {
            return
        }

        var updatedWorkspaces = workspaces
        updatedWorkspaces[index].topPaneRatios = normalizedRatios
        workspaces = updatedWorkspaces
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
        let sessionToClose = workspaces
            .lazy
            .compactMap { $0.session(for: sessionID) }
            .first
        let closesAgentSession = sessionToClose?.launchMode == .agent
        if closesAgentSession {
            stopAgentRun(sessionID: sessionID)
            pendingAgentDriverEventFlushTasks[sessionID]?.cancel()
            pendingAgentDriverEventFlushTasks.removeValue(forKey: sessionID)
            pendingAgentDriverEvents.removeValue(forKey: sessionID)
            agentSessionStates.removeValue(forKey: sessionID)
            agentInputDrafts.removeValue(forKey: sessionID)
        }
        noteTerminalLoadingState(sessionID, isLoading: false)
        removeTaskMemos(forSessionID: sessionID)
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
            if closesAgentSession == false {
                DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
            }
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
        if closesAgentSession == false {
            DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
        }
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

        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = isLastTerminal
            ? String(localized: "workspace.restart_current_terminal.title", defaultValue: "Restart Current Terminal", bundle: .module)
            : String(localized: "workspace.close_current_split.title", defaultValue: "Close Current Split", bundle: .module)
        alert.informativeText = isLastTerminal
            ? String(localized: "workspace.restart_current_terminal.message", defaultValue: "This is the last terminal for the project. Close it and start a fresh terminal?", bundle: .module)
            : String(localized: "workspace.close_current_split.message", defaultValue: "Are you sure you want to close the current split or tab?", bundle: .module)
        alert.addButton(
            withTitle: isLastTerminal
                ? String(localized: "common.restart_now", defaultValue: "Restart Now", bundle: .module)
                : String(localized: "common.close", defaultValue: "Close", bundle: .module)
        )
        alert.addButton(withTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module))

        alert.buttons.first?.keyEquivalent = "\r"
        alert.buttons.dropFirst().first?.keyEquivalent = "\u{1b}"
        alert.beginSheetModal(for: parentWindow) { [weak self] response in
            guard response == .alertFirstButtonReturn else {
                return
            }
            Task { @MainActor [weak self] in
                self?.closeSession(sessionID)
            }
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

    private func resignTextResponderIfNeeded() {
        let window = NSApp.keyWindow ?? NSApp.mainWindow
        guard window?.firstResponder is NSTextView else {
            return
        }
        window?.makeFirstResponder(nil)
    }

    func mutateSelectedWorkspace(_ transform: (inout ProjectWorkspace, Project) -> Void) {
        guard let selectedWorktreeID,
              let project = selectedProject,
              let index = workspaces.firstIndex(where: { $0.projectID == selectedWorktreeID }) else {
            return
        }

        var updatedWorkspaces = workspaces
        transform(&updatedWorkspaces[index], project)
        workspaces = updatedWorkspaces
        persist()
    }

    func mutateWorkspace(projectID: UUID, _ transform: (inout ProjectWorkspace) -> Void) {
        guard let index = workspaces.firstIndex(where: { $0.projectID == projectID }) else {
            return
        }

        var updatedWorkspaces = workspaces
        transform(&updatedWorkspaces[index])
        workspaces = updatedWorkspaces
        persist()
    }

    private static func normalizedTopPaneRatios(
        _ ratios: [CGFloat],
        expectedCount: Int
    ) -> [CGFloat]? {
        guard ratios.count == expectedCount, !ratios.isEmpty else {
            return nil
        }
        let positiveRatios = ratios.map { max(0, $0) }
        let total = positiveRatios.reduce(0, +)
        guard total > 0 else {
            return nil
        }
        return positiveRatios.map { $0 / total }
    }

    private static func topPaneRatiosDiffer(
        _ lhs: [CGFloat],
        _ rhs: [CGFloat],
        tolerance: CGFloat = 0.002
    ) -> Bool {
        guard lhs.count == rhs.count else {
            return true
        }
        return zip(lhs, rhs).contains { abs($0 - $1) > tolerance }
    }

    func createSplitTerminal(command: String, axis: PaneAxis) -> UUID? {
        createSplitTerminal(command: command, axis: axis, runCommandInsideShell: false)
    }

    func createSplitTerminalRunningCommandInShell(command: String, axis: PaneAxis, logCommand: String? = nil) -> UUID? {
        createSplitTerminal(command: command, axis: axis, runCommandInsideShell: true, logCommand: logCommand)
    }

    private func createSplitTerminal(command: String, axis: PaneAxis, runCommandInsideShell: Bool, logCommand: String? = nil) -> UUID? {
        guard let selectedWorktreeID,
              let project = selectedProject,
              let index = workspaces.firstIndex(where: { $0.projectID == selectedWorktreeID }) else {
            debugLog.log(
                "terminal-command",
                "split-failed axis=\(axis == .horizontal ? "horizontal" : "vertical") reason=missing-selected-project command=\(logCommand ?? command)"
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
            "split-created session=\(session.id.uuidString) axis=\(axis == .horizontal ? "horizontal" : "vertical") mode=\(runCommandInsideShell ? "shell-send" : "direct") command=\(logCommand ?? command)"
        )
        if runCommandInsideShell {
            sendTerminalCommandWhenShellReady(command, sessionID: session.id, logCommand: logCommand)
        }
        return session.id
    }

    private func sendTerminalCommandWhenShellReady(_ command: String, sessionID: UUID, logCommand: String? = nil) {
        Task { @MainActor [weak self] in
            guard let self else { return }
            for attempt in 1...30 {
                if let shellPID = DmuxTerminalBackend.shared.registry.shellPID(for: sessionID), shellPID > 0 {
                    let didSend = DmuxTerminalBackend.shared.registry.sendText(command + "\n", to: sessionID)
                    self.debugLog.log(
                        "terminal-command",
                        "shell-send session=\(sessionID.uuidString) shellPID=\(shellPID) attempt=\(attempt) sent=\(didSend) command=\(logCommand ?? command)"
                    )
                    return
                }
                try? await Task.sleep(for: .milliseconds(100))
            }
            self.debugLog.log(
                "terminal-command",
                "shell-send-failed session=\(sessionID.uuidString) reason=shell-timeout command=\(logCommand ?? command)"
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
