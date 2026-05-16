import XCTest
@testable import DmuxWorkspace

@MainActor
final class AgentSessionTests: XCTestCase {
    func testTerminalSessionDecodesLegacyPayloadAsTerminalMode() throws {
        let projectID = UUID()
        let sessionID = UUID()
        let data = """
        {
          "id": "\(sessionID.uuidString)",
          "projectID": "\(projectID.uuidString)",
          "projectName": "Project",
          "title": "Terminal",
          "cwd": "/tmp/project",
          "shell": "/bin/zsh",
          "command": "/bin/zsh",
          "previewLines": []
        }
        """.data(using: .utf8)!

        let session = try JSONDecoder().decode(TerminalSession.self, from: data)

        XCTAssertEqual(session.launchMode, .terminal)
        XCTAssertNil(session.agentTool)
    }

    func testAgentSessionFactoryMarksSessionAsAgent() {
        let project = makeProject()

        let session = TerminalSession.makeAgent(project: project, tool: .claude)

        XCTAssertEqual(session.launchMode, .agent)
        XCTAssertEqual(session.agentTool, AgentToolKind.claude.rawValue)
        XCTAssertEqual(session.command, "")
        XCTAssertEqual(session.tabTitle, "Claude Code")
    }

    func testAgentDriverFactoryResolvesBuiltInDrivers() throws {
        let factory = AgentDriverFactory.shared

        XCTAssertEqual(try factory.driver(for: .codex).tool, .codex)
        XCTAssertEqual(try factory.driver(for: .claude).tool, .claude)
        XCTAssertEqual(try factory.driver(for: .opencode).tool, .opencode)
    }

    func testAgentDriversUseStructuredTransportsWithoutPTY() throws {
        let request = AgentDriverRequest(
            tool: .codex,
            prompt: "Summarize this project.",
            cwd: "/tmp/project-agent",
            model: nil,
            reasoningEffort: "minimal",
            fullAccess: false,
            externalSessionID: nil
        )

        let codex = CodexAgentDriver()
        let claude = ClaudeAgentDriver()
        let opencode = OpenCodeAgentDriver()

        XCTAssertEqual(codex.transport, .codexAppServerJSONRPC)
        XCTAssertEqual(claude.transport, .claudeStreamJSON)
        XCTAssertEqual(opencode.transport, .openCodeACP)

        let codexInvocation = codex.invocation(for: request)
        XCTAssertEqual(Array(codexInvocation.arguments.suffix(3)), ["app-server", "--listen", "stdio://"])
        XCTAssertTrue(claude.invocation(for: request).arguments.contains("--output-format=stream-json"))
        XCTAssertEqual(opencode.invocation(for: request).arguments, ["opencode", "acp", "--cwd", "/tmp/project-agent"])
    }

    func testCodexReasoningEffortDecodesWithLegacySettings() throws {
        let legacyData = """
        {
          "codex": "fullAccess",
          "claudeCode": "default",
          "opencode": "default",
          "codexModel": "gpt-5.5"
        }
        """.data(using: .utf8)!

        let settings = try JSONDecoder().decode(AppAIToolPermissionSettings.self, from: legacyData)

        XCTAssertEqual(settings.codex, .fullAccess)
        XCTAssertEqual(settings.codexModel, "gpt-5.5")
        XCTAssertEqual(settings.codexEffort, .medium)

        let encoded = try JSONEncoder().encode(settings)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        XCTAssertEqual(payload["codexEffort"] as? String, "medium")
    }

    func testAppSettingsDefaultAgentSplitExperimentIsDisabled() throws {
        let settings = try JSONDecoder().decode(AppSettings.self, from: Data("{}".utf8))

        XCTAssertFalse(settings.experiments.agentSplitEnabled)
    }

    func testAppSettingsPersistsAgentSplitExperiment() throws {
        var settings = AppSettings()
        settings.experiments.agentSplitEnabled = true

        let encoded = try JSONEncoder().encode(settings)
        let decoded = try JSONDecoder().decode(AppSettings.self, from: encoded)

        XCTAssertTrue(decoded.experiments.agentSplitEnabled)
    }

