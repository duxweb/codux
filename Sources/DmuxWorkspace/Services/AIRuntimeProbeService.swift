import Foundation
import SQLite3

private let SQLITE_TRANSIENT_RUNTIME = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

struct AIRuntimeSourceLocator {
    static func claudeProjectLogURLs() -> [URL] {
        let baseURL = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".claude/projects", isDirectory: true)
        let enumerator = FileManager.default.enumerator(at: baseURL, includingPropertiesForKeys: [.isRegularFileKey], options: [.skipsHiddenFiles])
        var urls: [URL] = []
        while let next = enumerator?.nextObject() as? URL {
            guard next.pathExtension == "jsonl" else {
                continue
            }
            urls.append(next)
        }
        return urls.sorted { $0.path < $1.path }
    }

    static func codexDatabaseURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex/state_5.sqlite", isDirectory: false)
    }

    static func codexSessionDirectoryURL(startedAt: Double?) -> URL? {
        let date = startedAt.map { Date(timeIntervalSince1970: $0) } ?? Date()
        let calendar = Calendar(identifier: .gregorian)
        let components = calendar.dateComponents(in: TimeZone(secondsFromGMT: 0) ?? .gmt, from: date)
        guard let year = components.year,
              let month = components.month,
              let day = components.day else {
            return nil
        }

        return URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent(".codex/sessions", isDirectory: true)
            .appendingPathComponent(String(format: "%04d", year), isDirectory: true)
            .appendingPathComponent(String(format: "%02d", month), isDirectory: true)
            .appendingPathComponent(String(format: "%02d", day), isDirectory: true)
    }

    static func codexLatestSessionFile(projectPath: String, startedAt: Double?) -> URL? {
        guard let directoryURL = codexSessionDirectoryURL(startedAt: startedAt),
              let fileURLs = try? FileManager.default.contentsOfDirectory(
                  at: directoryURL,
                  includingPropertiesForKeys: [.contentModificationDateKey],
                  options: [.skipsHiddenFiles]
              ) else {
            return nil
        }

        let candidates = fileURLs
            .filter { $0.pathExtension == "jsonl" }
            .sorted {
                let lhs = ((try? $0.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
                let rhs = ((try? $1.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
                return lhs > rhs
            }

        for candidate in candidates {
            guard let data = try? Data(contentsOf: candidate),
                  let text = String(data: data.suffix(131_072), encoding: .utf8) else {
                continue
            }
            if text.contains(projectPath) {
                return candidate
            }
        }

        return candidates.first
    }

    static func opencodeDatabaseURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".local/share/opencode/opencode.db", isDirectory: false)
    }

    static func geminiProjectsURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".gemini/projects.json", isDirectory: false)
    }

    static func geminiTempDirectoryURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".gemini/tmp", isDirectory: true)
    }

    static func geminiProjectTempDirectoryURL(projectPath: String) -> URL? {
        let projectsURL = geminiProjectsURL()
        if let data = try? Data(contentsOf: projectsURL),
           let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let projects = object["projects"] as? [String: Any],
           let directoryName = projects[projectPath] as? String,
           !directoryName.isEmpty {
            return geminiTempDirectoryURL().appendingPathComponent(directoryName, isDirectory: true)
        }

        let tempURL = geminiTempDirectoryURL()
        guard let entries = try? FileManager.default.contentsOfDirectory(
            at: tempURL,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ) else {
            return nil
        }

        for entry in entries {
            let rootMarker = entry.appendingPathComponent(".project_root", isDirectory: false)
            guard let value = try? String(contentsOf: rootMarker, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines),
                  value == projectPath else {
                continue
            }
            return entry
        }
        return nil
    }

    static func geminiChatsDirectoryURL(projectPath: String) -> URL? {
        geminiProjectTempDirectoryURL(projectPath: projectPath)?
            .appendingPathComponent("chats", isDirectory: true)
    }

    static func geminiSessionFileURLs(projectPath: String) -> [URL] {
        guard let chatsDirectoryURL = geminiChatsDirectoryURL(projectPath: projectPath),
              let fileURLs = try? FileManager.default.contentsOfDirectory(
                  at: chatsDirectoryURL,
                  includingPropertiesForKeys: [.contentModificationDateKey],
                  options: [.skipsHiddenFiles]
              ) else {
            return []
        }

        return fileURLs
            .filter {
                $0.pathExtension == "json" && $0.lastPathComponent.hasPrefix("session-")
            }
            .sorted {
                let lhs = ((try? $0.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
                let rhs = ((try? $1.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
                return lhs > rhs
            }
    }
}

struct AIRuntimeContextSnapshot {
    var tool: String
    var externalSessionID: String?
    var model: String?
    var inputTokens: Int
    var outputTokens: Int
    var totalTokens: Int
    var updatedAt: Double
    var responseState: AIResponseState?
}

struct CodexParsedRuntimeState {
    var model: String?
    var totalTokens: Int?
    var updatedAt: Double?
    var responseState: AIResponseState?
}

struct GeminiParsedRuntimeState {
    var externalSessionID: String
    var title: String?
    var model: String?
    var inputTokens: Int
    var outputTokens: Int
    var totalTokens: Int
    var startedAt: Double
    var updatedAt: Double
    var responseState: AIResponseState?
}

func parseCodexRolloutRuntimeState(fileURL: URL?) -> CodexParsedRuntimeState? {
    parseCodexRuntimeState(fileURL: fileURL, projectPath: nil)
}

func parseCodexSessionRuntimeState(fileURL: URL?, projectPath: String) -> CodexParsedRuntimeState? {
    parseCodexRuntimeState(fileURL: fileURL, projectPath: projectPath)
}

private func parseCodexRuntimeState(fileURL: URL?, projectPath: String?) -> CodexParsedRuntimeState? {
    guard let fileURL,
          FileManager.default.fileExists(atPath: fileURL.path) else {
        return nil
    }

    let lines = tailJSONLinesFromFile(at: fileURL)
    guard !lines.isEmpty else {
        return nil
    }

    var latestModel: String?
    var latestUpdatedAt: Double?
    var latestStartedAt: Double?
    var latestCompletedAt: Double?
    var totalTokens: Int?

    for line in lines {
        guard let data = line.data(using: .utf8),
              let row = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            continue
        }

        let timestamp = (row["timestamp"] as? String).flatMap(parseCodexISO8601Date)?.timeIntervalSince1970
        if let timestamp {
            latestUpdatedAt = max(latestUpdatedAt ?? timestamp, timestamp)
        }

        let payload = row["payload"] as? [String: Any] ?? [:]
        if row["type"] as? String == "turn_context",
           let model = payload["model"] as? String,
           !model.isEmpty,
           projectPath == nil || (payload["cwd"] as? String) == projectPath {
            latestModel = model
            continue
        }

        guard row["type"] as? String == "event_msg",
              let eventType = payload["type"] as? String else {
            continue
        }

        switch eventType {
        case "task_started":
            if let started = payload["started_at"] as? NSNumber {
                latestStartedAt = started.doubleValue
            } else if let timestamp {
                latestStartedAt = timestamp
            }
        case "task_complete":
            if let completed = payload["completed_at"] as? NSNumber {
                latestCompletedAt = completed.doubleValue
            } else if let timestamp {
                latestCompletedAt = timestamp
            }
        case "token_count":
            let info = payload["info"] as? [String: Any] ?? [:]
            let totalUsage = info["total_token_usage"] as? [String: Any] ?? [:]
            if let total = totalUsage["total_tokens"] as? NSNumber {
                totalTokens = total.intValue
            }
        default:
            continue
        }
    }

    let responseState: AIResponseState? = {
        guard let latestStartedAt else {
            return nil
        }
        if let latestCompletedAt, latestCompletedAt >= latestStartedAt {
            return .idle
        }
        return .responding
    }()

    return CodexParsedRuntimeState(
        model: latestModel,
        totalTokens: totalTokens,
        updatedAt: latestUpdatedAt,
        responseState: responseState
    )
}

private func tailJSONLinesFromFile(at fileURL: URL, maxBytes: Int = 262_144) -> [String] {
    guard let handle = try? FileHandle(forReadingFrom: fileURL) else {
        return []
    }
    defer {
        try? handle.close()
    }

    let fileSize = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
    let offset = max(0, fileSize - maxBytes)
    try? handle.seek(toOffset: UInt64(offset))
    let data = handle.readDataToEndOfFile()
    guard let text = String(data: data, encoding: .utf8), !text.isEmpty else {
        return []
    }

    let lines = text.split(separator: "\n").map(String.init)
    if offset == 0 {
        return lines
    }
    return Array(lines.dropFirst())
}

private func parseCodexISO8601Date(_ value: String) -> Date? {
    let formatterWithFractional = ISO8601DateFormatter()
    formatterWithFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = formatterWithFractional.date(from: value) {
        return date
    }

    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: value)
}

func parseGeminiSessionRuntimeState(
    projectPath: String,
    startedAt: Double?,
    preferredSessionID: String?
) -> GeminiParsedRuntimeState? {
    let fileURLs = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: projectPath)
    guard !fileURLs.isEmpty else {
        return nil
    }

    var preferredMatch: GeminiParsedRuntimeState?
    var candidateMatch: GeminiParsedRuntimeState?

    for fileURL in fileURLs.prefix(16) {
        guard let state = parseGeminiSessionRuntimeState(fileURL: fileURL) else {
            continue
        }

        if let preferredSessionID,
           state.externalSessionID == preferredSessionID {
            preferredMatch = state
            break
        }

        if let startedAt {
            if state.updatedAt >= startedAt - 5 {
                if candidateMatch == nil || state.updatedAt > (candidateMatch?.updatedAt ?? 0) {
                    candidateMatch = state
                }
            }
        } else if candidateMatch == nil {
            candidateMatch = state
        }
    }

    return preferredMatch ?? candidateMatch
}

