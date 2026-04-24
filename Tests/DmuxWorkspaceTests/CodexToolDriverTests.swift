import XCTest
import SQLite3
@testable import DmuxWorkspace

final class CodexToolDriverTests: XCTestCase {
    func testRuntimeSnapshotUsesRolloutCumulativeTotalsForRestoredRunningSession() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let databaseURL = temporaryDirectoryURL.appendingPathComponent("state_5.sqlite", isDirectory: false)
        let rolloutURL = temporaryDirectoryURL.appendingPathComponent("rollout.jsonl", isDirectory: false)
        let projectPath = "/tmp/codex-runtime-project"
        let sessionID = "thread-1"

        try writeCodexRollout(
            rows: [
                #"{"timestamp":"2026-04-23T08:32:40.000Z","type":"turn_context","payload":{"cwd":"/tmp/codex-runtime-project","model":"gpt-5.4"}}"#,
                #"{"timestamp":"2026-04-23T08:32:41.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10000,"cached_input_tokens":8000,"output_tokens":1000,"reasoning_output_tokens":200,"total_tokens":11000}}}}"#,
                #"{"timestamp":"2026-04-23T08:32:47.700Z","type":"event_msg","payload":{"type":"task_started","started_at":1776933167}}"#,
                #"{"timestamp":"2026-04-23T08:32:56.958Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10000,"cached_input_tokens":8000,"output_tokens":1000,"reasoning_output_tokens":200,"total_tokens":11000},"last_token_usage":{"input_tokens":500,"cached_input_tokens":200,"output_tokens":100,"reasoning_output_tokens":20,"total_tokens":600}}}}"#
            ],
            to: rolloutURL
        )
        try seedCodexThreadDatabase(
            databaseURL: databaseURL,
            sessionID: sessionID,
            projectPath: projectPath,
            rolloutURL: rolloutURL,
            model: "gpt-5.4"
        )

        let driver = CodexToolDriver(databaseURL: databaseURL)
        let session = AISessionStore.TerminalSessionState(
            terminalID: UUID(),
            projectID: UUID(),
            projectName: "Codux",
            projectPath: projectPath,
            sessionTitle: "Codex",
            tool: "codex",
            aiSessionID: sessionID,
            state: .responding,
            model: "gpt-5.4",
            updatedAt: 1_776_933_167,
            wasInterrupted: false,
            hasCompletedTurn: false
        )

