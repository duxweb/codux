import Foundation

enum AIHookEventKind: String, Codable, Equatable, Sendable {
    case sessionStarted
    case promptSubmitted
    case needsInput
    case turnCompleted
    case sessionEnded
}

struct AIHookEventMetadata: Codable, Equatable, Sendable {
    var transcriptPath: String? = nil
    var notificationType: String? = nil
    var source: String? = nil
    var reason: String? = nil
    var targetToolName: String? = nil
    var message: String? = nil
    var wasInterrupted: Bool? = nil
    var hasCompletedTurn: Bool? = nil
}

struct AIHookEvent: Codable, Equatable, Sendable {
    var kind: AIHookEventKind
    var terminalID: UUID
    var terminalInstanceID: String?
    var projectID: UUID
    var projectName: String
    var projectPath: String? = nil
    var sessionTitle: String
    var tool: String
    var aiSessionID: String?
    var model: String?
    var totalTokens: Int?
    var updatedAt: Double
    var metadata: AIHookEventMetadata?
}
