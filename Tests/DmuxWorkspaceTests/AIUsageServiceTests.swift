import XCTest
@testable import DmuxWorkspace

final class AIUsageServiceTests: XCTestCase {
    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!

    override func setUpWithError() throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-ai-usage-service-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: temporaryDirectoryURL, withIntermediateDirectories: true)
        databaseURL = temporaryDirectoryURL.appendingPathComponent("ai-usage.sqlite3", isDirectory: false)
    }

    override func tearDownWithError() throws {
        if let temporaryDirectoryURL {
            try? FileManager.default.removeItem(at: temporaryDirectoryURL)
        }
        temporaryDirectoryURL = nil
        databaseURL = nil
    }

    func testSnapshotBackedPanelStateRemainsScopedToSelectedProject() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let sharedFilePath = "/tmp/shared-claude-history.jsonl"
        let modifiedAt = 1_713_690_000.0
        let indexedAt = Date(timeIntervalSince1970: 1_713_690_123)

        let projectA = makeProject(name: "Project A", path: "/tmp/project-a")
        let projectB = makeProject(name: "Project B", path: "/tmp/project-b")

        store.deleteExternalSummaries(projectPath: projectA.path)
        store.deleteExternalSummaries(projectPath: projectB.path)
        store.deleteProjectIndexState(projectID: projectA.id)
        store.deleteProjectIndexState(projectID: projectB.id)

        store.saveExternalSummary(
            AIExternalFileSummary(
                source: "claude",
                filePath: sharedFilePath,
                fileModifiedAt: modifiedAt,
                projectPath: projectA.path,
                usageBuckets: [makeUsageBucket(project: projectA, externalSessionID: "a-1", totalTokens: 111)],
                sessions: [makeSessionSummary(project: projectA, externalSessionID: "a-1", totalTokens: 111)],
                dayUsage: [AIHeatmapDay(day: Calendar.autoupdatingCurrent.startOfDay(for: Date()), totalTokens: 111, requestCount: 1)],
                timeBuckets: []
            )
        )
        store.saveExternalSummary(
            AIExternalFileSummary(
                source: "claude",
                filePath: sharedFilePath,
                fileModifiedAt: modifiedAt,
                projectPath: projectB.path,
                usageBuckets: [makeUsageBucket(project: projectB, externalSessionID: "b-1", totalTokens: 222)],
                sessions: [makeSessionSummary(project: projectB, externalSessionID: "b-1", totalTokens: 222)],
                dayUsage: [AIHeatmapDay(day: Calendar.autoupdatingCurrent.startOfDay(for: Date()), totalTokens: 222, requestCount: 1)],
                timeBuckets: []
            )
        )

        store.saveProjectIndexState(for:
            makeIndexedSnapshot(project: projectA, totalTokens: 111, indexedAt: indexedAt),
            projectPath: projectA.path
        )
        store.saveProjectIndexState(for:
            makeIndexedSnapshot(project: projectB, totalTokens: 222, indexedAt: indexedAt),
            projectPath: projectB.path
        )

        let panelA = service.snapshotBackedPanelState(
            project: projectA,
            liveSnapshots: [],
            currentSnapshot: nil,
            status: .completed(detail: "done")
        )
        let panelB = service.snapshotBackedPanelState(
            project: projectB,
            liveSnapshots: [],
            currentSnapshot: nil,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(panelA.projectSummary?.projectID, projectA.id)
        XCTAssertEqual(panelA.projectSummary?.projectTotalTokens, 111)
        XCTAssertEqual(panelA.projectSummary?.projectCachedInputTokens, 0)
        XCTAssertEqual(panelA.projectSummary?.todayTotalTokens, 111)
        XCTAssertEqual(panelA.sessions.map(\.totalTokens), [111])

        XCTAssertEqual(panelB.projectSummary?.projectID, projectB.id)
        XCTAssertEqual(panelB.projectSummary?.projectTotalTokens, 222)
        XCTAssertEqual(panelB.projectSummary?.projectCachedInputTokens, 0)
        XCTAssertEqual(panelB.projectSummary?.todayTotalTokens, 222)
        XCTAssertEqual(panelB.sessions.map(\.totalTokens), [222])
    }

    func testLightweightLivePanelStateCarriesCachedOverlayTokensSeparately() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let project = makeProject(name: "Project A", path: "/tmp/project-a")

        let baselineState = AIStatsPanelState(
            projectSummary: AIProjectUsageSummary(
                projectID: project.id,
                projectName: project.name,
                currentSessionTokens: 0,
                currentSessionCachedInputTokens: 0,
                projectTotalTokens: 500,
                projectCachedInputTokens: 120,
                todayTotalTokens: 300,
                todayCachedInputTokens: 80,
                currentTool: nil,
                currentModel: nil,
                currentContextUsagePercent: nil,
                currentContextUsedTokens: nil,
                currentContextWindow: nil,
                currentSessionUpdatedAt: nil
            ),
            currentSnapshot: nil,
            liveSnapshots: [],
            liveOverlayTokens: 0,
            liveOverlayCachedInputTokens: 0,
            sessions: [],
            heatmap: [],
            todayTimeBuckets: [],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: nil,
            indexingStatus: .completed(detail: "done")
        )

        let liveSnapshot = AITerminalSessionSnapshot(
            sessionID: UUID(),
            externalSessionID: "claude-1",
            projectID: project.id,
            projectName: project.name,
            sessionTitle: "Live",
            tool: "claude",
            model: "claude-sonnet-4-6",
            status: "running",
            isRunning: true,
            startedAt: nil,
            updatedAt: Date(),
            currentInputTokens: 40,
            currentOutputTokens: 15,
            currentTotalTokens: 55,
            currentCachedInputTokens: 20,
            baselineInputTokens: 10,
            baselineOutputTokens: 5,
            baselineTotalTokens: 15,
            baselineCachedInputTokens: 8,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: false,
            hasCompletedTurn: false
        )

        let nextState = service.lightweightLivePanelState(
            from: baselineState,
            project: project,
            liveSnapshots: [liveSnapshot],
            currentSnapshot: liveSnapshot,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(nextState.projectSummary?.projectTotalTokens, 540)
        XCTAssertEqual(nextState.projectSummary?.projectCachedInputTokens, 132)
        XCTAssertEqual(nextState.projectSummary?.todayTotalTokens, 340)
        XCTAssertEqual(nextState.projectSummary?.todayCachedInputTokens, 92)
        XCTAssertEqual(nextState.projectSummary?.currentSessionTokens, 55)
        XCTAssertEqual(nextState.projectSummary?.currentSessionCachedInputTokens, 20)
        XCTAssertEqual(nextState.currentSnapshot?.currentTotalTokens, 55)
        XCTAssertEqual(nextState.currentSnapshot?.currentCachedInputTokens, 20)
    }

    func testLightweightLivePanelStateFallbackSummaryIncludesLiveOverlay() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let project = makeProject(name: "Project A", path: "/tmp/project-a")

        let baselineState = AIStatsPanelState(
            projectSummary: nil,
            currentSnapshot: nil,
            liveSnapshots: [],
            liveOverlayTokens: 0,
            liveOverlayCachedInputTokens: 0,
            sessions: [makeSessionSummary(project: project, externalSessionID: "a-1", totalTokens: 500)],
            heatmap: [],
            todayTimeBuckets: [],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: nil,
            indexingStatus: .completed(detail: "done")
        )

        let liveSnapshot = AITerminalSessionSnapshot(
            sessionID: UUID(),
            externalSessionID: "claude-1",
            projectID: project.id,
            projectName: project.name,
            sessionTitle: "Live",
            tool: "claude",
            model: "claude-sonnet-4-6",
            status: "running",
            isRunning: true,
            startedAt: nil,
            updatedAt: Date(),
            currentInputTokens: 40,
            currentOutputTokens: 15,
            currentTotalTokens: 55,
            currentCachedInputTokens: 20,
            baselineInputTokens: 10,
            baselineOutputTokens: 5,
            baselineTotalTokens: 15,
            baselineCachedInputTokens: 8,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: false,
            hasCompletedTurn: false
        )

        let nextState = service.lightweightLivePanelState(
            from: baselineState,
            project: project,
            liveSnapshots: [liveSnapshot],
            currentSnapshot: liveSnapshot,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(nextState.projectSummary?.projectTotalTokens, 540)
        XCTAssertEqual(nextState.projectSummary?.projectCachedInputTokens, 12)
        XCTAssertEqual(nextState.projectSummary?.todayTotalTokens, 40)
        XCTAssertEqual(nextState.projectSummary?.todayCachedInputTokens, 12)
        XCTAssertEqual(nextState.projectSummary?.currentSessionTokens, 55)
        XCTAssertEqual(nextState.projectSummary?.currentSessionCachedInputTokens, 20)
        XCTAssertEqual(nextState.currentSnapshot?.currentTotalTokens, 55)
        XCTAssertEqual(nextState.currentSnapshot?.currentCachedInputTokens, 20)
    }

    func testLightweightLivePanelStatePreservesCompletedOverlayUntilIndexedRefresh() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let project = makeProject(name: "Project A", path: "/tmp/project-a")

        let baselineState = AIStatsPanelState(
            projectSummary: AIProjectUsageSummary(
                projectID: project.id,
                projectName: project.name,
                currentSessionTokens: 40,
                currentSessionCachedInputTokens: 12,
                projectTotalTokens: 540,
                projectCachedInputTokens: 132,
                todayTotalTokens: 340,
                todayCachedInputTokens: 92,
                currentTool: "claude",
                currentModel: "claude-sonnet-4-6",
                currentContextUsagePercent: nil,
                currentContextUsedTokens: nil,
                currentContextWindow: nil,
                currentSessionUpdatedAt: nil
            ),
            currentSnapshot: nil,
            liveSnapshots: [],
            liveOverlayTokens: 40,
            liveOverlayCachedInputTokens: 12,
            sessions: [],
            heatmap: [],
            todayTimeBuckets: [],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: nil,
            indexingStatus: .completed(detail: "done")
        )

        let completedSnapshot = AITerminalSessionSnapshot(
            sessionID: UUID(),
            externalSessionID: "claude-1",
            projectID: project.id,
            projectName: project.name,
            sessionTitle: "Live",
            tool: "claude",
            model: "claude-sonnet-4-6",
            status: "completed",
            isRunning: false,
            startedAt: nil,
            updatedAt: Date(),
            currentInputTokens: 40,
            currentOutputTokens: 15,
            currentTotalTokens: 55,
            currentCachedInputTokens: 20,
            baselineInputTokens: 40,
            baselineOutputTokens: 15,
            baselineTotalTokens: 55,
            baselineCachedInputTokens: 20,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: false,
            hasCompletedTurn: true
        )

        let nextState = service.lightweightLivePanelState(
            from: baselineState,
            project: project,
            liveSnapshots: [completedSnapshot],
            currentSnapshot: completedSnapshot,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(nextState.projectSummary?.projectTotalTokens, 540)
        XCTAssertEqual(nextState.projectSummary?.projectCachedInputTokens, 132)
        XCTAssertEqual(nextState.projectSummary?.todayTotalTokens, 340)
        XCTAssertEqual(nextState.projectSummary?.todayCachedInputTokens, 92)
        XCTAssertEqual(nextState.projectSummary?.currentSessionTokens, 55)
        XCTAssertEqual(nextState.projectSummary?.currentSessionCachedInputTokens, 20)
        XCTAssertEqual(nextState.currentSnapshot?.currentTotalTokens, 55)
        XCTAssertEqual(nextState.currentSnapshot?.currentCachedInputTokens, 20)
    }

    func testLightweightLivePanelStateDoesNotDoubleCountIndexedLiveSession() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let indexedSession = makeSessionSummary(
            project: project,
            externalSessionID: "codex-session-1",
            totalTokens: 80,
            cachedInputTokens: 20,
            lastTool: "codex"
        )

        let baselineState = AIStatsPanelState(
            projectSummary: AIProjectUsageSummary(
                projectID: project.id,
                projectName: project.name,
                currentSessionTokens: 0,
                currentSessionCachedInputTokens: 0,
                projectTotalTokens: 80,
                projectCachedInputTokens: 20,
                todayTotalTokens: 80,
                todayCachedInputTokens: 20,
                currentTool: nil,
                currentModel: nil,
                currentContextUsagePercent: nil,
                currentContextUsedTokens: nil,
                currentContextWindow: nil,
                currentSessionUpdatedAt: nil
            ),
            currentSnapshot: nil,
            liveSnapshots: [],
            liveOverlayTokens: 0,
            liveOverlayCachedInputTokens: 0,
            sessions: [indexedSession],
            heatmap: [],
            todayTimeBuckets: [],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: Date(),
            indexingStatus: .completed(detail: "done")
        )

        let liveSnapshot = AITerminalSessionSnapshot(
            sessionID: UUID(),
            externalSessionID: "codex-session-1",
            projectID: project.id,
            projectName: project.name,
            sessionTitle: "Live",
            tool: "codex",
            model: "gpt-5.5",
            status: "running",
            isRunning: true,
            startedAt: nil,
            updatedAt: Date(),
            currentInputTokens: 120,
            currentOutputTokens: 10,
            currentTotalTokens: 100,
            currentCachedInputTokens: 25,
            baselineInputTokens: 0,
            baselineOutputTokens: 0,
            baselineTotalTokens: 0,
            baselineCachedInputTokens: 0,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: false,
            hasCompletedTurn: false
        )

        let nextState = service.lightweightLivePanelState(
            from: baselineState,
            project: project,
            liveSnapshots: [liveSnapshot],
            currentSnapshot: liveSnapshot,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(nextState.liveOverlayTokens, 20)
        XCTAssertEqual(nextState.liveOverlayCachedInputTokens, 5)
        XCTAssertEqual(nextState.projectSummary?.projectTotalTokens, 100)
        XCTAssertEqual(nextState.projectSummary?.projectCachedInputTokens, 25)
        XCTAssertEqual(nextState.projectSummary?.todayTotalTokens, 100)
        XCTAssertEqual(nextState.projectSummary?.todayCachedInputTokens, 25)
    }

    func testLightweightLivePanelStateResetsStaleCachedTodayTotalsAfterDayChangesWithoutRestart() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)
        let yesterdayBucketStart = calendar.date(byAdding: .hour, value: 22, to: yesterday) ?? yesterday
        let yesterdayBucketEnd = calendar.date(byAdding: .minute, value: 30, to: yesterdayBucketStart) ?? yesterdayBucketStart

        let baselineState = AIStatsPanelState(
            projectSummary: AIProjectUsageSummary(
                projectID: project.id,
                projectName: project.name,
                currentSessionTokens: 0,
                currentSessionCachedInputTokens: 0,
                projectTotalTokens: 500,
                projectCachedInputTokens: 40,
                todayTotalTokens: 500,
                todayCachedInputTokens: 40,
                currentTool: nil,
                currentModel: nil,
                currentContextUsagePercent: nil,
                currentContextUsedTokens: nil,
                currentContextWindow: nil,
                currentSessionUpdatedAt: yesterdayBucketEnd
            ),
            currentSnapshot: nil,
            liveSnapshots: [],
            liveOverlayTokens: 0,
            liveOverlayCachedInputTokens: 0,
            sessions: [],
            heatmap: [
                AIHeatmapDay(
                    day: yesterday,
                    totalTokens: 500,
                    cachedInputTokens: 40,
                    requestCount: 1
                )
            ],
            todayTimeBuckets: [
                AITimeBucket(
                    start: yesterdayBucketStart,
                    end: yesterdayBucketEnd,
                    totalTokens: 500,
                    cachedInputTokens: 40,
                    requestCount: 1
                )
            ],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: yesterdayBucketEnd,
            indexingStatus: .completed(detail: "done")
        )

        let nextState = service.lightweightLivePanelState(
            from: baselineState,
            project: project,
            liveSnapshots: [],
            currentSnapshot: nil,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(nextState.projectSummary?.projectTotalTokens, 500)
        XCTAssertEqual(nextState.projectSummary?.projectCachedInputTokens, 40)
        XCTAssertEqual(nextState.projectSummary?.todayTotalTokens, 0)
        XCTAssertEqual(nextState.projectSummary?.todayCachedInputTokens, 0)
    }

    func testLightweightLivePanelStateCountsOnlyPostMidnightGrowthForLongRunningLiveSession() {
        let store = AIUsageStore(databaseURL: databaseURL)
        let service = AIUsageService(wrapperStore: store)
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)
        let yesterdayBucketStart = calendar.date(byAdding: .hour, value: 22, to: yesterday) ?? yesterday
        let yesterdayBucketEnd = calendar.date(byAdding: .minute, value: 30, to: yesterdayBucketStart) ?? yesterdayBucketStart
        let terminalID = UUID()

        let baselineState = AIStatsPanelState(
            projectSummary: AIProjectUsageSummary(
                projectID: project.id,
                projectName: project.name,
                currentSessionTokens: 0,
                currentSessionCachedInputTokens: 0,
                projectTotalTokens: 500,
                projectCachedInputTokens: 40,
                todayTotalTokens: 500,
                todayCachedInputTokens: 40,
                currentTool: nil,
                currentModel: nil,
                currentContextUsagePercent: nil,
                currentContextUsedTokens: nil,
                currentContextWindow: nil,
                currentSessionUpdatedAt: yesterdayBucketEnd
            ),
            currentSnapshot: nil,
            liveSnapshots: [],
            liveOverlayTokens: 0,
            liveOverlayCachedInputTokens: 0,
            sessions: [],
            heatmap: [
                AIHeatmapDay(
                    day: yesterday,
                    totalTokens: 500,
                    cachedInputTokens: 40,
                    requestCount: 1
                )
            ],
            todayTimeBuckets: [
                AITimeBucket(
                    start: yesterdayBucketStart,
                    end: yesterdayBucketEnd,
                    totalTokens: 500,
                    cachedInputTokens: 40,
                    requestCount: 1
                )
            ],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: yesterdayBucketEnd,
            indexingStatus: .completed(detail: "done")
        )

        let midnightBaselineSnapshot = makeLiveSnapshot(
            sessionID: terminalID,
            project: project,
            externalSessionID: "codex-long-running",
            startedAt: yesterdayBucketStart,
            updatedAt: today.addingTimeInterval(60),
            totalTokens: 700,
            cachedInputTokens: 50
        )
        let midnightState = service.lightweightLivePanelState(
            from: baselineState,
            project: project,
            liveSnapshots: [midnightBaselineSnapshot],
            currentSnapshot: midnightBaselineSnapshot,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(midnightState.projectSummary?.projectTotalTokens, 1_200)
        XCTAssertEqual(midnightState.projectSummary?.projectCachedInputTokens, 90)
        XCTAssertEqual(midnightState.projectSummary?.todayTotalTokens, 0)
        XCTAssertEqual(midnightState.projectSummary?.todayCachedInputTokens, 0)

        let todayGrowthSnapshot = makeLiveSnapshot(
            sessionID: terminalID,
            project: project,
            externalSessionID: "codex-long-running",
            startedAt: yesterdayBucketStart,
            updatedAt: today.addingTimeInterval(120),
            totalTokens: 760,
            cachedInputTokens: 55
        )
        let growthState = service.lightweightLivePanelState(
            from: midnightState,
            project: project,
            liveSnapshots: [todayGrowthSnapshot],
            currentSnapshot: todayGrowthSnapshot,
            status: .completed(detail: "done")
        )

        XCTAssertEqual(growthState.projectSummary?.projectTotalTokens, 1_260)
        XCTAssertEqual(growthState.projectSummary?.projectCachedInputTokens, 95)
        XCTAssertEqual(growthState.projectSummary?.todayTotalTokens, 60)
        XCTAssertEqual(growthState.projectSummary?.todayCachedInputTokens, 5)
    }

    private func makeProject(name: String, path: String) -> Project {
        Project(
            id: UUID(),
            name: name,
            path: path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
    }

    private func makeLiveSnapshot(
        sessionID: UUID,
        project: Project,
        externalSessionID: String,
        startedAt: Date,
        updatedAt: Date,
        totalTokens: Int,
        cachedInputTokens: Int
    ) -> AITerminalSessionSnapshot {
        AITerminalSessionSnapshot(
            sessionID: sessionID,
            externalSessionID: externalSessionID,
            projectID: project.id,
            projectName: project.name,
            sessionTitle: "Live",
            tool: "codex",
            model: "gpt-5.5",
            status: "running",
            isRunning: true,
            startedAt: startedAt,
            updatedAt: updatedAt,
            currentInputTokens: totalTokens,
            currentOutputTokens: 0,
            currentTotalTokens: totalTokens,
            currentCachedInputTokens: cachedInputTokens,
            baselineInputTokens: 0,
            baselineOutputTokens: 0,
            baselineTotalTokens: 0,
            baselineCachedInputTokens: 0,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: false,
            hasCompletedTurn: false
        )
    }

    private func makeSessionSummary(
        project: Project,
        externalSessionID: String,
        totalTokens: Int,
        cachedInputTokens: Int = 0,
        lastTool: String = "claude"
    ) -> AISessionSummary {
        AISessionSummary(
            sessionID: UUID(),
            externalSessionID: externalSessionID,
            projectID: project.id,
            projectName: project.name,
            sessionTitle: externalSessionID,
            firstSeenAt: Date(timeIntervalSince1970: 1_713_600_000),
            lastSeenAt: Date(timeIntervalSince1970: 1_713_600_100),
            lastTool: lastTool,
            lastModel: "claude-sonnet-4-6",
            requestCount: 1,
            totalInputTokens: totalTokens,
            totalOutputTokens: 0,
            totalTokens: totalTokens,
            cachedInputTokens: cachedInputTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 60,
            todayTokens: totalTokens,
            todayCachedInputTokens: cachedInputTokens
        )
    }

    private func makeUsageBucket(project: Project, externalSessionID: String, totalTokens: Int) -> AIUsageBucket {
        let start = Calendar.autoupdatingCurrent.date(bySettingHour: 10, minute: 0, second: 0, of: Date()) ?? Date()
        let end = Calendar.autoupdatingCurrent.date(byAdding: .hour, value: 1, to: start) ?? start
        return AIUsageBucket(
            source: "claude",
            sessionKey: externalSessionID,
            externalSessionID: externalSessionID,
            sessionTitle: externalSessionID,
            model: "claude-sonnet-4-6",
            projectID: project.id,
            projectName: project.name,
            bucketStart: start,
            bucketEnd: end,
            inputTokens: totalTokens,
            outputTokens: 0,
            totalTokens: totalTokens,
            cachedInputTokens: 0,
            requestCount: 1,
            activeDurationSeconds: 60,
            firstSeenAt: start,
            lastSeenAt: start.addingTimeInterval(60)
        )
    }

    private func makeIndexedSnapshot(project: Project, totalTokens: Int, indexedAt: Date) -> AIIndexedProjectSnapshot {
        AIIndexedProjectSnapshot(
            projectID: project.id,
            projectName: project.name,
            projectSummary: AIProjectUsageSummary(
                projectID: project.id,
                projectName: project.name,
                currentSessionTokens: 0,
                projectTotalTokens: totalTokens,
                todayTotalTokens: totalTokens,
                currentTool: nil,
                currentModel: nil,
                currentContextUsagePercent: nil,
                currentContextUsedTokens: nil,
                currentContextWindow: nil,
                currentSessionUpdatedAt: nil
            ),
            sessions: [],
            heatmap: [],
            todayTimeBuckets: [],
            toolBreakdown: [],
            modelBreakdown: [],
            indexedAt: indexedAt
        )
    }
}
