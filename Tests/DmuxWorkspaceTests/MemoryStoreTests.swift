import XCTest
@testable import DmuxWorkspace

final class MemoryStoreTests: XCTestCase {
    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!

    override func setUpWithError() throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-memory-store-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: temporaryDirectoryURL, withIntermediateDirectories: true)
        databaseURL = temporaryDirectoryURL.appendingPathComponent("memory.sqlite3", isDirectory: false)
    }

    override func tearDownWithError() throws {
        if let temporaryDirectoryURL {
            try? FileManager.default.removeItem(at: temporaryDirectoryURL)
        }
        temporaryDirectoryURL = nil
        databaseURL = nil
    }

    func testUpsertDeduplicatesNormalizedContentAndPromotesTier() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()

        let first = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .working,
                kind: .decision,
                content: "Use Swift Testing for new coverage.",
                rationale: "Initial note",
                sourceTool: "codex",
                sourceSessionID: "session-1",
                sourceFingerprint: "fp-1"
            )
        )

        let second = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .core,
                kind: .decision,
                content: "  use   swift testing for new coverage. ",
                rationale: "Promoted note",
                sourceTool: "claude",
                sourceSessionID: "session-2",
                sourceFingerprint: "fp-2"
            )
        )

        XCTAssertEqual(first.id, second.id)
        XCTAssertEqual(second.tier, .core)
        XCTAssertEqual(second.content, "  use   swift testing for new coverage. ")
        XCTAssertEqual(second.rationale, "Promoted note")

        let entries = try store.listEntries(
            scope: .project,
            projectID: projectID,
            tiers: [.core, .working, .archive],
            limit: 10
        )
        XCTAssertEqual(entries.count, 1)
        XCTAssertEqual(entries.first?.id, first.id)
    }

    func testExtractionQueueDeduplicatesByFingerprint() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let fingerprint = "tool|session|path|123"

        XCTAssertTrue(
            try store.enqueueExtractionIfNeeded(
                projectID: UUID(),
                tool: "codex",
                sessionID: "session-1",
                transcriptPath: "/tmp/rollout.jsonl",
                sourceFingerprint: fingerprint
            )
        )

        XCTAssertFalse(
            try store.enqueueExtractionIfNeeded(
                projectID: UUID(),
                tool: "codex",
                sessionID: "session-2",
                transcriptPath: "/tmp/other.jsonl",
                sourceFingerprint: fingerprint
            )
        )

        let task = try store.nextPendingExtractionTask()
        XCTAssertEqual(task?.sourceFingerprint, fingerprint)
        XCTAssertEqual(task?.tool, "codex")
    }

    func testFailedExtractionCanRetryUntilAttemptLimit() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()
        let fingerprint = "retry-fingerprint"

        XCTAssertTrue(
            try store.enqueueExtractionIfNeeded(
                projectID: projectID,
                tool: "codex",
                sessionID: "session-retry",
                transcriptPath: "/tmp/retry.jsonl",
                sourceFingerprint: fingerprint
            )
        )

        let first = try XCTUnwrap(store.nextPendingExtractionTask())
        try store.markExtractionTaskRunning(first.id)
        try store.markExtractionTaskFailed(first.id, error: "timeout")

        let retry = try XCTUnwrap(store.retryableFailedExtractionTask())
        XCTAssertEqual(retry.id, first.id)
        XCTAssertEqual(retry.attempts, 1)

        try store.resetExtractionTaskForRetry(first.id)
        let second = try XCTUnwrap(store.nextPendingExtractionTask())
        try store.markExtractionTaskRunning(second.id)
        try store.markExtractionTaskFailed(second.id, error: "timeout")

        try store.resetExtractionTaskForRetry(first.id)
        let third = try XCTUnwrap(store.nextPendingExtractionTask())
        try store.markExtractionTaskRunning(third.id)
        try store.markExtractionTaskFailed(third.id, error: "timeout")

        XCTAssertNil(try store.retryableFailedExtractionTask())
    }

    func testSummaryUpsertVersionsAndMergedEntriesAreHiddenFromActiveLists() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()
        let entry = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .working,
                kind: .decision,
                content: "Launch memory from generated prompt files.",
                rationale: nil,
                sourceTool: "codex",
                sourceSessionID: "session-1",
                sourceFingerprint: "fp-1"
            )
        )

        let first = try store.upsertSummary(
            scope: .project,
            projectID: projectID,
            content: "Project summary v1",
            sourceEntryIDs: [entry.id],
            maxVersions: 3
        )
        let second = try store.upsertSummary(
            scope: .project,
            projectID: projectID,
            content: "Project summary v2",
            sourceEntryIDs: [entry.id],
            maxVersions: 3
        )
        try store.markEntriesMerged([entry.id], summaryID: second.id)

        let summary = try XCTUnwrap(store.currentSummary(scope: .project, projectID: projectID))
        XCTAssertEqual(first.id, second.id)
        XCTAssertEqual(summary.version, 2)
        XCTAssertEqual(summary.content, "Project summary v2")
        XCTAssertEqual(summary.sourceEntryIDs, [entry.id])

        let activeEntries = try store.listEntries(
            scope: .project,
            projectID: projectID,
            tiers: [.working],
            limit: 10
        )
        XCTAssertTrue(activeEntries.isEmpty)
    }

    func testManagementQueriesListProjectsAndDeleteEntries() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()
        let userEntry = try store.upsert(
            MemoryCandidate(
                scope: .user,
                projectID: nil,
                toolID: nil,
                tier: .core,
                kind: .preference,
                content: "Prefer concise status updates.",
                rationale: nil,
                sourceTool: "codex",
                sourceSessionID: "user-session",
                sourceFingerprint: "user-fp"
            )
        )
        let projectEntry = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .working,
                kind: .decision,
                content: "Keep memory extraction API-only.",
                rationale: "Avoid extra CLI sessions.",
                sourceTool: "codex",
                sourceSessionID: "project-session",
                sourceFingerprint: "project-fp"
            )
        )
        let summary = try store.upsertSummary(
            scope: .project,
            projectID: projectID,
            content: "Project summary",
            sourceEntryIDs: [projectEntry.id],
            maxVersions: 3
        )

        let userOverview = try store.memoryScopeOverview(scope: .user)
        XCTAssertEqual(userOverview.activeEntryCount, 1)

        let projectOverviews = try store.projectOverviewsForManagement()
        XCTAssertEqual(projectOverviews.map(\.projectID), [projectID])
        XCTAssertEqual(projectOverviews.first?.activeEntryCount, 1)
        XCTAssertEqual(projectOverviews.first?.summaryCount, 1)

        let working = try store.listEntriesForManagement(
            scope: .project,
            projectID: projectID,
            tiers: [.working],
            statuses: [.active]
        )
        XCTAssertEqual(working.map(\.id), [projectEntry.id])

        try store.deleteEntry(userEntry.id)
        XCTAssertEqual(try store.memoryScopeOverview(scope: .user).activeEntryCount, 0)

        try store.deleteSummary(summary.id)
        XCTAssertTrue(try store.listSummariesForManagement(scope: .project, projectID: projectID).isEmpty)
    }
}
