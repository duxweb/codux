import Foundation
import SQLite3

struct AIUsageStore: Sendable {
    private static let normalizedHistorySchemaVersion = 3
    private let aggregator = AIHistoryAggregationService()
    private let databaseFileURL: URL

    init(databaseURL: URL? = nil) {
        self.databaseFileURL = databaseURL ?? Self.defaultDatabaseURL()
    }

    private static func defaultDatabaseURL() -> URL {
        let fileManager = FileManager.default
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let root = appSupport.appendingPathComponent("dmux", isDirectory: true)
        try? fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        return root.appendingPathComponent("ai-usage.sqlite3")
    }

    private func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        var db: OpaquePointer?
        let path = databaseFileURL.path
        guard sqlite3_open(path, &db) == SQLITE_OK, let db else {
            defer { if db != nil { sqlite3_close(db) } }
            throw NSError(domain: "AIUsageStore", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to open AI usage database"])
        }
        defer { sqlite3_close(db) }
        try initializeIfNeeded(db)
        return try body(db)
    }

    private func initializeIfNeeded(_ db: OpaquePointer) throws {
        let statements = [
            """
            CREATE TABLE IF NOT EXISTS ai_history_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_file_state (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                file_modified_at REAL NOT NULL,
                PRIMARY KEY (source, file_path, project_path)
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_file_session (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_id TEXT NOT NULL,
                external_session_id TEXT,
                project_id TEXT NOT NULL,
                project_name TEXT NOT NULL,
                session_title TEXT NOT NULL,
                first_seen_at REAL NOT NULL,
                last_seen_at REAL NOT NULL,
                last_tool TEXT,
                last_model TEXT,
                request_count INTEGER NOT NULL,
                total_input_tokens INTEGER NOT NULL,
                total_output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                max_context_usage_percent REAL,
                active_duration_seconds INTEGER NOT NULL,
                today_tokens INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, session_id)
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_file_day_usage (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                day_start REAL NOT NULL,
                total_tokens INTEGER NOT NULL,
                request_count INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, day_start)
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_file_time_bucket (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                bucket_start REAL NOT NULL,
                bucket_end REAL NOT NULL,
                total_tokens INTEGER NOT NULL,
                request_count INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, bucket_start)
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_project_index_state (
                project_id TEXT PRIMARY KEY,
                project_name TEXT NOT NULL,
                project_path TEXT NOT NULL,
                indexed_at REAL NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_file_checkpoint (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                file_modified_at REAL NOT NULL,
                file_size INTEGER NOT NULL,
                last_offset INTEGER NOT NULL,
                last_indexed_at REAL NOT NULL,
                payload_json TEXT,
                PRIMARY KEY (source, file_path, project_path)
            );
            """,
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_state_project_path ON ai_history_file_state(project_path);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_checkpoint_project_path ON ai_history_file_checkpoint(project_path);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_session_project_path ON ai_history_file_session(project_path);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_day_usage_project_path ON ai_history_file_day_usage(project_path, day_start);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_time_bucket_project_path ON ai_history_file_time_bucket(project_path, bucket_start);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_project_index_state_indexed_at ON ai_history_project_index_state(indexed_at DESC);"
        ]

        for statement in statements {
            guard sqlite3_exec(db, statement, nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to initialize AI usage database"])
            }
        }

        try migrateNormalizedHistoryIfNeeded(db)
    }

