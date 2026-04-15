import CryptoKit
import Foundation
import SQLite3

struct AIUsageService: Sendable {
    private let wrapperStore = AIUsageStore()
    private let calendar = Calendar.autoupdatingCurrent
    private let codexSource = "codex"
    private let unknownSessionDate = Date(timeIntervalSince1970: 0)

    private struct AISourceLoadResult {
        var sessions: [AISessionSummary]
        var summaries: [AIExternalFileSummary]
    }

    private struct CodexThreadMetadata {
        var titleByID: [String: String]
        var firstUserMessageByID: [String: String]
        var rolloutPaths: [URL]
    }

    func fastPanelState(project: Project, liveEnvelopes: [AIToolUsageEnvelope], selectedSessionID: UUID?) -> AIStatsPanelState {
        let liveSnapshots = snapshots(from: liveEnvelopes)
        let currentSnapshot = selectedLiveSnapshot(from: liveSnapshots, selectedSessionID: selectedSessionID)
        return fastPanelState(project: project, liveSnapshots: liveSnapshots, currentSnapshot: currentSnapshot)
    }

    func fastPanelState(project: Project, liveSnapshots: [AITerminalSessionSnapshot], currentSnapshot: AITerminalSessionSnapshot?) -> AIStatsPanelState {
        if let indexed = wrapperStore.indexedProjectSnapshot(projectID: project.id) {
            return overlayLiveSummary(
                on: indexed,
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: .indexing(progress: 0.0, detail: appI18n("ai.indexing.starting", fallback: "Starting index."))
            )
        }

        return liveSummaryOnlyState(
            project: project,
            indexed: nil,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: .indexing(progress: 0.0, detail: appI18n("ai.indexing.starting", fallback: "Starting index."))
        )
    }

    func snapshotBackedPanelState(project: Project, liveEnvelopes: [AIToolUsageEnvelope], selectedSessionID: UUID?, status: AIIndexingStatus) -> AIStatsPanelState {
        let liveSnapshots = snapshots(from: liveEnvelopes)
        let currentSnapshot = selectedLiveSnapshot(from: liveSnapshots, selectedSessionID: selectedSessionID)
        return snapshotBackedPanelState(project: project, liveSnapshots: liveSnapshots, currentSnapshot: currentSnapshot, status: status)
    }

    func snapshotBackedPanelState(project: Project, liveSnapshots: [AITerminalSessionSnapshot], currentSnapshot: AITerminalSessionSnapshot?, status: AIIndexingStatus) -> AIStatsPanelState {
        if let indexed = wrapperStore.indexedProjectSnapshot(projectID: project.id) {
            return overlayLiveSummary(
                on: indexed,
                project: project,
                liveSnapshots: liveSnapshots,
                currentSnapshot: currentSnapshot,
                status: status
            )
        }

        return liveSummaryOnlyState(
            project: project,
            indexed: nil,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: status
        )
    }

    func lightweightLivePanelState(
        from currentState: AIStatsPanelState,
        project: Project,
        liveEnvelopes: [AIToolUsageEnvelope],
        selectedSessionID: UUID?,
        status: AIIndexingStatus
    ) -> AIStatsPanelState {
        let liveSnapshots = snapshots(from: liveEnvelopes)
        let currentSnapshot = selectedLiveSnapshot(from: liveSnapshots, selectedSessionID: selectedSessionID)
        return lightweightLivePanelState(
            from: currentState,
            project: project,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: status
        )
    }

    func lightweightLivePanelState(
        from currentState: AIStatsPanelState,
        project: Project,
        liveSnapshots: [AITerminalSessionSnapshot],
        currentSnapshot: AITerminalSessionSnapshot?,
        status: AIIndexingStatus
    ) -> AIStatsPanelState {
        let nextLiveOverlayTokens = liveSnapshots.reduce(0) { $0 + $1.currentTotalTokens }

        var nextState = currentState
        nextState.currentSnapshot = currentSnapshot
        nextState.liveSnapshots = liveSnapshots
        nextState.liveOverlayTokens = nextLiveOverlayTokens
        nextState.indexingStatus = status

        if var summary = currentState.projectSummary, summary.projectID == project.id {
            let baseProjectTotal = max(0, summary.projectTotalTokens - currentState.liveOverlayTokens)
            let baseTodayTotal = max(0, summary.todayTotalTokens - currentState.liveOverlayTokens)
            summary.projectTotalTokens = baseProjectTotal + nextLiveOverlayTokens
            summary.todayTotalTokens = baseTodayTotal + nextLiveOverlayTokens
            summary.currentSessionTokens = currentSnapshot?.currentTotalTokens ?? 0
            summary.currentTool = currentSnapshot?.tool
            summary.currentModel = currentSnapshot?.model
            summary.currentContextUsagePercent = currentSnapshot?.currentContextUsagePercent
            summary.currentContextUsedTokens = currentSnapshot?.currentContextUsedTokens
            summary.currentContextWindow = currentSnapshot?.currentContextWindow
            summary.currentSessionUpdatedAt = currentSnapshot?.updatedAt
            nextState.projectSummary = summary
        } else {
            nextState.projectSummary = baseProjectSummary(
                project: project,
                liveSnapshot: currentSnapshot,
                sessions: currentState.sessions,
                todayTotalTokens: todayTotalTokens(
                    timeBuckets: currentState.todayTimeBuckets,
                    heatmap: currentState.heatmap
                )
            )
        }

        return nextState
    }

    func panelState(project: Project, liveEnvelopes: [AIToolUsageEnvelope], selectedSessionID: UUID?, onProgress: @Sendable @escaping (AIIndexingStatus) async -> Void) async -> AIStatsPanelState {
        do {
            try Task.checkCancellation()
            await onProgress(.indexing(progress: 0.05, detail: appI18n("ai.indexing.preparing", fallback: "Preparing usage data.")))
            let directorySummary = try await loadDirectoryDrivenSummary(project: project, onProgress: onProgress)
            try Task.checkCancellation()
            let liveSnapshots = snapshots(from: liveEnvelopes)
            let liveSnapshot = selectedLiveSnapshot(from: liveSnapshots, selectedSessionID: selectedSessionID)

            if !directorySummary.sessions.isEmpty || liveSnapshot != nil {
                let currentSnapshot = liveSnapshot

                let todayTotal = todayTotalTokens(
                    timeBuckets: directorySummary.todayTimeBuckets,
                    heatmap: directorySummary.heatmap
                )
                let summary = baseProjectSummary(
                    project: project,
                    liveSnapshot: currentSnapshot,
                    sessions: directorySummary.sessions,
                    todayTotalTokens: todayTotal
                )
                let indexedSnapshot = AIIndexedProjectSnapshot(
                    projectID: project.id,
                    projectName: project.name,
                    projectSummary: AIProjectUsageSummary(
                        projectID: project.id,
                        projectName: project.name,
                        currentSessionTokens: 0,
                        projectTotalTokens: directorySummary.sessions.reduce(0) { $0 + $1.totalTokens },
                        todayTotalTokens: todayTotal,
                        currentTool: nil,
                        currentModel: nil,
                        currentContextUsagePercent: nil,
                        currentContextUsedTokens: nil,
                        currentContextWindow: nil,
                        currentSessionUpdatedAt: directorySummary.sessions.lazy.compactMap { knownSessionDate($0.lastSeenAt) }.first
                    ),
                    sessions: directorySummary.sessions,
                    heatmap: directorySummary.heatmap,
                    todayTimeBuckets: directorySummary.todayTimeBuckets,
                    toolBreakdown: directorySummary.toolBreakdown,
                    modelBreakdown: directorySummary.modelBreakdown,
                    indexedAt: Date()
                )
                wrapperStore.saveIndexedProjectSnapshot(indexedSnapshot)

                _ = summary
                return overlayLiveSummary(
                    on: indexedSnapshot,
                    project: project,
                    liveSnapshots: liveSnapshots,
                    currentSnapshot: currentSnapshot,
                    status: .completed(detail: appI18n("ai.indexing.complete", fallback: "Index complete."))
                )
            }

            if let indexed = wrapperStore.indexedProjectSnapshot(projectID: project.id) {
                return overlayLiveSummary(
                    on: indexed,
                    project: project,
                    liveSnapshots: liveSnapshots,
                    currentSnapshot: liveSnapshot,
                    status: .completed(detail: appI18n("ai.indexing.complete", fallback: "Index complete."))
                )
            }

            return liveSummaryOnlyState(
                project: project,
                indexed: nil,
                liveSnapshots: liveSnapshots,
                currentSnapshot: liveSnapshot,
                status: .completed(detail: appI18n("ai.indexing.complete", fallback: "Index complete."))
            )
        } catch is CancellationError {
            return snapshotBackedPanelState(project: project, liveEnvelopes: liveEnvelopes, selectedSessionID: selectedSessionID, status: .cancelled(detail: appI18n("ai.indexing.stopped", fallback: "Indexing stopped.")))
        } catch {
            let detail = (error as NSError).localizedDescription
            return snapshotBackedPanelState(project: project, liveEnvelopes: liveEnvelopes, selectedSessionID: selectedSessionID, status: .failed(detail: detail))
        }
    }

