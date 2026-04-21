import XCTest
import SQLite3
@testable import DmuxWorkspace

final class AIProjectHistoryServiceTests: XCTestCase {
    func testClaudeHistoryUsesStoredExternalSummaryWhenFileStateIsUnchanged() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let project = makeProject(path: "/tmp/dmux-history-\(UUID().uuidString)")
        let store = AIUsageStore(databaseURL: temporaryDirectoryURL.appendingPathComponent("ai-usage.sqlite3"))
        let service = AIProjectHistoryService(usageStore: store)
        let sessionID = UUID().uuidString.lowercased()
        let logURL = AIRuntimeSourceLocator.claudeSessionLogURL(projectPath: project.path, externalSessionID: sessionID)
        try FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: logURL.deletingLastPathComponent()) }

        let fixedDate = Date(timeIntervalSince1970: 1_700_000_000)
        let rows = [
            """
            {"cwd":"\(project.path)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:40.000Z","type":"user","message":{"role":"user","content":"Fix cache bug"}}
            """,
            """
            {"cwd":"\(project.path)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:47.562Z","type":"assistant","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":30,"output_tokens":40},"stop_reason":"end_turn"}}
            """
        ]
        try "\(rows.joined(separator: "\n"))\n".write(to: logURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.modificationDate: fixedDate], ofItemAtPath: logURL.path)

        let firstSummary = try await service.loadProjectSummary(project: project) { _ in }
        let firstSession = try XCTUnwrap(firstSummary.sessions.first(where: { $0.externalSessionID == sessionID }))
        XCTAssertEqual(firstSession.totalTokens, 50)
        XCTAssertEqual(firstSession.requestCount, 1)

        let secondSummary = try await service.loadProjectSummary(project: project) { _ in }
        let secondSession = try XCTUnwrap(secondSummary.sessions.first(where: { $0.externalSessionID == sessionID }))
        XCTAssertEqual(secondSession.totalTokens, 50)
        XCTAssertEqual(secondSession.requestCount, 1)
    }

    func testClaudeHistoryAppendsOnlyNewTailData() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let project = makeProject(path: "/tmp/dmux-history-\(UUID().uuidString)")
        let store = AIUsageStore(databaseURL: temporaryDirectoryURL.appendingPathComponent("ai-usage.sqlite3"))
        let service = AIProjectHistoryService(usageStore: store)
        let sessionID = UUID().uuidString.lowercased()
        let logURL = AIRuntimeSourceLocator.claudeSessionLogURL(projectPath: project.path, externalSessionID: sessionID)
        try FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: logURL.deletingLastPathComponent()) }

        let initialRows = [
            """
            {"cwd":"\(project.path)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:40.000Z","type":"user","message":{"role":"user","content":"Turn 1"}}
            """,
            """
            {"cwd":"\(project.path)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:47.000Z","type":"assistant","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_read_input_tokens":0,"output_tokens":40},"stop_reason":"end_turn"}}
            """
        ]
        try "\(initialRows.joined(separator: "\n"))\n".write(to: logURL, atomically: true, encoding: .utf8)

        let firstSummary = try await service.loadProjectSummary(project: project) { _ in }
        let firstSession = try XCTUnwrap(firstSummary.sessions.first(where: { $0.externalSessionID == sessionID }))
        let appendedRows = [
            """
            {"cwd":"\(project.path)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:12:00.000Z","type":"user","message":{"role":"user","content":"Turn 2"}}
            """,
            """
            {"cwd":"\(project.path)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:12:09.000Z","type":"assistant","uuid":"row-2","message":{"id":"msg-2","model":"claude-sonnet-4-6","usage":{"input_tokens":15,"cache_read_input_tokens":0,"output_tokens":55},"stop_reason":"end_turn"}}
            """
        ]
        let handle = try FileHandle(forWritingTo: logURL)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("\(appendedRows.joined(separator: "\n"))\n".utf8))
        try handle.close()

        let secondSummary = try await service.loadProjectSummary(project: project) { _ in }
        let secondSession = try XCTUnwrap(secondSummary.sessions.first(where: { $0.externalSessionID == sessionID }))

        XCTAssertEqual(firstSession.totalTokens, 50)
        XCTAssertEqual(secondSession.totalTokens, 120)
        XCTAssertEqual(secondSession.requestCount, 2)

    }

    func testGeminiHistoryReadsProjectSessionsWithinScopedHome() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let project = makeProject(path: "/tmp/gemini-project-\(UUID().uuidString)")
        let store = AIUsageStore(databaseURL: temporaryDirectoryURL.appendingPathComponent("gemini-ai-usage.sqlite3"))
        let service = AIProjectHistoryService(usageStore: store, runtimeHomeURL: temporaryDirectoryURL)

        let projectsURL = AIRuntimeSourceLocator.geminiProjectsURL(homeURL: temporaryDirectoryURL)
        try FileManager.default.createDirectory(at: projectsURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try """
        {"projects":{"\(project.path)":"fixture-project"}}
        """.write(to: projectsURL, atomically: true, encoding: .utf8)

        let chatsDirectoryURL = temporaryDirectoryURL
            .appendingPathComponent(".gemini/tmp/fixture-project/chats", isDirectory: true)
        try FileManager.default.createDirectory(at: chatsDirectoryURL, withIntermediateDirectories: true)
        let sessionURL = chatsDirectoryURL.appendingPathComponent("session-1.json", isDirectory: false)
        try """
        {
          "sessionId":"gemini-session-1",
          "model":"gemini-2.5-pro",
          "messages":[
            {
              "role":"user",
              "timestamp":"2026-04-21T08:00:00Z",
              "content":"Investigate issue"
            },
            {
              "role":"assistant",
              "timestamp":"2026-04-21T08:00:05Z",
              "model":"gemini-2.5-pro",
              "usage":{"promptTokenCount":120,"candidatesTokenCount":80,"cachedContentTokenCount":20,"thoughtsTokenCount":10}
            }
          ]
        }
        """.write(to: sessionURL, atomically: true, encoding: .utf8)

        let firstSummary = try await service.loadProjectSummary(project: project) { _ in }
        let firstSession = try XCTUnwrap(firstSummary.sessions.first(where: { $0.externalSessionID == "gemini-session-1" }))
        XCTAssertEqual(firstSession.lastTool, "gemini")
        XCTAssertEqual(firstSession.lastModel, "gemini-2.5-pro")
        XCTAssertEqual(firstSession.totalInputTokens, 100)
        XCTAssertEqual(firstSession.totalOutputTokens, 70)
        XCTAssertEqual(firstSession.totalTokens, 180)
        XCTAssertEqual(firstSession.requestCount, 1)

        let secondSummary = try await service.loadProjectSummary(project: project) { _ in }
        let secondSession = try XCTUnwrap(secondSummary.sessions.first(where: { $0.externalSessionID == "gemini-session-1" }))
        XCTAssertEqual(secondSession.totalTokens, 180)
        XCTAssertEqual(secondSession.requestCount, 1)
    }

    func testCodexHistoryFallsBackToSessionsDirectoryScan() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let project = makeProject(path: "/tmp/codex-project-\(UUID().uuidString)")
        let store = AIUsageStore(databaseURL: temporaryDirectoryURL.appendingPathComponent("codex-ai-usage.sqlite3"))
        let service = AIProjectHistoryService(usageStore: store, runtimeHomeURL: temporaryDirectoryURL)

        let sessionURL = temporaryDirectoryURL
            .appendingPathComponent(".codex/sessions/2026/04/21/rollout-a.jsonl", isDirectory: false)
        try FileManager.default.createDirectory(at: sessionURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try """
        {"timestamp":"2026-04-21T07:00:00Z","type":"session_meta","payload":{"cwd":"\(project.path)","id":"thread-1","title":"Codex Session"}}
        {"timestamp":"2026-04-21T07:00:01Z","type":"turn_context","payload":{"cwd":"\(project.path)","model":"gpt-5-codex"}}
        {"timestamp":"2026-04-21T07:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5-codex","last_token_usage":{"input_tokens":120,"output_tokens":80,"cached_input_tokens":20,"reasoning_output_tokens":10,"total_tokens":210}}}}
        """.appending("\n").write(to: sessionURL, atomically: true, encoding: .utf8)

        let summary = try await service.loadProjectSummary(project: project) { _ in }
        let session = try XCTUnwrap(summary.sessions.first(where: { $0.externalSessionID == "thread-1" }))
        XCTAssertEqual(session.lastTool, "codex")
        XCTAssertEqual(session.lastModel, "gpt-5-codex")
        XCTAssertEqual(session.totalInputTokens, 100)
        XCTAssertEqual(session.totalOutputTokens, 70)
        XCTAssertEqual(session.totalTokens, 180)
        XCTAssertEqual(session.requestCount, 2)
    }

    func testOpenCodeHistoryReadsProjectDatabaseWithinScopedHome() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let project = makeProject(path: "/tmp/opencode-project-\(UUID().uuidString)")
        let store = AIUsageStore(databaseURL: temporaryDirectoryURL.appendingPathComponent("opencode-ai-usage.sqlite3"))
        let service = AIProjectHistoryService(usageStore: store, runtimeHomeURL: temporaryDirectoryURL)

        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL(homeURL: temporaryDirectoryURL)
        try FileManager.default.createDirectory(at: databaseURL.deletingLastPathComponent(), withIntermediateDirectories: true)

        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(databaseURL.path, &db), SQLITE_OK)
        guard let db else {
            return XCTFail("failed to open opencode fixture db")
        }
        defer { sqlite3_close(db) }

        XCTAssertEqual(sqlite3_exec(db, "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_archived INTEGER);", nil, nil, nil), SQLITE_OK)
        XCTAssertEqual(sqlite3_exec(db, "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);", nil, nil, nil), SQLITE_OK)

        let payloadUser = #"{"role":"user","time":{"created":"2026-04-21T09:00:00Z"},"path":{"root":"\#(project.path)"},"modelID":"gpt-4.1"}"#
        let payloadAssistant = #"{"role":"assistant","time":{"created":"2026-04-21T09:00:06Z"},"path":{"root":"\#(project.path)"},"modelID":"gpt-4.1","tokens":{"input":140,"output":60,"reasoning":15,"cache":{"read":25},"total":240}}"#
        XCTAssertEqual(
            sqlite3_exec(
                db,
                """
                INSERT INTO session (id, title, directory, time_archived) VALUES ('session-1', 'OpenCode Title', '\(project.path)', NULL);
                INSERT INTO message (id, session_id, time_created, data) VALUES
                ('msg-1', 'session-1', 1, '\(payloadUser.replacingOccurrences(of: "'", with: "''"))'),
                ('msg-2', 'session-1', 2, '\(payloadAssistant.replacingOccurrences(of: "'", with: "''"))');
                """,
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )

        let firstSummary = try await service.loadProjectSummary(project: project) { _ in }
        let firstSession = try XCTUnwrap(firstSummary.sessions.first(where: { $0.externalSessionID == "session-1" }))
        XCTAssertEqual(firstSession.lastTool, "opencode")
        XCTAssertEqual(firstSession.lastModel, "gpt-4.1")
        XCTAssertEqual(firstSession.totalInputTokens, 140)
        XCTAssertEqual(firstSession.totalOutputTokens, 60)
        XCTAssertEqual(firstSession.totalTokens, 215)
        XCTAssertEqual(firstSession.requestCount, 1)

        let secondSummary = try await service.loadProjectSummary(project: project) { _ in }
        let secondSession = try XCTUnwrap(secondSummary.sessions.first(where: { $0.externalSessionID == "session-1" }))
        XCTAssertEqual(secondSession.totalTokens, 215)
        XCTAssertEqual(secondSession.requestCount, 1)
    }

    func testOpenCodeHistoryFallsBackToLegacyMessageFiles() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let project = makeProject(path: "/tmp/opencode-legacy-\(UUID().uuidString)")
        let store = AIUsageStore(databaseURL: temporaryDirectoryURL.appendingPathComponent("opencode-legacy-ai-usage.sqlite3"))
        let service = AIProjectHistoryService(usageStore: store, runtimeHomeURL: temporaryDirectoryURL)

        let messageDirectoryURL = temporaryDirectoryURL
            .appendingPathComponent(".local/share/opencode/storage/message/ses_legacy", isDirectory: true)
        try FileManager.default.createDirectory(at: messageDirectoryURL, withIntermediateDirectories: true)

        let userPayload = """
        {
          "role":"user",
          "time":{"created":"2026-04-21T09:10:00Z"},
          "path":{"root":"\(project.path)"},
          "modelID":"gpt-4.1"
        }
        """
        let assistantPayload = """
        {
          "role":"assistant",
          "time":{"created":"2026-04-21T09:10:04Z"},
          "path":{"root":"\(project.path)"},
          "modelID":"gpt-4.1",
          "tokens":{"input":120,"output":50,"reasoning":10,"cache":{"read":20},"total":200}
        }
        """
        try userPayload.write(to: messageDirectoryURL.appendingPathComponent("msg-1.json"), atomically: true, encoding: .utf8)
        try assistantPayload.write(to: messageDirectoryURL.appendingPathComponent("msg-2.json"), atomically: true, encoding: .utf8)

        let summary = try await service.loadProjectSummary(project: project) { _ in }
        let session = try XCTUnwrap(summary.sessions.first(where: { $0.externalSessionID == "ses_legacy" }))
        XCTAssertEqual(session.lastTool, "opencode")
        XCTAssertEqual(session.lastModel, "gpt-4.1")
        XCTAssertEqual(session.totalInputTokens, 120)
        XCTAssertEqual(session.totalOutputTokens, 50)
        XCTAssertEqual(session.totalTokens, 180)
        XCTAssertEqual(session.requestCount, 1)
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-history-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func makeProject(path: String) -> Project {
        Project(
            id: UUID(),
            name: "Cache Project",
            path: path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
    }
}
