import Foundation

enum AgentRole: String, Codable, Hashable, Sendable {
    case user
    case assistant
    case system
    case tool
    case error
}

struct AgentMessage: Identifiable, Codable, Hashable, Sendable {
    var id: UUID
    var role: AgentRole
    var content: String
    var createdAt: Date
}

enum AgentTaskStatus: String, Codable, Hashable, Sendable {
    case pending
    case running
    case completed
    case failed
}

struct AgentTaskItem: Identifiable, Codable, Hashable, Sendable {
    var id: String
    var title: String
    var status: AgentTaskStatus
    var updatedAt: Date
}

struct AgentFileChange: Identifiable, Codable, Hashable, Sendable {
    var id: String { path }
    var path: String
    var status: WorktreeReviewFileStatus
    var summary: String?
    var diff: String?
    var additions: Int
    var deletions: Int
    var turnID: String?
    var itemID: String?
    var updatedAt: Date

    init(
        path: String,
        status: WorktreeReviewFileStatus,
        summary: String?,
        updatedAt: Date,
        diff: String? = nil,
        additions: Int = 0,
        deletions: Int = 0,
        turnID: String? = nil,
        itemID: String? = nil
    ) {
        self.path = path
        self.status = status
        self.summary = summary
        self.diff = diff
        self.additions = additions
        self.deletions = deletions
        self.turnID = turnID
        self.itemID = itemID
        self.updatedAt = updatedAt
    }
}

enum AgentTimelineKind: String, Codable, Hashable, Sendable {
    case userPrompt
    case assistantMessage
    case plan
    case reasoning
    case command
    case fileChange
    case tool
    case error
    case status
}

enum AgentTimelineStatus: String, Codable, Hashable, Sendable {
    case running
    case completed
    case failed
}

struct AgentTimelineItem: Identifiable, Codable, Hashable, Sendable {
    var id: String
    var turnID: String?
    var itemID: String?
    var kind: AgentTimelineKind
    var role: AgentRole?
    var title: String?
    var content: String
    var detail: String?
    var status: AgentTimelineStatus
    var createdAt: Date
    var updatedAt: Date
}

struct AgentTimelineDelta: Codable, Hashable, Sendable {
    var id: String
    var turnID: String?
    var itemID: String?
    var kind: AgentTimelineKind
    var role: AgentRole?
    var title: String?
    var detail: String?
    var delta: String
    var status: AgentTimelineStatus
}

enum AgentRunState: String, Codable, Hashable, Sendable {
    case idle
    case running
    case failed
}

enum AgentTransportKind: String, Codable, Hashable, Sendable {
    case codexAppServerJSONRPC
    case claudeStreamJSON
    case openCodeACP

    var displayName: String {
        switch self {
        case .codexAppServerJSONRPC:
            return "Codex app-server JSON-RPC"
        case .claudeStreamJSON:
            return "Claude stream-json"
        case .openCodeACP:
            return "OpenCode ACP"
        }
    }
}

struct AgentJSONRPCParams: Sendable {
    var rawValue: [String: AgentJSONValue]

    init(_ rawValue: [String: Any]) {
        var values: [String: AgentJSONValue] = [:]
        for (key, value) in rawValue.filteringNilValues() {
            if let jsonValue = AgentJSONValue(value) {
                values[key] = jsonValue
            }
        }
        self.rawValue = values
    }

    var jsonObject: [String: Any] {
        rawValue.mapValues { $0.jsonObject }
    }
}

indirect enum AgentJSONValue: Equatable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: AgentJSONValue])
    case array([AgentJSONValue])
    case null

    init?(_ raw: Any) {
        switch raw {
        case let value as String:
            self = .string(value)
        case let value as Bool:
            self = .bool(value)
        case let value as NSNumber:
            self = .number(value.doubleValue)
        case let value as [String: Any]:
            var object: [String: AgentJSONValue] = [:]
            for (key, child) in value {
                object[key] = AgentJSONValue(child) ?? .null
            }
            self = .object(object)
        case let value as [Any]:
            self = .array(value.map { AgentJSONValue($0) ?? .null })
        case _ as NSNull:
            self = .null
        default:
            return nil
        }
    }

    var stringValue: String? {
        if case .string(let value) = self {
            return normalizedNonEmptyString(value)
        }
        return nil
    }

    var rawStringValue: String? {
        if case .string(let value) = self {
            return value
        }
        return nil
    }

    var intValue: Int? {
        switch self {
        case .number(let value):
            return Int(value)
        case .string(let value):
            return Int(value)
        default:
            return nil
        }
    }

    var objectValue: [String: AgentJSONValue]? {
        if case .object(let value) = self {
            return value
        }
        return nil
    }

    var arrayValue: [AgentJSONValue]? {
        if case .array(let value) = self {
            return value
        }
        return nil
    }

    var jsonObject: Any {
        switch self {
        case .string(let value):
            return value
        case .number(let value):
            return value
        case .bool(let value):
            return value
        case .object(let value):
            return value.mapValues { $0.jsonObject }
        case .array(let value):
            return value.map { $0.jsonObject }
        case .null:
            return NSNull()
        }
    }
}

struct AgentSessionState: Codable, Hashable, Sendable {
    var sessionID: UUID
    var tool: AgentToolKind
    var messages: [AgentMessage]
    var timelineItems: [AgentTimelineItem]
    var tasks: [AgentTaskItem]
    var fileChanges: [AgentFileChange]
    var runState: AgentRunState
    var statusText: String?
    var externalSessionID: String?
    var runStartedAt: Date?
    var runCompletedAt: Date?
    var updatedAt: Date

    init(
        sessionID: UUID,
        tool: AgentToolKind,
        messages: [AgentMessage],
        timelineItems: [AgentTimelineItem] = [],
        tasks: [AgentTaskItem],
        fileChanges: [AgentFileChange],
        runState: AgentRunState,
        statusText: String?,
        externalSessionID: String?,
        runStartedAt: Date? = nil,
        runCompletedAt: Date? = nil,
        updatedAt: Date
    ) {
        self.sessionID = sessionID
        self.tool = tool
        self.messages = messages
        self.timelineItems = timelineItems
        self.tasks = tasks
        self.fileChanges = fileChanges
        self.runState = runState
        self.statusText = statusText
        self.externalSessionID = externalSessionID
        self.runStartedAt = runStartedAt
        self.runCompletedAt = runCompletedAt
        self.updatedAt = updatedAt
    }

    static func empty(sessionID: UUID, tool: AgentToolKind) -> AgentSessionState {
        AgentSessionState(
            sessionID: sessionID,
            tool: tool,
            messages: [],
            timelineItems: [],
            tasks: [],
            fileChanges: [],
            runState: .idle,
            statusText: nil,
            externalSessionID: nil,
            runStartedAt: nil,
            runCompletedAt: nil,
            updatedAt: Date()
        )
    }
}

enum AgentDriverEvent: Sendable {
    case status(String)
    case message(role: AgentRole, content: String)
    case timelineItem(AgentTimelineItem)
    case timelineDelta(AgentTimelineDelta)
    case task(AgentTaskItem)
    case fileChange(AgentFileChange)
    case fileChanges([AgentFileChange])
    case externalSessionID(String)
    case completed(exitCode: Int32)
}

struct AgentDriverRequest: Sendable {
    var tool: AgentToolKind
    var prompt: String
    var cwd: String
    var model: String?
    var reasoningEffort: String?
    var fullAccess: Bool
    var externalSessionID: String?
}

struct AgentDriverInvocation: Equatable, Sendable {
    var executablePath: String
    var arguments: [String]
    var currentDirectory: String
    var environment: [String: String]
    var transport: AgentTransportKind