func parseGeminiSessionRuntimeState(fileURL: URL) -> GeminiParsedRuntimeState? {
    guard let data = try? Data(contentsOf: fileURL),
          let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let externalSessionID = object["sessionId"] as? String,
          !externalSessionID.isEmpty else {
        return nil
    }

    let messages = object["messages"] as? [[String: Any]] ?? []
    let startedAtDate = (object["startTime"] as? String).flatMap(parseCodexISO8601Date)
        ?? messages.compactMap { ($0["timestamp"] as? String).flatMap(parseCodexISO8601Date) }.min()
        ?? .distantPast
    let updatedAtDate = (object["lastUpdated"] as? String).flatMap(parseCodexISO8601Date)
        ?? messages.compactMap { ($0["timestamp"] as? String).flatMap(parseCodexISO8601Date) }.max()
        ?? .distantPast

    var model: String?
    var totalTokens = 0
    var outputTokens = 0
    var title: String?

    for message in messages {
        let type = message["type"] as? String
        if title == nil, type == "user" {
            title = parseGeminiTitle(from: message["content"])
        }

        guard type == "gemini" else {
            continue
        }

        if let candidateModel = message["model"] as? String, !candidateModel.isEmpty {
            model = candidateModel
        }

        let tokens = message["tokens"] as? [String: Any] ?? [:]
        let messageTotal = (tokens["total"] as? NSNumber)?.intValue
            ?? ((tokens["input"] as? NSNumber)?.intValue ?? 0)
            + ((tokens["output"] as? NSNumber)?.intValue ?? 0)
            + ((tokens["cached"] as? NSNumber)?.intValue ?? 0)
            + ((tokens["thoughts"] as? NSNumber)?.intValue ?? 0)
            + ((tokens["tool"] as? NSNumber)?.intValue ?? 0)
        totalTokens += max(0, messageTotal)
        outputTokens += max(0, (tokens["output"] as? NSNumber)?.intValue ?? 0)
    }

    let inputTokens = max(0, totalTokens - outputTokens)
    let responseState: AIResponseState? = {
        let lastRelevantType = messages
            .reversed()
            .compactMap { $0["type"] as? String }
            .first { type in
                switch type {
                case "warning":
                    return false
                default:
                    return true
                }
            }

        switch lastRelevantType {
        case "user":
            return .responding
        case "gemini", "error", "info":
            return .idle
        default:
            if totalTokens > 0 || model != nil {
                return .idle
            }
            return nil
        }
    }()

    return GeminiParsedRuntimeState(
        externalSessionID: externalSessionID,
        title: title,
        model: model,
        inputTokens: inputTokens,
        outputTokens: outputTokens,
        totalTokens: totalTokens,
        startedAt: startedAtDate.timeIntervalSince1970,
        updatedAt: updatedAtDate.timeIntervalSince1970,
        responseState: responseState
    )
}

