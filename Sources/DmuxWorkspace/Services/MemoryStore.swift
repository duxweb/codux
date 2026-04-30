import CryptoKit
import Foundation
import SQLite3

struct MemoryStore: Sendable {
    private let databaseURL: URL
    private static let maxExtractionAttempts = 3
    private static let entrySelectColumns = """
    id, scope, project_id, tool_id, tier, kind, content, rationale, source_tool, source_session_id,
    source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at,
    access_count, last_accessed_at, created_at, updated_at
    """

    init(databaseURL: URL? = nil) {
        self.databaseURL = databaseURL ?? Self.defaultDatabaseURL()
    }

    private static func defaultDatabaseURL() -> URL {
        let fileManager = FileManager.default
        let root = AppRuntimePaths.appSupportRootURL(fileManager: fileManager)!
        try? fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        return root.appendingPathComponent("memory.sqlite3", isDirectory: false)
    }

    func upsert(_ candidate: MemoryCandidate) throws -> MemoryEntry {
        try withDatabase { db in
            let normalizedContent = normalizedMemoryContent(candidate.content)
            let normalizedHash = sha256(normalizedContent)
            let existing = try fetchEntry(
                db: db,
                sql: """
                SELECT \(Self.entrySelectColumns)
                FROM memory_entries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '')
                  AND COALESCE(tool_id, '') = COALESCE(?, '')
                  AND normalized_hash = ?
                LIMIT 1;
                """,
                bindings: [
                    .text(candidate.scope.rawValue),
                    .nullableText(candidate.projectID?.uuidString),
                    .nullableText(candidate.toolID),
                    .text(normalizedHash),
                ]
            )

            if var existing {
                existing.tier = preferredTier(existing.tier, candidate.tier)
                existing.kind = candidate.kind
                existing.content = candidate.content
                existing.rationale = candidate.rationale ?? existing.rationale
                existing.sourceTool = candidate.sourceTool ?? existing.sourceTool
                existing.sourceSessionID = candidate.sourceSessionID ?? existing.sourceSessionID
                existing.sourceFingerprint = candidate.sourceFingerprint ?? existing.sourceFingerprint
                existing.updatedAt = Date()

                try execute(
                    db,
                    sql: """
                    UPDATE memory_entries
                    SET tier = ?, kind = ?, content = ?, rationale = ?, source_tool = ?, source_session_id = ?,
                        source_fingerprint = ?, status = ?, merged_summary_id = NULL, merged_at = NULL, archived_at = NULL,
                        updated_at = ?
                    WHERE id = ?;
                    """,
                    bindings: [
                        .text(existing.tier.rawValue),
                        .text(existing.kind.rawValue),
                        .text(existing.content),
                        .nullableText(existing.rationale),
                        .nullableText(existing.sourceTool),
                        .nullableText(existing.sourceSessionID),
                        .nullableText(existing.sourceFingerprint),
                        .text(MemoryEntryStatus.active.rawValue),
                        .double(existing.updatedAt.timeIntervalSince1970),
                        .text(existing.id.uuidString),
                    ]
                )
                return existing
            }

            let now = Date()
            let entry = MemoryEntry(
                id: UUID(),
                scope: candidate.scope,
                projectID: candidate.projectID,
                toolID: candidate.toolID,
                tier: candidate.tier,
                kind: candidate.kind,
                content: candidate.content,
                rationale: candidate.rationale,
                sourceTool: candidate.sourceTool,
                sourceSessionID: candidate.sourceSessionID,
                sourceFingerprint: candidate.sourceFingerprint,
                normalizedHash: normalizedHash,
                supersededBy: nil,
                status: .active,
                mergedSummaryID: nil,
                mergedAt: nil,
                archivedAt: nil,
                accessCount: 0,
                lastAccessedAt: nil,
                createdAt: now,
                updatedAt: now
            )
            try execute(
                db,
                sql: """
                INSERT INTO memory_entries (
                    id, scope, project_id, tool_id, tier, kind, content, rationale, source_tool, source_session_id,
                    source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at,
                    access_count, last_accessed_at, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
                """,
                bindings: [
                    .text(entry.id.uuidString),
                    .text(entry.scope.rawValue),
                    .nullableText(entry.projectID?.uuidString),
                    .nullableText(entry.toolID),
                    .text(entry.tier.rawValue),
                    .text(entry.kind.rawValue),
                    .text(entry.content),
                    .nullableText(entry.rationale),
                    .nullableText(entry.sourceTool),
                    .nullableText(entry.sourceSessionID),
                    .nullableText(entry.sourceFingerprint),
                    .text(entry.normalizedHash),
                    .nullableText(entry.supersededBy?.uuidString),
                    .text(entry.status.rawValue),
                    .nullableText(entry.mergedSummaryID?.uuidString),
                    .nullableDouble(entry.mergedAt?.timeIntervalSince1970),
                    .nullableDouble(entry.archivedAt?.timeIntervalSince1970),
                    .int64(Int64(entry.accessCount)),
                    .nullableDouble(entry.lastAccessedAt?.timeIntervalSince1970),
                    .double(entry.createdAt.timeIntervalSince1970),
                    .double(entry.updatedAt.timeIntervalSince1970),
                ]
            )
            return entry
        }
    }

