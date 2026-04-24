import Foundation
import SQLite3

extension AIProjectHistoryService {
    func loadOpenCodeFileSummaries(
        project: Project,
        indexingProfile: AIProjectHistoryIndexingProfile = .foreground
    ) async -> [AIExternalFileSummary] {
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL(homeURL: runtimeHomeURL)
        if FileManager.default.fileExists(atPath: databaseURL.path) {
            logger.log(
                "history-refresh",
                "source=opencode locator=sqlite project=\(project.name) totalFiles=1 dbPath=\(databaseURL.path)"
            )
            return await loadFileSummaries(
                source: "opencode",
                fileURLs: [databaseURL],
                project: project,
                indexingProfile: indexingProfile,
                parser: parseOpenCodeDatabase
            )
        }

        let legacyMessageFiles = AIRuntimeSourceLocator.opencodeLegacyMessageFileURLs(homeURL: runtimeHomeURL)
        logger.log(
            "history-refresh",
            "source=opencode locator=legacy-json project=\(project.name) totalFiles=\(legacyMessageFiles.count)"
        )
        return await loadFileSummaries(
            source: "opencode",
            fileURLs: legacyMessageFiles,
            project: project,
            indexingProfile: indexingProfile,
            parser: parseOpenCodeLegacyMessageFile
        )
    }

    func parseOpenCodeDatabase(fileURL: URL, project: Project) -> AIHistoryParseResult {
        var db: OpaquePointer?
        guard sqlite3_open(fileURL.path, &db) == SQLITE_OK,
              let db else {
            if db != nil {
                sqlite3_close(db)
            }
            return .empty
        }
        defer { sqlite3_close(db) }

        let sql = """
        SELECT s.id,
               s.title,
               json_extract(m.data, '$.role') AS role,
               json_extract(m.data, '$.time.created') AS created_at,
               json_extract(m.data, '$.modelID') AS model_id,
               json_extract(m.data, '$.path.root') AS root_path,
               m.data
        FROM session s
        JOIN message m ON m.session_id = s.id
        WHERE s.time_archived IS NULL
        ORDER BY m.time_created ASC;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return .empty
        }
        defer { sqlite3_finalize(statement) }

        var result = AIHistoryParseResult.empty

        while sqlite3_step(statement) == SQLITE_ROW {
            guard !Task.isCancelled else {
                return .empty
            }
            guard let rawSessionID = sqlite3_column_text(statement, 0),
                  let rawCreatedAt = sqlite3_column_text(statement, 3),
                  let timestamp = parseOpenCodeTimestamp(String(cString: rawCreatedAt)),
                  let rawPayload = sqlite3_column_text(statement, 6),
                  let payloadData = String(cString: rawPayload).data(using: .utf8),
                  let payload = try? JSONSerialization.jsonObject(with: payloadData) as? [String: Any] else {
                continue
            }

            let rootPath = sqlite3_column_text(statement, 5).map { String(cString: $0) }
            guard pathsEquivalent(rootPath, project.path) else {
                continue
            }

            let sessionID = String(cString: rawSessionID)
            let key = AIHistorySessionKey(source: "opencode", sessionID: sessionID)
            let roleValue = sqlite3_column_text(statement, 2).map { String(cString: $0) } ?? "assistant"
            let role: AIHistorySessionRole = roleValue == "user" ? .user : .assistant
            result.events.append(
                AIHistorySessionEvent(
                    key: key,
                    projectName: project.name,
                    timestamp: timestamp,
                    role: role
                )
            )

            let model = normalizedNonEmptyString(sqlite3_column_text(statement, 4).map { String(cString: $0) })
                ?? normalizedNonEmptyString(payload["modelID"] as? String)
                ?? "unknown"
            let tokens = payload["tokens"] as? [String: Any] ?? [:]
            let cache = tokens["cache"] as? [String: Any] ?? [:]
            let input = numberValue(tokens["input"])
            let output = numberValue(tokens["output"])
            let cached = numberValue(cache["read"])
            let reasoning = numberValue(tokens["reasoning"])
            let total = max(numberValue(tokens["total"]), input + output + cached + reasoning)
            if total > 0 {
                result.entries.append(
                    AIHistoryUsageEntry(
                        key: key,
                        projectName: project.name,
                        timestamp: timestamp,
                        model: model,
                        inputTokens: input,
                        outputTokens: output,
                        cachedInputTokens: cached,
                        reasoningOutputTokens: reasoning
                    )
                )
            }

            let title = normalizedNonEmptyString(sqlite3_column_text(statement, 1).map { String(cString: $0) }) ?? project.name
            if result.metadataByKey[key] == nil {
                result.metadataByKey[key] = AIHistorySessionMetadata(
                    key: key,
                    externalSessionID: sessionID,
                    sessionTitle: title,
                    model: model
                )
            }
        }

        return result
    }

    func parseOpenCodeLegacyMessageFile(fileURL: URL, project: Project) -> AIHistoryParseResult {
        var result = AIHistoryParseResult.empty
        guard let data = try? Data(contentsOf: fileURL),
              let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let rootPath = normalizedNonEmptyString((payload["path"] as? [String: Any])?["root"] as? String),
              pathsEquivalent(rootPath, project.path),
              let createdValue = normalizedNonEmptyString((payload["time"] as? [String: Any])?["created"] as? String),
              let timestamp = parseOpenCodeTimestamp(createdValue) else {
            return .empty
        }

        let sessionID = fileURL.deletingLastPathComponent().lastPathComponent
        let key = AIHistorySessionKey(source: "opencode", sessionID: sessionID)
        let role: AIHistorySessionRole = (payload["role"] as? String) == "user" ? .user : .assistant
        result.events.append(
            AIHistorySessionEvent(
                key: key,
                projectName: project.name,
                timestamp: timestamp,
                role: role
            )
        )

        let model = normalizedNonEmptyString(payload["modelID"] as? String) ?? "unknown"
        let tokens = payload["tokens"] as? [String: Any] ?? [:]
        let cache = tokens["cache"] as? [String: Any] ?? [:]
        let input = numberValue(tokens["input"])
        let output = numberValue(tokens["output"])
        let cached = numberValue(cache["read"])
        let reasoning = numberValue(tokens["reasoning"])
        let total = max(numberValue(tokens["total"]), input + output + cached + reasoning)
        if total > 0 {
            result.entries.append(
                AIHistoryUsageEntry(
                    key: key,
                    projectName: project.name,
                    timestamp: timestamp,
                    model: model,
                    inputTokens: input,
                    outputTokens: output,
                    cachedInputTokens: cached,
                    reasoningOutputTokens: reasoning
                )
            )
        }

        result.metadataByKey[key] = AIHistorySessionMetadata(
            key: key,
            externalSessionID: sessionID,
            sessionTitle: project.name,
            model: model
        )
        return result
    }

    private func parseOpenCodeTimestamp(_ value: String) -> Date? {
        if let milliseconds = Double(value) {
            return Date(timeIntervalSince1970: milliseconds / 1000)
        }
        return parseCodexISO8601Date(value)
    }
}

let AIHistorySQLiteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