    private func migrateNormalizedHistoryIfNeeded(_ db: OpaquePointer) throws {
        let storedVersion = schemaVersion(db)
        guard storedVersion != Self.normalizedHistorySchemaVersion else {
            return
        }

        let resetStatements = [
            "DELETE FROM ai_history_file_time_bucket;",
            "DELETE FROM ai_history_file_day_usage;",
            "DELETE FROM ai_history_file_session;",
            "DELETE FROM ai_history_file_checkpoint;",
            "DELETE FROM ai_history_file_state;",
            "DELETE FROM ai_history_project_index_state;",
        ]

        for statement in resetStatements {
            guard sqlite3_exec(db, statement, nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to reset normalized AI history tables"])
            }
        }

        try execute(
            db,
            sql: """
                INSERT INTO ai_history_meta (key, value)
                VALUES ('normalized_history_schema_version', ?)
                ON CONFLICT(key) DO UPDATE SET value=excluded.value;
            """,
            bindings: [String(Self.normalizedHistorySchemaVersion)]
        )
    }

    private func schemaVersion(_ db: OpaquePointer) -> Int? {
        let sql = "SELECT value FROM ai_history_meta WHERE key = 'normalized_history_schema_version' LIMIT 1;"
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }

        guard sqlite3_step(statement) == SQLITE_ROW,
              let rawValue = sqlite3_column_text(statement, 0) else {
            return nil
        }

