import Foundation

@MainActor
final class AIRuntimePollingService {
    static let shared = AIRuntimePollingService()

    private struct HookPollSuppression: Sendable {
        var recordedAt: TimeInterval
        var deadline: TimeInterval
    }

    private let aiSessionStore: AISessionStore
    private let toolDriverFactory: AIToolDriverFactory
    private let notificationCenter: NotificationCenter
    private let logger = AppDebugLog.shared
    private let interval: TimeInterval
    private let hookSuppressionWindow: TimeInterval
    private let sessionSilenceThreshold: TimeInterval

    private var runtimeBridgeObserver: NSObjectProtocol?
    private var timer: Timer?
    private var isPolling = false
    private var pendingPollReason: String?
    private var hookPollSuppressionByTerminalID: [UUID: HookPollSuppression] = [:]

    init(
        aiSessionStore: AISessionStore = .shared,
        toolDriverFactory: AIToolDriverFactory = .shared,
        notificationCenter: NotificationCenter = .default,
        interval: TimeInterval = 6,
        hookSuppressionWindow: TimeInterval = 1.25,
        sessionSilenceThreshold: TimeInterval = 18
    ) {
        self.aiSessionStore = aiSessionStore
        self.toolDriverFactory = toolDriverFactory
        self.notificationCenter = notificationCenter
        self.interval = interval
        self.hookSuppressionWindow = hookSuppressionWindow
        self.sessionSilenceThreshold = max(interval, sessionSilenceThreshold)
    }

    func start() {
        guard runtimeBridgeObserver == nil else {
            sync(reason: "start-reuse")
            return
        }

        runtimeBridgeObserver = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            let kind = notification.userInfo?["kind"] as? String ?? "runtime-bridge"
            guard kind != "runtime-poll" else {
                return
            }
            Task { @MainActor [weak self] in
                self?.sync(reason: kind)
            }
        }

        sync(reason: "start")
    }

    func stop() {
        if let runtimeBridgeObserver {
            notificationCenter.removeObserver(runtimeBridgeObserver)
        }
        runtimeBridgeObserver = nil
        timer?.invalidate()
        timer = nil
        pendingPollReason = nil
        isPolling = false
        hookPollSuppressionByTerminalID.removeAll()
    }

    func noteHookApplied(for terminalID: UUID, reason: String) {
        let now = Date().timeIntervalSince1970
        hookPollSuppressionByTerminalID[terminalID] = HookPollSuppression(
            recordedAt: now,
            deadline: now + hookSuppressionWindow
        )
        pruneExpiredSuppressions(now: now)
        logger.log(
            "runtime-refresh",
            "suppress terminal=\(terminalID.uuidString) reason=\(reason) windowMs=\(Int(hookSuppressionWindow * 1000))"
        )
    }

    func sync(reason: String) {
        let trackedSessions = aiSessionStore.runtimeTrackedSessions()
        if trackedSessions.isEmpty {
            timer?.invalidate()
            timer = nil
            pendingPollReason = nil
            logger.log("runtime-refresh", "stop reason=\(reason) tracked=0")
            return
        }

        if timer == nil {
            timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.schedulePoll(reason: "interval")
                }
            }
            logger.log("runtime-refresh", "start interval=\(interval)s tracked=\(trackedSessions.count)")
        }

        schedulePoll(reason: reason)
    }

    private func schedulePoll(reason: String) {
        if isPolling {
            pendingPollReason = reason
            return
        }

        let now = Date().timeIntervalSince1970
        pruneExpiredSuppressions(now: now)
        let trackedSessions = aiSessionStore.runtimeTrackedSessions()
            .filter { shouldPoll(session: $0, now: now) }
        guard !trackedSessions.isEmpty else {
            logger.log("runtime-refresh", "skip reason=\(reason) eligible=0")
            return
        }

        isPolling = true
        Task.detached(priority: .utility) { [toolDriverFactory, trackedSessions, startedAt = now] in
            var updates: [(UUID, AIRuntimeContextSnapshot)] = []
            for session in trackedSessions {
                guard let driver = toolDriverFactory.driver(for: session.tool),
                      let snapshot = await driver.runtimeSnapshot(for: session) else {
                    continue
                }
                updates.append((session.terminalID, snapshot))
            }
            await MainActor.run { [weak self] in
                self?.finishPoll(
                    updates: updates,
                    reason: reason,
                    startedAt: startedAt
                )
            }
        }
    }

    private func finishPoll(
        updates: [(UUID, AIRuntimeContextSnapshot)],
        reason: String,
        startedAt: TimeInterval
    ) {
        let now = Date().timeIntervalSince1970
        var didChange = false
        for (terminalID, snapshot) in updates {
            if shouldSkipSnapshot(terminalID: terminalID, pollStartedAt: startedAt, now: now) {
                logger.log(
                    "runtime-refresh",
                    "drop terminal=\(terminalID.uuidString) reason=\(reason) cause=recent-hook"
                )
                continue
            }
            var observedSnapshot = snapshot
            if observedSnapshot.responseState == .responding {
                observedSnapshot.updatedAt = max(observedSnapshot.updatedAt, now)
            }
            didChange = aiSessionStore.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: observedSnapshot
            ) || didChange
        }

        if didChange {
            logger.log("runtime-refresh", "apply reason=\(reason) updates=\(updates.count)")
            notificationCenter.post(name: .dmuxAIRuntimeActivityPulse, object: nil)
            notificationCenter.post(
                name: .dmuxAIRuntimeBridgeDidChange,
                object: nil,
                userInfo: ["kind": "runtime-poll"]
            )
        }

        isPolling = false
        if let pendingPollReason {
            self.pendingPollReason = nil
            schedulePoll(reason: pendingPollReason)
        }

        pruneExpiredSuppressions(now: now)
    }

    private func isSuppressed(terminalID: UUID, now: TimeInterval) -> Bool {
        guard let suppression = hookPollSuppressionByTerminalID[terminalID] else {
            return false
        }
        return suppression.deadline > now
    }

    private func shouldPoll(
        session: AISessionStore.TerminalSessionState,
        now: TimeInterval
    ) -> Bool {
        if isSuppressed(terminalID: session.terminalID, now: now) {
            return true
        }
        if session.state == .responding || session.state == .needsInput {
            return true
        }
        if session.state == .idle {
            return session.hasCompletedTurn == false
        }
        let silence = max(0, now - session.updatedAt)
        return silence >= sessionSilenceThreshold
    }

    private func shouldSkipSnapshot(terminalID: UUID, pollStartedAt: TimeInterval, now: TimeInterval) -> Bool {
        guard let suppression = hookPollSuppressionByTerminalID[terminalID] else {
            return false
        }
        _ = now
        return suppression.recordedAt > pollStartedAt
    }

    private func pruneExpiredSuppressions(now: TimeInterval) {
        hookPollSuppressionByTerminalID = hookPollSuppressionByTerminalID.filter { $0.value.deadline > now }
    }
}
