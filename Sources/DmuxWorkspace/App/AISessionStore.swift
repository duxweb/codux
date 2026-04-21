import Foundation
import Observation

@MainActor
@Observable
final class AISessionStore {
    static let shared = AISessionStore()
    private let completedPhaseLifetime: TimeInterval = 6

    enum State: String, Codable, Equatable, Sendable {
        case idle
        case responding
        case needsInput
    }

    struct LogicalSessionKey: Hashable, Sendable {
        var tool: String
        var aiSessionID: String
    }

    struct LogicalSessionState: Equatable, Sendable {
        var key: LogicalSessionKey
        var model: String?
        var totalTokens: Int
        var updatedAt: Double
        var hasCompletedTurn: Bool
    }

    struct ExpectedLogicalSession: Equatable, Sendable {
        var tool: String
        var aiSessionID: String
        var indexedSummary: AISessionSummary?
    }

    struct TerminalSessionState: Equatable, Sendable {
        var terminalID: UUID
        var terminalInstanceID: String?
        var projectID: UUID
        var projectName: String
        var projectPath: String?
        var sessionTitle: String
        var tool: String
        var aiSessionID: String?
        var state: State
        var model: String?
        var baselineTotalTokens: Int
        var committedTotalTokens: Int
        var updatedAt: Double
        var startedAt: Double?
        var wasInterrupted: Bool
        var hasCompletedTurn: Bool
        var transcriptPath: String?
        var notificationType: String?
        var targetToolName: String?
        var interactionMessage: String?

        var logicalSessionKey: LogicalSessionKey? {
            guard let aiSessionID else {
                return nil
            }
            return LogicalSessionKey(tool: tool, aiSessionID: aiSessionID)
        }

        var status: String {
            switch state {
            case .idle:
                return "idle"
            case .responding:
                return "running"
            case .needsInput:
                return "needs-input"
            }
        }

        var isLive: Bool {
            !tool.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }

        var isLivePanelEligible: Bool {
            guard isLive else {
                return false
            }
            switch state {
            case .responding, .needsInput:
                return true
            case .idle:
                return hasCompletedTurn == false && wasInterrupted == false
            }
        }
    }

    private let logger = AppDebugLog.shared
    private let toolDriverFactory = AIToolDriverFactory.shared

    private(set) var terminalSessionsByID: [UUID: TerminalSessionState] = [:]
    private(set) var logicalSessionsByKey: [LogicalSessionKey: LogicalSessionState] = [:]
    private var expectedLogicalSessionsByTerminalID: [UUID: ExpectedLogicalSession] = [:]
    var renderVersion: UInt64 = 0

    private init() {}

    func reset() {
        terminalSessionsByID.removeAll()
        logicalSessionsByKey.removeAll()
        expectedLogicalSessionsByTerminalID.removeAll()
        renderVersion &+= 1
        logger.log("ai-session-store", "reset")
    }

    func registerExpectedLogicalSession(
        terminalID: UUID,
        tool: String,
        aiSessionID: String,
        indexedSummary: AISessionSummary? = nil
    ) {
        let normalizedTool = canonicalToolName(tool)
        let normalizedSessionID = normalizedNonEmptyString(aiSessionID)
        guard let normalizedSessionID else {
            return
        }
        expectedLogicalSessionsByTerminalID[terminalID] = ExpectedLogicalSession(
            tool: normalizedTool,
            aiSessionID: normalizedSessionID,
            indexedSummary: indexedSummary
        )
    }

    func clearExpectedLogicalSession(terminalID: UUID) {
        expectedLogicalSessionsByTerminalID[terminalID] = nil
    }