    func testCodexAppServerSmokeWhenEnabled() async throws {
        guard ProcessInfo.processInfo.environment["CODEX_APP_SERVER_SMOKE"] == "1" else {
            throw XCTSkip("Set CODEX_APP_SERVER_SMOKE=1 to run the real Codex app-server JSON-RPC smoke test.")
        }

        let collector = AgentSmokeCollector()
        let request = AgentDriverRequest(
            tool: .codex,
            prompt: "Reply with exactly: codux-smoke-ok",
            cwd: FileManager.default.currentDirectoryPath,
            model: nil,
            reasoningEffort: "minimal",
            fullAccess: false,
            externalSessionID: nil
        )

        try await withAgentTestTimeout(seconds: 90) {
            try await CodexAgentDriver().run(request: request) { event in
                await collector.record(event)
            }
        }

        let snapshot = await collector.snapshot()
        XCTAssertTrue(snapshot.completed, "Codex app-server did not emit completion.")
        XCTAssertTrue(snapshot.assistant.contains("codux-smoke-ok"), "Assistant response was: \(snapshot.assistant)")
    }

    func testRawCodexAppServerInitializeWhenEnabled() throws {
        guard ProcessInfo.processInfo.environment["CODEX_APP_SERVER_SMOKE"] == "1" else {
            throw XCTSkip("Set CODEX_APP_SERVER_SMOKE=1 to run the real Codex app-server JSON-RPC smoke test.")
        }

        let invocation = CodexAgentDriver().invocation(
            for: AgentDriverRequest(
                tool: .codex,
                prompt: "probe",
                cwd: FileManager.default.currentDirectoryPath,
                model: nil,
                reasoningEffort: "minimal",
                fullAccess: false,
                externalSessionID: nil
            )
        )
        let process = Process()
        process.executableURL = URL(fileURLWithPath: invocation.executablePath)
        process.arguments = invocation.arguments
        process.currentDirectoryURL = URL(fileURLWithPath: invocation.currentDirectory, isDirectory: true)
        process.environment = invocation.environment

        let stdin = Pipe()
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()
        let payload = """
        {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"codux","version":"dev"},"capabilities":{}}}

        """
        stdin.fileHandleForWriting.write(Data(payload.utf8))

        let output = blockingReadAvailableData(stdout.fileHandleForReading, timeout: 15)
        if process.isRunning {
            process.terminate()
        }
        process.waitUntilExit()

        let stderrText = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let text = String(data: output, encoding: .utf8) ?? ""
        XCTAssertTrue(
            text.contains("\"id\":1"),
            "executable: \(invocation.executablePath), args: \(invocation.arguments), status: \(process.terminationStatus), stdout: \(text), stderr: \(stderrText)"
        )
    }

