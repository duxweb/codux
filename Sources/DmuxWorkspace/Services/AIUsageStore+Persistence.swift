import Foundation
import SQLite3

extension AIUsageStore {
    private func columnText(_ statement: OpaquePointer?, index: Int32) -> String? {
        sqlite3_column_text(statement, index).map { String(cString: $0) }
    }

    private func columnInt(_ statement: OpaquePointer?, index: Int32) -> Int {
        Int(sqlite3_column_int64(statement, index))
    }

    private func columnDouble(_ statement: OpaquePointer?, index: Int32) -> Double {
        sqlite3_column_double(statement, index)
    }

    private func columnDate(_ statement: OpaquePointer?, index: Int32) -> Date {
        Date(timeIntervalSince1970: columnDouble(statement, index: index))
    }

    private func bindOptionalDouble(
        _ value: Double?,
        to statement: OpaquePointer,
        positions: ClosedRange<Int32>
    ) {
        for position in positions {
            if let value {
                sqlite3_bind_double(statement, position, value)
            } else {
                sqlite3_bind_null(statement, position)
            }
        }
    }

    func globalTodayNormalizedTokens(now: Date = .init()) -> Int {
        let calendar = Calendar.autoupdatingCurrent
        let startOfDay = calendar.startOfDay(for: now)
        let endOfDay = calendar.date(byAdding: .day, value: 1, to: startOfDay) ?? now

        return (try? withDatabase { db in
            let sql = """
            SELECT COALESCE(SUM(total_tokens), 0)
            FROM ai_history_file_usage_bucket
            WHERE bucket_end > ? AND bucket_start < ?;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
                  let statement else {
                return 0
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_double(statement, 1, startOfDay.timeIntervalSince1970)
            sqlite3_bind_double(statement, 2, endOfDay.timeIntervalSince1970)

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return 0
            }
            return columnInt(statement, index: 0)
        }) ?? 0
    }

    func globalAllTimeNormalizedTokens() -> Int {
        (try? withDatabase { db in
            let sql = """
            SELECT COALESCE(SUM(total_tokens), 0)
            FROM ai_history_file_usage_bucket;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
                  let statement else {
                return 0
            }
            defer { sqlite3_finalize(statement) }

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return 0
            }
            return columnInt(statement, index: 0)
        }) ?? 0
    }

    func indexedSessions(since cutoff: Date?, projectIDs: Set<UUID>? = nil) -> [AISessionSummary] {
        if let projectIDs, projectIDs.isEmpty {
            return []
        }

        let calendar = Calendar.autoupdatingCurrent
        let startOfDay = calendar.startOfDay(for: Date())
        let endOfDay = calendar.date(byAdding: .day, value: 1, to: startOfDay) ?? startOfDay
        let normalizedProjectIDs = projectIDs?.map(\.uuidString).sorted()
        let projectFilterSQL: String
        if let normalizedProjectIDs, !normalizedProjectIDs.isEmpty {
            let placeholders = Array(repeating: "?", count: normalizedProjectIDs.count).joined(separator: ",")
            projectFilterSQL = " AND session.project_id IN (\(placeholders))"
        } else {
            projectFilterSQL = ""
        }

        return (try? withDatabase { db in
            let sql = """
            SELECT
                session.project_id,
                session.project_name,
                session.session_title,
                session.first_seen_at,
                session.last_seen_at,
                session.last_model,
                session.external_session_id,
                session.source,
                CASE
                    WHEN ? IS NULL THEN session.active_duration_seconds
                    WHEN session.last_seen_at <= ? THEN 0
                    ELSE CAST(
                        MIN(
                            session.active_duration_seconds,
                            MAX(0, session.last_seen_at - MAX(session.first_seen_at, ?))
                        ) AS INTEGER
                    )
                END,
                COALESCE(SUM(CASE WHEN ? IS NULL OR bucket.bucket_start >= ? THEN bucket.request_count ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ? IS NULL OR bucket.bucket_start >= ? THEN bucket.input_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ? IS NULL OR bucket.bucket_start >= ? THEN bucket.output_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ? IS NULL OR bucket.bucket_start >= ? THEN bucket.total_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ? IS NULL OR bucket.bucket_start >= ? THEN bucket.cached_input_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ? AND bucket.bucket_start < ? THEN bucket.total_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ? AND bucket.bucket_start < ? THEN bucket.cached_input_tokens ELSE 0 END), 0)
            FROM ai_history_file_session_link AS session
            LEFT JOIN ai_history_file_usage_bucket AS bucket
              ON bucket.source = session.source
             AND bucket.file_path = session.file_path
             AND bucket.project_path = session.project_path
             AND bucket.session_key = session.session_key
            WHERE (? IS NULL OR session.last_seen_at >= ?)
            \(projectFilterSQL)
            GROUP BY
                session.source,
                session.file_path,
                session.project_path,
                session.session_key,
                session.project_id,
                session.project_name,
                session.session_title,
                session.first_seen_at,
                session.last_seen_at,
                session.last_model,
                session.external_session_id,
                session.active_duration_seconds
            ORDER BY session.last_seen_at DESC;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
                  let statement else {
                return []
            }
            defer { sqlite3_finalize(statement) }

            let cutoffSeconds = cutoff?.timeIntervalSince1970
            bindOptionalDouble(cutoffSeconds, to: statement, positions: 1 ... 13)
            sqlite3_bind_double(statement, 14, startOfDay.timeIntervalSince1970)
            sqlite3_bind_double(statement, 15, endOfDay.timeIntervalSince1970)
            sqlite3_bind_double(statement, 16, startOfDay.timeIntervalSince1970)
            sqlite3_bind_double(statement, 17, endOfDay.timeIntervalSince1970)
            bindOptionalDouble(cutoffSeconds, to: statement, positions: 18 ... 19)

            if let normalizedProjectIDs {
                for (offset, projectID) in normalizedProjectIDs.enumerated() {
                    sqlite3_bind_text(
                        statement,
                        Int32(20 + offset),
                        projectID,
                        -1,
                        SQLITE_TRANSIENT
                    )
                }
            }

            var items: [AISessionSummary] = []
            while sqlite3_step(statement) == SQLITE_ROW {
                guard let rawProjectID = columnText(statement, index: 0),
                      let projectID = UUID(uuidString: rawProjectID),
                      let projectName = columnText(statement, index: 1),
                      let sessionTitle = columnText(statement, index: 2) else {
                    continue
                }

                items.append(
                    AISessionSummary(
                        sessionID: UUID(),
                        externalSessionID: columnText(statement, index: 6),
                        projectID: projectID,
                        projectName: projectName,
                        sessionTitle: sessionTitle,
                        firstSeenAt: columnDate(statement, index: 3),
                        lastSeenAt: columnDate(statement, index: 4),
                        lastTool: columnText(statement, index: 7),
                        lastModel: columnText(statement, index: 5),
                        requestCount: columnInt(statement, index: 9),
                        totalInputTokens: columnInt(statement, index: 10),
                        totalOutputTokens: columnInt(statement, index: 11),
                        totalTokens: columnInt(statement, index: 12),
                        cachedInputTokens: columnInt(statement, index: 13),
                        maxContextUsagePercent: nil,
                        activeDurationSeconds: columnInt(statement, index: 8),
                        todayTokens: columnInt(statement, index: 14),
                        todayCachedInputTokens: columnInt(statement, index: 15)
                    )
                )
            }
            return items
        }) ?? []
    }

    func indexedProjectSnapshot(projectID: UUID) -> AIIndexedProjectSnapshot? {
        try? withDatabase { db in
            let sql = """
            SELECT project_name, project_path, indexed_at
            FROM ai_history_project_index_state
            WHERE project_id = ?
            LIMIT 1;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, projectID.uuidString, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_ROW,
                  let projectName = columnText(statement, index: 0),
                  let projectPath = columnText(statement, index: 1) else {
                return nil
            }
            let project = Project(
                id: projectID,
                name: projectName,
                path: projectPath,
                shell: "/bin/zsh",
                defaultCommand: "",
                badgeText: nil,
                badgeSymbol: nil,
                badgeColorHex: nil,
                gitDefaultPushRemoteName: nil
            )
            let sources = ["claude", "codex", "gemini", "opencode"]
            let fileSummaries = sources.flatMap { storedExternalSummaries(source: $0, projectPath: project.path) }
            let summary = aggregator.buildProjectSummary(project: project, fileSummaries: fileSummaries)
            let todayTotal = summary.todayTimeBuckets.reduce(0) { $0 + $1.totalTokens }
            return AIIndexedProjectSnapshot(
                projectID: project.id,
                projectName: project.name,
                projectSummary: AIProjectUsageSummary(
                    projectID: project.id,
                    projectName: project.name,
                    currentSessionTokens: 0,
                    currentSessionCachedInputTokens: 0,
                    projectTotalTokens: summary.sessions.reduce(0) { $0 + $1.totalTokens },
                    projectCachedInputTokens: summary.sessions.reduce(0) { $0 + $1.cachedInputTokens },
                    todayTotalTokens: todayTotal,
                    todayCachedInputTokens: summary.todayTimeBuckets.reduce(0) { $0 + $1.cachedInputTokens },
                    currentTool: nil,
                    currentModel: nil,
                    currentContextUsagePercent: nil,
                    currentContextUsedTokens: nil,
                    currentContextWindow: nil,
                    currentSessionUpdatedAt: summary.sessions.first?.lastSeenAt
                ),
                sessions: summary.sessions,
                heatmap: summary.heatmap,
                todayTimeBuckets: summary.todayTimeBuckets,
                toolBreakdown: summary.toolBreakdown,
                modelBreakdown: summary.modelBreakdown,
                indexedAt: columnDate(statement, index: 2)
            )
        }
    }

    func saveProjectIndexState(for snapshot: AIIndexedProjectSnapshot, projectPath: String) {
        try? withDatabase { db in
            try execute(db, sql: """
                INSERT INTO ai_history_project_index_state (project_id, project_name, project_path, indexed_at)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(project_id) DO UPDATE SET
                    project_name=excluded.project_name,
                    project_path=excluded.project_path,
                    indexed_at=excluded.indexed_at;
            """, bindings: [
                snapshot.projectID.uuidString,
                snapshot.projectName,
                projectPath,
                snapshot.indexedAt.timeIntervalSince1970,
            ])
        }
    }

    func deleteProjectIndexState(projectID: UUID) {
        try? withDatabase { db in
            try execute(db, sql: "DELETE FROM ai_history_project_index_state WHERE project_id = ?;", bindings: [projectID.uuidString])
        }
    }

    func externalFileCheckpoint(
        source: String,
        filePath: String,
        projectPath: String
    ) -> AIExternalFileCheckpoint? {
        try? withDatabase { db in
            let sql = """
            SELECT file_modified_at, file_size, last_offset, last_indexed_at, payload_json
            FROM ai_history_file_checkpoint
            WHERE source = ? AND file_path = ? AND project_path = ?
            LIMIT 1;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
                  let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 2, filePath, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 3, projectPath, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return nil
            }

            let fileModifiedAt = columnDouble(statement, index: 0)
            let fileSize = UInt64(max(0, Int64(columnInt(statement, index: 1))))
            let lastOffset = UInt64(max(0, Int64(columnInt(statement, index: 2))))
            let lastIndexedAt = columnDate(statement, index: 3)
            let payloadJSON = columnText(statement, index: 4)
            let payload = decodeCheckpointPayload(payloadJSON)
            return AIExternalFileCheckpoint(
                source: source,
                filePath: filePath,
                projectPath: projectPath,
                fileModifiedAt: fileModifiedAt,
                fileSize: fileSize,
                lastOffset: lastOffset,
                lastIndexedAt: lastIndexedAt,
                payload: payload
            )
        }
    }

    func storedExternalSummary(source: String, filePath: String, projectPath: String, modifiedAt: Double) -> AIExternalFileSummary? {
        try? withDatabase { db in
            guard hasNormalizedExternalSummary(
                db: db,
                source: source,
                filePath: filePath,
                projectPath: projectPath,
                modifiedAt: modifiedAt
            ) else {
                return nil
            }
            return loadNormalizedExternalSummary(
                db: db,
                source: source,
                filePath: filePath,
                projectPath: projectPath,
                modifiedAt: modifiedAt
            )
        }
    }

    func storedExternalSummary(
        source: String,
        filePath: String,
        projectPath: String
    ) -> AIExternalFileSummary? {
        try? withDatabase { db in
            let sql = """
            SELECT file_modified_at
            FROM ai_history_file_state
            WHERE source = ? AND file_path = ? AND project_path = ?
            LIMIT 1;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
                  let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 2, filePath, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 3, projectPath, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_ROW else {
                return nil
            }

            return loadNormalizedExternalSummary(
                db: db,
                source: source,
                filePath: filePath,
                projectPath: projectPath,
                modifiedAt: columnDouble(statement, index: 0)
            )
        }
    }

    func saveExternalSummary(_ summary: AIExternalFileSummary) {
        try? withDatabase { db in
            try replaceNormalizedExternalSummary(db: db, summary: summary, checkpoint: nil)
        }
    }

    func saveExternalSummary(
        _ summary: AIExternalFileSummary,
        checkpoint: AIExternalFileCheckpoint?
    ) {
        try? withDatabase { db in
            try replaceNormalizedExternalSummary(db: db, summary: summary, checkpoint: checkpoint)
        }
    }

    func storedExternalSummaries(source: String, projectPath: String) -> [AIExternalFileSummary] {
        (try? withDatabase { db in
            let fileStates = loadNormalizedFileStates(
                db: db,
                source: source,
                projectPath: projectPath
            )
            guard fileStates.isEmpty == false else {
                return []
            }

            let sessionLinksByFile = loadNormalizedSessionLinksByFile(
                db: db,
                source: source,
                projectPath: projectPath
            )
            let usageBucketsByFile = loadNormalizedUsageBucketsByFile(
                db: db,
                source: source,
                projectPath: projectPath,
                sessionLinksByFile: sessionLinksByFile
            )

            return fileStates.map { fileState in
                aggregator.externalFileSummary(
                    source: source,
                    filePath: fileState.filePath,
                    fileModifiedAt: fileState.modifiedAt,
                    projectPath: projectPath,
                    usageBuckets: usageBucketsByFile[fileState.filePath] ?? []
                )
            }
        }) ?? []
    }

    func deleteExternalSummaries(projectPath: String) {
        try? withDatabase { db in
            try execute(db, sql: "DELETE FROM ai_history_file_checkpoint WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_state WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_session_link WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_usage_bucket WHERE project_path = ?;", bindings: [projectPath])
        }
    }

    private func loadNormalizedFileStates(
        db: OpaquePointer,
        source: String,
        projectPath: String
    ) -> [(filePath: String, modifiedAt: Double)] {
        let sql = """
        SELECT file_path, file_modified_at
        FROM ai_history_file_state
        WHERE source = ? AND project_path = ?
        ORDER BY file_path ASC;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return []
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(statement, 2, projectPath, -1, SQLITE_TRANSIENT)

        var items: [(filePath: String, modifiedAt: Double)] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let filePath = columnText(statement, index: 0) else {
                continue
            }
            items.append((filePath: filePath, modifiedAt: columnDouble(statement, index: 1)))
        }
        return items
    }

    private func loadNormalizedSessionLinksByFile(
        db: OpaquePointer,
        source: String,
        projectPath: String
    ) -> [String: [String: NormalizedSessionLinkRow]] {
        let sql = """
        SELECT file_path, session_key, external_session_id, project_id, project_name, session_title,
               first_seen_at, last_seen_at, last_model, active_duration_seconds
        FROM ai_history_file_session_link
        WHERE source = ? AND project_path = ?
        ORDER BY file_path ASC, last_seen_at DESC;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return [:]
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(statement, 2, projectPath, -1, SQLITE_TRANSIENT)

        var items: [String: [String: NormalizedSessionLinkRow]] = [:]
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let filePath = columnText(statement, index: 0),
                  let sessionKey = columnText(statement, index: 1),
                  let rawProjectID = columnText(statement, index: 3),
                  let projectID = UUID(uuidString: rawProjectID),
                  let projectName = columnText(statement, index: 4),
                  let sessionTitle = columnText(statement, index: 5) else {
                continue
            }

            let firstSeenAt = columnDate(statement, index: 6)
            let lastSeenAt = columnDate(statement, index: 7)
            items[filePath, default: [:]][sessionKey] = NormalizedSessionLinkRow(
                sessionKey: sessionKey,
                externalSessionID: columnText(statement, index: 2),
                projectID: projectID,
                projectName: projectName,
                sessionTitle: sessionTitle,
                firstSeenAt: firstSeenAt,
                lastSeenAt: lastSeenAt,
                lastModel: columnText(statement, index: 8),
                activeDurationSeconds: sanitizedActiveDurationSeconds(
                    columnInt(statement, index: 9),
                    firstSeenAt: firstSeenAt,
                    lastSeenAt: lastSeenAt
                )
            )
        }
        return items
    }

    private func loadNormalizedUsageBucketsByFile(
        db: OpaquePointer,
        source: String,
        projectPath: String,
        sessionLinksByFile: [String: [String: NormalizedSessionLinkRow]]
    ) -> [String: [AIUsageBucket]] {
        let sql = """
        SELECT file_path, session_key, model, bucket_start, bucket_end, input_tokens, output_tokens,
               total_tokens, cached_input_tokens, request_count
        FROM ai_history_file_usage_bucket
        WHERE source = ? AND project_path = ?
        ORDER BY file_path ASC, bucket_start ASC, session_key ASC, model ASC;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return [:]
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(statement, 2, projectPath, -1, SQLITE_TRANSIENT)

        var rows: [(filePath: String, row: NormalizedUsageBucketRow)] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let filePath = columnText(statement, index: 0),
                  let sessionKey = columnText(statement, index: 1) else {
                continue
            }

            rows.append((
                filePath: filePath,
                row: NormalizedUsageBucketRow(
                    sessionKey: sessionKey,
                    model: columnText(statement, index: 2),
                    bucketStart: columnDate(statement, index: 3),
                    bucketEnd: columnDate(statement, index: 4),
                    inputTokens: columnInt(statement, index: 5),
                    outputTokens: columnInt(statement, index: 6),
                    totalTokens: columnInt(statement, index: 7),
                    cachedInputTokens: columnInt(statement, index: 8),
                    requestCount: columnInt(statement, index: 9)
                )
            ))
        }

        let lastBucketStartByFileAndSession = rows.reduce(into: [String: [String: Date]]()) { partial, item in
            let current = partial[item.filePath]?[item.row.sessionKey] ?? item.row.bucketStart
            partial[item.filePath, default: [:]][item.row.sessionKey] = max(current, item.row.bucketStart)
        }

        return rows.reduce(into: [String: [AIUsageBucket]]()) { partial, item in
            guard let session = sessionLinksByFile[item.filePath]?[item.row.sessionKey] else {
                return
            }

            let bucket = AIUsageBucket(
                source: source,
                sessionKey: item.row.sessionKey,
                externalSessionID: session.externalSessionID,
                sessionTitle: session.sessionTitle,
                model: normalizedNonEmptyString(item.row.model),
                projectID: session.projectID,
                projectName: session.projectName,
                bucketStart: item.row.bucketStart,
                bucketEnd: item.row.bucketEnd,
                inputTokens: item.row.inputTokens,
                outputTokens: item.row.outputTokens,
                totalTokens: item.row.totalTokens,
                cachedInputTokens: item.row.cachedInputTokens,
                requestCount: item.row.requestCount,
                activeDurationSeconds: lastBucketStartByFileAndSession[item.filePath]?[item.row.sessionKey] == item.row.bucketStart ? session.activeDurationSeconds : 0,
                firstSeenAt: session.firstSeenAt,
                lastSeenAt: session.lastSeenAt
            )
            partial[item.filePath, default: []].append(bucket)
        }
    }
}
