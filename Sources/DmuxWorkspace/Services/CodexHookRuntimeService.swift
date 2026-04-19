import Foundation
import SQLite3

struct CodexParsedRuntimeState {
    var model: String?
    var totalTokens: Int?
    var updatedAt: Double?
    var startedAt: Double?
    var completedAt: Double?
    var responseState: AIResponseState?
    var wasInterrupted: Bool
    var hasCompletedTurn: Bool
}

struct CodexHookStopResolution {
    var hasDefinitiveStop: Bool
    var shouldEmitIdle: Bool
    var effectiveWasInterrupted: Bool
    var effectiveHasCompletedTurn: Bool
}

struct CodexProbeResolution {
    var responseState: AIResponseState?
    var effectiveWasInterrupted: Bool
    var effectiveHasCompletedTurn: Bool
}

func resolveCodexHookStopResolution(
    parsedState: CodexParsedRuntimeState?,
    currentResponseState: AIResponseState?,
    shouldIgnoreDefinitiveStop: Bool
) -> CodexHookStopResolution {
    let stopLooksSettled = (parsedState?.responseState == .idle)
        && (parsedState?.wasInterrupted != true)
    let explicitDefinitiveStop = (parsedState?.wasInterrupted == true)
        || (parsedState?.hasCompletedTurn == true)
    let settledOnlyStop = stopLooksSettled && !explicitDefinitiveStop
    let shouldTreatSettledOnlyStopAsNonDefinitive =
        settledOnlyStop && currentResponseState == .responding
    let hasDefinitiveStop = explicitDefinitiveStop || (stopLooksSettled && !shouldTreatSettledOnlyStopAsNonDefinitive)
    let effectiveWasInterrupted = hasDefinitiveStop && !shouldIgnoreDefinitiveStop
        ? (parsedState?.wasInterrupted ?? false)
        : false
    let effectiveHasCompletedTurn = hasDefinitiveStop && !shouldIgnoreDefinitiveStop
        ? ((parsedState?.hasCompletedTurn ?? false) || (stopLooksSettled && !explicitDefinitiveStop))
        : false

    return CodexHookStopResolution(
        hasDefinitiveStop: hasDefinitiveStop,
        shouldEmitIdle: hasDefinitiveStop && !shouldIgnoreDefinitiveStop,
        effectiveWasInterrupted: effectiveWasInterrupted,
        effectiveHasCompletedTurn: effectiveHasCompletedTurn
    )
}

func resolveCodexProbeResolution(
    parsedState: CodexParsedRuntimeState?,
    shouldIgnoreDefinitiveStop: Bool,
    didReleaseDefinitiveStop: Bool
) -> CodexProbeResolution {
    let stopLooksSettled = (parsedState?.responseState == .idle)
        && (parsedState?.wasInterrupted != true)
    let explicitDefinitiveStop = (parsedState?.wasInterrupted == true)
        || (parsedState?.hasCompletedTurn == true)
    let hasDefinitiveStop =
        (explicitDefinitiveStop || stopLooksSettled)
        && !shouldIgnoreDefinitiveStop
        && didReleaseDefinitiveStop

    return CodexProbeResolution(
        responseState: hasDefinitiveStop ? .idle : nil,
        effectiveWasInterrupted: hasDefinitiveStop ? (parsedState?.wasInterrupted ?? false) : false,
        effectiveHasCompletedTurn: hasDefinitiveStop
            ? ((parsedState?.hasCompletedTurn ?? false) || stopLooksSettled)
            : false
    )
}

