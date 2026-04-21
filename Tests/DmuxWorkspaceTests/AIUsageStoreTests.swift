import XCTest
import SQLite3
@testable import DmuxWorkspace

final class AIUsageStoreTests: XCTestCase {
    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!

    override func setUpWithError() throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-ai-usage-tests-\(UUID().uuidString)", isDirectory: true)
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

    func testIndexedProjectSnapshotIsDerivedFromNormalizedHistoryTables() {
        let store = makeStore()
        let projectID = UUID()
        let projectPath = "/tmp/normalized-project"
        let indexedAt = Date(timeIntervalSince1970: 1_713_690_123)

        store.deleteExternalSummaries(projectPath: projectPath)
        store.deleteProjectIndexState(projectID: projectID)

        let summary = AIExternalFileSummary(
            source: "claude",
            filePath: "/tmp/claude.jsonl",
            fileModifiedAt: 1_713_690_000,
            projectPath: projectPath,
            sessions: [
                AISessionSummary(
                    sessionID: UUID(),
                    externalSessionID: "claude-1",
                    projectID: projectID,
                    projectName: "Normalized Project",
                    sessionTitle: "Fix bug",
                    firstSeenAt: Date(timeIntervalSince1970: 1_713_600_000),
                    lastSeenAt: Date(timeIntervalSince1970: 1_713_600_100),
                    lastTool: "claude",
                    lastModel: "claude-sonnet-4-6",
                    requestCount: 2,
                    totalInputTokens: 100,
                    totalOutputTokens: 50,
                    totalTokens: 150,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: 60,
                    todayTokens: 150
                )
            ],
            dayUsage: [
                AIHeatmapDay(day: Calendar.autoupdatingCurrent.startOfDay(for: Date()), totalTokens: 150, requestCount: 2)
            ],
            timeBuckets: [
                AITimeBucket(
                    start: Calendar.autoupdatingCurrent.date(bySettingHour: 10, minute: 0, second: 0, of: Date()) ?? Date(),
                    end: Calendar.autoupdatingCurrent.date(bySettingHour: 11, minute: 0, second: 0, of: Date()) ?? Date(),
                    totalTokens: 150,
                    requestCount: 2
                )
            ]
        )
        store.saveExternalSummary(summary)
        store.saveProjectIndexState(for:
            AIIndexedProjectSnapshot(
                projectID: projectID,
                projectName: "Normalized Project",
                projectSummary: AIProjectUsageSummary(
                    projectID: projectID,
                    projectName: "Normalized Project",
                    currentSessionTokens: 0,
                    projectTotalTokens: 150,
                    todayTotalTokens: 150,
                    currentTool: nil,
                    currentModel: nil,
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: nil
                ),
                sessions: summary.sessions,
                heatmap: summary.dayUsage,
                todayTimeBuckets: summary.timeBuckets,
                toolBreakdown: [],
                modelBreakdown: [],
                indexedAt: indexedAt
            ),
            projectPath: projectPath
        )

        let snapshot = store.indexedProjectSnapshot(projectID: projectID)
        XCTAssertEqual(snapshot?.projectID, projectID)
        XCTAssertEqual(snapshot?.projectSummary.projectTotalTokens, 150)
        XCTAssertEqual(snapshot?.projectSummary.todayTotalTokens, 150)
        XCTAssertEqual(snapshot?.sessions.count, 1)
        XCTAssertEqual(snapshot?.toolBreakdown.first?.key, "claude")
        XCTAssertEqual(snapshot?.toolBreakdown.first?.totalTokens, 150)
        XCTAssertEqual(snapshot?.indexedAt.timeIntervalSince1970, indexedAt.timeIntervalSince1970)
    }

