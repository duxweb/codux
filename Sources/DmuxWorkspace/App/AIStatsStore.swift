import Darwin
import Foundation
import Observation

extension Notification.Name {
    static let dmuxAIRuntimeActivityPulse = Notification.Name("dmuxAIRuntimeActivityPulse")
}

@MainActor
@Observable
final class AIStatsStore {
    private enum RefreshTrigger: Equatable {
        case initial
        case manual
        case automatic
        case background
    }

    private struct CachedAIStatsPanelEntry {
        var projectID: UUID
        var state: AIStatsPanelState
        var updatedAt: Date
    }

    var state = AIStatsPanelState.empty
    var refreshState: PanelRefreshState = .idle
    var isAutomaticRefreshInProgress = false
    var renderVersion: UInt64 = 0

    private let aiUsageService = AIUsageService()
    private let aiUsageStore = AIUsageStore()
    private let runtimeIngressService = AIRuntimeIngressService.shared
    private let aiSessionStore = AISessionStore.shared
    private let logger = AppDebugLog.shared
    private var refreshTasks: [UUID: Task<Void, Never>] = [:]
    private var indexingStatusByProjectID: [UUID: AIIndexingStatus] = [:]
    private var panelStateByProjectID: [UUID: AIStatsPanelState] = [:]
    private var refreshStateByProjectID: [UUID: PanelRefreshState] = [:]
    private var automaticRefreshInProgressByProjectID: [UUID: Bool] = [:]
    private var manualRefreshInProgressByProjectID: [UUID: Bool] = [:]
    private var lastCompletedRefreshAtByProjectID: [UUID: Date] = [:]
    private var cachedPanels = RecentProjectCache<CachedAIStatsPanelEntry>()
    private var refreshTimer: Timer?
    private var backgroundRefreshTimer: Timer?
    private var runtimeBridgeObserver: NSObjectProtocol?
    private var terminalFocusObserver: NSObjectProtocol?
    private var terminalOutputObserver: NSObjectProtocol?
    private var pendingRuntimeBridgeRefreshTask: Task<Void, Never>?
    private var pendingRuntimeBridgeRefreshShouldForce = false
    private var currentProjectID: UUID?
    private var currentSelectedSessionID: UUID?
    private var currentProjects: [Project] = []
    private var panelVisibilityProvider: (@MainActor () -> Bool)?
    private var selectedProjectProvider: (@MainActor () -> Project?)?
    private var selectedSessionIDProvider: (@MainActor () -> UUID?)?
    private var projectsProvider: (@MainActor () -> [Project])?
    private var automaticRefreshInterval: TimeInterval = 180
    private var backgroundRefreshInterval: TimeInterval = 600
    private let debugAIFocus = ProcessInfo.processInfo.environment["DMUX_DEBUG_AI_FOCUS"] == "1"

    private func effectiveSessionID(_ selectedSessionID: UUID?) -> UUID? {
        let focusedSessionID = DmuxTerminalBackend.shared.registry.focusedSessionID()
        let resolvedSessionID = focusedSessionID ?? selectedSessionID
        if debugAIFocus {
            let registrySnapshot = DmuxTerminalBackend.shared.registry.debugSnapshot()
            print("[AIStats] focusedSessionID=\(focusedSessionID?.uuidString ?? "nil") selectedSessionID=\(selectedSessionID?.uuidString ?? "nil") resolvedSessionID=\(resolvedSessionID?.uuidString ?? "nil") registry=[\(registrySnapshot)]")
        }
        return resolvedSessionID
    }