    func testAgentMessageDeltasDoNotDuplicateSnapshotPayloads() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "Hello"),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "Hello world"),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "Hello world"),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "!"),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.messages.map(\.content), ["Hello world!"])
    }

    func testStreamingAgentMessagesPreserveNewlineChunks() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .opencode)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "First line"),
            sessionID: session.id,
            tool: .opencode
        )
        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "\n"),
            sessionID: session.id,
            tool: .opencode
        )
        model.applyAgentDriverEventForTesting(
            .message(role: .assistant, content: "Second line"),
            sessionID: session.id,
            tool: .opencode
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.messages.map(\.content), ["First line\nSecond line"])
        XCTAssertEqual(state.timelineItems.map(\.content), ["First line\nSecond line"])
    }

    func testStreamingTimelineDeltasPreserveNewlineChunks() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventsForTesting(
            [
                .timelineDelta(
                    AgentTimelineDelta(
                        id: "turn-1#msg-1",
                        turnID: "turn-1",
                        itemID: "msg-1",
                        kind: .assistantMessage,
                        role: .assistant,
                        title: nil,
                        detail: nil,
                        delta: "First line",
                        status: .running
                    )
                ),
                .timelineDelta(
                    AgentTimelineDelta(
                        id: "turn-1#msg-1",
                        turnID: "turn-1",
                        itemID: "msg-1",
                        kind: .assistantMessage,
                        role: .assistant,
                        title: nil,
                        detail: nil,
                        delta: "\n",
                        status: .running
                    )
                ),
                .timelineDelta(
                    AgentTimelineDelta(
                        id: "turn-1#msg-1",
                        turnID: "turn-1",
                        itemID: "msg-1",
                        kind: .assistantMessage,
                        role: .assistant,
                        title: nil,
                        detail: nil,
                        delta: "Second line",
                        status: .running
                    )
                ),
            ],
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.content), ["First line\nSecond line"])
    }

    func testAgentStructuredEventsPopulateTasksAndFileChanges() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .task(
                AgentTaskItem(
                    id: "plan-1",
                    title: "Inspect files",
                    status: .running,
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .task(
                AgentTaskItem(
                    id: "plan-1",
                    title: "Inspect files",
                    status: .completed,
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .fileChange(
                AgentFileChange(
                    path: "Sources/App.swift",
                    status: .modified,
                    summary: "Updated app shell.",
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.tasks.count, 1)
        XCTAssertEqual(state.tasks.first?.status, .completed)
        XCTAssertEqual(state.fileChanges.map(\.path), ["Sources/App.swift"])
        XCTAssertEqual(state.fileChanges.first?.status, .modified)
    }

    func testAgentTimelineDeltasMergeByServerItemID() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "Hello",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: " world",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-2",
                    turnID: "turn-1",
                    itemID: "msg-2",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "Second block",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.content), ["Hello world", "Second block"])
    }

    func testRunningActivityPlaceholderKeepsStreamOrderBeforeAssistantDelta() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .timelineItem(
                AgentTimelineItem(
                    id: "turn-1#cmd-1",
                    turnID: "turn-1",
                    itemID: "cmd-1",
                    kind: .command,
                    role: .tool,
                    title: nil,
                    content: "",
                    detail: nil,
                    status: .running,
                    createdAt: Date(),
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "I am checking it.",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineItem(
                AgentTimelineItem(
                    id: "turn-1#cmd-1",
                    turnID: "turn-1",
                    itemID: "cmd-1",
                    kind: .command,
                    role: .tool,
                    title: "pwd",
                    content: "/tmp/project",
                    detail: nil,
                    status: .completed,
                    createdAt: Date(),
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.id), ["turn-1#cmd-1", "turn-1#msg-1"])
        XCTAssertEqual(state.timelineItems.first?.title, "pwd")
        XCTAssertEqual(state.timelineItems.first?.content, "/tmp/project")
        XCTAssertEqual(state.timelineItems.first?.status, .completed)
    }

    func testCompletedAgentEventSettlesUnfinishedActivityItems() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .timelineItem(
                AgentTimelineItem(
                    id: "turn-1#cmd-1",
                    turnID: "turn-1",
                    itemID: "cmd-1",
                    kind: .command,
                    role: .tool,
                    title: "sleep 1",
                    content: "",
                    detail: nil,
                    status: .running,
                    createdAt: Date(),
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        model.applyAgentDriverEventForTesting(
            .completed(exitCode: 0),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.runState, .idle)
        XCTAssertEqual(state.timelineItems.map(\.status), [.completed])
    }

    func testAgentDriverEventFlushCoalescesStreamingTimelineDeltas() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventsForTesting(
            [
                .timelineDelta(
                    AgentTimelineDelta(
                        id: "turn-1#msg-1",
                        turnID: "turn-1",
                        itemID: "msg-1",
                        kind: .assistantMessage,
                        role: .assistant,
                        title: nil,
                        detail: nil,
                        delta: "Hello",
                        status: .running
                    )
                ),
                .timelineDelta(
                    AgentTimelineDelta(
                        id: "turn-1#msg-1",
                        turnID: "turn-1",
                        itemID: "msg-1",
                        kind: .assistantMessage,
                        role: .assistant,
                        title: nil,
                        detail: nil,
                        delta: " world",
                        status: .running
                    )
                ),
                .status("Thinking"),
            ],
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.content), ["Hello world"])
        XCTAssertEqual(state.timelineItems.first?.status, .running)
        XCTAssertEqual(state.statusText, "Thinking")
    }

    func testCodexCompletedSnapshotFinalizesExistingAssistantTimelineItem() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "你好，",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineItem(
                AgentTimelineItem(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    content: "你好，我在。",
                    detail: nil,
                    status: .completed,
                    createdAt: Date(),
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.content), ["你好，我在。"])
        XCTAssertEqual(state.timelineItems.first?.status, .completed)
    }

    func testCodexCompletedSnapshotReplacesDivergentAssistantStream() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "，我你好在。 要有什么的？ 处理",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineItem(
                AgentTimelineItem(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    content: "你好，我在。有什么要处理的？",
                    detail: nil,
                    status: .completed,
                    createdAt: Date(),
                    updatedAt: Date()
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.content), ["你好，我在。有什么要处理的？"])
        XCTAssertEqual(state.timelineItems.first?.status, .completed)
    }

    func testCodexCumulativeAssistantDeltasReplacePreviousPrefix() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "你好",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )
        model.applyAgentDriverEventForTesting(
            .timelineDelta(
                AgentTimelineDelta(
                    id: "turn-1#msg-1",
                    turnID: "turn-1",
                    itemID: "msg-1",
                    kind: .assistantMessage,
                    role: .assistant,
                    title: nil,
                    detail: nil,
                    delta: "你好，我在。",
                    status: .running
                )
            ),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.timelineItems.map(\.content), ["你好，我在。"])
    }

    func testAgentTurnDiffEventReplacesSessionFileChanges() throws {
        let project = makeProject()
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())

        model.applyAgentDriverEventForTesting(
            .fileChanges([
                AgentFileChange(
                    path: "Sources/App.swift",
                    status: .modified,
                    summary: "diff --git a/Sources/App.swift b/Sources/App.swift",
                    updatedAt: Date()
                ),
                AgentFileChange(
                    path: "Tests/AppTests.swift",
                    status: .added,
                    summary: "diff --git a/Tests/AppTests.swift b/Tests/AppTests.swift",
                    updatedAt: Date()
                ),
            ]),
            sessionID: session.id,
            tool: .codex
        )

        var state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertEqual(state.fileChanges.map(\.path), ["Sources/App.swift", "Tests/AppTests.swift"])

        model.applyAgentDriverEventForTesting(
            .fileChanges([]),
            sessionID: session.id,
            tool: .codex
        )

        state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertTrue(state.fileChanges.isEmpty)
    }

    func testCodexTurnDiffNotificationProducesProtocolFileChanges() async {
        let collector = AgentEventCollector()
        let diff = """
        diff --git a/Sources/App.swift b/Sources/App.swift
        index 123..456
        --- a/Sources/App.swift
        +++ b/Sources/App.swift
        @@
        -old
        +new
        diff --git a/Tests/AppTests.swift b/Tests/AppTests.swift
        new file mode 100644
        index 0000000..789
        --- /dev/null
        +++ b/Tests/AppTests.swift
        @@
        +test
        """
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "turn/diff/updated",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "diff": diff,
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let events = await collector.snapshot()
        guard case .fileChanges(let changes)? = events.first(where: {
            if case .fileChanges = $0 { return true }
            return false
        }) else {
            XCTFail("Expected turn/diff/updated to emit fileChanges.")
            return
        }
        XCTAssertEqual(changes.map(\.path), ["Sources/App.swift", "Tests/AppTests.swift"])
        XCTAssertEqual(changes.map(\.status), [.modified, .added])
        XCTAssertEqual(changes.map(\.additions), [1, 1])
        XCTAssertEqual(changes.map(\.deletions), [1, 0])
        XCTAssertTrue(changes.first?.diff?.contains("-old") == true)
    }

    func testCodexAgentMessageDeltasProduceTimelineByItemID() async {
        let collector = AgentEventCollector()
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "item/agentMessage/delta",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "msg-1",
                    "delta": "Hello",
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let events = await collector.snapshot()
        guard case .timelineDelta(let delta)? = events.first(where: {
            if case .timelineDelta = $0 { return true }
            return false
        }) else {
            XCTFail("Expected agent message delta to emit timelineDelta.")
            return
        }
        XCTAssertEqual(delta.id, "turn-1#msg-1")
        XCTAssertEqual(delta.kind, .assistantMessage)
        XCTAssertEqual(delta.delta, "Hello")
    }

    func testCodexAgentMessageDeltaPreservesNewlineOnlyChunk() async {
        let collector = AgentEventCollector()
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "item/agentMessage/delta",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "msg-1",
                    "delta": "\n",
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let events = await collector.snapshot()
        guard case .timelineDelta(let delta)? = events.first(where: {
            if case .timelineDelta = $0 { return true }
            return false
        }) else {
            XCTFail("Expected newline-only agent message delta to emit timelineDelta.")
            return
        }
        XCTAssertEqual(delta.delta, "\n")
    }

    func testCodexUserMessageItemDoesNotEchoLocalPrompt() async {
        let collector = AgentEventCollector()
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "item/completed",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "item": [
                        "type": "userMessage",
                        "id": "user-1",
                        "text": "你好",
                    ],
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let events = await collector.snapshot()
        XCTAssertFalse(events.contains { event in
            if case .timelineItem(let item) = event, item.kind == .userPrompt {
                return true
            }
            return false
        })
        XCTAssertFalse(events.contains { event in
            if case .message(let role, _) = event, role == .user {
                return true
            }
            return false
        })
    }

    func testCodexItemStartedAndCompletedProduceTimelineItems() async {
        let collector = AgentEventCollector()
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "item/started",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "startedAtMs": 1,
                    "item": [
                        "type": "commandExecution",
                        "id": "cmd-1",
                        "command": "swift test",
                        "cwd": "/tmp/project",
                        "status": "inProgress",
                        "commandActions": [],
                        "aggregatedOutput": NSNull(),
                        "exitCode": NSNull(),
                        "durationMs": NSNull(),
                    ],
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "item/completed",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "completedAtMs": 2,
                    "item": [
                        "type": "commandExecution",
                        "id": "cmd-1",
                        "command": "swift test",
                        "cwd": "/tmp/project",
                        "status": "completed",
                        "commandActions": [],
                        "aggregatedOutput": "ok",
                        "exitCode": 0,
                        "durationMs": 1000,
                    ],
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let items = await collector.snapshot().compactMap { event -> AgentTimelineItem? in
            if case .timelineItem(let item) = event {
                return item
            }
            return nil
        }
        XCTAssertEqual(items.count, 2)
        XCTAssertEqual(items.map(\.id), ["turn-1#cmd-1", "turn-1#cmd-1"])
        XCTAssertEqual(items.last?.kind, .command)
        XCTAssertEqual(items.last?.title, "swift test")
        XCTAssertEqual(items.last?.content, "ok")
    }

    func testCodexEmptyTurnDiffNotificationClearsProtocolFileChanges() async {
        let collector = AgentEventCollector()
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "turn/diff/updated",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "diff": "",
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let events = await collector.snapshot()
        guard case .fileChanges(let changes)? = events.first(where: {
            if case .fileChanges = $0 { return true }
            return false
        }) else {
            XCTFail("Expected empty turn/diff/updated to emit fileChanges.")
            return
        }
        XCTAssertTrue(changes.isEmpty)
    }

    func testCodexFileChangeItemChangesProduceProtocolFileChanges() async {
        let collector = AgentEventCollector()
        await CodexAppServerRunner.handleMessage(
            AgentJSONValue([
                "method": "item/completed",
                "params": [
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "item": [
                        "type": "fileChange",
                        "id": "patch-1",
                        "status": "completed",
                        "changes": [
                            [
                                "path": "Sources/App.swift",
                                "kind": ["type": "update", "move_path": NSNull()],
                                "diff": "@@\n-old\n+new\n",
                            ],
                        ],
                    ],
                ],
            ])!,
            emit: { event in
                await collector.record(event)
            }
        )

        let events = await collector.snapshot()
        guard case .fileChange(let change)? = events.first(where: {
            if case .fileChange = $0 { return true }
            return false
        }) else {
            XCTFail("Expected fileChange item to emit fileChange.")
            return
        }
        XCTAssertEqual(change.path, "Sources/App.swift")
        XCTAssertEqual(change.status, .modified)
    }

    func testCompletedAgentRunDoesNotImportWholeWorktreeDiff() throws {
        let root = try makeDirtyGitRepository()
        defer { try? FileManager.default.removeItem(at: root) }
        let project = makeProject(path: root.path)
        let session = TerminalSession.makeAgent(project: project, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        model.projects = [project]
        model.worktrees = [ProjectWorktree.defaultWorktree(for: project)]
        model.agentSessionStates[session.id] = AgentSessionState.empty(sessionID: session.id, tool: .codex)

        model.applyAgentDriverEventForTesting(
            .completed(exitCode: 0),
            sessionID: session.id,
            tool: .codex
        )

        let state = try XCTUnwrap(model.agentSessionStates[session.id])
        XCTAssertTrue(state.fileChanges.isEmpty)
        XCTAssertEqual(state.runState, .idle)
        XCTAssertEqual(try GitService().workingTreeAuditFiles(at: root.path).map(\.path), ["Demo.txt", "Untracked.txt"])
    }

    func testReviewAgentChangesSelectsRequestedFile() throws {
        let root = makeProject()
        let session = TerminalSession.makeAgent(project: root, tool: .codex)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        model.projects = [root]
        model.worktrees = [ProjectWorktree.defaultWorktree(for: root)]
        model.workspaces = [ProjectWorkspace.sample(projectID: root.id, path: root.path)]
        model.selectedProjectID = root.id
        model.selectedWorktreeID = root.id
        model.agentSessionStates[session.id] = AgentSessionState(
            sessionID: session.id,
            tool: .codex,
            messages: [],
            tasks: [],
            fileChanges: [
                AgentFileChange(path: "Sources/App.swift", status: .modified, summary: nil, updatedAt: Date()),
                AgentFileChange(path: "Tests/AppTests.swift", status: .added, summary: nil, updatedAt: Date()),
            ],
            runState: .idle,
            statusText: nil,
            externalSessionID: nil,
            updatedAt: Date()
        )

        model.reviewAgentChanges(session: session, selectedPath: "Tests/AppTests.swift")

        XCTAssertEqual(model.selectedWorktreeReviewID, root.id)
        XCTAssertEqual(model.selectedWorktreeReviewFileID, "Tests/AppTests.swift")
        XCTAssertEqual(model.workspacePrimaryViewMode(for: root.id), .review)
        XCTAssertEqual(model.agentSessionStates[session.id]?.fileChanges.count, 2)
    }

    private func makeProject() -> Project {
        makeProject(path: "/tmp/project-agent")
    }

    private func makeProject(path: String) -> Project {
        Project(
            id: UUID(),
            name: "Project",
            path: path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
    }

    private func makeDirtyGitRepository() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "one\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try "new\n".write(to: root.appendingPathComponent("Untracked.txt"), atomically: true, encoding: .utf8)
        return root
    }

    private func runGit(_ arguments: [String], at url: URL) throws {
        let process = Process()
        process.currentDirectoryURL = url
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["git"] + arguments
        let stderr = Pipe()
        process.standardError = stderr
        try process.run()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            let message = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "git failed"
            XCTFail(message)
        }
    }
}

