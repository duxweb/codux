import Foundation

struct AIUsageLiveOverlayBaselines: Equatable, Sendable {
    var day: Date?
    var totalTokensBySessionKey: [String: Int]
    var cachedInputTokensBySessionKey: [String: Int]

    static let empty = AIUsageLiveOverlayBaselines(
        day: nil,
        totalTokensBySessionKey: [:],
        cachedInputTokensBySessionKey: [:]
    )
}

struct AIUsageLiveOverlayState: Equatable, Sendable {
    var snapshots: [AITerminalSessionSnapshot]
    var totalTokens: Int
    var cachedInputTokens: Int
    var todayTokens: Int
    var todayCachedInputTokens: Int
    var baselines: AIUsageLiveOverlayBaselines
}

struct AIUsageLiveOverlayCalculator: Sendable {
    var calendar: Calendar = .autoupdatingCurrent

    func calculate(
        snapshots: [AITerminalSessionSnapshot],
        indexedSessions: [AISessionSummary],
        existingBaselines: AIUsageLiveOverlayBaselines = .empty,
        useTodayBaseline: Bool = true
    ) -> AIUsageLiveOverlayState {
        let today = calendar.startOfDay(for: Date())
        let canReuseExistingBaselines = existingBaselines.day.map {
            calendar.isDate($0, inSameDayAs: today)
        } ?? false
        let indexedBaselines = indexedSessionBaselines(from: indexedSessions)
        var nextTotalBaselines: [String: Int] = [:]
        var nextCachedInputBaselines: [String: Int] = [:]
        var totalTodayTokens = 0
        var totalTodayCachedInputTokens = 0

        let overlaySnapshots = snapshots.map { snapshot in
            let indexedBaseline = indexedBaseline(for: snapshot, in: indexedBaselines)
            let snapshotKey = liveSnapshotKey(for: snapshot)
            let existingTotalBaseline = snapshotKey.flatMap {
                useTodayBaseline && canReuseExistingBaselines ? existingBaselines.totalTokensBySessionKey[$0] : nil
            }
            let existingCachedBaseline = snapshotKey.flatMap {
                useTodayBaseline && canReuseExistingBaselines ? existingBaselines.cachedInputTokensBySessionKey[$0] : nil
            }
            let startedBeforeToday = (snapshot.startedAt ?? snapshot.updatedAt) < today
            let todayTotalBaseline = existingTotalBaseline
                ?? (useTodayBaseline && startedBeforeToday ? snapshot.currentTotalTokens : nil)
            let todayCachedInputBaseline = existingCachedBaseline
                ?? (useTodayBaseline && startedBeforeToday ? snapshot.currentCachedInputTokens : nil)

            if let snapshotKey, let todayTotalBaseline {
                nextTotalBaselines[snapshotKey] = todayTotalBaseline
            }
            if let snapshotKey, let todayCachedInputBaseline {
                nextCachedInputBaselines[snapshotKey] = todayCachedInputBaseline
            }

            let allTimeTotalDelta = Self.liveDelta(
                current: snapshot.currentTotalTokens,
                runtimeBaseline: snapshot.baselineTotalTokens,
                indexedBaseline: indexedBaseline.totalTokens,
                todayBaseline: nil
            )
            let allTimeCachedInputDelta = Self.liveDelta(
                current: snapshot.currentCachedInputTokens,
                runtimeBaseline: snapshot.baselineCachedInputTokens,
                indexedBaseline: indexedBaseline.cachedInputTokens,
                todayBaseline: nil
            )
            let todayTotalDelta = Self.liveDelta(
                current: snapshot.currentTotalTokens,
                runtimeBaseline: snapshot.baselineTotalTokens,
                indexedBaseline: indexedBaseline.totalTokens,
                todayBaseline: todayTotalBaseline
            )
            let todayCachedInputDelta = Self.liveDelta(
                current: snapshot.currentCachedInputTokens,
                runtimeBaseline: snapshot.baselineCachedInputTokens,
                indexedBaseline: indexedBaseline.cachedInputTokens,
                todayBaseline: todayCachedInputBaseline
            )

            totalTodayTokens = clampedAdd(totalTodayTokens, todayTotalDelta)
            totalTodayCachedInputTokens = clampedAdd(totalTodayCachedInputTokens, todayCachedInputDelta)

            var overlaySnapshot = snapshot
            overlaySnapshot.currentInputTokens = max(0, snapshot.currentInputTokens - snapshot.baselineInputTokens)
            overlaySnapshot.currentOutputTokens = max(0, snapshot.currentOutputTokens - snapshot.baselineOutputTokens)
            overlaySnapshot.currentTotalTokens = allTimeTotalDelta
            overlaySnapshot.currentCachedInputTokens = allTimeCachedInputDelta
            return overlaySnapshot
        }

        return AIUsageLiveOverlayState(
            snapshots: overlaySnapshots,
            totalTokens: overlaySnapshots.reduce(0) { clampedAdd($0, $1.currentTotalTokens) },
            cachedInputTokens: overlaySnapshots.reduce(0) { clampedAdd($0, $1.currentCachedInputTokens) },
            todayTokens: totalTodayTokens,
            todayCachedInputTokens: totalTodayCachedInputTokens,
            baselines: AIUsageLiveOverlayBaselines(
                day: useTodayBaseline ? today : nil,
                totalTokensBySessionKey: useTodayBaseline ? nextTotalBaselines : [:],
                cachedInputTokensBySessionKey: useTodayBaseline ? nextCachedInputBaselines : [:]
            )
        )
    }