    func listEntries(
        scope: MemoryScope,
        projectID: UUID? = nil,
        toolID: String? = nil,
        tiers: [MemoryTier],
        limit: Int
    ) throws -> [MemoryEntry] {
        try withDatabase { db in
            let tierPlaceholders = Array(repeating: "?", count: max(1, tiers.count)).joined(separator: ",")
            var bindings: [SQLiteBinding] = [
                .text(scope.rawValue),
                .nullableText(projectID?.uuidString),
                .nullableText(toolID),
            ]
            bindings.append(contentsOf: tiers.map { .text($0.rawValue) })
            bindings.append(.int64(Int64(limit)))
            return try fetchEntries(
                db: db,
                sql: """
                SELECT \(Self.entrySelectColumns)
                FROM memory_entries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '')
                  AND (tool_id IS NULL OR tool_id = ?)
                  AND tier IN (\(tierPlaceholders))
                  AND superseded_by IS NULL
                  AND status = 'active'
                ORDER BY access_count DESC, updated_at DESC
                LIMIT ?;
                """,
                bindings: bindings
            )
        }
    }

    func searchEntries(
        scope: MemoryScope,
        projectID: UUID? = nil,
        query: String,
        limit: Int
    ) throws -> [MemoryEntry] {
        try withDatabase { db in
            try fetchEntries(
                db: db,
                sql: """
                SELECT m.id, m.scope, m.project_id, m.tool_id, m.tier, m.kind, m.content, m.rationale, m.source_tool,
                       m.source_session_id, m.source_fingerprint, m.normalized_hash, m.superseded_by, m.status,
                       m.merged_summary_id, m.merged_at, m.archived_at, m.access_count, m.last_accessed_at, m.created_at, m.updated_at
                FROM memory_fts fts
                JOIN memory_entries m ON m.rowid = fts.rowid
                WHERE m.scope = ?
                  AND COALESCE(m.project_id, '') = COALESCE(?, '')
                  AND m.superseded_by IS NULL
                  AND m.status = 'active'
                  AND fts MATCH ?
                ORDER BY bm25(memory_fts), m.access_count DESC, m.updated_at DESC
                LIMIT ?;
                """,
                bindings: [
                    .text(scope.rawValue),
                    .nullableText(projectID?.uuidString),
                    .text(query),
                    .int64(Int64(limit)),
                ]
            )
        }
    }

    func listEntriesForManagement(
        scope: MemoryScope,
        projectID: UUID? = nil,
        tiers: [MemoryTier]? = nil,
        statuses: [MemoryEntryStatus]? = nil,
        limit: Int = 500
    ) throws -> [MemoryEntry] {
        try withDatabase { db in
            var clauses = [
                "scope = ?",
                "COALESCE(project_id, '') = COALESCE(?, '')",
            ]
            var bindings: [SQLiteBinding] = [
                .text(scope.rawValue),
                .nullableText(projectID?.uuidString),
            ]

            if let tiers, !tiers.isEmpty {
                clauses.append(
                    "tier IN (\(Array(repeating: "?", count: tiers.count).joined(separator: ",")))"
                )
                bindings.append(contentsOf: tiers.map { .text($0.rawValue) })
            }

            if let statuses, !statuses.isEmpty {
                clauses.append(
                    "status IN (\(Array(repeating: "?", count: statuses.count).joined(separator: ",")))"
                )
                bindings.append(contentsOf: statuses.map { .text($0.rawValue) })
            }

            bindings.append(.int64(Int64(limit)))
            return try fetchEntries(
                db: db,
                sql: """
                SELECT \(Self.entrySelectColumns)
                FROM memory_entries
                WHERE \(clauses.joined(separator: " AND "))
                ORDER BY updated_at DESC, created_at DESC
                LIMIT ?;
                """,
                bindings: bindings
            )
        }
    }

    func memoryScopeOverview(scope: MemoryScope, projectID: UUID? = nil) throws -> MemoryScopeOverview {
        try withDatabase { db in
            let entryCounts = try fetchMemoryCountRow(
                db: db,
                sql: """
                SELECT
                    SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status = 'merged' THEN 1 ELSE 0 END),
                    MAX(updated_at)
                FROM memory_entries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '');
                """,
                bindings: [.text(scope.rawValue), .nullableText(projectID?.uuidString)]
            )
            let summaryCounts = try fetchMemoryCountRow(
                db: db,
                sql: """
                SELECT 0, 0, COUNT(*), MAX(updated_at)
                FROM memory_summaries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '');
                """,
                bindings: [.text(scope.rawValue), .nullableText(projectID?.uuidString)]
            )
            return MemoryScopeOverview(
                activeEntryCount: entryCounts.active,
                archivedEntryCount: entryCounts.archived,
                mergedEntryCount: entryCounts.merged,
                summaryCount: summaryCounts.merged,
                updatedAt: maxDate(entryCounts.updatedAt, summaryCounts.updatedAt)
            )
        }
    }