    func testStoredExternalSummaryIsScopedByProjectPathForSharedFilePath() {
        let store = makeStore()
        let sharedFilePath = "/tmp/shared-opencode.db"
        let modifiedAt = 1_713_690_000.0
        let projectA = "/tmp/project-a"
        let projectB = "/tmp/project-b"

        store.deleteExternalSummaries(projectPath: projectA)
        store.deleteExternalSummaries(projectPath: projectB)

        let summaryA = AIExternalFileSummary(
            source: "opencode",
            filePath: sharedFilePath,
            fileModifiedAt: modifiedAt,
            projectPath: projectA,
            sessions: [
                makeSessionSummary(projectPath: projectA, title: "A", totalTokens: 111)
            ],
            dayUsage: [
                AIHeatmapDay(day: Date(timeIntervalSince1970: 1_713_600_000), totalTokens: 111, requestCount: 1)
            ],
            timeBuckets: []
        )
        let summaryB = AIExternalFileSummary(
            source: "opencode",
            filePath: sharedFilePath,
            fileModifiedAt: modifiedAt,
            projectPath: projectB,
            sessions: [
                makeSessionSummary(projectPath: projectB, title: "B", totalTokens: 222)
            ],
            dayUsage: [
                AIHeatmapDay(day: Date(timeIntervalSince1970: 1_713_600_000), totalTokens: 222, requestCount: 1)
            ],
            timeBuckets: []
        )

        store.saveExternalSummary(summaryA)
        store.saveExternalSummary(summaryB)

        let storedA = store.storedExternalSummary(
            source: "opencode",
            filePath: sharedFilePath,
            projectPath: projectA,
            modifiedAt: modifiedAt
        )
        let storedB = store.storedExternalSummary(
            source: "opencode",
            filePath: sharedFilePath,
            projectPath: projectB,
            modifiedAt: modifiedAt
        )

        XCTAssertEqual(storedA?.projectPath, projectA)
        XCTAssertEqual(storedA?.sessions.first?.totalTokens, 111)
        XCTAssertEqual(storedB?.projectPath, projectB)
        XCTAssertEqual(storedB?.sessions.first?.totalTokens, 222)
    }

    func testLegacyTablesDoNotBreakInitializationOrNormalizedWrites() throws {
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(databaseURL.path, &db), SQLITE_OK)
        guard let db else {
            return XCTFail("failed to open sqlite database")
        }
        defer { sqlite3_close(db) }

        let legacyStatements = [
            """
            CREATE TABLE IF NOT EXISTS ai_external_file_cache (
                source TEXT NOT NULL,
                file_path TEXT PRIMARY KEY,
                file_modified_at REAL NOT NULL,
                project_path TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_indexed_project_snapshot (
                project_id TEXT PRIMARY KEY,
                indexed_at REAL NOT NULL,
                payload_json TEXT NOT NULL
            );
            """
        ]

        for statement in legacyStatements {
            XCTAssertEqual(sqlite3_exec(db, statement, nil, nil, nil), SQLITE_OK)
        }

        let store = makeStore()
        let projectID = UUID()
        let projectPath = "/tmp/legacy-upgrade"
        let summary = AIExternalFileSummary(
            source: "claude",
            filePath: "/tmp/legacy-claude.jsonl",
            fileModifiedAt: 1_713_690_000,
            projectPath: projectPath,
            sessions: [
                AISessionSummary(
                    sessionID: UUID(),
                    externalSessionID: "legacy",
                    projectID: projectID,
                    projectName: "Legacy Upgrade",
                    sessionTitle: "Legacy Session",
                    firstSeenAt: Date(timeIntervalSince1970: 1_713_600_000),
                    lastSeenAt: Date(timeIntervalSince1970: 1_713_600_060),
                    lastTool: "claude",
                    lastModel: "claude-sonnet-4-6",
                    requestCount: 1,
                    totalInputTokens: 50,
                    totalOutputTokens: 20,
                    totalTokens: 70,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: 60,
                    todayTokens: 70
                )
            ],
            dayUsage: [
                AIHeatmapDay(day: Calendar.autoupdatingCurrent.startOfDay(for: Date()), totalTokens: 70, requestCount: 1)
            ],
            timeBuckets: []
        )

