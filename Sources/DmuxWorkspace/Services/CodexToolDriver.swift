import Foundation
import SQLite3

struct CodexToolDriver: AIToolDriver {
    let id = "codex"
    let aliases: Set<String> = ["codex"]
    let isRealtimeTool = true

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        guard canonicalToolName(event.tool) == id else {
            return event
        }

        var resolvedEvent = event
        let fallbackTotalTokens = currentSession?.committedTotalTokens
        resolvedEvent.model = resolvedEvent.model ?? currentSession?.model

        guard event.kind == .turnCompleted,
              let transcriptPath = event.metadata?.transcriptPath,
              !transcriptPath.isEmpty,
              let parsedState = await resolveCodexStopRuntimeState(transcriptPath: transcriptPath) else {
            if resolvedEvent.totalTokens == nil {
                resolvedEvent.totalTokens = fallbackTotalTokens
            }
            return resolvedEvent
        }

        resolvedEvent.model = resolvedEvent.model ?? parsedState.model
        if let parsedTotalTokens = parsedState.totalTokens {
            resolvedEvent.totalTokens = max(
                resolvedEvent.totalTokens ?? 0,
                fallbackTotalTokens ?? 0,
                parsedTotalTokens
            )
        } else if resolvedEvent.totalTokens == nil {
            resolvedEvent.totalTokens = fallbackTotalTokens
        }
        var metadata = resolvedEvent.metadata ?? AIHookEventMetadata()
        metadata.wasInterrupted = parsedState.wasInterrupted
        metadata.hasCompletedTurn = parsedState.hasCompletedTurn || parsedState.wasInterrupted == false
        resolvedEvent.metadata = metadata
        if let completedAt = parsedState.completedAt ?? parsedState.updatedAt {
            resolvedEvent.updatedAt = max(resolvedEvent.updatedAt, completedAt)
        }
        return resolvedEvent
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedNonEmptyString(session.projectPath) else {
            return nil
        }

        let fileURL: URL?
        if let transcriptPath = normalizedNonEmptyString(session.transcriptPath) {
            fileURL = URL(fileURLWithPath: transcriptPath)
        } else if let externalSessionID = normalizedNonEmptyString(session.aiSessionID) {
            fileURL = AIRuntimeSourceLocator.codexRolloutPath(
                projectPath: projectPath,
                externalSessionID: externalSessionID
            )
        } else {
            fileURL = nil
        }

        guard let parsedState = parseCodexSessionRuntimeState(
            fileURL: fileURL,
            projectPath: projectPath
        ) else {
            return nil
        }

            return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: normalizedNonEmptyString(session.aiSessionID),
            model: parsedState.model ?? session.model,
            inputTokens: parsedState.totalTokens ?? 0,
            outputTokens: 0,
            totalTokens: parsedState.totalTokens ?? 0,
            updatedAt: parsedState.updatedAt ?? session.updatedAt,
            responseState: parsedState.responseState,
            wasInterrupted: parsedState.wasInterrupted,
            hasCompletedTurn: parsedState.hasCompletedTurn,
            sessionOrigin: .unknown,
            source: .probe
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

}