    func projectOverviewsForManagement() throws -> [MemoryProjectOverview] {
        try withDatabase { db in
            var overviews: [UUID: MemoryProjectOverview] = [:]

            let entryRows = try fetchProjectMemoryCountRows(
                db: db,
                sql: """
                SELECT
                    project_id,
                    SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN status = 'merged' THEN 1 ELSE 0 END),
                    MAX(updated_at)
                FROM memory_entries
                WHERE scope = 'project'
                  AND project_id IS NOT NULL
                GROUP BY project_id;
                """,
                bindings: []
            )
            for row in entryRows {
                guard let projectID = row.projectID else {
                    continue
                }
                overviews[projectID] = MemoryProjectOverview(
                    projectID: projectID,
                    activeEntryCount: row.active,
                    archivedEntryCount: row.archived,
                    mergedEntryCount: row.merged,
                    summaryCount: 0,
                    updatedAt: row.updatedAt
                )
            }

            let summaryRows = try fetchProjectMemoryCountRows(
                db: db,
                sql: """
                SELECT project_id, 0, 0, COUNT(*), MAX(updated_at)
                FROM memory_summaries
                WHERE scope = 'project'
                  AND project_id IS NOT NULL
                GROUP BY project_id;
                """,
                bindings: []
            )
            for row in summaryRows {
                guard let projectID = row.projectID else {
                    continue
                }
                var overview = overviews[projectID] ?? MemoryProjectOverview(
                    projectID: projectID,
                    activeEntryCount: 0,
                    archivedEntryCount: 0,
                    mergedEntryCount: 0,
                    summaryCount: 0,
                    updatedAt: nil
                )
                overview.summaryCount += row.merged
                overview.updatedAt = maxDate(overview.updatedAt, row.updatedAt)
                overviews[projectID] = overview
            }

            return overviews.values
                .filter { $0.totalCount > 0 }
                .sorted {
                    switch ($0.updatedAt, $1.updatedAt) {
                    case let (lhs?, rhs?):
                        return lhs > rhs
                    case (_?, nil):
                        return true
                    case (nil, _?):
                        return false
                    case (nil, nil):
                        return $0.projectID.uuidString < $1.projectID.uuidString
                    }
                }
        }
    }

    func listSummariesForManagement(scope: MemoryScope, projectID: UUID? = nil) throws -> [MemorySummary] {
        try withDatabase { db in
            try fetchSummaries(
                db: db,
                sql: """
                SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
                FROM memory_summaries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '')
                ORDER BY updated_at DESC;
                """,
                bindings: [.text(scope.rawValue), .nullableText(projectID?.uuidString)]
            )
        }
    }

    func deleteEntry(_ entryID: UUID) throws {
        try withDatabase { db in
            try execute(
                db,
                sql: "DELETE FROM memory_entries WHERE id = ?;",
                bindings: [.text(entryID.uuidString)]
            )
        }
    }

    func deleteSummary(_ summaryID: UUID) throws {
        try withDatabase { db in
            try execute(
                db,
                sql: "DELETE FROM memory_summary_versions WHERE summary_id = ?;",
                bindings: [.text(summaryID.uuidString)]
            )
            try execute(
                db,
                sql: "DELETE FROM memory_summaries WHERE id = ?;",
                bindings: [.text(summaryID.uuidString)]
            )
            try execute(
                db,
                sql: """
                UPDATE memory_entries
                SET merged_summary_id = NULL, updated_at = ?
                WHERE merged_summary_id = ?;
                """,
                bindings: [.double(Date().timeIntervalSince1970), .text(summaryID.uuidString)]
            )
        }
    }

    func bumpAccess(for entryIDs: [UUID]) throws {
        guard !entryIDs.isEmpty else {
            return
        }
        try withDatabase { db in
            let now = Date().timeIntervalSince1970
            for entryID in entryIDs {
                try execute(
                    db,
                    sql: """
                    UPDATE memory_entries
                    SET access_count = access_count + 1,
                        last_accessed_at = ?
                    WHERE id = ?;
                    """,
                    bindings: [
                        .double(now),
                        .text(entryID.uuidString),
                    ]
                )
            }
        }
    }

    func markSuperseded(oldID: UUID, by newID: UUID) throws {
        try withDatabase { db in
            try execute(
                db,
                sql: """
                UPDATE memory_entries
                SET superseded_by = ?, status = ?, tier = ?, archived_at = ?, updated_at = ?
                WHERE id = ?;
                """,
                bindings: [
                    .text(newID.uuidString),
                    .text(MemoryEntryStatus.archived.rawValue),
                    .text(MemoryTier.archive.rawValue),
                    .double(Date().timeIntervalSince1970),
                    .double(Date().timeIntervalSince1970),
                    .text(oldID.uuidString),
                ]
            )
        }
    }

    func archiveEntry(_ entryID: UUID) throws {
        try archiveEntries([entryID])
    }

    func archiveEntries(_ entryIDs: [UUID]) throws {
        guard !entryIDs.isEmpty else {
            return
        }
        try withDatabase { db in
            let now = Date().timeIntervalSince1970
            for entryID in entryIDs {
                try execute(
                    db,
                    sql: """
                    UPDATE memory_entries
                    SET tier = ?, status = ?, archived_at = ?, updated_at = ?
                    WHERE id = ?;
                    """,
                    bindings: [
                        .text(MemoryTier.archive.rawValue),
                        .text(MemoryEntryStatus.archived.rawValue),
                        .double(now),
                        .double(now),
                        .text(entryID.uuidString),
                    ]
                )
            }
        }
    }

    func markEntriesMerged(_ entryIDs: [UUID], summaryID: UUID) throws {
        guard !entryIDs.isEmpty else {
            return
        }
        try withDatabase { db in
            let now = Date().timeIntervalSince1970
            for entryID in entryIDs {
                try execute(
                    db,
                    sql: """
                    UPDATE memory_entries
                    SET status = ?, merged_summary_id = ?, merged_at = ?, updated_at = ?
                    WHERE id = ? AND status = 'active';
                    """,
                    bindings: [
                        .text(MemoryEntryStatus.merged.rawValue),
                        .text(summaryID.uuidString),
                        .double(now),
                        .double(now),
                        .text(entryID.uuidString),
                    ]
                )
            }
        }
    }

