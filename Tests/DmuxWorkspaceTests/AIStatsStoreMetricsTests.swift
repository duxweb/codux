import XCTest
@testable import DmuxWorkspace

@MainActor
final class AIStatsStoreMetricsTests: XCTestCase {
    private final class ProjectListBox: @unchecked Sendable {
        var value: [Project]

        init(_ value: [Project]) {
            self.value = value
        }
    }

    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!

    override func setUp() async throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-ai-stats-metrics-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: temporaryDirectoryURL, withIntermediateDirectories: true)
        databaseURL = temporaryDirectoryURL.appendingPathComponent("ai-usage.sqlite3", isDirectory: false)
        AISessionStore.shared.reset()
    }

    override func tearDown() async throws {
        AISessionStore.shared.reset()
        if let temporaryDirectoryURL {
            try? FileManager.default.removeItem(at: temporaryDirectoryURL)
        }
        temporaryDirectoryURL = nil
        databaseURL = nil
    }

    func testResolvedTodayTotalTokensPrefersLiveSummaryOverIndexedBuckets() {
        let store = AIStatsStore()
        let today = Calendar.autoupdatingCurrent.startOfDay(for: Date())
        let bucketStart = Calendar.autoupdatingCurrent.date(byAdding: .hour, value: 10, to: today) ?? today
        let bucketEnd = Calendar.autoupdatingCurrent.date(byAdding: .hour, value: 1, to: bucketStart) ?? bucketStart

        let resolved = store.resolvedTodayTotalTokens(
            summary: 2_920_000,
            timeBuckets: [
                AITimeBucket(
                    start: bucketStart,
                    end: bucketEnd,
                    totalTokens: 1_400_000,
                    cachedInputTokens: 0,
                    requestCount: 1
                )
            ],
            heatmap: [
                AIHeatmapDay(
                    day: today,
                    totalTokens: 1_400_000,
                    cachedInputTokens: 0,
                    requestCount: 1
                )
            ]
        )

        XCTAssertEqual(resolved, 2_920_000)
    }

    func testResolvedDisplayedTodayTotalTokensPrefersLiveSummaryOverIndexedBuckets() {
        let store = AIStatsStore()
        let today = Calendar.autoupdatingCurrent.startOfDay(for: Date())
        let bucketStart = Calendar.autoupdatingCurrent.date(byAdding: .hour, value: 10, to: today) ?? today
        let bucketEnd = Calendar.autoupdatingCurrent.date(byAdding: .hour, value: 1, to: bucketStart) ?? bucketStart

        let resolved = store.resolvedDisplayedTodayTotalTokens(
            summary: 2_920_000,
            summaryCached: 120_000,
            timeBuckets: [
                AITimeBucket(
                    start: bucketStart,
                    end: bucketEnd,
                    totalTokens: 1_400_000,
                    cachedInputTokens: 60_000,
                    requestCount: 1
                )
            ],
            heatmap: [
                AIHeatmapDay(
                    day: today,
                    totalTokens: 1_400_000,
                    cachedInputTokens: 60_000,
                    requestCount: 1
                )
            ]
        )

        XCTAssertEqual(resolved, 3_040_000)
    }

    func testProjectTodayTokensUseCachedProjectStateIncludingLiveOverlay() {
        let store = makeStore()
        let project = Project(
            id: UUID(),
            name: "Codux",
            path: "/tmp/codux",
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )

        store.currentProjects = [project]
        store.cacheState(
            AIStatsPanelState(
                projectSummary: AIProjectUsageSummary(
                    projectID: project.id,
                    projectName: project.name,
                    currentSessionTokens: 1_390,
                    currentSessionCachedInputTokens: 190,
                    projectTotalTokens: 10_100_000,
                    projectCachedInputTokens: 1_200_000,
                    todayTotalTokens: 2_920_000,
                    todayCachedInputTokens: 120_000,
                    currentTool: "codex",
                    currentModel: "gpt-5.4",
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: nil
                ),
                currentSnapshot: nil,
                liveSnapshots: [],
                liveOverlayTokens: 100,
                liveOverlayCachedInputTokens: 20,
                sessions: [],
                heatmap: [
                    AIHeatmapDay(
                        day: Calendar.autoupdatingCurrent.startOfDay(for: Date()),
                        totalTokens: 2_820_000,
                        cachedInputTokens: 0,
                        requestCount: 1
                    )
                ],
                todayTimeBuckets: [
                    AITimeBucket(
                        start: Date(),
                        end: Date().addingTimeInterval(3600),
                        totalTokens: 2_820_000,
                        cachedInputTokens: 0,
                        requestCount: 1
                    )
                ],
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: nil,
                indexingStatus: .completed(detail: "done")
            ),
            for: project.id
        )

        seedLiveSession(
            terminalID: UUID(),
            project: project,
            externalSessionID: "cached-overlay-live",
            totalTokens: 100,
            baselineTotalTokens: 0
        )

        XCTAssertEqual(store.totalTodayNormalizedTokensAcrossProjects([project]), 2_920_000)
    }

    func testTitlebarTodayLevelTokensResetsStaleCachedTodayStateAfterDayChangesWithoutRestart() {
        let store = makeStore()
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)
        let yesterdayBucketStart = calendar.date(byAdding: .hour, value: 22, to: yesterday) ?? yesterday
        let yesterdayBucketEnd = calendar.date(byAdding: .minute, value: 30, to: yesterdayBucketStart) ?? yesterdayBucketStart

        store.currentProjects = [project]
        store.cacheState(
            AIStatsPanelState(
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
            ),
            for: project.id
        )

        XCTAssertEqual(store.titlebarTodayLevelTokens(), 0)
        XCTAssertEqual(store.totalTodayDisplayedTokensAcrossProjects([project]), 0)
    }

    func testProjectTodayTokensCountOnlyPostMidnightLiveGrowthFromCachedBaseline() {
        let store = makeStore()
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)
        let yesterdayBucketEnd = calendar.date(byAdding: .hour, value: 23, to: yesterday) ?? yesterday
        let baselineKey = "codex|long-live"

        store.currentProjects = [project]
        store.cacheState(
            AIStatsPanelState(
                projectSummary: AIProjectUsageSummary(
                    projectID: project.id,
                    projectName: project.name,
                    currentSessionTokens: 1_000,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: 1_000,
                    projectCachedInputTokens: 0,
                    todayTotalTokens: 1_000,
                    todayCachedInputTokens: 0,
                    currentTool: "codex",
                    currentModel: "gpt-5.4",
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: yesterdayBucketEnd
                ),
                currentSnapshot: nil,
                liveSnapshots: [],
                liveOverlayTokens: 1_000,
                liveOverlayCachedInputTokens: 0,
                liveTodayOverlayTokens: 0,
                liveTodayOverlayCachedInputTokens: 0,
                liveOverlayBaselineDay: today,
                liveOverlayTotalBaselines: [baselineKey: 1_000],
                liveOverlayCachedInputBaselines: [baselineKey: 0],
                sessions: [],
                heatmap: [
                    AIHeatmapDay(
                        day: yesterday,
                        totalTokens: 1_000,
                        cachedInputTokens: 0,
                        requestCount: 1
                    )
                ],
                todayTimeBuckets: [],
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: yesterdayBucketEnd,
                indexingStatus: .completed(detail: "done")
            ),
            for: project.id
        )

        seedLiveSession(
            terminalID: UUID(),
            project: project,
            externalSessionID: "long-live",
            totalTokens: 1_250,
            baselineTotalTokens: 0,
            startedAt: yesterday
        )

        XCTAssertEqual(store.totalTodayNormalizedTokensAcrossProjects([project]), 250)
    }

    func testProjectTodayTokensKeepSummaryBaseWhenCrossDayLiveHasZeroTodayOverlay() {
        let store = makeStore()
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)
        let baselineKey = "codex|long-live"

        store.currentProjects = [project]
        store.cacheState(
            AIStatsPanelState(
                projectSummary: AIProjectUsageSummary(
                    projectID: project.id,
                    projectName: project.name,
                    currentSessionTokens: 700,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: 1_100,
                    projectCachedInputTokens: 0,
                    todayTotalTokens: 400,
                    todayCachedInputTokens: 0,
                    currentTool: "codex",
                    currentModel: "gpt-5.4",
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: today.addingTimeInterval(60)
                ),
                currentSnapshot: nil,
                liveSnapshots: [],
                liveOverlayTokens: 700,
                liveOverlayCachedInputTokens: 0,
                liveTodayOverlayTokens: 0,
                liveTodayOverlayCachedInputTokens: 0,
                liveOverlayBaselineDay: today,
                liveOverlayTotalBaselines: [baselineKey: 700],
                liveOverlayCachedInputBaselines: [baselineKey: 0],
                sessions: [],
                heatmap: [],
                todayTimeBuckets: [],
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: today.addingTimeInterval(30),
                indexingStatus: .completed(detail: "done")
            ),
            for: project.id
        )

        seedLiveSession(
            terminalID: UUID(),
            project: project,
            externalSessionID: "long-live",
            totalTokens: 700,
            baselineTotalTokens: 0,
            startedAt: yesterday
        )

        XCTAssertEqual(store.totalTodayNormalizedTokensAcrossProjects([project]), 400)
    }

    func testProjectAllTimeTokensIgnoreTodayBaselineForCrossDayLiveSession() {
        let store = makeStore()
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)
        let baselineKey = "codex|long-live"

        store.currentProjects = [project]
        store.cacheState(
            AIStatsPanelState(
                projectSummary: AIProjectUsageSummary(
                    projectID: project.id,
                    projectName: project.name,
                    currentSessionTokens: 700,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: 1_100,
                    projectCachedInputTokens: 0,
                    todayTotalTokens: 400,
                    todayCachedInputTokens: 0,
                    currentTool: "codex",
                    currentModel: "gpt-5.4",
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: today.addingTimeInterval(60)
                ),
                currentSnapshot: nil,
                liveSnapshots: [],
                liveOverlayTokens: 700,
                liveOverlayCachedInputTokens: 0,
                liveTodayOverlayTokens: 0,
                liveTodayOverlayCachedInputTokens: 0,
                liveOverlayBaselineDay: today,
                liveOverlayTotalBaselines: [baselineKey: 700],
                liveOverlayCachedInputBaselines: [baselineKey: 0],
                sessions: [],
                heatmap: [],
                todayTimeBuckets: [],
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: today.addingTimeInterval(30),
                indexingStatus: .completed(detail: "done")
            ),
            for: project.id
        )

        seedLiveSession(
            terminalID: UUID(),
            project: project,
            externalSessionID: "long-live",
            totalTokens: 700,
            baselineTotalTokens: 0,
            startedAt: yesterday
        )

        XCTAssertEqual(store.totalAllTimeNormalizedTokensAcrossProjects([project]), 1_100)
    }

    func testTitlebarTodayLevelTokensIgnoresProjectOpenAndRemovalScopeChanges() {
        let aiUsageStore = AIUsageStore(databaseURL: databaseURL)
        let store = makeStore(aiUsageStore: aiUsageStore)
        let projectA = makeProject(name: "Project A", path: "/tmp/project-a")
        let projectB = makeProject(name: "Project B", path: "/tmp/project-b")
        let projectC = makeProject(name: "Project C", path: "/tmp/project-c")
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        let yesterday = calendar.date(byAdding: .day, value: -1, to: today) ?? today.addingTimeInterval(-86_400)

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-today",
                firstSeenAt: calendar.date(byAdding: .hour, value: 9, to: today) ?? today,
                totalTokens: 200
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-today",
                firstSeenAt: calendar.date(byAdding: .hour, value: 10, to: today) ?? today,
                totalTokens: 300
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectC,
                externalSessionID: "c-yesterday",
                firstSeenAt: calendar.date(byAdding: .hour, value: 10, to: yesterday) ?? yesterday,
                totalTokens: 700
            )
        )

        store.currentProjects = [projectA]
        XCTAssertEqual(store.titlebarTodayLevelTokens(), 500)

        store.currentProjects = [projectA, projectB, projectC]
        XCTAssertEqual(store.titlebarTodayLevelTokens(), 500)

        store.currentProjects = [projectA]
        XCTAssertEqual(store.titlebarTodayLevelTokens(), 500)
    }

    func testTitlebarTodayLevelTokensIsPureReadAfterExplicitLiveOverlayRefresh() {
        let store = makeStore()
        let project = makeProject(name: "Project A", path: "/tmp/project-a")
        store.currentProjects = [project]

        seedLiveSession(
            terminalID: UUID(),
            project: project,
            externalSessionID: "titlebar-live",
            totalTokens: 120,
            baselineTotalTokens: 20
        )

        XCTAssertEqual(store.titlebarTodayLevelTokens(), 0)
        XCTAssertTrue(store.refreshTitlebarTodayLiveOverlay())
        XCTAssertEqual(store.titlebarTodayLevelTokens(), 100)

        let renderVersion = store.renderVersion
        XCTAssertEqual(store.titlebarTodayLevelTokens(), 100)
        XCTAssertEqual(store.renderVersion, renderVersion)
    }

    func testProjectTodayTokensUseFreshLiveOverlayAcrossAllCurrentProjects() {
        let store = makeStore()
        let projectA = makeProject(name: "Project A", path: "/tmp/project-a")
        let projectB = makeProject(name: "Project B", path: "/tmp/project-b")
        store.currentProjects = [projectA, projectB]

        store.cacheState(
            AIStatsPanelState(
                projectSummary: AIProjectUsageSummary(
                    projectID: projectA.id,
                    projectName: projectA.name,
                    currentSessionTokens: 0,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: 1_200,
                    projectCachedInputTokens: 0,
                    todayTotalTokens: 500,
                    todayCachedInputTokens: 0,
                    currentTool: nil,
                    currentModel: nil,
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: nil
                ),
                currentSnapshot: nil,
                liveSnapshots: [],
                liveOverlayTokens: 100,
                liveOverlayCachedInputTokens: 0,
                sessions: [],
                heatmap: [],
                todayTimeBuckets: [],
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: nil,
                indexingStatus: .completed(detail: "done")
            ),
            for: projectA.id
        )
        store.cacheState(
            AIStatsPanelState(
                projectSummary: AIProjectUsageSummary(
                    projectID: projectB.id,
                    projectName: projectB.name,
                    currentSessionTokens: 0,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: 2_400,
                    projectCachedInputTokens: 0,
                    todayTotalTokens: 800,
                    todayCachedInputTokens: 0,
                    currentTool: nil,
                    currentModel: nil,
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: nil
                ),
                currentSnapshot: nil,
                liveSnapshots: [],
                liveOverlayTokens: 20,
                liveOverlayCachedInputTokens: 0,
                sessions: [],
                heatmap: [],
                todayTimeBuckets: [],
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: nil,
                indexingStatus: .completed(detail: "done")
            ),
            for: projectB.id
        )

        seedLiveSession(
            terminalID: UUID(),
            project: projectA,
            externalSessionID: "a-live",
            totalTokens: 160,
            baselineTotalTokens: 100
        )
        seedLiveSession(
            terminalID: UUID(),
            project: projectB,
            externalSessionID: "b-live",
            totalTokens: 90,
            baselineTotalTokens: 20
        )

        store.refreshTitlebarTodayLiveOverlay()
        XCTAssertEqual(store.totalTodayNormalizedTokensAcrossProjects([projectA, projectB]), 1_310)
        XCTAssertEqual(store.titlebarTodayLevelTokens(), 130)
        XCTAssertEqual(store.totalAllTimeNormalizedTokensAcrossProjects([projectA, projectB]), 3_610)
    }

    func testPetTotalsIgnorePreClaimHistoryFromNewlyAddedProjectButCountPostClaimGrowth() {
        let aiUsageStore = AIUsageStore(databaseURL: databaseURL)
        let store = makeStore(aiUsageStore: aiUsageStore)
        let claimDate = Date(timeIntervalSince1970: 1_700_000_000)
        let projectA = makeProject(name: "Project A", path: "/tmp/project-a")
        let projectB = makeProject(name: "Project B", path: "/tmp/project-b")

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-before",
                firstSeenAt: claimDate.addingTimeInterval(60),
                totalTokens: 120
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-history",
                firstSeenAt: claimDate.addingTimeInterval(-600),
                totalTokens: 900
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-after",
                firstSeenAt: claimDate.addingTimeInterval(300),
                totalTokens: 80
            )
        )

        seedLiveSession(
            terminalID: UUID(),
            project: projectB,
            externalSessionID: "b-live-before-claim",
            totalTokens: 1_100,
            baselineTotalTokens: 1_000,
            startedAt: claimDate.addingTimeInterval(-60)
        )
        seedLiveSession(
            terminalID: UUID(),
            project: projectB,
            externalSessionID: "b-live-after-claim",
            totalTokens: 250,
            baselineTotalTokens: 200,
            startedAt: claimDate.addingTimeInterval(30)
        )

        XCTAssertEqual(
            store.normalizedTokenTotalsForPet([projectA, projectB], claimedAt: claimDate),
            [
                projectA.id: 120,
                projectB.id: 130,
            ]
        )
        XCTAssertEqual(
            store.totalNormalizedTokensForPet([projectA, projectB], claimedAt: claimDate),
            250
        )

        let stats = store.petStatsSinceClaimedAt(claimDate, projects: [projectA, projectB])
        XCTAssertEqual(
            stats,
            AIStatsStore.computePetStats(
                from: aiUsageStore.indexedSessions(
                    since: claimDate,
                    projectIDs: Set([projectA.id, projectB.id])
                )
            )
        )
    }

    func testPetRefreshCoordinatorIgnoresRemovedProjectUsageThatStillExistsInStatsStore() {
        let aiUsageStore = AIUsageStore(databaseURL: databaseURL)
        let statsStore = makeStore(aiUsageStore: aiUsageStore)
        let petStore = PetStore(storage: .inMemory)
        let coordinator = PetRefreshCoordinator(petStore: petStore)
        let projectA = makeProject(name: "Project A", path: "/tmp/project-a")
        let projectB = makeProject(name: "Project B", path: "/tmp/project-b")

        petStore.claim(option: .voidcat, customName: "")
        guard let claimDate = petStore.claimedAt else {
            return XCTFail("Expected claimed pet")
        }

        let currentProjects = ProjectListBox([projectA, projectB])
        coordinator.configure(
            totalNormalizedTokensByProject: {
                statsStore.normalizedTokenTotalsForPet(currentProjects.value, claimedAt: petStore.claimedAt)
            },
            computedStats: {
                statsStore.petStatsSinceClaimedAt(petStore.claimedAt, projects: currentProjects.value)
            }
        )

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-seed",
                firstSeenAt: claimDate.addingTimeInterval(60),
                totalTokens: 120
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-seed",
                firstSeenAt: claimDate.addingTimeInterval(120),
                totalTokens: 300
            )
        )
        coordinator.refreshNow(reason: .bootstrap, now: claimDate.addingTimeInterval(180))
        XCTAssertEqual(petStore.currentHatchTokens, 0)
        XCTAssertEqual(petStore.projectNormalizedTokenWatermarks[projectA.id], 120)
        XCTAssertEqual(petStore.projectNormalizedTokenWatermarks[projectB.id], 300)

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-growth",
                firstSeenAt: claimDate.addingTimeInterval(240),
                totalTokens: 60
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-growth",
                firstSeenAt: claimDate.addingTimeInterval(300),
                totalTokens: 40
            )
        )
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(360))
        XCTAssertEqual(petStore.currentHatchTokens, 100)

        petStore.forgetProjectBaseline(projectB.id)
        currentProjects.value = [projectA]

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-after-removal",
                firstSeenAt: claimDate.addingTimeInterval(420),
                totalTokens: 600
            )
        )
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(480))

        XCTAssertEqual(petStore.currentHatchTokens, 100)
        XCTAssertEqual(petStore.currentExperienceTokens, 0)
        XCTAssertNil(petStore.projectNormalizedTokenWatermarks[projectB.id])
        XCTAssertEqual(petStore.globalNormalizedTotalWatermark, 180)

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-after-removal",
                firstSeenAt: claimDate.addingTimeInterval(540),
                totalTokens: 80
            )
        )
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(600))
        XCTAssertEqual(petStore.currentHatchTokens, 180)
        XCTAssertEqual(petStore.currentExperienceTokens, 0)
    }

    func testPetRefreshCoordinatorTreatsReaddedProjectAsFreshBaseline() {
        let aiUsageStore = AIUsageStore(databaseURL: databaseURL)
        let statsStore = makeStore(aiUsageStore: aiUsageStore)
        let petStore = PetStore(storage: .inMemory)
        let coordinator = PetRefreshCoordinator(petStore: petStore)
        let projectA = makeProject(name: "Project A", path: "/tmp/project-a")
        let projectB = makeProject(name: "Project B", path: "/tmp/project-b")
        let reopenedProjectB = makeProject(name: "Project B", path: "/tmp/project-b")

        petStore.claim(option: .voidcat, customName: "")
        guard let claimDate = petStore.claimedAt else {
            return XCTFail("Expected claimed pet")
        }

        let currentProjects = ProjectListBox([projectA, projectB])
        coordinator.configure(
            totalNormalizedTokensByProject: {
                statsStore.normalizedTokenTotalsForPet(currentProjects.value, claimedAt: petStore.claimedAt)
            },
            computedStats: {
                statsStore.petStatsSinceClaimedAt(petStore.claimedAt, projects: currentProjects.value)
            }
        )

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-seed",
                firstSeenAt: claimDate.addingTimeInterval(60),
                totalTokens: 120
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-seed",
                firstSeenAt: claimDate.addingTimeInterval(120),
                totalTokens: 300
            )
        )
        coordinator.refreshNow(reason: .bootstrap, now: claimDate.addingTimeInterval(180))

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectA,
                externalSessionID: "a-growth",
                firstSeenAt: claimDate.addingTimeInterval(240),
                totalTokens: 60
            )
        )
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: projectB,
                externalSessionID: "b-growth",
                firstSeenAt: claimDate.addingTimeInterval(300),
                totalTokens: 40
            )
        )
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(360))
        XCTAssertEqual(petStore.currentHatchTokens, 100)

        petStore.forgetProjectBaseline(projectB.id)
        currentProjects.value = [projectA]
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(420))
        XCTAssertEqual(petStore.currentHatchTokens, 100)

        currentProjects.value = [projectA, reopenedProjectB]
        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: reopenedProjectB,
                externalSessionID: "b-readded-history",
                firstSeenAt: claimDate.addingTimeInterval(480),
                totalTokens: 900
            )
        )
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(540))

        XCTAssertEqual(petStore.currentHatchTokens, 100)
        XCTAssertEqual(petStore.currentExperienceTokens, 0)
        XCTAssertEqual(petStore.projectNormalizedTokenWatermarks[reopenedProjectB.id], 900)

        aiUsageStore.saveExternalSummary(
            makeExternalSummary(
                project: reopenedProjectB,
                externalSessionID: "b-readded-growth",
                firstSeenAt: claimDate.addingTimeInterval(600),
                totalTokens: 80
            )
        )
        coordinator.refreshNow(reason: .periodic, now: claimDate.addingTimeInterval(660))

        XCTAssertEqual(petStore.currentHatchTokens, 180)
        XCTAssertEqual(petStore.currentExperienceTokens, 0)
    }

    private func makeStore(aiUsageStore: AIUsageStore? = nil) -> AIStatsStore {
        AIStatsStore(aiUsageStore: aiUsageStore ?? AIUsageStore(databaseURL: databaseURL))
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

    private func seedLiveSession(
        terminalID: UUID,
        project: Project,
        externalSessionID: String,
        totalTokens: Int,
        baselineTotalTokens: Int,
        startedAt: Date = Date()
    ) {
        let sessionStore = AISessionStore.shared
        sessionStore.registerExpectedLogicalSession(
            terminalID: terminalID,
            tool: "codex",
            aiSessionID: externalSessionID
        )
        _ = sessionStore.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: terminalID.uuidString,
                projectID: project.id,
                projectName: project.name,
                sessionTitle: externalSessionID,
                tool: "codex",
                aiSessionID: nil,
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: startedAt.timeIntervalSince1970,
                metadata: nil
            )
        )

        XCTAssertTrue(
            sessionStore.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: externalSessionID,
                    model: "gpt-5.4",
                    inputTokens: baselineTotalTokens,
                    outputTokens: 0,
                    cachedInputTokens: 0,
                    totalTokens: baselineTotalTokens,
                    updatedAt: startedAt.addingTimeInterval(1).timeIntervalSince1970,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false,
                    sessionOrigin: .restored,
                    source: .probe
                )
            )
        )
        if totalTokens > baselineTotalTokens {
            XCTAssertTrue(
                sessionStore.applyRuntimeSnapshot(
                    terminalID: terminalID,
                    snapshot: AIRuntimeContextSnapshot(
                        tool: "codex",
                        externalSessionID: externalSessionID,
                        model: "gpt-5.4",
                        inputTokens: totalTokens,
                        outputTokens: 0,
                        cachedInputTokens: 0,
                        totalTokens: totalTokens,
                        updatedAt: startedAt.addingTimeInterval(2).timeIntervalSince1970,
                        responseState: .responding,
                        wasInterrupted: false,
                        hasCompletedTurn: false,
                        sessionOrigin: .restored,
                        source: .probe
                    )
                )
            )
        }
    }

    private func makeExternalSummary(
        project: Project,
        externalSessionID: String,
        firstSeenAt: Date,
        totalTokens: Int
    ) -> AIExternalFileSummary {
        let lastSeenAt = firstSeenAt.addingTimeInterval(120)
        return AIExternalFileSummary(
            source: "codex",
            filePath: "\(project.path)-\(externalSessionID).jsonl",
            fileModifiedAt: lastSeenAt.timeIntervalSince1970,
            projectPath: project.path,
            usageBuckets: [
                AIUsageBucket(
                    source: "codex",
                    sessionKey: externalSessionID,
                    externalSessionID: externalSessionID,
                    sessionTitle: externalSessionID,
                    model: "gpt-5.4",
                    projectID: project.id,
                    projectName: project.name,
                    bucketStart: firstSeenAt,
                    bucketEnd: lastSeenAt,
                    inputTokens: totalTokens,
                    outputTokens: 0,
                    totalTokens: totalTokens,
                    cachedInputTokens: 0,
                    requestCount: 1,
                    activeDurationSeconds: 120,
                    firstSeenAt: firstSeenAt,
                    lastSeenAt: lastSeenAt
                )
            ],
            sessions: [
                AISessionSummary(
                    sessionID: UUID(),
                    externalSessionID: externalSessionID,
                    projectID: project.id,
                    projectName: project.name,
                    sessionTitle: externalSessionID,
                    firstSeenAt: firstSeenAt,
                    lastSeenAt: lastSeenAt,
                    lastTool: "codex",
                    lastModel: "gpt-5.4",
                    requestCount: 1,
                    totalInputTokens: totalTokens,
                    totalOutputTokens: 0,
                    totalTokens: totalTokens,
                    cachedInputTokens: 0,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: 120,
                    todayTokens: totalTokens,
                    todayCachedInputTokens: 0
                )
            ],
            dayUsage: [],
            timeBuckets: []
        )
    }
}
