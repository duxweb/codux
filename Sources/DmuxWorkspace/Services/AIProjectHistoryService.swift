import CryptoKit
import Foundation
import SQLite3

struct AIProjectHistoryService: Sendable {
    private enum JSONLIndexMode: String {
        case unchanged
        case append
        case rebuild
    }

    private struct JSONLParseSnapshot {
        var result: AIHistoryParseResult
        var lastProcessedOffset: UInt64
        var modelTotalTokensByName: [String: Int]?
    }

    private struct IncrementalSessionComputation {
        var payload: AIExternalFileCheckpointPayload
        var session: AISessionSummary
    }

    private let aggregator: AIHistoryAggregationService
    private let usageStore: AIUsageStore
    private let runtimeHomeURL: URL?
    private let logger = AppDebugLog.shared
    private let calendar = Calendar.autoupdatingCurrent

    init(
        aggregator: AIHistoryAggregationService = AIHistoryAggregationService(),
        usageStore: AIUsageStore = AIUsageStore(),
        runtimeHomeURL: URL? = nil
    ) {
        self.aggregator = aggregator
        self.usageStore = usageStore
        self.runtimeHomeURL = runtimeHomeURL
    }

    func loadProjectSummary(
        project: Project,
        onProgress: @Sendable @escaping (AIIndexingStatus) async -> Void
    ) async throws -> AIProjectDirectorySourceSummary {
        let startedAt = Date()
        await onProgress(.indexing(progress: 0.12, detail: String(localized: "ai.indexing.reading_sources", defaultValue: "Reading index.", bundle: .module)))
        logger.log(
            "history-refresh",
            "project-sources start project=\(project.name) path=\(project.path)"
        )

        async let claudeTask = loadClaudeFileSummaries(project: project)
        async let codexTask = loadCodexFileSummaries(project: project)
        async let geminiTask = loadGeminiFileSummaries(project: project)
        async let opencodeTask = loadOpenCodeFileSummaries(project: project)

        let claude = await claudeTask
        logger.log(
            "history-refresh",
            "project-sources source=claude summary files=\(claude.count) requests=\(totalRequestCount(in: claude)) sessions=\(totalSessionCount(in: claude)) tokens=\(totalTokenCount(in: claude)) elapsedMs=\(elapsedMilliseconds(since: startedAt))"
        )
        await onProgress(.indexing(progress: 0.38, detail: String(localized: "ai.indexing.reading_sources", defaultValue: "Reading index.", bundle: .module)))
        try Task.checkCancellation()

        let codex = await codexTask
        logger.log(
            "history-refresh",
            "project-sources source=codex summary files=\(codex.count) requests=\(totalRequestCount(in: codex)) sessions=\(totalSessionCount(in: codex)) tokens=\(totalTokenCount(in: codex)) elapsedMs=\(elapsedMilliseconds(since: startedAt))"
        )
        await onProgress(.indexing(progress: 0.58, detail: String(localized: "ai.indexing.reading_sources", defaultValue: "Reading index.", bundle: .module)))
        try Task.checkCancellation()

        let gemini = await geminiTask
        logger.log(
            "history-refresh",
            "project-sources source=gemini summary files=\(gemini.count) requests=\(totalRequestCount(in: gemini)) sessions=\(totalSessionCount(in: gemini)) tokens=\(totalTokenCount(in: gemini)) elapsedMs=\(elapsedMilliseconds(since: startedAt))"
        )
        await onProgress(.indexing(progress: 0.74, detail: String(localized: "ai.indexing.reading_sources", defaultValue: "Reading index.", bundle: .module)))
        try Task.checkCancellation()

        let opencode = await opencodeTask
        logger.log(
            "history-refresh",
            "project-sources source=opencode summary files=\(opencode.count) requests=\(totalRequestCount(in: opencode)) sessions=\(totalSessionCount(in: opencode)) tokens=\(totalTokenCount(in: opencode)) elapsedMs=\(elapsedMilliseconds(since: startedAt))"
        )
        await onProgress(.indexing(progress: 0.88, detail: String(localized: "ai.indexing.reading_sources", defaultValue: "Reading index.", bundle: .module)))
        try Task.checkCancellation()

        let summary = aggregator.buildProjectSummary(
            project: project,
            fileSummaries: claude + codex + gemini + opencode
        )
        logger.log(
            "history-refresh",
            "project-sources finish project=\(project.name) files=\(claude.count + codex.count + gemini.count + opencode.count) requests=\(totalRequestCount(in: claude + codex + gemini + opencode)) sessions=\(totalSessionCount(in: claude + codex + gemini + opencode)) tokens=\(totalTokenCount(in: claude + codex + gemini + opencode)) elapsedMs=\(elapsedMilliseconds(since: startedAt))"
        )
        return summary
    }

