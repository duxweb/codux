import Foundation
import SQLite3

struct AIToolSessionCapabilities: Sendable {
    var canOpen: Bool
    var canRename: Bool
    var canRemove: Bool

    static let none = AIToolSessionCapabilities(canOpen: false, canRename: false, canRemove: false)
}

enum AIToolSessionControlError: LocalizedError {
    case unsupportedOperation
    case missingSessionID
    case sessionNotFound
    case storageFailure(String)

    var errorDescription: String? {
        switch self {
        case .unsupportedOperation:
            return String(localized: "ai.session.action.unsupported", defaultValue: "This action is not supported by the current tool.", bundle: .module)
        case .missingSessionID:
            return String(localized: "ai.session.identifier.missing", defaultValue: "Missing session identifier.", bundle: .module)
        case .sessionNotFound:
            return String(localized: "ai.session.record.not_found", defaultValue: "Matching session record was not found.", bundle: .module)
        case let .storageFailure(message):
            return message
        }
    }
}

protocol AIToolDriver: Sendable {
    var id: String { get }
    var aliases: Set<String> { get }
    var runtimeRefreshInterval: TimeInterval { get }
    var isRealtimeTool: Bool { get }
    var prefersHookDrivenResponseState: Bool { get }
    var freezesDisplayTokensWhileResponding: Bool { get }
    var seedsObservedBaselineOnFreshLaunch: Bool { get }
    var allowsRuntimeExternalSessionSwitch: Bool { get }
    var usesHistoricalExternalSessionHintForRuntimeProbe: Bool { get }
    var appliesGenericResponsePayloads: Bool { get }

    func matches(tool: String) -> Bool
    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor]
    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate?
    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate?
    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities
    func resumeCommand(for session: AISessionSummary) -> String?
    func renameSession(_ session: AISessionSummary, to title: String) throws
    func removeSession(_ session: AISessionSummary) throws
}

extension AIToolDriver {
    var prefersHookDrivenResponseState: Bool { false }
    var freezesDisplayTokensWhileResponding: Bool { false }
    var seedsObservedBaselineOnFreshLaunch: Bool { false }
    var allowsRuntimeExternalSessionSwitch: Bool { false }
    var usesHistoricalExternalSessionHintForRuntimeProbe: Bool { true }
    var appliesGenericResponsePayloads: Bool { true }

    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = descriptor
        _ = projects
        _ = liveEnvelopes
        return nil
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = kind
        _ = payloadData
        _ = projects
        _ = liveEnvelopes
        _ = existingRuntime
        return nil
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
        throw AIToolSessionControlError.unsupportedOperation
    }

    func removeSession(_ session: AISessionSummary) throws {
        _ = session
        throw AIToolSessionControlError.unsupportedOperation
    }
}

struct AIToolDriverFactory: Sendable {
    static let shared = AIToolDriverFactory()

    private let drivers: [AIToolDriver] = [
        ClaudeToolDriver(),
        CodexToolDriver(),
        OpenCodeToolDriver(),
        GeminiToolDriver(),
    ]

    func driver(for tool: String?) -> AIToolDriver? {
        guard let tool, !tool.isEmpty else {
            return nil
        }
        return drivers.first { $0.matches(tool: tool) }
    }

    func canonicalToolName(_ tool: String) -> String {
        driver(for: tool)?.id ?? tool
    }

    func runtimeRefreshInterval(for tool: String) -> TimeInterval {
        driver(for: tool)?.runtimeRefreshInterval ?? 0.55
    }

    func isRealtimeTool(_ tool: String) -> Bool {
        driver(for: tool)?.isRealtimeTool ?? false
    }

    func prefersHookDrivenResponseState(for tool: String) -> Bool {
        driver(for: tool)?.prefersHookDrivenResponseState ?? false
    }

    func freezesDisplayTokensWhileResponding(for tool: String) -> Bool {
        driver(for: tool)?.freezesDisplayTokensWhileResponding ?? false
    }

    func seedsObservedBaselineOnFreshLaunch(for tool: String) -> Bool {
        driver(for: tool)?.seedsObservedBaselineOnFreshLaunch ?? false
    }

    func allowsRuntimeExternalSessionSwitch(for tool: String) -> Bool {
        driver(for: tool)?.allowsRuntimeExternalSessionSwitch ?? false
    }

    func appliesGenericResponsePayloads(for tool: String) -> Bool {
        driver(for: tool)?.appliesGenericResponsePayloads ?? true
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        for driver in drivers {
            if let update = await driver.handleRuntimeSocketEvent(
                kind: kind,
                payloadData: payloadData,
                projects: projects,
                liveEnvelopes: liveEnvelopes,
                existingRuntime: existingRuntime
            ) {
                return update
            }
        }
        return nil
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        driver(for: session.lastTool)?.sessionCapabilities(for: session) ?? .none
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        driver(for: session.lastTool)?.resumeCommand(for: session)
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        guard let driver = driver(for: session.lastTool) else {
            throw AIToolSessionControlError.unsupportedOperation
        }
        try driver.renameSession(session, to: title)
    }

    func removeSession(_ session: AISessionSummary) throws {
        guard let driver = driver(for: session.lastTool) else {
            throw AIToolSessionControlError.unsupportedOperation
        }
        try driver.removeSession(session)
    }
}