    func apply(_ event: AIHookEvent) -> Bool {
        let normalizedTool = canonicalToolName(event.tool)
        let normalizedAISessionID = normalizedNonEmptyString(event.aiSessionID)
            ?? resolvedExpectedLogicalSessionID(for: event.terminalID, tool: normalizedTool)
        let normalizedModel = normalizedNonEmptyString(event.model)
        let normalizedInstanceID = normalizedNonEmptyString(event.terminalInstanceID)

        if shouldIgnore(event: event, terminalInstanceID: normalizedInstanceID) {
            logger.log(
                "ai-session-store",
                "ignore terminal=\(event.terminalID.uuidString) tool=\(normalizedTool) kind=\(event.kind.rawValue) reason=stale-instance"
            )
            return false
        }

        let previousLogicalKey = terminalSessionsByID[event.terminalID]?.logicalSessionKey
        let previousState = terminalSessionsByID[event.terminalID]

        var session = terminalSessionsByID[event.terminalID] ?? makeFreshSessionState(
            event: event,
            tool: normalizedTool,
            aiSessionID: normalizedAISessionID,
            model: normalizedModel,
            terminalInstanceID: normalizedInstanceID
        )

        if let existing = previousState,
           shouldResetSessionState(
               existing: existing,
               incomingTool: normalizedTool,
               incomingAISessionID: normalizedAISessionID,
               incomingTerminalInstanceID: normalizedInstanceID
           ) {
            session = makeFreshSessionState(
                event: event,
                tool: normalizedTool,
                aiSessionID: normalizedAISessionID,
                model: normalizedModel,
                terminalInstanceID: normalizedInstanceID
            )
        }

        session.terminalInstanceID = normalizedInstanceID ?? session.terminalInstanceID
        session.projectID = event.projectID
        session.projectName = event.projectName
        session.projectPath = normalizedNonEmptyString(event.projectPath) ?? session.projectPath
        session.sessionTitle = event.sessionTitle
        session.tool = normalizedTool
        session.updatedAt = max(session.updatedAt, event.updatedAt)
        session.startedAt = min(session.startedAt ?? event.updatedAt, event.updatedAt)
        session.model = normalizedModel ?? session.model
        if let transcriptPath = normalizedNonEmptyString(event.metadata?.transcriptPath) {
            session.transcriptPath = transcriptPath
        }
        if let notificationType = normalizedNonEmptyString(event.metadata?.notificationType) {
            session.notificationType = notificationType
        }
        if let targetToolName = normalizedNonEmptyString(event.metadata?.targetToolName) {
            session.targetToolName = targetToolName
        }
        if let interactionMessage = normalizedNonEmptyString(event.metadata?.message) {
            session.interactionMessage = interactionMessage
        }
        if let normalizedAISessionID {
            session.aiSessionID = normalizedAISessionID
        }

        switch event.kind {
        case .sessionStarted:
            seedSessionOnStart(&session, event: event)
        case .promptSubmitted:
            applyPromptSubmitted(&session, event: event)
        case .needsInput:
            session.state = .needsInput
            session.wasInterrupted = false
        case .turnCompleted:
            applyTurnCompleted(&session, event: event)
        case .sessionEnded:
            terminalSessionsByID[event.terminalID] = nil
            expectedLogicalSessionsByTerminalID[event.terminalID] = nil
            pruneLogicalSessionIfUnused(previousLogicalKey ?? session.logicalSessionKey)
            let didChange = previousState != nil
            if didChange {
                renderVersion &+= 1
                logger.log(
                    "ai-session-store",
                    "end terminal=\(event.terminalID.uuidString) tool=\(normalizedTool) external=\(normalizedAISessionID ?? "nil")"
                )
            }
            return didChange
        }

        terminalSessionsByID[event.terminalID] = session
        reconcileLogicalSession(for: session, previousLogicalKey: previousLogicalKey)
        pruneLogicalSessionIfUnused(previousLogicalKey)
        clearExpectedLogicalSessionIfMatched(session)

        let didChange = previousState != session
        if didChange {
            renderVersion &+= 1
            logger.log(
                "ai-session-store",
                "apply terminal=\(event.terminalID.uuidString) tool=\(normalizedTool) kind=\(event.kind.rawValue) state=\(session.state.rawValue) external=\(session.aiSessionID ?? "nil") total=\(session.committedTotalTokens) baseline=\(session.baselineTotalTokens)"
            )
        }
        return didChange
    }

