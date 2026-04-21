import Foundation

struct ClaudeToolDriver: AIToolDriver {
    let id = "claude"
    let aliases: Set<String> = ["claude", "claude-code"]
    let isRealtimeTool = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        guard canonicalTool(event.tool) == id else {
            return event
        }

        var resolvedEvent = event
        let fallbackTotalTokens = currentSession?.committedTotalTokens
        resolvedEvent.model = resolvedEvent.model ?? currentSession?.model

        if resolvedEvent.totalTokens == nil {
            resolvedEvent.totalTokens = fallbackTotalTokens
        }
        return resolvedEvent
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

    private func normalizedString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

}