func parseGeminiTitle(from content: Any?) -> String? {
    if let text = content as? String {
        return normalizeGeminiTitle(text)
    }
    if let items = content as? [[String: Any]] {
        for item in items {
            if let text = item["text"] as? String,
               let normalized = normalizeGeminiTitle(text) {
                return normalized
            }
        }
    }
    return nil
}

func normalizeGeminiTitle(_ value: String?) -> String? {
    guard let value else {
        return nil
    }
    let trimmed = value.replacingOccurrences(of: "\n", with: " ").trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else {
        return nil
    }
    return String(trimmed.prefix(80))
}

private actor ClaudeRuntimeLogCache {
    struct SessionAggregate {
        var model: String?
        var inputTokens: Int
        var outputTokens: Int
        var totalTokens: Int
        var updatedAt: Double
        var lastUserAt: Double
        var lastCompletionAt: Double
    }

    struct FileState {
        var offset: UInt64
        var sessions: [String: SessionAggregate]
    }

    static let shared = ClaudeRuntimeLogCache()

    private var fileStatesByProjectPath: [String: [String: FileState]] = [:]

    func snapshot(projectPath: String, externalSessionID: String) -> AIRuntimeContextSnapshot? {
        let sessions = updateAndMergeSessions(projectPath: projectPath)
        guard let session = sessions[externalSessionID] else {
            return nil
        }

        return AIRuntimeContextSnapshot(
            tool: "claude",
            externalSessionID: externalSessionID,
            model: session.model,
            inputTokens: session.inputTokens,
            outputTokens: session.outputTokens,
            totalTokens: session.totalTokens,
            updatedAt: session.updatedAt,
            responseState: responseState(for: session)
        )
    }

    private func updateAndMergeSessions(projectPath: String) -> [String: SessionAggregate] {
        let fileURLs = AIRuntimeSourceLocator.claudeProjectLogURLs()
        guard !fileURLs.isEmpty else {
            fileStatesByProjectPath[projectPath] = [:]
            return [:]
        }

        var fileStates = fileStatesByProjectPath[projectPath] ?? [:]
        let visiblePaths = Set(fileURLs.map(\.path))
        fileStates = fileStates.filter { visiblePaths.contains($0.key) }

        for fileURL in fileURLs {
            let path = fileURL.path
            let fileSize = currentFileSize(for: fileURL)
            let existing = fileStates[path]

            if existing == nil || fileSize < (existing?.offset ?? 0) {
                let sessions = parseSessions(in: fileURL, projectPath: projectPath, startingAt: 0)
                fileStates[path] = FileState(offset: fileSize, sessions: sessions)
                continue
            }

            guard let existing else {
                continue
            }

            if fileSize == existing.offset {
                continue
            }

            let deltaSessions = parseSessions(in: fileURL, projectPath: projectPath, startingAt: existing.offset)
            var mergedSessions = existing.sessions
            for (sessionID, delta) in deltaSessions {
                let aggregate = mergeSessionAggregate(
                    mergedSessions[sessionID],
                    with: delta
                )
                mergedSessions[sessionID] = aggregate
            }
            fileStates[path] = FileState(offset: fileSize, sessions: mergedSessions)
        }

        fileStatesByProjectPath[projectPath] = fileStates
        return mergedSessions(from: fileStates)
    }

    private func mergedSessions(from fileStates: [String: FileState]) -> [String: SessionAggregate] {
        var sessions: [String: SessionAggregate] = [:]
        for fileState in fileStates.values {
            for (sessionID, contribution) in fileState.sessions {
                let aggregate = mergeSessionAggregate(
                    sessions[sessionID],
                    with: contribution
                )
                sessions[sessionID] = aggregate
            }
        }
        return sessions
    }

    private func responseState(for aggregate: SessionAggregate) -> AIResponseState? {
        guard aggregate.lastUserAt > 0 else {
            return nil
        }
        return aggregate.lastUserAt > aggregate.lastCompletionAt ? .responding : .idle
    }

    private func currentFileSize(for fileURL: URL) -> UInt64 {
        let size = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
        return UInt64(max(0, size))
    }

    private func parseSessions(in fileURL: URL, projectPath: String, startingAt offset: UInt64) -> [String: SessionAggregate] {
        guard let handle = try? FileHandle(forReadingFrom: fileURL) else {
            return [:]
        }
        defer {
            try? handle.close()
        }

        do {
            try handle.seek(toOffset: offset)
        } catch {
            return [:]
        }

        let data = handle.readDataToEndOfFile()
        guard !data.isEmpty,
              let text = String(data: data, encoding: .utf8) else {
            return [:]
        }

        var sessions: [String: SessionAggregate] = [:]
        for line in text.split(separator: "\n") {
            guard let lineData = line.data(using: .utf8),
                  let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any],
                  let cwd = row["cwd"] as? String,
                  let sessionID = row["sessionId"] as? String,
                  cwd == projectPath else {
                continue
            }

            let timestamp = (row["timestamp"] as? String).flatMap(parseISO8601Date)?.timeIntervalSince1970 ?? 0
            let message = row["message"] as? [String: Any] ?? [:]
            let usage = message["usage"] as? [String: Any] ?? [:]
            let input = (usage["input_tokens"] as? NSNumber)?.intValue ?? 0
            let output = (usage["output_tokens"] as? NSNumber)?.intValue ?? 0
            let cacheCreation = (usage["cache_creation_input_tokens"] as? NSNumber)?.intValue ?? 0
            let cacheRead = (usage["cache_read_input_tokens"] as? NSNumber)?.intValue ?? 0
            let effectiveInput = input + cacheCreation + cacheRead
            let total = effectiveInput + output

            var aggregate = sessions[sessionID] ?? SessionAggregate(
                model: nil,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                updatedAt: timestamp,
                lastUserAt: 0,
                lastCompletionAt: 0
            )
            let rowType = row["type"] as? String
            if rowType == "user" {
                aggregate.lastUserAt = max(aggregate.lastUserAt, timestamp)
            } else if rowType == "assistant" {
                let stopReason = message["stop_reason"] as? String
                if stopReason == "end_turn" {
                    aggregate.lastCompletionAt = max(aggregate.lastCompletionAt, timestamp)
                }
            } else if rowType == "system" {
                let subtype = row["subtype"] as? String
                if subtype == "turn_duration" || subtype == "stop_hook_summary" {
                    aggregate.lastCompletionAt = max(aggregate.lastCompletionAt, timestamp)
                }
            }
            if let model = message["model"] as? String, !model.isEmpty {
                aggregate.model = model
            }
            aggregate.inputTokens += effectiveInput
            aggregate.outputTokens += output
            aggregate.totalTokens += total
            aggregate.updatedAt = max(aggregate.updatedAt, timestamp)
            sessions[sessionID] = aggregate
        }
        return sessions
    }

    private func mergeSessionAggregate(
        _ existing: SessionAggregate?,
        with contribution: SessionAggregate
    ) -> SessionAggregate {
        var aggregate = existing ?? SessionAggregate(
            model: nil,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            updatedAt: contribution.updatedAt,
            lastUserAt: contribution.lastUserAt,
            lastCompletionAt: contribution.lastCompletionAt
        )
        let previousUpdatedAt = aggregate.updatedAt
        if let model = contribution.model,
           !model.isEmpty,
           aggregate.model == nil || contribution.updatedAt >= previousUpdatedAt {
            aggregate.model = model
        }
        aggregate.inputTokens += contribution.inputTokens
        aggregate.outputTokens += contribution.outputTokens
        aggregate.totalTokens += contribution.totalTokens
        aggregate.updatedAt = max(previousUpdatedAt, contribution.updatedAt)
        aggregate.lastUserAt = max(aggregate.lastUserAt, contribution.lastUserAt)
        aggregate.lastCompletionAt = max(aggregate.lastCompletionAt, contribution.lastCompletionAt)
        return aggregate
    }

    private func parseISO8601Date(_ value: String) -> Date? {
        let formatterWithFractional = ISO8601DateFormatter()
        formatterWithFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatterWithFractional.date(from: value) {
            return date
        }

        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }
}

