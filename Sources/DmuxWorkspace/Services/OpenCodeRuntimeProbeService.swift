import Foundation
import SQLite3

actor OpenCodeRuntimeProbeService {
    private var sessionIDByRuntimeSessionID: [String: String] = [:]
    private var originByRuntimeSessionID: [String: AIRuntimeSessionOrigin] = [:]
    private let globalEventService = OpenCodeGlobalEventService.shared

    func reset(runtimeSessionID: String) {
        sessionIDByRuntimeSessionID[runtimeSessionID] = nil
        originByRuntimeSessionID[runtimeSessionID] = nil
    }

    func snapshot(
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) async -> AIRuntimeContextSnapshot? {
        let dbURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        guard FileManager.default.fileExists(atPath: dbURL.path) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(dbURL.path, &db) == SQLITE_OK, let db else {
            return nil
        }
        defer { sqlite3_close(db) }

        guard let resolvedSession = resolveSession(
            db: db,
            runtimeSessionID: runtimeSessionID,
            projectPath: projectPath,
            startedAt: startedAt,
            knownExternalSessionID: knownExternalSessionID
        ) else {
            return nil
        }

        let sql = """
        SELECT json_extract(m.data, '$.modelID') AS model,
               COALESCE(json_extract(m.data, '$.tokens.input'), 0) AS input_tokens,
               COALESCE(json_extract(m.data, '$.tokens.output'), 0) AS output_tokens,
               COALESCE(json_extract(m.data, '$.tokens.cache.read'), 0) AS cache_read_tokens,
               COALESCE(json_extract(m.data, '$.tokens.cache.write'), 0) AS cache_write_tokens,
               COALESCE(json_extract(m.data, '$.tokens.total'), 0) AS total_tokens,
               COALESCE(json_extract(m.data, '$.time.completed'), json_extract(m.data, '$.time.created'), 0) AS completed_at,
               s.time_updated AS session_updated_at
        FROM session s
        LEFT JOIN message m ON m.session_id = s.id
        WHERE s.directory = ?
          AND s.id = ?
        ORDER BY m.time_created DESC;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_OPENCODE_RUNTIME)
        sqlite3_bind_text(statement, 2, resolvedSession.id, -1, SQLITE_TRANSIENT_OPENCODE_RUNTIME)

        var latestModel: String?
        var inputTokens = 0
        var outputTokens = 0
        var totalTokens = 0
        var updatedAt = resolvedSession.updatedAt

        while sqlite3_step(statement) == SQLITE_ROW {
            if latestModel == nil, let rawModel = sqlite3_column_text(statement, 0) {
                let model = String(cString: rawModel)
                if !model.isEmpty {
                    latestModel = model
                }
            }
            let input = Int(sqlite3_column_int64(statement, 1))
            let output = Int(sqlite3_column_int64(statement, 2))
            let cacheRead = Int(sqlite3_column_int64(statement, 3))
            let cacheWrite = Int(sqlite3_column_int64(statement, 4))
            let explicitTotal = Int(sqlite3_column_int64(statement, 5))
            inputTokens += input + cacheRead + cacheWrite
            outputTokens += output
            totalTokens += max(explicitTotal, input + output + cacheRead + cacheWrite)
            updatedAt = max(updatedAt, sqlite3_column_double(statement, 6) / 1000)
            updatedAt = max(updatedAt, sqlite3_column_double(statement, 7) / 1000)
        }

        guard updatedAt > 0 else {
            return nil
        }

        let responseState = await globalEventService.sessionStatuses(directory: projectPath)?[resolvedSession.id]

        return AIRuntimeContextSnapshot(
            tool: "opencode",
            externalSessionID: resolvedSession.id,
            model: latestModel,
            inputTokens: inputTokens,
            outputTokens: outputTokens,
            totalTokens: totalTokens,
            updatedAt: updatedAt,
            responseState: responseState,
            sessionOrigin: resolvedSession.origin
        )
    }

    private struct ResolvedSession {
        var id: String
        var updatedAt: Double
        var origin: AIRuntimeSessionOrigin
    }

    private func resolveSession(
        db: OpaquePointer,
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) -> ResolvedSession? {
        if let knownExternalSessionID = normalizedSessionID(knownExternalSessionID),
           let resolved = sessionByID(db: db, projectPath: projectPath, sessionID: knownExternalSessionID) {
            sessionIDByRuntimeSessionID[runtimeSessionID] = resolved.id
            originByRuntimeSessionID[runtimeSessionID] = .restored
            return ResolvedSession(id: resolved.id, updatedAt: resolved.updatedAt, origin: .restored)
        }

        if let existingSessionID = sessionIDByRuntimeSessionID[runtimeSessionID],
           let existing = sessionByID(db: db, projectPath: projectPath, sessionID: existingSessionID) {
            let origin = originByRuntimeSessionID[runtimeSessionID] ?? .unknown
            return ResolvedSession(id: existing.id, updatedAt: existing.updatedAt, origin: origin)
        }

        guard let fresh = latestLaunchSession(db: db, projectPath: projectPath, startedAt: startedAt) else {
            return nil
        }
        sessionIDByRuntimeSessionID[runtimeSessionID] = fresh.id
        originByRuntimeSessionID[runtimeSessionID] = .fresh
        return ResolvedSession(id: fresh.id, updatedAt: fresh.updatedAt, origin: .fresh)
    }

    private func sessionByID(
        db: OpaquePointer,
        projectPath: String,
        sessionID: String
    ) -> (id: String, updatedAt: Double)? {
        let sql = """
        SELECT id, time_updated
        FROM session
        WHERE directory = ?
          AND id = ?
          AND time_archived IS NULL
        LIMIT 1;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_OPENCODE_RUNTIME)
        sqlite3_bind_text(statement, 2, sessionID, -1, SQLITE_TRANSIENT_OPENCODE_RUNTIME)

        guard sqlite3_step(statement) == SQLITE_ROW,
              let rawID = sqlite3_column_text(statement, 0) else {
            return nil
        }
        return (
            id: String(cString: rawID),
            updatedAt: sqlite3_column_double(statement, 1) / 1000
        )
    }

    private func latestLaunchSession(
        db: OpaquePointer,
        projectPath: String,
        startedAt: Double
    ) -> (id: String, updatedAt: Double)? {
        let sql = """
        SELECT id, time_updated
        FROM session
        WHERE directory = ?
          AND time_archived IS NULL
          AND time_created >= ?
        ORDER BY time_created DESC, time_updated DESC
        LIMIT 1;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_OPENCODE_RUNTIME)
        sqlite3_bind_double(statement, 2, max(0, startedAt - 2) * 1000)

        guard sqlite3_step(statement) == SQLITE_ROW,
              let rawID = sqlite3_column_text(statement, 0) else {
            return nil
        }
        return (
            id: String(cString: rawID),
            updatedAt: sqlite3_column_double(statement, 1) / 1000
        )
    }
    private func normalizedSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }
}

private let SQLITE_TRANSIENT_OPENCODE_RUNTIME = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