    static func structuredCLI(
        arguments: [String],
        currentDirectory: String,
        transport: AgentTransportKind,
        environmentService: AIToolEnvironmentService = AIToolEnvironmentService()
    ) -> AgentDriverInvocation {
        var environment = environmentService.mergedEnvironment(includeBundledWrappers: false)
        environment["TERM_PROGRAM"] = "codux-agent"
        environment["TERM"] = "dumb"
        return AgentDriverInvocation(
            executablePath: "/usr/bin/env",
            arguments: arguments,
            currentDirectory: currentDirectory,
            environment: environment,
            transport: transport
        )
    }

    static func codexAppServer(
        currentDirectory: String,
        environmentService: AIToolEnvironmentService = AIToolEnvironmentService()
    ) -> AgentDriverInvocation {
        var environment = environmentService.mergedEnvironment(includeBundledWrappers: false)
        environment["TERM_PROGRAM"] = "codux-agent"
        environment["TERM"] = "dumb"
        if let executablePath = resolvedExecutablePath(named: "codex", environment: environment) {
            return AgentDriverInvocation(
                executablePath: executablePath,
                arguments: ["app-server", "--listen", "stdio://"],
                currentDirectory: currentDirectory,
                environment: environment,
                transport: .codexAppServerJSONRPC
            )
        }
        return AgentDriverInvocation(
            executablePath: "/usr/bin/env",
            arguments: ["codex", "app-server", "--listen", "stdio://"],
            currentDirectory: currentDirectory,
            environment: environment,
            transport: .codexAppServerJSONRPC
        )
    }

    static func openCodeACP(
        currentDirectory: String,
        environmentService: AIToolEnvironmentService = AIToolEnvironmentService()
    ) -> AgentDriverInvocation {
        var environment = environmentService.mergedEnvironment(includeBundledWrappers: false)
        environment["TERM_PROGRAM"] = "codux-agent"
        environment["TERM"] = "dumb"
        return AgentDriverInvocation(
            executablePath: "/usr/bin/env",
            arguments: ["opencode", "acp", "--cwd", currentDirectory],
            currentDirectory: currentDirectory,
            environment: environment,
            transport: .openCodeACP
        )
    }
}

private func resolvedExecutablePath(
    named executableName: String,
    environment: [String: String],
    fileManager: FileManager = .default
) -> String? {
    let path = environment["PATH"] ?? ""
    for directory in path.components(separatedBy: ":").compactMap(normalizedNonEmptyString) {
        let candidate = URL(fileURLWithPath: directory, isDirectory: true)
            .appendingPathComponent(executableName, isDirectory: false)
            .path
        if fileManager.isExecutableFile(atPath: candidate) {
            return candidate
        }
    }
    return nil
}

protocol AgentDriver: Sendable {
    var tool: AgentToolKind { get }
    var transport: AgentTransportKind { get }
    func invocation(for request: AgentDriverRequest) -> AgentDriverInvocation
    func run(
        request: AgentDriverRequest,
        emit: @Sendable @escaping (AgentDriverEvent) async -> Void
    ) async throws
}

enum AgentDriverError: LocalizedError {
    case unsupportedTool(String)
    case launchFailed(String)
    case nonZeroExit(tool: String, exitCode: Int32, output: String)
    case protocolTimeout(tool: String, method: String, output: String)

    var errorDescription: String? {
        switch self {
        case .unsupportedTool(let tool):
            return String(
                format: String(localized: "agent.error.unsupported_tool_format", defaultValue: "%@ is not supported yet.", bundle: .module),
                tool
            )
        case .launchFailed(let message):
            return message
        case .nonZeroExit(let tool, let exitCode, let output):
            let detail = output.trimmingCharacters(in: .whitespacesAndNewlines)
            if detail.isEmpty {
                return String(
                    format: String(localized: "agent.error.exit_format", defaultValue: "%@ exited with code %@.", bundle: .module),
                    tool,
                    "\(exitCode)"
                )
            }
            return String(
                format: String(localized: "agent.error.exit_detail_format", defaultValue: "%@ exited with code %@.\n%@", bundle: .module),
                tool,
                "\(exitCode)",
                detail
            )
        case .protocolTimeout(let tool, let method, let output):
            let detail = output.trimmingCharacters(in: .whitespacesAndNewlines)
            if detail.isEmpty {
                return String(
                    format: String(localized: "agent.error.protocol_timeout_format", defaultValue: "%@ did not respond to %@.", bundle: .module),
                    tool,
                    method
                )
            }
            return String(
                format: String(localized: "agent.error.protocol_timeout_detail_format", defaultValue: "%@ did not respond to %@.\n%@", bundle: .module),
                tool,
                method,
                detail
            )
        }
    }
}

struct AgentDriverFactory: Sendable {
    static let shared = AgentDriverFactory()

    private let drivers: [AgentToolKind: any AgentDriver]

    init(drivers: [any AgentDriver] = [
        CodexAgentDriver(),
        ClaudeAgentDriver(),
        OpenCodeAgentDriver(),
    ]) {
        self.drivers = Dictionary(uniqueKeysWithValues: drivers.map { ($0.tool, $0) })
    }

    func driver(for tool: AgentToolKind) throws -> any AgentDriver {
        guard let driver = drivers[tool] else {
            throw AgentDriverError.unsupportedTool(tool.displayName)
        }
        return driver
    }
}

struct CodexAgentDriver: AgentDriver {
    let tool: AgentToolKind = .codex
    let transport: AgentTransportKind = .codexAppServerJSONRPC

    func run(
        request: AgentDriverRequest,
        emit: @Sendable @escaping (AgentDriverEvent) async -> Void
    ) async throws {
        let invocation = invocation(for: request)
        try await CodexAppServerRunner.run(
            toolName: tool.displayName,
            request: request,
            invocation: invocation,
            emit: emit
        )
    }

    func invocation(for request: AgentDriverRequest) -> AgentDriverInvocation {
        AgentDriverInvocation.codexAppServer(currentDirectory: request.cwd)
    }
}

struct ClaudeAgentDriver: AgentDriver {
    let tool: AgentToolKind = .claude
    let transport: AgentTransportKind = .claudeStreamJSON

    func run(
        request: AgentDriverRequest,
        emit: @Sendable @escaping (AgentDriverEvent) async -> Void
    ) async throws {
        let invocation = invocation(for: request)
        try await StructuredAgentProcessRunner.run(
            toolName: tool.displayName,
            invocation: invocation
        ) { payload in
            await emit(.status(Self.statusText(from: payload)))
            if let sessionID = Self.sessionID(from: payload) {
                await emit(.externalSessionID(sessionID))
            }
            if let message = Self.message(from: payload) {
                await emit(.message(role: message.role, content: message.content))
            }
        } onExit: { exitCode in
            await emit(.completed(exitCode: exitCode))
        }
    }

    func invocation(for request: AgentDriverRequest) -> AgentDriverInvocation {
        var arguments = [
            "claude",
            "--print",
            "--output-format=stream-json",
            "--include-partial-messages",
        ]
        if let model = normalizedNonEmptyString(request.model) {
            arguments.append(contentsOf: ["--model", model])
        }
        if request.fullAccess {
            arguments.append("--dangerously-skip-permissions")
        }
        if let externalSessionID = normalizedNonEmptyString(request.externalSessionID) {
            arguments.append(contentsOf: ["--resume", externalSessionID])
        }
        arguments.append(request.prompt)

        return AgentDriverInvocation.structuredCLI(
            arguments: arguments,
            currentDirectory: request.cwd,
            transport: transport
        )
    }

    private static func statusText(from payload: [String: Any]) -> String {
        normalizedNonEmptyString(payload["type"] as? String)
            ?? normalizedNonEmptyString(payload["subtype"] as? String)
            ?? "event"
    }

