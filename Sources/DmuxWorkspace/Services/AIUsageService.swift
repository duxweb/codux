import Foundation

struct AIUsageService: Sendable {
    private let wrapperStore: AIUsageStore
    private let historyService: AIProjectHistoryService
    private let calendar = Calendar.autoupdatingCurrent

    init(
        wrapperStore: AIUsageStore = AIUsageStore(),
        historyService: AIProjectHistoryService? = nil
    ) {
        self.wrapperStore = wrapperStore
        self.historyService = historyService ?? AIProjectHistoryService(usageStore: wrapperStore)
    }

    func fastPanelState(project: Project, liveSnapshots: [AITerminalSessionSnapshot], currentSnapshot: AITerminalSessionSnapshot?) -> AIStatsPanelState {
        if let indexed = wrapperStore.indexedProjectSnapshot(projectID: project.id) {
            return overlayLiveSummary(
                on: indexed,
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: .indexing(progress: 0.0, detail: String(localized: "ai.indexing.starting", defaultValue: "Starting index.", bundle: .module))
            )
        }

        return liveSummaryOnlyState(
            project: project,
            indexed: nil,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: .indexing(progress: 0.0, detail: String(localized: "ai.indexing.starting", defaultValue: "Starting index.", bundle: .module))
        )
    }

    func snapshotBackedPanelState(project: Project, liveSnapshots: [AITerminalSessionSnapshot], currentSnapshot: AITerminalSessionSnapshot?, status: AIIndexingStatus) -> AIStatsPanelState {
        if let indexed = wrapperStore.indexedProjectSnapshot(projectID: project.id) {
            return overlayLiveSummary(
                on: indexed,
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: status
            )
        }

        return liveSummaryOnlyState(
            project: project,
            indexed: nil,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: status
        )
    }

    func lightweightLivePanelState(
        from currentState: AIStatsPanelState,
        project: Project,
        liveSnapshots: [AITerminalSessionSnapshot],
        currentSnapshot: AITerminalSessionSnapshot?,
        status: AIIndexingStatus
    ) -> AIStatsPanelState {
        let adjustedLiveSnapshots = adjustedLiveSnapshots(from: liveSnapshots)
        let adjustedCurrentSnapshot = adjustedCurrentSnapshot(
            from: currentSnapshot,
            adjustedLiveSnapshots: adjustedLiveSnapshots
        )
        let nextLiveOverlayTokens = adjustedLiveSnapshots.reduce(0) { $0 + $1.currentTotalTokens }
        let nextLiveOverlayCachedInputTokens = adjustedLiveSnapshots.reduce(0) { $0 + $1.currentCachedInputTokens }

        var nextState = currentState
        nextState.currentSnapshot = adjustedCurrentSnapshot
        nextState.liveSnapshots = adjustedLiveSnapshots
        nextState.liveOverlayTokens = nextLiveOverlayTokens
        nextState.liveOverlayCachedInputTokens = nextLiveOverlayCachedInputTokens
        nextState.indexingStatus = status

        if var summary = currentState.projectSummary, summary.projectID == project.id {
            let baseProjectTotal = max(0, summary.projectTotalTokens - currentState.liveOverlayTokens)
            let baseProjectCached = max(0, summary.projectCachedInputTokens - currentState.liveOverlayCachedInputTokens)
            let baseTodayTotal = max(0, summary.todayTotalTokens - currentState.liveOverlayTokens)
            let baseTodayCached = max(0, summary.todayCachedInputTokens - currentState.liveOverlayCachedInputTokens)
            summary.projectTotalTokens = baseProjectTotal + nextLiveOverlayTokens
            summary.projectCachedInputTokens = baseProjectCached + nextLiveOverlayCachedInputTokens
            summary.todayTotalTokens = baseTodayTotal + nextLiveOverlayTokens
            summary.todayCachedInputTokens = baseTodayCached + nextLiveOverlayCachedInputTokens
            summary.currentSessionTokens = adjustedCurrentSnapshot?.currentTotalTokens ?? 0
            summary.currentSessionCachedInputTokens = adjustedCurrentSnapshot?.currentCachedInputTokens ?? 0
            summary.currentTool = adjustedCurrentSnapshot?.tool
            summary.currentModel = adjustedCurrentSnapshot?.model
            summary.currentContextUsagePercent = adjustedCurrentSnapshot?.currentContextUsagePercent
            summary.currentContextUsedTokens = adjustedCurrentSnapshot?.currentContextUsedTokens
            summary.currentContextWindow = adjustedCurrentSnapshot?.currentContextWindow
            summary.currentSessionUpdatedAt = adjustedCurrentSnapshot?.updatedAt
            nextState.projectSummary = summary
        } else {
            nextState.projectSummary = baseProjectSummary(
                project: project,
                liveSnapshot: adjustedCurrentSnapshot,
                sessions: currentState.sessions,
                todayTotalTokens: todayTotalTokens(
                    timeBuckets: currentState.todayTimeBuckets,
                    heatmap: currentState.heatmap
                ),
                todayCachedInputTokens: todayCachedInputTokens(
                    timeBuckets: currentState.todayTimeBuckets,
                    heatmap: currentState.heatmap
                )
            )
        }

        return nextState
    }

