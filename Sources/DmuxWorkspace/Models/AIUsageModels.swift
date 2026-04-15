import Foundation

enum AIResponseState: String, Codable, Equatable, Sendable {
    case idle
    case responding
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
    var responseState: AIResponseState?
    var startedAt: Date?
    var updatedAt: Date
    var currentInputTokens: Int
    var currentOutputTokens: Int
    var currentTotalTokens: Int
    var currentContextWindow: Int?
    var currentContextUsedTokens: Int?
    var currentContextUsagePercent: Double?
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
    var snapshot: AITerminalSessionSnapshot?
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
    var invocationId: String?
    var externalSessionID: String?
    var projectId: String
    var projectName: String
    var projectPath: String?
    var sessionTitle: String
    var tool: String
    var model: String?
    var status: String
    var responseState: AIResponseState?
    var updatedAt: Double
    var startedAt: Double?
    var finishedAt: Double?
    var inputTokens: Int?
    var outputTokens: Int?
    var totalTokens: Int?
    var contextWindow: Int?
    var contextUsedTokens: Int?
    var contextUsagePercent: Double?
}

struct AIResponseStatePayload: Codable, Equatable, Sendable {
    var sessionId: String
    var sessionInstanceId: String?
    var invocationId: String?
    var projectId: String
    var projectPath: String?
    var tool: String
    var responseState: AIResponseState
    var updatedAt: Double
}

struct AIManagedRealtimeSessionRecord: Codable, Equatable, Sendable {
    var recordID: String
    var invocationID: String?
    var runtimeSessionID: String
    var externalSessionID: String?
    var projectID: UUID
    var projectPath: String
    var projectName: String
    var sessionTitle: String
    var tool: String
    var model: String?
    var startedAt: Date
    var updatedAt: Date
    var finishedAt: Date?
    var status: String
    var responseState: AIResponseState?
    var totalInputTokens: Int
    var totalOutputTokens: Int
    var totalTokens: Int
    var maxContextUsagePercent: Double?
}

enum AIToolRuntimeWatchKind: String, Codable, Equatable, Sendable {
    case file
    case directory
}

struct AIToolRuntimeSourceDescriptor: Hashable, Sendable {
    var path: String
    var watchKind: AIToolRuntimeWatchKind
}

struct AIToolRuntimeIngressUpdate: Sendable {
    var responsePayloads: [AIResponseStatePayload] = []
    var runtimeSnapshotsBySessionID: [UUID: AIRuntimeContextSnapshot] = [:]

    var isEmpty: Bool {
        responsePayloads.isEmpty && runtimeSnapshotsBySessionID.isEmpty
    }
}

struct AICodexIncrementalState: Codable, Sendable {
    var processedOffset: UInt64
    var pendingData: Data
    var sessionID: UUID?
    var sessionTitle: String?
    var model: String?
    var firstSeenAt: Date?
    var lastSeenAt: Date?
    var totalTokens: Int
    var lastTokenTotal: Int
    var matchedProject: Bool
}

struct AIExternalFileSummary: Codable, Sendable {
    var source: String
    var filePath: String
    var fileModifiedAt: Double
    var projectPath: String
    var sessions: [AISessionSummary]
    var dayUsage: [AIHeatmapDay]
    var timeBuckets: [AITimeBucket]
    var codexState: AICodexIncrementalState?
}

struct AICachedProjectSummary {
    var projectPath: String
    var createdAt: Date
    var summary: AIProjectDirectorySourceSummary
}

actor AIProjectSummaryCache {
    static let shared = AIProjectSummaryCache()

    private var storage: [String: AICachedProjectSummary] = [:]
    private let ttl: TimeInterval = 8

    func get(projectPath: String) -> AIProjectDirectorySourceSummary? {
        guard let entry = storage[projectPath], Date().timeIntervalSince(entry.createdAt) < ttl else {
            storage[projectPath] = nil
            return nil
        }
        return entry.summary
    }

    func set(projectPath: String, summary: AIProjectDirectorySourceSummary) {
        storage[projectPath] = AICachedProjectSummary(projectPath: projectPath, createdAt: Date(), summary: summary)
    }

    func invalidate(projectPath: String) {
        storage[projectPath] = nil
    }
}