    func startTimers(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        panelVisibilityProvider = isPanelVisible
        selectedProjectProvider = selectedProject
        selectedSessionIDProvider = selectedSessionID
        projectsProvider = projects

        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: max(30, automaticRefreshInterval), repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, isPanelVisible(), let project = selectedProject() else {
                    return
                }
                let sessionID = self.effectiveSessionID(selectedSessionID())
                self.automaticRefreshInProgressByProjectID[project.id] = true
                self.manualRefreshInProgressByProjectID[project.id] = false
                self.syncCurrentAutomaticRefreshFlag()
                self.refresh(
                    project: project,
                    projects: projects(),
                    selectedSessionID: sessionID,
                    force: true,
                    trigger: .automatic
                )
            }
        }

        backgroundRefreshTimer?.invalidate()
        backgroundRefreshTimer = Timer.scheduledTimer(withTimeInterval: backgroundRefreshInterval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else {
                    return
                }
                self.runBackgroundRefreshTick(
                    isPanelVisible: isPanelVisible,
                    selectedProject: selectedProject,
                    selectedSessionID: selectedSessionID,
                    projects: projects
                )
            }
        }
        logger.log("history-refresh", "background-worker start interval=\(Int(backgroundRefreshInterval))s")

        startRuntimeBridgeObserver(
            isPanelVisible: isPanelVisible,
            selectedProject: selectedProject,
            selectedSessionID: selectedSessionID,
            projects: projects
        )

        startTerminalFocusObserver(
            isPanelVisible: isPanelVisible,
            selectedProject: selectedProject,
            selectedSessionID: selectedSessionID,
            projects: projects
        )

        startTerminalOutputObserver(
            isPanelVisible: isPanelVisible,
            selectedProject: selectedProject,
            selectedSessionID: selectedSessionID,
            projects: projects
        )

    }

    func configureIntervals(automatic: TimeInterval, background: TimeInterval) {
        automaticRefreshInterval = max(30, automatic)
        backgroundRefreshInterval = max(60, background)
        if let panelVisibilityProvider, let selectedProjectProvider, let selectedSessionIDProvider, let projectsProvider {
            startTimers(
                isPanelVisible: panelVisibilityProvider,
                selectedProject: selectedProjectProvider,
                selectedSessionID: selectedSessionIDProvider,
                projects: projectsProvider
            )
        }
    }

    func refreshLocalizedStatusTexts() {
        let projectIDs = Set(indexingStatusByProjectID.keys)
            .union(panelStateByProjectID.keys)
            .union(cachedPanels.projectIDs)

        for projectID in projectIDs {
            if let status = indexingStatusByProjectID[projectID] {
                indexingStatusByProjectID[projectID] = relocalizedStatus(status)
            }
            if var panelState = panelStateByProjectID[projectID] {
                panelState.indexingStatus = relocalizedStatus(panelState.indexingStatus)
                let nextRefreshState = refreshStateByProjectID[projectID] ?? .idle
                storeState(panelState, refreshState: nextRefreshState, for: projectID, updateCurrent: currentProjectID == projectID)
            }
        }
    }

    func refreshIfNeeded(project: Project?, projects: [Project], selectedSessionID: UUID?) {
        let selectedSessionID = effectiveSessionID(selectedSessionID)
        currentProjects = projects
        currentProjectID = project?.id
        currentSelectedSessionID = selectedSessionID
        syncCurrentAutomaticRefreshFlag()
        guard let project else {
            state = .empty
            refreshState = .idle
            isAutomaticRefreshInProgress = false
            return
        }

        _ = ingestRuntime(project: project, projects: projects)
        let displayLiveSnapshots = aiSessionStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = aiSessionStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
        let cachedState = cachedState(for: project.id)
        let refreshDecision = automaticRefreshDecision(
            projectID: project.id,
            state: cachedState,
            interval: automaticRefreshInterval
        )
        let shouldRefresh = refreshDecision.shouldRefresh
        let projectRefreshState: PanelRefreshState
        if refreshTasks[project.id] != nil {
            projectRefreshState = automaticRefreshInProgressByProjectID[project.id] == true ? .showingCached : .refreshing
        } else if shouldRefresh {
            projectRefreshState = cachedState == nil ? .refreshing : .showingCached
        } else {
            projectRefreshState = normalizedRestingRefreshState(refreshStateByProjectID[project.id])
        }
        if let cachedState {
            let status = indexingStatusByProjectID[project.id] ?? cachedState.indexingStatus
            var nextState = aiUsageService.snapshotBackedPanelState(
                project: project,
                liveSnapshots: summaryLiveSnapshots,
                currentSnapshot: currentSnapshot,
                status: status
            )
            nextState.liveSnapshots = displayLiveSnapshots
            nextState.indexingStatus = status
            storeState(nextState, refreshState: projectRefreshState, for: project.id, updateCurrent: true)
        } else {
            var emptyState = aiUsageService.fastPanelState(
                project: project,
                liveSnapshots: summaryLiveSnapshots,
                currentSnapshot: currentSnapshot
            )
            emptyState.liveSnapshots = displayLiveSnapshots
            storeState(emptyState, refreshState: .refreshing, for: project.id, updateCurrent: true)
        }

        if shouldRefresh {
            let trigger: RefreshTrigger = cachedState == nil ? .initial : .automatic
            if trigger == .automatic {
                automaticRefreshInProgressByProjectID[project.id] = true
                manualRefreshInProgressByProjectID[project.id] = false
                syncCurrentAutomaticRefreshFlag()
            }
            refresh(
                project: project,
                projects: projects,
                selectedSessionID: selectedSessionID,
                force: false,
                trigger: trigger
            )
        }

    }

    func syncSelection(project: Project?, projects: [Project], selectedSessionID: UUID?) {
        let selectedSessionID = effectiveSessionID(selectedSessionID)
        currentProjects = projects
        currentProjectID = project?.id
        currentSelectedSessionID = selectedSessionID
        syncCurrentAutomaticRefreshFlag()

        guard let project else {
            state = .empty
            refreshState = .idle
            isAutomaticRefreshInProgress = false
            return
        }

        _ = ingestRuntime(project: project, projects: projects)
        let displayLiveSnapshots = aiSessionStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = aiSessionStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
        let status = indexingStatusByProjectID[project.id]
            ?? cachedState(for: project.id)?.indexingStatus
            ?? .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))

        let hasCachedState = cachedState(for: project.id) != nil

        if hasCachedState {
            var nextState = aiUsageService.snapshotBackedPanelState(
                project: project,
                liveSnapshots: summaryLiveSnapshots,
                currentSnapshot: currentSnapshot,
                status: status
            )
            nextState.liveSnapshots = displayLiveSnapshots
            nextState.indexingStatus = status
            storeState(nextState, refreshState: normalizedRestingRefreshState(refreshStateByProjectID[project.id]), for: project.id, updateCurrent: true)
        } else {
            var nextState = aiUsageService.fastPanelState(
                project: project,
                liveSnapshots: summaryLiveSnapshots,
                currentSnapshot: currentSnapshot
            )
            nextState.liveSnapshots = displayLiveSnapshots
            nextState.indexingStatus = status
            storeState(nextState, refreshState: .idle, for: project.id, updateCurrent: true)
        }

        logger.log(
            "ai-panel-bridge",
            "phase=selection-sync project=\(project.id.uuidString) selected=\(selectedSessionID?.uuidString ?? "nil") cached=\(hasCachedState) live=\(displayLiveSnapshots.count)"
        )

    }

    func refreshCurrent(project: Project?, projects: [Project], selectedSessionID: UUID?) {
        guard let project else { return }
        let selectedSessionID = effectiveSessionID(selectedSessionID)
        currentProjects = projects
        currentProjectID = project.id
        currentSelectedSessionID = selectedSessionID
        automaticRefreshInProgressByProjectID[project.id] = false
        manualRefreshInProgressByProjectID[project.id] = true
        syncCurrentAutomaticRefreshFlag()
        refresh(
            project: project,
            projects: projects,
            selectedSessionID: selectedSessionID,
            force: true,
            trigger: .manual
        )
    }

    func totalTodayTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            let liveOrCached = cachedState(for: project.id)
            if let liveOrCached {
                return partial + resolvedTodayTotalTokens(for: liveOrCached)
            }

            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                return partial + resolvedTodayTotalTokens(
                    summary: indexed.projectSummary.todayTotalTokens,
                    timeBuckets: indexed.todayTimeBuckets,
                    heatmap: indexed.heatmap
                )
            }

            return partial
        }
    }

    func totalAllTimeTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                return partial + indexed.heatmap.reduce(0) { $0 + $1.totalTokens }
            }
            return partial
        }
    }

    func petStatsAcrossProjects(_ projects: [Project], claimedAt: Date?) -> PetStats {
        guard let claimedAt else {
            return .neutral
        }

        var sessions: [AISessionSummary] = []

        for project in projects {
            guard let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) else { continue }
            sessions.append(contentsOf: indexed.sessions.filter { $0.lastSeenAt >= claimedAt })
        }

        return Self.computePetStats(from: sessions)
    }

    static func computePetStats(from sessions: [AISessionSummary]) -> PetStats {
        guard !sessions.isEmpty else { return .neutral }

        let totalRequests = sessions.reduce(0) { $0 + $1.requestCount }
        let totalTokens   = sessions.reduce(0) { $0 + $1.totalTokens }
        let totalSecs     = sessions.reduce(0) { $0 + $1.activeDurationSeconds }
        let sessionCount  = max(1, sessions.count)

        let avgTokPerReq = totalRequests > 0 ? Double(totalTokens) / Double(totalRequests) : 0
        let reqPerHour   = totalSecs > 0 ? Double(totalRequests) / (Double(totalSecs) / 3600.0) : 0
        let shortCount   = sessions.filter { $0.activeDurationSeconds < 300 }.count
        let shortRatio   = Double(shortCount) / Double(sessions.count)
        let nightCount   = sessions.filter {
            let h = Calendar.current.component(.hour, from: $0.firstSeenAt)
            return h >= 22 || h < 6
        }.count
        let nightRatio   = Double(nightCount) / Double(sessionCount)
        let maxSecs      = sessions.map { $0.activeDurationSeconds }.max() ?? 0
        let avgSecs      = totalSecs / sessions.count
        let multiTurnSessions = sessions.filter { $0.requestCount >= 4 }
        let multiTurnRatio    = Double(multiTurnSessions.count) / Double(sessionCount)

        // Iterative-repair sessions: widened avgPerTurn window (200-3500) to
        // capture a broader range of debugging / refinement workflows.
        let iterativeRepairSessions = sessions.filter { s in
            guard s.requestCount >= 4, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return s.activeDurationSeconds >= 600 && avgPerTurn >= 200 && avgPerTurn <= 3_500
        }
        let repairSecs        = iterativeRepairSessions.reduce(0) { $0 + $1.activeDurationSeconds }
        let repairRatio       = min(1.0, Double(repairSecs) / Double(max(1, totalSecs)))
        let repairTokenBudget = iterativeRepairSessions.reduce(0) { $0 + $1.totalTokens }
        let adjustmentLoopCount = sessions.filter { s in
            guard s.requestCount >= 3, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return avgPerTurn >= 200 && avgPerTurn <= 2_800
        }.count

        // Scoring helpers.
        func logPts(_ value: Double, divisor: Double, weight: Double, cap: Double) -> Double {
            guard value > 0, divisor > 0, weight > 0 else { return 0 }
            return min(log1p(value / divisor) * weight, cap)
        }
        func ratioPts(_ value: Double, exponent: Double, weight: Double, cap: Double) -> Double {
            guard value > 0, exponent > 0, weight > 0 else { return 0 }
            return min(pow(value, exponent) * weight, cap)
        }

        // Shared growth (capped at 20) — provides a tiny baseline for any
        // active pet but cannot dominate or mask behavioral differences.
        let shared = logPts(Double(totalTokens), divisor: 250_000, weight: 16, cap: 20)

        // Wisdom — depth of thinking (avg tokens/request is the key signal).
        let wisdomScore =
            logPts(avgTokPerReq,       divisor: 400,    weight: 110, cap: 175) +
            logPts(Double(totalSecs),  divisor: 12_000, weight: 12,  cap: 24)  +
            shared

        // Chaos — speed and frequency (req/hour + short-session ratio).
        let chaosScore =
            logPts(reqPerHour,            divisor: 1.8, weight: 108, cap: 150) +
            ratioPts(shortRatio,          exponent: 0.68, weight: 62, cap: 62) +
            logPts(Double(totalRequests), divisor: 22,  weight: 26,  cap: 44)  +
            shared

        // Night — strictly zero when nightRatio < 0.10 (daytime users get 0).
        let nightScore: Double
        if nightRatio >= 0.10 {
            let nightTokens = Double(totalTokens) * max(0.15, nightRatio)
            nightScore =
                ratioPts(nightRatio,          exponent: 0.62, weight: 140, cap: 140) +
                logPts(Double(nightCount),    divisor: 3.5,   weight: 34,  cap: 68)  +
                logPts(nightTokens,           divisor: 120_000, weight: 14, cap: 28) +
                shared
        } else {
            nightScore = 0
        }

        // Stamina — endurance (longest session and average session length).
        let staminaScore =
            logPts(Double(maxSecs),  divisor: 800,    weight: 82, cap: 124) +
            logPts(Double(avgSecs),  divisor: 400,    weight: 80, cap: 100) +
            logPts(Double(totalSecs),divisor: 16_000, weight: 30, cap: 50)  +
            shared

        // Empathy — iterative refinement and debugging behaviour.
        let empathyScore =
            ratioPts(repairRatio,              exponent: 0.65, weight: 120, cap: 120) +
            ratioPts(multiTurnRatio,           exponent: 0.52, weight: 52,  cap: 52)  +
            logPts(Double(repairSecs) / 60,    divisor: 1_600, weight: 40,  cap: 72)  +
            logPts(Double(repairTokenBudget),  divisor: 120_000, weight: 24, cap: 46) +
            logPts(Double(adjustmentLoopCount),divisor: 1.8,   weight: 18,  cap: 30)  +
            shared

        return PetStats(
            wisdom:  max(0, Int(wisdomScore.rounded())),
            chaos:   max(0, Int(chaosScore.rounded())),
            night:   max(0, Int(nightScore.rounded())),
            stamina: max(0, Int(staminaScore.rounded())),
            empathy: max(0, Int(empathyScore.rounded()))
        )
    }

    func hiddenPetSpeciesChanceAcrossProjects(_ projects: [Project]) -> Double {
        let cutoff = Date().addingTimeInterval(-7 * 86400)
        var toolTotals: [String: Int] = [:]

        for project in projects {
            guard let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) else { continue }
            for session in indexed.sessions where session.lastSeenAt >= cutoff {
                guard let normalizedTool = Self.normalizedPetToolName(session.lastTool) else {
                    continue
                }
                toolTotals[normalizedTool, default: 0] += session.totalTokens
            }
        }

        return Self.hiddenPetSpeciesChance(forToolTotals: toolTotals)
    }

    static func hiddenPetSpeciesChance(forToolTotals toolTotals: [String: Int]) -> Double {
        toolTotals.keys.count >= 2 ? 0.50 : 0.15
    }

    private static func normalizedPetToolName(_ tool: String?) -> String? {
        guard let tool else { return nil }
        let normalized = tool.lowercased()
        if normalized.contains("claude") { return "claude" }
        if normalized.contains("codex") { return "codex" }
        if normalized.contains("gemini") { return "gemini" }
        if normalized.contains("opencode") { return "opencode" }
        return nil
    }

    func cancelCurrent(project: Project?, projects: [Project]) {
        guard let project,
              let task = refreshTasks[project.id] else {
            return
        }

        currentProjects = projects
        currentProjectID = project.id

        task.cancel()
        refreshTasks[project.id] = nil
        let cancelledStatus = AIIndexingStatus.cancelled(detail: String(localized: "ai.indexing.stopped", defaultValue: "Indexing stopped.", bundle: .module))
        indexingStatusByProjectID[project.id] = cancelledStatus
        _ = ingestRuntime(project: project, projects: projects)
        let displayLiveSnapshots = aiSessionStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = aiSessionStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: currentSelectedSessionID)
        var nextState = aiUsageService.snapshotBackedPanelState(
            project: project,
            liveSnapshots: summaryLiveSnapshots,
            currentSnapshot: currentSnapshot,
            status: cancelledStatus
        )
        nextState.liveSnapshots = displayLiveSnapshots
        storeState(nextState, refreshState: .idle, for: project.id, updateCurrent: true)
        automaticRefreshInProgressByProjectID[project.id] = false
        manualRefreshInProgressByProjectID[project.id] = false
        syncCurrentAutomaticRefreshFlag()
    }

    func isIndexing(projectID: UUID) -> Bool {
        if refreshTasks[projectID] != nil {
            return true
        }
        if automaticRefreshInProgressByProjectID[projectID] == true {
            return true
        }
        if case .indexing = indexingStatusByProjectID[projectID] {
            return true
        }
        return false
    }

    func isManualRefreshInProgress(projectID: UUID) -> Bool {
        manualRefreshInProgressByProjectID[projectID] == true
    }

    func renameSessionOptimistically(projectID: UUID, sessionID: UUID, title: String) {
        updatePanelState(projectID: projectID) { state in
            state.sessions = state.sessions.map { session in
                guard session.sessionID == sessionID else {
                    return session
                }
                var updated = session
                updated.sessionTitle = title
                return updated
            }
        }
    }

    func removeSessionOptimistically(projectID: UUID, sessionID: UUID) {
        updatePanelState(projectID: projectID) { state in
            state.sessions.removeAll { $0.sessionID == sessionID }
        }
    }

    func invalidateProjectCaches(project: Project) {
        aiUsageStore.deleteIndexedProjectSnapshot(projectID: project.id)
        aiUsageStore.deleteExternalSummaries(projectPath: project.path)
        panelStateByProjectID[project.id] = nil
        refreshStateByProjectID[project.id] = .idle
        cachedPanels.removeValue(for: project.id)
        if currentProjectID == project.id {
            refreshState = .idle
        }
    }

    private func refresh(
        project: Project,
        projects: [Project],
        selectedSessionID: UUID?,
        force: Bool,
        trigger: RefreshTrigger,
        updateCurrentSelectionContext: Bool = true
    ) {
        currentProjects = projects
        if updateCurrentSelectionContext {
            currentProjectID = project.id
            currentSelectedSessionID = selectedSessionID
        }
        let rawLiveEnvelopes = ingestRuntime(project: project, projects: projects)
        let liveEnvelopes = resolveProjectLiveEnvelopes(
            from: rawLiveEnvelopes,
            project: project,
            selectedSessionID: selectedSessionID
        )

        if force, let task = refreshTasks[project.id] {
            task.cancel()
            refreshTasks[project.id] = nil
        }

        if refreshTasks[project.id] != nil {
            logger.log(
                "history-refresh",
                "skip trigger=\(refreshTriggerName(trigger)) project=\(project.id.uuidString) reason=in-flight"
            )
            return
        }

        automaticRefreshInProgressByProjectID[project.id] = isAutomaticTrigger(trigger)
        manualRefreshInProgressByProjectID[project.id] = trigger == .manual || trigger == .initial
        syncCurrentAutomaticRefreshFlag()
        logger.log(
            "history-refresh",
            "start trigger=\(refreshTriggerName(trigger)) project=\(project.id.uuidString) name=\(project.name) force=\(force) live=\(liveEnvelopes.count) selectedSession=\(selectedSessionID?.uuidString ?? "nil")"
        )

        let runningStatus = AIIndexingStatus.indexing(progress: 0.0, detail: String(localized: "ai.indexing.starting", defaultValue: "Starting index.", bundle: .module))
        indexingStatusByProjectID[project.id] = runningStatus
        if var runningState = panelStateByProjectID[project.id] {
            runningState.indexingStatus = runningStatus
            let runningRefreshState = inFlightRefreshState(for: trigger, current: refreshStateByProjectID[project.id])
            storeState(runningState, refreshState: runningRefreshState, for: project.id, updateCurrent: true)
        }

        refreshTasks[project.id] = Task(priority: .utility) {
            let startedAt = Date()
            defer {
                Task { @MainActor in
                    self.refreshTasks[project.id] = nil
                }
            }

            let service = AIUsageService()
            let liveSnapshots = await MainActor.run {
                self.aiSessionStore.liveSnapshots(projectID: project.id)
            }
            let displayLiveSnapshots = await MainActor.run {
                self.aiSessionStore.liveDisplaySnapshots(projectID: project.id)
            }
            let summaryLiveSnapshots = await MainActor.run {
                self.aiSessionStore.liveAggregationSnapshots(projectID: project.id)
            }
            let currentSnapshot = await MainActor.run {
                self.aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
            }
            var quickState = await Task.detached(priority: .userInitiated) {
                service.fastPanelState(project: project, liveSnapshots: summaryLiveSnapshots, currentSnapshot: currentSnapshot)
            }.value
            quickState.liveSnapshots = displayLiveSnapshots
            await MainActor.run {
                self.logger.log(
                    "ai-panel-bridge",
                    "phase=quick project=\(project.id.uuidString) selected=\(selectedSessionID?.uuidString ?? "nil") live=\(liveSnapshots.count) liveUnique=\(self.uniqueLiveSnapshotCount(liveSnapshots)) snapshotSession=\(currentSnapshot?.sessionID.uuidString ?? "nil") snapshotTool=\(currentSnapshot?.tool ?? "nil") snapshotModel=\(currentSnapshot?.model ?? "nil") snapshotTotal=\(currentSnapshot?.currentTotalTokens ?? 0) summaryTool=\(quickState.projectSummary?.currentTool ?? "nil") summaryModel=\(quickState.projectSummary?.currentModel ?? "nil") summaryTotal=\(quickState.projectSummary?.currentSessionTokens ?? 0)"
                )
                self.indexingStatusByProjectID[project.id] = quickState.indexingStatus
                self.panelStateByProjectID[project.id] = quickState
                self.refreshStateByProjectID[project.id] = self.inFlightRefreshState(for: trigger, current: self.refreshStateByProjectID[project.id])
                self.cacheState(quickState, for: project.id)
                if self.currentProjectID == project.id {
                    self.state = quickState
                    self.refreshState = self.inFlightRefreshState(for: trigger, current: self.refreshStateByProjectID[project.id])
                }
            }

            let resultState = await service.panelState(project: project, liveEnvelopes: liveEnvelopes, selectedSessionID: selectedSessionID) { status in
                await MainActor.run {
                    self.indexingStatusByProjectID[project.id] = status
                    guard var nextState = self.panelStateByProjectID[project.id] else {
                        return
                    }
                    nextState.indexingStatus = status
                    let refresh = self.refreshStateByProjectID[project.id] ?? self.inFlightRefreshState(for: trigger, current: nil)
                    self.storeState(nextState, refreshState: refresh, for: project.id, updateCurrent: self.currentProjectID == project.id)
                }
            }

            await MainActor.run {
                let finalStatus = resultState.indexingStatus
                self.indexingStatusByProjectID[project.id] = finalStatus
                self.automaticRefreshInProgressByProjectID[project.id] = false
                self.manualRefreshInProgressByProjectID[project.id] = false
                self.syncCurrentAutomaticRefreshFlag()
                let nextRefreshState: PanelRefreshState
                if case .failed(let detail) = finalStatus {
                    nextRefreshState = .failed(detail)
                } else {
                    self.lastCompletedRefreshAtByProjectID[project.id] = Date()
                    nextRefreshState = .idle
                }
                let displayLiveSnapshots = self.aiSessionStore.liveDisplaySnapshots(projectID: project.id)
                let summaryLiveSnapshots = self.aiSessionStore.liveAggregationSnapshots(projectID: project.id)
                let currentSnapshot = self.aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
                var nextState = service.snapshotBackedPanelState(
                    project: project,
                    liveSnapshots: summaryLiveSnapshots,
                    currentSnapshot: currentSnapshot,
                    status: finalStatus
                )
                nextState.liveSnapshots = displayLiveSnapshots
                self.logger.log(
                    "ai-panel-bridge",
                    "phase=indexed project=\(project.id.uuidString) selected=\(selectedSessionID?.uuidString ?? "nil") live=\(nextState.liveSnapshots.count) indexedSessions=\(nextState.sessions.count) summaryTool=\(nextState.projectSummary?.currentTool ?? "nil") summaryModel=\(nextState.projectSummary?.currentModel ?? "nil") summaryTotal=\(nextState.projectSummary?.currentSessionTokens ?? 0)"
                )
                self.storeState(nextState, refreshState: nextRefreshState, for: project.id, updateCurrent: self.currentProjectID == project.id)
                let durationMS = Int(Date().timeIntervalSince(startedAt) * 1000)
                self.logger.log(
                    "history-refresh",
                    "finish trigger=\(self.refreshTriggerName(trigger)) project=\(project.id.uuidString) result=\(self.refreshResultName(finalStatus)) sessions=\(nextState.sessions.count) live=\(nextState.liveSnapshots.count) indexed=\(nextState.indexedAt != nil) durationMs=\(durationMS)"
                )
                if self.currentProjectID == project.id {
                    self.refreshLiveState(project: project, projects: projects, selectedSessionID: selectedSessionID)
                }
            }
        }
    }

    private func cacheState(_ state: AIStatsPanelState, for projectID: UUID) {
        cachedPanels.set(CachedAIStatsPanelEntry(projectID: projectID, state: state, updatedAt: Date()), for: projectID)
    }

    private func storeState(_ newState: AIStatsPanelState, refreshState newRefreshState: PanelRefreshState, for projectID: UUID, updateCurrent: Bool) {
        panelStateByProjectID[projectID] = newState
        refreshStateByProjectID[projectID] = newRefreshState
        cacheState(newState, for: projectID)
        if updateCurrent {
            state = newState
            refreshState = newRefreshState
            renderVersion &+= 1
        }
    }

    private func updatePanelState(projectID: UUID, transform: (inout AIStatsPanelState) -> Void) {
        guard var nextState = panelStateByProjectID[projectID] ?? cachedPanels.value(for: projectID)?.state else {
            if currentProjectID == projectID {
                var currentState = state
                transform(&currentState)
                state = currentState
                renderVersion &+= 1
            }
            return
        }

        transform(&nextState)
        let nextRefreshState = refreshStateByProjectID[projectID] ?? .idle
        storeState(nextState, refreshState: nextRefreshState, for: projectID, updateCurrent: currentProjectID == projectID)
    }

    private func cachedState(for projectID: UUID) -> AIStatsPanelState? {
        if let state = panelStateByProjectID[projectID] {
            return state
        }
        return cachedPanels.value(for: projectID)?.state
    }

    private func relocalizedStatus(_ status: AIIndexingStatus) -> AIIndexingStatus {
        switch status {
        case .idle:
            return .idle
        case .indexing(let progress, let detail):
            return .indexing(progress: progress, detail: detail)
        case .completed:
            return .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))
        case .cancelled:
            return .cancelled(detail: String(localized: "ai.indexing.stopped", defaultValue: "Indexing stopped.", bundle: .module))
        case .failed(let detail):
            return .failed(detail: detail)
        }
    }

    private func resolvedTodayTotalTokens(for state: AIStatsPanelState) -> Int {
        resolvedTodayTotalTokens(
            summary: state.projectSummary?.todayTotalTokens ?? 0,
            timeBuckets: state.todayTimeBuckets,
            heatmap: state.heatmap
        )
    }

    private func resolvedTodayTotalTokens(summary: Int, timeBuckets: [AITimeBucket], heatmap: [AIHeatmapDay]) -> Int {
        let bucketTotal = timeBuckets.reduce(0) { $0 + $1.totalTokens }
        if bucketTotal > 0 {
            return bucketTotal
        }

        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        if let heatmapToday = heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) })?.totalTokens,
           heatmapToday > 0 {
            return heatmapToday
        }

        return max(0, summary)
    }

    private func syncCurrentAutomaticRefreshFlag() {
        guard let currentProjectID else {
            isAutomaticRefreshInProgress = false
            return
        }
        isAutomaticRefreshInProgress = automaticRefreshInProgressByProjectID[currentProjectID] ?? false
    }

    private func automaticRefreshDecision(projectID: UUID, state: AIStatsPanelState?, interval: TimeInterval) -> (shouldRefresh: Bool, reason: String) {
        if refreshTasks[projectID] != nil {
            return (false, "in-flight")
        }
        if let lastCompleted = lastCompletedRefreshAtByProjectID[projectID] {
            let age = Date().timeIntervalSince(lastCompleted)
            return (
                age >= interval,
                "last-completed age=\(formatInterval(age)) threshold=\(Int(interval))s"
            )
        }
        if let indexedAt = state?.indexedAt {
            let age = Date().timeIntervalSince(indexedAt)
            return (
                age >= interval,
                "indexed age=\(formatInterval(age)) threshold=\(Int(interval))s"
            )
        }
        return (true, "no-indexed-snapshot")
    }

    private func normalizedRestingRefreshState(_ state: PanelRefreshState?) -> PanelRefreshState {
        switch state {
        case .failed(let detail):
            return .failed(detail)
        default:
            return .idle
        }
    }

    private func isAutomaticTrigger(_ trigger: RefreshTrigger) -> Bool {
        switch trigger {
        case .automatic, .background:
            return true
        case .initial, .manual:
            return false
        }
    }

    private func inFlightRefreshState(for trigger: RefreshTrigger, current: PanelRefreshState?) -> PanelRefreshState {
        switch trigger {
        case .automatic:
            return .showingCached
        case .background:
            return normalizedRestingRefreshState(current)
        case .initial, .manual:
            return .refreshing
        }
    }

    private func refreshTriggerName(_ trigger: RefreshTrigger) -> String {
        switch trigger {
        case .initial:
            return "initial"
        case .manual:
            return "manual"
        case .automatic:
            return "automatic"
        case .background:
            return "background"
        }
    }

    private func refreshResultName(_ status: AIIndexingStatus) -> String {
        switch status {
        case .idle:
            return "idle"
        case .indexing:
            return "indexing"
        case .completed:
            return "completed"
        case .cancelled:
            return "cancelled"
        case .failed:
            return "failed"
        }
    }

    private func formatInterval(_ value: TimeInterval) -> String {
        String(format: "%.1fs", value)
    }

    private func runBackgroundRefreshTick(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        if isPanelVisible() {
            logger.log("history-refresh", "skip trigger=background reason=panel-visible")
            return
        }

        guard let project = selectedProject() else {
            logger.log("history-refresh", "skip trigger=background reason=no-selected-project")
            return
        }

        let currentProjects = projects()
        let sessionID = effectiveSessionID(selectedSessionID())
        let decision = automaticRefreshDecision(
            projectID: project.id,
            state: cachedState(for: project.id),
            interval: backgroundRefreshInterval
        )
        guard decision.shouldRefresh else {
            logger.log(
                "history-refresh",
                "skip trigger=background project=\(project.id.uuidString) reason=\(decision.reason)"
            )
            return
        }

        logger.log(
            "history-refresh",
            "queue trigger=background project=\(project.id.uuidString) reason=\(decision.reason)"
        )
        refresh(
            project: project,
            projects: currentProjects,
            selectedSessionID: sessionID,
            force: false,
            trigger: .background,
            updateCurrentSelectionContext: false
        )
    }

    private func startRuntimeBridgeObserver(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        if let runtimeBridgeObserver {
            NotificationCenter.default.removeObserver(runtimeBridgeObserver)
        }
        runtimeBridgeObserver = NotificationCenter.default.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self else {
                return
            }
            let shouldForce = notification.userInfo?["kind"] != nil
            Task { @MainActor [weak self] in
                self?.scheduleRuntimeBridgeRefresh(force: shouldForce)
            }
        }
    }

    private func scheduleRuntimeBridgeRefresh(force: Bool) {
        pendingRuntimeBridgeRefreshShouldForce = pendingRuntimeBridgeRefreshShouldForce || force

        guard pendingRuntimeBridgeRefreshTask == nil else {
            return
        }

        pendingRuntimeBridgeRefreshTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(120))
            guard let self else {
                return
            }

            _ = self.pendingRuntimeBridgeRefreshShouldForce
            self.pendingRuntimeBridgeRefreshShouldForce = false
            self.pendingRuntimeBridgeRefreshTask = nil

            guard let isPanelVisible = self.panelVisibilityProvider,
                  let selectedProject = self.selectedProjectProvider,
                  let selectedSessionID = self.selectedSessionIDProvider,
                  let projects = self.projectsProvider,
                  isPanelVisible(),
                  let project = selectedProject() else {
                return
            }

            let currentProjects = projects()
            let sessionID = self.effectiveSessionID(selectedSessionID())
            _ = self.ingestRuntime(project: project, projects: currentProjects)
            self.refreshLiveState(project: project, projects: currentProjects, selectedSessionID: sessionID)
        }
    }

    private func startTerminalFocusObserver(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        if let terminalFocusObserver {
            NotificationCenter.default.removeObserver(terminalFocusObserver)
        }

        terminalFocusObserver = NotificationCenter.default.addObserver(
            forName: .dmuxTerminalFocusDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            guard let self else {
                return
            }
            Task { @MainActor in
                guard isPanelVisible(), let project = selectedProject() else {
                    return
                }
                let sessionID = self.effectiveSessionID(selectedSessionID())
                let currentProjects = projects()
                self.refreshLiveState(project: project, projects: currentProjects, selectedSessionID: sessionID)
            }
        }
    }

    private func startTerminalOutputObserver(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        if let terminalOutputObserver {
            NotificationCenter.default.removeObserver(terminalOutputObserver)
        }

        terminalOutputObserver = NotificationCenter.default.addObserver(
            forName: .dmuxTerminalOutputDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self,
                  let sessionID = notification.object as? UUID else {
                return
            }

            Task { @MainActor in
                guard isPanelVisible(),
                      let project = selectedProject() else {
                    return
                }

                let currentProjects = projects()
                let effectiveSelectedSessionID = self.effectiveSessionID(selectedSessionID())
                guard effectiveSelectedSessionID == sessionID else {
                    return
                }

                let liveSnapshot = self.aiSessionStore
                    .liveSnapshots(projectID: project.id)
                    .first(where: { $0.sessionID == sessionID })
                guard liveSnapshot != nil else {
                    return
                }
                self.refreshLiveState(project: project, projects: currentProjects, selectedSessionID: effectiveSelectedSessionID)
            }
        }
    }

    private func refreshLiveState(project: Project, projects: [Project], selectedSessionID: UUID?) {
        let liveSnapshots = aiSessionStore.liveSnapshots(projectID: project.id)
        let displayLiveSnapshots = aiSessionStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = aiSessionStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
        let status = indexingStatusByProjectID[project.id] ?? .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))
        let currentState = panelStateByProjectID[project.id] ?? cachedState(for: project.id) ?? .empty
        var nextState = aiUsageService.lightweightLivePanelState(
            from: currentState,
            project: project,
            liveSnapshots: summaryLiveSnapshots,
            currentSnapshot: currentSnapshot,
            status: status
        )
        nextState.liveSnapshots = displayLiveSnapshots
        if nextState == currentState,
           normalizedRestingRefreshState(refreshStateByProjectID[project.id]) == .idle {
            return
        }

        logger.log(
            "ai-panel-bridge",
            "phase=live project=\(project.id.uuidString) selected=\(selectedSessionID?.uuidString ?? "nil") live=\(liveSnapshots.count) liveUnique=\(uniqueLiveSnapshotCount(liveSnapshots)) snapshotSession=\(currentSnapshot?.sessionID.uuidString ?? "nil") snapshotTool=\(currentSnapshot?.tool ?? "nil") snapshotModel=\(currentSnapshot?.model ?? "nil") snapshotTotal=\(currentSnapshot?.currentTotalTokens ?? 0) summaryTool=\(nextState.projectSummary?.currentTool ?? "nil") summaryModel=\(nextState.projectSummary?.currentModel ?? "nil") summaryTotal=\(nextState.projectSummary?.currentSessionTokens ?? 0)"
        )
        storeState(nextState, refreshState: .idle, for: project.id, updateCurrent: true)
        syncCurrentAutomaticRefreshFlag()
    }

    private func uniqueLiveSnapshotCount(_ snapshots: [AITerminalSessionSnapshot]) -> Int {
        var keys = Set<String>()
        var fallbackCount = 0

        for snapshot in snapshots {
            guard let tool = snapshot.tool?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !tool.isEmpty,
                  let externalSessionID = snapshot.externalSessionID?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !externalSessionID.isEmpty else {
                fallbackCount += 1
                continue
            }
            keys.insert("\(tool.lowercased()):\(externalSessionID.lowercased())")
        }

        return keys.count + fallbackCount
    }

    private func scheduleRuntimeRefreshesForLiveSessions(
        project: Project,
        projects: [Project],
        force: Bool
    ) {
        _ = project
        _ = projects
        _ = force
    }

    private func scheduleRuntimeTailRefreshesForLiveSessions(
        project: Project,
        projects: [Project],
        selectedSessionID: UUID?
    ) {
        _ = project
        _ = projects
        _ = selectedSessionID
    }

    private func ingestRuntime(project: Project, projects: [Project]) -> [AIToolUsageEnvelope] {
        _ = runtimeIngressService.importRuntime(
            projects: projects,
            projectID: project.id
        )
        return resolveProjectLiveEnvelopes(
            from: [],
            project: project,
            selectedSessionID: currentSelectedSessionID
        )
    }

    func handleTerminalSessionClosed(sessionID: UUID, project: Project?, projects: [Project], selectedSessionID: UUID?) {
        aiSessionStore.removeTerminal(sessionID)
        guard let project else {
            return
        }
        refreshLiveState(project: project, projects: projects, selectedSessionID: effectiveSessionID(selectedSessionID))
    }

    private func resolveProjectLiveEnvelopes(
        from envelopes: [AIToolUsageEnvelope],
        project: Project,
        selectedSessionID: UUID?
    ) -> [AIToolUsageEnvelope] {
        _ = envelopes

        var resolved = aiSessionStore.liveSnapshots(projectID: project.id).map { snapshot in
            AIToolUsageEnvelope(
                sessionId: snapshot.sessionID.uuidString,
                sessionInstanceId: nil,
                invocationId: nil,
                externalSessionID: snapshot.externalSessionID,
                projectId: project.id.uuidString,
                projectName: project.name,
                projectPath: project.path,
                sessionTitle: snapshot.sessionTitle,
                tool: snapshot.tool ?? "",
                model: snapshot.model,
                status: snapshot.status,
                responseState: snapshot.isRunning ? .responding : .idle,
                updatedAt: snapshot.updatedAt.timeIntervalSince1970,
                startedAt: snapshot.startedAt?.timeIntervalSince1970,
                finishedAt: snapshot.isRunning ? nil : snapshot.updatedAt.timeIntervalSince1970,
                inputTokens: snapshot.currentInputTokens,
                outputTokens: snapshot.currentOutputTokens,
                totalTokens: snapshot.currentTotalTokens,
                baselineInputTokens: snapshot.baselineInputTokens,
                baselineOutputTokens: snapshot.baselineOutputTokens,
                baselineTotalTokens: snapshot.baselineTotalTokens,
                contextWindow: snapshot.currentContextWindow,
                contextUsedTokens: snapshot.currentContextUsedTokens,
                contextUsagePercent: snapshot.currentContextUsagePercent,
                source: .hook
            )
        }

        if let selectedSessionID,
           resolved.contains(where: { $0.sessionId == selectedSessionID.uuidString }) == false,
           let snapshot = aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID) {
            resolved.append(
                AIToolUsageEnvelope(
                    sessionId: snapshot.sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    externalSessionID: snapshot.externalSessionID,
                    projectId: project.id.uuidString,
                    projectName: project.name,
                    projectPath: project.path,
                    sessionTitle: snapshot.sessionTitle,
                    tool: snapshot.tool ?? "",
                    model: snapshot.model,
                    status: snapshot.status,
                    responseState: snapshot.isRunning ? .responding : .idle,
                    updatedAt: snapshot.updatedAt.timeIntervalSince1970,
                    startedAt: snapshot.startedAt?.timeIntervalSince1970,
                    finishedAt: snapshot.isRunning ? nil : snapshot.updatedAt.timeIntervalSince1970,
                    inputTokens: snapshot.currentInputTokens,
                    outputTokens: snapshot.currentOutputTokens,
                    totalTokens: snapshot.currentTotalTokens,
                    baselineInputTokens: snapshot.baselineInputTokens,
                    baselineOutputTokens: snapshot.baselineOutputTokens,
                    baselineTotalTokens: snapshot.baselineTotalTokens,
                    contextWindow: snapshot.currentContextWindow,
                    contextUsedTokens: snapshot.currentContextUsedTokens,
                    contextUsagePercent: snapshot.currentContextUsagePercent,
                    source: .hook
                )
            )
        }

        return resolved
            .filter(isMeaningfulLiveEnvelope)
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    private func isMeaningfulLiveEnvelope(_ envelope: AIToolUsageEnvelope) -> Bool {
        envelope.status == "running"
    }
}