actor AIRuntimeContextProbe {
    private var codexThreadIDByRuntimeSessionID: [String: String] = [:]
    private var claudeSessionIDByRuntimeSessionID: [String: String] = [:]
    private var lastClaudeLogMessageByRuntimeSessionID: [String: String] = [:]
    private var lastClaudeLogAtByDedupeKey: [String: Double] = [:]
    private var opencodeSessionIDByRuntimeSessionID: [String: String] = [:]
    private var geminiSessionIDByRuntimeSessionID: [String: String] = [:]
    private let fileManager = FileManager.default
    private let logger = AppDebugLog.shared

    func snapshot(for tool: String, runtimeSessionID: String, projectPath: String, startedAt: Double) async -> AIRuntimeContextSnapshot? {
        switch normalize(tool: tool) {
        case "codex":
            return codexSnapshot(runtimeSessionID: runtimeSessionID, projectPath: projectPath, startedAt: startedAt)
        case "claude":
            return await claudeSnapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: nil
            )
        case "opencode":
            return opencodeSnapshot(runtimeSessionID: runtimeSessionID, projectPath: projectPath, startedAt: startedAt)
        case "gemini":
            return geminiSnapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: nil
            )
        default:
            return AIRuntimeContextSnapshot(
                tool: tool,
                externalSessionID: nil,
                model: nil,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                updatedAt: Date().timeIntervalSince1970,
                responseState: nil
            )
        }
    }

    private func normalize(tool: String) -> String {
        switch tool {
        case "claude-code":
            return "claude"
        default:
            return tool
        }
    }

    private func codexSnapshot(runtimeSessionID: String, projectPath: String, startedAt: Double) -> AIRuntimeContextSnapshot? {
        let dbPath = NSHomeDirectory() + "/.codex/state_5.sqlite"
        guard FileManager.default.fileExists(atPath: dbPath) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(dbPath, &db) == SQLITE_OK, let db else {
            return nil
        }
        defer { sqlite3_close(db) }

        if let threadID = codexThreadIDByRuntimeSessionID[runtimeSessionID] {
            let sql = """
            SELECT model, tokens_used, updated_at
            FROM threads
            WHERE id = ?
            LIMIT 1;
            """

            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
                return nil
            }
            defer { sqlite3_finalize(statement) }
            sqlite3_bind_text(statement, 1, threadID, -1, SQLITE_TRANSIENT_RUNTIME)

            guard sqlite3_step(statement) == SQLITE_ROW else {
                codexThreadIDByRuntimeSessionID[runtimeSessionID] = nil
                return nil
            }

            var model = sqlite3_column_type(statement, 0) == SQLITE_NULL ? nil : String(cString: sqlite3_column_text(statement, 0))
            var totalTokens = Int(sqlite3_column_int64(statement, 1))
            var updatedAt = sqlite3_column_double(statement, 2)
            let parsedState = parseCodexSessionRuntimeState(
                fileURL: AIRuntimeSourceLocator.codexLatestSessionFile(projectPath: projectPath, startedAt: startedAt),
                projectPath: projectPath
            )
            if let parsedModel = parsedState?.model, !parsedModel.isEmpty {
                model = parsedModel
            }
            if let parsedTotalTokens = parsedState?.totalTokens {
                totalTokens = parsedTotalTokens
            }
            if let parsedUpdatedAt = parsedState?.updatedAt {
                updatedAt = max(updatedAt, parsedUpdatedAt)
            }
            return AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: threadID,
                model: model,
                inputTokens: totalTokens,
                outputTokens: 0,
                totalTokens: totalTokens,
                updatedAt: updatedAt,
                responseState: parsedState?.responseState
            )
        }

        let sql = """
        SELECT id, model, tokens_used, updated_at
        FROM threads
        WHERE cwd = ?
          AND updated_at >= ?
        ORDER BY updated_at DESC
        LIMIT 1;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_RUNTIME)
        sqlite3_bind_double(statement, 2, startedAt - 2)

        guard sqlite3_step(statement) == SQLITE_ROW else {
            return nil
        }

        guard let threadID = sqlite3_column_text(statement, 0).map({ String(cString: $0) }) else {
            return nil
        }

        codexThreadIDByRuntimeSessionID[runtimeSessionID] = threadID

        var model = sqlite3_column_type(statement, 1) == SQLITE_NULL ? nil : String(cString: sqlite3_column_text(statement, 1))
        var totalTokens = Int(sqlite3_column_int64(statement, 2))
        var updatedAt = sqlite3_column_double(statement, 3)
        let parsedState = parseCodexSessionRuntimeState(
            fileURL: AIRuntimeSourceLocator.codexLatestSessionFile(projectPath: projectPath, startedAt: startedAt),
            projectPath: projectPath
        )
        if let parsedModel = parsedState?.model, !parsedModel.isEmpty {
            model = parsedModel
        }
        if let parsedTotalTokens = parsedState?.totalTokens {
            totalTokens = parsedTotalTokens
        }
        if let parsedUpdatedAt = parsedState?.updatedAt {
            updatedAt = max(updatedAt, parsedUpdatedAt)
        }
        return AIRuntimeContextSnapshot(
            tool: "codex",
            externalSessionID: threadID,
            model: model,
            inputTokens: totalTokens,
            outputTokens: 0,
            totalTokens: totalTokens,
            updatedAt: updatedAt,
            responseState: parsedState?.responseState
        )
    }

    func snapshot(
        for tool: String,
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) async -> AIRuntimeContextSnapshot? {
        switch normalize(tool: tool) {
        case "codex":
            return codexSnapshot(runtimeSessionID: runtimeSessionID, projectPath: projectPath, startedAt: startedAt)
        case "claude":
            return await claudeSnapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: knownExternalSessionID
            )
        case "opencode":
            return opencodeSnapshot(runtimeSessionID: runtimeSessionID, projectPath: projectPath, startedAt: startedAt)
        case "gemini":
            return geminiSnapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: knownExternalSessionID
            )
        default:
            return AIRuntimeContextSnapshot(
                tool: tool,
                externalSessionID: nil,
                model: nil,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                updatedAt: Date().timeIntervalSince1970,
                responseState: nil
            )
        }
    }

    private func geminiSnapshot(
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) -> AIRuntimeContextSnapshot? {
        let preferredSessionID = knownExternalSessionID
            ?? geminiSessionIDByRuntimeSessionID[runtimeSessionID]
        guard let parsedState = parseGeminiSessionRuntimeState(
            projectPath: projectPath,
            startedAt: startedAt,
            preferredSessionID: preferredSessionID
        ) else {
            return nil
        }

        geminiSessionIDByRuntimeSessionID[runtimeSessionID] = parsedState.externalSessionID
        logger.log(
            "gemini-runtime",
            "hit runtimeSession=\(runtimeSessionID) externalSession=\(parsedState.externalSessionID) model=\(parsedState.model ?? "nil") total=\(parsedState.totalTokens) response=\(parsedState.responseState?.rawValue ?? "nil")"
        )
        return AIRuntimeContextSnapshot(
            tool: "gemini",
            externalSessionID: parsedState.externalSessionID,
            model: parsedState.model,
            inputTokens: parsedState.inputTokens,
            outputTokens: parsedState.outputTokens,
            totalTokens: parsedState.totalTokens,
            updatedAt: parsedState.updatedAt,
            responseState: parsedState.responseState
        )
    }

    private func claudeSnapshot(
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) async -> AIRuntimeContextSnapshot? {
        if let knownExternalSessionID, !knownExternalSessionID.isEmpty {
            if let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: knownExternalSessionID
            ) {
                logClaudeRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "hit source=live runtimeSession=\(runtimeSessionID) externalSession=\(knownExternalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
                )
                return normalizedClaudeSnapshot(
                    runtimeSessionID: runtimeSessionID,
                    externalSessionID: knownExternalSessionID,
                    snapshot: snapshot
                )
            }
            logClaudeRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "miss source=live runtimeSession=\(runtimeSessionID) externalSession=\(knownExternalSessionID)",
                dedupeKey: "\(runtimeSessionID):miss:live",
                minimumInterval: 15
            )
        }

        if let mappedExternalSessionID = claudeMappedExternalSessionID(for: runtimeSessionID) {
            guard let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: mappedExternalSessionID
            ) else {
                logClaudeRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "miss source=mapped runtimeSession=\(runtimeSessionID) externalSession=\(mappedExternalSessionID)",
                    dedupeKey: "\(runtimeSessionID):miss:mapped",
                    minimumInterval: 15
                )
                return nil
            }
            logClaudeRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "hit source=mapped runtimeSession=\(runtimeSessionID) externalSession=\(mappedExternalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
            )
            return normalizedClaudeSnapshot(
                runtimeSessionID: runtimeSessionID,
                externalSessionID: mappedExternalSessionID,
                snapshot: snapshot
            )
        }

        if let externalSessionID = claudeSessionIDByRuntimeSessionID[runtimeSessionID] {
            guard let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: externalSessionID
            ) else {
                logClaudeRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "miss source=cached runtimeSession=\(runtimeSessionID) externalSession=\(externalSessionID)",
                    dedupeKey: "\(runtimeSessionID):miss:cached",
                    minimumInterval: 15
                )
                return nil
            }
            logClaudeRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "hit source=cached runtimeSession=\(runtimeSessionID) externalSession=\(externalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
            )
            return normalizedClaudeSnapshot(
                runtimeSessionID: runtimeSessionID,
                externalSessionID: externalSessionID,
                snapshot: snapshot
            )
        }
        logClaudeRuntime(
            runtimeSessionID: runtimeSessionID,
            message: "miss source=none runtimeSession=\(runtimeSessionID) projectPath=\(projectPath)",
            dedupeKey: "\(runtimeSessionID):miss:none",
            minimumInterval: 15
        )
        return nil
    }

    private func logClaudeRuntime(
        runtimeSessionID: String,
        message: String,
        dedupeKey: String? = nil,
        minimumInterval: TimeInterval = 0
    ) {
        if let dedupeKey, minimumInterval > 0 {
            let now = Date().timeIntervalSince1970
            if let lastLoggedAt = lastClaudeLogAtByDedupeKey[dedupeKey],
               now - lastLoggedAt < minimumInterval {
                return
            }
            lastClaudeLogAtByDedupeKey[dedupeKey] = now
        }
        guard lastClaudeLogMessageByRuntimeSessionID[runtimeSessionID] != message else {
            return
        }
        lastClaudeLogMessageByRuntimeSessionID[runtimeSessionID] = message
        logger.log("claude-runtime", message)
    }

    private func claudeMappedExternalSessionID(for runtimeSessionID: String) -> String? {
        let fileURL = AIRuntimeBridgeService()
            .claudeSessionMapDirectoryURL()
            .appendingPathComponent("\(runtimeSessionID).json", isDirectory: false)
        guard let data = try? Data(contentsOf: fileURL),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let externalSessionID = object["externalSessionID"] as? String,
              !externalSessionID.isEmpty else {
            return nil
        }
        return externalSessionID
    }

    private func normalizedClaudeSnapshot(
        runtimeSessionID: String,
        externalSessionID: String,
        snapshot: AIRuntimeContextSnapshot
    ) -> AIRuntimeContextSnapshot {
        if claudeSessionIDByRuntimeSessionID[runtimeSessionID] != externalSessionID {
            claudeSessionIDByRuntimeSessionID[runtimeSessionID] = externalSessionID
        }

        return AIRuntimeContextSnapshot(
            tool: snapshot.tool,
            externalSessionID: externalSessionID,
            model: snapshot.model,
            inputTokens: snapshot.inputTokens,
            outputTokens: snapshot.outputTokens,
            totalTokens: snapshot.totalTokens,
            updatedAt: snapshot.updatedAt,
            responseState: snapshot.responseState
        )
    }

    private func opencodeSnapshot(runtimeSessionID: String, projectPath: String, startedAt: Double) -> AIRuntimeContextSnapshot? {
        let dbPath = NSHomeDirectory() + "/.local/share/opencode/opencode.db"
        guard FileManager.default.fileExists(atPath: dbPath) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(dbPath, &db) == SQLITE_OK, let db else {
            return nil
        }
        defer { sqlite3_close(db) }

        let latestSessionID: String
        if let existingSessionID = opencodeSessionIDByRuntimeSessionID[runtimeSessionID] {
            latestSessionID = existingSessionID
        } else {
            let latestSessionSQL = """
            SELECT s.id
            FROM session s
            JOIN message m ON m.session_id = s.id
            WHERE s.directory = ?
              AND m.time_created >= ?
            ORDER BY m.time_created DESC
            LIMIT 1;
            """

            var latestSessionStatement: OpaquePointer?
            guard sqlite3_prepare_v2(db, latestSessionSQL, -1, &latestSessionStatement, nil) == SQLITE_OK,
                  let latestSessionStatement else {
                return nil
            }
            defer { sqlite3_finalize(latestSessionStatement) }
            sqlite3_bind_text(latestSessionStatement, 1, projectPath, -1, SQLITE_TRANSIENT_RUNTIME)
            sqlite3_bind_double(latestSessionStatement, 2, (startedAt - 2) * 1000)

            guard sqlite3_step(latestSessionStatement) == SQLITE_ROW,
                  let latestSessionIDPointer = sqlite3_column_text(latestSessionStatement, 0) else {
                return nil
            }
            latestSessionID = String(cString: latestSessionIDPointer)
            opencodeSessionIDByRuntimeSessionID[runtimeSessionID] = latestSessionID
        }

        let sql = """
        SELECT json_extract(m.data, '$.modelID') AS model,
               COALESCE(json_extract(m.data, '$.tokens.input'), 0) AS input_tokens,
               COALESCE(json_extract(m.data, '$.tokens.output'), 0) AS output_tokens,
               COALESCE(json_extract(m.data, '$.tokens.cache.read'), 0) AS cache_read_tokens,
               COALESCE(json_extract(m.data, '$.tokens.cache.write'), 0) AS cache_write_tokens,
               COALESCE(json_extract(m.data, '$.tokens.total'), 0) AS total_tokens,
               COALESCE(json_extract(m.data, '$.time.completed'), json_extract(m.data, '$.time.created'), 0) AS completed_at
        FROM session s
        JOIN message m ON m.session_id = s.id
        WHERE s.directory = ?
          AND s.id = ?
        ORDER BY m.time_created DESC;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT_RUNTIME)
        sqlite3_bind_text(statement, 2, latestSessionID, -1, SQLITE_TRANSIENT_RUNTIME)

        var latestModel: String?
        var inputTokens = 0
        var outputTokens = 0
        var totalTokens = 0
        var updatedAt = 0.0

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
        }

        guard updatedAt > 0 else {
            return nil
        }

        return AIRuntimeContextSnapshot(
            tool: "opencode",
            externalSessionID: latestSessionID,
            model: latestModel,
            inputTokens: inputTokens,
            outputTokens: outputTokens,
            totalTokens: totalTokens,
            updatedAt: updatedAt,
            responseState: nil
        )
    }

}

