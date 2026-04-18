import Foundation
import SQLite3

struct CodexParsedRuntimeState {
    var model: String?
    var totalTokens: Int?
    var updatedAt: Double?
    var responseState: AIResponseState?
    var wasInterrupted: Bool
    var hasCompletedTurn: Bool
}

actor CodexRuntimeResponseLatch {
    static let shared = CodexRuntimeResponseLatch()

    private struct LatchState {
        var lastRespondingAt: Double
        var externalSessionID: String?
        var awaitingDefinitiveStop: Bool
    }

    private var stateByRuntimeSessionID: [String: LatchState] = [:]
    private var runtimeSessionIDsByExternalSessionID: [String: Set<String>] = [:]

    func reset(runtimeSessionID: String) {
        guard let existing = stateByRuntimeSessionID.removeValue(forKey: runtimeSessionID) else {
            return
        }
        guard let externalSessionID = existing.externalSessionID else {
            return
        }
        runtimeSessionIDsByExternalSessionID[externalSessionID]?.remove(runtimeSessionID)
        if runtimeSessionIDsByExternalSessionID[externalSessionID]?.isEmpty == true {
            runtimeSessionIDsByExternalSessionID[externalSessionID] = nil
        }
    }

    func markResponding(runtimeSessionID: String, externalSessionID: String?, updatedAt: Double) {
        let normalizedExternalSessionID = normalizedSessionID(externalSessionID)
        let previousExternalSessionID = stateByRuntimeSessionID[runtimeSessionID]?.externalSessionID
        if let previousExternalSessionID, previousExternalSessionID != normalizedExternalSessionID {
            runtimeSessionIDsByExternalSessionID[previousExternalSessionID]?.remove(runtimeSessionID)
            if runtimeSessionIDsByExternalSessionID[previousExternalSessionID]?.isEmpty == true {
                runtimeSessionIDsByExternalSessionID[previousExternalSessionID] = nil
            }
        }

        stateByRuntimeSessionID[runtimeSessionID] = LatchState(
            lastRespondingAt: updatedAt,
            externalSessionID: normalizedExternalSessionID,
            awaitingDefinitiveStop: true
        )
        if let normalizedExternalSessionID {
            runtimeSessionIDsByExternalSessionID[normalizedExternalSessionID, default: []].insert(runtimeSessionID)
        }
    }

    func releaseIfDefinitiveStop(
        runtimeSessionID: String,
        externalSessionID: String?,
        wasInterrupted: Bool,
        hasCompletedTurn: Bool
    ) {
        guard wasInterrupted || hasCompletedTurn else {
            return
        }
        release(runtimeSessionID: runtimeSessionID, externalSessionID: externalSessionID)
    }

    func releaseIfSettled(runtimeSessionID: String, externalSessionID: String?) {
        release(runtimeSessionID: runtimeSessionID, externalSessionID: externalSessionID)
    }

    private func release(runtimeSessionID: String, externalSessionID: String?) {
        reset(runtimeSessionID: runtimeSessionID)
        if let normalizedExternalSessionID = normalizedSessionID(externalSessionID),
           let runtimeSessionIDs = runtimeSessionIDsByExternalSessionID[normalizedExternalSessionID] {
            for id in runtimeSessionIDs {
                stateByRuntimeSessionID[id] = nil
            }
            runtimeSessionIDsByExternalSessionID[normalizedExternalSessionID] = nil
        }
    }

    func shouldForceResponding(
        runtimeSessionID: String,
        externalSessionID: String?,
        snapshotUpdatedAt: Double,
        wasInterrupted: Bool
    ) -> Bool {
        guard wasInterrupted == false else {
            return false
        }

        if let existing = stateByRuntimeSessionID[runtimeSessionID] {
            return existing.awaitingDefinitiveStop
        }

        if let normalizedExternalSessionID = normalizedSessionID(externalSessionID),
           let runtimeSessionIDs = runtimeSessionIDsByExternalSessionID[normalizedExternalSessionID] {
            for id in runtimeSessionIDs {
                if let existing = stateByRuntimeSessionID[id],
                   existing.awaitingDefinitiveStop {
                    stateByRuntimeSessionID[runtimeSessionID] = LatchState(
                        lastRespondingAt: existing.lastRespondingAt,
                        externalSessionID: normalizedExternalSessionID,
                        awaitingDefinitiveStop: true
                    )
                    runtimeSessionIDsByExternalSessionID[normalizedExternalSessionID, default: []].insert(runtimeSessionID)
                    return true
                }
            }
        }

        return false
    }
}

