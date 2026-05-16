import Foundation

extension AppModel {
    private static var agentDriverEventFlushInterval: Duration {
        .milliseconds(220)
    }

    func agentState(for session: TerminalSession) -> AgentSessionState {
        let tool = agentTool(for: session)
        if let state = agentSessionStates[session.id] {
            return state
        }
        return AgentSessionState.empty(sessionID: session.id, tool: tool)
    }

    func agentDraft(for sessionID: UUID) -> String {
        agentInputDrafts[sessionID] ?? ""
    }

    func updateAgentDraft(_ value: String, for sessionID: UUID) {
        agentInputDrafts[sessionID] = value
    }

    func createAgentSplit(tool: AgentToolKind = .codex, axis: PaneAxis = .horizontal) {
        var newSessionID: UUID?
        mutateSelectedWorkspace { workspace, project in
            let session = TerminalSession.makeAgent(project: project, tool: tool)
            newSessionID = session.id
            workspace.sessions.append(session)

            switch axis {
            case .horizontal:
                if workspace.addTopSession(session.id) {
                    statusMessage = String(localized: "agent.split.created", defaultValue: "Created an agent split.", bundle: .module)
                } else {
                    workspace.sessions.removeAll(where: { $0.id == session.id })
                    newSessionID = nil
                    statusMessage = String(
                        format: String(localized: "workspace.top_pane.limit_format", defaultValue: "The top row supports up to %@ panes.", bundle: .module),
                        "\(ProjectWorkspace.maxTopPanes)"
                    )
                }
            case .vertical:
                workspace.addBottomTab(session.id, title: tool.displayName)
                statusMessage = workspace.bottomTabSessionIDs.count == 1
                    ? String(localized: "agent.bottom_split.created", defaultValue: "Created the bottom agent area.", bundle: .module)
                    : String(localized: "agent.bottom_tab.created", defaultValue: "Added a new agent tab.", bundle: .module)
            }
        }
        guard let newSessionID else { return }
        agentSessionStates[newSessionID] = AgentSessionState.empty(sessionID: newSessionID, tool: tool)
        DmuxTerminalBackend.shared.registry.clearFocusedSession()
        refreshAIStatsIfNeeded()
    }

