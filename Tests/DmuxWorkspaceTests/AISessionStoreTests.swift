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
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
    }

    func testProjectPhaseReturnsCompletedBrieflyAfterSuccessfulTurn() throws {
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
                model: "claude-sonnet-4-6",
                totalTokens: 12,
                updatedAt: Date().timeIntervalSince1970 - 1,
                metadata: nil
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 42,
                updatedAt: Date().timeIntervalSince1970,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        guard case .completed(let tool, _, let exitCode) = store.projectPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "claude")
        XCTAssertNil(exitCode)
    }

    func testCompletedTurnRemainsVisibleAsLiveSnapshotDuringCompletionWindow() throws {
        let terminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
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
                updatedAt: now - 1,
                metadata: nil
            )
        )

        _ = store.apply(
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
                updatedAt: now,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        let snapshots = store.liveSnapshots(projectID: projectID)
        XCTAssertEqual(snapshots.count, 1)
        XCTAssertEqual(snapshots.first?.sessionID, terminalID)
        XCTAssertEqual(snapshots.first?.tool, "codex")
        XCTAssertEqual(snapshots.first?.model, "gpt-5.4")
        XCTAssertEqual(snapshots.first?.currentTotalTokens, 42)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
        guard case .completed(let tool, _, _) = store.projectPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "codex")
    }

    func testCompletedTurnFallsOutOfLiveSnapshotsAfterCompletionWindowExpires() throws {
        let terminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 42,
                updatedAt: now - 10,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testProjectPhaseDoesNotReturnCompletedForInterruptedTurn() throws {
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: Date().timeIntervalSince1970 - 1,
                metadata: nil
            )
        )

        _ = store.apply(
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
                totalTokens: 12,
                updatedAt: Date().timeIntervalSince1970,
                metadata: .init(wasInterrupted: true, hasCompletedTurn: false)
            )
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
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
        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
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

    func testSwitchingToolOnSameTerminalResetsBaselineAndCommittedTokens() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
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
                totalTokens: 14_659,
                updatedAt: 100,
                metadata: nil
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: nil,
                updatedAt: 101,
                metadata: nil
            )
        )

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
                model: "claude-sonnet-4-6",
                totalTokens: 0,
                updatedAt: 102,
                metadata: nil
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.tool, "claude")
        XCTAssertEqual(session.aiSessionID, "claude-session")
        XCTAssertEqual(session.committedTotalTokens, 0)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.state, .responding)
    }

    func testStartingNewToolOnSameTerminalDoesNotLeakCompletedPreviousToolIntoLiveSnapshots() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
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
                totalTokens: 14_659,
                updatedAt: 100,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)
        XCTAssertEqual(store.liveSnapshots(projectID: projectID).first?.tool, "codex")

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: nil,
                updatedAt: 101,
                metadata: nil
            )
        )

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.tool, "claude")
        XCTAssertEqual(snapshot.currentTotalTokens, 0)
        XCTAssertEqual(snapshot.baselineTotalTokens, 0)
    }

    func testSessionEndedRemovesLiveTerminalBinding() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 12,
                updatedAt: 100,
                metadata: nil
            )
        )

        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)

        XCTAssertTrue(
            store.apply(
                AIHookEvent(
                    kind: .sessionEnded,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-1",
                    projectID: projectID,
                    projectName: "Codux",
                    sessionTitle: "Claude",
                    tool: "claude",
                    aiSessionID: "claude-session",
                    model: "claude-sonnet-4-6",
                    totalTokens: 12,
                    updatedAt: 101,
                    metadata: nil
                )
            )
        )

        XCTAssertNil(store.session(for: terminalID))
        XCTAssertTrue(store.liveSnapshots(projectID: projectID).isEmpty)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }
}
