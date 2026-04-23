import Foundation

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
    var isRealtimeTool: Bool { get }

    func matches(tool: String) -> Bool
    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent
    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot?
    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities
    func resumeCommand(for session: AISessionSummary) -> String?
    func renameSession(_ session: AISessionSummary, to title: String) throws
    func removeSession(_ session: AISessionSummary) throws
}

extension AIToolDriver {
    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func canonicalToolName(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    func matchingFallbackSession(
        for event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) -> AISessionStore.TerminalSessionState? {
        guard let currentSession else {
            return nil
        }

        let incomingTool = canonicalToolName(event.tool)
        guard canonicalToolName(currentSession.tool) == incomingTool else {
            return nil
        }

        let incomingSessionID = normalizedNonEmptyString(event.aiSessionID)
        let currentSessionID = normalizedNonEmptyString(currentSession.aiSessionID)
        if let incomingSessionID, currentSessionID != incomingSessionID {
            return nil
        }

        if event.kind == .sessionStarted, incomingSessionID == nil {
            return nil
        }

        return currentSession
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        _ = currentSession
        return event
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        _ = session
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

    private let drivers: [any AIToolDriver]

    init(drivers: [any AIToolDriver] = [
        ClaudeToolDriver(),
        CodexToolDriver(),
        OpenCodeToolDriver(),
        GeminiToolDriver(),
    ]) {
        self.drivers = drivers
    }

    func driver(for tool: String?) -> AIToolDriver? {
        guard let tool, !tool.isEmpty else {
            return nil
        }
        return drivers.first { $0.matches(tool: tool) }
    }

    func canonicalToolName(_ tool: String) -> String {
        driver(for: tool)?.id ?? tool
    }

    func isRealtimeTool(_ tool: String) -> Bool {
        driver(for: tool)?.isRealtimeTool ?? false
    }

    func resolveHookEvent(
        _ event: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) async -> AIHookEvent {
        guard let driver = driver(for: event.tool) else {
            return event
        }
        return await driver.resolveHookEvent(event, currentSession: currentSession)
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