struct CodexHookRuntimeEnvelope: Decodable, Sendable {
    var event: String
    var tool: String
    var dmuxSessionId: String
    var dmuxProjectId: String
    var dmuxProjectPath: String?
    var receivedAt: Double
    var payload: String
}

func parseCodexRolloutRuntimeState(fileURL: URL?) -> CodexParsedRuntimeState? {
    parseCodexRuntimeState(fileURL: fileURL, projectPath: nil)
}

func parseCodexSessionRuntimeState(fileURL: URL?, projectPath: String) -> CodexParsedRuntimeState? {
    parseCodexRuntimeState(fileURL: fileURL, projectPath: projectPath)
}

func resolveCodexStopRuntimeState(transcriptPath: String?) async -> CodexParsedRuntimeState? {
    guard let transcriptPath else {
        return nil
    }

    let fileURL = URL(fileURLWithPath: transcriptPath)
    let retryDelays: [UInt64] = [0, 120_000_000, 280_000_000, 500_000_000, 900_000_000, 1_500_000_000]
    var latestState: CodexParsedRuntimeState?

    for delay in retryDelays {
        if delay > 0 {
            try? await Task.sleep(nanoseconds: delay)
        }
        latestState = parseCodexRolloutRuntimeState(fileURL: fileURL)
        guard let latestState else {
            continue
        }
        if latestState.wasInterrupted || latestState.hasCompletedTurn || latestState.responseState == .idle {
            return latestState
        }
    }

    return latestState
}

