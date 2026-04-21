import Foundation

struct GeminiToolDriver: AIToolDriver {
    let id = "gemini"
    let aliases: Set<String> = ["gemini"]
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

        guard let projectPath = normalizedNonEmptyString(event.projectPath),
              let parsedState = parseGeminiSessionRuntimeState(
                  projectPath: projectPath,
                  startedAt: currentSession?.startedAt ?? event.updatedAt,
                  preferredSessionID: normalizedNonEmptyString(event.aiSessionID ?? currentSession?.aiSessionID),
                  preferredSessionIsAuthoritative: normalizedNonEmptyString(event.aiSessionID ?? currentSession?.aiSessionID) != nil
              ) else {
            if resolvedEvent.totalTokens == nil {
                resolvedEvent.totalTokens = fallbackTotalTokens
            }
            return resolvedEvent
        }

        resolvedEvent.aiSessionID = normalizedNonEmptyString(resolvedEvent.aiSessionID) ?? parsedState.externalSessionID
        resolvedEvent.model = resolvedEvent.model ?? parsedState.model
        resolvedEvent.totalTokens = max(
            resolvedEvent.totalTokens ?? 0,
            fallbackTotalTokens ?? 0,
            parsedState.totalTokens
        )

        if resolvedEvent.metadata?.source == nil {
            var metadata = resolvedEvent.metadata ?? AIHookEventMetadata()
            metadata.source = parsedState.origin == .restored ? "resume" : "startup"
            resolvedEvent.metadata = metadata
        }

        return resolvedEvent
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedNonEmptyString(session.projectPath) else {
            return nil
        }
        guard let parsedState = parseGeminiSessionRuntimeState(
            projectPath: projectPath,
            startedAt: session.startedAt,
            preferredSessionID: normalizedNonEmptyString(session.aiSessionID),
            preferredSessionIsAuthoritative: normalizedNonEmptyString(session.aiSessionID) != nil
        ) else {
            return nil
        }

        return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: parsedState.externalSessionID,
            model: parsedState.model,
            inputTokens: parsedState.inputTokens,
            outputTokens: parsedState.outputTokens,
            totalTokens: parsedState.totalTokens,
            updatedAt: parsedState.updatedAt,
            responseState: parsedState.responseState,
            sessionOrigin: parsedState.origin,
            source: .probe
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

}
