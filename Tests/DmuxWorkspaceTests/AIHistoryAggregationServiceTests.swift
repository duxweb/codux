import XCTest
@testable import DmuxWorkspace

final class AIHistoryAggregationServiceTests: XCTestCase {
    func testBuildExternalFileSummaryAndProjectSummaryMergeFileSummaries() {
        let service = AIHistoryAggregationService()
        let project = Project(
            id: UUID(),
            name: "Workspace",
            path: "/tmp/workspace",
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )

        let now = Date()
        let earlier = now.addingTimeInterval(-86_400)
        let today = Calendar.autoupdatingCurrent.startOfDay(for: now)

        let claudeParse = AIHistoryParseResult(
            entries: [
                AIHistoryUsageEntry(
                    key: AIHistorySessionKey(source: "claude", sessionID: "claude-1"),
                    projectName: project.name,
                    timestamp: earlier,
                    model: "claude-sonnet",
                    inputTokens: 100,
                    outputTokens: 50,
                    cachedInputTokens: 10,
                    reasoningOutputTokens: 0
                )
            ],
            events: [
                AIHistorySessionEvent(
                    key: AIHistorySessionKey(source: "claude", sessionID: "claude-1"),
                    projectName: project.name,
                    timestamp: earlier,
                    role: .user
                ),
                AIHistorySessionEvent(
                    key: AIHistorySessionKey(source: "claude", sessionID: "claude-1"),
                    projectName: project.name,
                    timestamp: earlier.addingTimeInterval(60),
                    role: .assistant
                )
            ],
            metadataByKey: [
                AIHistorySessionKey(source: "claude", sessionID: "claude-1"): AIHistorySessionMetadata(
                    key: AIHistorySessionKey(source: "claude", sessionID: "claude-1"),
                    externalSessionID: "claude-1",
                    sessionTitle: "Claude Session",
                    model: "claude-sonnet"
                )
            ]
        )

        let codexParse = AIHistoryParseResult(
            entries: [
                AIHistoryUsageEntry(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now,
                    model: "gpt-5.4",
                    inputTokens: 40,
                    outputTokens: 20,
                    cachedInputTokens: 0,
                    reasoningOutputTokens: 5
                )
            ],
            events: [
                AIHistorySessionEvent(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now,
                    role: .user
                ),
                AIHistorySessionEvent(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now.addingTimeInterval(45),
                    role: .assistant
                )
            ],
            metadataByKey: [
                AIHistorySessionKey(source: "codex", sessionID: "codex-1"): AIHistorySessionMetadata(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    externalSessionID: "codex-1",
                    sessionTitle: "Codex Session",
                    model: "gpt-5.4"
                )
            ]
        )

        let claudeSummary = service.buildExternalFileSummary(
            source: "claude",
            filePath: "/tmp/claude.jsonl",
            fileModifiedAt: now.timeIntervalSince1970,
            project: project,
            parseResult: claudeParse
        )
        let codexSummary = service.buildExternalFileSummary(
            source: "codex",
            filePath: "/tmp/codex.jsonl",
            fileModifiedAt: now.timeIntervalSince1970,
            project: project,
            parseResult: codexParse
        )

        let merged = service.buildProjectSummary(
            project: project,
            fileSummaries: [claudeSummary, codexSummary]
        )

        XCTAssertEqual(merged.sessions.count, 2)
        XCTAssertEqual(merged.sessions.first?.lastTool, "codex")
        XCTAssertEqual(merged.sessions.first?.totalTokens, 65)
        XCTAssertEqual(merged.toolBreakdown.map(\.key), ["claude", "codex"])
        XCTAssertEqual(merged.toolBreakdown.map(\.totalTokens), [150, 65])
        XCTAssertEqual(merged.modelBreakdown.map(\.key), ["claude-sonnet", "gpt-5.4"])
        XCTAssertEqual(merged.heatmap.count, Set([Calendar.autoupdatingCurrent.startOfDay(for: earlier), today]).count)
        XCTAssertEqual(merged.todayTimeBuckets.reduce(0) { $0 + $1.totalTokens }, 65)
        XCTAssertEqual(merged.sessions.first?.lastTool, "codex")
    }

    func testRequestCountsFollowUserMessagesNotUsageEntries() {
        let service = AIHistoryAggregationService()
        let project = Project(
            id: UUID(),
            name: "Workspace",
            path: "/tmp/workspace",
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )

        let now = Date()
        let parseResult = AIHistoryParseResult(
            entries: [
                AIHistoryUsageEntry(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now.addingTimeInterval(5),
                    model: "gpt-5.4",
                    inputTokens: 10,
                    outputTokens: 4,
                    cachedInputTokens: 6,
                    reasoningOutputTokens: 2
                ),
                AIHistoryUsageEntry(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now.addingTimeInterval(15),
                    model: "gpt-5.4",
                    inputTokens: 8,
                    outputTokens: 3,
                    cachedInputTokens: 2,
                    reasoningOutputTokens: 1
                )
            ],
            events: [
                AIHistorySessionEvent(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now,
                    role: .user
                ),
                AIHistorySessionEvent(
                    key: AIHistorySessionKey(source: "codex", sessionID: "codex-1"),
                    projectName: project.name,
                    timestamp: now.addingTimeInterval(20),
                    role: .assistant
                )
            ],
            metadataByKey: [:]
        )

        let summary = service.buildProjectSummary(project: project, parseResults: [parseResult])
        let session = summary.sessions.first
        XCTAssertEqual(session?.requestCount, 1)
        XCTAssertEqual(session?.totalInputTokens, 18)
        XCTAssertEqual(session?.totalOutputTokens, 7)
        XCTAssertEqual(session?.totalTokens, 28)
        XCTAssertEqual(summary.heatmap.first?.requestCount, 1)
        XCTAssertEqual(summary.heatmap.first?.totalTokens, 28)
        XCTAssertEqual(summary.todayTimeBuckets.reduce(0) { $0 + $1.requestCount }, 1)
        XCTAssertEqual(summary.todayTimeBuckets.reduce(0) { $0 + $1.totalTokens }, 28)
    }
}
