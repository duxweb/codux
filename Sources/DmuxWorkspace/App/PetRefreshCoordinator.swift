import Foundation

@MainActor
final class PetRefreshCoordinator {
    static let liveDebounceDelay: Duration = .seconds(2)

    enum Reason: String {
        case bootstrap = "bootstrap"
        case aiSession = "ai-session"
        case claim = "claim"
        case periodic = "periodic"
    }

    private let petStore: PetStore
    private let logger = AppDebugLog.shared
    private var liveSnapshotsProvider: (@MainActor () -> [AITerminalSessionSnapshot])?
    private var computedStatsProvider: (@MainActor () -> PetStats)?
    private var pendingRefreshTask: Task<Void, Never>?
    private var periodicRefreshTimer: Timer?

    init(petStore: PetStore) {
        self.petStore = petStore
    }

    func configure(
        liveSnapshots: @escaping @MainActor () -> [AITerminalSessionSnapshot],
        computedStats: @escaping @MainActor () -> PetStats
    ) {
        liveSnapshotsProvider = liveSnapshots
        computedStatsProvider = computedStats
    }

    func start() {
        periodicRefreshTimer?.invalidate()
        periodicRefreshTimer = Timer.scheduledTimer(withTimeInterval: 180, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshNow(reason: .periodic)
            }
        }
        refreshNow(reason: .bootstrap)
    }

    func stop() {
        pendingRefreshTask?.cancel()
        pendingRefreshTask = nil
        periodicRefreshTimer?.invalidate()
        periodicRefreshTimer = nil
    }

    func scheduleRefresh(reason: Reason, delay: Duration = PetRefreshCoordinator.liveDebounceDelay) {
        pendingRefreshTask?.cancel()
        pendingRefreshTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: delay)
            guard let self, !Task.isCancelled else {
                return
            }
            self.pendingRefreshTask = nil
            self.refreshNow(reason: reason)
        }
    }

    func refreshNow(reason: Reason, now: Date = .init()) {
        guard petStore.isClaimed,
              let liveSnapshotsProvider,
              let computedStatsProvider else {
            return
        }

        let liveSnapshots = liveSnapshotsProvider()
        let realtimeSessionTotals = Dictionary(
            uniqueKeysWithValues: liveSnapshots.map {
                (Self.realtimeSessionKey(for: $0), max(0, $0.currentTotalTokens - $0.baselineTotalTokens))
            }
        )
        let computedStats = petStore.shouldRefreshStats(now: now)
            ? computedStatsProvider()
            : nil

        petStore.refreshDerivedState(
            realtimeSessionTotals: realtimeSessionTotals,
            computedStats: computedStats,
            now: now
        )

        logger.log(
            "pet-refresh",
            "reason=\(reason.rawValue) liveSessions=\(realtimeSessionTotals.count) applied=\(realtimeSessionTotals.values.reduce(0, +)) hatch=\(petStore.currentHatchTokens) xp=\(petStore.currentExperienceTokens)"
        )
    }

    private static func realtimeSessionKey(for snapshot: AITerminalSessionSnapshot) -> String {
        if let tool = snapshot.tool?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased(),
           let externalSessionID = snapshot.externalSessionID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !tool.isEmpty,
           !externalSessionID.isEmpty {
            return "\(tool)|\(externalSessionID)"
        }
        return "terminal|\(snapshot.sessionID.uuidString.lowercased())"
    }
}