    private static func sessionID(from payload: [String: Any]) -> String? {
        normalizedNonEmptyString(payload["session_id"] as? String)
            ?? normalizedNonEmptyString(payload["sessionId"] as? String)
    }

    private static func message(from payload: [String: Any]) -> (role: AgentRole, content: String)? {
        let role = role(from: payload["role"] as? String)
        let text = extractText(from: payload)
        guard let text else {
            return nil
        }
        return (role, text)
    }
}

struct OpenCodeAgentDriver: AgentDriver {
    let tool: AgentToolKind = .opencode
    let transport: AgentTransportKind = .openCodeACP

    func run(
        request: AgentDriverRequest,
        emit: @Sendable @escaping (AgentDriverEvent) async -> Void
    ) async throws {
        let invocation = invocation(for: request)
        try await OpenCodeACPRunner.run(
            toolName: tool.displayName,
            request: request,
            invocation: invocation,
            emit: emit
        )
    }

    func invocation(for request: AgentDriverRequest) -> AgentDriverInvocation {
        AgentDriverInvocation.openCodeACP(currentDirectory: request.cwd)
    }
}

enum CodexAppServerRunner {
    static func run(
        toolName: String,
        request: AgentDriverRequest,
        invocation: AgentDriverInvocation,
        emit: @escaping @Sendable (AgentDriverEvent) async -> Void
    ) async throws {
        let client = try AgentJSONRPCProcessClient(invocation: invocation)
        try await withTaskCancellationHandler {
            do {
                await client.startReading { payload in
                    await handleMessage(payload, emit: emit)
                }
                _ = try await client.request(
                    method: "initialize",
                    params: AgentJSONRPCParams([
                        "clientInfo": [
                            "name": "codux",
                            "version": "dev",
                        ],
                        "capabilities": [:],
                    ])
                )
                try await client.notify(method: "initialized")

                let threadID = try await startThread(request: request, client: client)
                await emit(.externalSessionID(threadID))

                let turnResult = try await client.request(
                    method: "turn/start",
                    params: AgentJSONRPCParams([
                        "threadId": threadID,
                        "input": [
                            [
                                "type": "text",
                                "text": request.prompt,
                                "text_elements": [],
                            ],
                        ],
                        "cwd": request.cwd,
                        "approvalPolicy": "never",
                        "sandboxPolicy": request.fullAccess ? ["type": "dangerFullAccess"] : Optional<[String: String]>.none as Any,
                        "model": normalizedNonEmptyString(request.model) as Any,
                        "effort": normalizedNonEmptyString(request.reasoningEffort) as Any,
                    ])
                )

                let turnID = turnResult.objectValue?["turn"]?.objectValue?["id"]?.stringValue
                await client.markExpectedCompletion(threadID: threadID, turnID: turnID)

                let exitCode = await client.waitForExpectedCompletion()
                try Task.checkCancellation()
                await emit(.completed(exitCode: exitCode))
                await client.terminate()
                try await client.waitForExit(toolName: toolName, expectedExitCode: nil)
            } catch is CancellationError {
                await client.terminate()
                throw CancellationError()
            } catch {
                await client.terminate()
                throw error
            }
        } onCancel: {
            Task {
                await client.terminate()
            }
        }
    }

    private static func startThread(
        request: AgentDriverRequest,
        client: AgentJSONRPCProcessClient
    ) async throws -> String {
        if let externalSessionID = normalizedNonEmptyString(request.externalSessionID) {
            let result = try await client.request(
                method: "thread/resume",
                params: AgentJSONRPCParams([
                    "threadId": externalSessionID,
                    "cwd": request.cwd,
                    "approvalPolicy": "never",
                    "sandbox": request.fullAccess ? "danger-full-access" : "workspace-write",
                    "model": normalizedNonEmptyString(request.model) as Any,
                    "effort": normalizedNonEmptyString(request.reasoningEffort) as Any,
                ])
            )
            if let threadID = threadID(from: result) {
                return threadID
            }
        }

        let result = try await client.request(
            method: "thread/start",
            params: AgentJSONRPCParams([
                "cwd": request.cwd,
                "ephemeral": false,
                "threadSource": "user",
                "sessionStartSource": "startup",
                "approvalPolicy": "never",
                "sandbox": request.fullAccess ? "danger-full-access" : "workspace-write",
                "model": normalizedNonEmptyString(request.model) as Any,
                "effort": normalizedNonEmptyString(request.reasoningEffort) as Any,
            ])
        )
        guard let threadID = threadID(from: result) else {
            throw AgentDriverError.launchFailed("Codex app-server did not return a thread id.")
        }
        return threadID
    }

    private static func threadID(from payload: AgentJSONValue) -> String? {
        guard let object = payload.objectValue else {
            return nil
        }
        if let thread = object["thread"]?.objectValue {
            return thread["id"]?.stringValue
                ?? thread["sessionId"]?.stringValue
        }
        return object["threadId"]?.stringValue
            ?? object["id"]?.stringValue
    }

    static func handleMessage(
        _ payload: AgentJSONValue,
        emit: @escaping @Sendable (AgentDriverEvent) async -> Void
    ) async {
        guard let object = payload.objectValue,
              let method = object["method"]?.stringValue else {
            return
        }
        let params = object["params"] ?? .object([:])

        switch method {
        case "thread/started":
            if let threadID = threadID(from: params) {
                await emit(.externalSessionID(threadID))
            }
        case "turn/started":
            if let item = turnTimelineItem(from: params, status: .running) {
                await emit(.timelineItem(item))
            }
        case "turn/completed":
            if let item = turnTimelineItem(from: params, status: .completed) {
                await emit(.timelineItem(item))
            }
        case "item/started":
            if let item = timelineItem(fromCodexItemNotification: params, fallbackStatus: .running) {
                await emit(.timelineItem(item))
            }
        case "item/agentMessage/delta":
            await emit(.status(agentThinkingStatusText()))
            if let delta = params.objectValue?["delta"]?.rawStringValue {
                await emit(
                    .timelineDelta(
                        AgentTimelineDelta(
                            id: timelineID(from: params, fallbackPrefix: "assistant"),
                            turnID: turnID(from: params),
                            itemID: itemID(from: params),
                            kind: .assistantMessage,
                            role: .assistant,
                            title: nil,
                            detail: nil,
                            delta: delta,
                            status: .running
                        )
                    )
                )
            }
        case "item/plan/delta", "item/reasoning/summaryTextDelta", "item/reasoning/textDelta":
            await emit(.status(agentThinkingStatusText()))
            if let delta = params.objectValue?["delta"]?.rawStringValue {
                let isPlan = method == "item/plan/delta"
                await emit(
                    .timelineDelta(
                        AgentTimelineDelta(
                            id: timelineID(from: params, fallbackPrefix: isPlan ? "plan" : "reasoning"),
                            turnID: turnID(from: params),
                            itemID: itemID(from: params),
                            kind: isPlan ? .plan : .reasoning,
                            role: .system,
                            title: nil,
                            detail: nil,
                            delta: delta,
                            status: .running
                        )
                    )
                )
                if let taskTitle = normalizedNonEmptyString(delta) {
                    await emit(
                        .task(
                            AgentTaskItem(
                                id: "\(method)-\(stableAgentSummaryID(taskTitle))",
                                title: taskTitle,
                                status: .running,
                                updatedAt: Date()
                            )
                        )
                    )
                }
            }
        case "item/commandExecution/outputDelta":
            await emit(.status(agentCommandStatusText()))
            if let delta = params.objectValue?["delta"]?.rawStringValue {
                await emit(
                    .timelineDelta(
                        AgentTimelineDelta(
                            id: timelineID(from: params, fallbackPrefix: "command"),
                            turnID: turnID(from: params),
                            itemID: itemID(from: params),
                            kind: .command,
                            role: .tool,
                            title: nil,
                            detail: nil,
                            delta: delta,
                            status: .running
                        )
                    )
                )
            }
        case "item/fileChange/outputDelta":
            await emit(.status(agentEditingStatusText()))
            if let delta = params.objectValue?["delta"]?.rawStringValue {
                await emit(
                    .timelineDelta(
                        AgentTimelineDelta(
                            id: timelineID(from: params, fallbackPrefix: "fileChange"),
                            turnID: turnID(from: params),
                            itemID: itemID(from: params),
                            kind: .fileChange,
                            role: .tool,
                            title: nil,
                            detail: nil,
                            delta: delta,
                            status: .running
                        )
                    )
                )
            }
        case "turn/diff/updated":
            if let fileChanges = fileChanges(fromTurnDiffNotification: params) {
                await emit(.fileChanges(fileChanges))
            }
        case "item/fileChange/patchUpdated":
            for fileChange in fileChanges(fromPatchUpdatedNotification: params) {
                await emit(.fileChange(fileChange))
            }
        case "item/completed":
            if let item = timelineItem(fromCodexItemNotification: params, fallbackStatus: .completed) {
                await emit(.timelineItem(item))
            }
            if let item = params.objectValue?["item"],
               let task = task(fromCodexThreadItem: item) {
                await emit(.task(task))
            }
            if let item = params.objectValue?["item"],
               let fileChanges = fileChanges(fromCodexThreadItem: item, turnID: turnID(from: params)) {
                for fileChange in fileChanges {
                    await emit(.fileChange(fileChange))
                }
            }
        case "error", "warning", "guardianWarning", "configWarning":
            if let text = extractText(from: params) {
                await emit(.message(role: .error, content: text))
            }
            if let text = extractText(from: params) {
                await emit(
                    .timelineItem(
                        AgentTimelineItem(
                            id: "error-\(stableAgentSummaryID(method + text))",
                            turnID: nil,
                            itemID: nil,
                            kind: .error,
                            role: .error,
                            title: method,
                            content: text,
                            detail: nil,
                            status: .failed,
                            createdAt: Date(),
                            updatedAt: Date()
                        )
                    )
                )
            }
        default:
            break
        }
    }

