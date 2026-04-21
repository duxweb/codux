import XCTest
@testable import DmuxWorkspace

@MainActor
final class CodexRuntimePollingServiceTests: XCTestCase {
    private let store = AISessionStore.shared

    override func setUp() async throws {
        store.reset()
    }

    override func tearDown() async throws {
        store.reset()
    }

    func testPollingResolvesInterruptedCodexTurnWithoutStopHook() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        let transcriptURL = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("dmux-codex-poll-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: transcriptURL) }

        let rows = [
            #"{"timestamp":"2026-04-21T04:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
            #"{"timestamp":"2026-04-21T04:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713672001}}"#,
            #"{"timestamp":"2026-04-21T04:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":91}}}}"#,
            #"{"timestamp":"2026-04-21T04:00:03Z","type":"event_msg","payload":{"type":"turn_aborted","completed_at":1713672003}}"#
        ]
        try rows.joined(separator: "\n").appending("\n").write(to: transcriptURL, atomically: true, encoding: .utf8)

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/codex-project",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 10,
                updatedAt: 100,
                metadata: nil
            )
        )

        let service = CodexRuntimePollingService(
            sessionStore: store,
            notificationCenter: NotificationCenter()
        ) { _, _ in
            transcriptURL
        }

        await service.pollOnceForTesting()

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.wasInterrupted)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertEqual(session.committedTotalTokens, 91)
        XCTAssertEqual(session.transcriptPath, transcriptURL.path)
    }
}
