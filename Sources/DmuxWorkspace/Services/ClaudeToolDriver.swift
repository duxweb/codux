import Foundation

struct ClaudeToolDriver: AIToolDriver {
    let id = "claude"
    let aliases: Set<String> = ["claude", "claude-code"]
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

        if resolvedEvent.kind == .turnCompleted,
           let projectPath = normalizedNonEmptyString(resolvedEvent.projectPath ?? currentSession?.projectPath),
           let externalSessionID = normalizedNonEmptyString(resolvedEvent.aiSessionID ?? currentSession?.aiSessionID),
           let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
               projectPath: projectPath,
               externalSessionID: externalSessionID
           ) {
            resolvedEvent.model = resolvedEvent.model ?? snapshot.model ?? currentSession?.model
            resolvedEvent.totalTokens = max(
                resolvedEvent.totalTokens ?? 0,
                fallbackTotalTokens ?? 0,
                snapshot.totalTokens
            )
            return resolvedEvent
        }

        if resolvedEvent.totalTokens == nil {
            resolvedEvent.totalTokens = fallbackTotalTokens
        }
        return resolvedEvent
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedNonEmptyString(session.projectPath),
              let externalSessionID = normalizedNonEmptyString(session.aiSessionID) else {
            return nil
        }
        return await ClaudeRuntimeLogCache.shared.snapshot(
            projectPath: projectPath,
            externalSessionID: externalSessionID
        )
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
            var matchesSession = false
            JSONLLineReader.forEachLine(in: fileURL) { lineData in
                guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any],
                      let sessionID = row["sessionId"] as? String else {
                    return true
                }
                matchesSession = (sessionID == targetSessionID)
                return false
            }
            return matchesSession
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
