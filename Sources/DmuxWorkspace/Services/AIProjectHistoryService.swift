import CryptoKit
import Foundation
import SQLite3

struct AIProjectHistoryService: Sendable {
    private enum JSONLIndexMode: String {
        case unchanged
        case append
        case rebuild
    }

    struct JSONLParseSnapshot {
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
    let runtimeHomeURL: URL?
    let logger = AppDebugLog.shared
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

    func loadIncrementalJSONLFileSummaries(
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
                    project: project,
                    storedSummary: storedSummary,
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
        project: Project,
        storedSummary: AIExternalFileSummary,
        snapshot: JSONLParseSnapshot
    ) -> AIExternalFileSummary {
        let deltaSummary = aggregator.buildExternalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            project: project,
            parseResult: snapshot.result
        )
        let mergedUsageBuckets = mergeUsageBuckets(storedSummary.usageBuckets, deltaSummary.usageBuckets)
        return aggregator.externalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            projectPath: project.path,
            usageBuckets: mergedUsageBuckets
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
                    totalCachedInputTokens: 0,
                    todayTokens: 0,
                    todayCachedInputTokens: 0,
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
            totalCachedInputTokens: 0,
            todayTokens: 0,
            todayCachedInputTokens: 0,
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
            payload.totalCachedInputTokens += entry.cachedInputTokens
            if calendar.startOfDay(for: entry.timestamp) == startOfToday {
                payload.todayTokens += entry.totalTokens
                payload.todayCachedInputTokens += entry.cachedInputTokens
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
            payload.todayCachedInputTokens = 0
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
            cachedInputTokens: payload.totalCachedInputTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: payload.activeDurationSeconds + activeInFlight,
            todayTokens: payload.todayTokens,
            todayCachedInputTokens: payload.todayCachedInputTokens
        )
    }

    private func mergeUsageBuckets(
        _ existing: [AIUsageBucket],
        _ delta: [AIUsageBucket]
    ) -> [AIUsageBucket] {
        var map: [String: AIUsageBucket] = [:]

        for bucket in existing + delta {
            if var current = map[bucket.id] {
                current.inputTokens += bucket.inputTokens
                current.outputTokens += bucket.outputTokens
                current.totalTokens += bucket.totalTokens
                current.cachedInputTokens += bucket.cachedInputTokens
                current.requestCount += bucket.requestCount
                current.activeDurationSeconds += bucket.activeDurationSeconds
                current.firstSeenAt = min(current.firstSeenAt, bucket.firstSeenAt)
                current.lastSeenAt = max(current.lastSeenAt, bucket.lastSeenAt)
                if current.externalSessionID == nil {
                    current.externalSessionID = bucket.externalSessionID
                }
                if current.model == nil {
                    current.model = bucket.model
                }
                if current.sessionTitle.isEmpty {
                    current.sessionTitle = bucket.sessionTitle
                }
                map[bucket.id] = current
            } else {
                map[bucket.id] = bucket
            }
        }

        return map.values.sorted {
            if $0.bucketStart != $1.bucketStart {
                return $0.bucketStart < $1.bucketStart
            }
            if $0.source != $1.source {
                return $0.source < $1.source
            }
            if $0.sessionKey != $1.sessionKey {
                return $0.sessionKey < $1.sessionKey
            }
            return ($0.model ?? "") < ($1.model ?? "")
        }
    }

    func loadFileSummaries(
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
        summary.usageBuckets.reduce(0) { $0 + $1.requestCount }
    }

    private func totalTokenCount(in summaries: [AIExternalFileSummary]) -> Int {
        summaries.reduce(0) { $0 + totalTokenCount(in: $1) }
    }

    private func totalTokenCount(in summary: AIExternalFileSummary) -> Int {
        summary.usageBuckets.reduce(0) { $0 + $1.totalTokens }
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

    func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }
}