private actor AgentSmokeCollector {
    private var assistant = ""
    private var completed = false

    func record(_ event: AgentDriverEvent) {
        switch event {
        case .timelineDelta(let delta) where delta.role == .assistant:
            assistant += delta.delta
        case .timelineItem(let item) where item.role == .assistant && item.content.isEmpty == false:
            assistant = item.content
        case .message(let role, let content) where role == .assistant:
            assistant += content
        case .completed(let exitCode):
            completed = exitCode == 0
        default:
            break
        }
    }

    func snapshot() -> (assistant: String, completed: Bool) {
        (assistant, completed)
    }
}

private actor AgentEventCollector {
    private var events: [AgentDriverEvent] = []

    func record(_ event: AgentDriverEvent) {
        events.append(event)
    }

    func snapshot() -> [AgentDriverEvent] {
        events
    }
}

private func withAgentTestTimeout<T: Sendable>(
    seconds: UInt64,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: seconds * 1_000_000_000)
            throw AgentTestTimeoutError()
        }

        let result = try await group.next()!
        group.cancelAll()
        return result
    }
}

private struct AgentTestTimeoutError: LocalizedError {
    var errorDescription: String? {
        "Timed out waiting for Codex app-server smoke test."
    }
}

private func blockingReadAvailableData(_ handle: FileHandle, timeout: TimeInterval) -> Data {
    let lock = NSLock()
    var result = Data()
    let semaphore = DispatchSemaphore(value: 0)
    DispatchQueue.global(qos: .utility).async {
        let data = handle.availableData
        lock.lock()
        result = data
        lock.unlock()
        semaphore.signal()
    }
    _ = semaphore.wait(timeout: .now() + timeout)

    lock.lock()
    defer { lock.unlock() }
    return result
}
