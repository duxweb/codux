import Foundation
import SQLite3

extension AIProjectHistoryService {
    func loadClaudeFileSummaries(project: Project) async -> [AIExternalFileSummary] {
        let files = AIRuntimeSourceLocator.claudeProjectLogURLs(projectPath: project.path)
        logger.log(
            "history-refresh",
            "source=claude locator=projects project=\(project.name) totalFiles=\(files.count)"
        )
        return loadIncrementalJSONLFileSummaries(
            source: "claude",
            fileURLs: files,
            project: project,
            fullParser: { fileURL, project in
                parseClaudeFile(fileURL: fileURL, project: project)
            },
            appendParser: { fileURL, project, checkpoint in
                parseClaudeFile(
                    fileURL: fileURL,
                    project: project,
                    startingAt: checkpoint.lastOffset,
                    seed: checkpoint.payload
                )
            }
        )
    }

    func parseClaudeFile(fileURL: URL, project: Project) -> JSONLParseSnapshot {
        parseClaudeFile(fileURL: fileURL, project: project, startingAt: 0, seed: nil)
    }

    func parseClaudeFile(
        fileURL: URL,
        project: Project,
        startingAt offset: UInt64,
        seed: AIExternalFileCheckpointPayload?
    ) -> JSONLParseSnapshot {
        var result = AIHistoryParseResult.empty
        var seenAssistantUUIDs = Set<String>()
        var lastProcessedOffset = offset
        var cwdConfirmed = offset > 0 || seed?.sessionKey != nil
        var cwdDenied = false
        var earlyLineCount = 0
        let stopOnInvalidJSON = offset > 0

        func processLine(_ lineData: Data, endOffset: UInt64) -> Bool {
            guard !cwdDenied else { return false }
            guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any] else {
                return stopOnInvalidJSON == false
            }

            if !cwdConfirmed, let cwd = row["cwd"] as? String {
                if cwd == project.path {
                    cwdConfirmed = true
                } else {
                    cwdDenied = true
                    return false
                }
            }

            guard cwdConfirmed else {
                lastProcessedOffset = endOffset
                return true
            }

            if let sessionID = row["sessionId"] as? String {
                let key = AIHistorySessionKey(source: "claude", sessionID: sessionID)

                if let role = claudeRole(from: row["type"] as? String),
                   let timestampString = row["timestamp"] as? String,
                   let timestamp = parseAIHistoryISO8601Date(timestampString) {
                    result.events.append(
                        AIHistorySessionEvent(
                            key: key,
                            projectName: project.name,
                            timestamp: timestamp,
                            role: role
                        )
                    )

                    if result.metadataByKey[key] == nil {
                        result.metadataByKey[key] = AIHistorySessionMetadata(
                            key: key,
                            externalSessionID: sessionID,
                            sessionTitle: claudeTitle(from: row) ?? project.name,
                            model: nil
                        )
                    } else if let title = claudeTitle(from: row) {
                        result.metadataByKey[key]?.sessionTitle = title
                    }

                    if row["type"] as? String == "assistant",
                       let message = row["message"] as? [String: Any],
                       let usage = message["usage"] as? [String: Any] {
                        if let uuid = row["uuid"] as? String, !uuid.isEmpty {
                            if seenAssistantUUIDs.contains(uuid) {
                                lastProcessedOffset = endOffset
                                return true
                            }
                            seenAssistantUUIDs.insert(uuid)
                        }

                        let inputTokens = numberValue(usage["input_tokens"])
                        let outputTokens = numberValue(usage["output_tokens"])
                        let cachedInputTokens = numberValue(usage["cache_read_input_tokens"])
                        let totalTokens = inputTokens + outputTokens + cachedInputTokens
                        if totalTokens > 0 {
                            let model = normalizedNonEmptyString(message["model"] as? String) ?? "unknown"
                            if var metadata = result.metadataByKey[key] {
                                metadata.model = metadata.model ?? model
                                result.metadataByKey[key] = metadata
                            }
                            result.entries.append(
                                AIHistoryUsageEntry(
                                    key: key,
                                    projectName: project.name,
                                    timestamp: timestamp,
                                    model: model,
                                    inputTokens: inputTokens,
                                    outputTokens: outputTokens,
                                    cachedInputTokens: cachedInputTokens,
                                    reasoningOutputTokens: 0
                                )
                            )
                        }
                    }
                }
            }

            lastProcessedOffset = endOffset
            return true
        }

