import Foundation

private actor ClaudeRuntimeLogCache {
    struct SessionAggregate {
        var model: String?
        var inputTokens: Int
        var outputTokens: Int
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
            responseState: responseState(for: session),
            wasInterrupted: wasInterrupted(for: session),
            hasCompletedTurn: hasCompletedTurn(for: session)
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

            let timestamp = (row["timestamp"] as? String).flatMap(parseClaudeISO8601Date)?.timeIntervalSince1970 ?? 0
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
        aggregate.totalTokens += contribution.totalTokens
        aggregate.updatedAt = max(previousUpdatedAt, contribution.updatedAt)
        aggregate.lastUserAt = max(aggregate.lastUserAt, contribution.lastUserAt)
        aggregate.lastCompletionAt = max(aggregate.lastCompletionAt, contribution.lastCompletionAt)
        aggregate.lastInterruptedAt = max(aggregate.lastInterruptedAt, contribution.lastInterruptedAt)
        aggregate.lastCompletedTurnAt = max(aggregate.lastCompletedTurnAt, contribution.lastCompletedTurnAt)
        return aggregate
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

        return AIRuntimeContextSnapshot(
            tool: snapshot.tool,
            externalSessionID: externalSessionID,
            model: snapshot.model,
            inputTokens: snapshot.inputTokens,
            outputTokens: snapshot.outputTokens,
            totalTokens: snapshot.totalTokens,
            updatedAt: snapshot.updatedAt,
            responseState: snapshot.responseState,
            wasInterrupted: snapshot.wasInterrupted,
            hasCompletedTurn: snapshot.hasCompletedTurn
        )
    }
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