    func applyOpencodeEnvelope(_ envelope: AIToolUsageEnvelope) -> Bool {
        guard let terminalID = UUID(uuidString: envelope.sessionId),
              let projectID = UUID(uuidString: envelope.projectId) else {
            return false
        }
        let kind: AIHookEventKind
        switch envelope.responseState {
        case .responding:
            kind = .promptSubmitted
        case .idle:
            kind = envelope.status == "completed" ? .turnCompleted : .sessionStarted
        case nil:
            kind = envelope.status == "running" ? .promptSubmitted : .turnCompleted
        }
        return apply(
            AIHookEvent(
                kind: kind,
                terminalID: terminalID,
                terminalInstanceID: envelope.sessionInstanceId,
                projectID: projectID,
                projectName: envelope.projectName,
                sessionTitle: envelope.sessionTitle,
                tool: canonicalToolName(envelope.tool),
                aiSessionID: envelope.externalSessionID,
                model: envelope.model,
                totalTokens: envelope.totalTokens,
                updatedAt: envelope.updatedAt,
                metadata: nil
            )
        )
    }

    func markInterrupted(terminalID: UUID, updatedAt: Double = Date().timeIntervalSince1970) -> Bool {
        guard var session = terminalSessionsByID[terminalID],
              session.state != .idle else {
            return false
        }
        session.state = .idle
        session.wasInterrupted = true
        session.hasCompletedTurn = false
        session.updatedAt = max(session.updatedAt, updatedAt)
        terminalSessionsByID[terminalID] = session
        renderVersion &+= 1
        logger.log("ai-session-store", "interrupt terminal=\(terminalID.uuidString) tool=\(session.tool)")
        return true
    }

    func removeTerminal(_ terminalID: UUID) {
        let previousLogicalKey = terminalSessionsByID[terminalID]?.logicalSessionKey
        terminalSessionsByID[terminalID] = nil
        expectedLogicalSessionsByTerminalID[terminalID] = nil
        pruneLogicalSessionIfUnused(previousLogicalKey)
        renderVersion &+= 1
        logger.log("ai-session-store", "remove terminal=\(terminalID.uuidString)")
    }

    func session(for terminalID: UUID) -> TerminalSessionState? {
        terminalSessionsByID[terminalID]
    }

    func tool(for terminalID: UUID) -> String? {
        terminalSessionsByID[terminalID]?.tool
    }

    func externalSessionID(for terminalID: UUID) -> String? {
        terminalSessionsByID[terminalID]?.aiSessionID
    }

    func isRunning(terminalID: UUID) -> Bool {
        terminalSessionsByID[terminalID]?.state == .responding
    }

