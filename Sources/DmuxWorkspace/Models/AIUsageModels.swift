import Foundation

enum AIResponseState: String, Codable, Equatable, Sendable {
    case idle
    case responding
}

enum AIRuntimeUpdateSource: String, Codable, Equatable, Sendable {
    case socket
    case hook
    case probe
}

struct AIStatsPanelState: Equatable {
    var projectSummary: AIProjectUsageSummary?
    var currentSnapshot: AITerminalSessionSnapshot?
    var liveSnapshots: [AITerminalSessionSnapshot]
    var liveOverlayTokens: Int
    var sessions: [AISessionSummary]
    var heatmap: [AIHeatmapDay]
    var todayTimeBuckets: [AITimeBucket]
    var toolBreakdown: [AIUsageBreakdownItem]
    var modelBreakdown: [AIUsageBreakdownItem]
    var indexedAt: Date?
    var indexingStatus: AIIndexingStatus

    static let empty = AIStatsPanelState(
        projectSummary: nil,
        currentSnapshot: nil,
        liveSnapshots: [],
        liveOverlayTokens: 0,
        sessions: [],
        heatmap: [],
        todayTimeBuckets: [],
        toolBreakdown: [],
        modelBreakdown: [],
        indexedAt: nil,
        indexingStatus: .idle
    )
}

enum AIIndexingStatus: Equatable {
    case idle
    case indexing(progress: Double, detail: String)
    case completed(detail: String)
    case cancelled(detail: String)
    case failed(detail: String)
}

struct AITerminalSessionSnapshot: Codable, Equatable, Identifiable, Sendable {
    var id: UUID { sessionID }
    var sessionID: UUID
    var externalSessionID: String?
    var projectID: UUID
    var projectName: String
    var sessionTitle: String
    var tool: String?
    var model: String?
    var status: String
    var isRunning: Bool
    var startedAt: Date?
    var updatedAt: Date
    var currentInputTokens: Int
    var currentOutputTokens: Int
    var currentTotalTokens: Int
    var baselineInputTokens: Int
    var baselineOutputTokens: Int
    var baselineTotalTokens: Int
    var currentContextWindow: Int?
    var currentContextUsedTokens: Int?
    var currentContextUsagePercent: Double?
    var wasInterrupted: Bool
    var hasCompletedTurn: Bool
}

struct AISessionSummary: Codable, Equatable, Identifiable, Sendable {
    var id: UUID { sessionID }
    var sessionID: UUID
    var externalSessionID: String?
    var projectID: UUID
    var projectName: String
    var sessionTitle: String
    var firstSeenAt: Date
    var lastSeenAt: Date
    var lastTool: String?
    var lastModel: String?
    var requestCount: Int
    var totalInputTokens: Int
    var totalOutputTokens: Int
    var totalTokens: Int
    var maxContextUsagePercent: Double?
    var activeDurationSeconds: Int
    var todayTokens: Int
}

struct AIProjectUsageSummary: Codable, Equatable, Sendable {
    var projectID: UUID
    var projectName: String
    var currentSessionTokens: Int
    var projectTotalTokens: Int
    var todayTotalTokens: Int
    var currentTool: String?
    var currentModel: String?
    var currentContextUsagePercent: Double?
    var currentContextUsedTokens: Int?
    var currentContextWindow: Int?
    var currentSessionUpdatedAt: Date?
}

struct AIIndexedProjectSnapshot: Codable, Equatable, Sendable {
    var projectID: UUID
    var projectName: String
    var projectSummary: AIProjectUsageSummary
    var sessions: [AISessionSummary]
    var heatmap: [AIHeatmapDay]
    var todayTimeBuckets: [AITimeBucket]
    var toolBreakdown: [AIUsageBreakdownItem]
    var modelBreakdown: [AIUsageBreakdownItem]
    var indexedAt: Date
}

struct AIHeatmapDay: Codable, Equatable, Identifiable, Sendable {
    var id: Date { day }
    var day: Date
    var totalTokens: Int
    var requestCount: Int
}

struct AIUsageBreakdownItem: Codable, Equatable, Identifiable, Sendable {
    var id: String { key }
    var key: String
    var totalTokens: Int
    var requestCount: Int
}

struct AIProjectDirectorySourceSummary {
    var sessions: [AISessionSummary]
    var heatmap: [AIHeatmapDay]
    var todayTimeBuckets: [AITimeBucket]
    var toolBreakdown: [AIUsageBreakdownItem]
    var modelBreakdown: [AIUsageBreakdownItem]
}

struct AITimeBucket: Codable, Equatable, Identifiable, Sendable {
    var id: Date { start }
    var start: Date
    var end: Date
    var totalTokens: Int
    var requestCount: Int
}

struct AIToolUsageEnvelope: Codable, Sendable {
    var sessionId: String
    var sessionInstanceId: String?
    var externalSessionID: String?
    var projectId: String
    var projectName: String
    var sessionTitle: String
    var tool: String
    var model: String?
    var status: String
    var responseState: AIResponseState?
    var updatedAt: Double
    var startedAt: Double?
    var inputTokens: Int?
    var outputTokens: Int?
    var totalTokens: Int?
    var baselineInputTokens: Int?
    var baselineOutputTokens: Int?
    var baselineTotalTokens: Int?
    var contextWindow: Int?
    var contextUsedTokens: Int?
    var contextUsagePercent: Double?
}

struct AIExternalFileSummary: Codable, Equatable, Sendable {
    var source: String
    var filePath: String
    var fileModifiedAt: Double
    var projectPath: String
    var sessions: [AISessionSummary]
    var dayUsage: [AIHeatmapDay]
    var timeBuckets: [AITimeBucket]
}

struct AIExternalFileCheckpointPayload: Codable, Equatable, Sendable {
    var sessionKey: String?
    var externalSessionID: String?
    var sessionTitle: String?
    var lastModel: String?
    var modelTotalTokensByName: [String: Int]
    var firstSeenAt: Date?
    var lastSeenAt: Date?
    var requestCount: Int
    var totalInputTokens: Int
    var totalOutputTokens: Int
    var totalTokens: Int
    var todayTokens: Int
    var activeDurationSeconds: Int
    var waitingForFirstResponse: Bool
    var pendingTurnStartAt: Date?
    var pendingTurnEndAt: Date?
}

struct AIExternalFileCheckpoint: Codable, Equatable, Sendable {
    var source: String
    var filePath: String
    var projectPath: String
    var fileModifiedAt: Double
    var fileSize: UInt64
    var lastOffset: UInt64
    var lastIndexedAt: Date
    var payload: AIExternalFileCheckpointPayload?
}
