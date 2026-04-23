import XCTest
import SQLite3
@testable import DmuxWorkspace

@MainActor
final class OpenCodeToolDriverTests: XCTestCase {
    func testRuntimeSnapshotExposesTurnBoundaryTimes() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let databaseURL = temporaryDirectoryURL.appendingPathComponent("opencode.db", isDirectory: false)
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(databaseURL.path, &db), SQLITE_OK)
        guard let db else {
            return XCTFail("failed to open opencode fixture db")
        }
        defer { sqlite3_close(db) }

        XCTAssertEqual(sqlite3_exec(db, "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_archived INTEGER, time_updated INTEGER);", nil, nil, nil), SQLITE_OK)
        XCTAssertEqual(sqlite3_exec(db, "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);", nil, nil, nil), SQLITE_OK)

        let projectPath = "/tmp/opencode-project-\(UUID().uuidString)"
        let payloadUser = #"{"role":"user","time":{"created":"2026-04-21T09:00:00Z"},"path":{"root":"\#(projectPath)"},"modelID":"gpt-4.1"}"#
        let payloadAssistant = #"{"role":"assistant","time":{"created":"2026-04-21T09:00:06Z","completed":"2026-04-21T09:00:08Z"},"path":{"root":"\#(projectPath)"},"modelID":"gpt-4.1","tokens":{"input":140,"output":60,"reasoning":15,"cache":{"read":25},"total":240}}"#
        XCTAssertEqual(
            sqlite3_exec(
                db,
                """
                INSERT INTO session (id, title, directory, time_archived, time_updated) VALUES ('session-1', 'OpenCode Title', '\(projectPath)', NULL, 1713690008000);
                INSERT INTO message (id, session_id, time_created, data) VALUES
                ('msg-1', 'session-1', 1713690000000, '\(payloadUser.replacingOccurrences(of: "'", with: "''"))'),
                ('msg-2', 'session-1', 1713690006000, '\(payloadAssistant.replacingOccurrences(of: "'", with: "''"))');
                """,
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )

        let driver = OpenCodeToolDriver(databaseURL: databaseURL)
        let snapshot = await driver.runtimeSnapshot(
            for: AISessionStore.TerminalSessionState(
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "OpenCode",
                tool: "opencode",
                aiSessionID: "session-1",
                state: .responding,
                model: "gpt-4.1",
                baselineTotalTokens: 0,
                committedTotalTokens: 0,
                updatedAt: 0,
                startedAt: 0,
                wasInterrupted: false,
                hasCompletedTurn: false,
                transcriptPath: nil,
                notificationType: nil,
                targetToolName: nil,
                interactionMessage: nil
            )
        )

        XCTAssertEqual(snapshot?.startedAt, parseCodexISO8601Date("2026-04-21T09:00:00Z")?.timeIntervalSince1970)
        XCTAssertEqual(snapshot?.completedAt, parseCodexISO8601Date("2026-04-21T09:00:08Z")?.timeIntervalSince1970)
        XCTAssertEqual(snapshot?.responseState, .idle)
        XCTAssertTrue(snapshot?.hasCompletedTurn ?? false)
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("opencode-driver-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}
