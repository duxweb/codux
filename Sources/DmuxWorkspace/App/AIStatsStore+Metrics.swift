import Darwin
import Foundation

@MainActor
extension AIStatsStore {
    func titlebarTodayLevelTokensAcrossProjects(_ projects: [Project]) -> Int {
        totalTodayNormalizedTokensAcrossProjects(projects)
    }

    func petExperienceTokensAcrossProjects(_ projects: [Project]) -> Int {
        totalAllTimeNormalizedTokensAcrossProjects(projects)
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

    func totalAllTimeNormalizedTokensAcrossProjects(_ projects: [Project]) -> Int {
        projects.reduce(0) { partial, project in
            if let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) {
                return partial + indexed.heatmap.reduce(0) { $0 + $1.totalTokens }
            }
            return partial
        }
    }

    func petStatsAcrossProjects(_ projects: [Project], claimedAt: Date?) -> PetStats {
        guard let claimedAt else {
            return .neutral
        }

        var sessions: [AISessionSummary] = []

        for project in projects {
            guard let indexed = aiUsageStore.indexedProjectSnapshot(projectID: project.id) else { continue }
            sessions.append(contentsOf: indexed.sessions.filter { $0.lastSeenAt >= claimedAt })
        }

        return Self.computePetStats(from: sessions)
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
        let maxSecs      = sessions.map { $0.activeDurationSeconds }.max() ?? 0
        let avgSecs      = totalSecs / sessions.count
        let multiTurnSessions = sessions.filter { $0.requestCount >= 4 }
        let multiTurnRatio    = Double(multiTurnSessions.count) / Double(sessionCount)

        let iterativeRepairSessions = sessions.filter { s in
            guard s.requestCount >= 4, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return s.activeDurationSeconds >= 600 && avgPerTurn >= 200 && avgPerTurn <= 3_500
        }
        let repairSecs        = iterativeRepairSessions.reduce(0) { $0 + $1.activeDurationSeconds }
        let repairRatio       = min(1.0, Double(repairSecs) / Double(max(1, totalSecs)))
        let repairTokenBudget = iterativeRepairSessions.reduce(0) { $0 + $1.totalTokens }
        let adjustmentLoopCount = sessions.filter { s in
            guard s.requestCount >= 3, s.totalTokens > 0 else { return false }
            let avgPerTurn = Double(s.totalTokens) / Double(s.requestCount)
            return avgPerTurn >= 200 && avgPerTurn <= 2_800
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
            logPts(reqPerHour,            divisor: 1.8, weight: 108, cap: 150) +
            ratioPts(shortRatio,          exponent: 0.68, weight: 62, cap: 62) +
            logPts(Double(totalRequests), divisor: 22,  weight: 26,  cap: 44)  +
            shared

        let nightScore: Double
        if nightRatio >= 0.10 {
            let nightTokens = Double(totalTokens) * max(0.15, nightRatio)
            nightScore =
                ratioPts(nightRatio,          exponent: 0.62, weight: 140, cap: 140) +
                logPts(Double(nightCount),    divisor: 3.5,   weight: 34,  cap: 68)  +
                logPts(nightTokens,           divisor: 120_000, weight: 14, cap: 28) +
                shared
        } else {
            nightScore = 0
        }

        let staminaScore =
            logPts(Double(maxSecs),  divisor: 800,    weight: 82, cap: 124) +
            logPts(Double(avgSecs),  divisor: 400,    weight: 80, cap: 100) +
            logPts(Double(totalSecs),divisor: 16_000, weight: 30, cap: 50)  +
            shared

        let empathyScore =
            ratioPts(repairRatio,              exponent: 0.65, weight: 120, cap: 120) +
            ratioPts(multiTurnRatio,           exponent: 0.52, weight: 52,  cap: 52)  +
            logPts(Double(repairSecs) / 60,    divisor: 1_600, weight: 40,  cap: 72)  +
            logPts(Double(repairTokenBudget),  divisor: 120_000, weight: 24, cap: 46) +
            logPts(Double(adjustmentLoopCount),divisor: 1.8,   weight: 18,  cap: 30)  +
            shared

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