        JSONLLineReader.forEachLine(in: fileURL, startingAt: offset) { lineData, endOffset in
            guard !Task.isCancelled, !cwdDenied else {
                return false
            }
            if processLine(lineData, endOffset: endOffset) == false {
                return false
            }
            if !cwdConfirmed {
                earlyLineCount += 1
                if earlyLineCount >= 10 {
                    return false
                }
            }
            return true
        }

        return JSONLParseSnapshot(
            result: cwdDenied ? .empty : result,
            lastProcessedOffset: lastProcessedOffset,
            modelTotalTokensByName: nil
        )
    }

    func loadCodexFileSummaries(project: Project) async -> [AIExternalFileSummary] {
        let databaseURL = AIRuntimeSourceLocator.codexDatabaseURL(homeURL: runtimeHomeURL)
        let databaseFiles = AIRuntimeSourceLocator.codexSessionFileURLsFromDatabase(
            projectPath: project.path,
            databaseURL: databaseURL
        )
        let files = databaseFiles.isEmpty
            ? AIRuntimeSourceLocator.codexSessionFileURLs(projectPath: project.path, homeURL: runtimeHomeURL)
            : databaseFiles
        logger.log(
            "history-refresh",
            "source=codex locator=\(databaseFiles.isEmpty ? "sessions-scan" : "state-db") project=\(project.name) totalFiles=\(files.count) dbExists=\(FileManager.default.fileExists(atPath: databaseURL.path))"
        )
        return loadIncrementalJSONLFileSummaries(
            source: "codex",
            fileURLs: files,
            project: project,
            fullParser: { fileURL, project in
                parseCodexFile(fileURL: fileURL, project: project)
            },
            appendParser: { fileURL, project, checkpoint in
                parseCodexFile(
                    fileURL: fileURL,
                    project: project,
                    startingAt: checkpoint.lastOffset,
                    seed: checkpoint.payload
                )
            }
        )
    }

    func parseCodexFile(fileURL: URL, project: Project) -> JSONLParseSnapshot {
        parseCodexFile(fileURL: fileURL, project: project, startingAt: 0, seed: nil)
    }

    func parseCodexFile(
        fileURL: URL,
        project: Project,
        startingAt offset: UInt64,
        seed: AIExternalFileCheckpointPayload?
    ) -> JSONLParseSnapshot {
        var result = AIHistoryParseResult.empty
        var matchedProject = seed?.sessionKey != nil
        var key = AIHistorySessionKey(source: "codex", sessionID: seed?.sessionKey ?? fileURL.path)
        var sessionTitle = normalizedNonEmptyString(seed?.sessionTitle)
        var model = normalizedNonEmptyString(seed?.lastModel)
        var totalByModel = seed?.modelTotalTokensByName ?? [:]
        var pendingEvents: [AIHistorySessionEvent] = []
        var pendingEntries: [AIHistoryUsageEntry] = []
        var lastProcessedOffset = offset
        let stopOnInvalidJSON = offset > 0

        JSONLLineReader.forEachLine(in: fileURL, startingAt: offset) { lineData, endOffset in
            guard !Task.isCancelled else {
                return false
            }
            guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any] else {
                return stopOnInvalidJSON == false
            }

            defer {
                lastProcessedOffset = endOffset
            }

            guard let timestampString = row["timestamp"] as? String,
                  let timestamp = parseCodexISO8601Date(timestampString) else {
                return true
            }

            let rowType = row["type"] as? String
            let payload = row["payload"] as? [String: Any] ?? [:]

            if rowType == "session_meta",
               let cwd = payload["cwd"] as? String,
               cwd == project.path {
                matchedProject = true
                if let sessionID = normalizedNonEmptyString(payload["id"] as? String) {
                    key = AIHistorySessionKey(source: "codex", sessionID: sessionID)
                }
                sessionTitle = normalizedNonEmptyString(payload["thread_name"] as? String)
                    ?? normalizedNonEmptyString(payload["title"] as? String)
                    ?? sessionTitle
            }

            if rowType == "turn_context",
               let cwd = payload["cwd"] as? String,
               cwd == project.path {
                matchedProject = true
                if let rawModel = normalizedNonEmptyString(payload["model"] as? String) {
                    model = rawModel
                }
            }

            guard matchedProject else {
                return true
            }

            pendingEvents.append(
                AIHistorySessionEvent(
                    key: key,
                    projectName: project.name,
                    timestamp: timestamp,
                    role: codexRole(for: rowType)
                )
            )

            if rowType == "response_item",
               sessionTitle == nil,
               let derivedTitle = codexResponseTitle(from: payload) {
                sessionTitle = derivedTitle
            }

            guard rowType == "event_msg",
                  payload["type"] as? String == "token_count" else {
                return true
            }

            let info = payload["info"] as? [String: Any] ?? [:]
            let resolvedModel = normalizedNonEmptyString(info["model"] as? String)
                ?? normalizedNonEmptyString(payload["model"] as? String)
                ?? model
                ?? "unknown"

            var usage = info["last_token_usage"] as? [String: Any]
            if usage == nil,
               let totalUsage = info["total_token_usage"] as? [String: Any] {
                let previous = totalByModel[resolvedModel] ?? 0
                let current = numberValue(totalUsage["total_tokens"])
                let delta = max(0, current - previous)
                totalByModel[resolvedModel] = max(previous, current)
                if delta > 0 {
                    usage = [
                        "input_tokens": numberValue(totalUsage["input_tokens"]),
                        "output_tokens": numberValue(totalUsage["output_tokens"]),
                        "cached_input_tokens": numberValue(totalUsage["cached_input_tokens"]),
                        "reasoning_output_tokens": numberValue(totalUsage["reasoning_output_tokens"]),
                        "total_tokens": delta,
                    ]
                }
            }

            guard let usage else {
                return true
            }

            let cachedInputTokens = numberValue(usage["cached_input_tokens"]) + numberValue(usage["cache_read_input_tokens"])
            let reasoningOutputTokens = numberValue(usage["reasoning_output_tokens"])
            let rawInputTokens = numberValue(usage["input_tokens"])
            let rawOutputTokens = numberValue(usage["output_tokens"])
            let inputTokens = max(0, rawInputTokens - cachedInputTokens)
            let outputTokens = max(0, rawOutputTokens - reasoningOutputTokens)
            let explicitTotal = numberValue(usage["total_tokens"])
            let totalTokens = max(explicitTotal, inputTokens + outputTokens + cachedInputTokens + reasoningOutputTokens)
            guard totalTokens > 0 else {
                return true
            }

            pendingEntries.append(
                AIHistoryUsageEntry(
                    key: key,
                    projectName: project.name,
                    timestamp: timestamp,
                    model: resolvedModel,
                    inputTokens: inputTokens,
                    outputTokens: outputTokens,
                    cachedInputTokens: cachedInputTokens,
                    reasoningOutputTokens: reasoningOutputTokens
                )
            )
            model = resolvedModel
            return true
        }

        guard matchedProject else {
            return JSONLParseSnapshot(
                result: .empty,
                lastProcessedOffset: lastProcessedOffset,
                modelTotalTokensByName: totalByModel
            )
        }

        result.events.append(contentsOf: pendingEvents)
        result.entries.append(contentsOf: pendingEntries)
        result.metadataByKey[key] = AIHistorySessionMetadata(
            key: key,
            externalSessionID: key.sessionID,
            sessionTitle: sessionTitle ?? project.name,
            model: model
        )

        return JSONLParseSnapshot(
            result: result,
            lastProcessedOffset: lastProcessedOffset,
            modelTotalTokensByName: totalByModel
        )
    }

    func loadGeminiFileSummaries(project: Project) async -> [AIExternalFileSummary] {
        let fileURLs = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: project.path, homeURL: runtimeHomeURL)
        let projectsURL = AIRuntimeSourceLocator.geminiProjectsURL(homeURL: runtimeHomeURL)
        let tempURL = AIRuntimeSourceLocator.geminiTempDirectoryURL(homeURL: runtimeHomeURL)
        logger.log(
            "history-refresh",
            "source=gemini locator=projects-or-root-marker project=\(project.name) totalFiles=\(fileURLs.count) projectsExists=\(FileManager.default.fileExists(atPath: projectsURL.path)) tmpExists=\(FileManager.default.fileExists(atPath: tempURL.path))"
        )
        return loadFileSummaries(
            source: "gemini",
            fileURLs: fileURLs,
            project: project,
            parser: parseGeminiFile
        )
    }

    func parseGeminiFile(fileURL: URL, project: Project) -> AIHistoryParseResult {
        var result = AIHistoryParseResult.empty
        guard let data = try? Data(contentsOf: fileURL),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let sessionID = normalizedNonEmptyString(object["sessionId"] as? String) else {
            return .empty
        }

        let key = AIHistorySessionKey(source: "gemini", sessionID: sessionID)
        let messages = object["messages"] as? [[String: Any]] ?? object["history"] as? [[String: Any]] ?? []
        var sessionTitle: String?
        var sessionModel: String?

        for message in messages {
            let timestampString = message["timestamp"] as? String
                ?? message["createTime"] as? String
                ?? object["createTime"] as? String
            guard let timestampString,
                  let timestamp = parseCodexISO8601Date(timestampString) else {
                continue
            }

            let role: AIHistorySessionRole = (message["role"] as? String) == "user" ? .user : .assistant
            result.events.append(
                AIHistorySessionEvent(
                    key: key,
                    projectName: project.name,
                    timestamp: timestamp,
                    role: role
                )
            )

            if role == .user, sessionTitle == nil {
                sessionTitle = parseGeminiTitle(from: message["content"])
            }

            let resolvedModel = normalizedNonEmptyString(message["model"] as? String)
                ?? normalizedNonEmptyString(object["model"] as? String)
                ?? sessionModel
                ?? "unknown"
            sessionModel = sessionModel ?? resolvedModel

            if let tokens = message["tokens"] as? [String: Any] {
                let cached = numberValue(tokens["cached"])
                let reasoning = numberValue(tokens["thoughts"])
                let input = max(0, numberValue(tokens["input"]) - cached)
                let output = max(0, numberValue(tokens["output"]) - reasoning)
                let total = input + output + cached + reasoning
                if total > 0 {
                    result.entries.append(
                        AIHistoryUsageEntry(
                            key: key,
                            projectName: project.name,
                            timestamp: timestamp,
                            model: resolvedModel,
                            inputTokens: input,
                            outputTokens: output,
                            cachedInputTokens: cached,
                            reasoningOutputTokens: reasoning
                        )
                    )
                }
                continue
            }

            let usage = message["usage"] as? [String: Any]
                ?? message["usageMetadata"] as? [String: Any]
                ?? message["token_count"] as? [String: Any]
            guard let usage else {
                continue
            }

            let cached = numberValue(usage["cachedContentTokenCount"])
            let reasoning = numberValue(usage["thoughtsTokenCount"])
            let input = max(0, numberValue(usage["promptTokenCount"]) + numberValue(usage["input_tokens"]) - cached)
            let output = max(0, numberValue(usage["candidatesTokenCount"]) + numberValue(usage["output_tokens"]) - reasoning)
            let total = input + output + cached + reasoning
            guard total > 0 else {
                continue
            }

            result.entries.append(
                AIHistoryUsageEntry(
                    key: key,
                    projectName: project.name,
                    timestamp: timestamp,
                    model: resolvedModel,
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
            sessionTitle: sessionTitle ?? project.name,
            model: sessionModel
        )

        return result
    }

    func loadOpenCodeFileSummaries(project: Project) async -> [AIExternalFileSummary] {
        let databaseURL = AIRuntimeSourceLocator.opencodeDatabaseURL(homeURL: runtimeHomeURL)
        if FileManager.default.fileExists(atPath: databaseURL.path) {
            logger.log(
                "history-refresh",
                "source=opencode locator=sqlite project=\(project.name) totalFiles=1 dbPath=\(databaseURL.path)"
            )
            return loadFileSummaries(
                source: "opencode",
                fileURLs: [databaseURL],
                project: project,
                parser: parseOpenCodeDatabase
            )
        }

        let legacyMessageFiles = AIRuntimeSourceLocator.opencodeLegacyMessageFileURLs(homeURL: runtimeHomeURL)
        logger.log(
            "history-refresh",
            "source=opencode locator=legacy-json project=\(project.name) totalFiles=\(legacyMessageFiles.count)"
        )
        return loadFileSummaries(
            source: "opencode",
            fileURLs: legacyMessageFiles,
            project: project,
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
        WHERE json_extract(m.data, '$.path.root') = ?
          AND s.time_archived IS NULL
        ORDER BY m.time_created ASC;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return .empty
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, project.path, -1, AIHistorySQLiteTransient)
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
              rootPath == project.path,
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

    private func claudeRole(from type: String?) -> AIHistorySessionRole? {
        switch type {
        case "user":
            return .user
        case "assistant", "tool_use", "tool_result":
            return .assistant
        default:
            return nil
        }
    }

    private func claudeTitle(from row: [String: Any]) -> String? {
        guard row["type"] as? String == "user",
              let message = row["message"] as? [String: Any] else {
            return normalizedNonEmptyString(row["slug"] as? String)
        }

        if let content = normalizedNonEmptyString(message["content"] as? String) {
            return truncateTitle(content)
        }

        if let items = message["content"] as? [[String: Any]] {
            for item in items {
                if let text = normalizedNonEmptyString(item["text"] as? String) {
                    return truncateTitle(text)
                }
            }
        }

        return normalizedNonEmptyString(row["slug"] as? String)
    }

    private func codexRole(for rowType: String?) -> AIHistorySessionRole {
        switch rowType {
        case "turn_context", "session_meta":
            return .user
        default:
            return .assistant
        }
    }

    private func codexResponseTitle(from payload: [String: Any]) -> String? {
        guard payload["type"] as? String == "message",
              payload["role"] as? String == "user",
              let content = payload["content"] as? [[String: Any]] else {
            return nil
        }

        for item in content {
            guard let text = normalizedNonEmptyString(item["text"] as? String),
                  !text.contains("<environment_context>") else {
                continue
            }
            return truncateTitle(text)
        }
        return nil
    }

    private func parseOpenCodeTimestamp(_ value: String) -> Date? {
        if let milliseconds = Double(value) {
            return Date(timeIntervalSince1970: milliseconds / 1000)
        }
        return parseCodexISO8601Date(value)
    }

    private func truncateTitle(_ value: String) -> String {
        String(value.replacingOccurrences(of: "\n", with: " ").prefix(80))
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func numberValue(_ value: Any?) -> Int {
        switch value {
        case let value as NSNumber:
            return value.intValue
        case let value as Int:
            return value
        case let value as Double:
            return Int(value)
        case let value as String:
            return Int(value) ?? 0
        default:
            return 0
        }
    }
}

let AIHistorySQLiteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

func parseAIHistoryISO8601Date(_ value: String) -> Date? {
    let formatterWithFractional = ISO8601DateFormatter()
    formatterWithFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = formatterWithFractional.date(from: value) {
        return date
    }

    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: value)
}