    func panelState(
        project: Project,
        liveSnapshots: [AITerminalSessionSnapshot],
        currentSnapshot: AITerminalSessionSnapshot?,
        indexingProfile: AIProjectHistoryIndexingProfile = .foreground,
        onProgress: @Sendable @escaping (AIIndexingStatus) async -> Void
    ) async -> AIStatsPanelState {
        do {
            try Task.checkCancellation()
            await onProgress(.indexing(progress: 0.05, detail: String(localized: "ai.indexing.preparing", defaultValue: "Preparing usage data.", bundle: .module)))
            let directorySummary = try await historyService.loadProjectSummary(
                project: project,
                indexingProfile: indexingProfile,
                onProgress: onProgress
            )
            try Task.checkCancellation()
            let todayTotal = todayTotalTokens(
                timeBuckets: directorySummary.todayTimeBuckets,
                heatmap: directorySummary.heatmap
            )
            let indexedSnapshot = AIIndexedProjectSnapshot(
                projectID: project.id,
                projectName: project.name,
                projectSummary: AIProjectUsageSummary(
                    projectID: project.id,
                    projectName: project.name,
                    currentSessionTokens: 0,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: directorySummary.sessions.reduce(0) { $0 + $1.totalTokens },
                    projectCachedInputTokens: directorySummary.sessions.reduce(0) { $0 + $1.cachedInputTokens },
                    todayTotalTokens: todayTotal,
                    todayCachedInputTokens: todayCachedInputTokens(
                        timeBuckets: directorySummary.todayTimeBuckets,
                        heatmap: directorySummary.heatmap
                    ),
                    currentTool: nil,
                    currentModel: nil,
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: directorySummary.sessions.first?.lastSeenAt
                ),
                sessions: directorySummary.sessions,
                heatmap: directorySummary.heatmap,
                todayTimeBuckets: directorySummary.todayTimeBuckets,
                toolBreakdown: directorySummary.toolBreakdown,
                modelBreakdown: directorySummary.modelBreakdown,
                indexedAt: Date()
            )
            wrapperStore.saveProjectIndexState(for: indexedSnapshot, projectPath: project.path)

            return overlayLiveSummary(
                on: indexedSnapshot,
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))
            )
        } catch is CancellationError {
            return snapshotBackedPanelState(
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: .cancelled(detail: String(localized: "ai.indexing.stopped", defaultValue: "Indexing stopped.", bundle: .module))
            )
        } catch {
            return snapshotBackedPanelState(
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: .failed(detail: (error as NSError).localizedDescription)
            )
        }
    }

    private func overlayLiveSummary(
        on indexed: AIIndexedProjectSnapshot,
        project: Project,
        liveSnapshots: [AITerminalSessionSnapshot],
        currentSnapshot: AITerminalSessionSnapshot?,
        status: AIIndexingStatus
    ) -> AIStatsPanelState {
        liveSummaryOnlyState(
            project: project,
            indexed: indexed,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: status
        )
    }

    private func liveSummaryOnlyState(
        project: Project,
        indexed: AIIndexedProjectSnapshot?,
        liveSnapshots: [AITerminalSessionSnapshot],
        currentSnapshot: AITerminalSessionSnapshot?,
        status: AIIndexingStatus
    ) -> AIStatsPanelState {
        let adjustedLiveSnapshots = adjustedLiveSnapshots(from: liveSnapshots)
        let adjustedCurrentSnapshot = adjustedCurrentSnapshot(
            from: currentSnapshot,
            adjustedLiveSnapshots: adjustedLiveSnapshots
        )
        let totalLiveDelta = adjustedLiveSnapshots.reduce(0) { $0 + $1.currentTotalTokens }
        let totalLiveCachedDelta = adjustedLiveSnapshots.reduce(0) { $0 + $1.currentCachedInputTokens }

        var summary = indexed?.projectSummary ?? AIProjectUsageSummary(
            projectID: project.id,
            projectName: project.name,
            currentSessionTokens: 0,
            currentSessionCachedInputTokens: 0,
            projectTotalTokens: 0,
            projectCachedInputTokens: 0,
            todayTotalTokens: 0,
            todayCachedInputTokens: 0,
            currentTool: nil,
            currentModel: nil,
            currentContextUsagePercent: nil,
            currentContextUsedTokens: nil,
            currentContextWindow: nil,
            currentSessionUpdatedAt: nil
        )

        summary.projectID = project.id
        summary.projectName = project.name
        summary.projectTotalTokens = (indexed?.projectSummary.projectTotalTokens ?? 0) + totalLiveDelta
        summary.projectCachedInputTokens = (indexed?.projectSummary.projectCachedInputTokens ?? 0) + totalLiveCachedDelta
        summary.todayTotalTokens = todayTotalTokens(
            timeBuckets: indexed?.todayTimeBuckets ?? [],
            heatmap: indexed?.heatmap ?? []
        ) + totalLiveDelta
        summary.todayCachedInputTokens = todayCachedInputTokens(
            timeBuckets: indexed?.todayTimeBuckets ?? [],
            heatmap: indexed?.heatmap ?? []
        ) + totalLiveCachedDelta
        summary.currentSessionTokens = adjustedCurrentSnapshot?.currentTotalTokens ?? 0
        summary.currentSessionCachedInputTokens = adjustedCurrentSnapshot?.currentCachedInputTokens ?? 0
        summary.currentTool = adjustedCurrentSnapshot?.tool
        summary.currentModel = adjustedCurrentSnapshot?.model
        summary.currentContextUsagePercent = adjustedCurrentSnapshot?.currentContextUsagePercent
        summary.currentContextUsedTokens = adjustedCurrentSnapshot?.currentContextUsedTokens
        summary.currentContextWindow = adjustedCurrentSnapshot?.currentContextWindow
        summary.currentSessionUpdatedAt = adjustedCurrentSnapshot?.updatedAt ?? indexed?.projectSummary.currentSessionUpdatedAt

        return AIStatsPanelState(
            projectSummary: summary,
            currentSnapshot: adjustedCurrentSnapshot,
            liveSnapshots: adjustedLiveSnapshots,
            liveOverlayTokens: totalLiveDelta,
            liveOverlayCachedInputTokens: totalLiveCachedDelta,
            sessions: indexed?.sessions ?? [],
            heatmap: indexed?.heatmap ?? [],
            todayTimeBuckets: indexed?.todayTimeBuckets ?? [],
            toolBreakdown: indexed?.toolBreakdown ?? [],
            modelBreakdown: indexed?.modelBreakdown ?? [],
            indexedAt: indexed?.indexedAt,
            indexingStatus: status
        )
    }

    private func adjustedCurrentSnapshot(
        from currentSnapshot: AITerminalSessionSnapshot?,
        adjustedLiveSnapshots: [AITerminalSessionSnapshot]
    ) -> AITerminalSessionSnapshot? {
        guard let currentSnapshot else {
            return nil
        }
        return adjustedLiveSnapshots.first(where: { $0.sessionID == currentSnapshot.sessionID }) ?? currentSnapshot
    }

    private func adjustedLiveSnapshots(from liveSnapshots: [AITerminalSessionSnapshot]) -> [AITerminalSessionSnapshot] {
        liveSnapshots.map { snapshot in
            var adjustedSnapshot = snapshot
            adjustedSnapshot.currentInputTokens = max(0, snapshot.currentInputTokens - snapshot.baselineInputTokens)
            adjustedSnapshot.currentOutputTokens = max(0, snapshot.currentOutputTokens - snapshot.baselineOutputTokens)
            adjustedSnapshot.currentTotalTokens = max(0, snapshot.currentTotalTokens - snapshot.baselineTotalTokens)
            adjustedSnapshot.currentCachedInputTokens = max(0, snapshot.currentCachedInputTokens - snapshot.baselineCachedInputTokens)
            return adjustedSnapshot
        }
    }

    private func baseProjectSummary(
        project: Project,
        liveSnapshot: AITerminalSessionSnapshot?,
        sessions: [AISessionSummary],
        todayTotalTokens: Int,
        todayCachedInputTokens: Int
    ) -> AIProjectUsageSummary {
        AIProjectUsageSummary(
            projectID: project.id,
            projectName: project.name,
            currentSessionTokens: liveSnapshot?.currentTotalTokens ?? 0,
            currentSessionCachedInputTokens: liveSnapshot?.currentCachedInputTokens ?? 0,
            projectTotalTokens: sessions.reduce(0) { $0 + $1.totalTokens },
            projectCachedInputTokens: sessions.reduce(0) { $0 + $1.cachedInputTokens },
            todayTotalTokens: todayTotalTokens,
            todayCachedInputTokens: todayCachedInputTokens,
            currentTool: liveSnapshot?.tool,
            currentModel: liveSnapshot?.model,
            currentContextUsagePercent: liveSnapshot?.currentContextUsagePercent,
            currentContextUsedTokens: liveSnapshot?.currentContextUsedTokens,
            currentContextWindow: liveSnapshot?.currentContextWindow,
            currentSessionUpdatedAt: liveSnapshot?.updatedAt
        )
    }

    private func todayTotalTokens(timeBuckets: [AITimeBucket], heatmap: [AIHeatmapDay]) -> Int {
        let bucketTotal = timeBuckets.reduce(0) { $0 + $1.totalTokens }
        if bucketTotal > 0 {
            return bucketTotal
        }

        let today = calendar.startOfDay(for: Date())
        return heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) })?.totalTokens ?? 0
    }

    private func todayCachedInputTokens(timeBuckets: [AITimeBucket], heatmap: [AIHeatmapDay]) -> Int {
        let bucketTotal = timeBuckets.reduce(0) { $0 + $1.cachedInputTokens }
        if bucketTotal > 0 {
            return bucketTotal
        }

        let today = calendar.startOfDay(for: Date())
        return heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) })?.cachedInputTokens ?? 0
    }
}
