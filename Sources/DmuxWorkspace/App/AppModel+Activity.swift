import AppKit
import Darwin
import Foundation

extension AppModel {
    func activityPhase(for projectID: UUID) -> ProjectActivityPhase {
        let payload = cachedActivityPayloadByProjectID[projectID]
        discardStaleCompletionPresentationIfNeeded(projectID: projectID)
        let runtimePhase = aiSessionStore.projectPhase(projectID: projectID)
        let cachedPhase = resolvedCachedProjectActivityPhase(payload: payload, projectID: projectID)
        let completionPhase = completionPresentationPhase(for: projectID)
        let resolvedPhase = Self.resolveDisplayedActivityPhase(
            runtimePhase: runtimePhase,
            cachedPhase: cachedPhase,
            completionPhase: completionPhase,
            cachedPayloadTool: cachedActivityPayloadByProjectID[projectID]?.tool,
            hasLiveRuntimeSessions: aiSessionStore.hasLiveSessions(projectID: projectID),
            isRealtimeTool: isRealtimeAITool
        )

        let source: String
        if resolvedPhase == runtimePhase {
            source = shouldUseRuntimeActivityOnly(projectID: projectID) ? "runtime-only" : "runtime"
        } else if resolvedPhase == completionPhase, completionPhase != .idle {
            source = "ui-completion"
        } else if resolvedPhase == cachedPhase {
            source = "cached"
        } else {
            source = "default"
        }

        logActivityPhaseResolution(projectID: projectID, source: source, phase: resolvedPhase)
        return resolvedPhase
    }

