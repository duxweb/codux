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

    func testPromptSubmitThenTurnCompleteKeepsSessionBaseline() throws {
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
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedTotalTokens, 0)
        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 0)
        guard case .completed(let tool, _, let exitCode) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "codex")
        XCTAssertNil(exitCode)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
    }

    func testProjectPhaseReturnsCompletedAfterSuccessfulTurnUntilCleared() throws {
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

        guard case .completed(let tool, _, let exitCode) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "claude")
        XCTAssertNil(exitCode)
    }

    func testToolActivityPromptSubmittedDoesNotStartLoadingFromIdle() throws {
        let terminalID = UUID()
        let projectID = UUID()

        XCTAssertFalse(
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
                    totalTokens: nil,
                    updatedAt: 100,
                    metadata: .init(source: "tool-use")
                )
            )
        )

        XCTAssertNil(store.session(for: terminalID))
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testToolActivityPromptSubmittedIsIgnoredWhileExistingLoadingSessionStaysRunning() throws {
        let terminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

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
                    totalTokens: nil,
                    updatedAt: now - 1,
                    metadata: .init(source: "user-input")
                )
            )
        )

        XCTAssertFalse(
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
                    totalTokens: nil,
                    updatedAt: now,
                    metadata: .init(source: "tool-use")
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.updatedAt, now - 1)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))
    }

    func testToolActivityPromptSubmittedDoesNotReviveCompletedSession() throws {
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
                totalTokens: nil,
                updatedAt: 100,
                metadata: .init(source: "user-input")
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
                totalTokens: nil,
                updatedAt: 101,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        XCTAssertFalse(
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
                    totalTokens: nil,
                    updatedAt: 120,
                    metadata: .init(source: "tool-use")
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.hasCompletedTurn)
        XCTAssertEqual(session.updatedAt, 101)
        guard case .completed(let tool, _, _) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "codex")
    }

    func testInternalCodexMemoryPromptSubmittedIsIgnored() throws {
        let terminalID = UUID()
        let projectID = UUID()
        let memoriesPath = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent(".codex/memories", isDirectory: true)
            .path

        XCTAssertFalse(
            store.apply(
                AIHookEvent(
                    kind: .promptSubmitted,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-1",
                    projectID: projectID,
                    projectName: "Codux",
                    projectPath: memoriesPath,
                    sessionTitle: "Codex Memory",
                    tool: "codex",
                    aiSessionID: "codex-memory-session",
                    model: "gpt-5.4",
                    totalTokens: nil,
                    updatedAt: 100,
                    metadata: .init(source: "user-input")
                )
            )
        )

        XCTAssertNil(store.session(for: terminalID))
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testInternalCodexMemoryToolActivityIsIgnoredWhileProjectSessionKeepsLoading() throws {
        let terminalID = UUID()
        let projectID = UUID()
        let projectPath = "/tmp/codux-project"
        let memoriesPath = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent(".codex/memories", isDirectory: true)
            .path
        let now = Date().timeIntervalSince1970

        XCTAssertTrue(
            store.apply(
                AIHookEvent(
                    kind: .promptSubmitted,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-1",
                    projectID: projectID,
                    projectName: "Codux",
                    projectPath: projectPath,
                    sessionTitle: "Terminal",
                    tool: "codex",
                    aiSessionID: "codex-session",
                    model: "gpt-5.4",
                    totalTokens: nil,
                    updatedAt: now - 1,
                    metadata: .init(source: "user-input")
                )
            )
        )

        XCTAssertFalse(
            store.apply(
                AIHookEvent(
                    kind: .promptSubmitted,
                    terminalID: terminalID,
                    terminalInstanceID: "instance-1",
                    projectID: projectID,
                    projectName: "Codux",
                    projectPath: memoriesPath,
                    sessionTitle: "Codex Memory",
                    tool: "codex",
                    aiSessionID: "codex-memory-session",
                    model: "gpt-5.4",
                    totalTokens: nil,
                    updatedAt: now,
                    metadata: .init(source: "tool-use")
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.aiSessionID, "codex-session")
        XCTAssertEqual(session.projectPath, projectPath)
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.updatedAt, now - 1)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))
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
        XCTAssertEqual(snapshots.first?.currentTotalTokens, 0)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
        guard case .completed(let tool, _, _) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "codex")
    }

    func testClearCompletedRemovesCompletedProjectPhase() throws {
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
        guard case .completed(let tool, _, _) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase before clearing")
        }
        XCTAssertEqual(tool, "claude")

        XCTAssertTrue(store.clearCompleted(projectID: projectID))
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)
    }

    func testRemoveMissingManagedTerminalSessionsPrunesStaleInstance() throws {
        let staleTerminalID = UUID()
        let activeTerminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: staleTerminalID,
                terminalInstanceID: "stale-instance",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Stale",
                tool: "codex",
                aiSessionID: "stale-session",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: now - 2,
                metadata: nil
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: activeTerminalID,
                terminalInstanceID: "active-instance",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Active",
                tool: "claude",
                aiSessionID: "active-session",
                model: "claude-sonnet-4-6",
                totalTokens: 24,
                updatedAt: now - 1,
                metadata: nil
            )
        )

        let removed = store.removeMissingManagedTerminalSessions(
            liveInstanceIDs: ["active-instance"]
        )

        XCTAssertEqual(removed, [staleTerminalID])
        XCTAssertNil(store.session(for: staleTerminalID))
        XCTAssertNotNil(store.session(for: activeTerminalID))
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

    func testRespondingPhaseExpiresToIdleAfterRunningLifetime() throws {
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
                updatedAt: Date().timeIntervalSince1970 - 181,
                metadata: nil
            )
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
        XCTAssertFalse(store.isRunning(terminalID: terminalID))
    }

    func testExpiredRespondingSessionDoesNotReviveOlderCompletedPhase() throws {
        let completedTerminalID = UUID()
        let staleRespondingTerminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: completedTerminalID,
                terminalInstanceID: "instance-completed",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 42,
                updatedAt: now - 300,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: staleRespondingTerminalID,
                terminalInstanceID: "instance-responding",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 0,
                updatedAt: now - 181,
                metadata: nil
            )
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
        XCTAssertNil(store.completedPhase(projectID: projectID))
    }

    func testRuntimeSnapshotRespondingRestoresRunningPhaseFromRuntime() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
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

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 12,
                    outputTokens: 8,
                    cachedInputTokens: 0,
                    totalTokens: 20,
                    updatedAt: Date().timeIntervalSince1970,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertTrue(store.isRunning(terminalID: terminalID))
        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))
        XCTAssertEqual(session.committedTotalTokens, 20)
    }

    func testSessionStartedSeedsBaselineToInitialCommittedTotals() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                inputTokens: 9_500_000,
                outputTokens: 0,
                cachedInputTokens: 1_200_000,
                totalTokens: 9_500_000,
                updatedAt: 100,
                metadata: nil
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.committedTotalTokens, 0)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedCachedInputTokens, 0)
        XCTAssertEqual(session.baselineCachedInputTokens, 0)
        XCTAssertFalse(session.baselineResolved)

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 0)
        XCTAssertEqual(
            (snapshot.currentTotalTokens + snapshot.currentCachedInputTokens)
                - (snapshot.baselineTotalTokens + snapshot.baselineCachedInputTokens),
            0
        )
    }

    func testLiveDeltaOnlyCountsGrowthAfterSessionStartedSeed() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                inputTokens: 9_500_000,
                outputTokens: 0,
                cachedInputTokens: 1_200_000,
                totalTokens: 9_500_000,
                updatedAt: 100,
                metadata: nil
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 9_650_000,
                    outputTokens: 80_000,
                    cachedInputTokens: 1_260_000,
                    totalTokens: 9_730_000,
                    updatedAt: 101,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 9_730_000)
        XCTAssertEqual(
            (snapshot.currentTotalTokens + snapshot.currentCachedInputTokens)
                - (snapshot.baselineTotalTokens + snapshot.baselineCachedInputTokens),
            10_990_000
        )
    }

    func testRepeatedPromptSubmittedWhileRespondingDoesNotResetBaseline() throws {
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
                totalTokens: 100,
                updatedAt: 100,
                metadata: nil
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 140,
                    outputTokens: 80,
                    cachedInputTokens: 0,
                    totalTokens: 220,
                    updatedAt: 110,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 220,
                updatedAt: 120,
                metadata: nil
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedTotalTokens, 220)

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 220)
    }

    func testPromptSubmittedAfterNeedsInputDoesNotResetExistingTurnBaseline() throws {
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
                totalTokens: 100,
                updatedAt: 100,
                metadata: nil
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 140,
                    outputTokens: 80,
                    cachedInputTokens: 0,
                    totalTokens: 220,
                    updatedAt: 110,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                updatedAt: 120,
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 220,
                updatedAt: 130,
                metadata: nil
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedTotalTokens, 220)
    }

    func testPromptSubmittedAfterCompletedTurnKeepsSessionBaseline() throws {
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
                totalTokens: 100,
                updatedAt: 100,
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
                totalTokens: 220,
                updatedAt: 110,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 220,
                updatedAt: 120,
                metadata: nil
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedTotalTokens, 0)

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 0)
    }

    func testRestoredSessionStartsLiveBaselineFromFirstObservedRuntimeTotals() throws {
        let terminalID = UUID()
        let projectID = UUID()

        store.registerExpectedLogicalSession(
            terminalID: terminalID,
            tool: "codex",
            aiSessionID: "codex-session"
        )

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Restored",
                tool: "codex",
                aiSessionID: nil,
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: 100,
                metadata: nil
            )
        )

        let seededSession = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(seededSession.aiSessionID, "codex-session")
        XCTAssertEqual(seededSession.baselineTotalTokens, 0)
        XCTAssertEqual(seededSession.committedTotalTokens, 0)
        XCTAssertEqual(seededSession.baselineCachedInputTokens, 0)

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 860,
                    outputTokens: 430,
                    cachedInputTokens: 170,
                    totalTokens: 1_290,
                    updatedAt: 110,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let runtimeSeededSession = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(runtimeSeededSession.baselineTotalTokens, 1_290)
        XCTAssertEqual(runtimeSeededSession.committedTotalTokens, 1_290)
        XCTAssertEqual(runtimeSeededSession.baselineCachedInputTokens, 170)
        XCTAssertEqual(runtimeSeededSession.committedCachedInputTokens, 170)

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 920,
                    outputTokens: 470,
                    cachedInputTokens: 190,
                    totalTokens: 1_390,
                    updatedAt: 111,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 100)
        XCTAssertEqual(snapshot.currentCachedInputTokens - snapshot.baselineCachedInputTokens, 20)
    }

    func testRestoredSessionWithDirectSessionIDSeedsBaselineFromRestoredOrigin() throws {
        let terminalID = UUID()
        let projectID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Restored",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: 100,
                metadata: nil
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 860,
                    outputTokens: 430,
                    cachedInputTokens: 170,
                    totalTokens: 1_290,
                    updatedAt: 110,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false,
                    sessionOrigin: .restored,
                    source: .probe
                )
            )
        )

        let restoredSession = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(restoredSession.baselineTotalTokens, 1_290)
        XCTAssertEqual(restoredSession.committedTotalTokens, 1_290)
        XCTAssertEqual(restoredSession.baselineCachedInputTokens, 170)
        XCTAssertEqual(restoredSession.committedCachedInputTokens, 170)

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 920,
                    outputTokens: 470,
                    cachedInputTokens: 190,
                    totalTokens: 1_390,
                    updatedAt: 111,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false,
                    sessionOrigin: .restored,
                    source: .probe
                )
            )
        )

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 100)
        XCTAssertEqual(snapshot.currentCachedInputTokens - snapshot.baselineCachedInputTokens, 20)
    }

    func testRestoredTerminalDoesNotBackfeedHistoricalLogicalTotalsIntoLiveBaseline() throws {
        let projectID = UUID()
        let sharedSessionID = "codex-session"
        let historicalTerminalID = UUID()
        let restoredTerminalID = UUID()

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: historicalTerminalID,
                terminalInstanceID: "instance-history",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "History",
                tool: "codex",
                aiSessionID: sharedSessionID,
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: 90,
                metadata: nil
            )
        )
        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: historicalTerminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: sharedSessionID,
                    model: "gpt-5.4",
                    inputTokens: 0,
                    outputTokens: 0,
                    cachedInputTokens: 0,
                    totalTokens: 1_200,
                    updatedAt: 91,
                    responseState: .idle,
                    wasInterrupted: false,
                    hasCompletedTurn: true
                )
            )
        )
        store.removeTerminal(historicalTerminalID)

        store.registerExpectedLogicalSession(
            terminalID: restoredTerminalID,
            tool: "codex",
            aiSessionID: sharedSessionID
        )

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: restoredTerminalID,
                terminalInstanceID: "instance-restored",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Restored",
                tool: "codex",
                aiSessionID: nil,
                model: "gpt-5.4",
                totalTokens: 1_200,
                updatedAt: 100,
                metadata: nil
            )
        )

        let restoredSession = try XCTUnwrap(store.session(for: restoredTerminalID))
        XCTAssertEqual(restoredSession.baselineTotalTokens, 0)
        XCTAssertEqual(restoredSession.committedTotalTokens, 0)

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: restoredTerminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: sharedSessionID,
                    model: "gpt-5.4",
                    inputTokens: 0,
                    outputTokens: 0,
                    cachedInputTokens: 0,
                    totalTokens: 1_260,
                    updatedAt: 101,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let snapshot = try XCTUnwrap(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: restoredTerminalID))
        XCTAssertEqual(snapshot.currentTotalTokens - snapshot.baselineTotalTokens, 60)
    }

    func testRuntimeSnapshotRespondingDoesNotOverrideCompletedPhase() throws {
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
                updatedAt: now,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "claude",
                    externalSessionID: "claude-session",
                    model: "claude-sonnet-4-6",
                    inputTokens: 20,
                    outputTokens: 30,
                    cachedInputTokens: 0,
                    totalTokens: 50,
                    updatedAt: now + 1,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
        guard case .completed(let tool, _, _) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "claude")
    }

    func testRuntimeIdleWithoutExplicitCompletionDoesNotCreateCompletedPhase() throws {
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

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 18,
                    outputTokens: 22,
                    cachedInputTokens: 0,
                    totalTokens: 40,
                    updatedAt: now - 0.25,
                    startedAt: now - 0.25,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 20,
                    outputTokens: 30,
                    cachedInputTokens: 0,
                    totalTokens: 50,
                    updatedAt: now,
                    responseState: .idle,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testQueuedPromptIgnoresRuntimeCompletionFromPreviousTurn() throws {
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
                updatedAt: now,
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
                updatedAt: now + 1,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 42,
                updatedAt: now + 2,
                metadata: .init(source: "user-input")
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 20,
                    outputTokens: 22,
                    cachedInputTokens: 0,
                    totalTokens: 42,
                    updatedAt: now + 2.1,
                    startedAt: now,
                    completedAt: now + 1,
                    responseState: .idle,
                    wasInterrupted: false,
                    hasCompletedTurn: true
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.activeTurnStartedAt, now + 2)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))
        XCTAssertEqual(
            store.latestActiveStartedAt(projectID: projectID),
            Date(timeIntervalSince1970: now + 2)
        )
    }

    func testProjectPhasePrefersRunningOverCompletedAcrossSplitSessions() throws {
        let completedTerminalID = UUID()
        let runningTerminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: completedTerminalID,
                terminalInstanceID: "instance-completed",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 42,
                updatedAt: now - 2,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: runningTerminalID,
                terminalInstanceID: "instance-running",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: now - 1,
                metadata: nil
            )
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))
    }

    func testProjectPhasePrefersWaitingInputOverCompletedAcrossSplitSessions() throws {
        let completedTerminalID = UUID()
        let waitingTerminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: completedTerminalID,
                terminalInstanceID: "instance-completed",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 42,
                updatedAt: now - 2,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .needsInput,
                terminalID: waitingTerminalID,
                terminalInstanceID: "instance-waiting",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                updatedAt: now - 1,
                metadata: .init(
                    notificationType: "permission-request",
                    targetToolName: "bash",
                    message: "Need approval"
                )
            )
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .waitingInput(tool: "claude"))
    }

    func testProjectPhaseKeepsCompletedWhenAnotherSplitIsIdleIncomplete() throws {
        let completedTerminalID = UUID()
        let idleTerminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: completedTerminalID,
                terminalInstanceID: "instance-completed",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 42,
                updatedAt: now - 2,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
                terminalID: idleTerminalID,
                terminalInstanceID: "instance-idle",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 0,
                updatedAt: now - 1,
                metadata: nil
            )
        )

        guard case .completed(let tool, _, _) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "claude")
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

    func testRuntimeSnapshotIdleClearsRespondingStateWithoutCreatingCompletion() throws {
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
                tool: "gemini",
                aiSessionID: "gemini-session",
                model: "gemini-2.5-pro",
                totalTokens: 8,
                updatedAt: now,
                metadata: nil
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "gemini",
                    externalSessionID: "gemini-session",
                    model: "gemini-2.5-pro",
                    inputTokens: 9,
                    outputTokens: 4,
                    cachedInputTokens: 0,
                    totalTokens: 13,
                    updatedAt: now + 0.5,
                    startedAt: now + 0.5,
                    responseState: .responding,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "gemini",
                    externalSessionID: "gemini-session",
                    model: "gemini-2.5-pro",
                    inputTokens: 10,
                    outputTokens: 6,
                    cachedInputTokens: 0,
                    totalTokens: 16,
                    updatedAt: now + 1,
                    responseState: .idle,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: terminalID)?.sessionID, terminalID)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testStaleIdleRuntimeSnapshotDoesNotClearNewlyStartedTurn() throws {
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
                totalTokens: 42,
                updatedAt: 100,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
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
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: 110,
                metadata: nil
            )
        )

        XCTAssertTrue(
            store.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: AIRuntimeContextSnapshot(
                    tool: "codex",
                    externalSessionID: "codex-session",
                    model: "gpt-5.4",
                    inputTokens: 20,
                    outputTokens: 30,
                    cachedInputTokens: 0,
                    totalTokens: 50,
                    updatedAt: 100.5,
                    responseState: .idle,
                    wasInterrupted: false,
                    hasCompletedTurn: false
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
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
        XCTAssertEqual(session.committedTotalTokens, 0)
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

    func testSessionEndedRetainsCompletedProjectPhase() throws {
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

        _ = store.apply(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 42,
                updatedAt: 101,
                metadata: .init(wasInterrupted: false, hasCompletedTurn: true)
            )
        )

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
                    totalTokens: 42,
                    updatedAt: 102,
                    metadata: nil
                )
            )
        )

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertEqual(store.liveSnapshots(projectID: projectID).count, 1)
        guard case .completed(let tool, _, let exitCode) = store.completedPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "claude")
        XCTAssertNil(exitCode)
    }
}
