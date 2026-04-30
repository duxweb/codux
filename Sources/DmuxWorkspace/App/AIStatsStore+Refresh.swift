import Foundation

@MainActor
extension AIStatsStore {
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
        _ = ingestRuntime(project: project, projects: projects, selectedSessionID: currentSelectedSessionID)
        let liveContext = liveSnapshotContext(projectID: project.id, selectedSessionID: currentSelectedSessionID)
        var nextState = aiUsageService.snapshotBackedPanelState(
            project: project,
            liveSnapshots: liveContext.summary,
            currentSnapshot: liveContext.current,
            status: cancelledStatus
        )
        nextState.liveSnapshots = liveContext.display
        storeState(nextState, refreshState: .idle, for: project.id, updateCurrent: true)
        setRefreshFlags(projectID: project.id, automatic: false, manual: false)
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
        aiUsageStore.deleteProjectIndexState(projectID: project.id)
        aiUsageStore.deleteExternalSummaries(projectPath: project.path)
        panelStateByProjectID[project.id] = nil
        refreshStateByProjectID[project.id] = .idle
        cachedPanels.removeValue(for: project.id)
        if currentProjectID == project.id {
            refreshState = .idle
        }
    }

    func refresh(
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
        let liveSnapshots = ingestRuntime(project: project, projects: projects, selectedSessionID: selectedSessionID)

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

        setRefreshFlags(
            projectID: project.id,
            automatic: isAutomaticTrigger(trigger),
            manual: trigger == .manual || trigger == .initial
        )
        syncCurrentAutomaticRefreshFlag()
        logger.log(
            "history-refresh",
            "start trigger=\(refreshTriggerName(trigger)) project=\(project.id.uuidString) name=\(project.name) force=\(force) live=\(liveSnapshots.count) selectedSession=\(selectedSessionID?.uuidString ?? "nil")"
        )

        let indexingProfile: AIProjectHistoryIndexingProfile = trigger == .background
            ? .background
            : .foreground

        let runningStatus = AIIndexingStatus.indexing(progress: 0.0, detail: String(localized: "ai.indexing.starting", defaultValue: "Starting index.", bundle: .module))
        indexingStatusByProjectID[project.id] = runningStatus
        if var runningState = panelStateByProjectID[project.id] {
            runningState.indexingStatus = runningStatus
            let runningRefreshState = inFlightRefreshState(for: trigger, current: refreshStateByProjectID[project.id])
            storeState(runningState, refreshState: runningRefreshState, for: project.id, updateCurrent: true)
        }

        refreshTasks[project.id] = Task(priority: indexingProfile.taskPriority) {
            let startedAt = Date()
            defer {
                Task { @MainActor in
                    self.refreshTasks[project.id] = nil
                }
            }

            let service = AIUsageService()
            let liveContext = await MainActor.run {
                self.liveSnapshotContext(projectID: project.id, selectedSessionID: selectedSessionID)
            }
            var quickState = await Task.detached(priority: .userInitiated) {
                service.fastPanelState(project: project, liveSnapshots: liveContext.summary, currentSnapshot: liveContext.current)
            }.value
            quickState.liveSnapshots = liveContext.display
            await MainActor.run {
                self.indexingStatusByProjectID[project.id] = quickState.indexingStatus
                self.panelStateByProjectID[project.id] = quickState
                self.refreshStateByProjectID[project.id] = self.inFlightRefreshState(for: trigger, current: self.refreshStateByProjectID[project.id])
                self.cacheState(quickState, for: project.id)
                if self.currentProjectID == project.id {
                    self.state = quickState
                    self.refreshState = self.inFlightRefreshState(for: trigger, current: self.refreshStateByProjectID[project.id])
                }
            }

            let resultState = await service.panelState(
                project: project,
                liveSnapshots: liveContext.summary,
                currentSnapshot: liveContext.current,
                indexingProfile: indexingProfile
            ) { status in
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
                self.setRefreshFlags(projectID: project.id, automatic: false, manual: false)
                self.syncCurrentAutomaticRefreshFlag()
                let nextRefreshState: PanelRefreshState
                if case .failed(let detail) = finalStatus {
                    nextRefreshState = .failed(detail)
                } else {
                    self.lastCompletedRefreshAtByProjectID[project.id] = Date()
                    nextRefreshState = .idle
                }
                let liveContext = self.liveSnapshotContext(projectID: project.id, selectedSessionID: selectedSessionID)
                var nextState = service.snapshotBackedPanelState(
                    project: project,
                    liveSnapshots: liveContext.summary,
                    currentSnapshot: liveContext.current,
                    status: finalStatus
                )
                nextState.liveSnapshots = liveContext.display
                self.storeState(nextState, refreshState: nextRefreshState, for: project.id, updateCurrent: self.currentProjectID == project.id)
                let durationMS = Int(Date().timeIntervalSince(startedAt) * 1000)
                self.logger.log(
                    "history-refresh",
                    "finish trigger=\(self.refreshTriggerName(trigger)) project=\(project.id.uuidString) result=\(self.refreshResultName(finalStatus)) projectTotal=\(nextState.projectSummary?.projectTotalTokens ?? 0) todayTotal=\(nextState.projectSummary?.todayTotalTokens ?? 0) sessions=\(nextState.sessions.count) live=\(nextState.liveSnapshots.count) indexed=\(nextState.indexedAt != nil) durationMs=\(durationMS)"
                )
                if self.currentProjectID == project.id {
                    self.refreshLiveState(
                        project: project,
                        selectedSessionID: selectedSessionID,
                        reason: .runtimeBridge
                    )
                }
            }
        }
    }

    func cacheState(_ state: AIStatsPanelState, for projectID: UUID) {
        cachedPanels.set(state, for: projectID)
    }

    func storeState(_ newState: AIStatsPanelState, refreshState newRefreshState: PanelRefreshState, for projectID: UUID, updateCurrent: Bool) {
        let titlebarDidChange = refreshTitlebarTodayLiveOverlay(notify: false)
        panelStateByProjectID[projectID] = newState
        refreshStateByProjectID[projectID] = newRefreshState
        cacheState(newState, for: projectID)
        if updateCurrent {
            state = newState
            refreshState = newRefreshState
            renderVersion &+= 1
        } else if titlebarDidChange || newState.indexedAt != nil {
            renderVersion &+= 1
        }
    }

    func updatePanelState(projectID: UUID, transform: (inout AIStatsPanelState) -> Void) {
        guard var nextState = panelStateByProjectID[projectID] ?? cachedPanels.value(for: projectID) else {
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

    func cachedState(for projectID: UUID) -> AIStatsPanelState? {
        if let state = panelStateByProjectID[projectID] {
            return state
        }
        return cachedPanels.peekValue(for: projectID)
    }

    func relocalizedStatus(_ status: AIIndexingStatus) -> AIIndexingStatus {
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

    func resolvedTodayTotalTokens(for state: AIStatsPanelState) -> Int {
        resolvedTodayTotalTokens(
            summary: state.projectSummary?.todayTotalTokens ?? 0,
            timeBuckets: state.todayTimeBuckets,
            heatmap: state.heatmap
        )
    }

    func resolvedTodayTotalTokens(summary: Int, timeBuckets: [AITimeBucket], heatmap: [AIHeatmapDay]) -> Int {
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let bucketTotal = timeBuckets.reduce(0) { partial, bucket in
            guard calendar.isDate(bucket.start, inSameDayAs: today) else {
                return partial
            }
            return partial + bucket.totalTokens
        }
        let heatmapToday = heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) })?.totalTokens ?? 0
        return max(max(0, summary), max(bucketTotal, heatmapToday))
    }

    func resolvedDisplayedTodayTotalTokens(for state: AIStatsPanelState) -> Int {
        resolvedDisplayedTodayTotalTokens(
            summary: state.projectSummary?.todayTotalTokens ?? 0,
            summaryCached: state.projectSummary?.todayCachedInputTokens ?? 0,
            timeBuckets: state.todayTimeBuckets,
            heatmap: state.heatmap
        )
    }

    func resolvedDisplayedTodayTotalTokens(
        summary: Int,
        summaryCached: Int,
        timeBuckets: [AITimeBucket],
        heatmap: [AIHeatmapDay]
    ) -> Int {
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let bucketTotal = timeBuckets.reduce(0) { partial, bucket in
            guard calendar.isDate(bucket.start, inSameDayAs: today) else {
                return partial
            }
            return partial + bucket.totalTokens + bucket.cachedInputTokens
        }
        let heatmapTotal: Int = {
            guard let heatmapToday = heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) }) else {
                return 0
            }
            return heatmapToday.totalTokens + heatmapToday.cachedInputTokens
        }()

        return max(max(0, summary) + max(0, summaryCached), max(bucketTotal, heatmapTotal))
    }

    func clearCurrentState() {
        state = .empty
        refreshState = .idle
        isAutomaticRefreshInProgress = false
    }

    func updateSelectionContext(project: Project?, projects: [Project], selectedSessionID: UUID?) {
        currentProjects = projects
        currentProjectID = project?.id
        currentSelectedSessionID = selectedSessionID
    }

    func liveSnapshotContext(projectID: UUID, selectedSessionID: UUID?) -> LiveSnapshotContext {
        (
            display: aiSessionStore.liveSnapshots(projectID: projectID),
            summary: aiSessionStore.liveAggregationSnapshots(projectID: projectID),
            current: aiSessionStore.currentDisplaySnapshot(projectID: projectID, selectedSessionID: selectedSessionID)
        )
    }

    func syncCurrentAutomaticRefreshFlag() {
        guard let currentProjectID else {
            isAutomaticRefreshInProgress = false
            return
        }
        isAutomaticRefreshInProgress = isAutomaticRefreshInProgress(projectID: currentProjectID)
    }

    func setRefreshFlags(projectID: UUID, automatic: Bool, manual: Bool) {
        automaticRefreshInProgressByProjectID[projectID] = automatic
        manualRefreshInProgressByProjectID[projectID] = manual
    }

    func isAutomaticRefreshInProgress(projectID: UUID) -> Bool {
        automaticRefreshInProgressByProjectID[projectID] ?? false
    }

    func projectIndexingStatus(projectID: UUID, fallback: AIIndexingStatus) -> AIIndexingStatus {
        indexingStatusByProjectID[projectID] ?? fallback
    }

    func restingRefreshState(projectID: UUID) -> PanelRefreshState {
        normalizedRestingRefreshState(refreshStateByProjectID[projectID])
    }

    func automaticRefreshDecision(
        projectID: UUID,
        state: AIStatsPanelState?,
        interval: TimeInterval,
        forceOnFirstOpen: Bool
    ) -> (shouldRefresh: Bool, reason: String) {
        if refreshTasks[projectID] != nil {
            return (false, "in-flight")
        }
        if forceOnFirstOpen {
            return (true, "first-open-this-launch")
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

    func normalizedRestingRefreshState(_ state: PanelRefreshState?) -> PanelRefreshState {
        switch state {
        case .failed(let detail):
            return .failed(detail)
        default:
            return .idle
        }
    }

    func isAutomaticTrigger(_ trigger: RefreshTrigger) -> Bool {
        switch trigger {
        case .automatic, .background:
            return true
        case .initial, .manual:
            return false
        }
    }

    func inFlightRefreshState(for trigger: RefreshTrigger, current: PanelRefreshState?) -> PanelRefreshState {
        switch trigger {
        case .automatic:
            return .showingCached
        case .background:
            return normalizedRestingRefreshState(current)
        case .initial, .manual:
            return .refreshing
        }
    }

    func refreshTriggerName(_ trigger: RefreshTrigger) -> String {
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

    func refreshResultName(_ status: AIIndexingStatus) -> String {
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

    func formatInterval(_ value: TimeInterval) -> String {
        String(format: "%.1fs", value)
    }
}