    func presentCreateSplitDialog() {
        guard appSettings.experiments.agentSplitEnabled else {
            splitSelectedPane(axis: .horizontal)
            return
        }

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "workspace.create_split.title", defaultValue: "Create Split", bundle: .module),
            message: String(localized: "workspace.create_split.message", defaultValue: "Choose a terminal split or a structured agent chat split.", bundle: .module),
            icon: "rectangle.split.2x1",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "agent.create", defaultValue: "Agent", bundle: .module),
            secondaryTitle: String(localized: "workspace.create_split.terminal", defaultValue: "Terminal", bundle: .module),
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else { return }
            switch result {
            case .primary:
                self.presentCreateAgentDialog()
            case .secondary:
                self.splitSelectedPane(axis: .horizontal)
            case .cancel, nil:
                break
            }
        }
    }

    func presentCreateAgentDialog() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = AgentSplitDialogState(
            title: String(localized: "agent.create.title", defaultValue: "New Agent Split", bundle: .module),
            message: String(localized: "agent.create.message", defaultValue: "Select the AI CLI driver to run with structured protocol events.", bundle: .module),
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            tools: AgentToolKind.allCases,
            selectedTool: .codex
        )
        AgentSplitDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, let result else { return }
            self.createAgentSplit(tool: result.tool, axis: .horizontal)
        }
    }

    func sendAgentMessage(session: TerminalSession, prompt: String) {
        let tool = agentTool(for: session)
        sendAgentMessage(
            session: session,
            prompt: prompt,
            model: agentModel(for: tool),
            fullAccess: agentFullAccess(for: tool),
            reasoningEffort: agentReasoningEffort(for: tool)
        )
    }

    func sendAgentMessage(
        session: TerminalSession,
        prompt: String,
        model overrideModel: String?,
        fullAccess: Bool,
        reasoningEffort: AppAICodexReasoningEffort?
    ) {
        let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedPrompt.isEmpty else { return }
        guard agentRunTasks[session.id] == nil else {
            return
        }

        let tool = agentTool(for: session)
        let resolvedModel = normalizedNonEmptyString(overrideModel)
        let resolvedEffort = tool == .codex ? reasoningEffort?.codexValue : nil
        var state = agentSessionStates[session.id] ?? AgentSessionState.empty(sessionID: session.id, tool: tool)
        state.tool = tool
        state.runState = .running
        state.statusText = String(localized: "agent.status.running", defaultValue: "Running", bundle: .module)
        state.runStartedAt = Date()
        state.runCompletedAt = nil
        state.updatedAt = Date()
        state.tasks = []
        state.fileChanges = []
        state.messages.append(
            AgentMessage(id: UUID(), role: .user, content: trimmedPrompt, createdAt: Date())
        )
        appendAgentTimelineItem(
            AgentTimelineItem(
                id: "user-\(UUID().uuidString)",
                turnID: nil,
                itemID: nil,
                kind: .userPrompt,
                role: .user,
                title: nil,
                content: trimmedPrompt,
                detail: nil,
                status: .completed,
                createdAt: Date(),
                updatedAt: Date()
            ),
            to: &state
        )
        agentSessionStates[session.id] = state
        agentInputDrafts[session.id] = ""
        DmuxTerminalBackend.shared.registry.clearFocusedSession()
        debugLog.log(
            "agent-driver",
            "start session=\(session.id.uuidString) tool=\(tool.rawValue) permission=\(fullAccess ? "fullAccess" : "default") model=\(resolvedModel ?? "default") effort=\(resolvedEffort ?? "none")"
        )

        let request = AgentDriverRequest(
            tool: tool,
            prompt: trimmedPrompt,
            cwd: session.cwd,
            model: resolvedModel,
            reasoningEffort: resolvedEffort,
            fullAccess: fullAccess,
            externalSessionID: state.externalSessionID
        )

        let task = Task { [weak self] in
            guard let self else { return }
            do {
                let driver = try self.agentDriverFactory.driver(for: tool)
                try await driver.run(request: request) { event in
                    await MainActor.run {
                        self.enqueueAgentDriverEvent(event, sessionID: session.id, tool: tool)
                    }
                }
                await MainActor.run {
                    self.clearAgentRunTask(sessionID: session.id, tool: tool)
                }
            } catch {
                await MainActor.run {
                    if error is CancellationError || Task.isCancelled {
                        self.finishAgentRunCancelled(sessionID: session.id, tool: tool)
                    } else {
                        self.failAgentRun(sessionID: session.id, tool: tool, error: error)
                    }
                }
            }
        }
        agentRunTasks[session.id] = task
    }

    func stopAgentRun(sessionID: UUID) {
        agentRunTasks[sessionID]?.cancel()
        agentRunTasks[sessionID] = nil
        flushAgentDriverEvents(sessionID: sessionID)
        guard var state = agentSessionStates[sessionID] else { return }
        state.runState = .idle
        state.statusText = String(localized: "agent.status.stopped", defaultValue: "Stopped", bundle: .module)
        state.updatedAt = Date()
        settleRunningAgentActivity(in: &state, status: .completed)
        agentSessionStates[sessionID] = state
        statusMessage = String(localized: "agent.status.stopped", defaultValue: "Stopped", bundle: .module)
    }

    func applyAgentDriverEventForTesting(_ event: AgentDriverEvent, sessionID: UUID, tool: AgentToolKind) {
        applyAgentDriverEvent(event, sessionID: sessionID, tool: tool)
    }

    func applyAgentDriverEventsForTesting(_ events: [AgentDriverEvent], sessionID: UUID, tool: AgentToolKind) {
        var state = agentSessionStates[sessionID] ?? AgentSessionState.empty(sessionID: sessionID, tool: tool)
        state.tool = tool
        state.updatedAt = Date()
        applyCoalescedAgentDriverEvents(events, to: &state, tool: tool)
        agentSessionStates[sessionID] = state
    }

    private func applyAgentDriverEvent(_ event: AgentDriverEvent, sessionID: UUID, tool: AgentToolKind) {
        var state = agentSessionStates[sessionID] ?? AgentSessionState.empty(sessionID: sessionID, tool: tool)
        state.tool = tool
        state.updatedAt = Date()

        switch event {
        case .status(let text):
            state.statusText = text
        case .message(let role, let content):
            appendAgentMessage(role: role, content: content, to: &state)
            if tool != .codex {
                appendAgentTimelineDelta(
                    AgentTimelineDelta(
                        id: "message-\(role.rawValue)-\(state.messages.last?.id.uuidString ?? UUID().uuidString)",
                        turnID: nil,
                        itemID: nil,
                        kind: timelineKind(for: role),
                        role: role,
                        title: nil,
                        detail: nil,
                        delta: content,
                        status: .running
                    ),
                    to: &state
                )
            }
        case .timelineItem(let item):
            appendAgentTimelineItem(item, to: &state)
        case .timelineDelta(let delta):
            appendAgentTimelineDelta(delta, to: &state)
        case .task(let task):
            upsertAgentTask(task, in: &state)
        case .fileChange(let fileChange):
            upsertAgentFileChange(fileChange, in: &state)
        case .fileChanges(let fileChanges):
            state.fileChanges = fileChanges
        case .externalSessionID(let externalSessionID):
            state.externalSessionID = externalSessionID
        case .completed(let exitCode):
            state.runState = exitCode == 0 ? .idle : .failed
            state.runCompletedAt = Date()
            state.statusText = exitCode == 0
                ? String(localized: "agent.status.completed", defaultValue: "Completed", bundle: .module)
                : String(
                    format: String(localized: "agent.status.exit_format", defaultValue: "Exited %@", bundle: .module),
                    "\(exitCode)"
                )
            settleRunningAgentActivity(in: &state, status: exitCode == 0 ? .completed : .failed)
        }
        agentSessionStates[sessionID] = state
    }

    private func clearAgentRunTask(sessionID: UUID, tool: AgentToolKind) {
        flushAgentDriverEvents(sessionID: sessionID)
        agentRunTasks[sessionID] = nil
        debugLog.log("agent-driver", "process-exited session=\(sessionID.uuidString) tool=\(tool.rawValue)")
    }

    private func failAgentRun(sessionID: UUID, tool: AgentToolKind, error: Error) {
        flushAgentDriverEvents(sessionID: sessionID)
        agentRunTasks[sessionID] = nil
        guard var state = agentSessionStates[sessionID] else {
            return
        }
        state.updatedAt = Date()
        state.runState = .failed
        state.runCompletedAt = Date()
        state.statusText = String(localized: "agent.status.failed", defaultValue: "Failed", bundle: .module)
        settleRunningAgentActivity(in: &state, status: .failed)
        appendAgentMessage(role: .error, content: error.localizedDescription, to: &state)
        appendAgentTimelineItem(
            AgentTimelineItem(
                id: "error-\(UUID().uuidString)",
                turnID: nil,
                itemID: nil,
                kind: .error,
                role: .error,
                title: nil,
                content: error.localizedDescription,
                detail: nil,
                status: .failed,
                createdAt: Date(),
                updatedAt: Date()
            ),
            to: &state
        )
        statusMessage = error.localizedDescription
        debugLog.log("agent-driver", "failed session=\(sessionID.uuidString) tool=\(tool.rawValue) error=\(error.localizedDescription)")
        agentSessionStates[sessionID] = state
    }

    private func finishAgentRunCancelled(sessionID: UUID, tool: AgentToolKind) {
        flushAgentDriverEvents(sessionID: sessionID)
        agentRunTasks[sessionID] = nil
        guard var state = agentSessionStates[sessionID] else {
            return
        }
        state.runState = .idle
        state.statusText = String(localized: "agent.status.stopped", defaultValue: "Stopped", bundle: .module)
        state.runCompletedAt = Date()
        state.updatedAt = Date()
        settleRunningAgentActivity(in: &state, status: .completed)
        agentSessionStates[sessionID] = state
        debugLog.log("agent-driver", "stopped session=\(sessionID.uuidString) tool=\(tool.rawValue)")
    }

    private func enqueueAgentDriverEvent(_ event: AgentDriverEvent, sessionID: UUID, tool: AgentToolKind) {
        switch event {
        case .status, .message, .timelineDelta:
            pendingAgentDriverEvents[sessionID, default: []].append(event)
            scheduleAgentDriverEventFlush(sessionID: sessionID, tool: tool)
        case .timelineItem, .task, .fileChange, .fileChanges, .externalSessionID, .completed:
            flushAgentDriverEvents(sessionID: sessionID)
            applyAgentDriverEvent(event, sessionID: sessionID, tool: tool)
        }
    }

    private func scheduleAgentDriverEventFlush(sessionID: UUID, tool: AgentToolKind) {
        guard pendingAgentDriverEventFlushTasks[sessionID] == nil else {
            return
        }
        pendingAgentDriverEventFlushTasks[sessionID] = Task { @MainActor [weak self] in
            try? await Task.sleep(for: Self.agentDriverEventFlushInterval)
            guard let self, !Task.isCancelled else {
                return
            }
            self.pendingAgentDriverEventFlushTasks[sessionID] = nil
            self.flushAgentDriverEvents(sessionID: sessionID, tool: tool)
        }
    }

    private func flushAgentDriverEvents(sessionID: UUID) {
        pendingAgentDriverEventFlushTasks[sessionID]?.cancel()
        pendingAgentDriverEventFlushTasks[sessionID] = nil
        let tool = agentSessionStates[sessionID]?.tool ?? .codex
        flushAgentDriverEvents(sessionID: sessionID, tool: tool)
    }

    private func flushAgentDriverEvents(sessionID: UUID, tool: AgentToolKind) {
        guard let events = pendingAgentDriverEvents.removeValue(forKey: sessionID),
              events.isEmpty == false else {
            return
        }

        var state = agentSessionStates[sessionID] ?? AgentSessionState.empty(sessionID: sessionID, tool: tool)
        state.tool = tool
        state.updatedAt = Date()
        applyCoalescedAgentDriverEvents(events, to: &state, tool: tool)
        agentSessionStates[sessionID] = state
    }

    private func applyCoalescedAgentDriverEvents(
        _ events: [AgentDriverEvent],
        to state: inout AgentSessionState,
        tool: AgentToolKind
    ) {
        var latestStatusText: String?
        var pendingMessages: [(role: AgentRole, content: String)] = []
        var pendingTimelineDeltas: [AgentTimelineDelta] = []
        var didReceiveCompletion = false

        for event in events {
            switch event {
            case .status(let text):
                latestStatusText = text
            case .message(let role, let content):
                pendingMessages.append((role, content))
            case .timelineDelta(let delta):
                pendingTimelineDeltas.append(delta)
            case .timelineItem(let item):
                appendAgentTimelineItem(item, to: &state)
            case .task(let task):
                upsertAgentTask(task, in: &state)
            case .fileChange(let fileChange):
                upsertAgentFileChange(fileChange, in: &state)
            case .fileChanges(let fileChanges):
                state.fileChanges = fileChanges
            case .externalSessionID(let externalSessionID):
                state.externalSessionID = externalSessionID
            case .completed(let exitCode):
                didReceiveCompletion = true
                state.runState = exitCode == 0 ? .idle : .failed
                state.runCompletedAt = Date()
                state.statusText = exitCode == 0
                    ? String(localized: "agent.status.completed", defaultValue: "Completed", bundle: .module)
                    : String(
                        format: String(localized: "agent.status.exit_format", defaultValue: "Exited %@", bundle: .module),
                        "\(exitCode)"
                    )
                settleRunningAgentActivity(in: &state, status: exitCode == 0 ? .completed : .failed)
            }
        }

        if let latestStatusText, didReceiveCompletion == false {
            state.statusText = latestStatusText
        }
        for message in coalescedAgentMessages(pendingMessages) {
            appendAgentMessage(role: message.role, content: message.content, to: &state)
            if tool != .codex {
                appendAgentTimelineDelta(
                    AgentTimelineDelta(
                        id: "message-\(message.role.rawValue)-\(state.messages.last?.id.uuidString ?? UUID().uuidString)",
                        turnID: nil,
                        itemID: nil,
                        kind: timelineKind(for: message.role),
                        role: message.role,
                        title: nil,
                        detail: nil,
                        delta: message.content,
                        status: .running
                    ),
                    to: &state
                )
            }
        }
        for delta in coalescedAgentTimelineDeltas(pendingTimelineDeltas) {
            appendAgentTimelineDelta(delta, to: &state)
        }
    }

    private func coalescedAgentMessages(
        _ messages: [(role: AgentRole, content: String)]
    ) -> [(role: AgentRole, content: String)] {
        var coalesced: [(role: AgentRole, content: String)] = []
        for message in messages {
            guard !message.content.isEmpty else { continue }
            if var last = coalesced.last, last.role == message.role, message.role != .user {
                last.content = mergedAgentTimelineContent(existing: last.content, incoming: message.content)
                coalesced[coalesced.count - 1] = last
            } else {
                coalesced.append(message)
            }
        }
        return coalesced
    }

    private func coalescedAgentTimelineDeltas(_ deltas: [AgentTimelineDelta]) -> [AgentTimelineDelta] {
        var order: [String] = []
        var coalesced: [String: AgentTimelineDelta] = [:]
        for delta in deltas {
            guard delta.delta.isEmpty == false else { continue }
            if var existing = coalesced[delta.id] {
                if let title = delta.title {
                    existing.title = title
                }
                if let detail = delta.detail {
                    existing.detail = detail
                }
                existing.delta = mergedAgentTimelineContent(existing: existing.delta, incoming: delta.delta)
                existing.status = delta.status
                coalesced[delta.id] = existing
            } else {
                order.append(delta.id)
                coalesced[delta.id] = delta
            }
        }
        return order.compactMap { coalesced[$0] }
    }

    private func appendAgentMessage(role: AgentRole, content: String, to state: inout AgentSessionState) {
        guard let normalizedContent = normalizedAgentMessageContent(role: role, content: content) else { return }
        if var last = state.messages.last,
           last.role == role,
           role != .user {
            if last.content == normalizedContent || last.content.hasSuffix(normalizedContent) {
                return
            }
            if normalizedContent.hasPrefix(last.content) {
                last.content = normalizedContent
            } else {
                last.content += normalizedContent
            }
            last.createdAt = Date()
            state.messages[state.messages.count - 1] = last
        } else {
            state.messages.append(
                AgentMessage(id: UUID(), role: role, content: normalizedContent, createdAt: Date())
            )
        }
    }

    private func normalizedAgentMessageContent(role: AgentRole, content: String) -> String? {
        if role == .user || role == .error {
            let trimmed = content.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }
        return content.isEmpty ? nil : content
    }

    private func appendAgentTimelineItem(_ item: AgentTimelineItem, to state: inout AgentSessionState) {
        var normalized = item
        normalized.updatedAt = Date()
        if normalized.createdAt > normalized.updatedAt {
            normalized.createdAt = normalized.updatedAt
        }
        if let index = state.timelineItems.firstIndex(where: { $0.id == normalized.id }) {
            let existing = state.timelineItems[index]
            if normalized.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                normalized.content = existing.content
            }
            if normalized.title == nil {
                normalized.title = existing.title
            }
            if normalized.detail == nil {
                normalized.detail = existing.detail
            }
            normalized.createdAt = existing.createdAt
            state.timelineItems[index] = normalized
        } else {
            guard shouldAppendNewAgentTimelineItem(normalized) else { return }
            state.timelineItems.append(normalized)
        }
    }

    private func settleRunningAgentActivity(in state: inout AgentSessionState, status: AgentTimelineStatus) {
        let now = Date()
        for index in state.timelineItems.indices where state.timelineItems[index].status == .running {
            state.timelineItems[index].status = status
            state.timelineItems[index].updatedAt = now
        }
    }

    private func shouldAppendNewAgentTimelineItem(_ item: AgentTimelineItem) -> Bool {
        if normalizedNonEmptyString(item.content) != nil
            || normalizedNonEmptyString(item.title) != nil
            || normalizedNonEmptyString(item.detail) != nil {
            return true
        }

        switch item.kind {
        case .command, .fileChange:
            return true
        case .plan, .reasoning, .tool:
            return item.status == .running
        case .userPrompt, .assistantMessage, .error, .status:
            return false
        }
    }

    private func appendAgentTimelineDelta(_ delta: AgentTimelineDelta, to state: inout AgentSessionState) {
        guard delta.delta.isEmpty == false else { return }
        let now = Date()
        if let index = state.timelineItems.firstIndex(where: { $0.id == delta.id }) {
            var item = state.timelineItems[index]
            if let title = delta.title {
                item.title = title
            }
            if let detail = delta.detail {
                item.detail = detail
            }
            item.content = mergedAgentTimelineContent(existing: item.content, incoming: delta.delta)
            item.status = delta.status
            item.updatedAt = now
            state.timelineItems[index] = item
            return
        }
        appendAgentTimelineItem(
            AgentTimelineItem(
                id: delta.id,
                turnID: delta.turnID,
                itemID: delta.itemID,
                kind: delta.kind,
                role: delta.role,
                title: delta.title,
                content: delta.delta,
                detail: delta.detail,
                status: delta.status,
                createdAt: now,
                updatedAt: now
            ),
            to: &state
        )
    }

    private func mergedAgentTimelineContent(existing: String, incoming: String) -> String {
        guard !incoming.isEmpty else { return existing }
        guard !existing.isEmpty else { return incoming }
        if existing == incoming || existing.hasSuffix(incoming) {
            return existing
        }
        if incoming.hasPrefix(existing) {
            return incoming
        }
        return existing + incoming
    }

    private func timelineKind(for role: AgentRole) -> AgentTimelineKind {
        switch role {
        case .user:
            return .userPrompt
        case .assistant:
            return .assistantMessage
        case .system:
            return .status
        case .tool:
            return .tool
        case .error:
            return .error
        }
    }

    func reviewAgentChanges(session: TerminalSession, selectedPath: String? = nil) {
        guard worktrees.contains(where: { $0.id == session.projectID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        selectedWorktreeID = session.projectID
        selectWorkspaceReview()
        selectedWorktreeReviewFileID = selectedPath ?? agentSessionStates[session.id]?.fileChanges.first?.path
        refreshWorktreeReview()
    }

    func discardAgentFileChange(_ fileChange: AgentFileChange, session: TerminalSession) {
        guard let worktree = worktrees.first(where: { $0.id == session.projectID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "agent.discard_file.title", defaultValue: "Discard File Change", bundle: .module),
            message: String(
                format: String(localized: "agent.discard_file.message_format", defaultValue: "Discard uncommitted changes in %@?", bundle: .module),
                fileChange.path
            ),
            icon: "arrow.uturn.backward.circle",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.performDiscardAgentFileChange(fileChange, worktree: worktree, sessionID: session.id)
        }
    }

    func discardAllAgentChanges(session: TerminalSession) {
        guard let worktree = worktrees.first(where: { $0.id == session.projectID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        let changes = agentSessionStates[session.id]?.fileChanges ?? []
        guard !changes.isEmpty else {
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "agent.discard_all.title", defaultValue: "Discard All Changes", bundle: .module),
            message: String(localized: "agent.discard_session.message", defaultValue: "Discard all file changes reported by this agent?", bundle: .module),
            icon: "arrow.uturn.backward.circle",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.files.discard_all", defaultValue: "Discard All", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.performDiscardAgentFileChanges(changes, worktree: worktree, sessionID: session.id)
        }
    }

    private func performDiscardAgentFileChange(_ fileChange: AgentFileChange, worktree: ProjectWorktree, sessionID: UUID) {
        statusMessage = String(localized: "agent.discard.running", defaultValue: "Discarding changes.", bundle: .module)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                try service.discardWorktreeReviewFile(fileChange.path, at: worktree.path)
                await MainActor.run {
                    self.removeAgentFileChanges([fileChange.path], sessionID: sessionID)
                    self.refreshWorktreeGitSummaries()
                    self.refreshWorktreeReview()
                    self.statusMessage = String(localized: "agent.discard_file.success", defaultValue: "Discarded file change.", bundle: .module)
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                }
            }
        }
    }

    private func performDiscardAgentFileChanges(_ changes: [AgentFileChange], worktree: ProjectWorktree, sessionID: UUID) {
        let paths = changes.map(\.path).filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        guard !paths.isEmpty else {
            return
        }
        statusMessage = String(localized: "agent.discard.running", defaultValue: "Discarding changes.", bundle: .module)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                for path in paths {
                    try service.discardWorktreeReviewFile(path, at: worktree.path)
                }
                await MainActor.run {
                    self.removeAgentFileChanges(paths, sessionID: sessionID)
                    self.refreshWorktreeGitSummaries()
                    self.refreshWorktreeReview()
                    self.statusMessage = String(localized: "agent.discard_all.success", defaultValue: "Discarded all changes.", bundle: .module)
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                }
            }
        }
    }

    private func agentTool(for session: TerminalSession) -> AgentToolKind {
        guard let rawValue = normalizedNonEmptyString(session.agentTool),
              let tool = AgentToolKind(rawValue: rawValue) else {
            return .codex
        }
        return tool
    }

    private func agentModel(for tool: AgentToolKind) -> String? {
        switch tool {
        case .codex:
            return normalizedNonEmptyString(appSettings.ai.runtimeTools.codexModel)
        case .claude:
            return normalizedNonEmptyString(appSettings.ai.runtimeTools.claudeCodeModel)
        case .opencode:
            return normalizedNonEmptyString(appSettings.ai.runtimeTools.opencodeModel)
        }
    }

    private func agentFullAccess(for tool: AgentToolKind) -> Bool {
        switch tool {
        case .codex:
            return appSettings.ai.runtimeTools.codex == .fullAccess
        case .claude:
            return appSettings.ai.runtimeTools.claudeCode == .fullAccess
        case .opencode:
            return appSettings.ai.runtimeTools.opencode == .fullAccess
        }
    }

    private func agentReasoningEffort(for tool: AgentToolKind) -> AppAICodexReasoningEffort? {
        switch tool {
        case .codex:
            return appSettings.ai.runtimeTools.codexEffort
        case .claude, .opencode:
            return nil
        }
    }

    private func upsertAgentTask(_ task: AgentTaskItem, in state: inout AgentSessionState) {
        let title = task.title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        var normalized = task
        normalized.title = title
        normalized.updatedAt = Date()
        if let index = state.tasks.firstIndex(where: { $0.id == normalized.id }) {
            state.tasks[index] = normalized
        } else {
            state.tasks.append(normalized)
        }
    }

    private func upsertAgentFileChange(_ fileChange: AgentFileChange, in state: inout AgentSessionState) {
        let path = fileChange.path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !path.isEmpty else { return }
        var normalized = fileChange
        normalized.path = path
        normalized.summary = normalizedNonEmptyString(fileChange.summary)
        normalized.diff = normalizedNonEmptyString(fileChange.diff) ?? normalizedNonEmptyString(fileChange.summary)
        normalized.updatedAt = Date()
        if let index = state.fileChanges.firstIndex(where: { $0.path == normalized.path }) {
            state.fileChanges[index] = normalized
        } else {
            state.fileChanges.append(normalized)
        }
    }

    private func removeAgentFileChanges(_ paths: [String], sessionID: UUID) {
        guard var state = agentSessionStates[sessionID] else { return }
        let normalizedPaths = Set(paths.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) })
        state.fileChanges.removeAll { normalizedPaths.contains($0.path) }
        state.updatedAt = Date()
        agentSessionStates[sessionID] = state
    }
}
