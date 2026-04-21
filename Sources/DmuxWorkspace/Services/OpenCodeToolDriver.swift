import Foundation
import SQLite3

struct OpenCodeToolDriver: AIToolDriver {
    let id = "opencode"
    let aliases: Set<String> = ["opencode"]
    let isRealtimeTool = true

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: true, canRemove: true)
    }

    func runtimeSnapshot(
        for session: AISessionStore.TerminalSessionState
    ) async -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedNonEmptyString(session.projectPath) else {
            return nil
        }
        guard let externalSessionID = normalizedNonEmptyString(session.aiSessionID) else {
            return nil
        }
        return resolvedExternalSessionSnapshot(
            projectPath: projectPath,
            externalSessionID: externalSessionID
        )
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            return nil
        }
        return "opencode --session \(shellQuoted(sessionID))"
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            throw AIToolSessionControlError.missingSessionID
        }
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            let sql = "UPDATE session SET title = ? WHERE id = ?;"
            try executeSQLite(
                db: db,
                sql: sql,
                bindings: [
                    .text(title),
                    .text(sessionID),
                ]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    func removeSession(_ session: AISessionSummary) throws {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            throw AIToolSessionControlError.missingSessionID
        }
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            try executeSQLite(
                db: db,
                sql: "PRAGMA foreign_keys = ON;",
                bindings: []
            )
            try executeSQLite(
                db: db,
                sql: "DELETE FROM session WHERE id = ?;",
                bindings: [.text(sessionID)]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    private func resolvedExternalSessionSnapshot(
        projectPath: String,
        externalSessionID: String
    ) -> AIRuntimeContextSnapshot? {
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        guard FileManager.default.fileExists(atPath: databaseURL.path) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &db) == SQLITE_OK,
              let db else {
            if db != nil {
                sqlite3_close(db)
            }
            return nil
        }
        defer { sqlite3_close(db) }

        return try? fetchOpenCodeSessionSnapshot(
            db: db,
            projectPath: projectPath,
            externalSessionID: externalSessionID
        )
    }
}

private func fetchOpenCodeSessionSnapshot(
    db: OpaquePointer,
    projectPath: String,
    externalSessionID: String
) throws -> AIRuntimeContextSnapshot? {
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
      AND s.time_archived IS NULL
    ORDER BY m.time_created DESC;
    """

    var statement: OpaquePointer?
    guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
          let statement else {
        throw AIToolSessionControlError.storageFailure(String(cString: sqlite3_errmsg(db)))
    }
    defer { sqlite3_finalize(statement) }

    sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_SESSION)
    sqlite3_bind_text(statement, 2, externalSessionID, -1, SQLITE_TRANSIENT_SESSION)

    var latestModel: String?
    var inputTokens = 0
    var outputTokens = 0
    var totalTokens = 0
    var updatedAt = 0.0
    var hadRow = false

    while sqlite3_step(statement) == SQLITE_ROW {
        hadRow = true
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

    guard hadRow else {
        return nil
    }

    return AIRuntimeContextSnapshot(
        tool: "opencode",
        externalSessionID: externalSessionID,
        model: latestModel,
        inputTokens: inputTokens,
        outputTokens: outputTokens,
        totalTokens: totalTokens,
        updatedAt: updatedAt,
        responseState: totalTokens > 0 ? .idle : nil,
        sessionOrigin: totalTokens > 0 ? .restored : .fresh,
        source: .probe
    )
}
