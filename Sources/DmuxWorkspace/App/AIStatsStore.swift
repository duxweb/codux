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
    private let runtimeContextProbe = AIRuntimeContextProbe()
    private let runtimeIngressService = AIRuntimeIngressService.shared
    private let runtimeStateStore = AIRuntimeStateStore.shared
    private let toolDriverFactory = AIToolDriverFactory.shared
    private let logger = AppDebugLog.shared
    private var refreshTasks: [UUID: Task<Void, Never>] = [:]
    private var runtimeRefreshTasksBySessionID: [UUID: Task<Void, Never>] = [:]
    private var lastRuntimeRefreshAtBySessionID: [UUID: Date] = [:]
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
    private var runtimeTailRefreshTasksBySessionID: [UUID: Task<Void, Never>] = [:]
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
    private let liveSessionCutoff = Date().timeIntervalSince1970

    private func effectiveSessionID(_ selectedSessionID: UUID?) -> UUID? {
        let focusedSessionID = SwiftTermTerminalRegistry.shared.focusedSessionID()
        let resolvedSessionID = focusedSessionID ?? selectedSessionID
        if debugAIFocus {
            let registrySnapshot = SwiftTermTerminalRegistry.shared.debugSnapshot()
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
        let displayLiveSnapshots = runtimeStateStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = runtimeStateStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = runtimeStateStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
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

        scheduleRuntimeRefreshesForLiveSessions(
            project: project,
            projects: projects,
            force: false
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
        let displayLiveSnapshots = runtimeStateStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = runtimeStateStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = runtimeStateStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: currentSelectedSessionID)
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
                self.runtimeStateStore.liveSnapshots(projectID: project.id)
            }
            let displayLiveSnapshots = await MainActor.run {
                self.runtimeStateStore.liveDisplaySnapshots(projectID: project.id)
            }
            let summaryLiveSnapshots = await MainActor.run {
                self.runtimeStateStore.liveAggregationSnapshots(projectID: project.id)
            }
            let currentSnapshot = await MainActor.run {
                self.runtimeStateStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
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
                let displayLiveSnapshots = self.runtimeStateStore.liveDisplaySnapshots(projectID: project.id)
                let summaryLiveSnapshots = self.runtimeStateStore.liveAggregationSnapshots(projectID: project.id)
                let currentSnapshot = self.runtimeStateStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
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
            let kind = notification.userInfo?["kind"] as? String
            let didSwitchExternalSession =
                (notification.userInfo?["didSwitchExternalSession"] as? Bool) == true
            let switchedSessionID = (notification.userInfo?["sessionID"] as? String)
                .flatMap(UUID.init(uuidString:))
            let switchedTool = notification.userInfo?["tool"] as? String
            let didClearRuntimeSession =
                (notification.userInfo?["didClearRuntimeSession"] as? Bool) == true
            let clearedSessionID = (notification.userInfo?["clearedSessionID"] as? String)
                .flatMap(UUID.init(uuidString:))
            let clearedTool = notification.userInfo?["clearedTool"] as? String
            Task { @MainActor [weak self] in
                if didSwitchExternalSession,
                   let switchedSessionID,
                   let switchedTool {
                    self?.resetRuntimeProbeState(
                        for: switchedSessionID,
                        tool: switchedTool,
                        reason: "bridge-external-switch"
                    )
                }
                if didClearRuntimeSession,
                   let clearedSessionID,
                   let clearedTool {
                    self?.resetRuntimeProbeState(
                        for: clearedSessionID,
                        tool: clearedTool,
                        reason: "bridge-session-cleared"
                    )
                }
                self?.scheduleRuntimeBridgeRefresh(force: kind != nil)
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

            let shouldForce = self.pendingRuntimeBridgeRefreshShouldForce
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
            self.scheduleRuntimeRefreshesForLiveSessions(
                project: project,
                projects: currentProjects,
                force: shouldForce
            )
            self.scheduleRuntimeTailRefreshesForLiveSessions(
                project: project,
                projects: currentProjects,
                selectedSessionID: sessionID
            )
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
                self.scheduleRuntimeRefreshesForLiveSessions(
                    project: project,
                    projects: currentProjects,
                    force: false
                )
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

                let liveSnapshot = self.runtimeStateStore
                    .liveSnapshots(projectID: project.id)
                    .first(where: { $0.sessionID == sessionID })
                guard let tool = liveSnapshot?.tool ?? self.currentRuntimeTool(for: sessionID, projectID: project.id),
                      self.toolDriverFactory.isRealtimeTool(tool) else {
                    return
                }

                if let driver = self.toolDriverFactory.driver(for: tool),
                   !driver.runtimeSourceDescriptors(project: project, envelope: nil).isEmpty {
                    // Source-backed runtimes should normally rely on their own event sources.
                    // Bootstrap them once when the live context is still empty so the first
                    // model/token snapshot appears without requiring a focus change.
                    if !self.needsRuntimeBootstrap(
                        sessionID: sessionID,
                        tool: tool,
                        projectID: project.id,
                        liveSnapshot: liveSnapshot
                    ) {
                        return
                    }
                }

                self.scheduleRuntimeRefresh(
                    for: sessionID,
                    tool: tool,
                    project: project,
                    projects: currentProjects,
                    force: true
                )
                self.scheduleRuntimeTailRefresh(
                    for: sessionID,
                    tool: tool,
                    project: project,
                    projects: currentProjects,
                    selectedSessionID: effectiveSelectedSessionID
                )
            }
        }
    }

    private func refreshLiveState(project: Project, projects: [Project], selectedSessionID: UUID?) {
        let liveSnapshots = runtimeStateStore.liveSnapshots(projectID: project.id)
        let displayLiveSnapshots = runtimeStateStore.liveDisplaySnapshots(projectID: project.id)
        let summaryLiveSnapshots = runtimeStateStore.liveAggregationSnapshots(projectID: project.id)
        let currentSnapshot = runtimeStateStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID)
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
        let liveSnapshots = runtimeStateStore.liveSnapshots(projectID: project.id)
        for snapshot in liveSnapshots {
            guard let tool = snapshot.tool ?? currentRuntimeTool(for: snapshot.sessionID, projectID: project.id),
                  toolDriverFactory.isRealtimeTool(tool) else {
                continue
            }
            scheduleRuntimeRefresh(
                for: snapshot.sessionID,
                tool: tool,
                project: project,
                projects: projects,
                force: force
            )
        }
    }

    private func scheduleRuntimeTailRefreshesForLiveSessions(
        project: Project,
        projects: [Project],
        selectedSessionID: UUID?
    ) {
        let liveSnapshots = runtimeStateStore.liveSnapshots(projectID: project.id)
        for snapshot in liveSnapshots {
            guard let tool = snapshot.tool ?? currentRuntimeTool(for: snapshot.sessionID, projectID: project.id),
                  toolDriverFactory.isRealtimeTool(tool) else {
                continue
            }
            scheduleRuntimeTailRefresh(
                for: snapshot.sessionID,
                tool: tool,
                project: project,
                projects: projects,
                selectedSessionID: selectedSessionID
            )
        }
    }

    private func ingestRuntime(project: Project, projects: [Project]) -> [AIToolUsageEnvelope] {
        let envelopes = runtimeIngressService.importRuntime(
            projects: projects,
            projectID: project.id,
            liveSessionCutoff: liveSessionCutoff
        )
        syncRuntimeSessions(with: envelopes, project: project)
        return envelopes
    }

    private func currentLiveTool(for sessionID: UUID, in envelopes: [AIToolUsageEnvelope]) -> String? {
        envelopes.first { UUID(uuidString: $0.sessionId) == sessionID && isCurrentContextEligibleEnvelope($0) }?.tool
    }

    private func currentRuntimeTool(for sessionID: UUID, projectID: UUID) -> String? {
        runtimeStateStore.liveSnapshots(projectID: projectID)
            .first(where: { $0.sessionID == sessionID })?
            .tool
            ?? runtimeStateStore.tool(for: sessionID)
    }

    private func normalizedToolName(_ tool: String) -> String {
        toolDriverFactory.canonicalToolName(tool)
    }

    private func needsRuntimeBootstrap(
        sessionID: UUID,
        tool: String,
        projectID: UUID,
        liveSnapshot: AITerminalSessionSnapshot?
    ) -> Bool {
        _ = projectID
        guard let runtime = runtimeStateStore.runtimeContext(for: sessionID) else {
            return true
        }

        if (runtime.model?.isEmpty ?? true) {
            return true
        }
        if (runtime.externalSessionID?.isEmpty ?? true),
           normalizedToolName(tool) == "claude",
           (liveSnapshot?.externalSessionID?.isEmpty ?? true) {
            return true
        }
        if runtime.totalTokens <= 0,
           (liveSnapshot?.currentTotalTokens ?? runtime.totalTokens) <= 0,
           (liveSnapshot?.responseState ?? runtime.responseState) == .responding {
            return true
        }

        return false
    }

    private func syncRuntimeSessions(with envelopes: [AIToolUsageEnvelope], project: Project) {
        let liveSessionIDs = Set(envelopes.compactMap { UUID(uuidString: $0.sessionId) })
        let trackedSessionIDs = Set(
            runtimeStateStore.terminalBindingsByID.values
                .filter { $0.projectID == project.id }
                .map(\.sessionID)
        )

        for envelope in envelopes {
            guard let sessionID = UUID(uuidString: envelope.sessionId) else {
                continue
            }

            let existingBinding = runtimeStateStore.terminalBindingsByID[sessionID]
            if let incomingInvocationID = normalizedInvocationID(envelope.invocationId),
               let previousInvocationID = normalizedInvocationID(existingBinding?.invocationID),
               previousInvocationID != incomingInvocationID {
                resetRuntimeProbeState(for: sessionID, tool: existingBinding?.tool ?? envelope.tool)
            }

            if let incomingInstanceID = normalizedInvocationID(envelope.sessionInstanceId),
               let previousInstanceID = normalizedInvocationID(existingBinding?.sessionInstanceID),
               previousInstanceID != incomingInstanceID {
                resetRuntimeProbeState(for: sessionID, tool: existingBinding?.tool ?? envelope.tool)
            }

            let previousExternalSessionID = normalizedExternalSessionID(
                runtimeStateStore.externalSessionID(for: sessionID)
                ?? runtimeStateStore.runtimeContext(for: sessionID)?.externalSessionID
                ?? existingBinding?.lastKnownExternalSessionID
            )
            runtimeStateStore.applyLiveEnvelope(envelope)
            let incomingExternalSessionID = normalizedExternalSessionID(
                runtimeStateStore.externalSessionID(for: sessionID)
                ?? runtimeStateStore.runtimeContext(for: sessionID)?.externalSessionID
                ?? envelope.externalSessionID
            )
            if toolDriverFactory.allowsRuntimeExternalSessionSwitch(for: envelope.tool),
               externalSessionIDDidChange(
                   previous: previousExternalSessionID,
                   incoming: incomingExternalSessionID
               ) {
                resetRuntimeProbeState(for: sessionID, tool: envelope.tool)
                logger.log(
                    "runtime-refresh",
                    "switch session=\(sessionID.uuidString) tool=\(normalizedToolName(envelope.tool)) external=\(previousExternalSessionID ?? "nil")->\(incomingExternalSessionID ?? "nil") source=live-envelope"
                )
            }
        }

        for sessionID in trackedSessionIDs where !liveSessionIDs.contains(sessionID) {
            clearRuntimeState(for: sessionID)
        }
        runtimeStateStore.prune(projectID: project.id, liveSessionIDs: liveSessionIDs)
    }

    private func clearRuntimeState(for sessionID: UUID) {
        let tool = runtimeStateStore.tool(for: sessionID)
        resetRuntimeProbeState(for: sessionID, tool: tool)
        runtimeStateStore.clearSession(sessionID)
    }

    private func resetRuntimeProbeState(for sessionID: UUID, tool: String?, reason: String = "reset") {
        logger.log(
            "runtime-refresh",
            "reset session=\(sessionID.uuidString) tool=\(tool.map(normalizedToolName(_:)) ?? "nil") reason=\(reason)"
        )
        runtimeRefreshTasksBySessionID[sessionID]?.cancel()
        runtimeRefreshTasksBySessionID[sessionID] = nil
        runtimeTailRefreshTasksBySessionID[sessionID]?.cancel()
        runtimeTailRefreshTasksBySessionID[sessionID] = nil
        lastRuntimeRefreshAtBySessionID[sessionID] = nil
        Task {
            await runtimeContextProbe.reset(
                for: tool,
                runtimeSessionID: sessionID.uuidString
            )
        }
    }

    private func matchesTrackedRuntimeSession(sessionID: UUID, projectID: UUID, tool: String) -> Bool {
        guard let binding = runtimeStateStore.terminalBindingsByID[sessionID],
              binding.projectID == projectID,
              binding.status == "running" else {
            return false
        }
        return normalizedToolName(binding.tool) == normalizedToolName(tool)
    }

    func handleTerminalSessionClosed(sessionID: UUID, project: Project?, projects: [Project], selectedSessionID: UUID?) {
        clearRuntimeState(for: sessionID)
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
        let liveSnapshotsBySessionID = Dictionary(
            uniqueKeysWithValues: runtimeStateStore
                .liveSnapshots(projectID: project.id)
                .map { ($0.sessionID, $0) }
        )
        let projectEnvelopes = envelopes
            .filter { UUID(uuidString: $0.projectId) == project.id }
            .filter { envelope in
                let startedAt = envelope.startedAt ?? envelope.updatedAt
                return startedAt >= liveSessionCutoff - 2
            }
            .map { envelope in
                var enriched = envelope
                if let sessionUUID = UUID(uuidString: enriched.sessionId),
                   let liveSnapshot = liveSnapshotsBySessionID[sessionUUID] {
                    if (enriched.externalSessionID?.isEmpty ?? true),
                       let externalSessionID = liveSnapshot.externalSessionID,
                       !externalSessionID.isEmpty {
                        enriched.externalSessionID = externalSessionID
                    }
                    if (enriched.model?.isEmpty ?? true),
                       let runtimeModel = liveSnapshot.model,
                       !runtimeModel.isEmpty {
                        enriched.model = runtimeModel
                    }
                    if enriched.tool.isEmpty,
                       let runtimeTool = liveSnapshot.tool,
                       !runtimeTool.isEmpty {
                        enriched.tool = runtimeTool
                    }
                    enriched.inputTokens = liveSnapshot.currentInputTokens
                    enriched.outputTokens = liveSnapshot.currentOutputTokens
                    enriched.totalTokens = liveSnapshot.currentTotalTokens
                    enriched.baselineInputTokens = liveSnapshot.baselineInputTokens
                    enriched.baselineOutputTokens = liveSnapshot.baselineOutputTokens
                    enriched.baselineTotalTokens = liveSnapshot.baselineTotalTokens
                    enriched.responseState = liveSnapshot.responseState ?? enriched.responseState
                    enriched.updatedAt = max(enriched.updatedAt, liveSnapshot.updatedAt.timeIntervalSince1970)
                }
                return enriched
            }
            .sorted { $0.updatedAt > $1.updatedAt }

        var filtered = projectEnvelopes
            .filter(isMeaningfulLiveEnvelope)

        if let selectedSessionID,
           SwiftTermTerminalRegistry.shared.shellPID(for: selectedSessionID) != nil,
           let selectedEnvelope = projectEnvelopes.first(where: {
               UUID(uuidString: $0.sessionId) == selectedSessionID && isCurrentContextEligibleEnvelope($0)
           }),
           filtered.contains(where: { $0.sessionId == selectedEnvelope.sessionId }) == false {
            filtered.append(selectedEnvelope)
        } else if let selectedSessionID,
                  let selectedSnapshot = liveSnapshotsBySessionID[selectedSessionID],
                  let binding = runtimeStateStore.terminalBindingsByID[selectedSessionID],
                  binding.projectID == project.id,
                  binding.status == "running",
                  filtered.contains(where: { $0.sessionId == selectedSessionID.uuidString }) == false {
            filtered.append(
                AIToolUsageEnvelope(
                    sessionId: selectedSessionID.uuidString,
                    sessionInstanceId: binding.sessionInstanceID,
                    invocationId: binding.invocationID,
                    externalSessionID: selectedSnapshot.externalSessionID,
                    projectId: project.id.uuidString,
                    projectName: project.name,
                    projectPath: project.path,
                    sessionTitle: binding.sessionTitle,
                    tool: selectedSnapshot.tool ?? binding.tool,
                    model: selectedSnapshot.model,
                    status: binding.status,
                    responseState: selectedSnapshot.responseState,
                    updatedAt: max(Date().timeIntervalSince1970, selectedSnapshot.updatedAt.timeIntervalSince1970),
                    startedAt: binding.startedAt,
                    finishedAt: nil,
                    inputTokens: selectedSnapshot.currentInputTokens,
                    outputTokens: selectedSnapshot.currentOutputTokens,
                    totalTokens: selectedSnapshot.currentTotalTokens,
                    baselineInputTokens: selectedSnapshot.baselineInputTokens,
                    baselineOutputTokens: selectedSnapshot.baselineOutputTokens,
                    baselineTotalTokens: selectedSnapshot.baselineTotalTokens,
                    contextWindow: selectedSnapshot.currentContextWindow,
                    contextUsedTokens: selectedSnapshot.currentContextUsedTokens,
                    contextUsagePercent: selectedSnapshot.currentContextUsagePercent
                )
            )
        }

        return filtered.sorted { $0.updatedAt > $1.updatedAt }
    }

    private func scheduleRuntimeRefresh(
        for sessionID: UUID,
        tool: String,
        project: Project,
        projects: [Project],
        force: Bool
    ) {
        guard runtimeRefreshTasksBySessionID[sessionID] == nil else {
            return
        }

        let now = Date()
        let refreshInterval = toolDriverFactory.runtimeRefreshInterval(for: tool)
        let delay: TimeInterval
        if force {
            delay = 0
        } else if let lastRefreshAt = lastRuntimeRefreshAtBySessionID[sessionID] {
            delay = max(0, refreshInterval - now.timeIntervalSince(lastRefreshAt))
        } else {
            delay = 0
        }

        if debugAIFocus {
            print("[AIStats] sessionID=\(sessionID.uuidString) scheduleRuntimeRefresh tool=\(tool) delay=\(String(format: "%.2f", delay))")
        }
        if force || lastRuntimeRefreshAtBySessionID[sessionID] == nil {
            logger.log(
                "runtime-refresh",
                "schedule session=\(sessionID.uuidString) tool=\(normalizedToolName(tool)) project=\(project.id.uuidString) delay=\(String(format: "%.2f", delay)) force=\(force)"
            )
        }

        let runtimeProbe = runtimeContextProbe
        runtimeRefreshTasksBySessionID[sessionID] = Task.detached(priority: .utility) {
            if delay > 0 {
                let nanoseconds = UInt64(delay * 1_000_000_000)
                try? await Task.sleep(nanoseconds: nanoseconds)
            }

            guard !Task.isCancelled else {
                return
            }

            let runtimeInputs = await MainActor.run { () -> (startedAt: Double, knownExternalSessionID: String?)? in
                guard self.matchesTrackedRuntimeSession(
                    sessionID: sessionID,
                    projectID: project.id,
                    tool: tool
                ) else {
                    return nil
                }
                return (
                    self.runtimeStateStore.terminalBindingsByID[sessionID]?.startedAt
                        ?? Date().timeIntervalSince1970,
                    self.runtimeStateStore.runtimeProbeExternalSessionHint(
                        for: sessionID,
                        tool: tool
                    )
                )
            }
            guard let runtimeInputs else {
                return
            }

            let runtime = await runtimeProbe.snapshot(
                for: tool,
                runtimeSessionID: sessionID.uuidString,
                projectPath: project.path,
                startedAt: runtimeInputs.startedAt,
                knownExternalSessionID: runtimeInputs.knownExternalSessionID
            )

            await MainActor.run {
                var shouldReschedule = false
                defer {
                    self.runtimeRefreshTasksBySessionID[sessionID] = nil
                    if shouldReschedule {
                        Task { @MainActor in
                            self.scheduleRuntimeRefresh(
                                for: sessionID,
                                tool: tool,
                                project: project,
                                projects: projects,
                                force: false
                            )
                        }
                    }
                }

                guard self.matchesTrackedRuntimeSession(
                    sessionID: sessionID,
                    projectID: project.id,
                    tool: tool
                ) else {
                    return
                }

                if self.debugAIFocus {
                    let runtimeModel = runtime?.model ?? "nil"
                    print("[AIStats] sessionID=\(sessionID.uuidString) applyRuntimeRefresh tool=\(tool) model=\(runtimeModel)")
                }

                let previousRuntime = self.runtimeStateStore.runtimeContext(for: sessionID)
                if let runtime {
                    guard let result = self.runtimeStateStore.applyRuntimeSnapshot(
                        sessionID: sessionID,
                        snapshot: runtime
                    ) else {
                        self.lastRuntimeRefreshAtBySessionID[sessionID] = Date()
                        shouldReschedule = self.shouldContinueRuntimeRefresh(
                            for: sessionID,
                            tool: tool,
                            projectID: project.id
                        )
                        return
                    }
                    if result.ignored {
                        self.lastRuntimeRefreshAtBySessionID[sessionID] = Date()
                        shouldReschedule = self.shouldContinueRuntimeRefresh(
                            for: sessionID,
                            tool: tool,
                            projectID: project.id
                        )
                        return
                    }
                    if result.didSwitchExternalSession {
                        self.logger.log(
                            "runtime-refresh",
                            "switch session=\(sessionID.uuidString) tool=\(self.normalizedToolName(tool)) external=\(result.previousContext?.externalSessionID ?? "nil")->\(result.currentContext.externalSessionID ?? "nil") source=runtime-refresh"
                        )
                    }
                    if result.didChangeDisplay {
                        self.logger.log(
                            "runtime-refresh",
                            "apply session=\(sessionID.uuidString) tool=\(self.normalizedToolName(tool)) model=\(result.currentContext.model ?? "nil") total=\(result.currentContext.totalTokens) response=\(result.currentContext.responseState?.rawValue ?? "nil")"
                        )
                    }
                    if result.didAdvance {
                        NotificationCenter.default.post(
                            name: .dmuxAIRuntimeActivityPulse,
                            object: nil,
                            userInfo: [
                                "projectID": project.id.uuidString,
                                "tool": result.currentContext.tool,
                            ]
                        )
                    }
                    guard result.didChangeDisplay else {
                        self.lastRuntimeRefreshAtBySessionID[sessionID] = Date()
                        shouldReschedule = self.shouldContinueRuntimeRefresh(
                            for: sessionID,
                            tool: tool,
                            projectID: project.id
                        )
                        return
                    }
                } else {
                    if previousRuntime == nil {
                        self.lastRuntimeRefreshAtBySessionID[sessionID] = Date()
                        shouldReschedule = self.shouldContinueRuntimeRefresh(
                            for: sessionID,
                            tool: tool,
                            projectID: project.id
                        )
                        self.logger.log(
                            "runtime-refresh",
                            "miss session=\(sessionID.uuidString) tool=\(self.normalizedToolName(tool)) project=\(project.id.uuidString) reschedule=\(shouldReschedule)"
                        )
                        return
                    }
                }

                self.lastRuntimeRefreshAtBySessionID[sessionID] = Date()
                shouldReschedule = self.shouldContinueRuntimeRefresh(
                    for: sessionID,
                    tool: tool,
                    projectID: project.id
                )

                guard self.currentProjectID == project.id else {
                    return
                }
                let currentSessionID = self.effectiveSessionID(self.currentSelectedSessionID)
                self.refreshLiveState(project: project, projects: projects, selectedSessionID: currentSessionID)
            }
        }
    }

    private func scheduleRuntimeTailRefresh(
        for sessionID: UUID,
        tool: String,
        project: Project,
        projects: [Project],
        selectedSessionID: UUID?
    ) {
        runtimeTailRefreshTasksBySessionID[sessionID]?.cancel()
        runtimeTailRefreshTasksBySessionID[sessionID] = Task.detached(priority: .utility) {
            try? await Task.sleep(nanoseconds: 850_000_000)
            guard !Task.isCancelled else {
                return
            }

            await MainActor.run {
                self.runtimeTailRefreshTasksBySessionID[sessionID] = nil
                guard self.matchesTrackedRuntimeSession(
                    sessionID: sessionID,
                    projectID: project.id,
                    tool: tool
                ) else {
                    return
                }
                self.scheduleRuntimeRefresh(
                    for: sessionID,
                    tool: tool,
                    project: project,
                    projects: projects,
                    force: true
                )
                if self.currentProjectID == project.id {
                    self.refreshLiveState(project: project, projects: projects, selectedSessionID: selectedSessionID)
                }
            }
        }
    }

    private func shouldContinueRuntimeRefresh(for sessionID: UUID, tool: String, projectID: UUID) -> Bool {
        guard toolDriverFactory.isRealtimeTool(tool),
              matchesTrackedRuntimeSession(sessionID: sessionID, projectID: projectID, tool: tool) else {
            logger.log(
                "runtime-refresh",
                "stop session=\(sessionID.uuidString) tool=\(normalizedToolName(tool)) project=\(projectID.uuidString) reason=not-live-or-mismatch"
            )
            return false
        }

        let liveSessionIDs = Set(runtimeStateStore.liveSnapshots(projectID: projectID).map(\.sessionID))
        let shouldContinue = liveSessionIDs.contains(sessionID)
        if shouldContinue == false {
            logger.log(
                "runtime-refresh",
                "stop session=\(sessionID.uuidString) tool=\(normalizedToolName(tool)) project=\(projectID.uuidString) reason=session-not-in-live-store"
            )
        }
        return shouldContinue
    }

    private func normalizedExternalSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func normalizedInvocationID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func externalSessionIDDidChange(previous: String?, incoming: String?) -> Bool {
        guard let previous = normalizedExternalSessionID(previous),
              let incoming = normalizedExternalSessionID(incoming) else {
            return false
        }
        return previous != incoming
    }

    private func isMeaningfulLiveEnvelope(_ envelope: AIToolUsageEnvelope) -> Bool {
        switch envelope.status {
        case "running":
            return true
        default:
            return false
        }
    }

    private func isCurrentContextEligibleEnvelope(_ envelope: AIToolUsageEnvelope) -> Bool {
        switch envelope.status {
        case "running":
            return true
        default:
            return false
        }
    }
}
