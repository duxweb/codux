import Foundation

actor ClaudeRuntimeInterruptWatchCache {
    struct FileState {
        var offset: UInt64
        var lastInterruptAtBySessionID: [String: Double]
    }

    struct InterruptEvent {
        var externalSessionID: String
        var updatedAt: Double
    }

    static let shared = ClaudeRuntimeInterruptWatchCache()

    private var fileStatesByPath: [String: FileState] = [:]

    func prime(fileURL: URL, externalSessionID: String?) {
        let path = fileURL.path
        let fileSize = currentFileSize(for: fileURL)
        var state = fileStatesByPath[path] ?? FileState(offset: 0, lastInterruptAtBySessionID: [:])
        state.offset = fileSize
        _ = externalSessionID
        fileStatesByPath[path] = state
    }

    func process(fileURL: URL, projectPath: String?) -> [InterruptEvent] {
        let path = fileURL.path
        let fileSize = currentFileSize(for: fileURL)
        guard fileSize > 0 else {
            fileStatesByPath[path] = FileState(offset: 0, lastInterruptAtBySessionID: [:])
            return []
        }

        guard let existing = fileStatesByPath[path] else {
            fileStatesByPath[path] = FileState(offset: fileSize, lastInterruptAtBySessionID: [:])
            return []
        }

        if fileSize < existing.offset {
            fileStatesByPath[path] = FileState(offset: fileSize, lastInterruptAtBySessionID: existing.lastInterruptAtBySessionID)
            return []
        }
        if fileSize == existing.offset {
            return []
        }

        let deltaEvents = parseInterruptEvents(
            in: fileURL,
            projectPath: projectPath,
            startingAt: existing.offset,
            lastInterruptAtBySessionID: existing.lastInterruptAtBySessionID
        )
        var nextState = existing
        nextState.offset = fileSize
        for event in deltaEvents {
            nextState.lastInterruptAtBySessionID[event.externalSessionID] = event.updatedAt
        }
        fileStatesByPath[path] = nextState
        return deltaEvents
    }

    private func currentFileSize(for fileURL: URL) -> UInt64 {
        let size = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
        return UInt64(max(0, size))
    }

    private func parseInterruptEvents(
        in fileURL: URL,
        projectPath: String?,
        startingAt offset: UInt64,
        lastInterruptAtBySessionID: [String: Double]
    ) -> [InterruptEvent] {
        var events: [InterruptEvent] = []
        JSONLLineReader.forEachLine(in: fileURL, startingAt: offset) { lineData in
            guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any],
                  let cwd = row["cwd"] as? String,
                  projectPath == nil || pathsEquivalent(cwd, projectPath),
                  let sessionID = row["sessionId"] as? String,
                  isClaudeInterruptedRow(row) else {
                return true
            }

            let timestamp = (row["timestamp"] as? String).flatMap(parseClaudeISO8601Date)?.timeIntervalSince1970 ?? Date().timeIntervalSince1970
            if let lastInterruptAt = lastInterruptAtBySessionID[sessionID],
               timestamp <= lastInterruptAt {
                return true
            }
            events.append(
                InterruptEvent(
                    externalSessionID: sessionID,
                    updatedAt: timestamp
                )
            )
            return true
        }
        return events
    }
}

