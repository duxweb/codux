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
        XCTAssertEqual(panelA.projectSummary?.todayTotalTokens, 111)
        XCTAssertEqual(panelA.sessions.map(\.totalTokens), [111])

        XCTAssertEqual(panelB.projectSummary?.projectID, projectB.id)
        XCTAssertEqual(panelB.projectSummary?.projectTotalTokens, 222)
        XCTAssertEqual(panelB.projectSummary?.todayTotalTokens, 222)
        XCTAssertEqual(panelB.sessions.map(\.totalTokens), [222])
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

    private func makeSessionSummary(project: Project, externalSessionID: String, totalTokens: Int) -> AISessionSummary {
        AISessionSummary(
            sessionID: UUID(),
            externalSessionID: externalSessionID,
            projectID: project.id,
            projectName: project.name,
            sessionTitle: externalSessionID,
            firstSeenAt: Date(timeIntervalSince1970: 1_713_600_000),
            lastSeenAt: Date(timeIntervalSince1970: 1_713_600_100),
            lastTool: "claude",
            lastModel: "claude-sonnet-4-6",
            requestCount: 1,
            totalInputTokens: totalTokens,
            totalOutputTokens: 0,
            totalTokens: totalTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 60,
            todayTokens: totalTokens
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
