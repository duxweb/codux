import Foundation
import SQLite3

struct CodexToolDriver: AIToolDriver {
    let id = "codex"
    let aliases: Set<String> = ["codex"]
    let isRealtimeTool = true
    private let databaseURL: URL?

    init(databaseURL: URL? = nil) {
        self.databaseURL = databaseURL
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        guard canonicalToolName(event.tool) == id else {
            return event
        }

        var resolvedEvent = event
        let fallbackSession = matchingFallbackSession(for: event, currentSession: currentSession)
        let fallbackTotalTokens = fallbackSession?.committedTotalTokens
        resolvedEvent.model = resolvedEvent.model ?? fallbackSession?.model

        guard event.kind == .turnCompleted,
              let transcriptPath = event.metadata?.transcriptPath,
              !transcriptPath.isEmpty,
              let parsedState = await resolveCodexStopRuntimeState(transcriptPath: transcriptPath) else {
            return resolvedEvent
        }

        resolvedEvent.model = resolvedEvent.model ?? parsedState.model
        resolvedEvent.inputTokens = parsedState.inputTokens ?? resolvedEvent.inputTokens
        resolvedEvent.outputTokens = parsedState.outputTokens ?? resolvedEvent.outputTokens
        resolvedEvent.cachedInputTokens = parsedState.cachedInputTokens ?? resolvedEvent.cachedInputTokens
        if let parsedTotalTokens = parsedState.totalTokens,
           parsedTotalTokens > 0 {
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
        metadata.hasCompletedTurn = parsedState.hasCompletedTurn || (parsedState.responseState == .idle && parsedState.wasInterrupted == false)
        resolvedEvent.metadata = metadata
        if parsedState.responseState == .responding {
            resolvedEvent.kind = .promptSubmitted
        }
        if let latestRuntimeUpdate = [parsedState.completedAt, parsedState.updatedAt].compactMap({ $0 }).max() {
            resolvedEvent.updatedAt = max(resolvedEvent.updatedAt, latestRuntimeUpdate)
        }
        return resolvedEvent
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedNonEmptyString(session.projectPath) else {
            return nil
        }

        let fileURL = runtimeRolloutPath(for: session, projectPath: projectPath)
        let parsedState = parseCodexSessionRuntimeState(
            fileURL: fileURL,
            projectPath: projectPath
        )

        guard let parsedState else {
            return nil
        }

        return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: normalizedNonEmptyString(session.aiSessionID),
            model: parsedState.model ?? session.model,
            inputTokens: max(session.committedInputTokens, parsedState.inputTokens ?? 0),
            outputTokens: max(session.committedOutputTokens, parsedState.outputTokens ?? 0),
            cachedInputTokens: max(session.committedCachedInputTokens, parsedState.cachedInputTokens ?? 0),
            totalTokens: max(session.committedTotalTokens, parsedState.totalTokens ?? 0),
            updatedAt: [session.updatedAt, parsedState.updatedAt].compactMap({ $0 }).max() ?? session.updatedAt,
            startedAt: parsedState.startedAt,
            completedAt: parsedState.completedAt,
            responseState: parsedState.responseState ?? responseState(for: session.state),
            wasInterrupted: parsedState.wasInterrupted,
            hasCompletedTurn: parsedState.hasCompletedTurn || session.hasCompletedTurn,
            sessionOrigin: parsedState.origin,
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
        let databaseURL = resolvedDatabaseURL()
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
        let databaseURL = resolvedDatabaseURL()
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

    private func resolvedDatabaseURL() -> URL {
        databaseURL ?? AIRuntimeSourceLocator.codexDatabaseURL()
    }

    private func runtimeRolloutPath(
        for session: AISessionStore.TerminalSessionState,
        projectPath: String
    ) -> URL? {
        if let transcriptPath = normalizedNonEmptyString(session.transcriptPath) {
            return URL(fileURLWithPath: transcriptPath)
        }
        guard let externalSessionID = normalizedNonEmptyString(session.aiSessionID) else {
            return nil
        }
        return AIRuntimeSourceLocator.codexRolloutPath(
            projectPath: projectPath,
            externalSessionID: externalSessionID,
            databaseURL: resolvedDatabaseURL()
        )
    }

    private func responseState(for state: AISessionStore.State) -> AIResponseState {
        switch state {
        case .idle, .needsInput:
            return .idle
        case .responding:
            return .responding
        }
    }
}