        store.saveExternalSummary(summary)
        let stored = store.storedExternalSummary(
            source: "claude",
            filePath: summary.filePath,
            projectPath: projectPath,
            modifiedAt: summary.fileModifiedAt
        )

        XCTAssertEqual(stored?.projectPath, projectPath)
        XCTAssertEqual(stored?.sessions.first?.totalTokens, 70)
    }

    func testExternalSummaryCheckpointRoundTrips() {
        let store = makeStore()
        let projectPath = "/tmp/checkpoint-project"
        let summary = AIExternalFileSummary(
            source: "claude",
            filePath: "/tmp/checkpoint.jsonl",
            fileModifiedAt: 1_713_690_000,
            projectPath: projectPath,
            sessions: [
                makeSessionSummary(projectPath: projectPath, title: "Checkpoint", totalTokens: 321)
            ],
            dayUsage: [],
            timeBuckets: []
        )
        let checkpoint = AIExternalFileCheckpoint(
            source: "claude",
            filePath: summary.filePath,
            projectPath: projectPath,
            fileModifiedAt: summary.fileModifiedAt,
            fileSize: 4096,
            lastOffset: 3072,
            lastIndexedAt: Date(timeIntervalSince1970: 1_713_690_123),
            payload: AIExternalFileCheckpointPayload(
                sessionKey: "session-1",
                externalSessionID: "session-1",
                sessionTitle: "Checkpoint",
                lastModel: "claude-sonnet-4-6",
                modelTotalTokensByName: ["claude-sonnet-4-6": 321],
                firstSeenAt: Date(timeIntervalSince1970: 1_713_600_000),
                lastSeenAt: Date(timeIntervalSince1970: 1_713_600_060),
                requestCount: 2,
                totalInputTokens: 111,
                totalOutputTokens: 210,
                totalTokens: 321,
                todayTokens: 321,
                activeDurationSeconds: 60,
                waitingForFirstResponse: false,
                pendingTurnStartAt: nil,
                pendingTurnEndAt: nil
            )
        )

        store.saveExternalSummary(summary, checkpoint: checkpoint)
        let storedCheckpoint = store.externalFileCheckpoint(
            source: "claude",
            filePath: summary.filePath,
            projectPath: projectPath
        )
        let storedSummaryWithoutModifiedAt = store.storedExternalSummary(
            source: "claude",
            filePath: summary.filePath,
            projectPath: projectPath
        )

        XCTAssertEqual(storedCheckpoint?.fileSize, 4096)
        XCTAssertEqual(storedCheckpoint?.lastOffset, 3072)
        XCTAssertEqual(storedCheckpoint?.payload?.sessionKey, "session-1")
        XCTAssertEqual(storedCheckpoint?.payload?.modelTotalTokensByName["claude-sonnet-4-6"], 321)
        XCTAssertEqual(storedSummaryWithoutModifiedAt?.sessions.first?.totalTokens, 321)
    }

    private func makeSessionSummary(projectPath: String, title: String, totalTokens: Int) -> AISessionSummary {
        let projectID = UUID()
        return AISessionSummary(
            sessionID: UUID(),
            externalSessionID: title,
            projectID: projectID,
            projectName: projectPath,
            sessionTitle: title,
            firstSeenAt: Date(timeIntervalSince1970: 1_713_600_000),
            lastSeenAt: Date(timeIntervalSince1970: 1_713_600_100),
            lastTool: "opencode",
            lastModel: "gpt-4.1",
            requestCount: 1,
            totalInputTokens: totalTokens,
            totalOutputTokens: 0,
            totalTokens: totalTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 10,
            todayTokens: totalTokens
        )
    }

    private func makeStore() -> AIUsageStore {
        AIUsageStore(databaseURL: databaseURL)
    }
}
