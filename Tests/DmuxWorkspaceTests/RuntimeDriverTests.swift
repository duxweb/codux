import XCTest
@testable import DmuxWorkspace

final class RuntimeDriverTests: XCTestCase {
    override func setUp() async throws {
        await AIToolRuntimeResponseLatch.shared.resetAll()
    }

    override func tearDown() async throws {
        await AIToolRuntimeResponseLatch.shared.resetAll()
    }

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

    func testClaudeUserPromptSubmitMarksResponding() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let updatedAt = 1_776_500_050.0

        let payload = """
        {
          "session_id": "claude-session-submit"
        }
        """

        let payloadData = Data(
            """
            {
              "event": "UserPromptSubmit",
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
            liveEnvelopes: [],
            existingRuntime: [:]
        )

        XCTAssertEqual(update?.responsePayloads.first?.responseState, .responding)
        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(update?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .responding)
        XCTAssertFalse(snapshot.hasCompletedTurn)
        XCTAssertEqual(snapshot.externalSessionID, "claude-session-submit")
    }

    func testClaudeQueuedPromptStopKeepsRespondingUntilLastQueuedTurnCompletes() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "claude-session-queued"

        let submitPayload = """
        {
          "session_id": "\(externalSessionID)"
        }
        """

        let firstSubmit = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500200,
              "payload": \(quoted(submitPayload))
            }
            """.utf8
        )
        let firstUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: firstSubmit,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )
        let firstSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(firstUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(firstSnapshot.responseState, .responding)

        let secondSubmit = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500200.5,
              "payload": \(quoted(submitPayload))
            }
            """.utf8
        )
        let secondUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: secondSubmit,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: firstSnapshot]
        )
        let secondSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(secondUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(secondSnapshot.responseState, .responding)

        let stopPayload = """
        {
          "session_id": "\(externalSessionID)"
        }
        """

        let firstStop = Data(
            """
            {
              "event": "Stop",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500201,
              "payload": \(quoted(stopPayload))
            }
            """.utf8
        )
        let firstStopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: firstStop,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: secondSnapshot]
        )
        XCTAssertNotNil(firstStopUpdate)
        XCTAssertTrue(firstStopUpdate?.responsePayloads.isEmpty ?? false)
        let deferredSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(firstStopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertNil(deferredSnapshot.responseState)
        XCTAssertFalse(deferredSnapshot.hasCompletedTurn)

        let secondStop = Data(
            """
            {
              "event": "Stop",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500202,
              "payload": \(quoted(stopPayload))
            }
            """.utf8
        )
        let secondStopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: secondStop,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: secondSnapshot]
        )

        XCTAssertEqual(secondStopUpdate?.responsePayloads.first?.responseState, .idle)
        let finalSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(secondStopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(finalSnapshot.responseState, .idle)
        XCTAssertTrue(finalSnapshot.hasCompletedTurn)
    }

    func testClaudeSessionEndAfterCompletedStopDoesNotOverwriteCompletion() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "claude-session-complete"

        let submitPayload = """
        {
          "session_id": "\(externalSessionID)"
        }
        """
        let submitData = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500300,
              "payload": \(quoted(submitPayload))
            }
            """.utf8
        )
        let submitUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: submitData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )
        let currentSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(submitUpdate?.runtimeSnapshotsBySessionID[sessionID])

        let stopPayload = """
        {
          "session_id": "\(externalSessionID)"
        }
        """
        let stopData = Data(
            """
            {
              "event": "Stop",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500301,
              "payload": \(quoted(stopPayload))
            }
            """.utf8
        )
        let stopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: stopData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: currentSnapshot]
        )
        let completedSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(stopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(completedSnapshot.responseState, .idle)
        XCTAssertTrue(completedSnapshot.hasCompletedTurn)

        let sessionEndData = Data(
            """
            {
              "event": "SessionEnd",
              "tool": "claude",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776500302,
              "payload": \(quoted(stopPayload))
            }
            """.utf8
        )
        let sessionEndUpdate = await factory.handleRuntimeSocketEvent(
            kind: "claude-hook",
            payloadData: sessionEndData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: completedSnapshot]
        )

        XCTAssertNil(sessionEndUpdate)
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

    func testCodexStopWithFinalAnswerBeforeTaskCompleteBecomesDefinitiveIdle() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()

        let submitPayload = """
        {
          "session_id": "codex-thread-final-answer"
        }
        """

        let submitData = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510700,
              "payload": \(quoted(submitPayload))
            }
            """.utf8
        )
        let submitUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: submitData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )
        let currentSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(submitUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(currentSnapshot.responseState, .responding)

        let transcriptURL = try makeCodexTranscript(lines: [
            """
            {"timestamp":"2026-04-18T11:11:40Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codux"}}
            """,
            """
            {"timestamp":"2026-04-18T11:11:41Z","type":"event_msg","payload":{"type":"task_started","started_at":1776510701}}
            """,
            """
            {"timestamp":"2026-04-18T11:11:42Z","type":"event_msg","payload":{"type":"agent_message","message":"已完成","phase":"final_answer"}}
            """,
            """
            {"timestamp":"2026-04-18T11:11:43Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":777}}}}
            """
        ])
        defer { try? FileManager.default.removeItem(at: transcriptURL) }

        let stopPayload = """
        {
          "session_id": "codex-thread-final-answer",
          "transcript_path": "\(transcriptURL.path)"
        }
        """

        let stopData = Data(
            """
            {
              "event": "Stop",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510704,
              "payload": \(quoted(stopPayload))
            }
            """.utf8
        )
        let stopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: stopData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: currentSnapshot]
        )

        XCTAssertEqual(stopUpdate?.responsePayloads.first?.responseState, .idle)
        let snapshot: AIRuntimeContextSnapshot = try XCTUnwrap(stopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(snapshot.responseState, .idle)
        XCTAssertTrue(snapshot.hasCompletedTurn)
        XCTAssertFalse(snapshot.wasInterrupted)
        XCTAssertEqual(snapshot.totalTokens, 777)
    }

    func testCodexLatePreviousTurnStopDoesNotClearNewRespondingTurn() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()

        let submitPayload = """
        {
          "session_id": "codex-thread-race"
        }
        """

        let submitData = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510005,
              "payload": \(quoted(submitPayload))
            }
            """.utf8
        )
        let submitUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: submitData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )
        let currentSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(submitUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(currentSnapshot.responseState, .responding)

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

        let stopPayload = """
        {
          "session_id": "codex-thread-race",
          "transcript_path": "\(transcriptURL.path)"
        }
        """

        let stopData = Data(
            """
            {
              "event": "Stop",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510006,
              "payload": \(quoted(stopPayload))
            }
            """.utf8
        )
        let stopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: stopData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: currentSnapshot]
        )

        XCTAssertNotNil(stopUpdate)
        XCTAssertTrue(stopUpdate?.responsePayloads.isEmpty ?? false)
        let staleStopSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(stopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertNil(staleStopSnapshot.responseState)
        XCTAssertFalse(staleStopSnapshot.hasCompletedTurn)
        XCTAssertFalse(staleStopSnapshot.wasInterrupted)
        XCTAssertEqual(staleStopSnapshot.totalTokens, 1234)
        XCTAssertEqual(staleStopSnapshot.externalSessionID, "codex-thread-race")
    }

    func testCodexQueuedPromptStopKeepsRespondingUntilLastQueuedTurnCompletes() async throws {
        let factory = AIToolDriverFactory.shared
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-thread-queued"

        let firstSubmitPayload = """
        {
          "session_id": "\(externalSessionID)"
        }
        """
        let firstSubmitData = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510800,
              "payload": \(quoted(firstSubmitPayload))
            }
            """.utf8
        )
        let firstSubmitUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: firstSubmitData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [:]
        )
        let firstSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(firstSubmitUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(firstSnapshot.responseState, .responding)

        let secondSubmitPayload = """
        {
          "session_id": "\(externalSessionID)"
        }
        """
        let secondSubmitData = Data(
            """
            {
              "event": "UserPromptSubmit",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510801,
              "payload": \(quoted(secondSubmitPayload))
            }
            """.utf8
        )
        let secondSubmitUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: secondSubmitData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: firstSnapshot]
        )
        let secondSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(secondSubmitUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(secondSnapshot.responseState, .responding)

        let firstStopURL = try makeCodexTranscript(lines: [
            """
            {"timestamp":"2026-04-18T11:13:20Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codux"}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:20Z","type":"event_msg","payload":{"type":"task_started","started_at":1776510800}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:21Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":100}}}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:22Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776510802}}
            """
        ])
        defer { try? FileManager.default.removeItem(at: firstStopURL) }

        let firstStopPayload = """
        {
          "session_id": "\(externalSessionID)",
          "transcript_path": "\(firstStopURL.path)"
        }
        """
        let firstStopData = Data(
            """
            {
              "event": "Stop",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510802,
              "payload": \(quoted(firstStopPayload))
            }
            """.utf8
        )
        let firstStopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: firstStopData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: secondSnapshot]
        )
        XCTAssertNotNil(firstStopUpdate)
        XCTAssertTrue(firstStopUpdate?.responsePayloads.isEmpty ?? false)
        let firstStopSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(firstStopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertNil(firstStopSnapshot.responseState)
        XCTAssertFalse(firstStopSnapshot.hasCompletedTurn)

        let secondStopURL = try makeCodexTranscript(lines: [
            """
            {"timestamp":"2026-04-18T11:13:20Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codux"}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:20Z","type":"event_msg","payload":{"type":"task_started","started_at":1776510800}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:22Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776510802}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:23Z","type":"event_msg","payload":{"type":"task_started","started_at":1776510803}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:24Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":150}}}}
            """,
            """
            {"timestamp":"2026-04-18T11:13:25Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1776510805}}
            """
        ])
        defer { try? FileManager.default.removeItem(at: secondStopURL) }

        let secondStopPayload = """
        {
          "session_id": "\(externalSessionID)",
          "transcript_path": "\(secondStopURL.path)"
        }
        """
        let secondStopData = Data(
            """
            {
              "event": "Stop",
              "tool": "codex",
              "dmuxSessionId": "\(sessionID.uuidString)",
              "dmuxProjectId": "\(projectID.uuidString)",
              "dmuxProjectPath": "/tmp/codux",
              "receivedAt": 1776510805,
              "payload": \(quoted(secondStopPayload))
            }
            """.utf8
        )
        let secondStopUpdate = await factory.handleRuntimeSocketEvent(
            kind: "codex-hook",
            payloadData: secondStopData,
            projects: [],
            liveEnvelopes: [],
            existingRuntime: [sessionID: secondSnapshot]
        )

        XCTAssertEqual(secondStopUpdate?.responsePayloads.first?.responseState, .idle)
        let finalSnapshot: AIRuntimeContextSnapshot = try XCTUnwrap(secondStopUpdate?.runtimeSnapshotsBySessionID[sessionID])
        XCTAssertEqual(finalSnapshot.responseState, .idle)
        XCTAssertTrue(finalSnapshot.hasCompletedTurn)
        XCTAssertEqual(finalSnapshot.totalTokens, 150)
    }

    func testCodexSettledOnlyStopWhileRespondingDoesNotEmitIdle() throws {
        let resolution = resolveCodexHookStopResolution(
            parsedState: CodexParsedRuntimeState(
                model: "gpt-5.4",
                totalTokens: 100,
                updatedAt: 123,
                startedAt: 100,
                completedAt: 123,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: false
            ),
            currentResponseState: .responding,
            shouldIgnoreDefinitiveStop: false
        )

        XCTAssertFalse(resolution.hasDefinitiveStop)
        XCTAssertFalse(resolution.shouldEmitIdle)
        XCTAssertFalse(resolution.effectiveWasInterrupted)
        XCTAssertFalse(resolution.effectiveHasCompletedTurn)
    }

    func testCodexProbeDefinitiveStopIgnoredWhenNewerRespondingExists() throws {
        let resolution = resolveCodexProbeResolution(
            parsedState: CodexParsedRuntimeState(
                model: "gpt-5.4",
                totalTokens: 200,
                updatedAt: 500,
                startedAt: 490,
                completedAt: 500,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true
            ),
            shouldIgnoreDefinitiveStop: true,
            didReleaseDefinitiveStop: false
        )

        XCTAssertNil(resolution.responseState)
        XCTAssertFalse(resolution.effectiveWasInterrupted)
        XCTAssertFalse(resolution.effectiveHasCompletedTurn)
    }

    func testCodexProbeReleasedDefinitiveStopBecomesIdle() throws {
        let resolution = resolveCodexProbeResolution(
            parsedState: CodexParsedRuntimeState(
                model: "gpt-5.4",
                totalTokens: 260,
                updatedAt: 510,
                startedAt: 500,
                completedAt: 510,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true
            ),
            shouldIgnoreDefinitiveStop: false,
            didReleaseDefinitiveStop: true
        )

        XCTAssertEqual(resolution.responseState, .idle)
        XCTAssertFalse(resolution.effectiveWasInterrupted)
        XCTAssertTrue(resolution.effectiveHasCompletedTurn)
    }

    func testCodexDefinitiveStopReferenceUsesCompletedAtBeforeThreadUpdatedAt() throws {
        let parsedState = CodexParsedRuntimeState(
            model: "gpt-5.4",
            totalTokens: 300,
            updatedAt: 1_776_613_307.972,
            startedAt: 1_776_613_001.000,
            completedAt: 1_776_613_307.100,
            responseState: .idle,
            wasInterrupted: false,
            hasCompletedTurn: true
        )

        let stopReference = resolveCodexDefinitiveStopReferenceUpdatedAt(
            parsedState: parsedState,
            fallbackUpdatedAt: 1_776_613_307.972
        )

        XCTAssertEqual(stopReference, 1_776_613_307.100, accuracy: 0.0001)
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
