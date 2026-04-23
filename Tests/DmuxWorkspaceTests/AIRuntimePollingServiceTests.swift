import Foundation
import XCTest
@testable import DmuxWorkspace

@MainActor
final class AIRuntimePollingServiceTests: XCTestCase {
    private let store = AISessionStore.shared

    override func setUp() async throws {
        store.reset()
    }

    override func tearDown() async throws {
        store.reset()
    }

    func testPollingClearsLoadingWhenRuntimeReturnsIdleWithoutCompletion() async throws {
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
                projectPath: "/tmp/codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 12,
                updatedAt: now,
                metadata: nil
            )
        )

        _ = store.applyRuntimeSnapshot(
            terminalID: terminalID,
            snapshot: AIRuntimeContextSnapshot(
                tool: "claude",
                externalSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                inputTokens: 100,
                outputTokens: 20,
                totalTokens: 120,
                updatedAt: now + 1,
                startedAt: now + 1,
                responseState: .responding,
                wasInterrupted: false,
                hasCompletedTurn: false,
                sessionOrigin: .unknown,
                source: .probe
            )
        )

        let notificationCenter = NotificationCenter()
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [
                MockRuntimeToolDriver(
                    id: "claude",
                    aliases: ["claude"],
                    snapshot: AIRuntimeContextSnapshot(
                        tool: "claude",
                        externalSessionID: "claude-session",
                        model: "claude-sonnet-4-6",
                        inputTokens: 120,
                        outputTokens: 30,
                        totalTokens: 150,
                        updatedAt: now + 2,
                        responseState: .idle,
                        wasInterrupted: false,
                        hasCompletedTurn: false,
                        sessionOrigin: .unknown,
                        source: .probe
                    )
                )
            ]),
            notificationCenter: notificationCenter,
            interval: 60
        )

        let expectation = expectation(description: "runtime poll notification")
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { _ in
            expectation.fulfill()
        }
        defer {
            notificationCenter.removeObserver(observer)
            service.stop()
        }

        service.sync(reason: "test")
        await fulfillment(of: [expectation], timeout: 2)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedTotalTokens, 150)
        XCTAssertEqual(session.model, "claude-sonnet-4-6")
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testRecentHookStillAllowsImmediatePoll() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        _ = store.apply(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 12,
                updatedAt: 100,
                metadata: nil
            )
        )

        let notificationCenter = NotificationCenter()
        let driver = CountingRuntimeToolDriver(
            id: "claude",
            aliases: ["claude"],
            snapshot: AIRuntimeContextSnapshot(
                tool: "claude",
                externalSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                inputTokens: 120,
                outputTokens: 30,
                totalTokens: 150,
                updatedAt: 110,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                sessionOrigin: .unknown,
                source: .probe
            )
        )
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [driver]),
            notificationCenter: notificationCenter,
            interval: 60,
            hookSuppressionWindow: 2
        )
        let expectation = expectation(description: "runtime poll notification")
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { note in
            if (note.userInfo?["kind"] as? String) == "runtime-poll" {
                expectation.fulfill()
            }
        }
        defer {
            notificationCenter.removeObserver(observer)
            service.stop()
        }

        service.noteHookApplied(for: terminalID, reason: "promptSubmitted")
        service.sync(reason: "ai-hook")

        await fulfillment(of: [expectation], timeout: 2)

        let snapshotCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(snapshotCallCount, 1)
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.committedTotalTokens, 150)
    }

    func testStalePollStartedBeforeHookIsDropped() async throws {
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
                projectPath: "/tmp/codux",
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                totalTokens: 12,
                updatedAt: now,
                metadata: nil
            )
        )

        let notificationCenter = NotificationCenter()
        let driver = DelayedRuntimeToolDriver(
            id: "claude",
            aliases: ["claude"],
            snapshot: AIRuntimeContextSnapshot(
                tool: "claude",
                externalSessionID: "claude-session",
                model: "claude-sonnet-4-6",
                inputTokens: 120,
                outputTokens: 30,
                totalTokens: 150,
                updatedAt: now + 1,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                sessionOrigin: .unknown,
                source: .probe
            ),
            delayNanoseconds: 200_000_000
        )
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [driver]),
            notificationCenter: notificationCenter,
            interval: 60,
            hookSuppressionWindow: 0.05
        )
        defer { service.stop() }

        let invertedExpectation = expectation(description: "runtime poll notification")
        invertedExpectation.isInverted = true
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { note in
            if (note.userInfo?["kind"] as? String) == "runtime-poll" {
                invertedExpectation.fulfill()
            }
        }
        defer { notificationCenter.removeObserver(observer) }

        service.sync(reason: "before-hook")
        try await Task.sleep(for: .milliseconds(40))
        service.noteHookApplied(for: terminalID, reason: "promptSubmitted")

        await fulfillment(of: [invertedExpectation], timeout: 0.4)

        let snapshotCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(snapshotCallCount, 1)
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.committedTotalTokens, 0)
    }

    func testRespondingSessionPollsAfterSuppressionEvenWhenHookJustUpdatedTimestamp() async throws {
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
                projectPath: "/tmp/codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: now,
                metadata: nil
            )
        )

        let notificationCenter = NotificationCenter()
        let driver = CountingRuntimeToolDriver(
            id: "codex",
            aliases: ["codex"],
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: "codex-session",
                model: "gpt-5.4",
                inputTokens: 120,
                outputTokens: 30,
                totalTokens: 150,
                updatedAt: now + 1,
                responseState: .responding,
                wasInterrupted: false,
                hasCompletedTurn: false,
                sessionOrigin: .unknown,
                source: .probe
            )
        )
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [driver]),
            notificationCenter: notificationCenter,
            interval: 60,
            hookSuppressionWindow: 0.05,
            sessionSilenceThreshold: 18
        )
        defer { service.stop() }

        let expectation = expectation(description: "runtime poll notification")
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { note in
            if (note.userInfo?["kind"] as? String) == "runtime-poll" {
                expectation.fulfill()
            }
        }
        defer { notificationCenter.removeObserver(observer) }

        service.noteHookApplied(for: terminalID, reason: "promptSubmitted")
        try await Task.sleep(for: .milliseconds(120))
        service.sync(reason: "post-hook")

        await fulfillment(of: [expectation], timeout: 2)
        let snapshotCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(snapshotCallCount, 1)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.baselineTotalTokens, 0)
        XCTAssertEqual(session.committedTotalTokens, 150)
    }

    func testIdleIncompleteSessionStillPollsAndCanEnterLoadingFromRuntime() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        let now = Date().timeIntervalSince1970

        _ = store.apply(
            AIHookEvent(
                kind: .sessionStarted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 0,
                updatedAt: now,
                metadata: nil
            )
        )

        let notificationCenter = NotificationCenter()
        let driver = CountingRuntimeToolDriver(
            id: "codex",
            aliases: ["codex"],
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: "codex-session",
                model: "gpt-5.4",
                inputTokens: 40,
                outputTokens: 10,
                totalTokens: 50,
                updatedAt: now + 1,
                responseState: .responding,
                wasInterrupted: false,
                hasCompletedTurn: false,
                sessionOrigin: .unknown,
                source: .probe
            )
        )
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [driver]),
            notificationCenter: notificationCenter,
            interval: 60,
            sessionSilenceThreshold: 18
        )
        defer { service.stop() }

        let expectation = expectation(description: "runtime poll notification")
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { note in
            if (note.userInfo?["kind"] as? String) == "runtime-poll" {
                expectation.fulfill()
            }
        }
        defer { notificationCenter.removeObserver(observer) }

        service.sync(reason: "idle-probe")
        await fulfillment(of: [expectation], timeout: 2)

        let snapshotCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(snapshotCallCount, 1)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertFalse(session.wasInterrupted)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .running(tool: "codex"))
    }

    func testIdleSessionPollsImmediatelyAfterTurnCompletedHook() async throws {
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
                projectPath: "/tmp/codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: nil,
                updatedAt: now,
                metadata: AIHookEventMetadata(hasCompletedTurn: true)
            )
        )

        let notificationCenter = NotificationCenter()
        let driver = CountingRuntimeToolDriver(
            id: "codex",
            aliases: ["codex"],
            snapshot: AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: "codex-session",
                model: "gpt-5.4",
                inputTokens: 120,
                outputTokens: 30,
                totalTokens: 150,
                updatedAt: now + 0.2,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                sessionOrigin: .unknown,
                source: .probe
            )
        )
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [driver]),
            notificationCenter: notificationCenter,
            interval: 60,
            hookSuppressionWindow: 2,
            sessionSilenceThreshold: 18
        )

        let expectation = expectation(description: "runtime poll notification")
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { note in
            if (note.userInfo?["kind"] as? String) == "runtime-poll" {
                expectation.fulfill()
            }
        }
        defer {
            notificationCenter.removeObserver(observer)
            service.stop()
        }

        service.noteHookApplied(for: terminalID, reason: "turnCompleted")
        service.sync(reason: "ai-hook")

        await fulfillment(of: [expectation], timeout: 2)

        let snapshotCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(snapshotCallCount, 1)
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.hasCompletedTurn)
        XCTAssertEqual(session.committedTotalTokens, 150)
    }

    func testRuntimeSnapshotInterruptedClearsRespondingPhase() async throws {
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
                projectPath: "/tmp/codux",
                sessionTitle: "Codex",
                tool: "codex",
                aiSessionID: "codex-session",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: now,
                metadata: nil
            )
        )

        let notificationCenter = NotificationCenter()
        let service = AIRuntimePollingService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [
                MockRuntimeToolDriver(
                    id: "codex",
                    aliases: ["codex"],
                    snapshot: AIRuntimeContextSnapshot(
                        tool: "codex",
                        externalSessionID: "codex-session",
                        model: "gpt-5.4",
                        inputTokens: 12,
                        outputTokens: 0,
                        totalTokens: 12,
                        updatedAt: now + 1,
                        responseState: .idle,
                        wasInterrupted: true,
                        hasCompletedTurn: false,
                        sessionOrigin: .unknown,
                        source: .probe
                    )
                )
            ]),
            notificationCenter: notificationCenter,
            interval: 60
        )

        let expectation = expectation(description: "runtime poll notification")
        let observer = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { _ in
            expectation.fulfill()
        }
        defer {
            notificationCenter.removeObserver(observer)
            service.stop()
        }

        service.sync(reason: "test")
        await fulfillment(of: [expectation], timeout: 2)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.wasInterrupted)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }
}