    func trimWorkingEntries(scope: MemoryScope, projectID: UUID? = nil, maxActive: Int) throws {
        guard maxActive >= 0 else {
            return
        }
        try withDatabase { db in
            let entries = try fetchEntries(
                db: db,
                sql: """
                SELECT \(Self.entrySelectColumns)
                FROM memory_entries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '')
                  AND tier = ?
                  AND status = 'active'
                ORDER BY updated_at DESC
                LIMIT -1 OFFSET ?;
                """,
                bindings: [
                    .text(scope.rawValue),
                    .nullableText(projectID?.uuidString),
                    .text(MemoryTier.working.rawValue),
                    .int64(Int64(maxActive)),
                ]
            )
            let staleIDs = entries.map(\.id)
            guard !staleIDs.isEmpty else {
                return
            }
            let now = Date().timeIntervalSince1970
            for entryID in staleIDs {
                try execute(
                    db,
                    sql: """
                    UPDATE memory_entries
                    SET status = ?, archived_at = ?, updated_at = ?
                    WHERE id = ?;
                    """,
                    bindings: [
                        .text(MemoryEntryStatus.archived.rawValue),
                        .double(now),
                        .double(now),
                        .text(entryID.uuidString),
                    ]
                )
            }
        }
    }

    func currentSummary(scope: MemoryScope, projectID: UUID? = nil, toolID: String? = nil) throws -> MemorySummary? {
        try withDatabase { db in
            try fetchSummary(
                db: db,
                sql: """
                SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
                FROM memory_summaries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '')
                  AND COALESCE(tool_id, '') = COALESCE(?, '')
                LIMIT 1;
                """,
                bindings: [
                    .text(scope.rawValue),
                    .nullableText(projectID?.uuidString),
                    .nullableText(toolID),
                ]
            )
        }
    }

    @discardableResult
    func upsertSummary(
        scope: MemoryScope,
        projectID: UUID? = nil,
        toolID: String? = nil,
        content: String,
        sourceEntryIDs: [UUID],
        maxVersions: Int
    ) throws -> MemorySummary {
        try withDatabase { db in
            let now = Date()
            let existing = try fetchSummary(
                db: db,
                sql: """
                SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
                FROM memory_summaries
                WHERE scope = ?
                  AND COALESCE(project_id, '') = COALESCE(?, '')
                  AND COALESCE(tool_id, '') = COALESCE(?, '')
                LIMIT 1;
                """,
                bindings: [
                    .text(scope.rawValue),
                    .nullableText(projectID?.uuidString),
                    .nullableText(toolID),
                ]
            )
            let trimmedContent = content.trimmingCharacters(in: .whitespacesAndNewlines)
            let sourceIDs = Array(Set(sourceEntryIDs)).sorted { $0.uuidString < $1.uuidString }
            let tokenEstimate = estimateTokens(trimmedContent)

            if let existing {
                let nextVersion = existing.version + 1
                try execute(
                    db,
                    sql: """
                    UPDATE memory_summaries
                    SET content = ?, version = ?, source_entry_ids = ?, token_estimate = ?, updated_at = ?
                    WHERE id = ?;
                    """,
                    bindings: [
                        .text(trimmedContent),
                        .int64(Int64(nextVersion)),
                        .text(encodeUUIDs(sourceIDs)),
                        .int64(Int64(tokenEstimate)),
                        .double(now.timeIntervalSince1970),
                        .text(existing.id.uuidString),
                    ]
                )
                try insertSummaryVersion(db: db, summaryID: existing.id, version: nextVersion, content: trimmedContent, sourceEntryIDs: sourceIDs, createdAt: now)
                try trimSummaryVersions(db: db, summaryID: existing.id, maxVersions: maxVersions)
                return MemorySummary(
                    id: existing.id,
                    scope: scope,
                    projectID: projectID,
                    toolID: toolID,
                    content: trimmedContent,
                    version: nextVersion,
                    sourceEntryIDs: sourceIDs,
                    tokenEstimate: tokenEstimate,
                    createdAt: existing.createdAt,
                    updatedAt: now
                )
            }

            let summary = MemorySummary(
                id: UUID(),
                scope: scope,
                projectID: projectID,
                toolID: toolID,
                content: trimmedContent,
                version: 1,
                sourceEntryIDs: sourceIDs,
                tokenEstimate: tokenEstimate,
                createdAt: now,
                updatedAt: now
            )
            try execute(
                db,
                sql: """
                INSERT INTO memory_summaries (
                    id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
                """,
                bindings: [
                    .text(summary.id.uuidString),
                    .text(summary.scope.rawValue),
                    .nullableText(summary.projectID?.uuidString),
                    .nullableText(summary.toolID),
                    .text(summary.content),
                    .int64(Int64(summary.version)),
                    .text(encodeUUIDs(summary.sourceEntryIDs)),
                    .int64(Int64(summary.tokenEstimate)),
                    .double(summary.createdAt.timeIntervalSince1970),
                    .double(summary.updatedAt.timeIntervalSince1970),
                ]
            )
            try insertSummaryVersion(db: db, summaryID: summary.id, version: summary.version, content: summary.content, sourceEntryIDs: sourceIDs, createdAt: now)
            try trimSummaryVersions(db: db, summaryID: summary.id, maxVersions: maxVersions)
            return summary
        }
    }

