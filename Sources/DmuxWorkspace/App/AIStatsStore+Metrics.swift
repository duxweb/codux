import Darwin
import Foundation

@MainActor
extension AIStatsStore {
    func titlebarTodayLevelTokens() -> Int {
        clampedAdd(aiUsageStore.globalTodayNormalizedTokens(), titlebarTodayLiveOverlayTokens)
    }

    @discardableResult
    func refreshTitlebarTodayLiveOverlay(notify: Bool = true) -> Bool {
        let previousBaselineDay = titlebarLiveOverlayBaselineDay
        let previousTotalBaselines = titlebarLiveOverlayTotalBaselines
        let previousCachedBaselines = titlebarLiveOverlayCachedInputBaselines
        let previousTokens = titlebarTodayLiveOverlayTokens
        let previousCachedTokens = titlebarTodayLiveOverlayCachedInputTokens

        let overlay = globalTodayLiveOverlayTotals(indexedSessions: aiUsageStore.indexedSessions(since: nil))
        titlebarTodayLiveOverlayTokens = overlay.tokens
        titlebarTodayLiveOverlayCachedInputTokens = overlay.cachedInputTokens

        let didChange = previousBaselineDay != titlebarLiveOverlayBaselineDay
            || previousTotalBaselines != titlebarLiveOverlayTotalBaselines
            || previousCachedBaselines != titlebarLiveOverlayCachedInputBaselines
            || previousTokens != titlebarTodayLiveOverlayTokens
            || previousCachedTokens != titlebarTodayLiveOverlayCachedInputTokens

        if didChange, notify {
            renderVersion &+= 1
        }
        return didChange
    }

    func normalizedTokenTotalsForPet(_ projects: [Project], claimedAt: Date?) -> [UUID: Int] {
        guard let claimedAt else {
            return [:]
        }
        let projectIDs = Set(projects.map(\.id))
        guard !projectIDs.isEmpty else {
            return [:]
        }

        let indexedSessions = aiUsageStore.indexedSessions(since: claimedAt, projectIDs: projectIDs)
        var totalsByProject = indexedSessions
            .reduce(into: [UUID: Int]()) { partial, session in
                partial[session.projectID] = clampedAdd(partial[session.projectID] ?? 0, session.totalTokens)
            }

        let liveTotals = liveOverlayTotalTokensForPet(
            projectIDs: projectIDs,
            claimedAt: claimedAt,
            indexedSessions: indexedSessions
        )
        for (projectID, liveTotal) in liveTotals {
            totalsByProject[projectID] = clampedAdd(totalsByProject[projectID] ?? 0, liveTotal)
        }

        return totalsByProject
    }

    func totalNormalizedTokensForPet(_ projects: [Project], claimedAt: Date?) -> Int {
        normalizedTokenTotalsForPet(projects, claimedAt: claimedAt).values.reduce(0) { partial, total in
            clampedAdd(partial, total)
        }
    }

    func petStatsSinceClaimedAt(_ claimedAt: Date?, projects: [Project]) -> PetStats {
        guard let claimedAt else {
            return .neutral
        }
        let projectIDs = Set(projects.map(\.id))
        guard !projectIDs.isEmpty else {
            return .neutral
        }
        return Self.computePetStats(
            from: aiUsageStore.indexedSessions(since: claimedAt, projectIDs: projectIDs)
        )
    }

