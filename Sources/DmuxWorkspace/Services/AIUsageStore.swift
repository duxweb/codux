import Foundation
import SQLite3

struct AIUsageStore: Sendable {
    private func databaseURL() -> URL {
        let fileManager = FileManager.default
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let root = appSupport.appendingPathComponent("dmux", isDirectory: true)
        try? fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        return root.appendingPathComponent("ai-usage.sqlite3")
    }

    private func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        var db: OpaquePointer?
        let path = databaseURL().path
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
            CREATE TABLE IF NOT EXISTS ai_external_file_cache (
                source TEXT NOT NULL,
                file_path TEXT PRIMARY KEY,
                file_modified_at REAL NOT NULL,
                project_path TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_indexed_project_snapshot (
                project_id TEXT PRIMARY KEY,
                indexed_at REAL NOT NULL,
                payload_json TEXT NOT NULL
            );
            """,
            """
            CREATE TABLE IF NOT EXISTS ai_managed_realtime_session (
                record_id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                project_path TEXT NOT NULL,
                tool TEXT NOT NULL,
                updated_at REAL NOT NULL,
                payload_json TEXT NOT NULL
            );
            """,
            "CREATE INDEX IF NOT EXISTS idx_ai_external_project_path ON ai_external_file_cache(project_path);",
            "CREATE INDEX IF NOT EXISTS idx_ai_project_snapshot_indexed_at ON ai_indexed_project_snapshot(indexed_at DESC);",
            "CREATE INDEX IF NOT EXISTS idx_ai_managed_realtime_project_path ON ai_managed_realtime_session(project_path, tool, updated_at DESC);",
            "CREATE INDEX IF NOT EXISTS idx_ai_managed_realtime_project_id ON ai_managed_realtime_session(project_id, updated_at DESC);"
        ]

        for statement in statements {
            guard sqlite3_exec(db, statement, nil, nil, nil) == SQLITE_OK else {
                throw NSError(domain: "AIUsageStore", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to initialize AI usage database"])
            }
        }
    }

    func indexedProjectSnapshot(projectID: UUID) -> AIIndexedProjectSnapshot? {
        try? withDatabase { db in
            let sql = "SELECT payload_json FROM ai_indexed_project_snapshot WHERE project_id = ? LIMIT 1;"
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, projectID.uuidString, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_ROW,
                  let raw = sqlite3_column_text(statement, 0) else {
                return nil
            }

            let data = Data(String(cString: raw).utf8)
            return try? JSONDecoder().decode(AIIndexedProjectSnapshot.self, from: data)
        }
    }

    func saveIndexedProjectSnapshot(_ snapshot: AIIndexedProjectSnapshot) {
        guard let data = try? JSONEncoder().encode(snapshot),
              let payload = String(data: data, encoding: .utf8) else {
            return
        }

        try? withDatabase { db in
            try execute(db, sql: """
                INSERT INTO ai_indexed_project_snapshot (project_id, indexed_at, payload_json)
                VALUES (?, ?, ?)
                ON CONFLICT(project_id) DO UPDATE SET
                    indexed_at=excluded.indexed_at,
                    payload_json=excluded.payload_json;
            """, bindings: [snapshot.projectID.uuidString, snapshot.indexedAt.timeIntervalSince1970, payload])
        }
    }

    func deleteIndexedProjectSnapshot(projectID: UUID) {
        try? withDatabase { db in
            try execute(db, sql: "DELETE FROM ai_indexed_project_snapshot WHERE project_id = ?;", bindings: [projectID.uuidString])
        }
    }

    func cachedExternalSummary(source: String, filePath: String, modifiedAt: Double) -> AIExternalFileSummary? {
        try? withDatabase { db in
            let sql = "SELECT payload_json FROM ai_external_file_cache WHERE source = ? AND file_path = ? AND file_modified_at = ? LIMIT 1;"
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 2, filePath, -1, SQLITE_TRANSIENT)
            sqlite3_bind_double(statement, 3, modifiedAt)

            guard sqlite3_step(statement) == SQLITE_ROW,
                  let raw = sqlite3_column_text(statement, 0) else {
                return nil
            }

            let data = Data(String(cString: raw).utf8)
            return try? JSONDecoder().decode(AIExternalFileSummary.self, from: data)
        }
    }

    func latestCachedExternalSummary(source: String, filePath: String) -> AIExternalFileSummary? {
        try? withDatabase { db in
            let sql = "SELECT payload_json FROM ai_external_file_cache WHERE source = ? AND file_path = ? ORDER BY file_modified_at DESC LIMIT 1;"
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 2, filePath, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_ROW,
                  let raw = sqlite3_column_text(statement, 0) else {
                return nil
            }

            let data = Data(String(cString: raw).utf8)
            return try? JSONDecoder().decode(AIExternalFileSummary.self, from: data)
        }
    }

    func saveExternalSummary(_ summary: AIExternalFileSummary) {
        guard let data = try? JSONEncoder().encode(summary),
              let payload = String(data: data, encoding: .utf8) else {
            return
        }

        try? withDatabase { db in
            try execute(db, sql: """
                INSERT INTO ai_external_file_cache (source, file_path, file_modified_at, project_path, payload_json)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(file_path) DO UPDATE SET
                    source=excluded.source,
                    file_modified_at=excluded.file_modified_at,
                    project_path=excluded.project_path,
                    payload_json=excluded.payload_json;
            """, bindings: [summary.source, summary.filePath, summary.fileModifiedAt, summary.projectPath, payload])
        }
    }

    func cachedExternalSummaries(source: String, projectPath: String) -> [AIExternalFileSummary] {
        (try? withDatabase { db in
            let sql = "SELECT payload_json FROM ai_external_file_cache WHERE source = ? AND project_path = ?;"
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return []
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, source, -1, SQLITE_TRANSIENT)
            sqlite3_bind_text(statement, 2, projectPath, -1, SQLITE_TRANSIENT)

            var items: [AIExternalFileSummary] = []
            while sqlite3_step(statement) == SQLITE_ROW {
                guard let raw = sqlite3_column_text(statement, 0) else { continue }
                let data = Data(String(cString: raw).utf8)
                if let item = try? JSONDecoder().decode(AIExternalFileSummary.self, from: data) {
                    items.append(item)
                }
            }
            return items
        }) ?? []
    }

    func deleteExternalSummaries(projectPath: String) {
        try? withDatabase { db in
            try execute(db, sql: "DELETE FROM ai_external_file_cache WHERE project_path = ?;", bindings: [projectPath])
        }
    }

    func managedRealtimeRecord(recordID: String) -> AIManagedRealtimeSessionRecord? {
        try? withDatabase { db in
            let sql = "SELECT payload_json FROM ai_managed_realtime_session WHERE record_id = ? LIMIT 1;"
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, recordID, -1, SQLITE_TRANSIENT)

            guard sqlite3_step(statement) == SQLITE_ROW,
                  let raw = sqlite3_column_text(statement, 0) else {
                return nil
            }

            let data = Data(String(cString: raw).utf8)
            return try? JSONDecoder().decode(AIManagedRealtimeSessionRecord.self, from: data)
        }
    }

    func saveManagedRealtimeRecord(_ record: AIManagedRealtimeSessionRecord) {
        guard let data = try? JSONEncoder().encode(record),
              let payload = String(data: data, encoding: .utf8) else {
            return
        }

        try? withDatabase { db in
            try execute(db, sql: """
                INSERT INTO ai_managed_realtime_session (record_id, project_id, project_path, tool, updated_at, payload_json)
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(record_id) DO UPDATE SET
                    project_id=excluded.project_id,
                    project_path=excluded.project_path,
                    tool=excluded.tool,
                    updated_at=excluded.updated_at,
                    payload_json=excluded.payload_json;
            """, bindings: [
                record.recordID,
                record.projectID.uuidString,
                record.projectPath,
                record.tool,
                record.updatedAt.timeIntervalSince1970,
                payload,
            ])
        }
    }

    func managedRealtimeRecords(projectPath: String, tools: Set<String>) -> [AIManagedRealtimeSessionRecord] {
        guard !tools.isEmpty else {
            return []
        }

        return (try? withDatabase { db in
            let placeholders = Array(repeating: "?", count: tools.count).joined(separator: ", ")
            let sql = """
            SELECT payload_json
            FROM ai_managed_realtime_session
            WHERE project_path = ?
              AND tool IN (\(placeholders))
            ORDER BY updated_at DESC;
            """
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return []
            }
            defer { sqlite3_finalize(statement) }

            sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT)
            for (index, tool) in tools.sorted().enumerated() {
                sqlite3_bind_text(statement, Int32(index + 2), tool, -1, SQLITE_TRANSIENT)
            }

            var items: [AIManagedRealtimeSessionRecord] = []
            while sqlite3_step(statement) == SQLITE_ROW {
                guard let raw = sqlite3_column_text(statement, 0) else {
                    continue
                }
                let data = Data(String(cString: raw).utf8)
                if let item = try? JSONDecoder().decode(AIManagedRealtimeSessionRecord.self, from: data) {
                    items.append(item)
                }
            }
            return items
        }) ?? []
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