private actor AIToolRuntimeEventDeduper {
    static let shared = AIToolRuntimeEventDeduper()

    private var lastSeenAtByKey: [String: Date] = [:]

    func shouldAccept(key: String, ttl: TimeInterval) -> Bool {
        let now = Date()
        lastSeenAtByKey = lastSeenAtByKey.filter { now.timeIntervalSince($0.value) < max(ttl * 4, 2) }
        if let previous = lastSeenAtByKey[key],
           now.timeIntervalSince(previous) < ttl {
            return false
        }
        lastSeenAtByKey[key] = now
        return true
    }
}

private struct ClaudeToolDriver: AIToolDriver {
    let id = "claude"
    let aliases: Set<String> = ["claude", "claude-code"]
    let runtimeRefreshInterval: TimeInterval = 0.9
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let allowsRuntimeExternalSessionSwitch = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        AIRuntimeSourceLocator.claudeProjectLogURLs().map {
            AIToolRuntimeSourceDescriptor(path: $0.path, watchKind: .file)
        }
    }

    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate? {
        guard descriptor.watchKind == .file else {
            return nil
        }

        let fileURL = URL(fileURLWithPath: descriptor.path)
        let interruptEvents = await ClaudeRuntimeInterruptWatchCache.shared.process(
            fileURL: fileURL,
            projectPath: Optional<String>.none
        )
        guard !interruptEvents.isEmpty else {
            return nil
        }

        var responsePayloads: [AIResponseStatePayload] = []
        var runtimeSnapshotsBySessionID: [UUID: AIRuntimeContextSnapshot] = [:]

        for interruptEvent in interruptEvents {
            guard let envelope = liveEnvelopes.first(where: {
                canonicalTool($0.tool) == id
                    && $0.externalSessionID == interruptEvent.externalSessionID
                    && UUID(uuidString: $0.projectId).flatMap { projectID in
                        projects.first(where: { $0.id == projectID })
                    } != nil
            }),
                  let sessionID = UUID(uuidString: envelope.sessionId),
                  let projectID = UUID(uuidString: envelope.projectId) else {
                continue
            }

            AppDebugLog.shared.log(
                "claude-watcher",
                "interrupt session=\(sessionID.uuidString) external=\(interruptEvent.externalSessionID) updatedAt=\(interruptEvent.updatedAt)"
            )

            responsePayloads.append(
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: envelope.sessionInstanceId,
                    invocationId: envelope.invocationId,
                    projectId: projectID.uuidString,
                    projectPath: envelope.projectPath,
                    tool: id,
                    responseState: .idle,
                    updatedAt: interruptEvent.updatedAt,
                    source: .watcher
                )
            )

            runtimeSnapshotsBySessionID[sessionID] = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: interruptEvent.externalSessionID,
                model: envelope.model,
                inputTokens: max(0, envelope.inputTokens ?? 0),
                outputTokens: max(0, envelope.outputTokens ?? 0),
                totalTokens: max(0, envelope.totalTokens ?? 0),
                updatedAt: interruptEvent.updatedAt,
                responseState: .idle,
                wasInterrupted: true,
                hasCompletedTurn: false,
                source: .watcher
            )
        }

        guard !responsePayloads.isEmpty || !runtimeSnapshotsBySessionID.isEmpty else {
            return nil
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: runtimeSnapshotsBySessionID
        )
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "claude-hook",
              let envelope = try? JSONDecoder().decode(ClaudeHookRuntimeEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.dmuxSessionId) else {
            return nil
        }

        let dedupeKey = "claude|\(envelope.event)|\(sessionID.uuidString)|\(payloadHash(envelope.payload))"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.0) else {
            AppDebugLog.shared.log(
                "claude-hook",
                "drop duplicate event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "claude-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "claude-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let payloadObject: [String: Any]? = envelope.payload.data(using: .utf8)
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let externalSessionID = stringValue(in: payloadObject, key: "session_id")
            ?? existingSnapshot?.externalSessionID
            ?? liveEnvelope?.externalSessionID
        let projectPath = envelope.dmuxProjectPath
            ?? liveEnvelope?.projectPath

        switch envelope.event {
        case "UserPromptSubmit":
            if let projectPath, let externalSessionID, !projectPath.isEmpty, !externalSessionID.isEmpty {
                let fileURL = AIRuntimeSourceLocator.claudeSessionLogURL(
                    projectPath: projectPath,
                    externalSessionID: externalSessionID
                )
                await ClaudeRuntimeInterruptWatchCache.shared.prime(
                    fileURL: fileURL,
                    externalSessionID: externalSessionID
                )
                AppDebugLog.shared.log(
                    "claude-hook",
                    "prime interrupt watcher session=\(sessionID.uuidString) external=\(externalSessionID) file=\(fileURL.lastPathComponent)"
                )
            } else {
                AppDebugLog.shared.log(
                    "claude-hook",
                    "skip prime session=\(sessionID.uuidString) reason=missing-path-or-external"
                )
            }
        case "Notification":
            let notificationType = stringValue(in: payloadObject, key: "notification_type") ?? "unknown"
            AppDebugLog.shared.log(
                "claude-hook",
                "notification session=\(sessionID.uuidString) type=\(notificationType)"
            )
        case "Stop":
            AppDebugLog.shared.log(
                "claude-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
            let settledSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: existingSnapshot?.model ?? liveEnvelope?.model,
                inputTokens: max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                outputTokens: max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0),
                totalTokens: max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                updatedAt: max(envelope.receivedAt, existingSnapshot?.updatedAt ?? 0, liveEnvelope?.updatedAt ?? 0),
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: true,
                source: .hook
            )
            return AIToolRuntimeIngressUpdate(
                runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
            )
        case "Idle", "SessionEnd":
            AppDebugLog.shared.log(
                "claude-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
            let settledSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: existingSnapshot?.model ?? liveEnvelope?.model,
                inputTokens: max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                outputTokens: max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0),
                totalTokens: max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                updatedAt: max(envelope.receivedAt, existingSnapshot?.updatedAt ?? 0, liveEnvelope?.updatedAt ?? 0),
                responseState: .idle,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
            return AIToolRuntimeIngressUpdate(
                runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
            )
        case "StopFailure", "SessionStart", "PreToolUse", "PostToolUse", "PermissionRequest":
            AppDebugLog.shared.log(
                "claude-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
        default:
            AppDebugLog.shared.log(
                "claude-hook",
                "ignore event=\(envelope.event) session=\(sessionID.uuidString)"
            )
        }

        return nil
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: false, canRemove: true)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            return nil
        }
        return "claude --resume \(shellQuoted(sessionID))"
    }

    func removeSession(_ session: AISessionSummary) throws {
        let targetSessionID = session.externalSessionID ?? session.sessionID.uuidString
        let candidates = AIRuntimeSourceLocator.claudeProjectLogURLs().filter { fileURL in
            if fileURL.lastPathComponent == "\(targetSessionID).jsonl" {
                return true
            }
            guard let text = try? String(contentsOf: fileURL, encoding: .utf8) else {
                return false
            }
            return text.contains("\"sessionId\":\"\(targetSessionID)\"")
        }
        guard !candidates.isEmpty else {
            throw AIToolSessionControlError.sessionNotFound
        }

        let fileManager = FileManager.default
        for fileURL in candidates {
            try fileManager.removeItem(at: fileURL)
        }
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func stringValue(in object: [String: Any]?, key: String) -> String? {
        guard let object,
              let value = object[key] as? String,
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func payloadHash(_ payload: String) -> Int {
        var hasher = Hasher()
        hasher.combine(payload.count)
        hasher.combine(payload.prefix(160))
        return hasher.finalize()
    }
}

private struct CodexToolDriver: AIToolDriver {
    let id = "codex"
    let aliases: Set<String> = ["codex"]
    let runtimeRefreshInterval: TimeInterval = 0.55
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let allowsRuntimeExternalSessionSwitch = true
    let appliesGenericResponsePayloads = false

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        []
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "codex-hook",
              let envelope = try? JSONDecoder().decode(CodexHookRuntimeEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.dmuxSessionId),
              let projectID = UUID(uuidString: envelope.dmuxProjectId),
              let payloadText = envelope.payload.data(using: .utf8),
              let payloadObject = try? JSONSerialization.jsonObject(with: payloadText) as? [String: Any] else {
            return nil
        }

        let dedupeKey = "codex|\(envelope.event)|\(sessionID.uuidString)|\(payloadHash(envelope.payload))"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.2) else {
            AppDebugLog.shared.log(
                "codex-hook",
                "drop duplicate event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "codex-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "codex-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let externalSessionID = stringValue(in: payloadObject, key: "session_id")
            ?? existingSnapshot?.externalSessionID
            ?? liveEnvelope?.externalSessionID
        let model = stringValue(in: payloadObject, key: "model")
            ?? existingSnapshot?.model
            ?? liveEnvelope?.model
        let canReuseExistingTotals = shouldReuseExistingTotals(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot
        )
        let inheritedInputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
            : max(0, liveEnvelope?.inputTokens ?? 0)
        let inheritedOutputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
            : max(0, liveEnvelope?.outputTokens ?? 0)
        let inheritedTotalTokens = canReuseExistingTotals
            ? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)
            : max(0, liveEnvelope?.totalTokens ?? 0)
        let updatedAt = max(
            envelope.receivedAt,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        if let existingSnapshot,
           existingSnapshot.externalSessionID == externalSessionID,
           existingSnapshot.responseState == .responding,
           envelope.event == "UserPromptSubmit",
           updatedAt <= existingSnapshot.updatedAt {
            AppDebugLog.shared.log(
                "codex-hook",
                "drop stale event=\(envelope.event) session=\(sessionID.uuidString) updatedAt=\(updatedAt) existingAt=\(existingSnapshot.updatedAt)"
            )
            return nil
        }

        let runtimeSnapshot: AIRuntimeContextSnapshot
        let responsePayload: AIResponseStatePayload
        switch envelope.event {
        case "UserPromptSubmit":
            await CodexRuntimeResponseLatch.shared.markResponding(
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID,
                updatedAt: updatedAt
            )
            runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: model,
                inputTokens: inheritedInputTokens,
                outputTokens: inheritedOutputTokens,
                totalTokens: inheritedTotalTokens,
                updatedAt: updatedAt,
                responseState: .responding,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
            responsePayload = AIResponseStatePayload(
                sessionId: sessionID.uuidString,
                sessionInstanceId: nil,
                invocationId: nil,
                projectId: projectID.uuidString,
                projectPath: nil,
                tool: id,
                responseState: .responding,
                updatedAt: updatedAt,
                source: .hook
            )
        case "Stop":
            let transcriptPath = stringValue(in: payloadObject, key: "transcript_path")
            let parsedState = await resolveCodexStopRuntimeState(transcriptPath: transcriptPath)
            let stopLooksSettled = (parsedState?.responseState == .idle)
                && (parsedState?.wasInterrupted != true)
            let hasDefinitiveStop = (parsedState?.wasInterrupted == true)
                || (parsedState?.hasCompletedTurn == true)
                || stopLooksSettled
            await CodexRuntimeResponseLatch.shared.releaseIfDefinitiveStop(
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID,
                wasInterrupted: parsedState?.wasInterrupted ?? false,
                hasCompletedTurn: (parsedState?.hasCompletedTurn ?? false) || stopLooksSettled
            )
            AppDebugLog.shared.log(
                "codex-hook",
                "stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") transcript=\(transcriptPath ?? "nil") parsedModel=\(parsedState?.model ?? model ?? "nil") parsedTokens=\(parsedState?.totalTokens.map(String.init) ?? "nil") interrupted=\(parsedState?.wasInterrupted == true) completed=\(parsedState?.hasCompletedTurn == true)"
            )
            runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: parsedState?.model ?? model,
                inputTokens: parsedState?.totalTokens ?? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                outputTokens: 0,
                totalTokens: parsedState?.totalTokens ?? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                updatedAt: max(updatedAt, parsedState?.updatedAt ?? 0),
                responseState: hasDefinitiveStop ? .idle : nil,
                wasInterrupted: parsedState?.wasInterrupted ?? false,
                hasCompletedTurn: (parsedState?.hasCompletedTurn ?? false) || stopLooksSettled,
                source: .hook
            )
            if hasDefinitiveStop {
                responsePayload = AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: projectID.uuidString,
                    projectPath: nil,
                    tool: id,
                    responseState: .idle,
                    updatedAt: runtimeSnapshot.updatedAt,
                    source: .hook
                )
            } else {
                AppDebugLog.shared.log(
                    "codex-hook",
                    "defer stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=non-definitive"
                )
                return AIToolRuntimeIngressUpdate(
                    runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
                )
            }
        default:
            AppDebugLog.shared.log("codex-hook", "ignore event=\(envelope.event) session=\(sessionID.uuidString)")
            return nil
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: [responsePayload],
            runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
        )
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: true, canRemove: true)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        let sessionID = session.externalSessionID ?? session.sessionID.uuidString
        return "codex resume \(shellQuoted(sessionID))"
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        let sessionID = session.externalSessionID ?? session.sessionID.uuidString
        let databaseURL = AIRuntimeSourceLocator.codexDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            let sql = "UPDATE threads SET title = ? WHERE id = ?;"
            try executeSQLite(
                db: db,
                sql: sql,
                bindings: [
                    .text(title),
                    .text(sessionID),
                ]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    func removeSession(_ session: AISessionSummary) throws {
        let sessionID = session.externalSessionID ?? session.sessionID.uuidString
        let now = Int64(Date().timeIntervalSince1970)
        let databaseURL = AIRuntimeSourceLocator.codexDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            let sql = "UPDATE threads SET archived = 1, archived_at = ?, updated_at = ? WHERE id = ?;"
            try executeSQLite(
                db: db,
                sql: sql,
                bindings: [
                    .int64(now),
                    .int64(now),
                    .text(sessionID),
                ]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func stringValue(in object: [String: Any]?, key: String) -> String? {
        guard let object else {
            return nil
        }
        guard let value = object[key] as? String, !value.isEmpty else {
            return nil
        }
        return value
    }

    private func payloadHash(_ payload: String) -> Int {
        var hasher = Hasher()
        hasher.combine(payload.count)
        hasher.combine(payload.prefix(160))
        return hasher.finalize()
    }

    private func shouldReuseExistingTotals(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?
    ) -> Bool {
        guard let externalSessionID, !externalSessionID.isEmpty else {
            return false
        }
        if liveEnvelope?.externalSessionID == externalSessionID {
            return true
        }
        if existingSnapshot?.externalSessionID == externalSessionID {
            return true
        }
        return false
    }
}

private struct OpenCodeToolDriver: AIToolDriver {
    let id = "opencode"
    let aliases: Set<String> = ["opencode"]
    let runtimeRefreshInterval: TimeInterval = 0.75
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let freezesDisplayTokensWhileResponding = true
    let allowsRuntimeExternalSessionSwitch = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        var descriptors: [AIToolRuntimeSourceDescriptor] = []
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        if FileManager.default.fileExists(atPath: databaseURL.path) {
            descriptors.append(AIToolRuntimeSourceDescriptor(path: databaseURL.path, watchKind: .file))
        }

        let walURL = URL(fileURLWithPath: databaseURL.path + "-wal")
        if FileManager.default.fileExists(atPath: walURL.path) {
            descriptors.append(AIToolRuntimeSourceDescriptor(path: walURL.path, watchKind: .file))
        }
        return descriptors
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "opencode-runtime",
              let envelope = try? JSONDecoder().decode(AIToolUsageEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.sessionId),
              let projectID = UUID(uuidString: envelope.projectId) else {
            return nil
        }

        let dedupeKey = "opencode|\(sessionID.uuidString)|\(envelope.responseState?.rawValue ?? "nil")|\(Int(envelope.updatedAt))|\(envelope.totalTokens ?? -1)"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.0) else {
            AppDebugLog.shared.log(
                "opencode-driver",
                "drop duplicate kind=\(kind) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "opencode-driver",
                "ignore stale kind=\(kind) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "opencode-driver",
                "ignore stale kind=\(kind) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let externalSessionID = normalizedSessionID(envelope.externalSessionID)
            ?? existingSnapshot?.externalSessionID
            ?? normalizedSessionID(liveEnvelope?.externalSessionID)
        let projectPath = normalizedSessionID(envelope.projectPath)
            ?? normalizedSessionID(liveEnvelope?.projectPath)
        let switchedExternalSession =
            externalSessionID != nil
            && externalSessionID != existingSnapshot?.externalSessionID
        let resolvedHistoricalSnapshot = resolvedExternalSessionSnapshot(
            projectPath: projectPath,
            externalSessionID: externalSessionID,
            shouldResolve: switchedExternalSession
                || ((envelope.totalTokens ?? 0) == 0 && envelope.responseState == .responding)
        )
        let model = normalizedSessionID(envelope.model)
            ?? resolvedHistoricalSnapshot?.model
            ?? existingSnapshot?.model
            ?? normalizedSessionID(liveEnvelope?.model)
        let canReuseExistingTotals = shouldReuseExistingTotals(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot
        )
        let inheritedInputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
            : 0
        let inheritedOutputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
            : 0
        let inheritedTotalTokens = canReuseExistingTotals
            ? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)
            : 0
        let updatedAt = max(
            envelope.updatedAt,
            resolvedHistoricalSnapshot?.updatedAt ?? 0,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        if let existingSnapshot,
           existingSnapshot.externalSessionID == externalSessionID,
           updatedAt < existingSnapshot.updatedAt,
           envelope.responseState != .responding {
            AppDebugLog.shared.log(
                "opencode-driver",
                "drop stale kind=\(kind) session=\(sessionID.uuidString) updatedAt=\(updatedAt) existingAt=\(existingSnapshot.updatedAt)"
            )
            return nil
        }

        let runtimeSnapshot = AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: externalSessionID,
            model: model,
            inputTokens: max(envelope.inputTokens ?? 0, resolvedHistoricalSnapshot?.inputTokens ?? 0, inheritedInputTokens),
            outputTokens: max(envelope.outputTokens ?? 0, resolvedHistoricalSnapshot?.outputTokens ?? 0, inheritedOutputTokens),
            totalTokens: max(envelope.totalTokens ?? 0, resolvedHistoricalSnapshot?.totalTokens ?? 0, inheritedTotalTokens),
            updatedAt: updatedAt,
            responseState: envelope.responseState,
            sessionOrigin: resolvedHistoricalSnapshot?.sessionOrigin ?? .unknown,
            source: .socket
        )

        let responsePayloads: [AIResponseStatePayload]
        if let responseState = envelope.responseState {
            responsePayloads = [
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: envelope.sessionInstanceId,
                    invocationId: envelope.invocationId,
                    projectId: projectID.uuidString,
                    projectPath: envelope.projectPath,
                    tool: id,
                    responseState: responseState,
                    updatedAt: updatedAt,
                    source: .socket
                ),
            ]
        } else {
            responsePayloads = []
        }

        AppDebugLog.shared.log(
            "opencode-driver",
            "socket kind=\(kind) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") model=\(model ?? "nil") response=\(envelope.responseState?.rawValue ?? "nil") total=\(runtimeSnapshot.totalTokens) reuseTotals=\(canReuseExistingTotals) origin=\(runtimeSnapshot.sessionOrigin.rawValue)"
        )

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
        )
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: true, canRemove: true)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            return nil
        }
        return "opencode --session \(shellQuoted(sessionID))"
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            throw AIToolSessionControlError.missingSessionID
        }
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            let sql = "UPDATE session SET title = ? WHERE id = ?;"
            try executeSQLite(
                db: db,
                sql: sql,
                bindings: [
                    .text(title),
                    .text(sessionID),
                ]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    func removeSession(_ session: AISessionSummary) throws {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            throw AIToolSessionControlError.missingSessionID
        }
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            try executeSQLite(
                db: db,
                sql: "PRAGMA foreign_keys = ON;",
                bindings: []
            )
            try executeSQLite(
                db: db,
                sql: "DELETE FROM session WHERE id = ?;",
                bindings: [.text(sessionID)]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func normalizedSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func shouldReuseExistingTotals(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?
    ) -> Bool {
        guard let externalSessionID, !externalSessionID.isEmpty else {
            return false
        }
        if normalizedSessionID(liveEnvelope?.externalSessionID) == externalSessionID {
            return true
        }
        if existingSnapshot?.externalSessionID == externalSessionID {
            return true
        }
        return false
    }

    private func resolvedExternalSessionSnapshot(
        projectPath: String?,
        externalSessionID: String?,
        shouldResolve: Bool
    ) -> AIRuntimeContextSnapshot? {
        guard shouldResolve,
              let projectPath,
              let externalSessionID else {
            return nil
        }

        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        guard FileManager.default.fileExists(atPath: databaseURL.path) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &db) == SQLITE_OK,
              let db else {
            if db != nil {
                sqlite3_close(db)
            }
            return nil
        }
        defer { sqlite3_close(db) }

        return try? fetchOpenCodeSessionSnapshot(
            db: db,
            projectPath: projectPath,
            externalSessionID: externalSessionID
        )
    }
}

