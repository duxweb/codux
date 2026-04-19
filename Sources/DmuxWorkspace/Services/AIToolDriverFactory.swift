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

actor AIToolRuntimeEventDeduper {
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
