import Foundation
import SQLite3

struct OpenCodeToolDriver: AIToolDriver {
    let id = "opencode"
    let aliases: Set<String> = ["opencode"]
    let runtimeRefreshInterval: TimeInterval = 0.75
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let freezesDisplayTokensWhileResponding = true
    let allowsRuntimeExternalSessionSwitch = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        var descriptors: [AIToolRuntimeSourceDescriptor] = []
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL()
        if FileManager.default.fileExists(atPath: databaseURL.path) {
            descriptors.append(AIToolRuntimeSourceDescriptor(path: databaseURL.path, watchKind: .file))
        }

        let walURL = URL(fileURLWithPath: databaseURL.path + "-wal")
        if FileManager.default.fileExists(atPath: walURL.path) {
            descriptors.append(AIToolRuntimeSourceDescriptor(path: walURL.path, watchKind: .file))
        }
        return descriptors
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "opencode-runtime",
              let envelope = try? JSONDecoder().decode(AIToolUsageEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.sessionId),
              let projectID = UUID(uuidString: envelope.projectId) else {
            return nil
        }

        let dedupeKey = "opencode|\(sessionID.uuidString)|\(envelope.responseState?.rawValue ?? "nil")|\(Int(envelope.updatedAt))|\(envelope.totalTokens ?? -1)"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.0) else {
            AppDebugLog.shared.log(
                "opencode-driver",
                "drop duplicate kind=\(kind) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "opencode-driver",
                "ignore stale kind=\(kind) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "opencode-driver",
                "ignore stale kind=\(kind) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let externalSessionID = normalizedSessionID(envelope.externalSessionID)
            ?? existingSnapshot?.externalSessionID
            ?? normalizedSessionID(liveEnvelope?.externalSessionID)
        let projectPath = normalizedSessionID(envelope.projectPath)
            ?? normalizedSessionID(liveEnvelope?.projectPath)
        let switchedExternalSession =
            externalSessionID != nil
            && externalSessionID != existingSnapshot?.externalSessionID
        let resolvedHistoricalSnapshot = resolvedExternalSessionSnapshot(
            projectPath: projectPath,
            externalSessionID: externalSessionID,
            shouldResolve: switchedExternalSession
                || ((envelope.totalTokens ?? 0) == 0 && envelope.responseState == .responding)
        )
        let model = normalizedSessionID(envelope.model)
            ?? resolvedHistoricalSnapshot?.model
            ?? existingSnapshot?.model
            ?? normalizedSessionID(liveEnvelope?.model)
        let canReuseExistingTotals = shouldReuseExistingTotals(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot
        )
        let inheritedInputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
            : 0
        let inheritedOutputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
            : 0
        let inheritedTotalTokens = canReuseExistingTotals
            ? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)
            : 0
        let updatedAt = max(
            envelope.updatedAt,
            resolvedHistoricalSnapshot?.updatedAt ?? 0,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        if let existingSnapshot,
           existingSnapshot.externalSessionID == externalSessionID,
           updatedAt < existingSnapshot.updatedAt,
           envelope.responseState != .responding {
            AppDebugLog.shared.log(
                "opencode-driver",
                "drop stale kind=\(kind) session=\(sessionID.uuidString) updatedAt=\(updatedAt) existingAt=\(existingSnapshot.updatedAt)"
            )
            return nil
        }

        let runtimeSnapshot = AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: externalSessionID,
            model: model,
            inputTokens: max(envelope.inputTokens ?? 0, resolvedHistoricalSnapshot?.inputTokens ?? 0, inheritedInputTokens),
            outputTokens: max(envelope.outputTokens ?? 0, resolvedHistoricalSnapshot?.outputTokens ?? 0, inheritedOutputTokens),
            totalTokens: max(envelope.totalTokens ?? 0, resolvedHistoricalSnapshot?.totalTokens ?? 0, inheritedTotalTokens),
            updatedAt: updatedAt,
            responseState: envelope.responseState,
            sessionOrigin: resolvedHistoricalSnapshot?.sessionOrigin ?? .unknown,
            source: .socket
        )

        let responsePayloads: [AIResponseStatePayload]
        if let responseState = envelope.responseState {
            responsePayloads = [
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: envelope.sessionInstanceId,
                    invocationId: envelope.invocationId,
                    projectId: projectID.uuidString,
                    projectPath: envelope.projectPath,
                    tool: id,
                    responseState: responseState,
                    updatedAt: updatedAt,
                    source: .socket
                ),
            ]
        } else {
            responsePayloads = []
        }

        AppDebugLog.shared.log(
            "opencode-driver",
            "socket kind=\(kind) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") model=\(model ?? "nil") response=\(envelope.responseState?.rawValue ?? "nil") total=\(runtimeSnapshot.totalTokens) reuseTotals=\(canReuseExistingTotals) origin=\(runtimeSnapshot.sessionOrigin.rawValue)"
        )

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
        )
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: true, canRemove: true)
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

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func normalizedSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func shouldReuseExistingTotals(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?
    ) -> Bool {
        guard let externalSessionID, !externalSessionID.isEmpty else {
            return false
        }
        if normalizedSessionID(liveEnvelope?.externalSessionID) == externalSessionID {
            return true
        }
        if existingSnapshot?.externalSessionID == externalSessionID {
            return true
        }
        return false
    }

    private func resolvedExternalSessionSnapshot(
        projectPath: String?,
        externalSessionID: String?,
        shouldResolve: Bool
    ) -> AIRuntimeContextSnapshot? {
        guard shouldResolve,
              let projectPath,
              let externalSessionID else {
            return nil
        }

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