private func fetchOpenCodeSessionSnapshot(
    db: OpaquePointer,
    projectPath: String,
    externalSessionID: String
) throws -> AIRuntimeContextSnapshot? {
    let sql = """
    SELECT json_extract(m.data, '$.modelID') AS model,
           COALESCE(json_extract(m.data, '$.tokens.input'), 0) AS input_tokens,
           COALESCE(json_extract(m.data, '$.tokens.output'), 0) AS output_tokens,
           COALESCE(json_extract(m.data, '$.tokens.cache.read'), 0) AS cache_read_tokens,
           COALESCE(json_extract(m.data, '$.tokens.cache.write'), 0) AS cache_write_tokens,
           COALESCE(json_extract(m.data, '$.tokens.total'), 0) AS total_tokens,
           COALESCE(json_extract(m.data, '$.time.completed'), json_extract(m.data, '$.time.created'), 0) AS completed_at,
           s.time_updated AS session_updated_at
    FROM session s
    LEFT JOIN message m ON m.session_id = s.id
    WHERE s.directory = ?
      AND s.id = ?
      AND s.time_archived IS NULL
    ORDER BY m.time_created DESC;
    """

    var statement: OpaquePointer?
    guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
          let statement else {
        throw AIToolSessionControlError.storageFailure(String(cString: sqlite3_errmsg(db)))
    }
    defer { sqlite3_finalize(statement) }

    sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_SESSION)
    sqlite3_bind_text(statement, 2, externalSessionID, -1, SQLITE_TRANSIENT_SESSION)

    var latestModel: String?
    var inputTokens = 0
    var outputTokens = 0
    var totalTokens = 0
    var updatedAt = 0.0
    var hadRow = false

    while sqlite3_step(statement) == SQLITE_ROW {
        hadRow = true
        if latestModel == nil, let rawModel = sqlite3_column_text(statement, 0) {
            let model = String(cString: rawModel)
            if !model.isEmpty {
                latestModel = model
            }
        }
        let input = Int(sqlite3_column_int64(statement, 1))
        let output = Int(sqlite3_column_int64(statement, 2))
        let cacheRead = Int(sqlite3_column_int64(statement, 3))
        let cacheWrite = Int(sqlite3_column_int64(statement, 4))
        let explicitTotal = Int(sqlite3_column_int64(statement, 5))
        inputTokens += input + cacheRead + cacheWrite
        outputTokens += output
        totalTokens += max(explicitTotal, input + output + cacheRead + cacheWrite)
        updatedAt = max(updatedAt, sqlite3_column_double(statement, 6) / 1000)
        updatedAt = max(updatedAt, sqlite3_column_double(statement, 7) / 1000)
    }

    guard hadRow else {
        return nil
    }

    return AIRuntimeContextSnapshot(
        tool: "opencode",
        externalSessionID: externalSessionID,
        model: latestModel,
        inputTokens: inputTokens,
        outputTokens: outputTokens,
        totalTokens: totalTokens,
        updatedAt: updatedAt,
        responseState: totalTokens > 0 ? .idle : nil,
        sessionOrigin: totalTokens > 0 ? .restored : .fresh,
        source: .probe
    )
}