    private func overlayLiveSummary(on indexed: AIIndexedProjectSnapshot, project: Project, liveSnapshots: [AITerminalSessionSnapshot], currentSnapshot: AITerminalSessionSnapshot?, status: AIIndexingStatus) -> AIStatsPanelState {
        liveSummaryOnlyState(
            project: project,
            indexed: indexed,
            liveSnapshots: liveSnapshots,
            currentSnapshot: currentSnapshot,
            status: status
        )
    }

    private func liveSummaryOnlyState(
        project: Project,
        indexed: AIIndexedProjectSnapshot?,
        liveSnapshots: [AITerminalSessionSnapshot],
        currentSnapshot: AITerminalSessionSnapshot?,
        status: AIIndexingStatus
    ) -> AIStatsPanelState {
        let totalLiveDelta = liveSnapshots.reduce(0) { partial, snapshot in
            let indexedSession = indexed.flatMap { self.indexedSession(matching: snapshot, in: $0.sessions) }
            let delta = max(0, snapshot.currentTotalTokens - (indexedSession?.totalTokens ?? 0))
            return partial + delta
        }

        var summary = indexed?.projectSummary ?? AIProjectUsageSummary(
            projectID: project.id,
            projectName: project.name,
            currentSessionTokens: 0,
            projectTotalTokens: 0,
            todayTotalTokens: 0,
            currentTool: nil,
            currentModel: nil,
            currentContextUsagePercent: nil,
            currentContextUsedTokens: nil,
            currentContextWindow: nil,
            currentSessionUpdatedAt: nil
        )

        summary.projectID = project.id
        summary.projectName = project.name
        summary.projectTotalTokens = (indexed?.projectSummary.projectTotalTokens ?? 0) + totalLiveDelta
        summary.todayTotalTokens = todayTotalTokens(
            timeBuckets: indexed?.todayTimeBuckets ?? [],
            heatmap: indexed?.heatmap ?? []
        ) + totalLiveDelta
        summary.currentSessionTokens = currentSnapshot?.currentTotalTokens ?? 0
        summary.currentTool = currentSnapshot?.tool
        summary.currentModel = currentSnapshot?.model
        summary.currentContextUsagePercent = currentSnapshot?.currentContextUsagePercent
        summary.currentContextUsedTokens = currentSnapshot?.currentContextUsedTokens
        summary.currentContextWindow = currentSnapshot?.currentContextWindow
        summary.currentSessionUpdatedAt = currentSnapshot?.updatedAt ?? indexed?.projectSummary.currentSessionUpdatedAt

        return AIStatsPanelState(
            projectSummary: summary,
            currentSnapshot: currentSnapshot,
            liveSnapshots: liveSnapshots,
            liveOverlayTokens: totalLiveDelta,
            sessions: indexed?.sessions ?? [],
            heatmap: indexed?.heatmap ?? [],
            todayTimeBuckets: indexed?.todayTimeBuckets ?? [],
            toolBreakdown: indexed?.toolBreakdown ?? [],
            modelBreakdown: indexed?.modelBreakdown ?? [],
            indexedAt: indexed?.indexedAt,
            indexingStatus: status
        )
    }

    private func snapshots(from envelopes: [AIToolUsageEnvelope]) -> [AITerminalSessionSnapshot] {
        var map: [UUID: AITerminalSessionSnapshot] = [:]
        for envelope in envelopes {
            guard let snapshot = snapshot(from: envelope) else {
                continue
            }
            if let existing = map[snapshot.sessionID] {
                if snapshot.updatedAt > existing.updatedAt {
                    map[snapshot.sessionID] = snapshot
                }
            } else {
                map[snapshot.sessionID] = snapshot
            }
        }
        return map.values.sorted { $0.updatedAt > $1.updatedAt }
    }

    private func selectedLiveSnapshot(from snapshots: [AITerminalSessionSnapshot], selectedSessionID: UUID?) -> AITerminalSessionSnapshot? {
        if let selectedSessionID {
            return snapshots.first(where: { $0.sessionID == selectedSessionID })
        }
        return snapshots.first
    }

    private func indexedSession(matching liveSnapshot: AITerminalSessionSnapshot, in sessions: [AISessionSummary]) -> AISessionSummary? {
        sessions.first { sessionMatchesLiveSnapshot(session: $0, liveSnapshot: liveSnapshot) }
    }

    private func sessionMatchesLiveSnapshot(session: AISessionSummary, liveSnapshot: AITerminalSessionSnapshot) -> Bool {
        if let liveExternalSessionID = liveSnapshot.externalSessionID,
           let indexedExternalSessionID = session.externalSessionID,
           !liveExternalSessionID.isEmpty,
           !indexedExternalSessionID.isEmpty {
            return liveExternalSessionID == indexedExternalSessionID
        }
        return session.sessionID == liveSnapshot.sessionID
    }

    private func baseProjectSummary(
        project: Project,
        liveSnapshot: AITerminalSessionSnapshot?,
        sessions: [AISessionSummary],
        todayTotalTokens: Int
    ) -> AIProjectUsageSummary {
        AIProjectUsageSummary(
            projectID: project.id,
            projectName: project.name,
            currentSessionTokens: liveSnapshot?.currentTotalTokens ?? 0,
            projectTotalTokens: sessions.reduce(0) { $0 + $1.totalTokens },
            todayTotalTokens: todayTotalTokens,
            currentTool: liveSnapshot?.tool,
            currentModel: liveSnapshot?.model,
            currentContextUsagePercent: liveSnapshot?.currentContextUsagePercent,
            currentContextUsedTokens: liveSnapshot?.currentContextUsedTokens,
            currentContextWindow: liveSnapshot?.currentContextWindow,
            currentSessionUpdatedAt: liveSnapshot?.updatedAt
        )
    }

