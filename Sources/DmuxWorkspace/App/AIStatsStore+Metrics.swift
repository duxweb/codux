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

    func petStatsRolling(_ projects: [Project], windowDays: Int = 14, now: Date = Date()) -> PetStats {
        let projectIDs = Set(projects.map(\.id))
        guard !projectIDs.isEmpty else {
            return .neutral
        }
        let cutoff = now.addingTimeInterval(-Double(windowDays) * 86_400)
        let sessions = aiUsageStore.indexedSessions(since: cutoff, projectIDs: projectIDs)
        return Self.computePetStats(from: sessions, now: now)
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

    static func computePetStats(from sessions: [AISessionSummary], now: Date = Date()) -> PetStats {
        guard !sessions.isEmpty else { return .neutral }

        let totalRequests = sessions.reduce(0) { $0 + $1.requestCount }
        let totalTokens   = sessions.reduce(0) { $0 + $1.totalTokens }
        let totalSecs     = sessions.reduce(0) { $0 + $1.activeDurationSeconds }
        let sessionCount  = max(1, sessions.count)

        if sessions.count < 3 || totalRequests < 5 || totalTokens < 20_000 {
            return PetStats(wisdom: 100, chaos: 100, night: 100, stamina: 100, empathy: 100)
        }

        let calendar = Calendar.autoupdatingCurrent
        let avgTokPerReq = totalRequests > 0 ? Double(totalTokens) / Double(totalRequests) : 0
        let reqPerHour   = totalSecs > 0 ? Double(totalRequests) / (Double(totalSecs) / 3600.0) : 0
        let shortCount   = sessions.filter { $0.activeDurationSeconds < 300 }.count
        let nightCount   = sessions.filter {
            let h = calendar.component(.hour, from: $0.firstSeenAt)
            return h >= 22 || h < 6
        }.count
        let sustainedSessionSeconds = sessions.map { session -> Int in
            let activeSeconds = max(0, session.activeDurationSeconds)
            let wallClockSeconds = max(0, Int(session.lastSeenAt.timeIntervalSince(session.firstSeenAt).rounded()))
            return max(activeSeconds, wallClockSeconds)
        }
        let maxSecs      = sustainedSessionSeconds.max() ?? 0
        let multiTurnSessions = sessions.filter { $0.requestCount >= 4 }

        let iterativeRepairSessions = sessions.filter { s in
            guard s.requestCount >= 3, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return s.activeDurationSeconds >= 360 && avgPerTurn >= 120 && avgPerTurn <= 4_200
        }
        let repairTokenBudget = iterativeRepairSessions.reduce(0) { $0 + $1.totalTokens }

        func smoothedRatio(positive: Int, total: Int) -> Double {
            let alpha = 2.0
            let beta = 2.0
            return (Double(max(0, positive)) + alpha) / (Double(max(0, total)) + alpha + beta)
        }

        func satRatio(_ value: Double, target: Double) -> Double {
            guard value > 0, target > 0 else { return 0 }
            return value / (value + target)
        }

        func displayPts(_ ratio: Double, weight: Double, exponent: Double = 0.55) -> Double {
            guard ratio > 0, weight > 0 else { return 0 }
            return min(weight, pow(min(1.0, max(0, ratio)), exponent) * weight)
        }

        let depthRatio = satRatio(avgTokPerReq, target: 6_000)
        let depth = displayPts(depthRatio, weight: 230, exponent: 0.60)
        let deepSessions = sessions.filter { session in
            session.requestCount > 0 && Double(session.totalTokens) / Double(session.requestCount) >= 2_000
        }.count
        let focus = displayPts(
            smoothedRatio(positive: deepSessions, total: sessions.count),
            weight: 80,
            exponent: 0.55
        )

        let burst = displayPts(
            smoothedRatio(positive: shortCount, total: sessions.count),
            weight: 200,
            exponent: 0.55
        )
        let rate = displayPts(satRatio(reqPerHour, target: 6.0), weight: 130, exponent: 0.65)

        let core = displayPts(
            smoothedRatio(positive: nightCount, total: sessionCount),
            weight: 240,
            exponent: 0.55
        )
        let streak = displayPts(satRatio(Double(nightCount), target: 8.0), weight: 70, exponent: 0.60)

        let longSessionCount = sustainedSessionSeconds.filter { $0 >= 1_800 }.count
        let long = displayPts(
            smoothedRatio(positive: longSessionCount, total: sessions.count),
            weight: 200,
            exponent: 0.55
        )
        let peak = displayPts(satRatio(Double(maxSecs), target: 3_600), weight: 130, exponent: 0.60)

        let repairTokenShare = totalTokens > 0
            ? Double(repairTokenBudget) / Double(totalTokens)
            : 0
        let repair = displayPts(min(1.0, repairTokenShare), weight: 210, exponent: 0.55)
        let collaboration = displayPts(
            smoothedRatio(positive: multiTurnSessions.count, total: sessionCount),
            weight: 120,
            exponent: 0.55
        )

        return PetStats(
            wisdom:  max(0, Int((depth + focus).rounded())),
            chaos:   max(0, Int((burst + rate).rounded())),
            night:   max(0, Int((core + streak).rounded())),
            stamina: max(0, Int((long + peak).rounded())),
            empathy: max(0, Int((repair + collaboration).rounded()))
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
