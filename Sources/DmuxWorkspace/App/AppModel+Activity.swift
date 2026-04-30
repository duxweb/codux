import AppKit
import Darwin
import Foundation

extension AppModel {
    func activityPhase(for projectID: UUID) -> ProjectActivityPhase {
        discardStaleCompletionPresentationIfNeeded(projectID: projectID)
        let runtimePhase = aiSessionStore.projectPhase(projectID: projectID)
        let completionPhase = completionPresentationPhase(for: projectID)
        let resolvedPhase = Self.resolveDisplayedActivityPhase(
            runtimePhase: runtimePhase,
            completionPhase: completionPhase
        )

        let source: String
        if resolvedPhase == runtimePhase {
            source = "runtime"
        } else if resolvedPhase == completionPhase, completionPhase != .idle {
            source = "ui-completion"
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

    func observeMemoryExtractionStatusChanges() {
        if let memoryExtractionStatusObserver {
            NotificationCenter.default.removeObserver(memoryExtractionStatusObserver)
        }

        memoryExtractionStatusObserver = NotificationCenter.default.addObserver(
            forName: .dmuxMemoryExtractionStatusDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            let snapshot = notification.userInfo?["snapshot"] as? MemoryExtractionStatusSnapshot
            Task { @MainActor [weak self] in
                guard let snapshot else {
                    return
                }
                self?.memoryExtractionStatus = snapshot
            }
        }

        Task { [weak self] in
            guard let self else {
                return
            }
            let snapshot = await self.memoryCoordinator.currentStatusSnapshot()
            await MainActor.run {
                self.memoryExtractionStatus = snapshot
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
            case .loading:
                activityLabel = "loading"
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
        runtimeIngressService.startWatching()
        if let runtimeBridgeObserver {
            NotificationCenter.default.removeObserver(runtimeBridgeObserver)
        }

        runtimeBridgeObserver = NotificationCenter.default.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else {
                    return
                }
                self.refreshProjectActivity(sendNotifications: true)
                self.scheduleProjectActivityRefresh(sendNotifications: true)
            }
        }
    }

    func refreshAIStatsIfNeeded() {
        aiStatsStore.refreshIfNeeded(project: selectedProject, projects: projects, selectedSessionID: selectedSessionID)
    }

    private func scheduleProjectActivityRefresh(
        sendNotifications: Bool
    ) {
        pendingActivityRefreshShouldNotify = pendingActivityRefreshShouldNotify || sendNotifications

        guard pendingActivityRefreshTask == nil else {
            return
        }

        pendingActivityRefreshTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(120))
            await MainActor.run {
                guard let self else {
                    return
                }

                let sendNotifications = self.pendingActivityRefreshShouldNotify
                self.pendingActivityRefreshShouldNotify = false
                self.pendingActivityRefreshTask = nil

                self.refreshProjectActivity(sendNotifications: sendNotifications)
            }
        }
    }

    func refreshProjectActivity(sendNotifications: Bool) {
        let currentProjects = projects
        runtimeIngressService.importRuntime(projects: currentProjects)

        var phases: [UUID: ProjectActivityPhase] = [:]
        let previousPhases = activityByProjectID

        for project in projects {
            let completionPhase = observedCompletionPhase(projectID: project.id)
            let phase = resolvedProjectActivityPhase(projectID: project.id)

            phases[project.id] = phase

            logProjectActivityTransitionIfNeeded(
                projectName: project.name,
                previousPhase: previousPhases[project.id],
                nextPhase: phase
            )

            if sendNotifications {
                handleWaitingInputNotificationIfNeeded(project: project, phase: phase)
                handleCompletionNotificationIfNeeded(project: project, completionPhase: completionPhase)
            }
        }

        if activityByProjectID != phases {
            activityByProjectID = phases
            markActivityStateChanged()
        }
    }

    func resolvedProjectActivityPhase(projectID: UUID) -> ProjectActivityPhase {
        discardStaleCompletionPresentationIfNeeded(projectID: projectID)
        if projectHasLoadingTerminal(projectID: projectID) {
            return .loading
        }
        let runtimePhase = aiSessionStore.projectPhase(projectID: projectID)
        let completionPhase = completionPresentationPhase(for: projectID)
        return Self.resolveDisplayedActivityPhase(
            runtimePhase: runtimePhase,
            completionPhase: completionPhase
        )
    }

    private func debugActivityDescription(_ phase: ProjectActivityPhase) -> String {
        switch phase {
        case .idle:
            return "idle"
        case .loading:
            return "loading"
        case .running(let tool):
            return "running:\(tool)"
        case .waitingInput(let tool):
            return "waiting-input:\(tool)"
        case .completed(let tool, _, let exitCode):
            return "completed:\(tool):\(exitCode.map(String.init) ?? "nil")"
        }
    }