    func liveTotalTokensForPet(
        snapshots: [AITerminalSessionSnapshot],
        projectIDs: Set<UUID>,
        claimedAt: Date,
        indexedSessions: [AISessionSummary]
    ) -> [UUID: Int] {
        let indexedBaselines = indexedSessionBaselines(from: indexedSessions)
        return snapshots.reduce(into: [UUID: Int]()) { partial, snapshot in
            guard projectIDs.contains(snapshot.projectID) else {
                return
            }
            let firstTrackedAt = snapshot.startedAt ?? snapshot.updatedAt
            guard firstTrackedAt >= claimedAt else {
                return
            }
            let indexedBaseline = indexedBaseline(for: snapshot, in: indexedBaselines)
            let delta = Self.liveDelta(
                current: snapshot.currentTotalTokens,
                runtimeBaseline: snapshot.baselineTotalTokens,
                indexedBaseline: indexedBaseline.totalTokens,
                todayBaseline: nil
            )
            partial[snapshot.projectID] = clampedAdd(partial[snapshot.projectID] ?? 0, delta)
        }
    }

    static func liveDelta(
        current: Int,
        runtimeBaseline: Int,
        indexedBaseline: Int,
        todayBaseline: Int?
    ) -> Int {
        max(0, current - max(runtimeBaseline, indexedBaseline, todayBaseline ?? 0))
    }

    private func indexedSessionBaselines(
        from sessions: [AISessionSummary]
    ) -> [String: (totalTokens: Int, cachedInputTokens: Int)] {
        sessions.reduce(into: [:]) { partial, session in
            guard let key = Self.indexedSessionKey(tool: session.lastTool, externalSessionID: session.externalSessionID) else {
                return
            }
            let existing = partial[key] ?? (totalTokens: 0, cachedInputTokens: 0)
            partial[key] = (
                totalTokens: max(existing.totalTokens, session.totalTokens),
                cachedInputTokens: max(existing.cachedInputTokens, session.cachedInputTokens)
            )
        }
    }

    private func indexedBaseline(
        for snapshot: AITerminalSessionSnapshot,
        in baselines: [String: (totalTokens: Int, cachedInputTokens: Int)]
    ) -> (totalTokens: Int, cachedInputTokens: Int) {
        guard let key = Self.indexedSessionKey(tool: snapshot.tool, externalSessionID: snapshot.externalSessionID),
              let baseline = baselines[key] else {
            return (totalTokens: 0, cachedInputTokens: 0)
        }
        return baseline
    }

    private func liveSnapshotKey(for snapshot: AITerminalSessionSnapshot) -> String? {
        if let key = Self.indexedSessionKey(tool: snapshot.tool, externalSessionID: snapshot.externalSessionID) {
            return key
        }
        return "terminal|\(snapshot.sessionID.uuidString)"
    }

    private static func indexedSessionKey(tool: String?, externalSessionID: String?) -> String? {
        guard let tool = normalizedNonEmptyString(tool),
              let externalSessionID = normalizedNonEmptyString(externalSessionID) else {
            return nil
        }
        return "\(tool)|\(externalSessionID)"
    }

    private func clampedAdd(_ lhs: Int, _ rhs: Int) -> Int {
        let base = max(0, lhs)
        let increment = max(0, rhs)
        return increment > Int.max - base ? Int.max : base + increment
    }
}
