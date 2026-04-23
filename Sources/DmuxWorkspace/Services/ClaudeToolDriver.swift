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
        let fallbackSession = matchingFallbackSession(for: event, currentSession: currentSession)
        let fallbackTotalTokens = fallbackSession?.committedTotalTokens
        let snapshot: AIRuntimeContextSnapshot?
        if resolvedEvent.kind == .turnCompleted,
           let projectPath = normalizedNonEmptyString(resolvedEvent.projectPath),
           let externalSessionID = normalizedNonEmptyString(resolvedEvent.aiSessionID ?? fallbackSession?.aiSessionID) {
            snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: externalSessionID
            )
            resolvedEvent.aiSessionID = normalizedNonEmptyString(resolvedEvent.aiSessionID) ?? externalSessionID
        } else {
            snapshot = nil
        }
        resolvedEvent.model = resolvedEvent.model ?? fallbackSession?.model
        resolvedEvent.model = resolvedEvent.model ?? snapshot?.model
        if resolvedEvent.kind == .turnCompleted {
            var metadata = resolvedEvent.metadata ?? AIHookEventMetadata()
            let wasInterrupted = metadata.wasInterrupted ?? snapshot?.wasInterrupted ?? false
            metadata.wasInterrupted = wasInterrupted
            metadata.hasCompletedTurn = metadata.hasCompletedTurn ?? snapshot?.hasCompletedTurn ?? !wasInterrupted
            resolvedEvent.metadata = metadata
        }

        resolvedEvent.inputTokens = resolvedEvent.inputTokens ?? snapshot?.inputTokens
        resolvedEvent.outputTokens = resolvedEvent.outputTokens ?? snapshot?.outputTokens
        resolvedEvent.cachedInputTokens = resolvedEvent.cachedInputTokens ?? snapshot?.cachedInputTokens
        if resolvedEvent.totalTokens == nil {
            resolvedEvent.totalTokens = max(snapshot?.totalTokens ?? 0, fallbackTotalTokens ?? 0)
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