    func liveSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLivePanelEligible }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map(snapshot(from:))
    }

    func liveDisplaySnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        liveSnapshots(projectID: projectID)
    }

    func liveAggregationSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        var snapshotsByLogicalKey: [LogicalSessionKey: AITerminalSessionSnapshot] = [:]
        var fallbackSnapshots: [UUID: AITerminalSessionSnapshot] = [:]

        for snapshot in liveSnapshots(projectID: projectID) {
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

        let combined = Array(snapshotsByLogicalKey.values) + Array(fallbackSnapshots.values)
        return combined
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    func currentDisplaySnapshot(projectID: UUID, selectedSessionID: UUID?) -> AITerminalSessionSnapshot? {
        let snapshots = liveDisplaySnapshots(projectID: projectID)
        if let selectedSessionID,
           let selected = snapshots.first(where: { $0.sessionID == selectedSessionID }) {
            return selected
        }
        return snapshots.first
    }

    func projectPhase(projectID: UUID) -> ProjectActivityPhase {
        let trackedSessions = terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
            .sorted(by: { $0.updatedAt > $1.updatedAt })

        if let responding = trackedSessions.first(where: { $0.state == .responding }) {
            return .running(tool: responding.tool)
        }
        if let needsInput = trackedSessions.first(where: { $0.state == .needsInput }) {
            return .waitingInput(tool: needsInput.tool)
        }
        let now = Date().timeIntervalSince1970
        if let completed = trackedSessions.first(where: {
            $0.state == .idle
                && $0.wasInterrupted == false
                && $0.hasCompletedTurn
                && now - $0.updatedAt <= completedPhaseLifetime
        }) {
            return .completed(
                tool: completed.tool,
                finishedAt: Date(timeIntervalSince1970: completed.updatedAt),
                exitCode: nil
            )
        }
        return .idle
    }

    struct WaitingInputContext: Equatable, Sendable {
        var tool: String
        var updatedAt: Double
        var notificationType: String?
        var targetToolName: String?
        var message: String?
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
            "terminal=\(session.terminalID.uuidString) tool=\(session.tool) state=\(session.state.rawValue) external=\(session.aiSessionID ?? "nil") total=\(session.committedTotalTokens)"
        }
        .joined(separator: " | ")
    }

    private func seedSessionOnStart(_ session: inout TerminalSessionState, event: AIHookEvent) {
        if let totalTokens = event.totalTokens {
            session.committedTotalTokens = max(session.committedTotalTokens, totalTokens)
            session.baselineTotalTokens = min(session.baselineTotalTokens, session.committedTotalTokens)
        }
        session.state = .idle
        session.wasInterrupted = false
        session.hasCompletedTurn = false
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
    }

    private func applyPromptSubmitted(_ session: inout TerminalSessionState, event: AIHookEvent) {
        let currentTotal = resolvedCommittedTotalTokens(for: session, incomingTotalTokens: event.totalTokens)
        session.state = .responding
        session.wasInterrupted = false
        session.hasCompletedTurn = false
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
        session.committedTotalTokens = currentTotal
        session.baselineTotalTokens = currentTotal
    }

    private func applyTurnCompleted(_ session: inout TerminalSessionState, event: AIHookEvent) {
        let committedTotal = resolvedCommittedTotalTokens(for: session, incomingTotalTokens: event.totalTokens)
        let wasInterrupted = event.metadata?.wasInterrupted == true
        session.state = .idle
        session.wasInterrupted = wasInterrupted
        session.hasCompletedTurn = event.metadata?.hasCompletedTurn ?? !wasInterrupted
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
        session.committedTotalTokens = committedTotal
    }

    private func resolvedCommittedTotalTokens(for session: TerminalSessionState, incomingTotalTokens: Int?) -> Int {
        let logicalTotal = session.logicalSessionKey.flatMap { logicalSessionsByKey[$0]?.totalTokens } ?? 0
        return max(session.committedTotalTokens, logicalTotal, incomingTotalTokens ?? 0)
    }

    private func reconcileLogicalSession(for session: TerminalSessionState, previousLogicalKey: LogicalSessionKey?) {
        guard let logicalKey = session.logicalSessionKey else {
            return
        }
        let totalTokens = max(session.committedTotalTokens, session.baselineTotalTokens)
        let nextState = LogicalSessionState(
            key: logicalKey,
            model: session.model,
            totalTokens: totalTokens,
            updatedAt: session.updatedAt,
            hasCompletedTurn: session.hasCompletedTurn
        )
        if let existing = logicalSessionsByKey[logicalKey] {
            logicalSessionsByKey[logicalKey] = LogicalSessionState(
                key: logicalKey,
                model: nextState.model ?? existing.model,
                totalTokens: max(existing.totalTokens, nextState.totalTokens),
                updatedAt: max(existing.updatedAt, nextState.updatedAt),
                hasCompletedTurn: existing.hasCompletedTurn || nextState.hasCompletedTurn
            )
        } else {
            logicalSessionsByKey[logicalKey] = nextState
        }
        if previousLogicalKey != logicalKey {
            pruneLogicalSessionIfUnused(previousLogicalKey)
        }
    }

    private func pruneLogicalSessionIfUnused(_ logicalKey: LogicalSessionKey?) {
        guard let logicalKey else {
            return
        }
        let stillReferenced = terminalSessionsByID.values.contains { $0.logicalSessionKey == logicalKey }
        if !stillReferenced {
            logicalSessionsByKey[logicalKey] = nil
        }
    }

    private func clearExpectedLogicalSessionIfMatched(_ session: TerminalSessionState) {
        guard let expected = expectedLogicalSessionsByTerminalID[session.terminalID],
              expected.tool == session.tool,
              expected.aiSessionID == session.aiSessionID else {
            return
        }
        expectedLogicalSessionsByTerminalID[session.terminalID] = nil
    }

    private func resolvedExpectedLogicalSessionID(for terminalID: UUID, tool: String) -> String? {
        guard let expected = expectedLogicalSessionsByTerminalID[terminalID],
              expected.tool == tool else {
            return nil
        }
        return expected.aiSessionID
    }

    private func shouldIgnore(event: AIHookEvent, terminalInstanceID: String?) -> Bool {
        guard let existing = terminalSessionsByID[event.terminalID],
              let terminalInstanceID,
              let existingInstanceID = existing.terminalInstanceID,
              existingInstanceID != terminalInstanceID else {
            return false
        }
        return event.updatedAt < existing.updatedAt
    }

    private func snapshot(from session: TerminalSessionState) -> AITerminalSessionSnapshot {
        AITerminalSessionSnapshot(
            sessionID: session.terminalID,
            externalSessionID: session.aiSessionID,
            projectID: session.projectID,
            projectName: session.projectName,
            sessionTitle: session.sessionTitle,
            tool: session.tool,
            model: session.model,
            status: session.status,
            isRunning: session.state == .responding,
            startedAt: session.startedAt.map { Date(timeIntervalSince1970: $0) },
            updatedAt: Date(timeIntervalSince1970: session.updatedAt),
            currentInputTokens: session.committedTotalTokens,
            currentOutputTokens: 0,
            currentTotalTokens: session.committedTotalTokens,
            baselineInputTokens: session.baselineTotalTokens,
            baselineOutputTokens: 0,
            baselineTotalTokens: session.baselineTotalTokens,
            currentContextWindow: nil,
            currentContextUsedTokens: nil,
            currentContextUsagePercent: nil,
            wasInterrupted: session.wasInterrupted,
            hasCompletedTurn: session.hasCompletedTurn
        )
    }

    private func makeFreshSessionState(
        event: AIHookEvent,
        tool: String,
        aiSessionID: String?,
        model: String?,
        terminalInstanceID: String?
    ) -> TerminalSessionState {
        TerminalSessionState(
            terminalID: event.terminalID,
            terminalInstanceID: terminalInstanceID,
            projectID: event.projectID,
            projectName: event.projectName,
            projectPath: normalizedNonEmptyString(event.projectPath),
            sessionTitle: event.sessionTitle,
            tool: tool,
            aiSessionID: aiSessionID,
            state: .idle,
            model: model,
            baselineTotalTokens: 0,
            committedTotalTokens: 0,
            updatedAt: event.updatedAt,
            startedAt: event.updatedAt,
            wasInterrupted: false,
            hasCompletedTurn: false,
            transcriptPath: normalizedNonEmptyString(event.metadata?.transcriptPath),
            notificationType: event.metadata?.notificationType,
            targetToolName: event.metadata?.targetToolName,
            interactionMessage: event.metadata?.message
        )
    }

    private func shouldResetSessionState(
        existing: TerminalSessionState,
        incomingTool: String,
        incomingAISessionID: String?,
        incomingTerminalInstanceID: String?
    ) -> Bool {
        if let incomingTerminalInstanceID,
           let existingTerminalInstanceID = existing.terminalInstanceID,
           existingTerminalInstanceID != incomingTerminalInstanceID {
            return true
        }

        if existing.tool != incomingTool {
            return true
        }

        if let existingSessionID = normalizedNonEmptyString(existing.aiSessionID),
           let incomingAISessionID,
           existingSessionID != incomingAISessionID {
            return true
        }

        return false
    }

    private func canonicalToolName(_ tool: String) -> String {
        toolDriverFactory.canonicalToolName(tool.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    private func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }
}