    private func projectHasLoadingTerminal(projectID: UUID) -> Bool {
        guard !terminalLoadingSessionIDs.isEmpty,
              let workspace = workspaces.first(where: { $0.projectID == projectID }) else {
            return false
        }
        return workspace.sessions.contains { terminalLoadingSessionIDs.contains($0.id) }
    }

    private func completionPresentationPhase(for projectID: UUID) -> ProjectActivityPhase {
        guard let presentation = activityCacheByProjectID[projectID]?.completionPresentation else {
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
        guard clearCompletionPresentation(projectID: projectID) else {
            return false
        }
        activityByProjectID[projectID] = resolvedProjectActivityPhase(projectID: projectID)
        debugLog.log(
            "activity-ui",
            "clear-completed project=\(projectID.uuidString) reason=\(reason)"
        )
        markActivityStateChanged()
        return true
    }

    private func discardStaleCompletionPresentationIfNeeded(projectID: UUID) {
        guard let presentation = activityCacheByProjectID[projectID]?.completionPresentation,
              let latestActiveStartedAt = aiSessionStore.latestActiveStartedAt(projectID: projectID),
              latestActiveStartedAt > presentation.finishedAt else {
            return
        }
        _ = clearCompletionPresentation(projectID: projectID)
        debugLog.log(
            "activity-ui",
            "drop-stale-completed project=\(projectID.uuidString) startedAt=\(latestActiveStartedAt.timeIntervalSince1970)"
        )
    }

    private func observedCompletionPhase(projectID: UUID) -> ProjectActivityPhase {
        if let runtimeCompletion = aiSessionStore.completedPhase(projectID: projectID) {
            return runtimeCompletion
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
        let token = "\(source)|\(debugActivityDescription(phase))"
        guard activityCacheByProjectID[projectID]?.lastResolutionLogToken != token else {
            return
        }
        updateActivityCache(projectID: projectID) { cache in
            cache.lastResolutionLogToken = token
        }

        let projectName = projects.first(where: { $0.id == projectID })?.name ?? projectID.uuidString
        let runtimeSummary = aiSessionStore.debugSummary(projectID: projectID)
        debugLog.log(
            "activity-phase",
            "project=\(projectName) source=\(source) phase=\(debugActivityDescription(phase)) completion=\(completionActivityDescription(for: projectID)) runtime=\(runtimeSummary)"
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
            "project=\(project.name) runtimePhase=\(debugActivityDescription(runtimePhase)) completion=\(completionActivityDescription(for: project.id)) runtime=\(runtimeSummary)"
        )
    }

    nonisolated static func resolveDisplayedActivityPhase(
        runtimePhase: ProjectActivityPhase,
        completionPhase: ProjectActivityPhase
    ) -> ProjectActivityPhase {
        if runtimePhase != .idle {
            return runtimePhase
        }
        if completionPhase != .idle {
            return completionPhase
        }
        return .idle
    }

    func markActivityStateChanged() {
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
            updateActivityCache(projectID: project.id) { cache in
                cache.lastWaitingInputToken = nil
            }
            return
        }

        let token = waitingInputNotificationToken(tool: tool, context: context)
        guard activityCacheByProjectID[project.id]?.lastWaitingInputToken != token else {
            return
        }

        updateActivityCache(projectID: project.id) { cache in
            cache.lastWaitingInputToken = token
        }
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
        completionPhase: ProjectActivityPhase
    ) {
        guard case .completed(let tool, let finishedAt, let exitCode) = completionPhase else {
            return
        }

        let token = aiSessionStore.completedNotificationToken(projectID: project.id)
            ?? completionActivityToken(tool: tool, finishedAt: finishedAt)

        guard activityCacheByProjectID[project.id]?.lastCompletionToken != token else {
            return
        }

        updateActivityCache(projectID: project.id) { cache in
            cache.lastCompletionToken = token
            cache.completionPresentation = ProjectCompletionPresentation(
                tool: tool,
                finishedAt: finishedAt,
                exitCode: exitCode
            )
        }
        activityService.notifyCompletion(
            projectName: project.name,
            tool: tool,
            exitCode: exitCode,
            settings: appSettings.notifications
        )
    }

    private func completionActivityToken(
        tool: String,
        finishedAt: Date
    ) -> String {
        return "realtime-\(tool)-\(Int(finishedAt.timeIntervalSince1970 * 1000))"
    }

    private func updateActivityCache(
        projectID: UUID,
        update: (inout ProjectActivityCache) -> Void
    ) {
        var cache = activityCacheByProjectID[projectID] ?? ProjectActivityCache()
        update(&cache)
        if cache == ProjectActivityCache() {
            activityCacheByProjectID[projectID] = nil
        } else {
            activityCacheByProjectID[projectID] = cache
        }
    }

    @discardableResult
    private func clearCompletionPresentation(projectID: UUID) -> Bool {
        guard activityCacheByProjectID[projectID]?.completionPresentation != nil else {
            return false
        }
        updateActivityCache(projectID: projectID) { cache in
            cache.completionPresentation = nil
        }
        return true
    }
}