private struct GeminiToolDriver: AIToolDriver {
    let id = "gemini"
    let aliases: Set<String> = ["gemini"]
    let runtimeRefreshInterval: TimeInterval = 0.75
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let freezesDisplayTokensWhileResponding = true
    let allowsRuntimeExternalSessionSwitch = true
    let seedsObservedBaselineOnFreshLaunch = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        let projectPath = envelope?.projectPath ?? project.path
        guard let chatsDirectoryURL = AIRuntimeSourceLocator.geminiChatsDirectoryURL(projectPath: projectPath),
              FileManager.default.fileExists(atPath: chatsDirectoryURL.path) else {
            return []
        }
        return [AIToolRuntimeSourceDescriptor(path: chatsDirectoryURL.path, watchKind: .directory)]
    }

    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate? {
        guard descriptor.watchKind == .directory else {
            return nil
        }

        let matchingProjectPaths = Set(projects.compactMap { project -> String? in
            guard AIRuntimeSourceLocator.geminiChatsDirectoryURL(projectPath: project.path)?.path == descriptor.path else {
                return nil
            }
            return project.path
        })
        guard !matchingProjectPaths.isEmpty else {
            return nil
        }

        var responsePayloads: [AIResponseStatePayload] = []
        var runtimeSnapshotsBySessionID: [UUID: AIRuntimeContextSnapshot] = [:]

        for envelope in liveEnvelopes {
            guard canonicalTool(envelope.tool) == id,
                  let sessionID = UUID(uuidString: envelope.sessionId),
                  let projectID = UUID(uuidString: envelope.projectId),
                  let projectPath = envelope.projectPath,
                  matchingProjectPaths.contains(projectPath),
                  let snapshot = resolvedSnapshot(
                      projectPath: projectPath,
                      liveEnvelope: envelope,
                      existingSnapshot: nil,
                      responseStateOverride: nil,
                      updatedAt: envelope.updatedAt,
                      marksCompletedTurn: envelope.responseState == .idle,
                      source: .probe
                  ) else {
                continue
            }

            runtimeSnapshotsBySessionID[sessionID] = snapshot
            if let responseState = snapshot.responseState {
                responsePayloads.append(
                    AIResponseStatePayload(
                        sessionId: sessionID.uuidString,
                        sessionInstanceId: envelope.sessionInstanceId,
                        invocationId: envelope.invocationId,
                        projectId: projectID.uuidString,
                        projectPath: projectPath,
                        tool: id,
                        responseState: responseState,
                        updatedAt: snapshot.updatedAt,
                        source: .probe
                    )
                )
            }
        }

        guard !responsePayloads.isEmpty || !runtimeSnapshotsBySessionID.isEmpty else {
            return nil
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: runtimeSnapshotsBySessionID
        )
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "gemini-hook",
              let envelope = try? JSONDecoder().decode(GeminiHookRuntimeEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.dmuxSessionId),
              let projectID = UUID(uuidString: envelope.dmuxProjectId) else {
            return nil
        }

        let dedupeKey = "gemini|\(envelope.event)|\(sessionID.uuidString)|\(payloadHash(envelope.payload))"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.0) else {
            AppDebugLog.shared.log(
                "gemini-hook",
                "drop duplicate event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "gemini-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "gemini-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let payloadObject: [String: Any]? = envelope.payload.data(using: .utf8)
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let externalSessionID = extractedSessionID(from: payloadObject)
            ?? existingSnapshot?.externalSessionID
            ?? normalizedSessionID(liveEnvelope?.externalSessionID)
        let projectPath = normalizedSessionID(envelope.dmuxProjectPath)
            ?? normalizedSessionID(liveEnvelope?.projectPath)
        let updatedAt = max(
            envelope.receivedAt,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        let responseStateOverride: AIResponseState?
        switch envelope.event {
        case "SessionStart":
            responseStateOverride = .idle
        case "BeforeAgent":
            responseStateOverride = .responding
        case "AfterAgent":
            responseStateOverride = .idle
        case "SessionEnd":
            AppDebugLog.shared.log(
                "gemini-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
            return nil
        default:
            AppDebugLog.shared.log(
                "gemini-hook",
                "ignore event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let runtimeSnapshot = resolvedSnapshot(
            projectPath: projectPath,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot,
            preferredExternalSessionID: externalSessionID,
            responseStateOverride: responseStateOverride,
            updatedAt: updatedAt,
            marksCompletedTurn: false,
            source: .hook
        ) ?? fallbackSnapshot(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot,
            responseStateOverride: responseStateOverride,
            updatedAt: updatedAt,
            marksCompletedTurn: false
        )

        let marksCompletedTurn: Bool = {
            guard envelope.event == "AfterAgent" else {
                return false
            }
            let previousTotal = max(
                liveEnvelope?.totalTokens ?? 0,
                existingSnapshot?.totalTokens ?? 0
            )
            return runtimeSnapshot.totalTokens > previousTotal
        }()

        let effectiveSnapshot: AIRuntimeContextSnapshot = {
            guard marksCompletedTurn else {
                return runtimeSnapshot
            }
            var next = runtimeSnapshot
            next.hasCompletedTurn = true
            return next
        }()

        AppDebugLog.shared.log(
            "gemini-hook",
            "event=\(envelope.event) session=\(sessionID.uuidString) external=\(effectiveSnapshot.externalSessionID ?? "nil") response=\(effectiveSnapshot.responseState?.rawValue ?? "nil") total=\(effectiveSnapshot.totalTokens) completed=\(marksCompletedTurn)"
        )

        let responsePayloads: [AIResponseStatePayload]
        if let responseState = effectiveSnapshot.responseState {
            responsePayloads = [
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: liveEnvelope?.sessionInstanceId,
                    invocationId: liveEnvelope?.invocationId,
                    projectId: projectID.uuidString,
                    projectPath: projectPath,
                    tool: id,
                    responseState: responseState,
                    updatedAt: effectiveSnapshot.updatedAt,
                    source: .hook
                ),
            ]
        } else {
            responsePayloads = []
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: [sessionID: effectiveSnapshot]
        )
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        let canOpen = !(session.externalSessionID?.isEmpty ?? true)
        return AIToolSessionCapabilities(canOpen: canOpen, canRename: false, canRemove: false)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            return nil
        }
        return "gemini --resume \(shellQuoted(sessionID))"
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func normalizedSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func extractedSessionID(from object: [String: Any]?) -> String? {
        firstString(in: object, keys: ["session_id", "sessionId", "id"])
    }

    private func firstString(in root: Any?, keys: [String]) -> String? {
        var stack: [Any] = []
        if let root {
            stack.append(root)
        }

        while let current = stack.popLast() {
            if let dictionary = current as? [String: Any] {
                for key in keys {
                    if let value = dictionary[key] as? String,
                       !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        return value
                    }
                }
                stack.append(contentsOf: dictionary.values)
                continue
            }
            if let array = current as? [Any] {
                stack.append(contentsOf: array)
            }
        }

        return nil
    }

    private func payloadHash(_ payload: String) -> Int {
        var hasher = Hasher()
        hasher.combine(payload.count)
        hasher.combine(payload.prefix(160))
        return hasher.finalize()
    }

    private func shouldReuseExistingTotals(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?
    ) -> Bool {
        guard let externalSessionID, !externalSessionID.isEmpty else {
            return false
        }
        if normalizedSessionID(liveEnvelope?.externalSessionID) == externalSessionID {
            return true
        }
        if existingSnapshot?.externalSessionID == externalSessionID {
            return true
        }
        return false
    }

    private func resolvedSnapshot(
        projectPath: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?,
        preferredExternalSessionID: String? = nil,
        responseStateOverride: AIResponseState?,
        updatedAt: Double,
        marksCompletedTurn: Bool,
        source: AIRuntimeUpdateSource
    ) -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedSessionID(projectPath) else {
            return nil
        }

        let externalSessionID = normalizedSessionID(preferredExternalSessionID)
            ?? normalizedSessionID(liveEnvelope?.externalSessionID)
            ?? existingSnapshot?.externalSessionID
        let startedAt = liveEnvelope?.startedAt ?? updatedAt
        let parsedState = parseGeminiSessionRuntimeState(
            projectPath: projectPath,
            startedAt: startedAt,
            preferredSessionID: externalSessionID,
            preferredSessionIsAuthoritative: externalSessionID != nil
        )
        guard let parsedState else {
            return nil
        }

        return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: parsedState.externalSessionID,
            model: parsedState.model
                ?? existingSnapshot?.model
                ?? normalizedSessionID(liveEnvelope?.model),
            inputTokens: parsedState.inputTokens,
            outputTokens: parsedState.outputTokens,
            totalTokens: parsedState.totalTokens,
            updatedAt: max(updatedAt, parsedState.updatedAt),
            responseState: responseStateOverride ?? parsedState.responseState,
            wasInterrupted: false,
            hasCompletedTurn: marksCompletedTurn,
            sessionOrigin: parsedState.origin,
            source: source
        )
    }

    private func fallbackSnapshot(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?,
        responseStateOverride: AIResponseState?,
        updatedAt: Double,
        marksCompletedTurn: Bool
    ) -> AIRuntimeContextSnapshot {
        let canReuseExistingTotals = shouldReuseExistingTotals(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot
        )
        let inputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
            : 0
        let outputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
            : 0
        let totalTokens = canReuseExistingTotals
            ? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)
            : 0

        return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: externalSessionID,
            model: existingSnapshot?.model ?? normalizedSessionID(liveEnvelope?.model),
            inputTokens: inputTokens,
            outputTokens: outputTokens,
            totalTokens: totalTokens,
            updatedAt: updatedAt,
            responseState: responseStateOverride,
            wasInterrupted: false,
            hasCompletedTurn: marksCompletedTurn,
            source: .hook
        )
    }
}