    private static func turnTimelineItem(from payload: AgentJSONValue, status: AgentTimelineStatus) -> AgentTimelineItem? {
        guard let object = payload.objectValue else {
            return nil
        }
        let threadID = object["threadId"]?.stringValue
        let turnObject = object["turn"]?.objectValue
        let turnID = object["turnId"]?.stringValue ?? turnObject?["id"]?.stringValue
        let id = "turn-\(turnID ?? threadID ?? stableAgentSummaryID(String(describing: object)))"
        let title = status == .running ? "Turn started" : "Turn completed"
        return AgentTimelineItem(
            id: id,
            turnID: turnID,
            itemID: nil,
            kind: .status,
            role: .system,
            title: title,
            content: "",
            detail: nil,
            status: status,
            createdAt: Date(),
            updatedAt: Date()
        )
    }

    private static func timelineItem(
        fromCodexItemNotification payload: AgentJSONValue,
        fallbackStatus: AgentTimelineStatus
    ) -> AgentTimelineItem? {
        guard let object = payload.objectValue,
              let item = object["item"]?.objectValue,
              let type = item["type"]?.stringValue else {
            return nil
        }
        guard type != "userMessage" else {
            return nil
        }
        let resolvedItemID = item["id"]?.stringValue ?? object["itemId"]?.stringValue
        let resolvedTurnID = object["turnId"]?.stringValue
        let kind = timelineKind(fromCodexItemType: type)
        let title = timelineTitle(fromCodexItem: item, type: type)
        let content = timelineContent(fromCodexItem: item, type: type) ?? ""
        let detail = timelineDetail(fromCodexItem: item, type: type)
        return AgentTimelineItem(
            id: timelineID(turnID: resolvedTurnID, itemID: resolvedItemID, fallbackPrefix: type),
            turnID: resolvedTurnID,
            itemID: resolvedItemID,
            kind: kind,
            role: role(forTimelineKind: kind),
            title: title,
            content: content,
            detail: detail,
            status: fallbackStatus,
            createdAt: Date(),
            updatedAt: Date()
        )
    }

    private static func timelineKind(fromCodexItemType type: String) -> AgentTimelineKind {
        switch type {
        case "userMessage":
            return .userPrompt
        case "agentMessage":
            return .assistantMessage
        case "plan":
            return .plan
        case "reasoning":
            return .reasoning
        case "commandExecution":
            return .command
        case "fileChange":
            return .fileChange
        case "mcpToolCall", "dynamicToolCall", "collabAgentToolCall", "webSearch", "imageView", "imageGeneration":
            return .tool
        default:
            return .status
        }
    }

    private static func role(forTimelineKind kind: AgentTimelineKind) -> AgentRole? {
        switch kind {
        case .userPrompt:
            return .user
        case .assistantMessage:
            return .assistant
        case .plan, .reasoning, .status:
            return .system
        case .command, .fileChange, .tool:
            return .tool
        case .error:
            return .error
        }
    }

    private static func timelineTitle(fromCodexItem item: [String: AgentJSONValue], type: String) -> String? {
        switch type {
        case "commandExecution":
            return item["command"]?.stringValue
        case "mcpToolCall":
            let server = item["server"]?.stringValue
            let tool = item["tool"]?.stringValue
            return [server, tool].compactMap(normalizedNonEmptyString).joined(separator: " / ")
        case "dynamicToolCall", "collabAgentToolCall":
            return item["tool"]?.stringValue
        case "webSearch":
            return item["query"]?.stringValue
        case "fileChange":
            return "File changes"
        default:
            return nil
        }
    }

    private static func timelineContent(fromCodexItem item: [String: AgentJSONValue], type: String) -> String? {
        switch type {
        case "agentMessage":
            return item["text"]?.stringValue
        case "plan":
            return item["text"]?.stringValue
        case "reasoning":
            let summary = item["summary"]?.arrayValue?.compactMap(extractText(from:)).joined(separator: "\n")
            let content = item["content"]?.arrayValue?.compactMap(extractText(from:)).joined(separator: "\n")
            return normalizedNonEmptyString([summary, content].compactMap(normalizedNonEmptyString).joined(separator: "\n\n"))
        case "userMessage":
            return extractText(from: .object(item))
        case "commandExecution":
            return item["aggregatedOutput"]?.stringValue
        case "fileChange":
            let changes = item["changes"]?.arrayValue?.compactMap { change -> String? in
                guard let object = change.objectValue else { return nil }
                return object["path"]?.stringValue
            }
            return changes?.joined(separator: "\n")
        default:
            return extractText(from: .object(item))
        }
    }

    private static func timelineDetail(fromCodexItem item: [String: AgentJSONValue], type: String) -> String? {
        switch type {
        case "commandExecution":
            let status = item["status"]?.stringValue
            let exitCode = item["exitCode"]?.intValue.map { "exit \($0)" }
            return [status, exitCode].compactMap(normalizedNonEmptyString).joined(separator: " / ")
        case "fileChange":
            return item["status"]?.stringValue
        case "mcpToolCall", "dynamicToolCall":
            return item["status"]?.stringValue
        default:
            return nil
        }
    }

    private static func timelineID(from payload: AgentJSONValue, fallbackPrefix: String) -> String {
        timelineID(
            turnID: turnID(from: payload),
            itemID: itemID(from: payload),
            fallbackPrefix: fallbackPrefix
        )
    }

