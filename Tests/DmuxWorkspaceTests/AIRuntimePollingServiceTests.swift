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

    func testPollingUpdatesRuntimeTokensWithoutChangingPhase() async throws {
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
                        updatedAt: 110,
                        responseState: .idle,
                        wasInterrupted: false,
                        hasCompletedTurn: true,
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
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.baselineTotalTokens, 12)
        XCTAssertEqual(session.committedTotalTokens, 150)
        XCTAssertEqual(session.model, "claude-sonnet-4-6")
    }

    func testRecentHookSuppressesImmediatePoll() async throws {
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
        defer { service.stop() }

        service.noteHookApplied(for: terminalID, reason: "turnCompleted")
        service.sync(reason: "ai-hook")
        try await Task.sleep(for: .milliseconds(150))

        let suppressedCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(suppressedCallCount, 0)
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.committedTotalTokens, 12)
    }

    func testPollingResumesAfterHookSuppressionWindowExpires() async throws {
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
            hookSuppressionWindow: 0.05
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

        service.noteHookApplied(for: terminalID, reason: "turnCompleted")
        service.sync(reason: "ai-hook")
        try await Task.sleep(for: .milliseconds(120))
        service.sync(reason: "post-window")

        await fulfillment(of: [expectation], timeout: 2)
        let resumedCallCount = await driver.snapshotCallCount()
        XCTAssertEqual(resumedCallCount, 1)
        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.committedTotalTokens, 150)
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
