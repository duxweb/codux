import Foundation
import SQLite3

struct AIUsageStore: Sendable {
    private static let normalizedHistorySchemaVersion = 6
    let aggregator = AIHistoryAggregationService()
    private let databaseFileURL: URL

    init(databaseURL: URL? = nil) {
        self.databaseFileURL = databaseURL ?? Self.defaultDatabaseURL()
    }

    private static func defaultDatabaseURL() -> URL {
        let fileManager = FileManager.default
        let root = AppRuntimePaths.appSupportRootURL(fileManager: fileManager)!
        try? fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        return root.appendingPathComponent("ai-usage.sqlite3")
    }

    func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        var db: OpaquePointer?
        let path = databaseFileURL.path
        guard sqlite3_open(path, &db) == SQLITE_OK, let db else {
            defer { if db != nil { sqlite3_close(db) } }
            throw NSError(domain: "AIUsageStore", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to open AI usage database"])
        }
        defer { sqlite3_close(db) }
        sqlite3_busy_timeout(db, 3000)
        try initializeIfNeeded(db)
        return try body(db)
    }

    private func initializeIfNeeded(_ db: OpaquePointer) throws {
        let statements = normalizedSchemaStatements()

        for statement in statements {
            guard sqlite3_exec(db, statement, nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to initialize AI usage database"])
            }
        }

        try migrateNormalizedHistoryIfNeeded(db)
    }

    private func normalizedSchemaStatements() -> [String] {
        [
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
            CREATE TABLE IF NOT EXISTS ai_history_file_session_link (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                external_session_id TEXT,
                project_id TEXT NOT NULL,
                project_name TEXT NOT NULL,
                session_title TEXT NOT NULL,
                first_seen_at REAL NOT NULL,
                last_seen_at REAL NOT NULL,
                last_model TEXT,
                active_duration_seconds INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, session_key)
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_history_file_usage_bucket (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                model TEXT NOT NULL,
                bucket_start REAL NOT NULL,
                bucket_end REAL NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL,
                request_count INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start)
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
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_session_link_project_path ON ai_history_file_session_link(project_path);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_bucket_project_path ON ai_history_file_usage_bucket(project_path, bucket_start);",
            "CREATE INDEX IF NOT EXISTS idx_ai_history_project_index_state_indexed_at ON ai_history_project_index_state(indexed_at DESC);"
        ]
    }