    private static func timelineID(turnID: String?, itemID: String?, fallbackPrefix: String) -> String {
        if let itemID = normalizedNonEmptyString(itemID) {
            return "\(turnID ?? "turn")#\(itemID)"
        }
        return "\(fallbackPrefix)-\(turnID ?? UUID().uuidString)"
    }

    private static func turnID(from payload: AgentJSONValue) -> String? {
        payload.objectValue?["turnId"]?.stringValue
            ?? payload.objectValue?["turn"]?.objectValue?["id"]?.stringValue
    }

    private static func itemID(from payload: AgentJSONValue) -> String? {
        payload.objectValue?["itemId"]?.stringValue
            ?? payload.objectValue?["item"]?.objectValue?["id"]?.stringValue
    }

    private static func message(fromCodexThreadItem item: AgentJSONValue) -> (role: AgentRole, content: String)? {
        guard let object = item.objectValue,
              let type = object["type"]?.stringValue else {
            return nil
        }
        switch type {
        case "agentMessage":
            return object["text"]?.stringValue.map { (.assistant, $0) }
        case "plan", "reasoning":
            return extractText(from: item).map { (.system, $0) }
        case "commandExecution", "mcpToolCall", "dynamicToolCall", "fileChange":
            return extractText(from: item).map { (.tool, $0) }
        default:
            return nil
        }
    }

    private static func task(fromCodexThreadItem item: AgentJSONValue) -> AgentTaskItem? {
        guard let object = item.objectValue,
              let type = object["type"]?.stringValue,
              ["plan", "reasoning"].contains(type),
              let title = extractText(from: item) else {
            return nil
        }
        let id = object["id"]?.stringValue ?? "\(type)-\(stableAgentSummaryID(title))"
        return AgentTaskItem(id: id, title: title, status: .completed, updatedAt: Date())
    }

    private static func fileChanges(fromCodexThreadItem item: AgentJSONValue, turnID: String?) -> [AgentFileChange]? {
        guard let object = item.objectValue,
              object["type"]?.stringValue == "fileChange" else {
            return nil
        }
        let itemID = object["id"]?.stringValue
        if let changes = object["changes"]?.arrayValue {
            let fileChanges = changes.compactMap {
                fileChange(fromCodexFileUpdateChange: $0, turnID: turnID, itemID: itemID)
            }
            return fileChanges.isEmpty ? nil : fileChanges
        }
        guard let path = firstString(in: object, keys: ["path", "filePath", "relativePath", "targetPath"]) else {
            return nil
        }
        let rawStatus = firstString(in: object, keys: ["status", "changeType", "operation", "action"])
        let diff = object["diff"]?.stringValue ?? extractText(from: item)
        let stats = diffLineCounts(in: diff)
        return [
            AgentFileChange(
                path: path,
                status: worktreeReviewStatus(fromAgentStatus: rawStatus),
                summary: normalizedNonEmptyString(diff),
                updatedAt: Date(),
                diff: normalizedNonEmptyString(diff),
                additions: stats.additions,
                deletions: stats.deletions,
                turnID: turnID,
                itemID: itemID
            ),
        ]
    }

    private static func fileChanges(fromPatchUpdatedNotification payload: AgentJSONValue) -> [AgentFileChange] {
        guard let changes = payload.objectValue?["changes"]?.arrayValue else {
            return []
        }
        return changes.compactMap {
            fileChange(
                fromCodexFileUpdateChange: $0,
                turnID: turnID(from: payload),
                itemID: itemID(from: payload)
            )
        }
    }

    private static func fileChange(
        fromCodexFileUpdateChange change: AgentJSONValue,
        turnID: String?,
        itemID: String?
    ) -> AgentFileChange? {
        guard let object = change.objectValue,
              let path = object["path"]?.stringValue else {
            return nil
        }
        let kind = object["kind"]?.objectValue
        let status: WorktreeReviewFileStatus
        if kind?["move_path"]?.stringValue != nil {
            status = .renamed
        } else {
            status = worktreeReviewStatus(fromAgentStatus: kind?["type"]?.stringValue)
        }
        let diff = object["diff"]?.stringValue
        let stats = diffLineCounts(in: diff)
        return AgentFileChange(
            path: path,
            status: status,
            summary: normalizedNonEmptyString(diff),
            updatedAt: Date(),
            diff: normalizedNonEmptyString(diff),
            additions: stats.additions,
            deletions: stats.deletions,
            turnID: turnID,
            itemID: itemID
        )
    }

    private static func fileChanges(fromTurnDiffNotification payload: AgentJSONValue) -> [AgentFileChange]? {
        guard let diff = rawString(from: payload.objectValue?["diff"]) else {
            return nil
        }
        return fileChanges(fromUnifiedDiff: diff, turnID: turnID(from: payload), itemID: nil)
    }

    private static func fileChanges(fromUnifiedDiff diff: String, turnID: String?, itemID: String?) -> [AgentFileChange] {
        let trimmedDiff = diff.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedDiff.isEmpty else {
            return []
        }
        var changes: [AgentFileChange] = []
        var currentPath: String?
        var currentStatus = WorktreeReviewFileStatus.modified
        var currentLines: [String] = []

        func flushCurrentChange() {
            guard let currentPath else { return }
            let currentDiff = currentLines.joined(separator: "\n")
            let stats = diffLineCounts(in: currentDiff)
            changes.append(
                AgentFileChange(
                    path: currentPath,
                    status: currentStatus,
                    summary: currentDiff,
                    updatedAt: Date(),
                    diff: currentDiff,
                    additions: stats.additions,
                    deletions: stats.deletions,
                    turnID: turnID,
                    itemID: itemID
                )
            )
        }

        for line in trimmedDiff.split(separator: "\n", omittingEmptySubsequences: false).map(String.init) {
            if line.hasPrefix("diff --git ") {
                flushCurrentChange()
                currentPath = pathFromDiffGitLine(line)
                currentStatus = .modified
                currentLines = [line]
                continue
            }
            guard currentPath != nil else {
                continue
            }
            currentLines.append(line)
            if line.hasPrefix("new file mode") {
                currentStatus = .added
            } else if line.hasPrefix("deleted file mode") {
                currentStatus = .deleted
            } else if line.hasPrefix("rename from ") {
                currentStatus = .renamed
            }
            if line.hasPrefix("+++ b/"),
               let path = normalizedDiffPath(String(line.dropFirst(6))) {
                currentPath = path
            }
        }
        flushCurrentChange()
        return changes
    }

    private static func pathFromDiffGitLine(_ line: String) -> String? {
        let parts = line.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        guard parts.count >= 4 else {
            return nil
        }
        return normalizedDiffPath(parts[3])
            ?? normalizedDiffPath(parts[2])
    }

    private static func normalizedDiffPath(_ rawPath: String) -> String? {
        let trimmedPath = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedPath.isEmpty, trimmedPath != "/dev/null" else {
            return nil
        }
        if trimmedPath.hasPrefix("a/") || trimmedPath.hasPrefix("b/") {
            return String(trimmedPath.dropFirst(2))
        }
        return trimmedPath
    }

    private static func rawString(from value: AgentJSONValue?) -> String? {
        guard case .string(let text) = value else {
            return nil
        }
        return text
    }
}

private func diffLineCounts(in diff: String?) -> (additions: Int, deletions: Int) {
    guard let diff else {
        return (0, 0)
    }
    var additions = 0
    var deletions = 0
    for line in diff.split(separator: "\n", omittingEmptySubsequences: false) {
        if line.hasPrefix("+++") || line.hasPrefix("---") {
            continue
        }
        if line.hasPrefix("+") {
            additions += 1
        } else if line.hasPrefix("-") {
            deletions += 1
        }
    }
    return (additions, deletions)
}

