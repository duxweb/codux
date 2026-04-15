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
            return appI18n("ai.session.action.unsupported", fallback: "This action is not supported by the current tool.")
        case .missingSessionID:
            return appI18n("ai.session.identifier.missing", fallback: "Missing session identifier.")
        case .sessionNotFound:
            return appI18n("ai.session.record.not_found", fallback: "Matching session record was not found.")
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

    func matches(tool: String) -> Bool
    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor]
    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate?
    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities
    func resumeCommand(for session: AISessionSummary) -> String?
    func renameSession(_ session: AISessionSummary, to title: String) throws
    func removeSession(_ session: AISessionSummary) throws
}

extension AIToolDriver {
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

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        []
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
}

private struct OpenCodeToolDriver: AIToolDriver {
    let id = "opencode"
    let aliases: Set<String> = ["opencode"]
    let runtimeRefreshInterval: TimeInterval = 0.75
    let isRealtimeTool = true

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
}

private struct GeminiToolDriver: AIToolDriver {
    let id = "gemini"
    let aliases: Set<String> = ["gemini"]
    let runtimeRefreshInterval: TimeInterval = 0.75
    let isRealtimeTool = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        _ = envelope
        guard let chatsDirectoryURL = AIRuntimeSourceLocator.geminiChatsDirectoryURL(projectPath: project.path),
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
        _ = descriptor
        let relevantEnvelopes = liveEnvelopes.filter { matches(tool: $0.tool) && $0.status == "running" }
        guard !relevantEnvelopes.isEmpty else {
            return nil
        }

        var update = AIToolRuntimeIngressUpdate()
        for envelope in relevantEnvelopes {
            guard let sessionID = UUID(uuidString: envelope.sessionId),
                  let projectID = UUID(uuidString: envelope.projectId),
                  let project = projects.first(where: { $0.id == projectID }),
                  let parsed = parseGeminiSessionRuntimeState(
                      projectPath: project.path,
                      startedAt: envelope.startedAt ?? envelope.updatedAt,
                      preferredSessionID: envelope.externalSessionID
                  ) else {
                continue
            }

            update.runtimeSnapshotsBySessionID[sessionID] = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: parsed.externalSessionID,
                model: parsed.model,
                inputTokens: parsed.inputTokens,
                outputTokens: parsed.outputTokens,
                totalTokens: parsed.totalTokens,
                updatedAt: parsed.updatedAt,
                responseState: parsed.responseState
            )

            if let responseState = parsed.responseState {
                update.responsePayloads.append(
                    AIResponseStatePayload(
                        sessionId: sessionID.uuidString,
                        sessionInstanceId: envelope.sessionInstanceId,
                        invocationId: envelope.invocationId,
                        projectId: projectID.uuidString,
                        projectPath: project.path,
                        tool: id,
                        responseState: responseState,
                        updatedAt: parsed.updatedAt
                    )
                )
            }
        }
        return update.isEmpty ? nil : update
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
        throw AIToolSessionControlError.storageFailure(appI18n("ai.session.storage.open_failed", fallback: "Unable to open session storage."))
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