private struct MockRuntimeToolDriver: AIToolDriver {
    let id: String
    let aliases: Set<String>
    let snapshot: AIRuntimeContextSnapshot
    let isRealtimeTool = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        _ = currentSession
        return event
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        _ = session
        return snapshot
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return .none
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        _ = session
        return nil
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        _ = session
        _ = title
    }

    func removeSession(_ session: AISessionSummary) throws {
        _ = session
    }
}

private actor CountingSnapshotCounter {
    private var value = 0

    func increment() {
        value += 1
    }

    func current() -> Int {
        value
    }
}

private struct CountingRuntimeToolDriver: AIToolDriver {
    let id: String
    let aliases: Set<String>
    let snapshot: AIRuntimeContextSnapshot
    let isRealtimeTool = true
    private let counter = CountingSnapshotCounter()

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        _ = currentSession
        return event
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        _ = session
        await counter.increment()
        return snapshot
    }

    func snapshotCallCount() async -> Int {
        await counter.current()
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return .none
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        _ = session
        return nil
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        _ = session
        _ = title
    }

    func removeSession(_ session: AISessionSummary) throws {
        _ = session
    }
}

private struct DelayedRuntimeToolDriver: AIToolDriver {
    let id: String
    let aliases: Set<String>
    let snapshot: AIRuntimeContextSnapshot
    let delayNanoseconds: UInt64
    let isRealtimeTool = true
    private let counter = CountingSnapshotCounter()

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        _ = currentSession
        return event
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        _ = session
        await counter.increment()
        try? await Task.sleep(nanoseconds: delayNanoseconds)
        return snapshot
    }

    func snapshotCallCount() async -> Int {
        await counter.current()
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return .none
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        _ = session
        return nil
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        _ = session
        _ = title
    }

    func removeSession(_ session: AISessionSummary) throws {
        _ = session
    }
}