        let runtimeSnapshot = await driver.runtimeSnapshot(for: session)
        let snapshot = try XCTUnwrap(runtimeSnapshot)
        XCTAssertEqual(snapshot.totalTokens, 3_400)
        XCTAssertEqual(snapshot.cachedInputTokens, 8_200)
        XCTAssertEqual(snapshot.responseState, .responding)
        XCTAssertEqual(snapshot.model, "gpt-5.4")
        XCTAssertEqual(snapshot.externalSessionID, sessionID)
        XCTAssertEqual(snapshot.sessionOrigin, .restored)
    }

    func testResolveHookEventUsesRolloutTotalsOnTurnCompleted() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let databaseURL = temporaryDirectoryURL.appendingPathComponent("state_5.sqlite", isDirectory: false)
        let rolloutURL = temporaryDirectoryURL.appendingPathComponent("rollout.jsonl", isDirectory: false)
        let projectPath = "/tmp/codex-runtime-project"
        let sessionID = "thread-2"

        try writeCodexRollout(
            rows: [
                #"{"timestamp":"2026-04-23T08:32:47.614Z","type":"turn_context","payload":{"cwd":"/tmp/codex-runtime-project","model":"gpt-5.4"}}"#,
                #"{"timestamp":"2026-04-23T08:32:47.700Z","type":"event_msg","payload":{"type":"task_started","started_at":1776933167}}"#,
                #"{"timestamp":"2026-04-23T08:33:11.975Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":12000,"cached_input_tokens":8200,"output_tokens":1200,"reasoning_output_tokens":220,"total_tokens":13200}}}}"#,
                #"{"timestamp":"2026-04-23T08:33:12.154Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776933192}}"#
            ],
            to: rolloutURL
        )
        try seedCodexThreadDatabase(
            databaseURL: databaseURL,
            sessionID: sessionID,
            projectPath: projectPath,
            rolloutURL: rolloutURL,
            model: "gpt-5.4"
        )

        let driver = CodexToolDriver(databaseURL: databaseURL)
        let event = AIHookEvent(
            kind: .turnCompleted,
            terminalID: UUID(),
            terminalInstanceID: "instance-1",
            projectID: UUID(),
            projectName: "Codux",
            projectPath: projectPath,
            sessionTitle: "Codex",
            tool: "codex",
            aiSessionID: sessionID,
            model: "gpt-5.4",
            totalTokens: nil,
            updatedAt: 1_776_933_192,
            metadata: AIHookEventMetadata(transcriptPath: rolloutURL.path)
        )

        let resolved = await driver.resolveHookEvent(event, currentSession: nil)
        XCTAssertEqual(resolved.totalTokens, 5_000)
        XCTAssertEqual(resolved.cachedInputTokens, 8_200)
        XCTAssertEqual(resolved.model, "gpt-5.4")
        XCTAssertEqual(resolved.metadata?.hasCompletedTurn, true)
    }

    func testRuntimeSnapshotDoesNotDoubleCountWhenNewTurnStartsWithRepeatedPreviousTotals() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let databaseURL = temporaryDirectoryURL.appendingPathComponent("state_5.sqlite", isDirectory: false)
        let rolloutURL = temporaryDirectoryURL.appendingPathComponent("rollout.jsonl", isDirectory: false)
        let projectPath = "/tmp/codex-runtime-project"
        let sessionID = "thread-3"

        try writeCodexRollout(
            rows: [
                #"{"timestamp":"2026-04-23T09:09:55.702Z","type":"session_meta","payload":{"id":"thread-3","cwd":"/tmp/codex-runtime-project"}}"#,
                #"{"timestamp":"2026-04-23T09:09:55.703Z","type":"turn_context","payload":{"cwd":"/tmp/codex-runtime-project","model":"gpt-5.4"}}"#,
                #"{"timestamp":"2026-04-23T09:09:55.703Z","type":"event_msg","payload":{"type":"task_started","started_at":1776935395}}"#,
                #"{"timestamp":"2026-04-23T09:09:59.434Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":14391,"cached_input_tokens":3456,"output_tokens":12,"reasoning_output_tokens":0,"total_tokens":14403},"last_token_usage":{"input_tokens":14391,"cached_input_tokens":3456,"output_tokens":12,"reasoning_output_tokens":0,"total_tokens":14403}}}}"#,
                #"{"timestamp":"2026-04-23T09:09:59.624Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776935399}}"#,
                #"{"timestamp":"2026-04-23T09:10:04.268Z","type":"event_msg","payload":{"type":"task_started","started_at":1776935404}}"#,
                #"{"timestamp":"2026-04-23T09:10:06.375Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":14391,"cached_input_tokens":3456,"output_tokens":12,"reasoning_output_tokens":0,"total_tokens":14403},"last_token_usage":{"input_tokens":14391,"cached_input_tokens":3456,"output_tokens":12,"reasoning_output_tokens":0,"total_tokens":14403}}}}"#
            ],
            to: rolloutURL
        )
        try seedCodexThreadDatabase(
            databaseURL: databaseURL,
            sessionID: sessionID,
            projectPath: projectPath,
            rolloutURL: rolloutURL,
            model: "gpt-5.4"
        )

        let driver = CodexToolDriver(databaseURL: databaseURL)
        let session = AISessionStore.TerminalSessionState(
            terminalID: UUID(),
            projectID: UUID(),
            projectName: "Codux",
            projectPath: projectPath,
            sessionTitle: "Codex",
            tool: "codex",
            aiSessionID: sessionID,
            state: .responding,
            model: "gpt-5.4",
            updatedAt: 1_776_935_404,
            wasInterrupted: false,
            hasCompletedTurn: false
        )

        let runtimeSnapshot = await driver.runtimeSnapshot(for: session)
        let snapshot = try XCTUnwrap(runtimeSnapshot)
        XCTAssertEqual(snapshot.totalTokens, 10_947)
        XCTAssertEqual(snapshot.cachedInputTokens, 3_456)
        XCTAssertEqual(snapshot.responseState, .responding)
    }

    func testResolveHookEventDowngradesStaleCompletedTurnAfterNewPrompt() async throws {
        let temporaryDirectoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporaryDirectoryURL) }

        let databaseURL = temporaryDirectoryURL.appendingPathComponent("state_5.sqlite", isDirectory: false)
        let rolloutURL = temporaryDirectoryURL.appendingPathComponent("rollout.jsonl", isDirectory: false)
        let projectPath = "/tmp/codex-runtime-project"
        let sessionID = "thread-stale-stop"

        try writeCodexRollout(
            rows: [
                #"{"timestamp":"2026-04-23T09:09:55.702Z","type":"session_meta","payload":{"id":"thread-stale-stop","cwd":"/tmp/codex-runtime-project"}}"#,
                #"{"timestamp":"2026-04-23T09:09:55.703Z","type":"turn_context","payload":{"cwd":"/tmp/codex-runtime-project","model":"gpt-5.4"}}"#,
                #"{"timestamp":"2026-04-23T09:09:55.703Z","type":"event_msg","payload":{"type":"task_started","started_at":1776935395}}"#,
                #"{"timestamp":"2026-04-23T09:09:59.434Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":14391,"cached_input_tokens":3456,"output_tokens":12,"reasoning_output_tokens":0,"total_tokens":14403}}}}"#,
                #"{"timestamp":"2026-04-23T09:09:59.624Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776935399}}"#
            ],
            to: rolloutURL
        )
        try seedCodexThreadDatabase(
            databaseURL: databaseURL,
            sessionID: sessionID,
            projectPath: projectPath,
            rolloutURL: rolloutURL,
            model: "gpt-5.4"
        )

        let driver = CodexToolDriver(databaseURL: databaseURL)
        let terminalID = UUID()
        let projectID = UUID()
        let currentSession = AISessionStore.TerminalSessionState(
            terminalID: terminalID,
            projectID: projectID,
            projectName: "Codux",
            projectPath: projectPath,
            sessionTitle: "Codex",
            tool: "codex",
            aiSessionID: sessionID,
            state: .responding,
            model: "gpt-5.4",
            updatedAt: 1_776_935_404,
            activeTurnStartedAt: 1_776_935_404,
            wasInterrupted: false,
            hasCompletedTurn: false
        )
        let event = AIHookEvent(
            kind: .turnCompleted,
            terminalID: terminalID,
            terminalInstanceID: "instance-1",
            projectID: projectID,
            projectName: "Codux",
            projectPath: projectPath,
            sessionTitle: "Codex",
            tool: "codex",
            aiSessionID: sessionID,
            model: "gpt-5.4",
            totalTokens: nil,
            updatedAt: 1_776_935_410,
            metadata: AIHookEventMetadata(transcriptPath: rolloutURL.path)
        )

        let resolved = await driver.resolveHookEvent(event, currentSession: currentSession)
        XCTAssertEqual(resolved.kind, .promptSubmitted)
        XCTAssertEqual(resolved.metadata?.hasCompletedTurn, false)
        XCTAssertEqual(resolved.metadata?.wasInterrupted, false)
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-codex-driver-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func writeCodexRollout(rows: [String], to url: URL) throws {
        try "\(rows.joined(separator: "\n"))\n".write(to: url, atomically: true, encoding: .utf8)
    }

    private func seedCodexThreadDatabase(
        databaseURL: URL,
        sessionID: String,
        projectPath: String,
        rolloutURL: URL,
        model: String
    ) throws {
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(databaseURL.path, &db), SQLITE_OK)
        guard let db else {
            return XCTFail("failed to open sqlite database")
        }
        defer { sqlite3_close(db) }

        XCTAssertEqual(
            sqlite3_exec(
                db,
                """
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    rollout_path TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    updated_at INTEGER NOT NULL DEFAULT 0,
                    model TEXT
                );
                """,
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )

        let escapedRolloutPath = rolloutURL.path.replacingOccurrences(of: "'", with: "''")
        let escapedProjectPath = projectPath.replacingOccurrences(of: "'", with: "''")
        let escapedModel = model.replacingOccurrences(of: "'", with: "''")
        XCTAssertEqual(
            sqlite3_exec(
                db,
                """
                INSERT INTO threads (id, rollout_path, cwd, updated_at, model)
                VALUES (
                    '\(sessionID)',
                    '\(escapedRolloutPath)',
                    '\(escapedProjectPath)',
                    1776933192,
                    '\(escapedModel)'
                );
                """,
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )
    }
}
