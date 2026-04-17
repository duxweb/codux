import Foundation

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
    var origin: AIRuntimeSessionOrigin
}

actor GeminiRuntimeProbeService {
    private var sessionIDByRuntimeSessionID: [String: String] = [:]
    private let logger = AppDebugLog.shared

    func reset(runtimeSessionID: String) {
        sessionIDByRuntimeSessionID[runtimeSessionID] = nil
    }

    func snapshot(
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) -> AIRuntimeContextSnapshot? {
        let previousExternalSessionID = sessionIDByRuntimeSessionID[runtimeSessionID]
        let preferredSessionID = knownExternalSessionID
            ?? previousExternalSessionID
        let preferredSessionIsAuthoritative =
            knownExternalSessionID?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        guard let parsedState = parseGeminiSessionRuntimeState(
            projectPath: projectPath,
            startedAt: startedAt,
            preferredSessionID: preferredSessionID,
            preferredSessionIsAuthoritative: preferredSessionIsAuthoritative
        ) else {
            return nil
        }

        sessionIDByRuntimeSessionID[runtimeSessionID] = parsedState.externalSessionID
        if let previousExternalSessionID,
           previousExternalSessionID != parsedState.externalSessionID {
            logger.log(
                "gemini-runtime",
                "switch runtimeSession=\(runtimeSessionID) externalSession=\(previousExternalSessionID)->\(parsedState.externalSessionID) reason=probe-detected"
            )
        }
        logger.log(
            "gemini-runtime",
            "hit runtimeSession=\(runtimeSessionID) externalSession=\(parsedState.externalSessionID) model=\(parsedState.model ?? "nil") total=\(parsedState.totalTokens) response=\(parsedState.responseState?.rawValue ?? "nil") origin=\(parsedState.origin.rawValue)"
        )
        return AIRuntimeContextSnapshot(
            tool: "gemini",
            externalSessionID: parsedState.externalSessionID,
            model: parsedState.model,
            inputTokens: parsedState.inputTokens,
            outputTokens: parsedState.outputTokens,
            totalTokens: parsedState.totalTokens,
            updatedAt: parsedState.updatedAt,
            responseState: parsedState.responseState,
            sessionOrigin: parsedState.origin
        )
    }
}

func parseGeminiSessionRuntimeState(
    projectPath: String,
    startedAt: Double?,
    preferredSessionID: String?,
    preferredSessionIsAuthoritative: Bool
) -> GeminiParsedRuntimeState? {
    let fileURLs = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: projectPath)
    guard !fileURLs.isEmpty else {
        return nil
    }

    var preferredMatch: (state: GeminiParsedRuntimeState, isCurrentLaunch: Bool)?
    var currentLaunchMatch: GeminiParsedRuntimeState?
    var candidateMatch: GeminiParsedRuntimeState?

    for fileURL in fileURLs.prefix(16) {
        guard let state = parseGeminiSessionRuntimeState(fileURL: fileURL) else {
            continue
        }

        let isCurrentLaunch: Bool = {
            guard let startedAt else {
                return false
            }
            return state.startedAt >= startedAt
        }()
        if let preferredSessionID,
           state.externalSessionID == preferredSessionID {
            preferredMatch = (state, isCurrentLaunch)
        }

        if isCurrentLaunch {
            if currentLaunchMatch == nil || state.updatedAt > (currentLaunchMatch?.updatedAt ?? 0) {
                currentLaunchMatch = state
            }
            continue
        }

        if startedAt == nil, candidateMatch == nil {
            candidateMatch = state
        }

        if candidateMatch == nil || state.updatedAt > (candidateMatch?.updatedAt ?? 0) {
            candidateMatch = state
        }
    }

    if preferredSessionIsAuthoritative {
        guard let preferredMatch else {
            return nil
        }
        var state = preferredMatch.state
        state.origin = preferredMatch.isCurrentLaunch ? .fresh : .restored
        return state
    }
    if var currentLaunchMatch {
        currentLaunchMatch.origin = .fresh
        return currentLaunchMatch
    }
    if let preferredMatch {
        var state = preferredMatch.state
        state.origin = preferredMatch.isCurrentLaunch ? .fresh : .restored
        return state
    }
    if startedAt != nil {
        return nil
    }
    return candidateMatch
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
        let totalValue = (tokens["total"] as? NSNumber)?.intValue
        let inputValue = (tokens["input"] as? NSNumber)?.intValue ?? 0
        let outputValue = (tokens["output"] as? NSNumber)?.intValue ?? 0
        let cachedValue = (tokens["cached"] as? NSNumber)?.intValue ?? 0
        let thoughtsValue = (tokens["thoughts"] as? NSNumber)?.intValue ?? 0
        let toolValue = (tokens["tool"] as? NSNumber)?.intValue ?? 0
        let messageTotal = totalValue ?? (inputValue + outputValue + cachedValue + thoughtsValue + toolValue)
        totalTokens += max(0, messageTotal)
        outputTokens += max(0, outputValue)
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
        responseState: responseState,
        origin: .unknown
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
            if let nested = parseGeminiTitle(from: item["content"]),
               !nested.isEmpty {
                return nested
            }
        }
    }
    if let item = content as? [String: Any] {
        if let text = item["text"] as? String {
            return normalizeGeminiTitle(text)
        }
        return parseGeminiTitle(from: item["content"])
    }
    return nil
}

func normalizeGeminiTitle(_ value: String?) -> String? {
    guard let value else {
        return nil
    }
    let normalized = value
        .replacingOccurrences(of: "\n", with: " ")
        .trimmingCharacters(in: .whitespacesAndNewlines)
    guard !normalized.isEmpty else {
        return nil
    }
    return String(normalized.prefix(60))
}