    private func loadClaudeFileSummaries(project: Project) async -> [AIExternalFileSummary] {
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

    private func parseClaudeFile(fileURL: URL, project: Project) -> JSONLParseSnapshot {
        parseClaudeFile(fileURL: fileURL, project: project, startingAt: 0, seed: nil)
    }

    private func parseClaudeFile(
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

    private func loadCodexFileSummaries(project: Project) async -> [AIExternalFileSummary] {
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

    private func parseCodexFile(fileURL: URL, project: Project) -> JSONLParseSnapshot {
        parseCodexFile(fileURL: fileURL, project: project, startingAt: 0, seed: nil)
    }

    private func parseCodexFile(
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

    private func loadGeminiFileSummaries(project: Project) async -> [AIExternalFileSummary] {
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

    private func parseGeminiFile(fileURL: URL, project: Project) -> AIHistoryParseResult {
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

    private func loadOpenCodeFileSummaries(project: Project) async -> [AIExternalFileSummary] {
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

    private func parseOpenCodeDatabase(fileURL: URL, project: Project) -> AIHistoryParseResult {
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

    private func parseOpenCodeLegacyMessageFile(fileURL: URL, project: Project) -> AIHistoryParseResult {
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

    private func loadIncrementalJSONLFileSummaries(
        source: String,
        fileURLs: [URL],
        project: Project,
        fullParser: (URL, Project) -> JSONLParseSnapshot,
        appendParser: (URL, Project, AIExternalFileCheckpoint) -> JSONLParseSnapshot
    ) -> [AIExternalFileSummary] {
        guard !fileURLs.isEmpty else {
            logger.log(
                "history-refresh",
                "source=\(source) files start project=\(project.name) totalFiles=0"
            )
            logger.log(
                "history-refresh",
                "source=\(source) files finish project=\(project.name) totalFiles=0 cached=0 appended=0 rebuilt=0 requests=0 sessions=0 tokens=0 durationMs=0"
            )
            return []
        }

        let startedAt = Date()
        logger.log(
            "history-refresh",
            "source=\(source) files start project=\(project.name) totalFiles=\(fileURLs.count)"
        )

        var summaries: [AIExternalFileSummary] = []
        summaries.reserveCapacity(fileURLs.count)
        var cachedCount = 0
        var appendedCount = 0
        var rebuiltCount = 0
        var totalRequests = 0
        var totalSessions = 0
        var totalTokens = 0

        for fileURL in fileURLs {
            guard !Task.isCancelled else {
                return []
            }

            let normalizedURL = fileURL.standardizedFileURL
            let filePath = normalizedURL.path
            let modifiedAt = fileModifiedAt(normalizedURL)
            let fileSize = JSONLLineReader.currentFileSize(for: normalizedURL)
            let storedSummary = usageStore.storedExternalSummary(
                source: source,
                filePath: filePath,
                projectPath: project.path
            ) ?? usageStore.storedExternalSummaries(
                source: source,
                projectPath: project.path
            ).first(where: { $0.filePath == filePath })
            let checkpoint = usageStore.externalFileCheckpoint(
                source: source,
                filePath: filePath,
                projectPath: project.path
            )

            switch indexingMode(
                currentModifiedAt: modifiedAt,
                currentFileSize: fileSize,
                storedSummary: storedSummary,
                checkpoint: checkpoint
            ) {
            case .unchanged:
                logger.log(
                    "history-refresh",
                    "source=\(source) file mode=unchanged project=\(project.name) name=\(normalizedURL.lastPathComponent) size=\(fileSize) modifiedAt=\(modifiedAt) hasSummary=\(storedSummary != nil) checkpointOffset=\(checkpoint?.lastOffset ?? 0) checkpointSize=\(checkpoint?.fileSize ?? 0)"
                )
                if let storedSummary {
                    summaries.append(storedSummary)
                    cachedCount += 1
                    totalRequests += totalRequestCount(in: storedSummary)
                    totalSessions += storedSummary.sessions.count
                    totalTokens += totalTokenCount(in: storedSummary)
                }

            case .append:
                logger.log(
                    "history-refresh",
                    "source=\(source) file mode=append project=\(project.name) name=\(normalizedURL.lastPathComponent) size=\(fileSize) modifiedAt=\(modifiedAt) hasSummary=\(storedSummary != nil) checkpointOffset=\(checkpoint?.lastOffset ?? 0) checkpointSize=\(checkpoint?.fileSize ?? 0)"
                )
                guard let storedSummary, let checkpoint else {
                    let snapshot = fullParser(normalizedURL, project)
                    let summary = aggregator.buildExternalFileSummary(
                        source: source,
                        filePath: filePath,
                        fileModifiedAt: modifiedAt,
                        project: project,
                        parseResult: snapshot.result
                    )
                    usageStore.saveExternalSummary(
                        summary,
                        checkpoint: buildCheckpoint(
                            source: source,
                            filePath: filePath,
                            projectPath: project.path,
                            fileModifiedAt: modifiedAt,
                            fileSize: fileSize,
                            snapshot: snapshot,
                            project: project
                        )
                    )
                    summaries.append(summary)
                    logger.log(
                        "history-refresh",
                        "source=\(source) file append-promoted-to-rebuild project=\(project.name) name=\(normalizedURL.lastPathComponent) sessions=\(summary.sessions.count) requests=\(totalRequestCount(in: summary)) tokens=\(totalTokenCount(in: summary)) lastOffset=\(snapshot.lastProcessedOffset)"
                    )
                    rebuiltCount += 1
                    totalRequests += totalRequestCount(in: summary)
                    totalSessions += summary.sessions.count
                    totalTokens += totalTokenCount(in: summary)
                    continue
                }

                let snapshot = appendParser(normalizedURL, project, checkpoint)
                let summary = mergeIncrementalSummary(
                    source: source,
                    filePath: filePath,
                    fileModifiedAt: modifiedAt,
                    fileSize: fileSize,
                    project: project,
                    storedSummary: storedSummary,
                    checkpoint: checkpoint,
                    snapshot: snapshot
                )
                usageStore.saveExternalSummary(
                    summary,
                    checkpoint: buildCheckpoint(
                        source: source,
                        filePath: filePath,
                        projectPath: project.path,
                        fileModifiedAt: modifiedAt,
                        fileSize: fileSize,
                        snapshot: snapshot,
                        project: project,
                        seed: checkpoint.payload
                    )
                )
                summaries.append(summary)
                logger.log(
                    "history-refresh",
                    "source=\(source) file append-complete project=\(project.name) name=\(normalizedURL.lastPathComponent) sessions=\(summary.sessions.count) requests=\(totalRequestCount(in: summary)) tokens=\(totalTokenCount(in: summary)) lastOffset=\(snapshot.lastProcessedOffset)"
                )
                appendedCount += 1
                totalRequests += totalRequestCount(in: summary)
                totalSessions += summary.sessions.count
                totalTokens += totalTokenCount(in: summary)

            case .rebuild:
                logger.log(
                    "history-refresh",
                    "source=\(source) file mode=rebuild project=\(project.name) name=\(normalizedURL.lastPathComponent) size=\(fileSize) modifiedAt=\(modifiedAt) hasSummary=\(storedSummary != nil) checkpointOffset=\(checkpoint?.lastOffset ?? 0) checkpointSize=\(checkpoint?.fileSize ?? 0)"
                )
                let snapshot = fullParser(normalizedURL, project)
                let summary = aggregator.buildExternalFileSummary(
                    source: source,
                    filePath: filePath,
                    fileModifiedAt: modifiedAt,
                    project: project,
                    parseResult: snapshot.result
                )
                usageStore.saveExternalSummary(
                    summary,
                    checkpoint: buildCheckpoint(
                        source: source,
                        filePath: filePath,
                        projectPath: project.path,
                        fileModifiedAt: modifiedAt,
                        fileSize: fileSize,
                        snapshot: snapshot,
                        project: project
                    )
                )
                summaries.append(summary)
                logger.log(
                    "history-refresh",
                    "source=\(source) file rebuild-complete project=\(project.name) name=\(normalizedURL.lastPathComponent) sessions=\(summary.sessions.count) requests=\(totalRequestCount(in: summary)) tokens=\(totalTokenCount(in: summary)) lastOffset=\(snapshot.lastProcessedOffset)"
                )
                rebuiltCount += 1
                totalRequests += totalRequestCount(in: summary)
                totalSessions += summary.sessions.count
                totalTokens += totalTokenCount(in: summary)
            }
        }

        logger.log(
            "history-refresh",
            "source=\(source) files finish project=\(project.name) totalFiles=\(fileURLs.count) cached=\(cachedCount) appended=\(appendedCount) rebuilt=\(rebuiltCount) requests=\(totalRequests) sessions=\(totalSessions) tokens=\(totalTokens) durationMs=\(elapsedMilliseconds(since: startedAt))"
        )
        return summaries
    }

    private func indexingMode(
        currentModifiedAt: Double,
        currentFileSize: UInt64,
        storedSummary: AIExternalFileSummary?,
        checkpoint: AIExternalFileCheckpoint?
    ) -> JSONLIndexMode {
        guard let storedSummary, let checkpoint else {
            return .rebuild
        }

        if checkpoint.lastOffset < currentFileSize {
            return .append
        }

        if currentFileSize < checkpoint.fileSize {
            return .rebuild
        }

        if storedSummary.fileModifiedAt == currentModifiedAt,
           checkpoint.fileModifiedAt == currentModifiedAt,
           checkpoint.lastOffset >= currentFileSize {
            return .unchanged
        }

        if currentFileSize >= checkpoint.fileSize,
           checkpoint.lastOffset <= currentFileSize {
            return .append
        }

        return .rebuild
    }

    private func mergeIncrementalSummary(
        source: String,
        filePath: String,
        fileModifiedAt: Double,
        fileSize: UInt64,
        project: Project,
        storedSummary: AIExternalFileSummary,
        checkpoint: AIExternalFileCheckpoint,
        snapshot: JSONLParseSnapshot
    ) -> AIExternalFileSummary {
        let deltaSummary = aggregator.buildExternalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            project: project,
            parseResult: snapshot.result
        )
        let computation = applyIncrementalParseResult(
            source: source,
            project: project,
            seed: checkpoint.payload,
            parseResult: snapshot.result
        )

        return AIExternalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            projectPath: project.path,
            sessions: computation.map { [$0.session] } ?? storedSummary.sessions,
            dayUsage: mergeHeatmap(storedSummary.dayUsage, deltaSummary.dayUsage),
            timeBuckets: mergeTimeBuckets(storedSummary.timeBuckets, deltaSummary.timeBuckets),
        )
    }

    private func buildCheckpoint(
        source: String,
        filePath: String,
        projectPath: String,
        fileModifiedAt: Double,
        fileSize: UInt64,
        snapshot: JSONLParseSnapshot,
        project: Project,
        seed: AIExternalFileCheckpointPayload? = nil
    ) -> AIExternalFileCheckpoint {
        let computation = applyIncrementalParseResult(
            source: source,
            project: project,
            seed: seed,
            parseResult: snapshot.result
        )
        var payload = computation?.payload ?? normalizePayloadForCurrentDay(seed)
        if let modelTotalTokensByName = snapshot.modelTotalTokensByName {
            if payload == nil {
                payload = AIExternalFileCheckpointPayload(
                    sessionKey: nil,
                    externalSessionID: nil,
                    sessionTitle: nil,
                    lastModel: nil,
                    modelTotalTokensByName: modelTotalTokensByName,
                    firstSeenAt: nil,
                    lastSeenAt: nil,
                    requestCount: 0,
                    totalInputTokens: 0,
                    totalOutputTokens: 0,
                    totalTokens: 0,
                    todayTokens: 0,
                    activeDurationSeconds: 0,
                    waitingForFirstResponse: false,
                    pendingTurnStartAt: nil,
                    pendingTurnEndAt: nil
                )
            } else {
                payload?.modelTotalTokensByName = modelTotalTokensByName
            }
        }

        return AIExternalFileCheckpoint(
            source: source,
            filePath: filePath,
            projectPath: projectPath,
            fileModifiedAt: fileModifiedAt,
            fileSize: fileSize,
            lastOffset: snapshot.lastProcessedOffset,
            lastIndexedAt: Date(),
            payload: payload
        )
    }

    private func applyIncrementalParseResult(
        source: String,
        project: Project,
        seed: AIExternalFileCheckpointPayload?,
        parseResult: AIHistoryParseResult
    ) -> IncrementalSessionComputation? {
        let key = parseResult.metadataByKey.keys.first
            ?? parseResult.entries.first?.key
            ?? parseResult.events.first?.key
            ?? seed.flatMap { payloadKey(from: $0, source: source) }
        guard let key else {
            return nil
        }

        let metadata = parseResult.metadataByKey[key]
        var payload = normalizePayloadForCurrentDay(seed) ?? AIExternalFileCheckpointPayload(
            sessionKey: key.sessionID,
            externalSessionID: nil,
            sessionTitle: nil,
            lastModel: nil,
            modelTotalTokensByName: [:],
            firstSeenAt: nil,
            lastSeenAt: nil,
            requestCount: 0,
            totalInputTokens: 0,
            totalOutputTokens: 0,
            totalTokens: 0,
            todayTokens: 0,
            activeDurationSeconds: 0,
            waitingForFirstResponse: false,
            pendingTurnStartAt: nil,
            pendingTurnEndAt: nil
        )

        payload.sessionKey = key.sessionID
        payload.externalSessionID = normalizedNonEmptyString(metadata?.externalSessionID)
            ?? normalizedNonEmptyString(payload.externalSessionID)
            ?? key.sessionID
        payload.sessionTitle = preferredTitle(payload.sessionTitle, metadata?.sessionTitle) ?? project.name
        payload.lastModel = normalizedNonEmptyString(metadata?.model) ?? normalizedNonEmptyString(payload.lastModel)

        let orderedEvents = parseResult.events
            .filter { $0.key == key }
            .sorted { $0.timestamp < $1.timestamp }
        for event in orderedEvents {
            payload.firstSeenAt = minDate(payload.firstSeenAt, event.timestamp)
            payload.lastSeenAt = maxDate(payload.lastSeenAt, event.timestamp)

            switch event.role {
            case .user:
                if let start = payload.pendingTurnStartAt,
                   let end = payload.pendingTurnEndAt,
                   end > start {
                    payload.activeDurationSeconds += max(0, Int(end.timeIntervalSince(start).rounded()))
                }
                payload.pendingTurnStartAt = nil
                payload.pendingTurnEndAt = nil
                payload.waitingForFirstResponse = true
                payload.requestCount += 1

            case .assistant:
                if payload.waitingForFirstResponse {
                    payload.pendingTurnStartAt = event.timestamp
                    payload.pendingTurnEndAt = event.timestamp
                    payload.waitingForFirstResponse = false
                } else if payload.pendingTurnStartAt != nil {
                    payload.pendingTurnEndAt = event.timestamp
                }
            }
        }

        let startOfToday = calendar.startOfDay(for: Date())
        let orderedEntries = parseResult.entries
            .filter { $0.key == key }
            .sorted { $0.timestamp < $1.timestamp }
        for entry in orderedEntries {
            payload.firstSeenAt = minDate(payload.firstSeenAt, entry.timestamp)
            payload.lastSeenAt = maxDate(payload.lastSeenAt, entry.timestamp)
            payload.lastModel = normalizedNonEmptyString(entry.model) ?? payload.lastModel
            payload.totalInputTokens += entry.inputTokens
            payload.totalOutputTokens += entry.outputTokens
            payload.totalTokens += entry.totalTokens
            if calendar.startOfDay(for: entry.timestamp) == startOfToday {
                payload.todayTokens += entry.totalTokens
            }
        }

        return IncrementalSessionComputation(
            payload: payload,
            session: makeSessionSummary(from: payload, source: source, project: project)
        )
    }

    private func payloadKey(
        from payload: AIExternalFileCheckpointPayload,
        source: String
    ) -> AIHistorySessionKey? {
        guard let sessionID = normalizedNonEmptyString(payload.sessionKey) else {
            return nil
        }
        return AIHistorySessionKey(source: source, sessionID: sessionID)
    }

    private func normalizePayloadForCurrentDay(
        _ payload: AIExternalFileCheckpointPayload?
    ) -> AIExternalFileCheckpointPayload? {
        guard var payload else {
            return nil
        }
        let startOfToday = calendar.startOfDay(for: Date())
        if let lastSeenAt = payload.lastSeenAt,
           calendar.startOfDay(for: lastSeenAt) != startOfToday {
            payload.todayTokens = 0
        }
        return payload
    }

    private func makeSessionSummary(
        from payload: AIExternalFileCheckpointPayload,
        source: String,
        project: Project
    ) -> AISessionSummary {
        let externalSessionID = normalizedNonEmptyString(payload.externalSessionID)
            ?? normalizedNonEmptyString(payload.sessionKey)
            ?? UUID().uuidString
        let firstSeenAt = payload.firstSeenAt ?? Date.distantPast
        let lastSeenAt = payload.lastSeenAt ?? firstSeenAt
        let activeInFlight = {
            guard let start = payload.pendingTurnStartAt,
                  let end = payload.pendingTurnEndAt,
                  end > start else {
                return 0
            }
            return max(0, Int(end.timeIntervalSince(start).rounded()))
        }()

        return AISessionSummary(
            sessionID: deterministicUUID(from: "\(source):\(externalSessionID)"),
            externalSessionID: externalSessionID,
            projectID: project.id,
            projectName: project.name,
            sessionTitle: preferredTitle(payload.sessionTitle, project.name) ?? project.name,
            firstSeenAt: firstSeenAt,
            lastSeenAt: lastSeenAt,
            lastTool: source,
            lastModel: normalizedNonEmptyString(payload.lastModel),
            requestCount: max(payload.requestCount, 1),
            totalInputTokens: payload.totalInputTokens,
            totalOutputTokens: payload.totalOutputTokens,
            totalTokens: payload.totalTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: payload.activeDurationSeconds + activeInFlight,
            todayTokens: payload.todayTokens
        )
    }

    private func mergeHeatmap(
        _ existing: [AIHeatmapDay],
        _ delta: [AIHeatmapDay]
    ) -> [AIHeatmapDay] {
        var map: [Date: AIHeatmapDay] = [:]
        for item in existing + delta {
            if var current = map[item.day] {
                current.totalTokens += item.totalTokens
                current.requestCount += item.requestCount
                map[item.day] = current
            } else {
                map[item.day] = item
            }
        }
        return map.values.sorted { $0.day < $1.day }
    }

    private func mergeTimeBuckets(
        _ existing: [AITimeBucket],
        _ delta: [AITimeBucket]
    ) -> [AITimeBucket] {
        let startOfToday = calendar.startOfDay(for: Date())
        let base = existing.contains(where: { calendar.startOfDay(for: $0.start) == startOfToday }) ? existing : []

        var map: [Date: AITimeBucket] = [:]
        for item in base + delta {
            guard calendar.startOfDay(for: item.start) == startOfToday else {
                continue
            }
            if var current = map[item.start] {
                current.totalTokens += item.totalTokens
                current.requestCount += item.requestCount
                map[item.start] = current
            } else {
                map[item.start] = item
            }
        }
        return map.values.sorted { $0.start < $1.start }
    }

    private func loadFileSummaries(
        source: String,
        fileURLs: [URL],
        project: Project,
        parser: (URL, Project) -> AIHistoryParseResult
    ) -> [AIExternalFileSummary] {
        guard !fileURLs.isEmpty else {
            logger.log(
                "history-refresh",
                "source=\(source) files start project=\(project.name) totalFiles=0"
            )
            logger.log(
                "history-refresh",
                "source=\(source) files finish project=\(project.name) totalFiles=0 cached=0 parsed=0 requests=0 sessions=0 tokens=0 durationMs=0"
            )
            return []
        }

        let startedAt = Date()
        logger.log(
            "history-refresh",
            "source=\(source) files start project=\(project.name) totalFiles=\(fileURLs.count)"
        )

        var summaries: [AIExternalFileSummary] = []
        summaries.reserveCapacity(fileURLs.count)
        var cachedCount = 0
        var parsedCount = 0
        var totalRequests = 0
        var totalSessions = 0
        var totalTokens = 0

        for fileURL in fileURLs {
            guard !Task.isCancelled else {
                return []
            }

            let normalizedURL = fileURL.standardizedFileURL
            let filePath = normalizedURL.path
            let modifiedAt = fileModifiedAt(normalizedURL)

            if let stored = usageStore.storedExternalSummary(
                source: source,
                filePath: filePath,
                projectPath: project.path,
                modifiedAt: modifiedAt
            ) {
                summaries.append(stored)
                cachedCount += 1
                totalRequests += totalRequestCount(in: stored)
                totalSessions += stored.sessions.count
                totalTokens += totalTokenCount(in: stored)
                continue
            }

            let parseResult = parser(normalizedURL, project)
            let summary = aggregator.buildExternalFileSummary(
                source: source,
                filePath: filePath,
                fileModifiedAt: modifiedAt,
                project: project,
                parseResult: parseResult
            )
            usageStore.saveExternalSummary(summary)
            summaries.append(summary)
            parsedCount += 1
            totalRequests += totalRequestCount(in: summary)
            totalSessions += summary.sessions.count
            totalTokens += totalTokenCount(in: summary)
        }

        logger.log(
            "history-refresh",
            "source=\(source) files finish project=\(project.name) totalFiles=\(fileURLs.count) cached=\(cachedCount) parsed=\(parsedCount) requests=\(totalRequests) sessions=\(totalSessions) tokens=\(totalTokens) durationMs=\(elapsedMilliseconds(since: startedAt))"
        )
        return summaries
    }

    private func fileModifiedAt(_ fileURL: URL) -> Double {
        let values = try? fileURL.resourceValues(forKeys: [.contentModificationDateKey])
        return values?.contentModificationDate?.timeIntervalSince1970 ?? 0
    }

    private func elapsedMilliseconds(since startedAt: Date) -> Int {
        Int(Date().timeIntervalSince(startedAt) * 1000)
    }

    private func totalRequestCount(in summaries: [AIExternalFileSummary]) -> Int {
        summaries.reduce(0) { $0 + totalRequestCount(in: $1) }
    }

    private func totalSessionCount(in summaries: [AIExternalFileSummary]) -> Int {
        summaries.reduce(0) { $0 + $1.sessions.count }
    }

    private func totalRequestCount(in summary: AIExternalFileSummary) -> Int {
        summary.sessions.reduce(0) { $0 + $1.requestCount }
    }

    private func totalTokenCount(in summaries: [AIExternalFileSummary]) -> Int {
        summaries.reduce(0) { $0 + totalTokenCount(in: $1) }
    }

    private func totalTokenCount(in summary: AIExternalFileSummary) -> Int {
        summary.timeBuckets.reduce(0) { $0 + $1.totalTokens }
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

    private func preferredTitle(_ lhs: String?, _ rhs: String?) -> String? {
        normalizedNonEmptyString(lhs) ?? normalizedNonEmptyString(rhs)
    }

    private func minDate(_ lhs: Date?, _ rhs: Date) -> Date {
        guard let lhs else {
            return rhs
        }
        return min(lhs, rhs)
    }

    private func maxDate(_ lhs: Date?, _ rhs: Date) -> Date {
        guard let lhs else {
            return rhs
        }
        return max(lhs, rhs)
    }

    private func deterministicUUID(from value: String) -> UUID {
        let digest = SHA256.hash(data: Data(value.utf8))
        let bytes = Array(digest.prefix(16))
        let uuidBytes: uuid_t = (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        )
        return UUID(uuid: uuidBytes)
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

    private func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }
}

private let AIHistorySQLiteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

private func parseAIHistoryISO8601Date(_ value: String) -> Date? {
    let formatterWithFractional = ISO8601DateFormatter()
    formatterWithFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = formatterWithFractional.date(from: value) {
        return date
    }

    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: value)
}