struct TerminalProcessInspector: Sendable {
    func activeTool(forShellPID shellPID: Int32) -> String? {
        let snapshot = processSnapshot()
        guard !snapshot.isEmpty else {
            return nil
        }

        let rowsByPID = Dictionary(uniqueKeysWithValues: snapshot.map { ($0.pid, $0) })
        var childrenByParent: [Int32: [ProcessInfoRow]] = [:]
        for row in snapshot {
            childrenByParent[row.ppid, default: []].append(row)
        }

        let candidateRoots = candidateShellRoots(startingAt: shellPID, rowsByPID: rowsByPID)

        for rootPID in candidateRoots {
            if let tool = deepestToolMatch(rootPID: rootPID, childrenByParent: childrenByParent) {
                return tool
            }
        }

        return nil
    }

    func hasActiveCommand(forShellPID shellPID: Int32) -> Bool {
        let snapshot = processSnapshot()
        guard !snapshot.isEmpty else {
            return false
        }

        let rowsByPID = Dictionary(uniqueKeysWithValues: snapshot.map { ($0.pid, $0) })
        var childrenByParent: [Int32: [ProcessInfoRow]] = [:]
        for row in snapshot {
            childrenByParent[row.ppid, default: []].append(row)
        }

        let candidateRoots = candidateShellRoots(startingAt: shellPID, rowsByPID: rowsByPID)
        for rootPID in candidateRoots {
            if containsNonShellDescendant(rootPID: rootPID, childrenByParent: childrenByParent) {
                return true
            }
        }
        return false
    }