actor CodexRuntimeProbeService {
    private var threadIDByRuntimeSessionID: [String: String] = [:]
    private let logger = AppDebugLog.shared

    func reset(runtimeSessionID: String) {
        threadIDByRuntimeSessionID[runtimeSessionID] = nil
        Task {
            await CodexRuntimeResponseLatch.shared.reset(runtimeSessionID: runtimeSessionID)
        }
    }

    func snapshot(
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) async -> AIRuntimeContextSnapshot? {
        _ = startedAt
        let dbURL = AIRuntimeSourceLocator.codexDatabaseURL()
        guard FileManager.default.fileExists(atPath: dbURL.path) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(dbURL.path, &db) == SQLITE_OK, let db else {
            return nil
        }
        defer { sqlite3_close(db) }

        if let knownExternalSessionID, !knownExternalSessionID.isEmpty {
            threadIDByRuntimeSessionID[runtimeSessionID] = knownExternalSessionID
            if let snapshot = await threadSnapshot(
                db: db,
                runtimeSessionID: runtimeSessionID,
                threadID: knownExternalSessionID
            ) {
                return snapshot
            }
            threadIDByRuntimeSessionID[runtimeSessionID] = nil
        }

        if let threadID = threadIDByRuntimeSessionID[runtimeSessionID],
           let snapshot = await threadSnapshot(
                db: db,
                runtimeSessionID: runtimeSessionID,
                threadID: threadID
           ) {
            return snapshot
        }
        logger.log(
            "codex-runtime",
            "miss runtimeSession=\(runtimeSessionID) projectPath=\(projectPath) reason=no-thread-binding"
        )
        return nil
    }

    private func threadSnapshot(
        db: OpaquePointer,
        runtimeSessionID: String,
        threadID: String
    ) async -> AIRuntimeContextSnapshot? {
        let sql = """
        SELECT model, tokens_used, updated_at, rollout_path
        FROM threads
        WHERE id = ?
        LIMIT 1;
        """

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, threadID, -1, SQLITE_TRANSIENT_CODEX_RUNTIME)

        guard sqlite3_step(statement) == SQLITE_ROW else {
            return nil
        }

        let model = sqlite3_column_type(statement, 0) == SQLITE_NULL ? nil : String(cString: sqlite3_column_text(statement, 0))
        let totalTokens = Int(sqlite3_column_int64(statement, 1))
        let updatedAt = sqlite3_column_double(statement, 2)
        let rolloutPath = sqlite3_column_type(statement, 3) == SQLITE_NULL ? nil : String(cString: sqlite3_column_text(statement, 3))
        let parsedState = parseCodexRolloutRuntimeState(fileURL: rolloutPath.map(URL.init(fileURLWithPath:)))

        let stopLooksSettled = (parsedState?.responseState == .idle)
            && (parsedState?.wasInterrupted != true)
        let hasDefinitiveStop = (parsedState?.hasCompletedTurn == true)
            || (parsedState?.wasInterrupted == true)
            || stopLooksSettled
        if hasDefinitiveStop {
            await CodexRuntimeResponseLatch.shared.releaseIfSettled(
                runtimeSessionID: runtimeSessionID,
                externalSessionID: threadID
            )
        }
        let shouldForceResponding = hasDefinitiveStop ? false : await CodexRuntimeResponseLatch.shared.shouldForceResponding(
            runtimeSessionID: runtimeSessionID,
            externalSessionID: threadID,
            snapshotUpdatedAt: max(updatedAt, parsedState?.updatedAt ?? 0),
            wasInterrupted: parsedState?.wasInterrupted ?? false
        )
        let probeResponseState: AIResponseState? = {
            if shouldForceResponding {
                logger.log(
                    "codex-runtime",
                    "suppress phase runtimeSession=\(runtimeSessionID) external=\(threadID) reason=hook-responding-latch"
                )
                return nil
            }
            if hasDefinitiveStop {
                return .idle
            }
            if parsedState?.responseState == .idle {
                logger.log(
                    "codex-runtime",
                    "reject idle runtimeSession=\(runtimeSessionID) external=\(threadID) reason=non-definitive-probe-idle updatedAt=\(max(updatedAt, parsedState?.updatedAt ?? 0)) total=\(parsedState?.totalTokens ?? totalTokens)"
                )
            }
            return nil
        }()

        return AIRuntimeContextSnapshot(
            tool: "codex",
            externalSessionID: threadID,
            model: parsedState?.model ?? model,
            inputTokens: parsedState?.totalTokens ?? totalTokens,
            outputTokens: 0,
            totalTokens: parsedState?.totalTokens ?? totalTokens,
            updatedAt: max(updatedAt, parsedState?.updatedAt ?? 0),
            responseState: probeResponseState,
            wasInterrupted: parsedState?.wasInterrupted ?? false,
            hasCompletedTurn: probeResponseState == nil ? false : ((parsedState?.hasCompletedTurn ?? false) || stopLooksSettled)
        )
    }
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
    var latestTurnWasInterrupted = false
    var latestTurnCompleted = false

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
            latestTurnWasInterrupted = false
            latestTurnCompleted = false
        case "task_complete":
            let completedAt = (payload["completed_at"] as? NSNumber)?.doubleValue ?? timestamp
            if let completedAt,
               latestCompletedAt == nil || completedAt >= (latestCompletedAt ?? 0) {
                latestCompletedAt = completedAt
                latestTurnWasInterrupted = false
                latestTurnCompleted = true
            }
        case "turn_aborted":
            let completedAt = (payload["completed_at"] as? NSNumber)?.doubleValue ?? timestamp
            if let completedAt,
               latestCompletedAt == nil || completedAt >= (latestCompletedAt ?? 0) {
                latestCompletedAt = completedAt
                latestTurnWasInterrupted = true
                latestTurnCompleted = false
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
        responseState: responseState,
        wasInterrupted: latestTurnWasInterrupted,
        hasCompletedTurn: latestTurnCompleted
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

private let SQLITE_TRANSIENT_CODEX_RUNTIME = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

private func normalizedSessionID(_ value: String?) -> String? {
    guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
          !value.isEmpty else {
        return nil
    }
    return value
}
