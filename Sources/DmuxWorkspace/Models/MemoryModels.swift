import Foundation

func memoryL(_ key: StaticString, _ defaultValue: String.LocalizationValue) -> String {
    String(localized: key, defaultValue: defaultValue, bundle: .module)
}

enum MemoryScope: String, Codable, Sendable, CaseIterable {
    case user
    case project
}

enum MemoryTier: String, Codable, Sendable, CaseIterable {
    case core
    case working
    case archive
}

enum MemoryKind: String, Codable, Sendable, CaseIterable {
    case preference
    case convention
    case decision
    case fact
    case bugLesson = "bug_lesson"
}

enum MemoryEntryStatus: String, Codable, Sendable, CaseIterable {
    case active
    case merged
    case archived
}

struct MemoryEntry: Identifiable, Codable, Equatable, Sendable {
    var id: UUID
    var scope: MemoryScope
    var projectID: UUID?
    var toolID: String?
    var tier: MemoryTier
    var kind: MemoryKind
    var content: String
    var rationale: String?
    var sourceTool: String?
    var sourceSessionID: String?
    var sourceFingerprint: String?
    var normalizedHash: String
    var supersededBy: UUID?
    var status: MemoryEntryStatus
    var mergedSummaryID: UUID?
    var mergedAt: Date?
    var archivedAt: Date?
    var accessCount: Int
    var lastAccessedAt: Date?
    var createdAt: Date
    var updatedAt: Date
}

struct MemoryProjectOverview: Identifiable, Codable, Equatable, Sendable {
    var projectID: UUID
    var activeEntryCount: Int
    var archivedEntryCount: Int
    var mergedEntryCount: Int
    var summaryCount: Int
    var updatedAt: Date?

    var id: UUID { projectID }

    var totalCount: Int {
        activeEntryCount + archivedEntryCount + mergedEntryCount + summaryCount
    }
}

struct MemoryScopeOverview: Codable, Equatable, Sendable {
    var activeEntryCount: Int
    var archivedEntryCount: Int
    var mergedEntryCount: Int
    var summaryCount: Int
    var updatedAt: Date?

    var totalCount: Int {
        activeEntryCount + archivedEntryCount + mergedEntryCount + summaryCount
    }
}

struct MemorySummary: Identifiable, Codable, Equatable, Sendable {
    var id: UUID
    var scope: MemoryScope
    var projectID: UUID?
    var toolID: String?
    var content: String
    var version: Int
    var sourceEntryIDs: [UUID]
    var tokenEstimate: Int
    var createdAt: Date
    var updatedAt: Date
}

struct MemoryCandidate: Sendable {
    var scope: MemoryScope
    var projectID: UUID?
    var toolID: String?
    var tier: MemoryTier
    var kind: MemoryKind
    var content: String
    var rationale: String?
    var sourceTool: String?
    var sourceSessionID: String?
    var sourceFingerprint: String?
}

struct MemoryExtractionTask: Sendable {
    var id: UUID
    var projectID: UUID
    var tool: String
    var sessionID: String
    var transcriptPath: String
    var sourceFingerprint: String
    var status: String
    var attempts: Int
    var error: String?
    var enqueuedAt: Date
}

enum MemoryExtractionStatus: String, Codable, Equatable, Sendable {
    case idle
    case queued
    case processing
    case failed
}

struct MemoryExtractionStatusSnapshot: Codable, Equatable, Sendable {
    var status: MemoryExtractionStatus
    var pendingCount: Int
    var runningCount: Int
    var lastError: String?
    var updatedAt: Date
}