    private func deepestToolMatch(rootPID: Int32, childrenByParent: [Int32: [ProcessInfoRow]]) -> String? {
        var stack = childrenByParent[rootPID] ?? []
        var matches: [(depth: Int, tool: String)] = []
        var depthByPID: [Int32: Int] = [rootPID: 0]

        while let row = stack.popLast() {
            let parentDepth = depthByPID[row.ppid] ?? 0
            let depth = parentDepth + 1
            depthByPID[row.pid] = depth

            if let tool = detectTool(in: row.command) {
                matches.append((depth, tool))
            }

            stack.append(contentsOf: childrenByParent[row.pid] ?? [])
        }

        return matches.sorted { lhs, rhs in
            if lhs.depth == rhs.depth {
                return lhs.tool < rhs.tool
            }
            return lhs.depth > rhs.depth
        }.first?.tool
    }

    private func containsNonShellDescendant(rootPID: Int32, childrenByParent: [Int32: [ProcessInfoRow]]) -> Bool {
        var stack = childrenByParent[rootPID] ?? []

        while let row = stack.popLast() {
            if isShellCommand(row.command) || isLoginCommand(row.command) {
                stack.append(contentsOf: childrenByParent[row.pid] ?? [])
                continue
            }
            return true
        }

        return false
    }