    @discardableResult
    func enqueueExtractionIfNeeded(
        projectID: UUID,
        tool: String,
        sessionID: String,
        transcriptPath: String,
        sourceFingerprint: String
    ) throws -> Bool {
        try withDatabase { db in
            let existing = try fetchScalarInt(
                db: db,
                sql: "SELECT COUNT(*) FROM memory_extraction_queue WHERE source_fingerprint = ?;",
                bindings: [.text(sourceFingerprint)]
            ) ?? 0
            guard existing == 0 else {
                return false
            }

            try execute(
                db,
                sql: """
                INSERT INTO memory_extraction_queue (
                    id, project_id, tool, session_id, transcript_path, source_fingerprint, status, attempts, error, enqueued_at
                ) VALUES (?, ?, ?, ?, ?, ?, 'pending', 0, NULL, ?);
                """,
                bindings: [
                    .text(UUID().uuidString),
                    .text(projectID.uuidString),
                    .text(tool),
                    .text(sessionID),
                    .text(transcriptPath),
                    .text(sourceFingerprint),
                    .double(Date().timeIntervalSince1970),
                ]
            )
            return true
        }
    }

    func nextPendingExtractionTask() throws -> MemoryExtractionTask? {
        try withDatabase { db in
            try fetchTask(
                db: db,
                sql: """
                SELECT id, project_id, tool, session_id, transcript_path, source_fingerprint, status, attempts, error, enqueued_at
                FROM memory_extraction_queue
                WHERE status = 'pending'
                ORDER BY enqueued_at ASC
                LIMIT 1;
                """,
                bindings: []
            )
        }
    }

    func retryableFailedExtractionTask() throws -> MemoryExtractionTask? {
        try withDatabase { db in
            try fetchTask(
                db: db,
                sql: """
                SELECT id, project_id, tool, session_id, transcript_path, source_fingerprint, status, attempts, error, enqueued_at
                FROM memory_extraction_queue
                WHERE status = 'failed'
                  AND attempts < ?
                  AND NOT EXISTS (
                    SELECT 1 FROM memory_extraction_queue done
                    WHERE done.source_fingerprint = memory_extraction_queue.source_fingerprint
                      AND done.status = 'done'
                  )
                ORDER BY enqueued_at DESC
                LIMIT 1;
                """,
                bindings: [.int64(Int64(Self.maxExtractionAttempts))]
            )
        }
    }

    func resetExtractionTaskForRetry(_ taskID: UUID) throws {
        try updateTaskStatus(taskID: taskID, status: "pending", error: nil, incrementAttempts: false)
    }

    func resetRunningExtractionTasks(reason: String) throws -> Int {
        try withDatabase { db in
            let count = try fetchScalarInt(
                db: db,
                sql: "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'running';",
                bindings: []
            ) ?? 0
            guard count > 0 else {
                return 0
            }
            try execute(
                db,
                sql: """
                UPDATE memory_extraction_queue
                SET status = 'pending', error = ?
                WHERE status = 'running'
                  AND attempts < ?;
                """,
                bindings: [.text(reason), .int64(Int64(Self.maxExtractionAttempts))]
            )
            try execute(
                db,
                sql: """
                UPDATE memory_extraction_queue
                SET status = 'failed', error = ?
                WHERE status = 'running'
                  AND attempts >= ?;
                """,
                bindings: [.text("\(reason) Retry limit reached."), .int64(Int64(Self.maxExtractionAttempts))]
            )
            return count
        }
    }

    func markExtractionTaskRunning(_ taskID: UUID) throws {
        try updateTaskStatus(taskID: taskID, status: "running", error: nil, incrementAttempts: true)
    }

    func markExtractionTaskDone(_ taskID: UUID) throws {
        try updateTaskStatus(taskID: taskID, status: "done", error: nil, incrementAttempts: false)
    }

    func markExtractionTaskFailed(_ taskID: UUID, error: String) throws {
        try updateTaskStatus(taskID: taskID, status: "failed", error: error, incrementAttempts: false)
    }

    func extractionStatusSnapshot() throws -> MemoryExtractionStatusSnapshot {
        try withDatabase { db in
            let pendingCount = try fetchScalarInt(
                db: db,
                sql: "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'pending';",
                bindings: []
            ) ?? 0
            let runningCount = try fetchScalarInt(
                db: db,
                sql: "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'running';",
                bindings: []
            ) ?? 0
            let latestTerminalStatus = try fetchScalarText(
                db: db,
                sql: """
                SELECT status
                FROM memory_extraction_queue
                WHERE status IN ('done', 'failed')
                ORDER BY enqueued_at DESC
                LIMIT 1;
                """,
                bindings: []
            )
            let latestTerminalError = try fetchScalarText(
                db: db,
                sql: """
                SELECT error
                FROM memory_extraction_queue
                WHERE status IN ('done', 'failed')
                ORDER BY enqueued_at DESC
                LIMIT 1;
                """,
                bindings: []
            )
            let status: MemoryExtractionStatus
            let lastError: String?
            if runningCount > 0 {
                status = .processing
                lastError = nil
            } else if pendingCount > 0 {
                status = .queued
                lastError = nil
            } else if latestTerminalStatus == "failed" {
                status = .failed
                lastError = normalizedNonEmptyString(latestTerminalError) ?? "Memory extraction failed."
            } else {
                status = .idle
                lastError = nil
            }
            return MemoryExtractionStatusSnapshot(
                status: status,
                pendingCount: pendingCount,
                runningCount: runningCount,
                lastError: lastError,
                updatedAt: Date()
            )
        }
    }

