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

        let runtimeSnapshot: AIRuntimeContextSnapshot
        let responsePayload: AIResponseStatePayload
        switch envelope.event {
        case "UserPromptSubmit":
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
                hasCompletedTurn: false
            )
            responsePayload = AIResponseStatePayload(
                sessionId: sessionID.uuidString,
                sessionInstanceId: nil,
                invocationId: nil,
                projectId: projectID.uuidString,
                projectPath: nil,
                tool: id,
                responseState: .responding,
                updatedAt: updatedAt
            )
        case "Stop":
            let transcriptPath = stringValue(in: payloadObject, key: "transcript_path")
            let parsedState = await resolveCodexStopRuntimeState(transcriptPath: transcriptPath)
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
                responseState: .idle,
                wasInterrupted: parsedState?.wasInterrupted ?? false,
                hasCompletedTurn: parsedState?.hasCompletedTurn ?? false
            )
            responsePayload = AIResponseStatePayload(
                sessionId: sessionID.uuidString,
                sessionInstanceId: nil,
                invocationId: nil,
                projectId: projectID.uuidString,
                projectPath: nil,
                tool: id,
                responseState: .idle,
                updatedAt: runtimeSnapshot.updatedAt
            )
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

    private func stringValue(in object: [String: Any], key: String) -> String? {
        guard let value = object[key] as? String, !value.isEmpty else {
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
        let model = normalizedSessionID(envelope.model)
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
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        let runtimeSnapshot = AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: externalSessionID,
            model: model,
            inputTokens: max(envelope.inputTokens ?? 0, inheritedInputTokens),
            outputTokens: max(envelope.outputTokens ?? 0, inheritedOutputTokens),
            totalTokens: max(envelope.totalTokens ?? 0, inheritedTotalTokens),
            updatedAt: updatedAt,
            responseState: envelope.responseState
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
                    updatedAt: updatedAt
                ),
            ]
        } else {
            responsePayloads = []
        }

        AppDebugLog.shared.log(
            "opencode-driver",
            "socket kind=\(kind) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") model=\(model ?? "nil") response=\(envelope.responseState?.rawValue ?? "nil") total=\(runtimeSnapshot.totalTokens) reuseTotals=\(canReuseExistingTotals)"
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
    let usesHistoricalExternalSessionHintForRuntimeProbe = false

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        _ = envelope
        return []
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