    func totalTodayNormalizedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            if let liveOrCached = cachedState(for: project.id) {
                let liveOverlay = liveOverlayTotals(
                    projectID: project.id,
                    indexedSessions: liveOrCached.sessions,
                    state: liveOrCached
                )
                return clampedAdd(
                    partial,
                    clampedAdd(historicalTodayBase(from: liveOrCached), liveOverlay.tokens)
                )
            }

            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                let liveOverlay = liveOverlayTotals(projectID: project.id, indexedSessions: indexed.sessions)
                return clampedAdd(
                    partial,
                    clampedAdd(
                        resolvedTodayTotalTokens(
                            summary: indexed.projectSummary.todayTotalTokens,
                            timeBuckets: indexed.todayTimeBuckets,
                            heatmap: indexed.heatmap
                        ),
                        liveOverlay.tokens
                    )
                )
            }

            let liveOverlay = liveOverlayTotals(projectID: project.id)
            return clampedAdd(partial, liveOverlay.tokens)
        }
    }

    func totalTodayDisplayedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            if let liveOrCached = cachedState(for: project.id) {
                let liveOverlay = liveOverlayTotals(
                    projectID: project.id,
                    indexedSessions: liveOrCached.sessions,
                    state: liveOrCached
                )
                return clampedAdd(
                    partial,
                    clampedAdd(
                        historicalDisplayedTodayBase(from: liveOrCached),
                        clampedAdd(liveOverlay.tokens, liveOverlay.cachedInputTokens)
                    )
                )
            }

            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                let liveOverlay = liveOverlayTotals(projectID: project.id, indexedSessions: indexed.sessions)
                return clampedAdd(
                    partial,
                    clampedAdd(
                        resolvedDisplayedTodayTotalTokens(
                            summary: indexed.projectSummary.todayTotalTokens,
                            summaryCached: indexed.projectSummary.todayCachedInputTokens,
                            timeBuckets: indexed.todayTimeBuckets,
                            heatmap: indexed.heatmap
                        ),
                        clampedAdd(liveOverlay.tokens, liveOverlay.cachedInputTokens)
                    )
                )
            }

            let liveOverlay = liveOverlayTotals(projectID: project.id)
            return clampedAdd(partial, clampedAdd(liveOverlay.tokens, liveOverlay.cachedInputTokens))
        }
    }

    func totalAllTimeNormalizedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            if let liveOrCached = cachedState(for: project.id) {
                let liveOverlay = liveOverlayTotals(
                    projectID: project.id,
                    indexedSessions: liveOrCached.sessions,
                    state: liveOrCached,
                    useTodayBaseline: false
                )
                return clampedAdd(
                    partial,
                    clampedAdd(historicalAllTimeBase(from: liveOrCached), liveOverlay.tokens)
                )
            }

            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                let liveOverlay = liveOverlayTotals(
                    projectID: project.id,
                    indexedSessions: indexed.sessions,
                    useTodayBaseline: false
                )
                let indexedTotal = max(
                    indexed.projectSummary.projectTotalTokens,
                    indexed.sessions.reduce(0) { clampedAdd($0, $1.totalTokens) }
                )
                return clampedAdd(partial, clampedAdd(indexedTotal, liveOverlay.tokens))
            }
            let liveOverlay = liveOverlayTotals(projectID: project.id, useTodayBaseline: false)
            return clampedAdd(partial, liveOverlay.tokens)
        }
    }

    private func liveOverlayTotals(
        projectID: UUID,
        indexedSessions: [AISessionSummary] = [],
        state: AIStatsPanelState? = nil,
        useTodayBaseline: Bool = true
    ) -> (tokens: Int, cachedInputTokens: Int) {
        let overlay = AIUsageLiveOverlayCalculator().calculate(
            snapshots: aiSessionStore.liveAggregationSnapshots(projectID: projectID),
            indexedSessions: indexedSessions,
            existingBaselines: AIUsageLiveOverlayBaselines(
                day: state?.liveOverlayBaselineDay,
                totalTokensBySessionKey: state?.liveOverlayTotalBaselines ?? [:],
                cachedInputTokensBySessionKey: state?.liveOverlayCachedInputBaselines ?? [:]
            ),
            useTodayBaseline: useTodayBaseline
        )
        if useTodayBaseline {
            return (tokens: overlay.todayTokens, cachedInputTokens: overlay.todayCachedInputTokens)
        }
        return (tokens: overlay.totalTokens, cachedInputTokens: overlay.cachedInputTokens)
    }

    private func liveOverlayTotalTokensForPet(
        projectIDs: Set<UUID>,
        claimedAt: Date,
        indexedSessions: [AISessionSummary]
    ) -> [UUID: Int] {
        AIUsageLiveOverlayCalculator().liveTotalTokensForPet(
            snapshots: aiSessionStore.globalLiveAggregationSnapshots(),
            projectIDs: projectIDs,
            claimedAt: claimedAt,
            indexedSessions: indexedSessions
        )
    }

    private func globalTodayLiveOverlayTotals(
        indexedSessions: [AISessionSummary]
    ) -> (tokens: Int, cachedInputTokens: Int) {
        let overlay = AIUsageLiveOverlayCalculator().calculate(
            snapshots: aiSessionStore.globalLiveAggregationSnapshots(),
            indexedSessions: indexedSessions,
            existingBaselines: AIUsageLiveOverlayBaselines(
                day: titlebarLiveOverlayBaselineDay,
                totalTokensBySessionKey: titlebarLiveOverlayTotalBaselines,
                cachedInputTokensBySessionKey: titlebarLiveOverlayCachedInputBaselines
            )
        )
        titlebarLiveOverlayBaselineDay = overlay.baselines.day
        titlebarLiveOverlayTotalBaselines = overlay.baselines.totalTokensBySessionKey
        titlebarLiveOverlayCachedInputBaselines = overlay.baselines.cachedInputTokensBySessionKey
        return (tokens: overlay.todayTokens, cachedInputTokens: overlay.todayCachedInputTokens)
    }

    private func historicalAllTimeBase(from state: AIStatsPanelState) -> Int {
        let summaryBase = max(0, (state.projectSummary?.projectTotalTokens ?? 0) - max(0, state.liveOverlayTokens))
        let sessionBase = state.sessions.reduce(0) { partial, session in
            clampedAdd(partial, session.totalTokens)
        }
        return max(summaryBase, sessionBase)
    }

    private func historicalTodayBase(from state: AIStatsPanelState) -> Int {
        let summary = staleCachedTodayEvidence(in: state)
            ? 0
            : max(0, (state.projectSummary?.todayTotalTokens ?? 0) - historicalLiveTodayOverlayTokens(from: state))
        return resolvedTodayTotalTokens(
            summary: summary,
            timeBuckets: state.todayTimeBuckets,
            heatmap: state.heatmap
        )
    }

    private func historicalDisplayedTodayBase(from state: AIStatsPanelState) -> Int {
        let staleToday = staleCachedTodayEvidence(in: state)
        let summary = staleToday
            ? 0
            : max(0, (state.projectSummary?.todayTotalTokens ?? 0) - historicalLiveTodayOverlayTokens(from: state))
        let summaryCached = staleToday
            ? 0
            : max(
                0,
                (state.projectSummary?.todayCachedInputTokens ?? 0) - historicalLiveTodayOverlayCachedInputTokens(from: state)
            )
        return resolvedDisplayedTodayTotalTokens(
            summary: summary,
            summaryCached: summaryCached,
            timeBuckets: state.todayTimeBuckets,
            heatmap: state.heatmap
        )
    }

    private func staleCachedTodayEvidence(in state: AIStatsPanelState) -> Bool {
        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        var hasDatedEvidence = false

        for bucket in state.todayTimeBuckets {
            hasDatedEvidence = true
            if calendar.isDate(bucket.start, inSameDayAs: today) {
                return false
            }
        }
        for day in state.heatmap {
            hasDatedEvidence = true
            if calendar.isDate(day.day, inSameDayAs: today) {
                return false
            }
        }
        if let updatedAt = state.projectSummary?.currentSessionUpdatedAt {
            hasDatedEvidence = true
            if calendar.isDate(updatedAt, inSameDayAs: today) {
                return false
            }
        }
        if let indexedAt = state.indexedAt {
            hasDatedEvidence = true
            if calendar.isDate(indexedAt, inSameDayAs: today) {
                return false
            }
        }

        return hasDatedEvidence
    }

    private func historicalLiveTodayOverlayTokens(from state: AIStatsPanelState) -> Int {
        if state.liveOverlayBaselineDay != nil {
            return max(0, state.liveTodayOverlayTokens)
        }
        return max(0, state.liveOverlayTokens)
    }

    private func historicalLiveTodayOverlayCachedInputTokens(from state: AIStatsPanelState) -> Int {
        if state.liveOverlayBaselineDay != nil {
            return max(0, state.liveTodayOverlayCachedInputTokens)
        }
        return max(0, state.liveOverlayCachedInputTokens)
    }

    private func clampedAdd(_ lhs: Int, _ rhs: Int) -> Int {
        let base = max(0, lhs)
        let increment = max(0, rhs)
        return increment > Int.max - base ? Int.max : base + increment
    }

    static func computePetStats(from sessions: [AISessionSummary]) -> PetStats {
        guard !sessions.isEmpty else { return .neutral }

        let totalRequests = sessions.reduce(0) { $0 + $1.requestCount }
        let totalTokens   = sessions.reduce(0) { $0 + $1.totalTokens }
        let totalSecs     = sessions.reduce(0) { $0 + $1.activeDurationSeconds }
        let sessionCount  = max(1, sessions.count)

        let avgTokPerReq = totalRequests > 0 ? Double(totalTokens) / Double(totalRequests) : 0
        let reqPerHour   = totalSecs > 0 ? Double(totalRequests) / (Double(totalSecs) / 3600.0) : 0
        let shortCount   = sessions.filter { $0.activeDurationSeconds < 300 }.count
        let shortRatio   = Double(shortCount) / Double(sessions.count)
        let nightCount   = sessions.filter {
            let h = Calendar.current.component(.hour, from: $0.firstSeenAt)
            return h >= 22 || h < 6
        }.count
        let nightRatio   = Double(nightCount) / Double(sessionCount)
        let sustainedSessionSeconds = sessions.map { session -> Int in
            let activeSeconds = max(0, session.activeDurationSeconds)
            let wallClockSeconds = max(0, Int(session.lastSeenAt.timeIntervalSince(session.firstSeenAt).rounded()))
            return max(activeSeconds, wallClockSeconds)
        }
        let maxSecs      = sustainedSessionSeconds.max() ?? 0
        let avgSecs      = sustainedSessionSeconds.reduce(0, +) / sessions.count
        let totalSustainedSecs = sustainedSessionSeconds.reduce(0, +)
        let multiTurnSessions = sessions.filter { $0.requestCount >= 4 }
        let multiTurnRatio    = Double(multiTurnSessions.count) / Double(sessionCount)

        let iterativeRepairSessions = sessions.filter { s in
            guard s.requestCount >= 3, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return s.activeDurationSeconds >= 360 && avgPerTurn >= 120 && avgPerTurn <= 4_200
        }
        let repairSecs        = iterativeRepairSessions.reduce(0) { $0 + $1.activeDurationSeconds }
        let repairRatio       = min(1.0, Double(repairSecs) / Double(max(1, totalSecs)))
        let repairTokenBudget = iterativeRepairSessions.reduce(0) { $0 + $1.totalTokens }
        let adjustmentLoopCount = sessions.filter { s in
            guard s.requestCount >= 3, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return s.activeDurationSeconds < 360 && avgPerTurn >= 120 && avgPerTurn <= 3_600
        }.count

        func logPts(_ value: Double, divisor: Double, weight: Double, cap: Double) -> Double {
            guard value > 0, divisor > 0, weight > 0 else { return 0 }
            return min(log1p(value / divisor) * weight, cap)
        }
        func ratioPts(_ value: Double, exponent: Double, weight: Double, cap: Double) -> Double {
            guard value > 0, exponent > 0, weight > 0 else { return 0 }
            return min(pow(value, exponent) * weight, cap)
        }

        let shared = logPts(Double(totalTokens), divisor: 250_000, weight: 16, cap: 20)

        let wisdomScore =
            logPts(avgTokPerReq,       divisor: 400,    weight: 110, cap: 175) +
            logPts(Double(totalSecs),  divisor: 12_000, weight: 12,  cap: 24)  +
            shared

        let chaosScore =
            logPts(reqPerHour,            divisor: 2.2, weight: 92, cap: 138) +
            ratioPts(shortRatio,          exponent: 0.72, weight: 46, cap: 46) +
            logPts(Double(totalRequests), divisor: 26,  weight: 20, cap: 34)  +
            shared

        let nightTokens = Double(totalTokens) * max(0.08, nightRatio)
        let nightScore =
            ratioPts(nightRatio,          exponent: 0.72, weight: 96, cap: 96) +
            logPts(Double(nightCount),    divisor: 4.5,  weight: 22, cap: 42) +
            logPts(nightTokens,           divisor: 150_000, weight: 10, cap: 18) +
            shared * 0.35

        let staminaScore =
            logPts(Double(maxSecs),  divisor: 1_400, weight: 58, cap: 88) +
            logPts(Double(avgSecs),  divisor: 900,   weight: 52, cap: 72) +
            logPts(Double(totalSustainedSecs), divisor: 28_800, weight: 18, cap: 28) +
            logPts(Double(totalRequests), divisor: 18, weight: 14, cap: 20) +
            shared * 0.5

        let empathyScore =
            ratioPts(repairRatio,              exponent: 0.70, weight: 92, cap: 92) +
            ratioPts(multiTurnRatio,           exponent: 0.58, weight: 42, cap: 42) +
            logPts(Double(repairSecs) / 60,    divisor: 900,   weight: 30, cap: 46) +
            logPts(Double(repairTokenBudget),  divisor: 150_000, weight: 18, cap: 30) +
            logPts(Double(adjustmentLoopCount),divisor: 2.4,   weight: 14, cap: 20) +
            shared * 0.5

        return PetStats(
            wisdom:  max(0, Int(wisdomScore.rounded())),
            chaos:   max(0, Int(chaosScore.rounded())),
            night:   max(0, Int(nightScore.rounded())),
            stamina: max(0, Int(staminaScore.rounded())),
            empathy: max(0, Int(empathyScore.rounded()))
        )
    }

    func hiddenPetSpeciesChanceAcrossProjects(_ projects: [Project]) -> Double {
        let cutoff = Date().addingTimeInterval(-7 * 86400)
        var toolTotals: [String: Int] = [:]

        for project in projects {
            guard let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) else { continue }
            for session in indexed.sessions where session.lastSeenAt >= cutoff {
                guard let normalizedTool = Self.normalizedPetToolName(session.lastTool) else {
                    continue
                }
                toolTotals[normalizedTool, default: 0] += session.totalTokens
            }
        }

        return Self.hiddenPetSpeciesChance(forToolTotals: toolTotals)
    }

    static func hiddenPetSpeciesChance(forToolTotals toolTotals: [String: Int]) -> Double {
        toolTotals.keys.count >= 2 ? 0.50 : 0.15
    }

    static func normalizedPetToolName(_ tool: String?) -> String? {
        guard let tool else { return nil }
        let normalized = tool.lowercased()
        if normalized.contains("claude") { return "claude" }
        if normalized.contains("codex") { return "codex" }
        if normalized.contains("gemini") { return "gemini" }
        if normalized.contains("opencode") { return "opencode" }
        return nil
    }
}