func resolveCodexDefinitiveStopReferenceUpdatedAt(
    parsedState: CodexParsedRuntimeState?,
    fallbackUpdatedAt: Double
) -> Double {
    if let completedAt = parsedState?.completedAt {
        return completedAt
    }
    return parsedState?.updatedAt ?? fallbackUpdatedAt
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
            await AIToolRuntimeResponseLatch.shared.reset(tool: "codex", runtimeSessionID: runtimeSessionID)
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
        let hasCandidateDefinitiveStop = (parsedState?.hasCompletedTurn == true)
            || (parsedState?.wasInterrupted == true)
            || stopLooksSettled
        let semanticUpdatedAt = max(updatedAt, parsedState?.updatedAt ?? 0)
        let definitiveStopReferenceUpdatedAt = resolveCodexDefinitiveStopReferenceUpdatedAt(
            parsedState: parsedState,
            fallbackUpdatedAt: semanticUpdatedAt
        )
        let shouldIgnoreDefinitiveStop: Bool
        if hasCandidateDefinitiveStop {
            shouldIgnoreDefinitiveStop = await AIToolRuntimeResponseLatch.shared.shouldIgnoreDefinitiveStop(
                tool: "codex",
                runtimeSessionID: runtimeSessionID,
                externalSessionID: threadID,
                stopUpdatedAt: definitiveStopReferenceUpdatedAt
            )
        } else {
            shouldIgnoreDefinitiveStop = false
        }
        let didReleaseDefinitiveStop = await AIToolRuntimeResponseLatch.shared.releaseIfDefinitiveStop(
            tool: "codex",
            runtimeSessionID: runtimeSessionID,
            externalSessionID: threadID,
            stopUpdatedAt: definitiveStopReferenceUpdatedAt,
            wasInterrupted: hasCandidateDefinitiveStop && !shouldIgnoreDefinitiveStop
                ? (parsedState?.wasInterrupted ?? false)
                : false,
            hasCompletedTurn: hasCandidateDefinitiveStop && !shouldIgnoreDefinitiveStop
                ? ((parsedState?.hasCompletedTurn ?? false) || stopLooksSettled)
                : false
        )
        let resolution = resolveCodexProbeResolution(
            parsedState: parsedState,
            shouldIgnoreDefinitiveStop: shouldIgnoreDefinitiveStop,
            didReleaseDefinitiveStop: didReleaseDefinitiveStop
        )
        let shouldForceResponding = resolution.responseState == .idle ? false : await AIToolRuntimeResponseLatch.shared.shouldForceResponding(
            tool: "codex",
            runtimeSessionID: runtimeSessionID,
            externalSessionID: threadID,
            snapshotUpdatedAt: semanticUpdatedAt,
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
            if resolution.responseState == .idle {
                return .idle
            }
            if parsedState?.responseState == .idle {
                logger.log(
                    "codex-runtime",
                    "reject idle runtimeSession=\(runtimeSessionID) external=\(threadID) reason=\(shouldIgnoreDefinitiveStop ? "stale-definitive-stop" : "non-definitive-probe-idle") updatedAt=\(semanticUpdatedAt) total=\(parsedState?.totalTokens ?? totalTokens)"
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
            updatedAt: semanticUpdatedAt,
            responseState: probeResponseState,
            wasInterrupted: resolution.effectiveWasInterrupted,
            hasCompletedTurn: resolution.effectiveHasCompletedTurn
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

        let rowType = row["type"] as? String
        let payload = row["payload"] as? [String: Any] ?? [:]
        if rowType == "turn_context",
           let model = payload["model"] as? String,
           !model.isEmpty,
           projectPath == nil || (payload["cwd"] as? String) == projectPath {
            latestModel = model
            continue
        }

        let marksAssistantFinalAnswer: Bool = {
            if rowType == "event_msg",
               payload["type"] as? String == "agent_message",
               payload["phase"] as? String == "final_answer" {
                return true
            }
            if rowType == "response_item",
               payload["type"] as? String == "message",
               payload["phase"] as? String == "final_answer" {
                return true
            }
            return false
        }()

        if marksAssistantFinalAnswer {
            let completedAt = timestamp ?? latestUpdatedAt
            if let completedAt,
               latestCompletedAt == nil || completedAt >= (latestCompletedAt ?? 0) {
                latestCompletedAt = completedAt
                latestTurnWasInterrupted = false
                latestTurnCompleted = true
            }
            continue
        }

        guard rowType == "event_msg",
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
        startedAt: latestStartedAt,
        completedAt: latestCompletedAt,
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
