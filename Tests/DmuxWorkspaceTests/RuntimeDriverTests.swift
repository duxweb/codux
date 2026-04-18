import XCTest
@testable import DmuxWorkspace

final class RuntimeDriverTests: XCTestCase {
    func testClaudeStopMarksCompletedTurnFromHookSemantics() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let updatedAt = 1_776_500_000.0

        let liveEnvelope = AIToolUsageEnvelope(
            sessionId: sessionID.uuidString,
            sessionInstanceId: "instance-1",
            invocationId: "invoke-1",
            externalSessionID: "claude-session-1",
            projectId: projectID.uuidString,
            projectName: "codux",
            projectPath: "/tmp/codux",
            sessionTitle: "Terminal",
            tool: "claude",
            model: "claude-haiku",
            status: "running",
            responseState: .responding,
            updatedAt: updatedAt,
            startedAt: updatedAt - 10,
            finishedAt: nil,
            inputTokens: 12,
            outputTokens: 34,
            totalTokens: 46,
            contextWindow: nil,
            contextUsedTokens: nil,
            contextUsagePercent: nil,
            source: .socket
        )

        let payload = """
        {
          "session_id": "claude-session-1"
        }
        """

        let payloadData = Data(
            """
            {
              "event": "Stop",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": \(updatedAt),
              "payload": \(quoted(payload))
            }
            """.utf8
        )
        let update = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: payloadData,
            projects: [],
            liveEnvelopes: [liveEnvelope],
            existingRuntime: [:]
        )

        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .idle)
        XCTAssertTrue(snapshot.hasCompletedTurn)
        XCTAssertFalse(snapshot.wasInterrupted)
        XCTAssertEqual(snapshot.externalSessionID, "claude-session-1")
    }

    func testClaudeSessionEndClearsLoadingWithoutMarkingCompletion() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let updatedAt = 1_776_500_100.0

        let liveEnvelope = AIToolUsageEnvelope(
            sessionId: sessionID.uuidString,
            sessionInstanceId: "instance-2",
            invocationId: "invoke-2",
            externalSessionID: "claude-session-2",
            projectId: projectID.uuidString,
            projectName: "codux",
            projectPath: "/tmp/codux",
            sessionTitle: "Terminal",
            tool: "claude",
            model: "claude-haiku",
            status: "running",
            responseState: .responding,
            updatedAt: updatedAt,
            startedAt: updatedAt - 20,
            finishedAt: nil,
            inputTokens: 22,
            outputTokens: 44,
            totalTokens: 66,
            contextWindow: nil,
            contextUsedTokens: nil,
            contextUsagePercent: nil,
            source: .socket
        )

        let payload = """
        {
          "session_id": "claude-session-2"
        }
        """

        let payloadData = Data(
            """
            {
              "event": "SessionEnd",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": \(updatedAt),
              "payload": \(quoted(payload))
            }
            """.utf8
        )
        let update = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: payloadData,
            projects: [],
            liveEnvelopes: [liveEnvelope],
            existingRuntime: [:]
        )

        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .idle)
        XCTAssertFalse(snapshot.hasCompletedTurn)
    }

    func testCodexStopWithCompletedTranscriptBecomesDefinitiveIdle() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let transcriptURL = try makeCodexTranscript(lines: [
            """
            {"timestamp":"2026-04-18T11:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codux"}}
            """,
            """
            {"timestamp":"2026-04-18T11:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1776510001}}
            """,
            """
            {"timestamp":"2026-04-18T11:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":1234}}}}
            """,
            """
            {"timestamp":"2026-04-18T11:00:03Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776510003}}
            """
        ])
        defer { try? FileManager.default.removeItem(at: transcriptURL) }

        let payload = """
        {
          "session_id": "codex-thread-1",
          "transcript_path": "\(transcriptURL.path)"
        }
        """

        let payloadData = Data(
            """
            {
              "event": "Stop",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510004,
              "payload": \(quoted(payload))
            }
            """.utf8
        )
        let update = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: payloadData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )

        XCTAssertEqual(update?.responsePayloads.first?.responseState, .idle)
        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .idle)
        XCTAssertTrue(snapshot.hasCompletedTurn)
        XCTAssertEqual(snapshot.totalTokens, 1234)
    }

    func testCodexStopWithoutDefinitiveCompletionDoesNotReassertResponding() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let transcriptURL = try makeCodexTranscript(lines: [
            """
            {"timestamp":"2026-04-18T11:10:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codux"}}
            """,
            """
            {"timestamp":"2026-04-18T11:10:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1776510601}}
            """,
            """
            {"timestamp":"2026-04-18T11:10:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":5678}}}}
            """
        ])
        defer { try? FileManager.default.removeItem(at: transcriptURL) }

        let payload = """
        {
          "session_id": "codex-thread-2",
          "transcript_path": "\(transcriptURL.path)"
        }
        """

        let payloadData = Data(
            """
            {
              "event": "Stop",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510605,
              "payload": \(quoted(payload))
            }
            """.utf8
        )
        let update = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: payloadData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )

        XCTAssertNotNil(update)
        XCTAssertTrue(update?.responsePayloads.isEmpty ?? false)
        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertNil(snapshot.responseState)
        XCTAssertFalse(snapshot.hasCompletedTurn)
    }

    func testGeminiSessionStartProducesIdleState() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()

        let payloadData = Data(
            """
            {
              "event": "SessionStart",
              "tool": "gemini",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776511000,
              "payload": "{}"
            }
            """.utf8
        )

        let update = await factory.handleRuntimeSocketEvent(
            kind: "gemini-hook",
            payloadData: payloadData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )

        XCTAssertEqual(update?.responsePayloads.first?.responseState, .idle)
        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .idle)
        XCTAssertFalse(snapshot.hasCompletedTurn)
    }

    func testOpenCodeRuntimeSocketPassesThroughRespondingState() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()

        let envelope = AIToolUsageEnvelope(
            sessionId: sessionID.uuidString,
            sessionInstanceId: "instance-opencode",
            invocationId: "invoke-opencode",
            externalSessionID: "opencode-session-1",
            projectId: projectID.uuidString,
            projectName: "codux",
            projectPath: "/tmp/codux",
            sessionTitle: "Terminal",
            tool: "opencode",
            model: "open-small",
            status: "running",
            responseState: .responding,
            updatedAt: 1_776_511_100,
            startedAt: 1_776_511_050,
            finishedAt: nil,
            inputTokens: 11,
            outputTokens: 22,
            totalTokens: 33,
            contextWindow: nil,
            contextUsedTokens: nil,
            contextUsagePercent: nil,
            source: .socket
        )

        let payloadData = try XCTUnwrap(try? JSONEncoder().encode(envelope))
        let update = await factory.handleRuntimeSocketEvent(
            kind: "opencode-runtime",
            payloadData: payloadData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )

        XCTAssertEqual(update?.responsePayloads.first?.responseState, .responding)
        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .responding)
        XCTAssertEqual(snapshot.externalSessionID, "opencode-session-1")
        XCTAssertEqual(snapshot.totalTokens, 33)
    }

    @MainActor
    func testRuntimeStateStorePreservesRespondingUntilDefinitiveCompletion() throws {
        let store = AIRuntimeStateStore.shared
        store.reset()
        defer { store.reset() }

        let sessionID = UUID()
        let projectID = UUID()

        store.applyLiveEnvelope(
            AIToolUsageEnvelope(
                sessionId: sessionID.uuidString,
                sessionInstanceId: "instance-store",
                invocationId: "invoke-store",
                externalSessionID: "codex-thread-store",
                projectId: projectID.uuidString,
                projectName: "codux",
                projectPath: "/tmp/codux",
                sessionTitle: "Terminal",
                tool: "codex",
                model: "gpt-5.4",
                status: "running",
                responseState: .responding,
                updatedAt: 100,
                startedAt: 90,
                finishedAt: nil,
                inputTokens: 10,
                outputTokens: 0,
                totalTokens: 10,
                contextWindow: nil,
                contextUsedTokens: nil,
                contextUsagePercent: nil,
                source: .socket
            )
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: "codex-thread-store",
                model: "gpt-5.4",
                inputTokens: 10,
                outputTokens: 0,
                totalTokens: 10,
                updatedAt: 101,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: "codex-thread-store",
                model: "gpt-5.4",
                inputTokens: 10,
                outputTokens: 5,
                totalTokens: 15,
                updatedAt: 102,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                source: .hook
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    private func makeCodexTranscript(lines: [String]) throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-runtime-tests", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent(UUID().uuidString + ".jsonl")
        try lines.joined(separator: "\n").appending("\n").write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    private func quoted(_ string: String) -> String {
        let data = try! JSONEncoder().encode(string)
        return String(decoding: data, as: UTF8.self)
    }
}