private enum OpenCodeACPRunner {
    static func run(
        toolName: String,
        request: AgentDriverRequest,
        invocation: AgentDriverInvocation,
        emit: @escaping @Sendable (AgentDriverEvent) async -> Void
    ) async throws {
        let client = try AgentJSONRPCProcessClient(invocation: invocation)
        try await withTaskCancellationHandler {
            do {
                await client.startReading { payload in
                    await handleMessage(payload, emit: emit)
                }
                _ = try await client.request(
                    method: "initialize",
                    params: AgentJSONRPCParams([
                        "protocolVersion": 1,
                        "clientCapabilities": [:],
                        "clientInfo": [
                            "name": "codux",
                            "title": "Codux",
                            "version": "dev",
                        ],
                    ])
                )

                let session = try await client.request(
                    method: "session/new",
                    params: AgentJSONRPCParams([
                        "cwd": request.cwd,
                        "mcpServers": [],
                    ])
                )
                let sessionObject = session.objectValue ?? [:]
                guard let sessionID = sessionObject["sessionId"]?.stringValue
                    ?? sessionObject["id"]?.stringValue
                    ?? sessionObject["session"]?.objectValue?["id"]?.stringValue else {
                    throw AgentDriverError.launchFailed("OpenCode ACP did not return a session id.")
                }
                await emit(.externalSessionID(sessionID))

                let promptResult = try await client.request(
                    method: "session/prompt",
                    params: AgentJSONRPCParams([
                        "sessionId": sessionID,
                        "prompt": [
                            [
                                "type": "text",
                                "text": request.prompt,
                            ],
                        ],
                    ])
                )
                if let stopReason = promptResult.objectValue?["stopReason"]?.stringValue {
                    await emit(.status(stopReason))
                }

                try Task.checkCancellation()
                await emit(.completed(exitCode: 0))
                await client.terminate()
                try await client.waitForExit(toolName: toolName, expectedExitCode: nil)
            } catch is CancellationError {
                await client.terminate()
                throw CancellationError()
            } catch {
                await client.terminate()
                throw error
            }
        } onCancel: {
            Task {
                await client.terminate()
            }
        }
    }

    private static func handleMessage(
        _ payload: AgentJSONValue,
        emit: @escaping @Sendable (AgentDriverEvent) async -> Void
    ) async {
        guard let object = payload.objectValue,
              let method = object["method"]?.stringValue else {
            return
        }
        let params = object["params"] ?? .object([:])
        await emit(.status(agentThinkingStatusText()))

        if method.contains("session") || method.contains("agent") || method.contains("content") {
            if let text = extractText(from: params) {
                await emit(.message(role: role(from: params.objectValue?["role"]?.stringValue), content: text))
            }
        }
    }
}

private func agentThinkingStatusText() -> String {
    String(localized: "agent.status.thinking", defaultValue: "Thinking", bundle: .module)
}

private func agentCommandStatusText() -> String {
    String(localized: "agent.status.running_command", defaultValue: "Running command", bundle: .module)
}

private func agentEditingStatusText() -> String {
    String(localized: "agent.status.editing_files", defaultValue: "Editing files", bundle: .module)
}