actor ClaudeRuntimeLogCache {
    struct CountedUsageKey: Hashable {
        var sessionID: String
        var messageID: String
    }

    struct UsageTotals: Equatable {
        var inputTokens: Int
        var outputTokens: Int
        var cachedInputTokens: Int
        var totalTokens: Int

        static let zero = UsageTotals(inputTokens: 0, outputTokens: 0, cachedInputTokens: 0, totalTokens: 0)

        func delta(from previous: UsageTotals) -> UsageTotals {
            UsageTotals(
                inputTokens: max(0, inputTokens - previous.inputTokens),
                outputTokens: max(0, outputTokens - previous.outputTokens),
                cachedInputTokens: max(0, cachedInputTokens - previous.cachedInputTokens),
                totalTokens: max(0, totalTokens - previous.totalTokens)
            )
        }

        var isZero: Bool {
            inputTokens == 0 && outputTokens == 0 && cachedInputTokens == 0 && totalTokens == 0
        }
    }

    struct SessionAggregate {
        var model: String?
        var inputTokens: Int
        var outputTokens: Int
        var cachedInputTokens: Int
        var totalTokens: Int
        var updatedAt: Double
        var lastUserAt: Double
        var lastCompletionAt: Double
        var lastInterruptedAt: Double
        var lastCompletedTurnAt: Double
    }

    struct FileState {
        var offset: UInt64
        var sessions: [String: SessionAggregate]
        var usageTotalsByKey: [CountedUsageKey: UsageTotals]
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
            cachedInputTokens: session.cachedInputTokens,
            totalTokens: session.totalTokens,
            updatedAt: session.updatedAt,
            startedAt: startedAt(for: session),
            completedAt: completedAt(for: session),
            responseState: responseState(for: session),
            wasInterrupted: wasInterrupted(for: session),
            hasCompletedTurn: hasCompletedTurn(for: session)
        )
    }

    private func updateAndMergeSessions(projectPath: String) -> [String: SessionAggregate] {
        let fileURLs = AIRuntimeSourceLocator.claudeProjectLogURLs(projectPath: projectPath)
        guard !fileURLs.isEmpty else {
            fileStatesByProjectPath[projectPath] = [:]
            return [:]
        }

        var fileStates = fileStatesByProjectPath[projectPath] ?? [:]
        let visiblePaths = Set(fileURLs.map { $0.path })
        fileStates = fileStates.filter { visiblePaths.contains($0.key) }

        for fileURL in fileURLs {
            let path = fileURL.path
            let fileSize = currentFileSize(for: fileURL)
            let existing = fileStates[path]

            if existing == nil || fileSize < (existing?.offset ?? 0) {
                let parsed = parseSessions(
                    in: fileURL,
                    projectPath: projectPath,
                    startingAt: 0,
                    existingUsageTotalsByKey: [:]
                )
                fileStates[path] = FileState(
                    offset: fileSize,
                    sessions: parsed.sessions,
                    usageTotalsByKey: parsed.usageTotalsByKey
                )
                continue
            }

            guard let existing else {
                continue
            }

            if fileSize == existing.offset {
                continue
            }

            let parsed = parseSessions(
                in: fileURL,
                projectPath: projectPath,
                startingAt: existing.offset,
                existingUsageTotalsByKey: existing.usageTotalsByKey
            )
            var mergedSessions = existing.sessions
            for (sessionID, delta) in parsed.sessions {
                let aggregate = mergeSessionAggregate(
                    mergedSessions[sessionID],
                    with: delta
                )
                mergedSessions[sessionID] = aggregate
            }
            fileStates[path] = FileState(
                offset: fileSize,
                sessions: mergedSessions,
                usageTotalsByKey: parsed.usageTotalsByKey
            )
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

    private func wasInterrupted(for aggregate: SessionAggregate) -> Bool {
        guard aggregate.lastInterruptedAt > 0 else {
            return false
        }
        let latestConflictingAt = max(aggregate.lastUserAt, aggregate.lastCompletedTurnAt)
        return aggregate.lastInterruptedAt >= latestConflictingAt
    }

    private func hasCompletedTurn(for aggregate: SessionAggregate) -> Bool {
        guard aggregate.lastCompletedTurnAt > 0 else {
            return false
        }
        let latestConflictingAt = max(aggregate.lastUserAt, aggregate.lastInterruptedAt)
        return aggregate.lastCompletedTurnAt >= latestConflictingAt
    }

    private func startedAt(for aggregate: SessionAggregate) -> Double? {
        aggregate.lastUserAt > 0 ? aggregate.lastUserAt : nil
    }

    private func completedAt(for aggregate: SessionAggregate) -> Double? {
        let completion = max(aggregate.lastCompletedTurnAt, aggregate.lastInterruptedAt)
        return completion > 0 ? completion : nil
    }

    private func currentFileSize(for fileURL: URL) -> UInt64 {
        let size = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
        return UInt64(max(0, size))
    }

    private func parseSessions(
        in fileURL: URL,
        projectPath: String,
        startingAt offset: UInt64,
        existingUsageTotalsByKey: [CountedUsageKey: UsageTotals]
    ) -> (sessions: [String: SessionAggregate], usageTotalsByKey: [CountedUsageKey: UsageTotals]) {
        var sessions: [String: SessionAggregate] = [:]
        var usageTotalsByKey = existingUsageTotalsByKey
        JSONLLineReader.forEachLine(in: fileURL, startingAt: offset) { lineData in
            guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any],
                  let cwd = row["cwd"] as? String,
                  let sessionID = row["sessionId"] as? String,
                  pathsEquivalent(cwd, projectPath) else {
                return true
            }

            let timestamp = (row["timestamp"] as? String).flatMap(parseClaudeISO8601Date)?.timeIntervalSince1970 ?? 0
            let message = row["message"] as? [String: Any] ?? [:]

            var aggregate = sessions[sessionID] ?? SessionAggregate(
                model: nil,
                inputTokens: 0,
                outputTokens: 0,
                cachedInputTokens: 0,
                totalTokens: 0,
                updatedAt: timestamp,
                lastUserAt: 0,
                lastCompletionAt: 0,
                lastInterruptedAt: 0,
                lastCompletedTurnAt: 0
            )
            let rowType = row["type"] as? String
            if rowType == "user" {
                if isClaudeInterruptedRow(row) {
                    aggregate.lastInterruptedAt = max(aggregate.lastInterruptedAt, timestamp)
                    aggregate.lastCompletionAt = max(aggregate.lastCompletionAt, timestamp)
                } else {
                    aggregate.lastUserAt = max(aggregate.lastUserAt, timestamp)
                }
            } else if rowType == "assistant" {
                let stopReason = message["stop_reason"] as? String
                if stopReason == "end_turn" {
                    aggregate.lastCompletedTurnAt = max(aggregate.lastCompletedTurnAt, timestamp)
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
            if let usageKey = usageKey(for: row, message: message, sessionID: sessionID) {
                let usage = message["usage"] as? [String: Any] ?? [:]
                let nextUsageTotals = usageTotals(from: usage)
                let previousUsageTotals = usageTotalsByKey[usageKey] ?? .zero
                let usageDelta = nextUsageTotals.delta(from: previousUsageTotals)
                usageTotalsByKey[usageKey] = UsageTotals(
                    inputTokens: max(previousUsageTotals.inputTokens, nextUsageTotals.inputTokens),
                    outputTokens: max(previousUsageTotals.outputTokens, nextUsageTotals.outputTokens),
                    cachedInputTokens: max(previousUsageTotals.cachedInputTokens, nextUsageTotals.cachedInputTokens),
                    totalTokens: max(previousUsageTotals.totalTokens, nextUsageTotals.totalTokens)
                )

                if usageDelta.isZero == false {
                    aggregate.inputTokens += usageDelta.inputTokens
                    aggregate.outputTokens += usageDelta.outputTokens
                    aggregate.cachedInputTokens += usageDelta.cachedInputTokens
                    aggregate.totalTokens += usageDelta.totalTokens
                }
            }
            aggregate.updatedAt = max(aggregate.updatedAt, timestamp)
            sessions[sessionID] = aggregate
            return true
        }
        return (sessions, usageTotalsByKey)
    }

    private func usageKey(
        for row: [String: Any],
        message: [String: Any],
        sessionID: String
    ) -> CountedUsageKey? {
        if let messageID = message["id"] as? String, !messageID.isEmpty {
            return CountedUsageKey(sessionID: sessionID, messageID: messageID)
        }
        if let rowUUID = row["uuid"] as? String, !rowUUID.isEmpty {
            return CountedUsageKey(sessionID: sessionID, messageID: rowUUID)
        }
        return nil
    }

    private func mergeSessionAggregate(
        _ existing: SessionAggregate?,
        with contribution: SessionAggregate
    ) -> SessionAggregate {
        var aggregate = existing ?? SessionAggregate(
            model: nil,
            inputTokens: 0,
            outputTokens: 0,
            cachedInputTokens: 0,
            totalTokens: 0,
            updatedAt: contribution.updatedAt,
            lastUserAt: contribution.lastUserAt,
            lastCompletionAt: contribution.lastCompletionAt,
            lastInterruptedAt: contribution.lastInterruptedAt,
            lastCompletedTurnAt: contribution.lastCompletedTurnAt
        )
        let previousUpdatedAt = aggregate.updatedAt
        if let model = contribution.model,
           !model.isEmpty,
           aggregate.model == nil || contribution.updatedAt >= previousUpdatedAt {
            aggregate.model = model
        }
        aggregate.inputTokens += contribution.inputTokens
        aggregate.outputTokens += contribution.outputTokens
        aggregate.cachedInputTokens += contribution.cachedInputTokens
        aggregate.totalTokens += contribution.totalTokens
        aggregate.updatedAt = max(previousUpdatedAt, contribution.updatedAt)
        aggregate.lastUserAt = max(aggregate.lastUserAt, contribution.lastUserAt)
        aggregate.lastCompletionAt = max(aggregate.lastCompletionAt, contribution.lastCompletionAt)
        aggregate.lastInterruptedAt = max(aggregate.lastInterruptedAt, contribution.lastInterruptedAt)
        aggregate.lastCompletedTurnAt = max(aggregate.lastCompletedTurnAt, contribution.lastCompletedTurnAt)
        return aggregate
    }

    private func usageTotals(from usage: [String: Any]) -> UsageTotals {
        let input = (usage["input_tokens"] as? NSNumber)?.intValue ?? 0
        let output = (usage["output_tokens"] as? NSNumber)?.intValue ?? 0
        let cacheRead = (usage["cache_read_input_tokens"] as? NSNumber)?.intValue ?? 0
        return UsageTotals(
            inputTokens: input,
            outputTokens: output,
            cachedInputTokens: cacheRead,
            totalTokens: input + output
        )
    }

    private func isClaudeInterruptedRow(_ row: [String: Any]) -> Bool {
        if let toolUseResult = row["toolUseResult"] as? [String: Any],
           let interrupted = toolUseResult["interrupted"] as? Bool,
           interrupted {
            return true
        }

        let message = row["message"] as? [String: Any] ?? [:]
        if let content = message["content"] as? String {
            return content.contains("[Request interrupted by user]")
        }
        if let items = message["content"] as? [[String: Any]] {
            for item in items {
                if let text = item["text"] as? String,
                   text.contains("[Request interrupted by user]") {
                    return true
                }
            }
        }
        return false
    }
}

actor ClaudeRuntimeProbeService {
    private var externalSessionIDByRuntimeSessionID: [String: String] = [:]
    private var lastLogMessageByRuntimeSessionID: [String: String] = [:]
    private var lastLogAtByDedupeKey: [String: Double] = [:]
    private let logger = AppDebugLog.shared

    func reset(runtimeSessionID: String) {
        externalSessionIDByRuntimeSessionID[runtimeSessionID] = nil
        lastLogMessageByRuntimeSessionID[runtimeSessionID] = nil
    }

    func snapshot(
        runtimeSessionID: String,
        projectPath: String,
        knownExternalSessionID: String?
    ) async -> AIRuntimeContextSnapshot? {
        let mappedExternalSessionID = mappedExternalSessionID(for: runtimeSessionID)
        let preferredExternalSessionID: String? = {
            if let mappedExternalSessionID, !mappedExternalSessionID.isEmpty {
                return mappedExternalSessionID
            }
            guard let knownExternalSessionID, !knownExternalSessionID.isEmpty else {
                return nil
            }
            return knownExternalSessionID
        }()

        if let preferredExternalSessionID {
            if let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: preferredExternalSessionID
            ) {
                let source = mappedExternalSessionID == preferredExternalSessionID ? "mapped" : "live"
                logRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "hit source=\(source) runtimeSession=\(runtimeSessionID) externalSession=\(preferredExternalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
                )
                return normalizedSnapshot(
                    runtimeSessionID: runtimeSessionID,
                    externalSessionID: preferredExternalSessionID,
                    snapshot: snapshot
                )
            }
            let source = mappedExternalSessionID == preferredExternalSessionID ? "mapped" : "live"
            logRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "miss source=\(source) runtimeSession=\(runtimeSessionID) externalSession=\(preferredExternalSessionID)",
                dedupeKey: "\(runtimeSessionID):miss:\(source)",
                minimumInterval: 15
            )
            if mappedExternalSessionID == preferredExternalSessionID {
                return nil
            }
        }

        if let knownExternalSessionID, !knownExternalSessionID.isEmpty {
            if let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: knownExternalSessionID
            ) {
                logRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "hit source=live runtimeSession=\(runtimeSessionID) externalSession=\(knownExternalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
                )
                return normalizedSnapshot(
                    runtimeSessionID: runtimeSessionID,
                    externalSessionID: knownExternalSessionID,
                    snapshot: snapshot
                )
            }
            logRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "miss source=live runtimeSession=\(runtimeSessionID) externalSession=\(knownExternalSessionID)",
                dedupeKey: "\(runtimeSessionID):miss:live",
                minimumInterval: 15
            )
        }

        if let mappedExternalSessionID {
            guard let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: mappedExternalSessionID
            ) else {
                logRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "miss source=mapped runtimeSession=\(runtimeSessionID) externalSession=\(mappedExternalSessionID)",
                    dedupeKey: "\(runtimeSessionID):miss:mapped",
                    minimumInterval: 15
                )
                return nil
            }
            logRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "hit source=mapped runtimeSession=\(runtimeSessionID) externalSession=\(mappedExternalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
            )
            return normalizedSnapshot(
                runtimeSessionID: runtimeSessionID,
                externalSessionID: mappedExternalSessionID,
                snapshot: snapshot
            )
        }

        if let externalSessionID = externalSessionIDByRuntimeSessionID[runtimeSessionID] {
            guard let snapshot = await ClaudeRuntimeLogCache.shared.snapshot(
                projectPath: projectPath,
                externalSessionID: externalSessionID
            ) else {
                logRuntime(
                    runtimeSessionID: runtimeSessionID,
                    message: "miss source=cached runtimeSession=\(runtimeSessionID) externalSession=\(externalSessionID)",
                    dedupeKey: "\(runtimeSessionID):miss:cached",
                    minimumInterval: 15
                )
                return nil
            }
            logRuntime(
                runtimeSessionID: runtimeSessionID,
                message: "hit source=cached runtimeSession=\(runtimeSessionID) externalSession=\(externalSessionID) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil")"
            )
            return normalizedSnapshot(
                runtimeSessionID: runtimeSessionID,
                externalSessionID: externalSessionID,
                snapshot: snapshot
            )
        }

        logRuntime(
            runtimeSessionID: runtimeSessionID,
            message: "miss source=none runtimeSession=\(runtimeSessionID) projectPath=\(projectPath)",
            dedupeKey: "\(runtimeSessionID):miss:none",
            minimumInterval: 15
        )
        return nil
    }

    private func logRuntime(
        runtimeSessionID: String,
        message: String,
        dedupeKey: String? = nil,
        minimumInterval: TimeInterval = 0
    ) {
        if let dedupeKey, minimumInterval > 0 {
            let now = Date().timeIntervalSince1970
            if let lastLoggedAt = lastLogAtByDedupeKey[dedupeKey],
               now - lastLoggedAt < minimumInterval {
                return
            }
            lastLogAtByDedupeKey[dedupeKey] = now
        }
        guard lastLogMessageByRuntimeSessionID[runtimeSessionID] != message else {
            return
        }
        lastLogMessageByRuntimeSessionID[runtimeSessionID] = message
        logger.log("claude-runtime", message)
    }

    private func mappedExternalSessionID(for runtimeSessionID: String) -> String? {
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

    private func normalizedSnapshot(
        runtimeSessionID: String,
        externalSessionID: String,
        snapshot: AIRuntimeContextSnapshot
    ) -> AIRuntimeContextSnapshot {
        if externalSessionIDByRuntimeSessionID[runtimeSessionID] != externalSessionID {
            externalSessionIDByRuntimeSessionID[runtimeSessionID] = externalSessionID
        }

        let normalizedResponseState: AIResponseState? = {
            guard let responseState = snapshot.responseState else {
                return nil
            }

            logger.log(
                "claude-runtime",
                "suppress phase runtimeSession=\(runtimeSessionID) externalSession=\(externalSessionID) response=\(responseState.rawValue) reason=hook-owned-phase total=\(snapshot.totalTokens)"
            )
            return nil
        }()

        return AIRuntimeContextSnapshot(
            tool: snapshot.tool,
            externalSessionID: externalSessionID,
            model: snapshot.model,
            inputTokens: snapshot.inputTokens,
            outputTokens: snapshot.outputTokens,
            cachedInputTokens: snapshot.cachedInputTokens,
            totalTokens: snapshot.totalTokens,
            updatedAt: snapshot.updatedAt,
            startedAt: snapshot.startedAt,
            completedAt: snapshot.completedAt,
            responseState: normalizedResponseState,
            wasInterrupted: snapshot.wasInterrupted,
            hasCompletedTurn: snapshot.hasCompletedTurn
        )
    }
}

func isClaudeInterruptedRow(_ row: [String: Any]) -> Bool {
    if let toolUseResult = row["toolUseResult"] as? [String: Any],
       let interrupted = toolUseResult["interrupted"] as? Bool,
       interrupted {
        return true
    }

    let message = row["message"] as? [String: Any] ?? [:]
    if let content = message["content"] as? String {
        return content.contains("[Request interrupted by user]")
    }
    if let items = message["content"] as? [[String: Any]] {
        for item in items {
            if let text = item["text"] as? String,
               text.contains("[Request interrupted by user]") {
                return true
            }
        }
    }
    return false
}

private func parseClaudeISO8601Date(_ value: String) -> Date? {
    let formatterWithFractional = ISO8601DateFormatter()
    formatterWithFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = formatterWithFractional.date(from: value) {
        return date
    }

    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: value)
}
