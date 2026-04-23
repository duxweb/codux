import Foundation

@MainActor
extension AISessionStore {
    func hasLiveSessions(projectID: UUID) -> Bool {
        terminalSessionsByID.values.contains { $0.projectID == projectID && $0.isLive }
    }

    func liveSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map(snapshot(from:))
    }

    func globalLiveAggregationSnapshots() -> [AITerminalSessionSnapshot] {
        aggregationSnapshots(from: terminalSessionsByID.values.filter(\.isLive).map(snapshot(from:)))
    }

    func runtimeTrackedSessions() -> [TerminalSessionState] {
        terminalSessionsByID.values
            .filter { isRuntimeTracked($0) }
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    func liveAggregationSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        aggregationSnapshots(from: liveSnapshots(projectID: projectID))
    }

    func currentDisplaySnapshot(projectID: UUID, selectedSessionID: UUID?) -> AITerminalSessionSnapshot? {
        let snapshots = liveSnapshots(projectID: projectID)
        if let selectedSessionID,
           let selected = snapshots.first(where: { $0.sessionID == selectedSessionID }) {
            return selected
        }
        return snapshots.first
    }

    func projectPhase(projectID: UUID) -> ProjectActivityPhase {
        let now = Date().timeIntervalSince1970
        let trackedSessions = terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
            .sorted(by: { $0.updatedAt > $1.updatedAt })

        if let responding = trackedSessions.first(where: {
            isVisibleRunningSession($0, now: now)
        }) {
            return .running(tool: responding.tool)
        }
        if let needsInput = trackedSessions.first(where: { $0.state == .needsInput }) {
            return .waitingInput(tool: needsInput.tool)
        }
        return .idle
    }

    func completedPhase(projectID: UUID) -> ProjectActivityPhase? {
        let trackedSessions = terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
            .sorted(by: { $0.updatedAt > $1.updatedAt })

        guard trackedSessions.contains(where: { $0.state == .responding || $0.state == .needsInput }) == false else {
            return nil
        }

        guard let completed = trackedSessions.first(where: {
            $0.state == .idle
                && $0.wasInterrupted == false
                && $0.hasCompletedTurn
        }) else {
            return nil
        }

        return .completed(
            tool: completed.tool,
            finishedAt: Date(timeIntervalSince1970: completed.updatedAt),
            exitCode: nil
        )
    }

    func latestActiveStartedAt(projectID: UUID) -> Date? {
        terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
            .filter { $0.state == .responding || $0.state == .needsInput }
            .compactMap { session in
                let timestamp = session.activeTurnStartedAt ?? session.startedAt ?? session.updatedAt
                return Date(timeIntervalSince1970: timestamp)
            }
            .max()
    }

    func waitingInputContext(projectID: UUID) -> WaitingInputContext? {
        guard let session = terminalSessionsByID.values
            .filter({ $0.projectID == projectID && $0.isLive && $0.state == .needsInput })
            .sorted(by: { $0.updatedAt > $1.updatedAt })
            .first else {
            return nil
        }

        return WaitingInputContext(
            tool: session.tool,
            updatedAt: session.updatedAt,
            notificationType: session.notificationType,
            targetToolName: session.targetToolName,
            message: session.interactionMessage
        )
    }

    func debugSummary(projectID: UUID) -> String {
        let sessions = terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
            .sorted { $0.updatedAt > $1.updatedAt }
        guard !sessions.isEmpty else {
            return "none"
        }
        return sessions.map { session in
            let activeTurn = session.activeTurnStartedAt.map { String($0) } ?? "nil"
            let runtimeTurn = session.runtimeTurnStartedAt.map { String($0) } ?? "nil"
            return "terminal=\(session.terminalID.uuidString) tool=\(session.tool) state=\(session.state.rawValue) external=\(session.aiSessionID ?? "nil") total=\(session.committedTotalTokens) activeTurn=\(activeTurn) runtimeTurn=\(runtimeTurn)"
        }
        .joined(separator: " | ")
    }

    private func isRuntimeTracked(_ session: TerminalSessionState) -> Bool {
        guard session.isLive else {
            return false
        }

        switch session.state {
        case .responding, .needsInput:
            return true
        case .idle:
            return session.wasInterrupted == false
        }
    }

    private func snapshot(from session: TerminalSessionState) -> AITerminalSessionSnapshot {
        let now = Date().timeIntervalSince1970
        return AITerminalSessionSnapshot(
            sessionID: session.terminalID,
            externalSessionID: session.aiSessionID,
            projectID: session.projectID,
            projectName: session.projectName,
            sessionTitle: session.sessionTitle,
            tool: session.tool,
            model: session.model,
            status: session.status,
            isRunning: isVisibleRunningSession(session, now: now),
            startedAt: session.startedAt.map { Date(timeIntervalSince1970: $0) },
            updatedAt: Date(timeIntervalSince1970: session.updatedAt),
            currentInputTokens: session.committedInputTokens,
            currentOutputTokens: session.committedOutputTokens,
            currentTotalTokens: session.committedTotalTokens,
            currentCachedInputTokens: session.committedCachedInputTokens,
            baselineInputTokens: session.baselineInputTokens,
            baselineOutputTokens: session.baselineOutputTokens,
            baselineTotalTokens: session.baselineTotalTokens,
            baselineCachedInputTokens: session.baselineCachedInputTokens,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: session.wasInterrupted,
            hasCompletedTurn: session.hasCompletedTurn
        )
    }

    private func aggregationSnapshots(from snapshots: [AITerminalSessionSnapshot]) -> [AITerminalSessionSnapshot] {
        var snapshotsByLogicalKey: [LogicalSessionKey: AITerminalSessionSnapshot] = [:]
        var fallbackSnapshots: [UUID: AITerminalSessionSnapshot] = [:]

        for snapshot in snapshots where snapshot.isRunning || snapshot.hasCompletedTurn || snapshot.currentTotalTokens > snapshot.baselineTotalTokens {
            guard let tool = normalizedNonEmptyString(snapshot.tool),
                  let aiSessionID = normalizedNonEmptyString(snapshot.externalSessionID) else {
                fallbackSnapshots[snapshot.sessionID] = snapshot
                continue
            }
            let key = LogicalSessionKey(tool: tool, aiSessionID: aiSessionID)
            if let existing = snapshotsByLogicalKey[key], existing.updatedAt >= snapshot.updatedAt {
                continue
            }
            snapshotsByLogicalKey[key] = snapshot
        }

        return (Array(snapshotsByLogicalKey.values) + Array(fallbackSnapshots.values))
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    private func isVisibleRunningSession(
        _ session: TerminalSessionState,
        now: TimeInterval
    ) -> Bool {
        guard session.state == .responding else {
            return false
        }
        return max(0, now - session.updatedAt) <= runningPhaseLifetime
    }
}