private actor AgentJSONRPCProcessClient {
    private let process: Process
    private let stdin: Pipe
    private let stdout: Pipe
    private let stderr: Pipe
    private let debugLog = AppDebugLog.shared
    private let invocation: AgentDriverInvocation
    private let outputBuffer = AgentProcessOutputBuffer()
    private let errorBuffer = AgentProcessOutputBuffer()
    private var nextRequestID = 1
    private var pendingResponses: [Int: CheckedContinuation<AgentJSONValue, Error>] = [:]
    private var expectedCompletion: (threadID: String, turnID: String?)?
    private var completionContinuation: CheckedContinuation<Int32, Never>?
    private var completedTurnKeys: Set<String> = []
    private var completedExitCode: Int32?
    private var isCompletionCancelled = false
    private var didStartReading = false
    private var pendingResponseTimeouts: [Int: Task<Void, Never>] = [:]
    private var didProcessExit = false
    private var outputQueue: [[String: Any]] = []
    private var isProcessingOutput = false

    init(invocation: AgentDriverInvocation) throws {
        self.invocation = invocation
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

        self.process = process
        self.stdin = stdin
        self.stdout = stdout
        self.stderr = stderr

        do {
            try process.run()
            debugLog.log(
                "agent-rpc",
                "launch transport=\(invocation.transport.rawValue) executable=\(invocation.executablePath) args=\(invocation.arguments.joined(separator: " "))"
            )
        } catch {
            throw AgentDriverError.launchFailed(error.localizedDescription)
        }
    }

    func startReading(onNotification: @escaping @Sendable (AgentJSONValue) async -> Void) {
        guard !didStartReading else {
            return
        }
        didStartReading = true
        let stdoutHandle = stdout.fileHandleForReading
        let stderrHandle = stderr.fileHandleForReading
        stdoutHandle.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty {
                handle.readabilityHandler = nil
                return
            }
            Task {
                await self.handleOutput(data, onNotification: onNotification)
            }
        }
        stderrHandle.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty {
                handle.readabilityHandler = nil
                return
            }
            Task {
                await self.captureError(data)
            }
        }
        DispatchQueue.global(qos: .utility).async {
            Task {
                await self.waitForProcessExitAndFailPendingRequests()
            }
        }
    }

    func request(method: String, params: AgentJSONRPCParams) async throws -> AgentJSONValue {
        let requestID = nextRequestID
        nextRequestID += 1
        let payload: [String: Any] = [
            "jsonrpc": "2.0",
            "id": requestID,
            "method": method,
            "params": params.jsonObject,
        ]
        debugLog.log("agent-rpc", "request id=\(requestID) method=\(method)")
        return try await withCheckedThrowingContinuation { continuation in
            registerPendingResponse(
                requestID: requestID,
                method: method,
                payload: payload,
                continuation: continuation
            )
        }
    }

    private func registerPendingResponse(
        requestID: Int,
        method: String,
        payload: [String: Any],
        continuation: CheckedContinuation<AgentJSONValue, Error>
    ) {
        if didProcessExit {
            continuation.resume(throwing: AgentDriverError.launchFailed("Agent process exited before \(method) completed."))
            return
        }
        pendingResponses[requestID] = continuation
        pendingResponseTimeouts[requestID] = Task { [weak debugLog] in
            try? await Task.sleep(nanoseconds: 30_000_000_000)
            guard !Task.isCancelled else { return }
            await self.failPendingResponse(requestID: requestID, method: method)
            debugLog?.log("agent-rpc", "request-timeout id=\(requestID) method=\(method)")
        }
        do {
            try write(payload)
        } catch {
            pendingResponses.removeValue(forKey: requestID)
            pendingResponseTimeouts.removeValue(forKey: requestID)?.cancel()
            debugLog.log("agent-rpc", "request-write-failed id=\(requestID) method=\(method) error=\(error.localizedDescription)")
            continuation.resume(throwing: error)
        }
    }

    private func failPendingResponse(requestID: Int, method: String) async {
        guard let continuation = pendingResponses.removeValue(forKey: requestID) else {
            return
        }
        pendingResponseTimeouts.removeValue(forKey: requestID)?.cancel()
        continuation.resume(throwing: await timeoutError(method: method))
    }

    func notify(method: String, params: AgentJSONRPCParams? = nil) throws {
        var payload: [String: Any] = [
            "jsonrpc": "2.0",
            "method": method,
        ]
        if let params {
            payload["params"] = params.jsonObject
        }
        debugLog.log("agent-rpc", "notify method=\(method)")
        try write(payload)
    }

    func handleOutput(
        _ data: Data,
        onNotification: @escaping @Sendable (AgentJSONValue) async -> Void
    ) async {
        let lines = await outputBuffer.append(data)
        for line in lines {
            guard let payload = parseJSONObjectLine(line) else {
                continue
            }
            outputQueue.append(payload)
        }
        await processQueuedOutput(onNotification: onNotification)
    }

    private func processQueuedOutput(
        onNotification: @escaping @Sendable (AgentJSONValue) async -> Void
    ) async {
        guard !isProcessingOutput else {
            return
        }
        isProcessingOutput = true
        defer { isProcessingOutput = false }
        while outputQueue.isEmpty == false {
            let payload = outputQueue.removeFirst()
            guard let value = AgentJSONValue(payload) else {
                continue
            }
            if payload["id"] != nil, payload["method"] != nil {
                try? handleServerRequest(payload)
            } else if payload["id"] != nil {
                handleResponse(payload)
            } else {
                await handleNotification(value, onNotification: onNotification)
            }
        }
    }

    func captureError(_ data: Data) async {
        _ = await errorBuffer.append(data)
    }

    func markExpectedCompletion(threadID: String, turnID: String?) {
        expectedCompletion = (threadID, turnID)
        if let turnID,
           completedTurnKeys.contains(completionKey(threadID: threadID, turnID: turnID)) {
            finishExpectedCompletion(exitCode: 0)
        } else if turnID == nil,
                  completedTurnKeys.contains(where: { $0.hasPrefix("\(threadID)#") }) {
            finishExpectedCompletion(exitCode: 0)
        }
    }

    func waitForExpectedCompletion() async -> Int32 {
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                if isCompletionCancelled {
                    continuation.resume(returning: 1)
                } else if let completedExitCode {
                    continuation.resume(returning: completedExitCode)
                } else {
                    completionContinuation = continuation
                }
            }
        } onCancel: {
            Task {
                await self.cancelExpectedCompletion()
            }
        }
    }

    func waitForExit(toolName: String, expectedExitCode: Int32?) async throws {
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                DispatchQueue.global(qos: .utility).async {
                    self.process.waitUntilExit()
                    continuation.resume()
                }
            }
        } onCancel: {
            process.terminate()
        }

        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        let errorTail = await errorBuffer.drainText()
        let outputTail = await outputBuffer.drainText()
        if let expectedExitCode, process.terminationStatus != expectedExitCode {
            throw AgentDriverError.nonZeroExit(
                tool: toolName,
                exitCode: process.terminationStatus,
                output: errorTail.isEmpty ? outputTail : errorTail
            )
        }
    }

    func terminate() {
        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        completionContinuation?.resume(returning: 1)
        completionContinuation = nil
        for (_, continuation) in pendingResponses {
            continuation.resume(throwing: CancellationError())
        }
        pendingResponses.removeAll()
        pendingResponseTimeouts.values.forEach { $0.cancel() }
        pendingResponseTimeouts.removeAll()
        if process.isRunning {
            process.terminate()
        }
    }

    private func waitForProcessExitAndFailPendingRequests() async {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                self.process.waitUntilExit()
                continuation.resume()
            }
        }
        didProcessExit = true
        debugLog.log("agent-rpc", "process-exit status=\(process.terminationStatus)")
        finishExpectedCompletion(exitCode: process.terminationStatus == 0 ? 0 : 1)
        let error = await processExitError()
        for (_, continuation) in pendingResponses {
            continuation.resume(throwing: error)
        }
        pendingResponses.removeAll()
        pendingResponseTimeouts.values.forEach { $0.cancel() }
        pendingResponseTimeouts.removeAll()
    }

    private func processExitError() async -> AgentDriverError {
        await AgentDriverError.nonZeroExit(
            tool: invocation.transport.displayName,
            exitCode: process.terminationStatus,
            output: recentProcessOutput()
        )
    }

    private func timeoutError(method: String) async -> AgentDriverError {
        await AgentDriverError.protocolTimeout(
            tool: invocation.transport.displayName,
            method: method,
            output: recentProcessOutput()
        )
    }

    private func recentProcessOutput() async -> String {
        let errorText = await errorBuffer.currentText()
        if !errorText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return errorText
        }
        return await outputBuffer.currentText()
    }

    private func write(_ payload: [String: Any]) throws {
        let data = try JSONSerialization.data(withJSONObject: payload)
        stdin.fileHandleForWriting.write(data)
        stdin.fileHandleForWriting.write(Data([UInt8(ascii: "\n")]))
    }

    private func handleServerRequest(_ payload: [String: Any]) throws {
        guard let id = payload["id"],
              let method = payload["method"] as? String else {
            return
        }
        debugLog.log("agent-rpc", "server-request id=\(id) method=\(method)")
        let result: [String: Any]
        switch method {
        case "item/commandExecution/requestApproval", "execCommandApproval":
            result = ["decision": "accept"]
        case "item/fileChange/requestApproval", "applyPatchApproval":
            result = ["decision": "accept"]
        case "item/permissions/requestApproval":
            result = ["permissions": [:], "scope": "turn"]
        case "item/tool/requestUserInput", "mcpServer/elicitation/request":
            result = ["answers": [:]]
        case "item/tool/call":
            result = ["contentItems": [], "success": false]
        default:
            result = [:]
        }
        try write([
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        ])
    }

    private func handleResponse(_ payload: [String: Any]) {
        guard let id = intValue(from: payload["id"]) else {
            return
        }
        guard let continuation = pendingResponses.removeValue(forKey: id) else {
            debugLog.log("agent-rpc", "response-unmatched id=\(id) pending=\(pendingResponses.keys.sorted())")
            return
        }
        pendingResponseTimeouts.removeValue(forKey: id)?.cancel()
        if let error = payload["error"] {
            debugLog.log("agent-rpc", "response-error id=\(id)")
            continuation.resume(throwing: AgentDriverError.launchFailed(extractText(fromAny: error) ?? "Agent protocol request failed."))
        } else {
            debugLog.log("agent-rpc", "response id=\(id)")
            continuation.resume(returning: payload["result"].flatMap(AgentJSONValue.init) ?? .object([:]))
        }
    }

    private func handleNotification(
        _ payload: AgentJSONValue,
        onNotification: @escaping @Sendable (AgentJSONValue) async -> Void
    ) async {
        await onNotification(payload)
        guard let object = payload.objectValue,
              let method = object["method"]?.stringValue else {
            return
        }
        let params = object["params"] ?? .object([:])
        if method == "turn/completed" || method == "item/agentMessage/delta" || method == "error" {
            debugLog.log("agent-rpc", "notification method=\(method)")
        }
        switch method {
        case "turn/completed":
            if matchesExpectedCompletion(params: params) {
                finishExpectedCompletion(exitCode: 0)
            } else if let key = turnCompletionKey(params: params) {
                completedTurnKeys.insert(key)
            }
        case "error":
            finishExpectedCompletion(exitCode: 1)
        case "session/prompt_complete", "session/prompt/complete", "session/done":
            finishExpectedCompletion(exitCode: 0)
        case "session/error":
            finishExpectedCompletion(exitCode: 1)
        default:
            break
        }
    }

    private func matchesExpectedCompletion(params: AgentJSONValue) -> Bool {
        guard let expectedCompletion else {
            return false
        }
        let object = params.objectValue ?? [:]
        let threadID = object["threadId"]?.stringValue
        guard threadID == nil || threadID == expectedCompletion.threadID else {
            return false
        }
        guard let expectedTurnID = expectedCompletion.turnID else {
            return true
        }
        let turn = object["turn"]?.objectValue
        return turn?["id"]?.stringValue == expectedTurnID
    }

    private func finishExpectedCompletion(exitCode: Int32) {
        guard let continuation = completionContinuation else {
            completedExitCode = exitCode
            return
        }
        completionContinuation = nil
        continuation.resume(returning: exitCode)
    }

    private func cancelExpectedCompletion() {
        isCompletionCancelled = true
        finishExpectedCompletion(exitCode: 1)
        terminate()
    }

    private func turnCompletionKey(params: AgentJSONValue) -> String? {
        let object = params.objectValue ?? [:]
        guard let threadID = object["threadId"]?.stringValue else {
            return nil
        }
        let turnID = object["turn"]?.objectValue?["id"]?.stringValue
        return completionKey(threadID: threadID, turnID: turnID)
    }

    private func completionKey(threadID: String, turnID: String?) -> String {
        "\(threadID)#\(turnID ?? "")"
    }
}