        return Int(String(cString: rawValue))
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
                  let rawProjectName = sqlite3_column_text(statement, 0),
                  let rawProjectPath = sqlite3_column_text(statement, 1) else {
                return nil
            }
            let project = Project(
                id: projectID,
                name: String(cString: rawProjectName),
                path: String(cString: rawProjectPath),
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
                    projectTotalTokens: summary.sessions.reduce(0) { $0 + $1.totalTokens },
                    todayTotalTokens: todayTotal,
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
                indexedAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 2))
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

            let fileModifiedAt = sqlite3_column_double(statement, 0)
            let fileSize = UInt64(max(0, sqlite3_column_int64(statement, 1)))
            let lastOffset = UInt64(max(0, sqlite3_column_int64(statement, 2)))
            let lastIndexedAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 3))
            let payloadJSON = sqlite3_column_text(statement, 4).map { String(cString: $0) }
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
                modifiedAt: sqlite3_column_double(statement, 0)
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
            let sql = """
            SELECT file_path, file_modified_at
            FROM ai_history_file_state
            WHERE source = ? AND project_path = ?;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return []
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 2, projectPath, -1, SQLITE_TRANSIENT)

            var items: [AIExternalFileSummary] = []
            while sqlite3_step(statement) == SQLITE_ROW {
                guard let rawPath = sqlite3_column_text(statement, 0) else { continue }
                let filePath = String(cString: rawPath)
                let modifiedAt = sqlite3_column_double(statement, 1)
                if let item = loadNormalizedExternalSummary(
                    db: db,
                    source: source,
                    filePath: filePath,
                    projectPath: projectPath,
                    modifiedAt: modifiedAt
                ) {
                    items.append(item)
                }
            }
            return items
        }) ?? []
    }

    func deleteExternalSummaries(projectPath: String) {
        try? withDatabase { db in
            try execute(db, sql: "DELETE FROM ai_history_file_checkpoint WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_state WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_session WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_day_usage WHERE project_path = ?;", bindings: [projectPath])
            try execute(db, sql: "DELETE FROM ai_history_file_time_bucket WHERE project_path = ?;", bindings: [projectPath])
        }
    }

    private func hasNormalizedExternalSummary(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String,
        modifiedAt: Double
    ) -> Bool {
        let sql = """
        SELECT 1
        FROM ai_history_file_state
        WHERE source = ?
          AND file_path = ?
          AND project_path = ?
          AND file_modified_at = ?
        LIMIT 1;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return false
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(statement, 2, filePath, -1, SQLITE_TRANSIENT)
        sqlite3_bind_text(statement, 3, projectPath, -1, SQLITE_TRANSIENT)
        sqlite3_bind_double(statement, 4, modifiedAt)
        return sqlite3_step(statement) == SQLITE_ROW
    }

    private func loadNormalizedExternalSummary(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String,
        modifiedAt: Double
    ) -> AIExternalFileSummary? {
        guard let sessions = loadNormalizedSessions(db: db, source: source, filePath: filePath, projectPath: projectPath),
              let dayUsage = loadNormalizedDayUsage(db: db, source: source, filePath: filePath, projectPath: projectPath),
              let timeBuckets = loadNormalizedTimeBuckets(db: db, source: source, filePath: filePath, projectPath: projectPath) else {
            return nil
        }
        return AIExternalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: modifiedAt,
            projectPath: projectPath,
            sessions: sessions,
            dayUsage: dayUsage,
            timeBuckets: timeBuckets
        )
    }

    private func replaceNormalizedExternalSummary(
        db: OpaquePointer,
        summary: AIExternalFileSummary,
        checkpoint: AIExternalFileCheckpoint?
    ) throws {
        guard sqlite3_exec(db, "BEGIN IMMEDIATE TRANSACTION;", nil, nil, nil) == SQLITE_OK else {
            throw NSError(domain: "AIUsageStore", code: 10, userInfo: [NSLocalizedDescriptionKey: "Failed to begin AI history transaction"])
        }
        do {
            try execute(
                db,
                sql: "DELETE FROM ai_history_file_session WHERE source = ? AND file_path = ? AND project_path = ?;",
                bindings: [summary.source, summary.filePath, summary.projectPath]
            )
            try execute(
                db,
                sql: "DELETE FROM ai_history_file_day_usage WHERE source = ? AND file_path = ? AND project_path = ?;",
                bindings: [summary.source, summary.filePath, summary.projectPath]
            )
            try execute(
                db,
                sql: "DELETE FROM ai_history_file_time_bucket WHERE source = ? AND file_path = ? AND project_path = ?;",
                bindings: [summary.source, summary.filePath, summary.projectPath]
            )
            try execute(
                db,
                sql: """
                    INSERT INTO ai_history_file_state (source, file_path, project_path, file_modified_at)
                    VALUES (?, ?, ?, ?)
                    ON CONFLICT(source, file_path, project_path) DO UPDATE SET
                        file_modified_at = excluded.file_modified_at;
                """,
                bindings: [summary.source, summary.filePath, summary.projectPath, summary.fileModifiedAt]
            )

            if let checkpoint {
                try execute(
                    db,
                    sql: """
                        INSERT INTO ai_history_file_checkpoint (
                            source, file_path, project_path, file_modified_at,
                            file_size, last_offset, last_indexed_at, payload_json
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                        ON CONFLICT(source, file_path, project_path) DO UPDATE SET
                            file_modified_at = excluded.file_modified_at,
                            file_size = excluded.file_size,
                            last_offset = excluded.last_offset,
                            last_indexed_at = excluded.last_indexed_at,
                            payload_json = excluded.payload_json;
                    """,
                    bindings: [
                        checkpoint.source,
                        checkpoint.filePath,
                        checkpoint.projectPath,
                        checkpoint.fileModifiedAt,
                        Int(clamping: checkpoint.fileSize),
                        Int(clamping: checkpoint.lastOffset),
                        checkpoint.lastIndexedAt.timeIntervalSince1970,
                        encodeCheckpointPayload(checkpoint.payload) as Any,
                    ]
                )
            } else {
                try execute(
                    db,
                    sql: "DELETE FROM ai_history_file_checkpoint WHERE source = ? AND file_path = ? AND project_path = ?;",
                    bindings: [summary.source, summary.filePath, summary.projectPath]
                )
            }

            for session in summary.sessions {
                try execute(
                    db,
                    sql: """
                        INSERT INTO ai_history_file_session (
                            source, file_path, project_path, session_id, external_session_id,
                            project_id, project_name, session_title, first_seen_at, last_seen_at,
                            last_tool, last_model, request_count, total_input_tokens,
                            total_output_tokens, total_tokens, max_context_usage_percent,
                            active_duration_seconds, today_tokens
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
                    """,
                    bindings: [
                        summary.source,
                        summary.filePath,
                        summary.projectPath,
                        session.sessionID.uuidString,
                        session.externalSessionID as Any,
                        session.projectID.uuidString,
                        session.projectName,
                        session.sessionTitle,
                        session.firstSeenAt.timeIntervalSince1970,
                        session.lastSeenAt.timeIntervalSince1970,
                        session.lastTool as Any,
                        session.lastModel as Any,
                        session.requestCount,
                        session.totalInputTokens,
                        session.totalOutputTokens,
                        session.totalTokens,
                        session.maxContextUsagePercent as Any,
                        session.activeDurationSeconds,
                        session.todayTokens,
                    ]
                )
            }

            for day in summary.dayUsage {
                try execute(
                    db,
                    sql: """
                        INSERT INTO ai_history_file_day_usage (
                            source, file_path, project_path, day_start, total_tokens, request_count
                        ) VALUES (?, ?, ?, ?, ?, ?);
                    """,
                    bindings: [
                        summary.source,
                        summary.filePath,
                        summary.projectPath,
                        day.day.timeIntervalSince1970,
                        day.totalTokens,
                        day.requestCount,
                    ]
                )
            }

            for bucket in summary.timeBuckets {
                try execute(
                    db,
                    sql: """
                        INSERT INTO ai_history_file_time_bucket (
                            source, file_path, project_path, bucket_start, bucket_end, total_tokens, request_count
                        ) VALUES (?, ?, ?, ?, ?, ?, ?);
                    """,
                    bindings: [
                        summary.source,
                        summary.filePath,
                        summary.projectPath,
                        bucket.start.timeIntervalSince1970,
                        bucket.end.timeIntervalSince1970,
                        bucket.totalTokens,
                        bucket.requestCount,
                    ]
                )
            }

            guard sqlite3_exec(db, "COMMIT;", nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 11, userInfo: [NSLocalizedDescriptionKey: "Failed to commit AI history transaction"])
            }
        } catch {
            sqlite3_exec(db, "ROLLBACK;", nil, nil, nil)
            throw error
        }
    }

    private func loadNormalizedSessions(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String
    ) -> [AISessionSummary]? {
        let sql = """
        SELECT session_id, external_session_id, project_id, project_name, session_title,
               first_seen_at, last_seen_at, last_tool, last_model, request_count,
               total_input_tokens, total_output_tokens, total_tokens,
               max_context_usage_percent, active_duration_seconds, today_tokens
        FROM ai_history_file_session
        WHERE source = ? AND file_path = ? AND project_path = ?
        ORDER BY last_seen_at DESC;
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

        var items: [AISessionSummary] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawSessionID = sqlite3_column_text(statement, 0),
                  let sessionID = UUID(uuidString: String(cString: rawSessionID)),
                  let rawProjectID = sqlite3_column_text(statement, 2),
                  let projectID = UUID(uuidString: String(cString: rawProjectID)),
                  let rawProjectName = sqlite3_column_text(statement, 3),
                  let rawSessionTitle = sqlite3_column_text(statement, 4) else {
                continue
            }
            let externalSessionID = sqlite3_column_text(statement, 1).map { String(cString: $0) }
            let firstSeenAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 5))
            let lastSeenAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 6))
            let lastTool = sqlite3_column_text(statement, 7).map { String(cString: $0) }
            let lastModel = sqlite3_column_text(statement, 8).map { String(cString: $0) }
            let maxContextUsagePercent = sqlite3_column_type(statement, 13) == SQLITE_NULL ? nil : sqlite3_column_double(statement, 13)
            items.append(
                AISessionSummary(
                    sessionID: sessionID,
                    externalSessionID: externalSessionID,
                    projectID: projectID,
                    projectName: String(cString: rawProjectName),
                    sessionTitle: String(cString: rawSessionTitle),
                    firstSeenAt: firstSeenAt,
                    lastSeenAt: lastSeenAt,
                    lastTool: lastTool,
                    lastModel: lastModel,
                    requestCount: Int(sqlite3_column_int64(statement, 9)),
                    totalInputTokens: Int(sqlite3_column_int64(statement, 10)),
                    totalOutputTokens: Int(sqlite3_column_int64(statement, 11)),
                    totalTokens: Int(sqlite3_column_int64(statement, 12)),
                    maxContextUsagePercent: maxContextUsagePercent,
                    activeDurationSeconds: Int(sqlite3_column_int64(statement, 14)),
                    todayTokens: Int(sqlite3_column_int64(statement, 15))
                )
            )
        }
        return items
    }

    private func loadNormalizedDayUsage(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String
    ) -> [AIHeatmapDay]? {
        let sql = """
        SELECT day_start, total_tokens, request_count
        FROM ai_history_file_day_usage
        WHERE source = ? AND file_path = ? AND project_path = ?
        ORDER BY day_start ASC;
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

        var items: [AIHeatmapDay] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            items.append(
                AIHeatmapDay(
                    day: Date(timeIntervalSince1970: sqlite3_column_double(statement, 0)),
                    totalTokens: Int(sqlite3_column_int64(statement, 1)),
                    requestCount: Int(sqlite3_column_int64(statement, 2))
                )
            )
        }
        return items
    }

    private func loadNormalizedTimeBuckets(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String
    ) -> [AITimeBucket]? {
        let sql = """
        SELECT bucket_start, bucket_end, total_tokens, request_count
        FROM ai_history_file_time_bucket
        WHERE source = ? AND file_path = ? AND project_path = ?
        ORDER BY bucket_start ASC;
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

        var items: [AITimeBucket] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            items.append(
                AITimeBucket(
                    start: Date(timeIntervalSince1970: sqlite3_column_double(statement, 0)),
                    end: Date(timeIntervalSince1970: sqlite3_column_double(statement, 1)),
                    totalTokens: Int(sqlite3_column_int64(statement, 2)),
                    requestCount: Int(sqlite3_column_int64(statement, 3))
                )
            )
        }
        return items
    }

    private func decodeCheckpointPayload(_ payloadJSON: String?) -> AIExternalFileCheckpointPayload? {
        guard let payloadJSON,
              let data = payloadJSON.data(using: .utf8) else {
            return nil
        }
        return try? JSONDecoder().decode(AIExternalFileCheckpointPayload.self, from: data)
    }

    private func encodeCheckpointPayload(_ payload: AIExternalFileCheckpointPayload?) -> String? {
        guard let payload,
              let data = try? JSONEncoder().encode(payload),
              let json = String(data: data, encoding: .utf8) else {
            return nil
        }
        return json
    }

    private func execute(_ db: OpaquePointer, sql: String, bindings: [Any]) throws {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            throw NSError(domain: "AIUsageStore", code: 7)
        }
        defer { sqlite3_finalize(statement) }

        for (index, value) in bindings.enumerated() {
            let position = Int32(index + 1)
            switch value {
            case let string as String:
                sqlite3_bind_text(statement, position, string, -1, SQLITE_TRANSIENT)
            case let int as Int:
                sqlite3_bind_int64(statement, position, sqlite3_int64(int))
            case let double as Double:
                sqlite3_bind_double(statement, position, double)
            case let uuid as UUID:
                sqlite3_bind_text(statement, position, uuid.uuidString, -1, SQLITE_TRANSIENT)
            case Optional<Any>.none:
                sqlite3_bind_null(statement, position)
            default:
                if value is NSNull {
                    sqlite3_bind_null(statement, position)
                } else {
                    sqlite3_bind_text(statement, position, String(describing: value), -1, SQLITE_TRANSIENT)
                }
            }
        }

        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw NSError(domain: "AIUsageStore", code: 8, userInfo: [NSLocalizedDescriptionKey: String(cString: sqlite3_errmsg(db))])
        }
    }

}

private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
