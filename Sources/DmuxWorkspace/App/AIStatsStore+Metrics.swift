import Darwin
import Foundation

@MainActor
extension AIStatsStore {
    func titlebarTodayLevelTokens() -> Int {
        aiUsageStore.globalTodayNormalizedTokens()
    }

    func petStatsSinceClaimedAt(_ claimedAt: Date?) -> PetStats {
        guard let claimedAt else {
            return .neutral
        }
        return Self.computePetStats(from: aiUsageStore.indexedSessions(since: claimedAt))
    }

    func totalTodayNormalizedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            let liveOrCached = cachedState(for: project.id)
            if let liveOrCached {
                return partial + resolvedTodayTotalTokens(for: liveOrCached)
            }

            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                return partial + resolvedTodayTotalTokens(
                    summary: indexed.projectSummary.todayTotalTokens,
                    timeBuckets: indexed.todayTimeBuckets,
                    heatmap: indexed.heatmap
                )
            }

            return partial
        }
    }

    func totalTodayDisplayedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            let liveOrCached = cachedState(for: project.id)
            if let liveOrCached {
                return partial + resolvedDisplayedTodayTotalTokens(for: liveOrCached)
            }

            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                return partial + resolvedDisplayedTodayTotalTokens(
                    summary: indexed.projectSummary.todayTotalTokens,
                    summaryCached: indexed.projectSummary.todayCachedInputTokens,
                    timeBuckets: indexed.todayTimeBuckets,
                    heatmap: indexed.heatmap
                )
            }

            return partial
        }
    }

    func totalAllTimeNormalizedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            if let liveOrCached = cachedState(for: project.id),
               let projectTotalTokens = liveOrCached.projectSummary?.projectTotalTokens {
                return partial + max(0, projectTotalTokens)
            }

            let liveOverlayTokens = liveOverlayTotalTokens(projectID: project.id)
            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                let indexedTotal = max(
                    indexed.projectSummary.projectTotalTokens,
                    indexed.heatmap.reduce(0) { $0 + $1.totalTokens }
                )
                return partial + indexedTotal + liveOverlayTokens
            }
            return partial + liveOverlayTokens
        }
    }

    private func liveOverlayTotalTokens(projectID: UUID) -> Int {
        aiSessionStore.liveAggregationSnapshots(projectID: projectID).reduce(0) { partial, snapshot in
            partial + max(0, snapshot.currentTotalTokens - snapshot.baselineTotalTokens)
        }
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
            return avgPerTurn >= 120 && avgPerTurn <= 3_600
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