private func extractText(from payload: AgentJSONValue) -> String? {
    switch payload {
    case .string(let value):
        return normalizedNonEmptyString(value)
    case .object(let object):
        let directKeys = ["text", "content", "message", "delta", "result", "summary"]
        for key in directKeys {
            if let text = object[key]?.stringValue {
                return text
            }
        }

        for key in ["item", "message"] {
            if let text = object[key].flatMap(extractText(from:)) {
                return text
            }
        }

        for key in ["content", "parts", "summary"] {
            if let array = object[key]?.arrayValue {
                let text = array.compactMap(extractText(from:)).joined()
                if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    return text
                }
            }
        }
        return nil
    case .array(let array):
        let text = array.compactMap(extractText(from:)).joined()
        return normalizedNonEmptyString(text)
    case .number, .bool, .null:
        return nil
    }
}

private func firstString(in object: [String: AgentJSONValue], keys: [String]) -> String? {
    for key in keys {
        if let value = object[key]?.stringValue {
            return value
        }
    }
    return nil
}

private func stableAgentSummaryID(_ text: String) -> String {
    String(text.unicodeScalars.reduce(UInt64(14_695_981_039_346_656_037)) { partial, scalar in
        (partial ^ UInt64(scalar.value)) &* 1_099_511_628_211
    }, radix: 16)
}

private func worktreeReviewStatus(fromAgentStatus rawStatus: String?) -> WorktreeReviewFileStatus {
    switch rawStatus?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
    case "add", "added", "create", "created", "new":
        return .added
    case "delete", "deleted", "remove", "removed":
        return .deleted
    case "rename", "renamed", "move", "moved":
        return .renamed
    case "copy", "copied":
        return .copied
    case "typechange", "type_changed":
        return .typeChanged
    case "modify", "modified", "update", "updated", "edit", "edited":
        return .modified
    default:
        return .unknown
    }
}

private func intValue(from rawValue: Any?) -> Int? {
    switch rawValue {
    case let value as Int:
        return value
    case let value as NSNumber:
        return value.intValue
    case let value as String:
        return Int(value)
    default:
        return nil
    }
}

private func extractText(fromAny payload: Any) -> String? {
    if let text = normalizedNonEmptyString(payload as? String) {
        return text
    }
    if let object = payload as? [String: Any] {
        return extractText(from: object)
    }
    if let array = payload as? [Any] {
        let text = array.compactMap(extractText(fromAny:)).joined()
        return normalizedNonEmptyString(text)
    }
    return nil
}

private extension Dictionary where Key == String, Value == Any {
    func filteringNilValues() -> [String: Any] {
        filter { !($0.value is OptionalProtocol && ($0.value as? OptionalProtocol)?.isNil == true) }
    }
}

private protocol OptionalProtocol {
    var isNil: Bool { get }
}

extension Optional: OptionalProtocol {
    var isNil: Bool {
        self == nil
    }
}

private enum StructuredAgentProcessRunner {
    static func run(
        toolName: String,
        invocation: AgentDriverInvocation,
        onPayload: @escaping @Sendable ([String: Any]) async -> Void,
        onExit: @escaping @Sendable (Int32) async -> Void
    ) async throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: invocation.executablePath)
        process.arguments = invocation.arguments
        process.currentDirectoryURL = URL(fileURLWithPath: invocation.currentDirectory, isDirectory: true)
        process.environment = invocation.environment

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        let outputBuffer = AgentProcessOutputBuffer()
        let errorBuffer = AgentProcessOutputBuffer()

        stdout.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard data.isEmpty == false else { return }
            Task {
                let lines = await outputBuffer.append(data)
                for line in lines {
                    if let payload = parseJSONObjectLine(line) {
                        await onPayload(payload)
                    }
                }
            }
        }

        stderr.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard data.isEmpty == false else { return }
            Task {
                _ = await errorBuffer.append(data)
            }
        }

        do {
            try process.run()
        } catch {
            stdout.fileHandleForReading.readabilityHandler = nil
            stderr.fileHandleForReading.readabilityHandler = nil
            throw AgentDriverError.launchFailed(error.localizedDescription)
        }

        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                DispatchQueue.global(qos: .utility).async {
                    process.waitUntilExit()
                    continuation.resume()
                }
            }
        } onCancel: {
            process.terminate()
        }

        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        try Task.checkCancellation()
        let outputTail = await outputBuffer.drainText()
        let errorTail = await errorBuffer.drainText()
        for line in outputTail.split(whereSeparator: \.isNewline) {
            if let payload = parseJSONObjectLine(String(line)) {
                await onPayload(payload)
            }
        }
        let exitCode = process.terminationStatus
        await onExit(exitCode)
        guard exitCode == 0 else {
            throw AgentDriverError.nonZeroExit(
                tool: toolName,
                exitCode: exitCode,
                output: errorTail.isEmpty ? outputTail : errorTail
            )
        }
    }
}

private actor AgentProcessOutputBuffer {
    private var data = Data()

    func append(_ chunk: Data) -> [String] {
        data.append(chunk)
        var lines: [String] = []
        while let newlineIndex = data.firstIndex(of: UInt8(ascii: "\n")) {
            let lineData = data.subdata(in: 0..<newlineIndex)
            data.removeSubrange(0...newlineIndex)
            guard lineData.isEmpty == false,
                  let line = String(data: lineData, encoding: .utf8) else {
                continue
            }
            lines.append(line)
        }
        return lines
    }

    func drainText() -> String {
        guard data.isEmpty == false else {
            return ""
        }
        let text = String(data: data, encoding: .utf8) ?? ""
        data.removeAll()
        return text
    }

    func currentText() -> String {
        String(data: data, encoding: .utf8) ?? ""
    }
}

private func parseJSONObjectLine(_ line: String) -> [String: Any]? {
    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
    guard trimmed.hasPrefix("{"),
          let data = trimmed.data(using: .utf8) else {
        return nil
    }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
}

private func role(from rawRole: String?) -> AgentRole {
    switch rawRole?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
    case "user":
        return .user
    case "assistant", "assistant_message":
        return .assistant
    case "system":
        return .system
    case "tool", "tool_use", "tool_result":
        return .tool
    case "error":
        return .error
    default:
        return .assistant
    }
}

private func extractText(from payload: [String: Any]) -> String? {
    let directKeys = ["text", "content", "message", "delta", "result", "summary"]
    for key in directKeys {
        if let text = normalizedNonEmptyString(payload[key] as? String) {
            return text
        }
    }

    if let item = payload["item"] as? [String: Any],
       let text = extractText(from: item) {
        return text
    }
    if let message = payload["message"] as? [String: Any],
       let text = extractText(from: message) {
        return text
    }

    for key in ["content", "parts"] {
        if let array = payload[key] as? [[String: Any]] {
            let text = array.compactMap { extractText(from: $0) }.joined()
            if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return text
            }
        }
    }

    return nil
}