private enum SQLiteBindingValue {
    case text(String)
    case int64(Int64)
}

private let SQLITE_TRANSIENT_SESSION = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

private func withSQLiteDatabase(path: String, body: (OpaquePointer) throws -> Void) throws {
    var db: OpaquePointer?
    guard sqlite3_open(path, &db) == SQLITE_OK, let db else {
        defer {
            if db != nil {
                sqlite3_close(db)
            }
        }
        throw AIToolSessionControlError.storageFailure(String(localized: "ai.session.storage.open_failed", defaultValue: "Unable to open session storage.", bundle: .module))
    }
    defer { sqlite3_close(db) }
    try body(db)
}

private func executeSQLite(db: OpaquePointer, sql: String, bindings: [SQLiteBindingValue]) throws {
    var statement: OpaquePointer?
    guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
        throw AIToolSessionControlError.storageFailure(String(cString: sqlite3_errmsg(db)))
    }
    defer { sqlite3_finalize(statement) }

    for (index, binding) in bindings.enumerated() {
        let position = Int32(index + 1)
        switch binding {
        case let .text(value):
            sqlite3_bind_text(statement, position, value, -1, SQLITE_TRANSIENT_SESSION)
        case let .int64(value):
            sqlite3_bind_int64(statement, position, value)
        }
    }

    let result = sqlite3_step(statement)
    guard result == SQLITE_DONE else {
        throw AIToolSessionControlError.storageFailure(String(cString: sqlite3_errmsg(db)))
    }
}

private func shellQuoted(_ value: String) -> String {
    "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
}