    private func snapshot(from envelope: AIToolUsageEnvelope) -> AITerminalSessionSnapshot? {
        guard let projectID = UUID(uuidString: envelope.projectId),
              let sessionID = UUID(uuidString: envelope.sessionId) else {
            return nil
        }
        return AITerminalSessionSnapshot(
            sessionID: sessionID,
            externalSessionID: envelope.externalSessionID,
            projectID: projectID,
            projectName: envelope.projectName,
            sessionTitle: envelope.sessionTitle,
            tool: envelope.tool,
            model: envelope.model,
            status: envelope.status,
            responseState: envelope.responseState,
            startedAt: envelope.startedAt.map { Date(timeIntervalSince1970: $0) },
            updatedAt: Date(timeIntervalSince1970: envelope.updatedAt),
            currentInputTokens: envelope.inputTokens ?? 0,
            currentOutputTokens: envelope.outputTokens ?? 0,
            currentTotalTokens: envelope.totalTokens ?? 0,
            currentContextWindow: envelope.contextWindow,
            currentContextUsedTokens: envelope.contextUsedTokens,
            currentContextUsagePercent: envelope.contextUsagePercent
        )
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

    private func loadDirectoryDrivenSummary(project: Project, onProgress: @Sendable @escaping (AIIndexingStatus) async -> Void) async throws -> AIProjectDirectorySourceSummary {
        try Task.checkCancellation()
        if let cached = await AIProjectSummaryCache.shared.get(projectPath: project.path) {
            return cached
        }

        await onProgress(.indexing(progress: 0.12, detail: appI18n("ai.indexing.reading_sources", fallback: "Reading AI usage sources in parallel.")))
        async let opencodeSessionsTask = loadOpencodeSource(project: project)
        async let codexSessionsTask = loadCodexSource(project: project)
        async let claudeSessionsTask = loadClaudeSource(project: project)
        async let geminiSessionsTask = loadGeminiSource(project: project)

        let opencodeResult = await opencodeSessionsTask
        await onProgress(.indexing(progress: 0.38, detail: appI18n("ai.indexing.loaded_opencode", fallback: "Loaded OpenCode usage.")))
        try Task.checkCancellation()

        let codexResult = await codexSessionsTask
        await onProgress(.indexing(progress: 0.64, detail: appI18n("ai.indexing.loaded_codex", fallback: "Indexed Codex sessions.")))
        try Task.checkCancellation()

        let claudeResult = await claudeSessionsTask
        await onProgress(.indexing(progress: 0.8, detail: appI18n("ai.indexing.loaded_claude", fallback: "Loaded Claude usage.")))
        try Task.checkCancellation()
        let geminiResult = await geminiSessionsTask
        let externalSummaries = opencodeResult.summaries + codexResult.summaries + claudeResult.summaries + geminiResult.summaries
        let opencodeSessions = opencodeResult.sessions
        let codexSessions = codexResult.sessions
        let claudeSessions = claudeResult.sessions
        let geminiSessions = geminiResult.sessions
        let sessions = sortSessions(opencodeSessions + codexSessions + claudeSessions + geminiSessions)

        let toolBreakdown = breakdown(items: sessions.map { ($0.lastTool ?? appI18n("ai.unknown_tool", fallback: "Unknown Tool"), $0.totalTokens, 1) })
        let modelBreakdown = breakdown(items: sessions.compactMap {
            guard let model = $0.lastModel, !model.isEmpty else {
                return nil
            }
            return (model, $0.totalTokens, 1)
        })
        await onProgress(.indexing(progress: 0.84, detail: appI18n("ai.indexing.summarized_sessions", fallback: "Summarized sessions and rankings.")))
        await onProgress(.indexing(progress: 0.9, detail: appI18n("ai.indexing.summarizing_recent", fallback: "Summarizing recent usage.")))
        let heatmap = buildHeatmap(from: externalSummaries, fallbackSessions: sessions)
        let todayTimeBuckets = buildTodayTimeBuckets(from: externalSummaries)
        try Task.checkCancellation()

        let snapshot = sessions.first.map {
            AITerminalSessionSnapshot(
                sessionID: $0.sessionID,
                externalSessionID: $0.externalSessionID,
                projectID: $0.projectID,
                projectName: $0.projectName,
                sessionTitle: $0.sessionTitle,
                tool: $0.lastTool,
                model: $0.lastModel,
                status: "idle",
                responseState: .idle,
                startedAt: $0.firstSeenAt,
                updatedAt: knownSessionDate($0.lastSeenAt) ?? $0.firstSeenAt,
                currentInputTokens: $0.totalInputTokens,
                currentOutputTokens: $0.totalOutputTokens,
                currentTotalTokens: $0.totalTokens,
                currentContextWindow: nil,
                currentContextUsedTokens: nil,
                currentContextUsagePercent: $0.maxContextUsagePercent
            )
        }

        let summary = AIProjectDirectorySourceSummary(
            snapshot: snapshot,
            sessions: sessions,
            heatmap: heatmap,
            todayTimeBuckets: todayTimeBuckets,
            toolBreakdown: toolBreakdown,
            modelBreakdown: modelBreakdown
        )

        await AIProjectSummaryCache.shared.set(projectPath: project.path, summary: summary)
        return summary
    }

    private func loadOpencodeSource(project: Project) async -> AISourceLoadResult {
        let dbPath = NSHomeDirectory() + "/.local/share/opencode/opencode.db"
        guard FileManager.default.fileExists(atPath: dbPath) else { return AISourceLoadResult(sessions: [], summaries: []) }

        let modifiedAt = ((try? URL(fileURLWithPath: dbPath).resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast).timeIntervalSince1970
        if let cached = wrapperStore.cachedExternalSummary(source: "opencode-db", filePath: dbPath, modifiedAt: modifiedAt), cached.projectPath == project.path {
            return AISourceLoadResult(sessions: cached.sessions, summaries: [cached])
        }

        var db: OpaquePointer?
        guard sqlite3_open(dbPath, &db) == SQLITE_OK, let db else { return AISourceLoadResult(sessions: [], summaries: []) }
        defer { sqlite3_close(db) }

        let sql = """
        SELECT s.id, s.title, s.directory, m.data
        FROM session s
        JOIN message m ON m.session_id = s.id
        WHERE s.directory = ? AND s.time_archived IS NULL
        ORDER BY m.time_created DESC;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return AISourceLoadResult(sessions: [], summaries: [])
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, project.path, -1, SQLITE_TRANSIENT)

        var map: [String: AISessionSummary] = [:]
        var dayUsageMap: [Date: AIHeatmapDay] = [:]
        var timeBucketMap: [Date: AITimeBucket] = [:]
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let sessionID = text(at: 0, statement),
                  let sessionTitle = text(at: 1, statement),
                  let json = text(at: 3, statement),
                  let data = json.data(using: .utf8),
                  let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                continue
            }

            let tokens = payload["tokens"] as? [String: Any] ?? [:]
            let cache = tokens["cache"] as? [String: Any] ?? [:]
            let total = (tokens["total"] as? NSNumber)?.intValue ?? 0
            let input = (tokens["input"] as? NSNumber)?.intValue ?? 0
            let output = (tokens["output"] as? NSNumber)?.intValue ?? 0
            let cacheRead = (cache["read"] as? NSNumber)?.intValue ?? 0
            let cacheWrite = (cache["write"] as? NSNumber)?.intValue ?? 0
            let finalTotal = max(total, input + output + cacheRead + cacheWrite)
            let model = payload["modelID"] as? String
            let time = payload["time"] as? [String: Any]
            let completed = (time?["completed"] as? NSNumber)?.doubleValue
            let created = (time?["created"] as? NSNumber)?.doubleValue
            let lastSeenAt = completed.map { Date(timeIntervalSince1970: $0 / 1000) } ?? unknownSessionDate
            let firstSeenAt = created.map { Date(timeIntervalSince1970: $0 / 1000) } ?? lastSeenAt
            if let knownLastSeenAt = knownSessionDate(lastSeenAt) {
                accumulateUsage(date: knownLastSeenAt, tokens: finalTotal, requests: 1, dayUsageMap: &dayUsageMap, timeBucketMap: &timeBucketMap)
            }

            let sessionUUID = UUID(uuidString: sessionID) ?? UUID()
            if var existing = map[sessionID] {
                existing.totalInputTokens += input + cacheWrite + cacheRead
                existing.totalOutputTokens += output
                existing.totalTokens += finalTotal
                existing.lastSeenAt = maxSessionDate(existing.lastSeenAt, lastSeenAt)
                existing.lastModel = existing.lastModel ?? model
                existing.todayTokens += isKnownSessionDate(lastSeenAt) && isToday(lastSeenAt) ? finalTotal : 0
                map[sessionID] = existing
            } else {
                map[sessionID] = AISessionSummary(
                    sessionID: sessionUUID,
                    externalSessionID: sessionID,
                    projectID: project.id,
                    projectName: project.name,
                    sessionTitle: sessionTitle,
                    firstSeenAt: firstSeenAt,
                    lastSeenAt: lastSeenAt,
                    lastTool: "opencode",
                    lastModel: model,
                    requestCount: 1,
                    totalInputTokens: input + cacheWrite + cacheRead,
                    totalOutputTokens: output,
                    totalTokens: finalTotal,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: activeDuration(firstSeenAt: firstSeenAt, lastSeenAt: lastSeenAt),
                    todayTokens: isKnownSessionDate(lastSeenAt) && isToday(lastSeenAt) ? finalTotal : 0
                )
            }
        }
        let sessions = sortSessions(Array(map.values))
        let summary = AIExternalFileSummary(
            source: "opencode-db",
            filePath: dbPath,
            fileModifiedAt: modifiedAt,
            projectPath: project.path,
            sessions: sessions,
            dayUsage: dayUsageMap.values.sorted { $0.day < $1.day },
            timeBuckets: mergeCachedTimeBuckets(timeBucketMap),
            codexState: nil
        )
        wrapperStore.saveExternalSummary(summary)
        return AISourceLoadResult(sessions: sessions, summaries: [summary])
    }

    private func fastProjectSessions(project: Project) -> [AISessionSummary] {
        let opencodeSessions = fastOpencodeSessions(project: project)
        let codexSessions = fastCodexSessions(project: project)
        let geminiSessions = fastGeminiSessions(project: project)
        return sortSessions(opencodeSessions + codexSessions + geminiSessions)
    }

    private func fastGeminiSessions(project: Project) -> [AISessionSummary] {
        loadGeminiSessions(project: project)
    }

    private func fastOpencodeSessions(project: Project) -> [AISessionSummary] {
        let dbPath = NSHomeDirectory() + "/.local/share/opencode/opencode.db"
        guard FileManager.default.fileExists(atPath: dbPath) else { return [] }

        var db: OpaquePointer?
        guard sqlite3_open(dbPath, &db) == SQLITE_OK, let db else { return [] }
        defer { sqlite3_close(db) }

        let sql = """
        SELECT s.id, s.title, m.data
        FROM session s
        JOIN message m ON m.session_id = s.id
        WHERE s.directory = ? AND s.time_archived IS NULL
        ORDER BY m.time_created DESC;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return []
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, project.path, -1, SQLITE_TRANSIENT)

        var map: [String: AISessionSummary] = [:]
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let sessionID = text(at: 0, statement),
                  let sessionTitle = text(at: 1, statement),
                  let json = text(at: 2, statement),
                  let data = json.data(using: .utf8),
                  let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                continue
            }

            let tokens = payload["tokens"] as? [String: Any] ?? [:]
            let cache = tokens["cache"] as? [String: Any] ?? [:]
            let total = (tokens["total"] as? NSNumber)?.intValue ?? 0
            let input = (tokens["input"] as? NSNumber)?.intValue ?? 0
            let output = (tokens["output"] as? NSNumber)?.intValue ?? 0
            let cacheRead = (cache["read"] as? NSNumber)?.intValue ?? 0
            let cacheWrite = (cache["write"] as? NSNumber)?.intValue ?? 0
            let finalTotal = max(total, input + output + cacheRead + cacheWrite)
            let model = payload["modelID"] as? String
            let time = payload["time"] as? [String: Any]
            let completed = (time?["completed"] as? NSNumber)?.doubleValue
            let created = (time?["created"] as? NSNumber)?.doubleValue
            let lastSeenAt = completed.map { Date(timeIntervalSince1970: $0 / 1000) } ?? unknownSessionDate
            let firstSeenAt = created.map { Date(timeIntervalSince1970: $0 / 1000) } ?? lastSeenAt

            if var existing = map[sessionID] {
                existing.totalInputTokens += input + cacheWrite + cacheRead
                existing.totalOutputTokens += output
                existing.totalTokens += finalTotal
                existing.lastSeenAt = maxSessionDate(existing.lastSeenAt, lastSeenAt)
                existing.lastModel = existing.lastModel ?? model
                existing.todayTokens += isKnownSessionDate(lastSeenAt) && isToday(lastSeenAt) ? finalTotal : 0
                map[sessionID] = existing
            } else {
                map[sessionID] = AISessionSummary(
                    sessionID: UUID(uuidString: sessionID) ?? UUID(),
                    externalSessionID: sessionID,
                    projectID: project.id,
                    projectName: project.name,
                    sessionTitle: sessionTitle,
                    firstSeenAt: firstSeenAt,
                    lastSeenAt: lastSeenAt,
                    lastTool: "opencode",
                    lastModel: model,
                    requestCount: 1,
                    totalInputTokens: input + cacheWrite + cacheRead,
                    totalOutputTokens: output,
                    totalTokens: finalTotal,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: activeDuration(firstSeenAt: firstSeenAt, lastSeenAt: lastSeenAt),
                    todayTokens: isKnownSessionDate(lastSeenAt) && isToday(lastSeenAt) ? finalTotal : 0
                )
            }
        }
        return sortSessions(Array(map.values))
    }

    private func fastCodexSessions(project: Project) -> [AISessionSummary] {
        let metadata = loadCodexThreadMetadata(projectPath: project.path)
        let dbPath = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex/state_5.sqlite").path
        guard FileManager.default.fileExists(atPath: dbPath) else { return [] }

        var db: OpaquePointer?
        guard sqlite3_open(dbPath, &db) == SQLITE_OK, let db else { return [] }
        defer { sqlite3_close(db) }

        let sql = "SELECT id, title, first_user_message, tokens_used, model, created_at, updated_at FROM threads WHERE cwd = ? AND archived = 0 ORDER BY updated_at DESC;"
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return []
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, project.path, -1, SQLITE_TRANSIENT)

        var sessions: [AISessionSummary] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawID = text(at: 0, statement) else {
                continue
            }
            let sessionID = UUID(uuidString: rawID) ?? UUID()
            let title = codexSessionTitle(
                sessionID: sessionID,
                inlineTitle: text(at: 1, statement),
                derivedTitle: text(at: 2, statement),
                threadMetadata: metadata
            ) ?? project.name
            let totalTokens = int(at: 3, statement)
            let model = text(at: 4, statement)
            let createdAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 5))
            let updatedAt = Date(timeIntervalSince1970: sqlite3_column_double(statement, 6))
            sessions.append(
                AISessionSummary(
                    sessionID: sessionID,
                    externalSessionID: rawID,
                    projectID: project.id,
                    projectName: project.name,
                    sessionTitle: title,
                    firstSeenAt: createdAt,
                    lastSeenAt: updatedAt,
                    lastTool: "codex",
                    lastModel: model,
                    requestCount: 1,
                    totalInputTokens: totalTokens,
                    totalOutputTokens: 0,
                    totalTokens: totalTokens,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: max(0, Int(updatedAt.timeIntervalSince(createdAt))),
                    todayTokens: 0
                )
            )
        }
        return sessions
    }

    private func loadCodexSource(project: Project) async -> AISourceLoadResult {
        let threadMetadata = loadCodexThreadMetadata(projectPath: project.path)
        let targetFiles = threadMetadata.rolloutPaths
        guard !targetFiles.isEmpty else {
            return AISourceLoadResult(sessions: [], summaries: [])
        }

        let fileBatches = stride(from: 0, to: targetFiles.count, by: 6).map {
            Array(targetFiles[$0 ..< min($0 + 6, targetFiles.count)])
        }

        var summaries: [AIExternalFileSummary] = []
        for batch in fileBatches {
            let batchSummaries = await withTaskGroup(of: AIExternalFileSummary?.self) { group in
                for fileURL in batch {
                    group.addTask {
                        let modifiedAt = ((try? fileURL.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast).timeIntervalSince1970
                        if let cached = wrapperStore.cachedExternalSummary(source: codexSource, filePath: fileURL.path, modifiedAt: modifiedAt), cached.projectPath == project.path {
                            return cached
                        }

                        let previous = wrapperStore.latestCachedExternalSummary(source: codexSource, filePath: fileURL.path)
                        guard let summary = loadCodexSessionSummary(from: fileURL, modifiedAt: modifiedAt, project: project, previous: previous, threadMetadata: threadMetadata) else {
                            return nil
                        }

                        wrapperStore.saveExternalSummary(summary)
                        return summary
                    }
                }

                var merged: [AIExternalFileSummary] = []
                for await summary in group {
                    if let summary {
                        merged.append(summary)
                    }
                }
                return merged
            }
            summaries.append(contentsOf: batchSummaries)
        }

        let sessions = sortSessions(summaries.flatMap(\.sessions))
        return AISourceLoadResult(sessions: sessions, summaries: summaries)
    }

    private func loadCodexSessionSummary(from fileURL: URL, modifiedAt: Double, project: Project, previous: AIExternalFileSummary?, threadMetadata: CodexThreadMetadata) -> AIExternalFileSummary? {
        _ = previous
        guard let handle = try? FileHandle(forReadingFrom: fileURL) else {
            return nil
        }
        defer { try? handle.close() }

        _ = try? handle.seekToEnd()
        try? handle.seek(toOffset: 0)

        var sessionID: UUID?
        var title: String?
        var model: String?
        var derivedTitle: String?
        var firstSeenAt: Date?
        var lastSeenAt: Date?
        var totalTokens = 0
        var lastTokenTotal = 0
        var matchedProject = false
        var dayUsageMap: [Date: AIHeatmapDay] = [:]
        var timeBucketMap: [Date: AITimeBucket] = [:]
        var buffer = Data()
        var currentOffset: UInt64 = 0
        while true {
            let chunk = try? handle.read(upToCount: 64 * 1024)
            guard let chunk, !chunk.isEmpty else { break }
            buffer.append(chunk)
            currentOffset += UInt64(chunk.count)

            while let newlineRange = buffer.range(of: Data([0x0A])) {
                let lineData = buffer.subdata(in: 0..<newlineRange.lowerBound)
                buffer.removeSubrange(0...newlineRange.lowerBound)
                guard !lineData.isEmpty else { continue }
                processCodexLine(
                    lineData,
                    project: project,
                    matchedProject: &matchedProject,
                    sessionID: &sessionID,
                    title: &title,
                    derivedTitle: &derivedTitle,
                    model: &model,
                    firstSeenAt: &firstSeenAt,
                    lastSeenAt: &lastSeenAt,
                    totalTokens: &totalTokens,
                    lastTokenTotal: &lastTokenTotal,
                    dayUsageMap: &dayUsageMap,
                    timeBucketMap: &timeBucketMap
                )
            }
        }

        if !buffer.isEmpty {
            processCodexLine(
                buffer,
                project: project,
                matchedProject: &matchedProject,
                sessionID: &sessionID,
                title: &title,
                derivedTitle: &derivedTitle,
                model: &model,
                firstSeenAt: &firstSeenAt,
                lastSeenAt: &lastSeenAt,
                totalTokens: &totalTokens,
                lastTokenTotal: &lastTokenTotal,
                dayUsageMap: &dayUsageMap,
                timeBucketMap: &timeBucketMap
            )
        }

        guard matchedProject,
              let resolvedSessionID = sessionID,
              let resolvedFirstSeenAt = firstSeenAt,
              let resolvedLastSeenAt = lastSeenAt else {
            return nil
        }

        let session = AISessionSummary(
            sessionID: resolvedSessionID,
            externalSessionID: resolvedSessionID.uuidString,
            projectID: project.id,
            projectName: project.name,
            sessionTitle: codexSessionTitle(sessionID: resolvedSessionID, inlineTitle: title, derivedTitle: derivedTitle, threadMetadata: threadMetadata) ?? project.name,
            firstSeenAt: resolvedFirstSeenAt,
            lastSeenAt: resolvedLastSeenAt,
            lastTool: "codex",
            lastModel: model,
            requestCount: max(dayUsageMap.values.reduce(0) { $0 + $1.requestCount }, 1),
            totalInputTokens: totalTokens,
            totalOutputTokens: 0,
            totalTokens: totalTokens,
            maxContextUsagePercent: nil,
            activeDurationSeconds: max(0, Int(resolvedLastSeenAt.timeIntervalSince(resolvedFirstSeenAt))),
            todayTokens: todayTokenTotal(from: dayUsageMap)
        )

        return AIExternalFileSummary(
            source: codexSource,
            filePath: fileURL.path,
            fileModifiedAt: modifiedAt,
            projectPath: project.path,
            sessions: [session],
            dayUsage: dayUsageMap.values.sorted { $0.day < $1.day },
            timeBuckets: timeBucketMap.values.sorted { $0.start < $1.start },
            codexState: AICodexIncrementalState(
                processedOffset: currentOffset - UInt64(buffer.count),
                pendingData: buffer,
                sessionID: resolvedSessionID,
                sessionTitle: title,
                model: model,
                firstSeenAt: resolvedFirstSeenAt,
                lastSeenAt: resolvedLastSeenAt,
                totalTokens: totalTokens,
                lastTokenTotal: lastTokenTotal,
                matchedProject: matchedProject
            )
        )
    }

    private func loadClaudeSource(project: Project) async -> AISourceLoadResult {
        let files = claudeProjectLogURLs()
        guard !files.isEmpty else {
            return AISourceLoadResult(sessions: [], summaries: [])
        }

        var map: [String: AISessionSummary] = [:]
        var dayUsageMap: [Date: AIHeatmapDay] = [:]
        var timeBucketMap: [Date: AITimeBucket] = [:]
        var modifiedAt = 0.0

        for file in files {
            modifiedAt = max(modifiedAt, ((try? file.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast).timeIntervalSince1970)
            guard let text = try? String(contentsOf: file, encoding: .utf8) else {
                continue
            }

            for line in text.split(separator: "\n") {
                guard let data = line.data(using: .utf8),
                      let row = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let cwd = row["cwd"] as? String,
                      cwd == project.path,
                      let sessionIDString = row["sessionId"] as? String else {
                    continue
                }

                let message = row["message"] as? [String: Any] ?? [:]
                let usage = message["usage"] as? [String: Any] ?? [:]
                let model = message["model"] as? String
                let input = (usage["input_tokens"] as? NSNumber)?.intValue ?? 0
                let output = (usage["output_tokens"] as? NSNumber)?.intValue ?? 0
                let cacheCreation = (usage["cache_creation_input_tokens"] as? NSNumber)?.intValue ?? 0
                let cacheRead = (usage["cache_read_input_tokens"] as? NSNumber)?.intValue ?? 0
                let total = input + output + cacheCreation + cacheRead
                let timestamp = (row["timestamp"] as? String).flatMap(parseISO8601Date) ?? unknownSessionDate
                let sessionID = UUID(uuidString: sessionIDString) ?? UUID()
                let userTitleCandidate = claudeUserMessageTitle(from: row)
                let slugTitleCandidate = normalizeClaudeTitle(row["slug"] as? String)
                let titleCandidate = userTitleCandidate ?? slugTitleCandidate
                accumulateUsage(date: timestamp, tokens: total, requests: 1, dayUsageMap: &dayUsageMap, timeBucketMap: &timeBucketMap)

                if var existing = map[sessionIDString] {
                    existing.totalInputTokens += input + cacheCreation + cacheRead
                    existing.totalOutputTokens += output
                    existing.totalTokens += total
                    existing.lastSeenAt = maxSessionDate(existing.lastSeenAt, timestamp)
                    existing.lastModel = existing.lastModel ?? model
                    if let userTitleCandidate {
                        existing.sessionTitle = userTitleCandidate
                    } else if let slugTitleCandidate, existing.sessionTitle == project.name {
                        existing.sessionTitle = slugTitleCandidate
                    } else if let titleCandidate, existing.sessionTitle == project.name {
                        existing.sessionTitle = titleCandidate
                    }
                    existing.todayTokens += isKnownSessionDate(timestamp) && isToday(timestamp) ? total : 0
                    map[sessionIDString] = existing
                } else {
                    map[sessionIDString] = AISessionSummary(
                        sessionID: sessionID,
                        externalSessionID: sessionIDString,
                        projectID: project.id,
                        projectName: project.name,
                        sessionTitle: titleCandidate ?? project.name,
                        firstSeenAt: timestamp,
                        lastSeenAt: timestamp,
                        lastTool: "claude",
                        lastModel: model,
                        requestCount: 1,
                        totalInputTokens: input + cacheCreation + cacheRead,
                        totalOutputTokens: output,
                        totalTokens: total,
                        maxContextUsagePercent: nil,
                        activeDurationSeconds: 0,
                        todayTokens: isKnownSessionDate(timestamp) && isToday(timestamp) ? total : 0
                    )
                }
            }
        }

        let sessions = sortSessions(Array(map.values))
        let summary = AIExternalFileSummary(
            source: "claude-jsonl",
            filePath: URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".claude/projects").path,
            fileModifiedAt: modifiedAt,
            projectPath: project.path,
            sessions: sessions,
            dayUsage: dayUsageMap.values.sorted { $0.day < $1.day },
            timeBuckets: mergeCachedTimeBuckets(timeBucketMap),
            codexState: nil
        )
        wrapperStore.saveExternalSummary(summary)
        return AISourceLoadResult(sessions: sessions, summaries: [summary])
    }

    private func loadGeminiSource(project: Project) async -> AISourceLoadResult {
        let sessions = loadGeminiSessions(project: project)
        guard !sessions.isEmpty else {
            return AISourceLoadResult(sessions: [], summaries: [])
        }

        var dayUsageMap: [Date: AIHeatmapDay] = [:]
        var timeBucketMap: [Date: AITimeBucket] = [:]
        var modifiedAt = 0.0

        for session in sessions {
            if let knownLastSeenAt = knownSessionDate(session.lastSeenAt), session.totalTokens > 0 {
                accumulateUsage(
                    date: knownLastSeenAt,
                    tokens: session.totalTokens,
                    requests: max(1, session.requestCount),
                    dayUsageMap: &dayUsageMap,
                    timeBucketMap: &timeBucketMap
                )
            }
            modifiedAt = max(modifiedAt, session.lastSeenAt.timeIntervalSince1970)
        }

        let summary = AIExternalFileSummary(
            source: "gemini-chats",
            filePath: AIRuntimeSourceLocator.geminiChatsDirectoryURL(projectPath: project.path)?.path ?? "gemini://\(project.id.uuidString)",
            fileModifiedAt: modifiedAt,
            projectPath: project.path,
            sessions: sortSessions(sessions),
            dayUsage: dayUsageMap.values.sorted { $0.day < $1.day },
            timeBuckets: mergeCachedTimeBuckets(timeBucketMap),
            codexState: nil
        )
        return AISourceLoadResult(sessions: summary.sessions, summaries: [summary])
    }

    private func loadGeminiSessions(project: Project) -> [AISessionSummary] {
        let fileURLs = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: project.path)
        guard !fileURLs.isEmpty else {
            return []
        }

        var sessions: [AISessionSummary] = []
        for fileURL in fileURLs {
            guard let state = parseGeminiSessionRuntimeState(fileURL: fileURL) else {
                continue
            }

            let fileDate = ((try? fileURL.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
            let startedAt = max(Date.distantPast, Date(timeIntervalSince1970: state.startedAt))
            let updatedAt = max(Date(timeIntervalSince1970: state.updatedAt), fileDate)
            sessions.append(
                AISessionSummary(
                    sessionID: UUID(uuidString: state.externalSessionID) ?? deterministicUUID(from: fileURL.path),
                    externalSessionID: state.externalSessionID,
                    projectID: project.id,
                    projectName: project.name,
                    sessionTitle: state.title ?? project.name,
                    firstSeenAt: startedAt == .distantPast ? fileDate : startedAt,
                    lastSeenAt: updatedAt,
                    lastTool: "gemini",
                    lastModel: state.model,
                    requestCount: 1,
                    totalInputTokens: state.inputTokens,
                    totalOutputTokens: state.outputTokens,
                    totalTokens: state.totalTokens,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: activeDuration(
                        firstSeenAt: startedAt == .distantPast ? fileDate : startedAt,
                        lastSeenAt: updatedAt
                    ),
                    todayTokens: isToday(updatedAt) ? state.totalTokens : 0
                )
            )
        }
        return sortSessions(sessions)
    }

    private func claudeProjectLogURLs() -> [URL] {
        let baseURL = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".claude/projects", isDirectory: true)
        guard let enumerator = FileManager.default.enumerator(at: baseURL, includingPropertiesForKeys: nil) else {
            return []
        }

        var urls: [URL] = []
        while let next = enumerator.nextObject() as? URL {
            if next.pathExtension == "jsonl" {
                urls.append(next)
            }
        }
        return urls
    }

    private func processCodexLine(
        _ data: Data,
        project: Project,
        matchedProject: inout Bool,
        sessionID: inout UUID?,
        title: inout String?,
        derivedTitle: inout String?,
        model: inout String?,
        firstSeenAt: inout Date?,
        lastSeenAt: inout Date?,
        totalTokens: inout Int,
        lastTokenTotal: inout Int,
        dayUsageMap: inout [Date: AIHeatmapDay],
        timeBucketMap: inout [Date: AITimeBucket]
    ) {
        guard let row = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let timestampString = row["timestamp"] as? String,
              let timestamp = parseISO8601Date(timestampString) else {
            return
        }

        let payload = row["payload"] as? [String: Any] ?? [:]

        if row["type"] as? String == "session_meta", let cwd = payload["cwd"] as? String, cwd == project.path {
            matchedProject = true
            if let id = payload["id"] as? String {
                sessionID = UUID(uuidString: id) ?? UUID()
            }
            title = payload["thread_name"] as? String ?? payload["title"] as? String ?? title
            firstSeenAt = firstSeenAt ?? timestamp
            lastSeenAt = max(lastSeenAt ?? timestamp, timestamp)
        }

        if row["type"] as? String == "turn_context", let cwd = payload["cwd"] as? String, cwd == project.path {
            matchedProject = true
            if let rawModel = payload["model"] as? String, !rawModel.isEmpty {
                model = rawModel
            }
            firstSeenAt = firstSeenAt ?? timestamp
            lastSeenAt = max(lastSeenAt ?? timestamp, timestamp)
        }

        if row["type"] as? String == "response_item", matchedProject {
            if let candidate = codexResponseTitle(from: payload), derivedTitle == nil {
                derivedTitle = candidate
            }
            firstSeenAt = firstSeenAt ?? timestamp
            lastSeenAt = max(lastSeenAt ?? timestamp, timestamp)
        }

        if row["type"] as? String == "event_msg", payload["type"] as? String == "token_count", matchedProject {
            let info = payload["info"] as? [String: Any] ?? [:]
            let totalUsage = info["total_token_usage"] as? [String: Any] ?? [:]
            let input = ((totalUsage["input_tokens"] as? NSNumber)?.intValue ?? 0)
                + ((totalUsage["cached_input_tokens"] as? NSNumber)?.intValue ?? 0)
            let output = ((totalUsage["output_tokens"] as? NSNumber)?.intValue ?? 0)
                + ((totalUsage["reasoning_output_tokens"] as? NSNumber)?.intValue ?? 0)
            let total = (totalUsage["total_tokens"] as? NSNumber)?.intValue
                ?? (info["total_tokens"] as? NSNumber)?.intValue
                ?? (input + output)
            if total > lastTokenTotal {
                let delta = total - lastTokenTotal
                totalTokens += delta
                lastTokenTotal = total

                let day = calendar.startOfDay(for: timestamp)
                if var existing = dayUsageMap[day] {
                    existing.totalTokens += delta
                    existing.requestCount += 1
                    dayUsageMap[day] = existing
                } else {
                    dayUsageMap[day] = AIHeatmapDay(day: day, totalTokens: delta, requestCount: 1)
                }

                let bucketStart = bucketStartDate(for: timestamp)
                let bucketEnd = calendar.date(byAdding: .minute, value: 30, to: bucketStart) ?? bucketStart
                if var existing = timeBucketMap[bucketStart] {
                    existing.totalTokens += delta
                    existing.requestCount += 1
                    timeBucketMap[bucketStart] = existing
                } else {
                    timeBucketMap[bucketStart] = AITimeBucket(start: bucketStart, end: bucketEnd, totalTokens: delta, requestCount: 1)
                }
            }
            firstSeenAt = firstSeenAt ?? timestamp
            lastSeenAt = max(lastSeenAt ?? timestamp, timestamp)
        }
    }

    private func codexResponseTitle(from payload: [String: Any]) -> String? {
        guard payload["type"] as? String == "message",
              payload["role"] as? String == "user",
              let content = payload["content"] as? [[String: Any]] else {
            return nil
        }

        for item in content {
            guard let text = item["text"] as? String else {
                continue
            }
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty || trimmed.contains("<environment_context>") {
                continue
            }
            return normalizeCodexTitle(trimmed)
        }
        return nil
    }

    private func normalizeCodexTitle(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return nil
        }
        return String(trimmed.prefix(80))
    }

    private func codexSessionTitle(sessionID: UUID, inlineTitle: String?, derivedTitle: String?, threadMetadata: CodexThreadMetadata) -> String? {
        let key = sessionID.uuidString.lowercased()
        return normalizeCodexTitle(threadMetadata.titleByID[key])
            ?? normalizeCodexTitle(threadMetadata.firstUserMessageByID[key])
            ?? normalizeCodexTitle(inlineTitle)
            ?? normalizeCodexTitle(derivedTitle)
    }

    private func claudeUserMessageTitle(from row: [String: Any]) -> String? {
        guard row["type"] as? String == "user",
              let message = row["message"] as? [String: Any] else {
            return nil
        }

        if let content = message["content"] as? String {
            return normalizeClaudeTitle(content)
        }

        if let content = message["content"] as? [[String: Any]] {
            for item in content {
                if let text = item["text"] as? String,
                   let normalized = normalizeClaudeTitle(text) {
                    return normalized
                }
            }
        }

        return nil
    }

    private func normalizeClaudeTitle(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return nil
        }
        return String(trimmed.prefix(80))
    }

    private func todayTokenTotal(from dayUsageMap: [Date: AIHeatmapDay]) -> Int {
        let today = calendar.startOfDay(for: Date())
        return dayUsageMap[today]?.totalTokens ?? 0
    }

    private func todayTotalTokens(timeBuckets: [AITimeBucket], heatmap: [AIHeatmapDay]) -> Int {
        let bucketTotal = timeBuckets.reduce(0) { $0 + $1.totalTokens }
        if bucketTotal > 0 {
            return bucketTotal
        }

        let today = calendar.startOfDay(for: Date())
        return heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) })?.totalTokens ?? 0
    }

    private func loadCodexThreadMetadata(projectPath: String) -> CodexThreadMetadata {
        let dbPath = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex/state_5.sqlite").path
        guard FileManager.default.fileExists(atPath: dbPath) else {
            return CodexThreadMetadata(titleByID: [:], firstUserMessageByID: [:], rolloutPaths: [])
        }

        var db: OpaquePointer?
        guard sqlite3_open(dbPath, &db) == SQLITE_OK, let db else {
            return CodexThreadMetadata(titleByID: [:], firstUserMessageByID: [:], rolloutPaths: [])
        }
        defer { sqlite3_close(db) }

        let sql = "SELECT id, title, first_user_message, rollout_path FROM threads WHERE cwd = ? AND archived = 0;"
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            return CodexThreadMetadata(titleByID: [:], firstUserMessageByID: [:], rolloutPaths: [])
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, projectPath, -1, SQLITE_TRANSIENT)

        var titles: [String: String] = [:]
        var firstMessages: [String: String] = [:]
        var rolloutPaths: [URL] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let id = sqlite3_column_text(statement, 0) else {
                continue
            }
            let key = String(cString: id).lowercased()
            if let rawTitle = sqlite3_column_text(statement, 1) {
                titles[key] = String(cString: rawTitle)
            }
            if let rawMessage = sqlite3_column_text(statement, 2) {
                firstMessages[key] = String(cString: rawMessage)
            }
            if let rawPath = sqlite3_column_text(statement, 3) {
                rolloutPaths.append(URL(fileURLWithPath: String(cString: rawPath)))
            }
        }

        return CodexThreadMetadata(titleByID: titles, firstUserMessageByID: firstMessages, rolloutPaths: rolloutPaths)
    }

    private func buildHeatmap(from summaries: [AIExternalFileSummary], fallbackSessions: [AISessionSummary]) -> [AIHeatmapDay] {
        let merged = mergeDayUsage(from: summaries)
        if !merged.isEmpty {
            return merged
        }

        var byDay: [Date: AIHeatmapDay] = [:]
        for session in fallbackSessions {
            guard let lastSeenAt = knownSessionDate(session.lastSeenAt) else {
                continue
            }
            let day = calendar.startOfDay(for: lastSeenAt)
            if var existing = byDay[day] {
                existing.totalTokens += session.totalTokens
                existing.requestCount += session.requestCount
                byDay[day] = existing
            } else {
                byDay[day] = AIHeatmapDay(day: day, totalTokens: session.totalTokens, requestCount: session.requestCount)
            }
        }
        return byDay.values.sorted { $0.day < $1.day }
    }

    private func breakdown(items: [(String, Int, Int)]) -> [AIUsageBreakdownItem] {
        var map: [String: AIUsageBreakdownItem] = [:]
        for item in items {
            if var existing = map[item.0] {
                existing.totalTokens += item.1
                existing.requestCount += item.2
                map[item.0] = existing
            } else {
                map[item.0] = AIUsageBreakdownItem(key: item.0, totalTokens: item.1, requestCount: item.2)
            }
        }
        return map.values.sorted { $0.totalTokens > $1.totalTokens }
    }

    private func accumulateUsage(
        date: Date,
        tokens: Int,
        requests: Int,
        dayUsageMap: inout [Date: AIHeatmapDay],
        timeBucketMap: inout [Date: AITimeBucket]
    ) {
        let day = calendar.startOfDay(for: date)
        if var existing = dayUsageMap[day] {
            existing.totalTokens += tokens
            existing.requestCount += requests
            dayUsageMap[day] = existing
        } else {
            dayUsageMap[day] = AIHeatmapDay(day: day, totalTokens: tokens, requestCount: requests)
        }

        let bucketStart = bucketStartDate(for: date)
        let bucketEnd = calendar.date(byAdding: .minute, value: 30, to: bucketStart) ?? bucketStart
        if var existing = timeBucketMap[bucketStart] {
            existing.totalTokens += tokens
            existing.requestCount += requests
            timeBucketMap[bucketStart] = existing
        } else {
            timeBucketMap[bucketStart] = AITimeBucket(start: bucketStart, end: bucketEnd, totalTokens: tokens, requestCount: requests)
        }
    }

    private func mergeCachedTimeBuckets(_ timeBucketMap: [Date: AITimeBucket]) -> [AITimeBucket] {
        let startOfDay = calendar.startOfDay(for: Date())
        return stride(from: 0, to: 24 * 60, by: 30).map { minuteOffset in
            let bucketStart = calendar.date(byAdding: .minute, value: minuteOffset, to: startOfDay)!
            let bucketEnd = calendar.date(byAdding: .minute, value: 30, to: bucketStart)!
            return timeBucketMap[bucketStart] ?? AITimeBucket(start: bucketStart, end: bucketEnd, totalTokens: 0, requestCount: 0)
        }
    }

    private func mergeDayUsage(from summaries: [AIExternalFileSummary]) -> [AIHeatmapDay] {
        var map: [Date: AIHeatmapDay] = [:]
        for summary in summaries {
            for item in summary.dayUsage {
                if var existing = map[item.day] {
                    existing.totalTokens += item.totalTokens
                    existing.requestCount += item.requestCount
                    map[item.day] = existing
                } else {
                    map[item.day] = item
                }
            }
        }
        return map.values.sorted { $0.day < $1.day }
    }

    private func mergeTimeBuckets(from summaries: [AIExternalFileSummary]) -> [AITimeBucket] {
        var map: [Date: AITimeBucket] = [:]
        for summary in summaries {
            for bucket in summary.timeBuckets {
                if var existing = map[bucket.start] {
                    existing.totalTokens += bucket.totalTokens
                    existing.requestCount += bucket.requestCount
                    map[bucket.start] = existing
                } else {
                    map[bucket.start] = bucket
                }
            }
        }
        return mergeCachedTimeBuckets(map)
    }

    private func isToday(_ date: Date) -> Bool {
        calendar.isDateInToday(date)
    }

    private func buildTodayTimeBuckets(from summaries: [AIExternalFileSummary]) -> [AITimeBucket] {
        let merged = mergeTimeBuckets(from: summaries)
        let hasData = merged.contains { $0.totalTokens > 0 || $0.requestCount > 0 }
        if hasData {
            return merged
        }

        let startOfDay = calendar.startOfDay(for: Date())
        return stride(from: 0, to: 24 * 60, by: 30).map { minuteOffset in
            let bucketStart = calendar.date(byAdding: .minute, value: minuteOffset, to: startOfDay)!
            let bucketEnd = calendar.date(byAdding: .minute, value: 30, to: bucketStart)!
            return AITimeBucket(start: bucketStart, end: bucketEnd, totalTokens: 0, requestCount: 0)
        }
    }

    private func bucketStartDate(for date: Date) -> Date {
        let components = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        let roundedMinute = ((components.minute ?? 0) / 30) * 30
        return calendar.date(from: DateComponents(
            year: components.year,
            month: components.month,
            day: components.day,
            hour: components.hour,
            minute: roundedMinute
        )) ?? date
    }

    private func text(at index: Int32, _ statement: OpaquePointer) -> String? {
        guard let value = sqlite3_column_text(statement, index) else { return nil }
        return String(cString: value)
    }

    private func int(at index: Int32, _ statement: OpaquePointer) -> Int {
        Int(sqlite3_column_int64(statement, index))
    }

    private func isKnownSessionDate(_ date: Date) -> Bool {
        date.timeIntervalSince1970 > 0
    }

    private func knownSessionDate(_ date: Date) -> Date? {
        isKnownSessionDate(date) ? date : nil
    }

    private func maxSessionDate(_ lhs: Date, _ rhs: Date) -> Date {
        switch (isKnownSessionDate(lhs), isKnownSessionDate(rhs)) {
        case (true, true):
            return max(lhs, rhs)
        case (true, false):
            return lhs
        case (false, true):
            return rhs
        case (false, false):
            return unknownSessionDate
        }
    }

    private func activeDuration(firstSeenAt: Date, lastSeenAt: Date) -> Int {
        guard let resolvedFirstSeenAt = knownSessionDate(firstSeenAt),
              let resolvedLastSeenAt = knownSessionDate(lastSeenAt) else {
            return 0
        }
        return max(0, Int(resolvedLastSeenAt.timeIntervalSince(resolvedFirstSeenAt)))
    }

    private func sortSessions(_ sessions: [AISessionSummary]) -> [AISessionSummary] {
        sessions.sorted { lhs, rhs in
            switch (knownSessionDate(lhs.lastSeenAt), knownSessionDate(rhs.lastSeenAt)) {
            case let (lhsDate?, rhsDate?):
                return lhsDate > rhsDate
            case (.some, nil):
                return true
            case (nil, .some):
                return false
            case (nil, nil):
                return lhs.sessionTitle.localizedStandardCompare(rhs.sessionTitle) == .orderedAscending
            }
        }
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
}

private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