    private func updateTaskStatus(taskID: UUID, status: String, error: String?, incrementAttempts: Bool) throws {
        try withDatabase { db in
            try execute(
                db,
                sql: """
                UPDATE memory_extraction_queue
                SET status = ?,
                    attempts = attempts + ?,
                    error = ?
                WHERE id = ?;
                """,
                bindings: [
                    .text(status),
                    .int64(incrementAttempts ? 1 : 0),
                    .nullableText(error),
                    .text(taskID.uuidString),
                ]
            )
        }
    }

    private func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        var db: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &db) == SQLITE_OK,
              let db else {
            if db != nil {
                sqlite3_close(db)
            }
            throw NSError(domain: "MemoryStore", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to open memory database."])
        }
        defer { sqlite3_close(db) }
        sqlite3_busy_timeout(db, 3000)
        try configure(db)
        return try body(db)
    }

    private func configure(_ db: OpaquePointer) throws {
        let statements = [
            "PRAGMA journal_mode=WAL;",
            "PRAGMA synchronous=NORMAL;",
            """
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL,
                project_id TEXT,
                tool_id TEXT,
                tier TEXT NOT NULL,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                rationale TEXT,
                source_tool TEXT,
                source_session_id TEXT,
                source_fingerprint TEXT,
                normalized_hash TEXT NOT NULL,
                superseded_by TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                merged_summary_id TEXT,
                merged_at REAL,
                archived_at REAL,
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at REAL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            """,
            """
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                content, rationale, content='memory_entries', content_rowid='rowid'
            );
            """,
            """
            CREATE TRIGGER IF NOT EXISTS memory_entries_ai AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_fts(rowid, content, rationale)
                VALUES (new.rowid, new.content, COALESCE(new.rationale, ''));
            END;
            """,
            """
            CREATE TRIGGER IF NOT EXISTS memory_entries_ad AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, rationale)
                VALUES('delete', old.rowid, old.content, COALESCE(old.rationale, ''));
            END;
            """,
            """
            CREATE TRIGGER IF NOT EXISTS memory_entries_au AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, rationale)
                VALUES('delete', old.rowid, old.content, COALESCE(old.rationale, ''));
                INSERT INTO memory_fts(rowid, content, rationale)
                VALUES (new.rowid, new.content, COALESCE(new.rationale, ''));
            END;
            """,
            """
            CREATE TABLE IF NOT EXISTS memory_extraction_queue (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                tool TEXT NOT NULL,
                session_id TEXT NOT NULL,
                transcript_path TEXT NOT NULL,
                source_fingerprint TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                error TEXT,
                enqueued_at REAL NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS memory_summaries (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL,
                project_id TEXT,
                tool_id TEXT,
                content TEXT NOT NULL,
                version INTEGER NOT NULL,
                source_entry_ids TEXT,
                token_estimate INTEGER NOT NULL DEFAULT 0,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS memory_summary_versions (
                id TEXT PRIMARY KEY,
                summary_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                content TEXT NOT NULL,
                source_entry_ids TEXT,
                created_at REAL NOT NULL
            );
            """,
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_scope_project_tier ON memory_entries(scope, project_id, tier);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_tool ON memory_entries(tool_id);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_hash ON memory_entries(scope, project_id, tool_id, normalized_hash);",
            "CREATE INDEX IF NOT EXISTS idx_memory_queue_status_time ON memory_extraction_queue(status, enqueued_at);",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_summaries_scope_project_tool ON memory_summaries(scope, COALESCE(project_id, ''), COALESCE(tool_id, ''));",
            "CREATE INDEX IF NOT EXISTS idx_memory_summary_versions_summary ON memory_summary_versions(summary_id, version);",
        ]
        for statement in statements {
            let result = sqlite3_exec(db, statement, nil, nil, nil)
            guard result == SQLITE_OK else {
                throw NSError(domain: "MemoryStore", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to initialize memory database."])
            }
        }
    }

    private func fetchEntry(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> MemoryEntry? {
        try fetchEntries(db: db, sql: sql, bindings: bindings).first
    }

    private func fetchEntries(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> [MemoryEntry] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare memory query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)

        var entries: [MemoryEntry] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let entry = decodeEntry(statement: statement) else {
                continue
            }
            entries.append(entry)
        }
        return entries
    }

    private func fetchTask(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> MemoryExtractionTask? {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 4, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare extraction task query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)

        guard sqlite3_step(statement) == SQLITE_ROW,
              let rawID = sqlite3_column_text(statement, 0),
              let rawProjectID = sqlite3_column_text(statement, 1),
              let rawTool = sqlite3_column_text(statement, 2),
              let rawSessionID = sqlite3_column_text(statement, 3),
              let rawTranscriptPath = sqlite3_column_text(statement, 4),
              let rawFingerprint = sqlite3_column_text(statement, 5),
              let rawStatus = sqlite3_column_text(statement, 6),
              let taskID = UUID(uuidString: String(cString: rawID)),
              let projectID = UUID(uuidString: String(cString: rawProjectID)) else {
            return nil
        }

        return MemoryExtractionTask(
            id: taskID,
            projectID: projectID,
            tool: String(cString: rawTool),
            sessionID: String(cString: rawSessionID),
            transcriptPath: String(cString: rawTranscriptPath),
            sourceFingerprint: String(cString: rawFingerprint),
            status: String(cString: rawStatus),
            attempts: Int(sqlite3_column_int64(statement, 7)),
            error: sqlite3_column_text(statement, 8).map { String(cString: $0) },
            enqueuedAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 9))
        )
    }

    private func fetchSummary(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> MemorySummary? {
        try fetchSummaries(db: db, sql: sql, bindings: bindings).first
    }

    private func fetchSummaries(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> [MemorySummary] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 8, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare memory summary query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)

        var summaries: [MemorySummary] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawID = sqlite3_column_text(statement, 0),
                  let id = UUID(uuidString: String(cString: rawID)),
                  let rawScope = sqlite3_column_text(statement, 1),
                  let scope = MemoryScope(rawValue: String(cString: rawScope)),
                  let rawContent = sqlite3_column_text(statement, 4) else {
                continue
            }

            summaries.append(
                MemorySummary(
                    id: id,
                    scope: scope,
                    projectID: sqlite3_column_text(statement, 2).flatMap { UUID(uuidString: String(cString: $0)) },
                    toolID: sqlite3_column_text(statement, 3).map { String(cString: $0) },
                    content: String(cString: rawContent),
                    version: Int(sqlite3_column_int64(statement, 5)),
                    sourceEntryIDs: decodeUUIDs(sqlite3_column_text(statement, 6).map { String(cString: $0) }),
                    tokenEstimate: Int(sqlite3_column_int64(statement, 7)),
                    createdAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 8)),
                    updatedAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 9))
                )
            )
        }
        return summaries
    }

    private func insertSummaryVersion(
        db: OpaquePointer,
        summaryID: UUID,
        version: Int,
        content: String,
        sourceEntryIDs: [UUID],
        createdAt: Date
    ) throws {
        try execute(
            db,
            sql: """
            INSERT INTO memory_summary_versions (id, summary_id, version, content, source_entry_ids, created_at)
            VALUES (?, ?, ?, ?, ?, ?);
            """,
            bindings: [
                .text(UUID().uuidString),
                .text(summaryID.uuidString),
                .int64(Int64(version)),
                .text(content),
                .text(encodeUUIDs(sourceEntryIDs)),
                .double(createdAt.timeIntervalSince1970),
            ]
        )
    }

    private func trimSummaryVersions(db: OpaquePointer, summaryID: UUID, maxVersions: Int) throws {
        guard maxVersions > 0 else {
            try execute(
                db,
                sql: "DELETE FROM memory_summary_versions WHERE summary_id = ?;",
                bindings: [.text(summaryID.uuidString)]
            )
            return
        }
        try execute(
            db,
            sql: """
            DELETE FROM memory_summary_versions
            WHERE summary_id = ?
              AND id NOT IN (
                SELECT id FROM memory_summary_versions
                WHERE summary_id = ?
                ORDER BY version DESC
                LIMIT ?
              );
            """,
            bindings: [
                .text(summaryID.uuidString),
                .text(summaryID.uuidString),
                .int64(Int64(maxVersions)),
            ]
        )
    }

    private func fetchScalarInt(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> Int? {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 5, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare scalar query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)
        guard sqlite3_step(statement) == SQLITE_ROW else {
            return nil
        }
        return Int(sqlite3_column_int64(statement, 0))
    }

    private func fetchScalarText(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> String? {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 6, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare scalar text query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)
        guard sqlite3_step(statement) == SQLITE_ROW else {
            return nil
        }
        return sqlite3_column_text(statement, 0).map { String(cString: $0) }
    }

    private func fetchMemoryCountRow(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> MemoryCountRow {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 9, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare memory count query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)
        guard sqlite3_step(statement) == SQLITE_ROW else {
            return MemoryCountRow(projectID: nil, active: 0, archived: 0, merged: 0, updatedAt: nil)
        }
        return MemoryCountRow(
            projectID: nil,
            active: Int(sqlite3_column_int64(statement, 0)),
            archived: Int(sqlite3_column_int64(statement, 1)),
            merged: Int(sqlite3_column_int64(statement, 2)),
            updatedAt: sqlite3_column_type(statement, 3) == SQLITE_NULL ? nil : Date(timeIntervalSince1970: sqlite3_column_double(statement, 3))
        )
    }

    private func fetchProjectMemoryCountRows(db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws -> [MemoryCountRow] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 10, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare project memory count query."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)

        var rows: [MemoryCountRow] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawProjectID = sqlite3_column_text(statement, 0),
                  let projectID = UUID(uuidString: String(cString: rawProjectID)) else {
                continue
            }
            rows.append(
                MemoryCountRow(
                    projectID: projectID,
                    active: Int(sqlite3_column_int64(statement, 1)),
                    archived: Int(sqlite3_column_int64(statement, 2)),
                    merged: Int(sqlite3_column_int64(statement, 3)),
                    updatedAt: sqlite3_column_type(statement, 4) == SQLITE_NULL ? nil : Date(timeIntervalSince1970: sqlite3_column_double(statement, 4))
                )
            )
        }
        return rows
    }

    private func execute(_ db: OpaquePointer, sql: String, bindings: [SQLiteBinding]) throws {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw NSError(domain: "MemoryStore", code: 7, userInfo: [NSLocalizedDescriptionKey: "Failed to prepare memory statement."])
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)
        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw NSError(domain: "MemoryStore", code: 8, userInfo: [NSLocalizedDescriptionKey: "Failed to execute memory statement."])
        }
    }

    private func bind(_ bindings: [SQLiteBinding], to statement: OpaquePointer) throws {
        for (offset, binding) in bindings.enumerated() {
            let index = Int32(offset + 1)
            switch binding {
            case let .text(value):
                sqlite3_bind_text(statement, index, value, -1, SQLITE_TRANSIENT_SESSION)
            case let .nullableText(value):
                if let value {
                    sqlite3_bind_text(statement, index, value, -1, SQLITE_TRANSIENT_SESSION)
                } else {
                    sqlite3_bind_null(statement, index)
                }
            case let .int64(value):
                sqlite3_bind_int64(statement, index, value)
            case let .double(value):
                sqlite3_bind_double(statement, index, value)
            case let .nullableDouble(value):
                if let value {
                    sqlite3_bind_double(statement, index, value)
                } else {
                    sqlite3_bind_null(statement, index)
                }
            }
        }
    }

    private func decodeEntry(statement: OpaquePointer) -> MemoryEntry? {
        guard let rawID = sqlite3_column_text(statement, 0),
              let id = UUID(uuidString: String(cString: rawID)),
              let rawScope = sqlite3_column_text(statement, 1),
              let scope = MemoryScope(rawValue: String(cString: rawScope)),
              let rawTier = sqlite3_column_text(statement, 4),
              let tier = MemoryTier(rawValue: String(cString: rawTier)),
              let rawKind = sqlite3_column_text(statement, 5),
              let kind = MemoryKind(rawValue: String(cString: rawKind)),
              let rawContent = sqlite3_column_text(statement, 6),
              let rawHash = sqlite3_column_text(statement, 11) else {
            return nil
        }

        return MemoryEntry(
            id: id,
            scope: scope,
            projectID: sqlite3_column_text(statement, 2).flatMap { UUID(uuidString: String(cString: $0)) },
            toolID: sqlite3_column_text(statement, 3).map { String(cString: $0) },
            tier: tier,
            kind: kind,
            content: String(cString: rawContent),
            rationale: sqlite3_column_text(statement, 7).map { String(cString: $0) },
            sourceTool: sqlite3_column_text(statement, 8).map { String(cString: $0) },
            sourceSessionID: sqlite3_column_text(statement, 9).map { String(cString: $0) },
            sourceFingerprint: sqlite3_column_text(statement, 10).map { String(cString: $0) },
            normalizedHash: String(cString: rawHash),
            supersededBy: sqlite3_column_text(statement, 12).flatMap { UUID(uuidString: String(cString: $0)) },
            status: sqlite3_column_text(statement, 13).flatMap { MemoryEntryStatus(rawValue: String(cString: $0)) } ?? .active,
            mergedSummaryID: sqlite3_column_text(statement, 14).flatMap { UUID(uuidString: String(cString: $0)) },
            mergedAt: sqlite3_column_type(statement, 15) == SQLITE_NULL ? nil : Date(timeIntervalSince1970: sqlite3_column_double(statement, 15)),
            archivedAt: sqlite3_column_type(statement, 16) == SQLITE_NULL ? nil : Date(timeIntervalSince1970: sqlite3_column_double(statement, 16)),
            accessCount: Int(sqlite3_column_int64(statement, 17)),
            lastAccessedAt: sqlite3_column_type(statement, 18) == SQLITE_NULL ? nil : Date(timeIntervalSince1970: sqlite3_column_double(statement, 18)),
            createdAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 19)),
            updatedAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 20))
        )
    }

    private func encodeUUIDs(_ ids: [UUID]) -> String {
        let strings = ids.map(\.uuidString)
        guard let data = try? JSONEncoder().encode(strings),
              let text = String(data: data, encoding: .utf8) else {
            return "[]"
        }
        return text
    }

    private func decodeUUIDs(_ text: String?) -> [UUID] {
        guard let text,
              let data = text.data(using: .utf8),
              let strings = try? JSONDecoder().decode([String].self, from: data) else {
            return []
        }
        return strings.compactMap { UUID(uuidString: $0) }
    }

    private func estimateTokens(_ text: String) -> Int {
        max(1, text.count / 4)
    }

    private func normalizedMemoryContent(_ value: String) -> String {
        value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
            .lowercased()
    }

    private func sha256(_ value: String) -> String {
        let digest = SHA256.hash(data: Data(value.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    private func preferredTier(_ lhs: MemoryTier, _ rhs: MemoryTier) -> MemoryTier {
        let order: [MemoryTier: Int] = [.core: 0, .working: 1, .archive: 2]
        return (order[lhs] ?? 9) <= (order[rhs] ?? 9) ? lhs : rhs
    }

    private func maxDate(_ lhs: Date?, _ rhs: Date?) -> Date? {
        switch (lhs, rhs) {
        case let (lhs?, rhs?):
            return max(lhs, rhs)
        case let (lhs?, nil):
            return lhs
        case let (nil, rhs?):
            return rhs
        case (nil, nil):
            return nil
        }
    }
}

private struct MemoryCountRow {
    var projectID: UUID?
    var active: Int
    var archived: Int
    var merged: Int
    var updatedAt: Date?
}

private enum SQLiteBinding {
    case text(String)
    case nullableText(String?)
    case int64(Int64)
    case double(Double)
    case nullableDouble(Double?)
}