    private func candidateShellRoots(startingAt shellPID: Int32, rowsByPID: [Int32: ProcessInfoRow]) -> [Int32] {
        var roots: [Int32] = []
        var currentPID: Int32? = shellPID
        var visited = Set<Int32>()

        while let pid = currentPID, pid > 0, visited.insert(pid).inserted {
            guard let row = rowsByPID[pid] else {
                break
            }
            if isShellCommand(row.command) {
                roots.append(pid)
            }

            guard let parent = rowsByPID[row.ppid] else {
                break
            }
            if isShellCommand(parent.command) || isLoginCommand(parent.command) {
                currentPID = parent.pid
            } else {
                break
            }
        }

        if roots.isEmpty {
            roots.append(shellPID)
        }
        return roots
    }

    private func detectTool(in command: String) -> String? {
        let normalized = command.lowercased()
        if normalized.contains("tool-wrapper.sh")
            || normalized.contains("/scripts/wrappers/bin/")
            || normalized.contains("/scripts/wrappers/tool-wrapper.sh")
        {
            return nil
        }
        let tools = ["claude-code", "claude", "codex", "opencode", "gemini"]
        for tool in tools {
            if normalized.contains("/\(tool)")
                || normalized.contains(" \(tool)")
                || normalized.contains(" \"\(tool)\"")
                || normalized.contains(" '\(tool)'")
                || normalized.hasPrefix("\(tool) ")
                || normalized == tool
            {
                return tool
            }
        }
        return nil
    }

