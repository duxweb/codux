import XCTest
@testable import DmuxWorkspace

@MainActor
final class AISessionStoreTests: XCTestCase {
    private let store = AISessionStore.shared

    override func setUp() async throws {
        store.reset()
    }

    override func tearDown() async throws {
        store.reset()
    }

    func testPromptSubmitThenTurnCompleteTracksBaselineAndCommittedTokens() throws {
        let terminalID = UUID()
        let projectID = UUID()

        XCTAssertTrue(
            store.apply(
                AIHookEvent(
                    kind: .promptSubmitted,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-1",
                    projectID: projectID,
                    projectName: "Codux",
                    sessionTitle: "Terminal",
                    tool: "codex",
                    aiSessionID: "codex-session",
                    model: "gpt-5.4",
                    totalTokens: 12,
                    updatedAt: 100,
                    metadata: nil
                )
            )
        )

        XCTAssertTrue(
            store.apply(
                AIHookEvent(
                    kind: .turnCompleted,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-1",
                    projectID: projectID,
                    projectName: "Codux",
                    sessionTitle: "Terminal",
                    tool: "codex",
                    aiSessionID: "codex-session",
                    model: "gpt-5.4",
                    totalTokens: 42,
                    updatedAt: 101,
                    metadata: nil
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertEqual(session.baselineTotalTokens, 12)
        XCTAssertEqual(session.committedTotalTokens, 42)
        XCTAssertEqual(session.turnSequence, 1)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens, 42)
        XCTAssertEqual(snapshot.baselineTotalTokens, 12)
    }

    func testNeedsInputProducesWaitingPhase() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "sonnet",
                totalTokens: 5,
                updatedAt: 100,
                metadata: nil
            )
        )
        _ = store.apply(
            AIHookEvent(
                kind: .needsInput,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "sonnet",
                totalTokens: nil,
                updatedAt: 101,
                metadata: .init(transcriptPath: nil, notificationType: "permission-request", reason: nil)
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .needsInput)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .waitingInput(tool: "claude"))
    }

    func testWaitingInputContextUsesNewestInteractionMetadata() throws {
        let projectID = UUID()
        let firstTerminalID = UUID()
        let secondTerminalID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .needsInput,
                terminalID: firstTerminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal A",
                tool: "claude",
                aiSessionID: "claude-a",
                model: "sonnet",
                totalTokens: nil,
                updatedAt: 100,
                metadata: .init(
                    transcriptPath: nil,
                    notificationType: "permission-request",
                    reason: nil,
                    targetToolName: "Bash",
                    message: "Approve Bash?"
                )
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .needsInput,
                terminalID: secondTerminalID,
                terminalInstanceID: "instance-2",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal B",
                tool: "codex",
                aiSessionID: "codex-b",
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: 101,
                metadata: .init(
                    transcriptPath: nil,
                    notificationType: "review",
                    reason: nil,
                    targetToolName: "ReadFile",
                    message: "Need review"
                )
            )
        )

        let context = try XCTUnwrap(store.waitingInputContext(projectID: projectID))
        XCTAssertEqual(context.tool, "codex")
        XCTAssertEqual(context.notificationType, "review")
        XCTAssertEqual(context.targetToolName, "ReadFile")
        XCTAssertEqual(context.message, "Need review")
        XCTAssertEqual(context.updatedAt, 101)
    }

    func testInterruptClearsRespondingState() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "gemini",
                aiSessionID: "gemini-session",
                model: "gemini-2.5-pro",
                totalTokens: 8,
                updatedAt: 100,
                metadata: nil
            )
        )

        XCTAssertTrue(store.markInterrupted(terminalID: terminalID, updatedAt: 101))
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.wasInterrupted)
    }

    func testRuntimeResolutionIgnoresStaleTurnSequence() throws {
        let terminalID = UUID()
        let projectID = UUID()

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
                totalTokens: 8,
                updatedAt: 100,
                metadata: nil
            )
        )
        let firstTurn = try XCTUnwrap(store.session(for: terminalID)).turnSequence

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
                totalTokens: 9,
                updatedAt: 101,
                metadata: nil
            )
        )

        XCTAssertFalse(
            store.applyRuntimeResolution(
                .init(
                    terminalID: terminalID,
                    turnSequence: firstTurn,
                    updatedAt: 102,
                    model: "gpt-5.4",
                    totalTokens: 25,
                    transcriptPath: "/tmp/old.jsonl",
                    wasInterrupted: true,
                    hasCompletedTurn: false
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertEqual(session.turnSequence, firstTurn + 1)
    }

    func testRuntimeResolutionCompletesCurrentCodexTurn() throws {
        let terminalID = UUID()
        let projectID = UUID()

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
                totalTokens: 8,
                updatedAt: 100,
                metadata: nil
            )
        )
        let turnSequence = try XCTUnwrap(store.session(for: terminalID)).turnSequence

        XCTAssertTrue(
            store.applyRuntimeResolution(
                .init(
                    terminalID: terminalID,
                    turnSequence: turnSequence,
                    updatedAt: 102,
                    model: "gpt-5.4-mini",
                    totalTokens: 25,
                    transcriptPath: "/tmp/codex.jsonl",
                    wasInterrupted: true,
                    hasCompletedTurn: false
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.wasInterrupted)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertEqual(session.committedTotalTokens, 25)
        XCTAssertEqual(session.transcriptPath, "/tmp/codex.jsonl")
        XCTAssertEqual(session.model, "gpt-5.4-mini")
    }

    func testStaleTerminalInstanceEventIsIgnored() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-new",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "session-new",
                model: "gpt-5.4",
                totalTokens: 20,
                updatedAt: 200,
                metadata: nil
            )
        )

        XCTAssertFalse(
            store.apply(
                AIHookEvent(
                    kind: .turnCompleted,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-old",
                    projectID: projectID,
                    projectName: "Codux",
                    sessionTitle: "Terminal",
                    tool: "codex",
                    aiSessionID: "session-old",
                    model: "gpt-5.4-mini",
                    totalTokens: 3,
                    updatedAt: 100,
                    metadata: nil
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.terminalInstanceID, "instance-new")
        XCTAssertEqual(session.aiSessionID, "session-new")
        XCTAssertEqual(session.committedTotalTokens, 20)
        XCTAssertEqual(session.state, .responding)
    }
}