    func observeApplicationActivation() {
        let center = NotificationCenter.default

        let becameActive = center.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.logApplicationActivationActivity()
                self?.runtimeIngressService.ensureSocketListening()
                self?.performanceMonitor.setApplicationActive(true)
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
                self?.performanceMonitor.setApplicationActive(false)
                self?.updateGitRemoteSyncPolling()
            }
        }

        appActivationObservers = [becameActive, resignedActive]
    }

    func observeTerminalFocusChanges() {
        if let terminalFocusObserver {
            NotificationCenter.default.removeObserver(terminalFocusObserver)
        }

        terminalFocusObserver = NotificationCenter.default.addObserver(
            forName: .dmuxTerminalFocusDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.terminalFocusRenderVersion &+= 1
            }
        }
    }

    func performanceMonitorContextSnapshot() -> AppPerformanceMonitorStore.ContextSnapshot {
        let activityLabel: String
        if let project = selectedProject,
           let phase = activityByProjectID[project.id] {
            switch phase {
            case .idle:
                activityLabel = "idle"
            case .running(let tool):
                activityLabel = "running:\(tool)"
            case .waitingInput(let tool):
                activityLabel = "waiting-input:\(tool)"
            case .completed(let tool, _, let exitCode):
                activityLabel = "completed:\(tool):\(exitCode.map(String.init) ?? "nil")"
            }
        } else {
            activityLabel = "idle"
        }

        return AppPerformanceMonitorStore.ContextSnapshot(
            projectName: selectedProject?.name,
            panelName: rightPanel?.rawValue ?? "none",
            selectedSessionID: selectedSessionID?.uuidString,
            activity: activityLabel
        )
    }

    func startActivityWatchers() {
        activityStatusWatcher?.cancel()

        activityStatusWatcher = makeDirectoryWatcher(for: activityService.statusDirectoryURL())
        runtimeIngressService.startWatching()
        runtimeBridgeObserver = replaceActivityObserver(
            runtimeBridgeObserver,
            name: .dmuxAIRuntimeBridgeDidChange,
            sendNotifications: true,
            refreshAIStats: false,
            requiresStatusReload: false,
            refreshImmediatelyFromRuntime: true
        )
        runtimeActivityObserver = replaceActivityObserver(
            runtimeActivityObserver,
            name: .dmuxAIRuntimeActivityPulse,
            sendNotifications: false,
            refreshAIStats: false,
            requiresStatusReload: false,
            refreshImmediatelyFromRuntime: true
        )
    }

    func refreshAIStatsIfNeeded() {
        aiStatsStore.refreshIfNeeded(project: selectedProject, projects: projects, selectedSessionID: selectedSessionID)
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
            self?.queueProjectActivityRefresh(
                sendNotifications: true,
                refreshAIStats: true,
                requiresStatusReload: true
            )
        }
        watcher.setCancelHandler {
            close(fd)
        }
        watcher.resume()
        return watcher
    }

    private func replaceActivityObserver(
        _ existingObserver: NSObjectProtocol?,
        name: Notification.Name,
        sendNotifications: Bool,
        refreshAIStats: Bool,
        requiresStatusReload: Bool,
        refreshImmediatelyFromRuntime: Bool
    ) -> NSObjectProtocol {
        if let existingObserver {
            NotificationCenter.default.removeObserver(existingObserver)
        }

        return NotificationCenter.default.addObserver(
            forName: name,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            if refreshImmediatelyFromRuntime {
                Task { @MainActor [weak self] in
                    self?.refreshProjectActivityImmediatelyFromRuntime(
                        sendNotifications: sendNotifications
                    )
                }
            }
            self?.queueProjectActivityRefresh(
                sendNotifications: sendNotifications,
                refreshAIStats: refreshAIStats,
                requiresStatusReload: requiresStatusReload
            )
        }
    }

    nonisolated private func queueProjectActivityRefresh(
        sendNotifications: Bool,
        refreshAIStats: Bool,
        requiresStatusReload: Bool
    ) {
        Task { @MainActor [weak self] in
            self?.scheduleProjectActivityRefresh(
                sendNotifications: sendNotifications,
                refreshAIStats: refreshAIStats,
                requiresStatusReload: requiresStatusReload
            )
        }
    }

    private func refreshProjectActivityImmediatelyFromRuntime(sendNotifications: Bool) {
        refreshProjectActivity(
            sendNotifications: sendNotifications,
            useCachedStatusesOnly: true
        )
    }

    private func scheduleProjectActivityRefresh(
        sendNotifications: Bool,
        refreshAIStats: Bool,
        requiresStatusReload: Bool
    ) {
        pendingActivityRefreshShouldNotify = pendingActivityRefreshShouldNotify || sendNotifications
        pendingActivityRefreshShouldRefreshAIStats = pendingActivityRefreshShouldRefreshAIStats || refreshAIStats
        pendingActivityRefreshRequiresStatusReload = pendingActivityRefreshRequiresStatusReload || requiresStatusReload

        guard pendingActivityRefreshTask == nil else {
            return
        }

        pendingActivityRefreshTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(120))
            await MainActor.run {
                guard let self else {
                    return
                }

                let pendingRequest = self.drainPendingActivityRefreshRequest()
                self.pendingActivityRefreshTask = nil

                self.refreshProjectActivity(
                    sendNotifications: pendingRequest.sendNotifications,
                    useCachedStatusesOnly: !pendingRequest.requiresStatusReload
                )
                if pendingRequest.refreshAIStats, self.rightPanel == .aiStats {
                    self.refreshAIStatsIfNeeded()
                }
            }
        }
    }

    func refreshProjectActivity(
        sendNotifications: Bool,
        useCachedStatusesOnly: Bool = false
    ) {
        let currentProjects = projects
        let payloads = projectActivityPayloads(
            useCachedStatusesOnly: useCachedStatusesOnly,
            projects: currentProjects
        )
        if useCachedStatusesOnly == false {
            runtimeIngressService.importRuntime(projects: currentProjects)
        }

        var phases: [UUID: ProjectActivityPhase] = [:]
        let previousPhases = activityByProjectID

        for project in projects {
            let payload = payloads[project.id]
            let completionPhase = observedCompletionPhase(projectID: project.id, payload: payload)
            let phase = resolvedProjectActivityPhase(projectID: project.id, payload: payload)

            phases[project.id] = phase

            logProjectActivityTransitionIfNeeded(
                projectName: project.name,
                previousPhase: previousPhases[project.id],
                nextPhase: phase
            )

            if sendNotifications {
                handleWaitingInputNotificationIfNeeded(project: project, phase: phase)
                handleCompletionNotificationIfNeeded(project: project, completionPhase: completionPhase, payload: payload)
            }
        }

        if activityByProjectID != phases {
            activityByProjectID = phases
            markActivityStateChanged()
        }
    }

    private func drainPendingActivityRefreshRequest() -> (
        sendNotifications: Bool,
        refreshAIStats: Bool,
        requiresStatusReload: Bool
    ) {
        defer {
            pendingActivityRefreshShouldNotify = false
            pendingActivityRefreshShouldRefreshAIStats = false
            pendingActivityRefreshRequiresStatusReload = false
        }

        return (
            sendNotifications: pendingActivityRefreshShouldNotify,
            refreshAIStats: pendingActivityRefreshShouldRefreshAIStats,
            requiresStatusReload: pendingActivityRefreshRequiresStatusReload
        )
    }

    private func projectActivityPayloads(
        useCachedStatusesOnly: Bool,
        projects: [Project]
    ) -> [UUID: ProjectActivityPayload] {
        if useCachedStatusesOnly {
            return cachedActivityPayloadByProjectID
        }

        let latestPayloads = activityService.loadStatuses(projects: projects)
        cachedActivityPayloadByProjectID = latestPayloads
        return latestPayloads
    }

    private func resolvedCachedProjectActivityPhase(
        payload: ProjectActivityPayload?,
        projectID: UUID
    ) -> ProjectActivityPhase {
        guard let payload else {
            return .idle
        }

        guard isRealtimeAITool(payload.tool) == false else {
            activityService.clearStatus(for: projectID)
            cachedActivityPayloadByProjectID[projectID] = nil
            return .idle
        }

        return sanitizedCachedActivityPhase(activityService.phase(for: payload))
    }

    private func resolvedProjectActivityPhase(
        projectID: UUID,
        payload: ProjectActivityPayload?
    ) -> ProjectActivityPhase {
        discardStaleCompletionPresentationIfNeeded(projectID: projectID)
        let cachedPhase = resolvedCachedProjectActivityPhase(payload: payload, projectID: projectID)
        let runtimePhase = aiSessionStore.projectPhase(projectID: projectID)
        let completionPhase = completionPresentationPhase(for: projectID)
        return Self.resolveDisplayedActivityPhase(
            runtimePhase: runtimePhase,
            cachedPhase: cachedPhase,
            completionPhase: completionPhase,
            cachedPayloadTool: payload?.tool,
            hasLiveRuntimeSessions: aiSessionStore.hasLiveSessions(projectID: projectID),
            isRealtimeTool: isRealtimeAITool
        )
    }

    private func debugActivityDescription(_ phase: ProjectActivityPhase) -> String {
        switch phase {
        case .idle:
            return "idle"
        case .running(let tool):
            return "running:\(tool)"
        case .waitingInput(let tool):
            return "waiting-input:\(tool)"
        case .completed(let tool, _, let exitCode):
            return "completed:\(tool):\(exitCode.map(String.init) ?? "nil")"
        }
    }

    private func shouldUseRuntimeActivityOnly(projectID: UUID) -> Bool {
        Self.shouldUseRuntimeActivityOnly(
            cachedPhase: activityByProjectID[projectID],
            cachedPayloadTool: cachedActivityPayloadByProjectID[projectID]?.tool,
            hasLiveRuntimeSessions: aiSessionStore.hasLiveSessions(projectID: projectID),
            isRealtimeTool: isRealtimeAITool
        )
    }

    private func cachedActivityDescription(for projectID: UUID) -> String {
        activityByProjectID[projectID].map(debugActivityDescription) ?? "nil"
    }

    private func completionPresentationPhase(for projectID: UUID) -> ProjectActivityPhase {
        guard let presentation = completionPresentationByProjectID[projectID] else {
            return .idle
        }

        return .completed(
            tool: presentation.tool,
            finishedAt: presentation.finishedAt,
            exitCode: presentation.exitCode
        )
    }

    private func completionActivityDescription(for projectID: UUID) -> String {
        debugActivityDescription(completionPresentationPhase(for: projectID))
    }

    @discardableResult
    func dismissCompletionPresentationIfNeeded(
        projectID: UUID,
        reason: String
    ) -> Bool {
        guard completionPresentationByProjectID[projectID] != nil else {
            return false
        }

        completionPresentationByProjectID[projectID] = nil
        let payload = cachedActivityPayloadByProjectID[projectID]
        activityByProjectID[projectID] = resolvedProjectActivityPhase(
            projectID: projectID,
            payload: payload
        )
        debugLog.log(
            "activity-ui",
            "clear-completed project=\(projectID.uuidString) reason=\(reason)"
        )
        markActivityStateChanged()
        return true
    }

    private func discardStaleCompletionPresentationIfNeeded(projectID: UUID) {
        guard let presentation = completionPresentationByProjectID[projectID],
              let latestActiveStartedAt = aiSessionStore.latestActiveStartedAt(projectID: projectID),
              latestActiveStartedAt > presentation.finishedAt else {
            return
        }

        completionPresentationByProjectID[projectID] = nil
        debugLog.log(
            "activity-ui",
            "drop-stale-completed project=\(projectID.uuidString) startedAt=\(latestActiveStartedAt.timeIntervalSince1970)"
        )
    }

    private func observedCompletionPhase(
        projectID: UUID,
        payload: ProjectActivityPayload?
    ) -> ProjectActivityPhase {
        if let runtimeCompletion = aiSessionStore.completedPhase(projectID: projectID) {
            return runtimeCompletion
        }

        let cachedPhase = resolvedCachedProjectActivityPhase(payload: payload, projectID: projectID)
        if case .completed = cachedPhase {
            return cachedPhase
        }

        return .idle
    }

    private func logProjectActivityTransitionIfNeeded(
        projectName: String,
        previousPhase: ProjectActivityPhase?,
        nextPhase: ProjectActivityPhase
    ) {
        guard previousPhase != nextPhase else {
            return
        }
        debugLog.log(
            "activity",
            "project=\(projectName) phase=\(debugActivityDescription(nextPhase))"
        )
    }

    private func logActivityPhaseResolution(projectID: UUID, source: String, phase: ProjectActivityPhase) {
        let projectName = projects.first(where: { $0.id == projectID })?.name ?? projectID.uuidString
        let runtimeSummary = aiSessionStore.debugSummary(projectID: projectID)
        debugLog.log(
            "activity-phase",
            "project=\(projectName) source=\(source) phase=\(debugActivityDescription(phase)) cached=\(cachedActivityDescription(for: projectID)) completion=\(completionActivityDescription(for: projectID)) runtime=\(runtimeSummary)"
        )
    }

    private func logApplicationActivationActivity() {
        guard let project = selectedProject else {
            debugLog.log("app-activation", "selectedProject=nil")
            return
        }

        let runtimePhase = aiSessionStore.projectPhase(projectID: project.id)
        let runtimeSummary = aiSessionStore.debugSummary(projectID: project.id)
        debugLog.log(
            "app-activation",
            "project=\(project.name) runtimePhase=\(debugActivityDescription(runtimePhase)) cached=\(cachedActivityDescription(for: project.id)) completion=\(completionActivityDescription(for: project.id)) runtime=\(runtimeSummary)"
        )
    }

    private func isRealtimeAITool(_ tool: String) -> Bool {
        runtimeIngressService.isRealtimeTool(tool)
    }

    private func sanitizedCachedActivityPhase(_ phase: ProjectActivityPhase?) -> ProjectActivityPhase {
        guard let phase else {
            return .idle
        }

        switch phase {
        case .running(let tool) where isRealtimeAITool(tool):
            return .idle
        case .waitingInput(let tool) where isRealtimeAITool(tool):
            return .idle
        default:
            return phase
        }
    }

    nonisolated static func resolveDisplayedActivityPhase(
        runtimePhase: ProjectActivityPhase,
        cachedPhase: ProjectActivityPhase,
        completionPhase: ProjectActivityPhase,
        cachedPayloadTool: String?,
        hasLiveRuntimeSessions: Bool,
        isRealtimeTool: (String) -> Bool
    ) -> ProjectActivityPhase {
        if runtimePhase != .idle {
            return runtimePhase
        }
        if completionPhase != .idle {
            return completionPhase
        }
        if shouldUseRuntimeActivityOnly(
            cachedPhase: cachedPhase,
            cachedPayloadTool: cachedPayloadTool,
            hasLiveRuntimeSessions: hasLiveRuntimeSessions,
            isRealtimeTool: isRealtimeTool
        ) {
            return .idle
        }
        if cachedPhase != .idle {
            return cachedPhase
        }
        return .idle
    }

    nonisolated static func shouldUseRuntimeActivityOnly(
        cachedPhase: ProjectActivityPhase?,
        cachedPayloadTool: String?,
        hasLiveRuntimeSessions: Bool,
        isRealtimeTool: (String) -> Bool
    ) -> Bool {
        if hasLiveRuntimeSessions {
            return true
        }

        if let cachedPayloadTool,
           isRealtimeTool(cachedPayloadTool) {
            return true
        }

        if let tool = activityToolName(for: cachedPhase),
           isRealtimeTool(tool) {
            return true
        }

        return false
    }

    nonisolated static func activityToolName(for phase: ProjectActivityPhase?) -> String? {
        guard let phase else {
            return nil
        }

        switch phase {
        case .idle:
            return nil
        case .running(let tool), .waitingInput(let tool), .completed(let tool, _, _):
            return tool
        }
    }

    private func markActivityStateChanged() {
        activityRenderVersion &+= 1
        updateDockBadge()
    }

    private func waitingInputNotificationToken(
        tool: String,
        context: AISessionStore.WaitingInputContext
    ) -> String {
        let timestamp = Int(context.updatedAt * 1000)
        return [
            tool,
            String(timestamp),
            context.notificationType ?? "",
            context.targetToolName ?? "",
            context.message ?? ""
        ].joined(separator: "|")
    }

    private func shouldNotifyForWaitingInput(
        _ context: AISessionStore.WaitingInputContext
    ) -> Bool {
        context.notificationType == "permission-request"
    }

    private func handleWaitingInputNotificationIfNeeded(project: Project, phase: ProjectActivityPhase) {
        guard case .waitingInput(let tool) = phase,
              let context = aiSessionStore.waitingInputContext(projectID: project.id),
              shouldNotifyForWaitingInput(context) else {
            lastWaitingInputTokenByProjectID[project.id] = nil
            return
        }

        let token = waitingInputNotificationToken(tool: tool, context: context)
        guard lastWaitingInputTokenByProjectID[project.id] != token else {
            return
        }

        lastWaitingInputTokenByProjectID[project.id] = token
        activityService.notifyNeedsInput(
            projectName: project.name,
            tool: tool,
            notificationType: context.notificationType,
            targetToolName: context.targetToolName,
            message: context.message
        )
    }

    private func handleCompletionNotificationIfNeeded(
        project: Project,
        completionPhase: ProjectActivityPhase,
        payload: ProjectActivityPayload?
    ) {
        guard case .completed(let tool, let finishedAt, let exitCode) = completionPhase else {
            return
        }

        let token = completionActivityToken(payload: payload, tool: tool, finishedAt: finishedAt)

        guard lastCompletionTokenByProjectID[project.id] != token else {
            return
        }

        lastCompletionTokenByProjectID[project.id] = token
        completionPresentationByProjectID[project.id] = ProjectCompletionPresentation(
            token: token,
            tool: tool,
            finishedAt: finishedAt,
            exitCode: exitCode
        )
        activityService.notifyCompletion(
            projectName: project.name,
            tool: tool,
            exitCode: exitCode,
            settings: appSettings.notifications
        )
    }

    private func completionActivityToken(
        payload: ProjectActivityPayload?,
        tool: String,
        finishedAt: Date
    ) -> String {
        if let payload {
            return activityService.completionToken(for: payload)
        }

        return "realtime-\(tool)-\(Int(finishedAt.timeIntervalSince1970 * 1000))"
    }
}
