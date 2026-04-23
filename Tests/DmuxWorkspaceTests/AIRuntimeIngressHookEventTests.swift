import XCTest
@testable import DmuxWorkspace

@MainActor
final class AIRuntimeIngressHookEventTests: XCTestCase {
    private let ingress = AIRuntimeIngressService.shared
    private let store = AISessionStore.shared

    override func setUp() async throws {
        store.reset()
        ingress.resetEphemeralState()
    }

    override func tearDown() async throws {
        store.reset()
        ingress.resetEphemeralState()
    }

    func testClaudeSnapshotTracksLatestUsageForRepeatedMessageID() async throws {
        let projectPath = "/tmp/dmux-claude-runtime-\(UUID().uuidString)"
        let sessionID = UUID().uuidString.lowercased()
        let logURL = AIRuntimeSourceLocator.claudeSessionLogURL(projectPath: projectPath, externalSessionID: sessionID)
        try FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: logURL.deletingLastPathComponent()) }

        let firstRow = """
        {"cwd":"\(projectPath)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:00:44.548Z","type":"assistant","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":3,"cache_creation_input_tokens":19077,"cache_read_input_tokens":8151,"output_tokens":1}}}
        """
        let secondRow = """
        {"cwd":"\(projectPath)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:00:47.562Z","type":"assistant","uuid":"row-2","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":3,"cache_creation_input_tokens":19077,"cache_read_input_tokens":8151,"output_tokens":228}}}
        """
        try "\(firstRow)\n\(secondRow)\n".write(to: logURL, atomically: true, encoding: .utf8)

        let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
            projectPath: projectPath,
            externalSessionID: sessionID
        )

        XCTAssertEqual(snapshot?.model, "claude-sonnet-4-6")
        XCTAssertEqual(snapshot?.totalTokens, 231)
        XCTAssertEqual(snapshot?.cachedInputTokens, 8_151)
        XCTAssertEqual(snapshot?.outputTokens, 228)
    }

    func testClaudeSnapshotFindsUsageWhenProjectDirectoryNameIsSanitizedDifferently() async throws {
        let homeURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-claude-runtime-fallback-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: homeURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: homeURL) }

        let projectPath = "/Volumes/Web/未命名文件夹"
        let sessionID = UUID().uuidString.lowercased()
        let actualDirectory = homeURL
            .appendingPathComponent(".claude/projects", isDirectory: true)
            .appendingPathComponent("-Volumes-Web-------", isDirectory: true)
        try FileManager.default.createDirectory(at: actualDirectory, withIntermediateDirectories: true)

        let logURL = actualDirectory.appendingPathComponent("\(sessionID).jsonl", isDirectory: false)
        let row = """
        {"cwd":"\(projectPath)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:47.562Z","type":"assistant","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":40},"stop_reason":"end_turn"}}
        """
        try "\(row)\n".write(to: logURL, atomically: true, encoding: .utf8)

        let urls = AIRuntimeSourceLocator.claudeProjectLogURLs(projectPath: projectPath, homeURL: homeURL)
        XCTAssertEqual(
            urls.map { $0.resolvingSymlinksInPath().standardizedFileURL },
            [logURL.resolvingSymlinksInPath().standardizedFileURL]
        )
    }

    func testClaudeTurnCompletedUsesCurrentSessionFallbackWithoutSnapshotScan() async throws {
        let projectPath = "/tmp/dmux-claude-driver-\(UUID().uuidString)"
        let sessionID = UUID().uuidString.lowercased()
        let resolved = await ClaudeToolDriver().resolveHookEvent(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: sessionID,
                model: nil,
                totalTokens: nil,
                updatedAt: 200,
                metadata: .init(transcriptPath: nil, notificationType: nil, reason: nil)
            ),
            currentSession: AISessionStore.TerminalSessionState(
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: sessionID,
                state: .responding,
                model: "claude-sonnet-4-6",
                baselineTotalTokens: 0,
                committedTotalTokens: 321,
                updatedAt: 199,
                startedAt: 198,
                wasInterrupted: false,
                hasCompletedTurn: false,
                transcriptPath: nil,
                notificationType: nil,
                targetToolName: nil,
                interactionMessage: nil
            )
        )

        XCTAssertEqual(resolved.model, "claude-sonnet-4-6")
        XCTAssertEqual(resolved.totalTokens, 321)
        XCTAssertEqual(resolved.metadata?.wasInterrupted, false)
        XCTAssertEqual(resolved.metadata?.hasCompletedTurn, true)
    }

    func testClaudeTurnCompletedUsesRuntimeLogSnapshotWhenAvailable() async throws {
        let projectPath = "/tmp/dmux-claude-driver-snapshot-\(UUID().uuidString)"
        let sessionID = UUID().uuidString.lowercased()
        let logURL = AIRuntimeSourceLocator.claudeSessionLogURL(projectPath: projectPath, externalSessionID: sessionID)
        try FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: logURL.deletingLastPathComponent()) }

        let row = """
        {"cwd":"\(projectPath)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:47.562Z","type":"assistant","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":40},"stop_reason":"end_turn"}}
        """
        let stopSummary = """
        {"cwd":"\(projectPath)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:48.000Z","type":"system","subtype":"stop_hook_summary"}
        """
        try "\(row)\n\(stopSummary)\n".write(to: logURL, atomically: true, encoding: .utf8)

        let resolved = await ClaudeToolDriver().resolveHookEvent(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: sessionID,
                model: nil,
                totalTokens: nil,
                updatedAt: 200,
                metadata: .init(transcriptPath: nil, notificationType: nil, reason: nil)
            ),
            currentSession: AISessionStore.TerminalSessionState(
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: sessionID,
                state: .responding,
                model: "claude-sonnet-4-6",
                baselineTotalTokens: 0,
                committedTotalTokens: 0,
                updatedAt: 199,
                startedAt: 198,
                wasInterrupted: false,
                hasCompletedTurn: false,
                transcriptPath: nil,
                notificationType: nil,
                targetToolName: nil,
                interactionMessage: nil
            )
        )

        XCTAssertEqual(resolved.model, "claude-sonnet-4-6")
        XCTAssertEqual(resolved.totalTokens, 50)
        XCTAssertEqual(resolved.cachedInputTokens, 30)
    }

    func testClaudePromptSubmittedDoesNotWaitForSnapshotTotals() async throws {
        let projectPath = "/tmp/dmux-claude-driver-fast-\(UUID().uuidString)"
        let sessionID = UUID().uuidString.lowercased()
        let logURL = AIRuntimeSourceLocator.claudeSessionLogURL(projectPath: projectPath, externalSessionID: sessionID)
        try FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: logURL.deletingLastPathComponent()) }

        let row = """
        {"cwd":"\(projectPath)","sessionId":"\(sessionID)","timestamp":"2026-04-21T03:10:47.562Z","type":"assistant","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":40},"stop_reason":"end_turn"}}
        """
        try "\(row)\n".write(to: logURL, atomically: true, encoding: .utf8)

        let resolved = await ClaudeToolDriver().resolveHookEvent(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: sessionID,
                model: nil,
                totalTokens: nil,
                updatedAt: 200,
                metadata: nil
            ),
            currentSession: AISessionStore.TerminalSessionState(
                terminalID: UUID(),
                terminalInstanceID: "instance-1",
                projectID: UUID(),
                projectName: "Codux",
                projectPath: projectPath,
                sessionTitle: "Claude",
                tool: "claude",
                aiSessionID: sessionID,
                state: .idle,
                model: "claude-sonnet-4-6",
                baselineTotalTokens: 0,
                committedTotalTokens: 321,
                updatedAt: 199,
                startedAt: 198,
                wasInterrupted: false,
                hasCompletedTurn: false,
                transcriptPath: nil,
                notificationType: nil,
                targetToolName: nil,
                interactionMessage: nil
            )
        )

        XCTAssertEqual(resolved.model, "claude-sonnet-4-6")
        XCTAssertEqual(resolved.totalTokens, 321)
    }

    func testAIHookPromptSubmittedUpdatesSessionStore() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        let payload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-thread",
                model: "gpt-5.4",
                totalTokens: 10,
                updatedAt: 100,
                metadata: nil
            )
        )

        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: payload)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .responding)
        XCTAssertEqual(session.tool, "codex")
        XCTAssertEqual(session.aiSessionID, "codex-thread")
        XCTAssertEqual(session.model, "gpt-5.4")
    }

    func testAIHookTurnCompletedTransitionsSessionToIdle() async throws {
        let terminalID = UUID()
        let projectID = UUID()

        let promptPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-thread",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: 100,
                metadata: nil
            )
        )
        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: promptPayload)

        let stopPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: terminalID,
                terminalInstanceID: "instance-1",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-thread",
                model: nil,
                totalTokens: 44,
                updatedAt: 101,
                metadata: .init(transcriptPath: nil, notificationType: nil, reason: nil)
            )
        )
        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: stopPayload)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertEqual(session.committedTotalTokens, 44)
        guard case .completed(let tool, _, let exitCode) = store.projectPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "codex")
        XCTAssertNil(exitCode)
    }

    func testCodexStopHookUsesTranscriptInterruptedState() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        let transcriptURL = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("dmux-codex-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: transcriptURL) }

        let rows = [
            #"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
            #"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713668401}}"#,
            #"{"timestamp":"2026-04-21T03:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":77}}}}"#,
            #"{"timestamp":"2026-04-21T03:00:03Z","type":"event_msg","payload":{"type":"turn_aborted","completed_at":1713668403}}"#
        ]
        try rows.joined(separator: "\n").appending("\n").write(to: transcriptURL, atomically: true, encoding: .utf8)

        let promptPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-codex",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/codex-project",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-thread",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: 100,
                metadata: nil
            )
        )
        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: promptPayload)

        let stopPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: terminalID,
                terminalInstanceID: "instance-codex",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/codex-project",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-thread",
                model: nil,
                totalTokens: nil,
                updatedAt: 101,
                metadata: .init(transcriptPath: transcriptURL.path)
            )
        )
        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: stopPayload)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.wasInterrupted)
        XCTAssertFalse(session.hasCompletedTurn)
        XCTAssertEqual(session.committedTotalTokens, 77)
        XCTAssertEqual(session.model, "gpt-5.4")
    }

    func testAIHookNeedsInputPreservesInteractionMetadata() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        let payload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .needsInput,
                terminalID: terminalID,
                terminalInstanceID: "instance-2",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-thread",
                model: "sonnet",
                totalTokens: nil,
                updatedAt: 200,
                metadata: .init(
                    transcriptPath: nil,
                    notificationType: "permission-request",
                    reason: "tool-approval",
                    targetToolName: "Bash",
                    message: "Approve Bash?"
                )
            )
        )

        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: payload)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .needsInput)
        XCTAssertEqual(session.notificationType, "permission-request")
        XCTAssertEqual(session.targetToolName, "Bash")
        XCTAssertEqual(session.interactionMessage, "Approve Bash?")
        XCTAssertEqual(store.projectPhase(projectID: projectID), .waitingInput(tool: "claude"))
    }

    func testManualInterruptTransitionsRunningSessionToIdleInterrupted() async throws {
        let terminalID = UUID()
        let projectID = UUID()

        let promptPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-3",
                projectID: projectID,
                projectName: "Codux",
                sessionTitle: "Terminal",
                tool: "codex",
                aiSessionID: "codex-thread",
                model: "gpt-5.4",
                totalTokens: 12,
                updatedAt: 300,
                metadata: nil
            )
        )
        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: promptPayload)

        let interruptPayload = try JSONEncoder().encode(
            AIManualInterruptEvent(
                terminalID: terminalID,
                updatedAt: 301
            )
        )
        await ingress.ingestManagedRuntimeSocketEventForTesting(kind: "manual-interrupt", payloadData: interruptPayload)

        let session = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(session.state, .idle)
        XCTAssertTrue(session.wasInterrupted)
        XCTAssertEqual(store.projectPhase(projectID: projectID), .idle)
    }

    func testClaudeTurnCompletedAppliesImmediatelyThenBackfillsRuntimeTokens() async throws {
        let terminalID = UUID()
        let projectID = UUID()
        let driver = DeferredClaudeBackfillDriver(
            snapshot: AIRuntimeContextSnapshot(
                tool: "claude",
                externalSessionID: "claude-thread",
                model: "claude-sonnet-4-6",
                inputTokens: 200,
                outputTokens: 98,
                cachedInputTokens: 0,
                totalTokens: 298,
                updatedAt: 102,
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                sessionOrigin: .unknown,
                source: .probe
            ),
            nilSnapshotCount: 2
        )
        let testIngress = AIRuntimeIngressService(
            aiSessionStore: store,
            toolDriverFactory: AIToolDriverFactory(drivers: [driver])
        )

        let promptPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .promptSubmitted,
                terminalID: terminalID,
                terminalInstanceID: "instance-claude",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/claude-project",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-thread",
                model: "claude-sonnet-4-6",
                totalTokens: 0,
                updatedAt: 100,
                metadata: nil
            )
        )
        await testIngress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: promptPayload)

        let stopPayload = try JSONEncoder().encode(
            AIHookEvent(
                kind: .turnCompleted,
                terminalID: terminalID,
                terminalInstanceID: "instance-claude",
                projectID: projectID,
                projectName: "Codux",
                projectPath: "/tmp/claude-project",
                sessionTitle: "Terminal",
                tool: "claude",
                aiSessionID: "claude-thread",
                model: nil,
                totalTokens: nil,
                updatedAt: 101,
                metadata: .init(reason: "end_turn")
            )
        )
        await testIngress.ingestManagedRuntimeSocketEventForTesting(kind: "ai-hook", payloadData: stopPayload)

        let immediateSession = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(immediateSession.state, .idle)
        XCTAssertEqual(immediateSession.committedTotalTokens, 0)
        XCTAssertTrue(immediateSession.hasCompletedTurn)
        guard case .completed(let tool, _, _) = store.projectPhase(projectID: projectID) else {
            return XCTFail("expected completed project phase")
        }
        XCTAssertEqual(tool, "claude")

        for _ in 1...10 {
            try await Task.sleep(for: .milliseconds(350))
            if store.session(for: terminalID)?.committedTotalTokens == 298 {
                break
            }
        }

        let backfilledSession = try XCTUnwrap(store.session(for: terminalID))
        XCTAssertEqual(backfilledSession.committedTotalTokens, 298)
        let snapshotCalls = await driver.snapshotCallCount()
        XCTAssertGreaterThanOrEqual(snapshotCalls, 3)
    }
}

private actor DeferredClaudeBackfillCounter {
    private var calls = 0

    func nextCall() -> Int {
        calls += 1
        return calls
    }

    func current() -> Int {
        calls
    }
}

private struct DeferredClaudeBackfillDriver: AIToolDriver {
    let id = "claude"
    let aliases: Set<String> = ["claude"]
    let isRealtimeTool = true
    let snapshot: AIRuntimeContextSnapshot
    let nilSnapshotCount: Int
    private let counter = DeferredClaudeBackfillCounter()

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        var resolved = event
        resolved.model = resolved.model ?? currentSession?.model
        if resolved.kind == .turnCompleted {
            resolved.totalTokens = resolved.totalTokens ?? currentSession?.committedTotalTokens
            var metadata = resolved.metadata ?? AIHookEventMetadata()
            metadata.wasInterrupted = false
            metadata.hasCompletedTurn = true
            resolved.metadata = metadata
        }
        return resolved
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        _ = session
        let call = await counter.nextCall()
        return call <= nilSnapshotCount ? nil : snapshot
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
