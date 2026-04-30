import Foundation

@MainActor
final class PetRefreshCoordinator {
    static let liveDebounceDelay: Duration = .seconds(2)
    private static let dailyRecordTokenStep = 10_000_000

    enum Reason: String {
        case bootstrap = "bootstrap"
        case aiSession = "ai-session"
        case claim = "claim"
        case periodic = "periodic"
    }

    private let petStore: PetStore
    private let logger = AppDebugLog.shared
    private let liveRefreshDelay: Duration
    private var totalNormalizedTokensByProjectProvider: (@MainActor () -> [UUID: Int])?
    private var computedStatsProvider: (@MainActor (Date) -> PetStats)?
    private var dailyTotalTokensProvider: (@MainActor () -> Int)?
    private var pendingRefreshTask: Task<Void, Never>?
    private var periodicRefreshTimer: Timer?
    private var highestObservedDailyTokens = 0
    private var highestObservedDailyRecordBucket = 0
    private var observedDailyRecordDay: Date?
    var onSpeechEvent: (@MainActor (PetSpeechEvent) -> Void)?

    init(
        petStore: PetStore,
        liveRefreshDelay: Duration = PetRefreshCoordinator.liveDebounceDelay
    ) {
        self.petStore = petStore
        self.liveRefreshDelay = liveRefreshDelay
    }

    func configure(
        totalNormalizedTokensByProject: @escaping @MainActor () -> [UUID: Int],
        computedStats: @escaping @MainActor (Date) -> PetStats,
        dailyTotalTokens: (@MainActor () -> Int)? = nil
    ) {
        totalNormalizedTokensByProjectProvider = totalNormalizedTokensByProject
        computedStatsProvider = computedStats
        dailyTotalTokensProvider = dailyTotalTokens
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

    func scheduleRefresh(reason: Reason, delay: Duration? = nil) {
        pendingRefreshTask?.cancel()
        pendingRefreshTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: delay ?? self?.liveRefreshDelay ?? PetRefreshCoordinator.liveDebounceDelay)
            guard let self, !Task.isCancelled else {
                return
            }
            self.pendingRefreshTask = nil
            self.refreshNow(reason: reason)
        }
    }

    func refreshNow(reason: Reason, now: Date = .init()) {
        guard petStore.isClaimed,
              let totalNormalizedTokensByProjectProvider,
              let computedStatsProvider else {
            return
        }

        let totalNormalizedTokensByProject = totalNormalizedTokensByProjectProvider()
            .reduce(into: [UUID: Int]()) { partial, entry in
                partial[entry.key] = max(0, entry.value)
            }
        let totalNormalizedTokens = totalNormalizedTokensByProject.values.reduce(0) { partial, total in
            let base = max(0, partial)
            let increment = max(0, total)
            return increment > Int.max - base ? Int.max : base + increment
        }
        let computedStats = petStore.shouldRefreshStats(now: now)
            ? computedStatsProvider(now)
            : nil

        petStore.refreshDerivedState(
            totalNormalizedTokensByProject: totalNormalizedTokensByProject,
            computedStats: computedStats,
            now: now
        )

        emitDailyRecordIfNeeded(todayTokens: dailyTotalTokensProvider?(), now: now)

        logger.log(
            "pet-refresh",
            "reason=\(reason.rawValue) projects=\(totalNormalizedTokensByProject.count) total=\(totalNormalizedTokens) watermark=\(petStore.globalNormalizedTotalWatermark ?? 0) hatch=\(petStore.currentHatchTokens) xp=\(petStore.currentExperienceTokens)"
        )
    }

    private func emitDailyRecordIfNeeded(todayTokens rawTodayTokens: Int?, now: Date) {
        guard let rawTodayTokens else {
            return
        }

        let todayTokens = max(0, rawTodayTokens)
        let day = Calendar.current.startOfDay(for: now)
        if observedDailyRecordDay.map({ Calendar.current.isDate($0, inSameDayAs: day) }) != true {
            observedDailyRecordDay = day
            highestObservedDailyTokens = todayTokens
            highestObservedDailyRecordBucket = dailyRecordBucket(for: todayTokens)
            return
        }

        guard todayTokens > highestObservedDailyTokens else {
            return
        }

        let bucket = dailyRecordBucket(for: todayTokens)
        if bucket > highestObservedDailyRecordBucket,
           highestObservedDailyTokens > 0 {
            onSpeechEvent?(
                PetSpeechEvent(
                    kind: .usageDailyRecord,
                    payload: ["tokensK": "\(max(1, todayTokens / 1000))K"],
                    occurredAt: now
                )
            )
        }

        highestObservedDailyTokens = todayTokens
        highestObservedDailyRecordBucket = max(highestObservedDailyRecordBucket, bucket)
    }

    private func dailyRecordBucket(for tokens: Int) -> Int {
        max(0, tokens / Self.dailyRecordTokenStep)
    }
}
