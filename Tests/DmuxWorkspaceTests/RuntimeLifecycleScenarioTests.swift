import XCTest
@testable import DmuxWorkspace

@MainActor
final class RuntimeLifecycleScenarioTests: XCTestCase {
    private let store = AIRuntimeStateStore.shared

    override func setUp() async throws {
        store.reset()
    }

    override func tearDown() async throws {
        store.reset()
    }

    func testNewSessionTwoMessagesTrackLoadingAndTokenGrowth() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-new-1"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 100,
            totalTokens: 10
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 10)

        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 110,
            totalTokens: 20
        )
        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertTrue(displaySnapshot(projectID: projectID, sessionID: sessionID)?.hasCompletedTurn == true)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 20)

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 120,
            totalTokens: 25
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 25)

        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 130,
            totalTokens: 40
        )
        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 40)
    }

    func testInterruptDuringSessionClearsLoading() throws {
        let sessionID = UUID()
        let projectID = UUID()

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: "codex-interrupt-1",
            updatedAt: 200,
            totalTokens: 12
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        XCTAssertTrue(store.markInterrupted(sessionID: sessionID, updatedAt: 201))
        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertFalse(displaySnapshot(projectID: projectID, sessionID: sessionID)?.hasCompletedTurn ?? true)
    }

    func testInterruptAfterCompletionDoesNotRestoreLoading() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-interrupt-2"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 300,
            totalTokens: 15
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 310,
            totalTokens: 30
        )
        XCTAssertEqual(store.responseState(for: sessionID), .idle)

        XCTAssertFalse(store.markInterrupted(sessionID: sessionID, updatedAt: 311))
        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 30)
    }

    func testCodexStaleStopSnapshotDoesNotClearCurrentRespondingState() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-stale-stop"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 320,
            totalTokens: 40
        )

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: "gpt-5.4",
                inputTokens: 35,
                outputTokens: 0,
                totalTokens: 35,
                updatedAt: 321,
                responseState: nil,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .responding)
    }

    func testCodexQueuedSecondTurnFinalAnswerClearsLoadingImmediately() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-queued-final-answer"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 330,
            totalTokens: 100
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 331,
            totalTokens: 140
        )
        XCTAssertEqual(store.responseState(for: sessionID), .idle)

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 340,
            totalTokens: 140
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: "gpt-5.4",
                inputTokens: 177,
                outputTokens: 0,
                totalTokens: 177,
                updatedAt: 341,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                source: .hook
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertTrue(displaySnapshot(projectID: projectID, sessionID: sessionID)?.hasCompletedTurn == true)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 177)
    }

    func testCodexQueuedSecondTurnIgnoresStaleProbeIdleUntilRealSecondStop() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-queued-probe-stale"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 350,
            totalTokens: 100
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 351,
            totalTokens: 140
        )
        XCTAssertEqual(store.responseState(for: sessionID), .idle)

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 360,
            totalTokens: 140
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: "gpt-5.4",
                inputTokens: 140,
                outputTokens: 0,
                totalTokens: 140,
                updatedAt: 361,
                responseState: nil,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .probe
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 370,
            totalTokens: 220
        )

        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 220)
    }

    func testCodexQueuedSubmitKeepsLoadingAfterFirstStopAndClearsAfterSecondStop() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-queued-submit-store"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 380,
            totalTokens: 100
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 381,
            totalTokens: 100
        )
        XCTAssertEqual(store.responseState(for: sessionID), .responding)

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: "gpt-5.4",
                inputTokens: 140,
                outputTokens: 0,
                totalTokens: 140,
                updatedAt: 382,
                responseState: nil,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .responding)
        XCTAssertFalse(displaySnapshot(projectID: projectID, sessionID: sessionID)?.hasCompletedTurn ?? true)

        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 390,
            totalTokens: 180
        )

        XCTAssertEqual(store.responseState(for: sessionID), .idle)
        XCTAssertTrue(displaySnapshot(projectID: projectID, sessionID: sessionID)?.hasCompletedTurn == true)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 180)
    }

    func testCodexProbeSnapshotAfterNewSubmitKeepsRespondingWhenOnlyOldTurnWasCompleted() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-health-race"

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 470,
            totalTokens: 140
        )

        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: "gpt-5.4",
                inputTokens: 140,
                outputTokens: 0,
                totalTokens: 140,
                updatedAt: 472,
                responseState: nil,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .probe
            )
        )

        XCTAssertEqual(store.responseState(for: sessionID), .responding)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 140)
    }

    func testRestoredSessionStartsFromZeroThenGrowsAcrossMessages() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-restored-1"

        store.registerExpectedLogicalSession(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            indexedSummary: indexedSummary(
                sessionID: sessionID,
                projectID: projectID,
                tool: "codex",
                externalSessionID: externalSessionID,
                totalTokens: 500
            )
        )

        applyLiveIdle(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 400,
            totalTokens: 500
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 401,
            totalTokens: 500
        )
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 0)

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 410,
            totalTokens: 530
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 420,
            totalTokens: 550
        )
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 550)

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 430,
            totalTokens: 560
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 440,
            totalTokens: 580
        )
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 580)
    }

    func testResetThenRestoreHistoricalSessionStartsFromZero() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "codex-restored-2"

        store.registerExpectedLogicalSession(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            indexedSummary: indexedSummary(
                sessionID: sessionID,
                projectID: projectID,
                tool: "codex",
                externalSessionID: externalSessionID,
                totalTokens: 800
            )
        )
        applyLiveIdle(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 500,
            totalTokens: 800
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 501,
            totalTokens: 800
        )

        store.reset()

        store.registerExpectedLogicalSession(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            indexedSummary: indexedSummary(
                sessionID: sessionID,
                projectID: projectID,
                tool: "codex",
                externalSessionID: externalSessionID,
                totalTokens: 800
            )
        )
        applyLiveIdle(
            sessionID: sessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 510,
            totalTokens: 800
        )
        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "codex",
            externalSessionID: externalSessionID,
            updatedAt: 511,
            totalTokens: 800
        )

        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 0)
        XCTAssertEqual(store.responseState(for: sessionID), .idle)
    }

    func testResetThenMixFreshAndHistoricalSessionsKeepsBaselinesSeparate() throws {
        let projectID = UUID()
        let freshSessionID = UUID()
        let restoredSessionID = UUID()

        applyLiveResponding(
            sessionID: freshSessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: "codex-fresh-mixed",
            updatedAt: 600,
            totalTokens: 20
        )
        applyCompletedSnapshot(
            sessionID: freshSessionID,
            tool: "codex",
            externalSessionID: "codex-fresh-mixed",
            updatedAt: 610,
            totalTokens: 40
        )

        store.reset()

        applyLiveResponding(
            sessionID: freshSessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: "codex-fresh-mixed-2",
            updatedAt: 700,
            totalTokens: 15
        )
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: freshSessionID)?.currentTotalTokens, 15)

        store.registerExpectedLogicalSession(
            sessionID: restoredSessionID,
            tool: "codex",
            externalSessionID: "codex-restored-mixed",
            indexedSummary: indexedSummary(
                sessionID: restoredSessionID,
                projectID: projectID,
                tool: "codex",
                externalSessionID: "codex-restored-mixed",
                totalTokens: 900
            )
        )
        applyLiveIdle(
            sessionID: restoredSessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: "codex-restored-mixed",
            updatedAt: 710,
            totalTokens: 900
        )
        applyCompletedSnapshot(
            sessionID: restoredSessionID,
            tool: "codex",
            externalSessionID: "codex-restored-mixed",
            updatedAt: 711,
            totalTokens: 900
        )

        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: restoredSessionID)?.currentTotalTokens, 0)
        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: freshSessionID)?.currentTotalTokens, 15)
    }

    func testProjectPhasePrefersConcurrentRespondingSession() throws {
        let projectID = UUID()
        let codexSessionID = UUID()
        let claudeSessionID = UUID()

        applyLiveIdle(
            sessionID: codexSessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: "codex-concurrent",
            updatedAt: 800,
            totalTokens: 100
        )
        applyCompletedSnapshot(
            sessionID: codexSessionID,
            tool: "codex",
            externalSessionID: "codex-concurrent",
            updatedAt: 801,
            totalTokens: 100
        )

        applyLiveResponding(
            sessionID: claudeSessionID,
            projectID: projectID,
            tool: "claude",
            externalSessionID: "claude-concurrent",
            updatedAt: 900,
            totalTokens: 10
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "claude"))

        applyCompletedSnapshot(
            sessionID: claudeSessionID,
            tool: "claude",
            externalSessionID: "claude-concurrent",
            updatedAt: 901,
            totalTokens: 20
        )

        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testCurrentDisplaySnapshotRespectsSelectedSession() throws {
        let projectID = UUID()
        let olderSessionID = UUID()
        let newerSessionID = UUID()

        applyLiveResponding(
            sessionID: olderSessionID,
            projectID: projectID,
            tool: "codex",
            externalSessionID: "codex-selected",
            updatedAt: 1000,
            totalTokens: 10
        )
        applyLiveResponding(
            sessionID: newerSessionID,
            projectID: projectID,
            tool: "claude",
            externalSessionID: "claude-selected",
            updatedAt: 1100,
            totalTokens: 20
        )

        XCTAssertEqual(store.currentSnapshot(projectID: projectID, selectedSessionID: olderSessionID)?.sessionID, olderSessionID)
        XCTAssertEqual(store.currentDisplaySnapshot(projectID: projectID, selectedSessionID: olderSessionID)?.sessionID, olderSessionID)
        XCTAssertEqual(store.currentSnapshot(projectID: projectID, selectedSessionID: nil)?.sessionID, newerSessionID)
    }

    func testRestoredOpencodeRespondingDisplaysZeroUntilCompletedTurn() throws {
        let sessionID = UUID()
        let projectID = UUID()
        let externalSessionID = "opencode-restored"

        store.registerExpectedLogicalSession(
            sessionID: sessionID,
            tool: "opencode",
            externalSessionID: externalSessionID,
            indexedSummary: indexedSummary(
                sessionID: sessionID,
                projectID: projectID,
                tool: "opencode",
                externalSessionID: externalSessionID,
                totalTokens: 1200
            )
        )

        applyLiveResponding(
            sessionID: sessionID,
            projectID: projectID,
            tool: "opencode",
            externalSessionID: externalSessionID,
            updatedAt: 1200,
            totalTokens: 1200
        )

        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 0)

        applyCompletedSnapshot(
            sessionID: sessionID,
            tool: "opencode",
            externalSessionID: externalSessionID,
            updatedAt: 1210,
            totalTokens: 1250
        )

        XCTAssertEqual(displaySnapshot(projectID: projectID, sessionID: sessionID)?.currentTotalTokens, 50)
    }

    private func applyLiveResponding(
        sessionID: UUID,
        projectID: UUID,
        tool: String,
        externalSessionID: String,
        updatedAt: Double,
        totalTokens: Int
    ) {
        store.applyLiveEnvelope(
            AIToolUsageEnvelope(
                sessionId: sessionID.uuidString,
                sessionInstanceId: "instance-\(sessionID.uuidString)",
                invocationId: "invoke-\(updatedAt)",
                externalSessionID: externalSessionID,
                projectId: projectID.uuidString,
                projectName: "codux",
                projectPath: "/tmp/codux",
                sessionTitle: "Terminal",
                tool: tool,
                model: "gpt-5.4",
                status: "running",
                responseState: .responding,
                updatedAt: updatedAt,
                startedAt: updatedAt - 1,
                finishedAt: nil,
                inputTokens: totalTokens,
                outputTokens: 0,
                totalTokens: totalTokens,
                contextWindow: nil,
                contextUsedTokens: nil,
                contextUsagePercent: nil,
                source: .socket
            )
        )
    }

    private func applyLiveIdle(
        sessionID: UUID,
        projectID: UUID,
        tool: String,
        externalSessionID: String,
        updatedAt: Double,
        totalTokens: Int
    ) {
        store.applyLiveEnvelope(
            AIToolUsageEnvelope(
                sessionId: sessionID.uuidString,
                sessionInstanceId: "instance-\(sessionID.uuidString)",
                invocationId: "invoke-\(updatedAt)",
                externalSessionID: externalSessionID,
                projectId: projectID.uuidString,
                projectName: "codux",
                projectPath: "/tmp/codux",
                sessionTitle: "Terminal",
                tool: tool,
                model: "gpt-5.4",
                status: "running",
                responseState: .idle,
                updatedAt: updatedAt,
                startedAt: updatedAt - 1,
                finishedAt: nil,
                inputTokens: totalTokens,
                outputTokens: 0,
                totalTokens: totalTokens,
                contextWindow: nil,
                contextUsedTokens: nil,
                contextUsagePercent: nil,
                source: .socket
            )
        )
    }

    private func applyCompletedSnapshot(
        sessionID: UUID,
        tool: String,
        externalSessionID: String,
        updatedAt: Double,
        totalTokens: Int
    ) {
        _ = store.applyRuntimeSnapshot(
            sessionID: sessionID,
            snapshot: AIRuntimeContextSnapshot(
                tool: tool,
                externalSessionID: externalSessionID,
                model: "gpt-5.4",
                inputTokens: totalTokens,
                outputTokens: 0,
                totalTokens: totalTokens,
                updatedAt: updatedAt,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                source: .hook
            )
        )
    }

    private func displaySnapshot(projectID: UUID, sessionID: UUID) -> AITerminalSessionSnapshot? {
        store.liveDisplaySnapshots(projectID: projectID).first { $0.sessionID == sessionID }
    }

    private func indexedSummary(
        sessionID: UUID,
        projectID: UUID,
        tool: String,
        externalSessionID: String,
        totalTokens: Int
    ) -> AISessionSummary {
        AISessionSummary(
            sessionID: sessionID,
            externalSessionID: externalSessionID,
            projectID: projectID,
            projectName: "codux",
            sessionTitle: "Restored",
            firstSeenAt: Date(timeIntervalSince1970: 1),
            lastSeenAt: Date(timeIntervalSince1970: 2),
            lastTool: tool,
            lastModel: "gpt-5.4",
            requestCount: 10,
            totalInputTokens: totalTokens,
            totalOutputTokens: 0,
            totalTokens: totalTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 10,
            todayTokens: 0
        )
    }
}