    private func migrateNormalizedHistoryIfNeeded(_ db: OpaquePointer) throws {
        let storedVersion = schemaVersion(db)
        guard storedVersion != Self.normalizedHistorySchemaVersion else {
            return
        }

        let resetStatements = [
            "DROP TABLE IF EXISTS ai_history_file_usage_bucket;",
            "DROP TABLE IF EXISTS ai_history_file_session_link;",
            "DROP TABLE IF EXISTS ai_history_file_time_bucket;",
            "DROP TABLE IF EXISTS ai_history_file_day_usage;",
            "DROP TABLE IF EXISTS ai_history_file_session;",
            "DROP TABLE IF EXISTS ai_history_file_checkpoint;",
            "DROP TABLE IF EXISTS ai_history_file_state;",
            "DROP TABLE IF EXISTS ai_history_project_index_state;",
        ]

        for statement in resetStatements {
            guard sqlite3_exec(db, statement, nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to reset normalized AI history tables"])
            }
        }

        for statement in normalizedSchemaStatements() where !statement.contains("CREATE TABLE IF NOT EXISTS ai_history_meta") {
            guard sqlite3_exec(db, statement, nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 4, userInfo: [NSLocalizedDescriptionKey: "Failed to rebuild normalized AI history tables"])
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

    func hasNormalizedExternalSummary(
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

    func loadNormalizedExternalSummary(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String,
        modifiedAt: Double
    ) -> AIExternalFileSummary? {
        guard let usageBuckets = loadNormalizedUsageBuckets(
            db: db,
            source: source,
            filePath: filePath,
            projectPath: projectPath
        ) else {
            return nil
        }
        return aggregator.externalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: modifiedAt,
            projectPath: projectPath,
            usageBuckets: usageBuckets
        )
    }

    func replaceNormalizedExternalSummary(
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
                sql: "DELETE FROM ai_history_file_session_link WHERE source = ? AND file_path = ? AND project_path = ?;",
                bindings: [summary.source, summary.filePath, summary.projectPath]
            )
            try execute(
                db,
                sql: "DELETE FROM ai_history_file_usage_bucket WHERE source = ? AND file_path = ? AND project_path = ?;",
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

            for session in buildSessionLinks(from: summary.usageBuckets) {
                try execute(
                    db,
                    sql: """
                        INSERT INTO ai_history_file_session_link (
                            source, file_path, project_path, session_key, external_session_id,
                            project_id, project_name, session_title, first_seen_at, last_seen_at,
                            last_model, active_duration_seconds
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
                    """,
                    bindings: [
                        summary.source,
                        summary.filePath,
                        summary.projectPath,
                        session.sessionKey,
                        session.externalSessionID as Any,
                        session.projectID.uuidString,
                        session.projectName,
                        session.sessionTitle,
                        session.firstSeenAt.timeIntervalSince1970,
                        session.lastSeenAt.timeIntervalSince1970,
                        session.lastModel as Any,
                        session.activeDurationSeconds,
                    ]
                )
            }

            for bucket in summary.usageBuckets {
                try execute(
                    db,
                    sql: """
                        INSERT INTO ai_history_file_usage_bucket (
                            source, file_path, project_path, session_key, model, bucket_start, bucket_end,
                            input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
                    """,
                    bindings: [
                        summary.source,
                        summary.filePath,
                        summary.projectPath,
                        bucket.sessionKey,
                        bucket.model ?? "",
                        bucket.bucketStart.timeIntervalSince1970,
                        bucket.bucketEnd.timeIntervalSince1970,
                        bucket.inputTokens,
                        bucket.outputTokens,
                        bucket.totalTokens,
                        bucket.cachedInputTokens,
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

    private func loadNormalizedUsageBuckets(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String
    ) -> [AIUsageBucket]? {
        guard let sessionLinks = loadNormalizedSessionLinks(
            db: db,
            source: source,
            filePath: filePath,
            projectPath: projectPath
        ) else {
            return nil
        }

        let sql = """
        SELECT session_key, model, bucket_start, bucket_end, input_tokens, output_tokens,
               total_tokens, cached_input_tokens, request_count
        FROM ai_history_file_usage_bucket
        WHERE source = ? AND file_path = ? AND project_path = ?
        ORDER BY bucket_start ASC, session_key ASC, model ASC;
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

        var rows: [NormalizedUsageBucketRow] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawSessionKey = sqlite3_column_text(statement, 0) else {
                continue
            }
            rows.append(
                NormalizedUsageBucketRow(
                    sessionKey: String(cString: rawSessionKey),
                    model: sqlite3_column_text(statement, 1).map { String(cString: $0) },
                    bucketStart: Date(timeIntervalSince1970: sqlite3_column_double(statement, 2)),
                    bucketEnd: Date(timeIntervalSince1970: sqlite3_column_double(statement, 3)),
                    inputTokens: Int(sqlite3_column_int64(statement, 4)),
                    outputTokens: Int(sqlite3_column_int64(statement, 5)),
                    totalTokens: Int(sqlite3_column_int64(statement, 6)),
                    cachedInputTokens: Int(sqlite3_column_int64(statement, 7)),
                    requestCount: Int(sqlite3_column_int64(statement, 8))
                )
            )
        }
        let lastBucketStartBySession = rows.reduce(into: [String: Date]()) { partial, row in
            partial[row.sessionKey] = max(partial[row.sessionKey] ?? row.bucketStart, row.bucketStart)
        }

        return rows.compactMap { row in
            guard let session = sessionLinks[row.sessionKey] else {
                return nil
            }
            return AIUsageBucket(
                source: source,
                sessionKey: row.sessionKey,
                externalSessionID: session.externalSessionID,
                sessionTitle: session.sessionTitle,
                model: normalizedNonEmptyString(row.model),
                projectID: session.projectID,
                projectName: session.projectName,
                bucketStart: row.bucketStart,
                bucketEnd: row.bucketEnd,
                inputTokens: row.inputTokens,
                outputTokens: row.outputTokens,
                totalTokens: row.totalTokens,
                cachedInputTokens: row.cachedInputTokens,
                requestCount: row.requestCount,
                activeDurationSeconds: lastBucketStartBySession[row.sessionKey] == row.bucketStart ? session.activeDurationSeconds : 0,
                firstSeenAt: session.firstSeenAt,
                lastSeenAt: session.lastSeenAt
            )
        }
    }

    private func loadNormalizedSessionLinks(
        db: OpaquePointer,
        source: String,
        filePath: String,
        projectPath: String
    ) -> [String: NormalizedSessionLinkRow]? {
        let sql = """
        SELECT session_key, external_session_id, project_id, project_name, session_title,
               first_seen_at, last_seen_at, last_model, active_duration_seconds
        FROM ai_history_file_session_link
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

        var items: [String: NormalizedSessionLinkRow] = [:]
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawSessionKey = sqlite3_column_text(statement, 0),
                  let rawProjectID = sqlite3_column_text(statement, 2),
                  let projectID = UUID(uuidString: String(cString: rawProjectID)),
                  let rawProjectName = sqlite3_column_text(statement, 3),
                  let rawSessionTitle = sqlite3_column_text(statement, 4) else {
                continue
            }
            let sessionKey = String(cString: rawSessionKey)
            items[sessionKey] = NormalizedSessionLinkRow(
                sessionKey: sessionKey,
                externalSessionID: sqlite3_column_text(statement, 1).map { String(cString: $0) },
                projectID: projectID,
                projectName: String(cString: rawProjectName),
                sessionTitle: String(cString: rawSessionTitle),
                firstSeenAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 5)),
                lastSeenAt: Date(timeIntervalSince1970: sqlite3_column_double(statement, 6)),
                lastModel: sqlite3_column_text(statement, 7).map { String(cString: $0) },
                activeDurationSeconds: Int(sqlite3_column_int64(statement, 8))
            )
        }
        return items
    }

    private func buildSessionLinks(from usageBuckets: [AIUsageBucket]) -> [NormalizedSessionLinkRow] {
        var map: [String: NormalizedSessionLinkRow] = [:]

        for bucket in usageBuckets {
            if var current = map[bucket.sessionKey] {
                let previousLastSeenAt = current.lastSeenAt
                current.externalSessionID = current.externalSessionID ?? bucket.externalSessionID
                current.sessionTitle = normalizedNonEmptyString(bucket.sessionTitle) ?? current.sessionTitle
                current.firstSeenAt = min(current.firstSeenAt, bucket.firstSeenAt)
                current.lastSeenAt = max(current.lastSeenAt, bucket.lastSeenAt)
                current.activeDurationSeconds += bucket.activeDurationSeconds
                if bucket.lastSeenAt >= previousLastSeenAt,
                   let model = normalizedNonEmptyString(bucket.model) {
                    current.lastModel = model
                } else if current.lastModel == nil {
                    current.lastModel = normalizedNonEmptyString(bucket.model)
                }
                map[bucket.sessionKey] = current
            } else {
                map[bucket.sessionKey] = NormalizedSessionLinkRow(
                    sessionKey: bucket.sessionKey,
                    externalSessionID: bucket.externalSessionID,
                    projectID: bucket.projectID,
                    projectName: bucket.projectName,
                    sessionTitle: normalizedNonEmptyString(bucket.sessionTitle) ?? bucket.projectName,
                    firstSeenAt: bucket.firstSeenAt,
                    lastSeenAt: bucket.lastSeenAt,
                    lastModel: normalizedNonEmptyString(bucket.model),
                    activeDurationSeconds: bucket.activeDurationSeconds
                )
            }
        }

        return map.values.sorted { lhs, rhs in
            if lhs.lastSeenAt != rhs.lastSeenAt {
                return lhs.lastSeenAt > rhs.lastSeenAt
            }
            return lhs.sessionKey < rhs.sessionKey
        }
    }

    func decodeCheckpointPayload(_ payloadJSON: String?) -> AIExternalFileCheckpointPayload? {
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

    func execute(_ db: OpaquePointer, sql: String, bindings: [Any]) throws {
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

    private func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }
}

private struct NormalizedSessionLinkRow {
    var sessionKey: String
    var externalSessionID: String?
    var projectID: UUID
    var projectName: String
    var sessionTitle: String
    var firstSeenAt: Date
    var lastSeenAt: Date
    var lastModel: String?
    var activeDurationSeconds: Int
}

private struct NormalizedUsageBucketRow {
    var sessionKey: String
    var model: String?
    var bucketStart: Date
    var bucketEnd: Date
    var inputTokens: Int
    var outputTokens: Int
    var totalTokens: Int
    var cachedInputTokens: Int
    var requestCount: Int
}

let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
