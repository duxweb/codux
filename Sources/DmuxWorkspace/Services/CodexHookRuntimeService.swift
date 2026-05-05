import Foundation

struct CodexParsedRuntimeState {
    var model: String?
    var inputTokens: Int?
    var outputTokens: Int?
    var cachedInputTokens: Int?
    var totalTokens: Int?
    var origin: AIRuntimeSessionOrigin
    var updatedAt: Double?
    var startedAt: Double?
    var completedAt: Double?
    var responseState: AIResponseState?
    var wasInterrupted: Bool
    var hasCompletedTurn: Bool
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

private func parseCodexRuntimeState(fileURL: URL?, projectPath: String?) -> CodexParsedRuntimeState? {
    guard let fileURL,
          FileManager.default.fileExists(atPath: fileURL.path) else {
        return nil
    }

    let lines = JSONLLineReader.tailLines(in: fileURL)
    guard !lines.isEmpty else {
        return nil
    }

    var latestModel: String?
    var latestUpdatedAt: Double?
    var latestStartedAt: Double?
    var latestCompletedAt: Double?
    var inputTokens: Int?
    var outputTokens: Int?
    var cachedInputTokens: Int?
    var totalTokens: Int?
    var latestCumulativeUsage: CodexUsageTotals?
    var cumulativeUsageAtTurnStart: CodexUsageTotals?
    var latestResolvedUsage: CodexUsageTotals?
    var latestTurnWasInterrupted = false
    var latestTurnCompleted = false

    for line in lines {
        guard let row = try? JSONSerialization.jsonObject(with: line) as? [String: Any] else {
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
            cumulativeUsageAtTurnStart = latestCumulativeUsage
            latestResolvedUsage = latestCumulativeUsage
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
            let lastUsage = parseCodexUsageTotals(info["last_token_usage"] as? [String: Any] ?? [:])
            let parsedTotalUsage = parseCodexUsageTotals(totalUsage)
            if let parsedTotalUsage {
                latestCumulativeUsage = parsedTotalUsage
            }
            if let resolvedUsage = resolveCodexRuntimeUsage(
                totalUsage: parsedTotalUsage,
                baseUsage: cumulativeUsageAtTurnStart ?? latestCumulativeUsage,
                lastUsage: lastUsage
            ) {
                latestResolvedUsage = resolvedUsage
                inputTokens = resolvedUsage.inputTokens
                outputTokens = resolvedUsage.outputTokens
                cachedInputTokens = resolvedUsage.cachedInputTokens
                totalTokens = resolvedUsage.totalTokens
            }
        default:
            continue
        }
    }

    let responseState: AIResponseState? = {
        if let latestStartedAt, let latestCompletedAt, latestCompletedAt >= latestStartedAt {
            return .idle
        }
        if latestStartedAt == nil, latestCompletedAt != nil {
            return .idle
        }
        guard latestStartedAt != nil else {
            return nil
        }
        return .responding
    }()

    let finalUsage: CodexUsageTotals? = {
        switch responseState {
        case .idle:
            return latestCumulativeUsage ?? latestResolvedUsage
        case .responding:
            return latestResolvedUsage ?? latestCumulativeUsage
        case nil:
            return latestCumulativeUsage ?? latestResolvedUsage
        }
    }()

    let origin: AIRuntimeSessionOrigin = {
        guard responseState == .responding else {
            return .unknown
        }
        let baseUsage = cumulativeUsageAtTurnStart ?? latestCumulativeUsage
        let historicalTotal = (baseUsage?.totalTokens ?? 0) + (baseUsage?.cachedInputTokens ?? 0)
        return historicalTotal > 0 ? .restored : .fresh
    }()

    return CodexParsedRuntimeState(
        model: latestModel,
        inputTokens: finalUsage?.inputTokens ?? inputTokens,
        outputTokens: finalUsage?.outputTokens ?? outputTokens,
        cachedInputTokens: finalUsage?.cachedInputTokens ?? cachedInputTokens,
        totalTokens: finalUsage?.totalTokens ?? totalTokens,
        origin: origin,
        updatedAt: latestUpdatedAt,
        startedAt: latestStartedAt,
        completedAt: latestCompletedAt,
        responseState: responseState,
        wasInterrupted: latestTurnWasInterrupted,
        hasCompletedTurn: latestTurnCompleted
    )
}

private struct CodexUsageTotals {
    var inputTokens: Int
    var outputTokens: Int
    var cachedInputTokens: Int
    var totalTokens: Int

    static func + (lhs: CodexUsageTotals, rhs: CodexUsageTotals) -> CodexUsageTotals {
        CodexUsageTotals(
            inputTokens: lhs.inputTokens + rhs.inputTokens,
            outputTokens: lhs.outputTokens + rhs.outputTokens,
            cachedInputTokens: lhs.cachedInputTokens + rhs.cachedInputTokens,
            totalTokens: lhs.totalTokens + rhs.totalTokens
        )
    }
}

private func resolveCodexRuntimeUsage(
    totalUsage: CodexUsageTotals?,
    baseUsage: CodexUsageTotals?,
    lastUsage: CodexUsageTotals?
) -> CodexUsageTotals? {
    guard totalUsage != nil || lastUsage != nil else {
        return nil
    }

    guard let lastUsage else {
        return totalUsage
    }

    let baseUsage = baseUsage ?? CodexUsageTotals(
        inputTokens: 0,
        outputTokens: 0,
        cachedInputTokens: 0,
        totalTokens: 0
    )

    if let totalUsage {
        let totalWithCache = totalUsage.totalTokens + totalUsage.cachedInputTokens
        let baseWithCache = baseUsage.totalTokens + baseUsage.cachedInputTokens
        if totalWithCache > baseWithCache {
            return totalUsage
        }
        if totalWithCache == baseWithCache {
            let lastWithCache = lastUsage.totalTokens + lastUsage.cachedInputTokens
            if lastWithCache == totalWithCache {
                return totalUsage
            }
        }
    }

    return baseUsage + lastUsage
}

private func parseCodexUsageTotals(_ usage: [String: Any]) -> CodexUsageTotals? {
    if usage.isEmpty {
        return nil
    }
    let rawInputTokens = (usage["input_tokens"] as? NSNumber)?.intValue ?? 0
    let rawOutputTokens = (usage["output_tokens"] as? NSNumber)?.intValue ?? 0
    let cachedInputTokens = (usage["cached_input_tokens"] as? NSNumber)?.intValue
        ?? (usage["cache_read_input_tokens"] as? NSNumber)?.intValue
        ?? 0
    let reasoningOutputTokens = (usage["reasoning_output_tokens"] as? NSNumber)?.intValue ?? 0
    if rawInputTokens == 0,
       rawOutputTokens == 0,
       let rawTotalTokens = (usage["total_tokens"] as? NSNumber)?.intValue,
       rawTotalTokens > 0 {
        return CodexUsageTotals(
            inputTokens: rawTotalTokens,
            outputTokens: 0,
            cachedInputTokens: cachedInputTokens,
            totalTokens: rawTotalTokens
        )
    }
    let inputTokens = max(0, rawInputTokens - cachedInputTokens)
    let outputTokens = max(0, rawOutputTokens - reasoningOutputTokens)
    let totalTokens = inputTokens + outputTokens + reasoningOutputTokens
    guard inputTokens > 0 || outputTokens > 0 || cachedInputTokens > 0 || totalTokens > 0 else {
        return nil
    }
    return CodexUsageTotals(
        inputTokens: inputTokens,
        outputTokens: outputTokens,
        cachedInputTokens: cachedInputTokens,
        totalTokens: totalTokens
    )
}