    private func isShellCommand(_ command: String) -> Bool {
        let normalized = command.lowercased()
        let shells = ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish", " -/bin/zsh", " -/bin/bash", " -/bin/sh"]
        return shells.contains(where: normalized.contains) || normalized.hasPrefix("-/bin/")
    }

    private func isLoginCommand(_ command: String) -> Bool {
        command.lowercased().contains("/usr/bin/login")
    }

    private func processSnapshot() -> [ProcessInfoRow] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = ["-wwaxo", "pid=,ppid=,command="]

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
        } catch {
            return []
        }

        let outputData = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0,
              let output = String(data: outputData, encoding: .utf8) else {
            return []
        }

        return output
            .split(separator: "\n")
            .compactMap { line in
                let trimmed = line.trimmingCharacters(in: .whitespaces)
                guard !trimmed.isEmpty else { return nil }
                let parts = trimmed.split(maxSplits: 2, whereSeparator: \.isWhitespace)
                guard parts.count == 3,
                      let pid = Int32(parts[0]),
                      let ppid = Int32(parts[1]) else {
                    return nil
                }
                return ProcessInfoRow(pid: pid, ppid: ppid, command: String(parts[2]))
            }
    }

    private struct ProcessInfoRow {
        var pid: Int32
        var ppid: Int32
        var command: String
    }
}
