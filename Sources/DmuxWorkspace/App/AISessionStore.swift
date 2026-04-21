import Foundation
import Observation

@MainActor
@Observable
final class AISessionStore {
    static let shared = AISessionStore()

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
        var turnSequence: UInt64
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
    }

    struct CodexPollingTarget: Equatable, Sendable {
        var terminalID: UUID
        var projectPath: String
        var aiSessionID: String
        var transcriptPath: String?
        var turnSequence: UInt64
        var updatedAt: Double
    }

    struct RuntimeResolution: Equatable, Sendable {
        var terminalID: UUID
        var turnSequence: UInt64
        var updatedAt: Double
        var model: String?
        var totalTokens: Int?
        var transcriptPath: String?
        var wasInterrupted: Bool
        var hasCompletedTurn: Bool
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

        var session = terminalSessionsByID[event.terminalID] ?? TerminalSessionState(
            terminalID: event.terminalID,
            terminalInstanceID: normalizedInstanceID,
            projectID: event.projectID,
            projectName: event.projectName,
            projectPath: normalizedNonEmptyString(event.projectPath),
            sessionTitle: event.sessionTitle,
            tool: normalizedTool,
            aiSessionID: normalizedAISessionID,
            state: .idle,
            model: normalizedModel,
            baselineTotalTokens: 0,
            committedTotalTokens: 0,
            updatedAt: event.updatedAt,
            startedAt: event.updatedAt,
            wasInterrupted: false,
            hasCompletedTurn: false,
            transcriptPath: normalizedNonEmptyString(event.metadata?.transcriptPath),
            turnSequence: 0,
            notificationType: event.metadata?.notificationType,
            targetToolName: event.metadata?.targetToolName,
            interactionMessage: event.metadata?.message
        )

        if normalizedInstanceID != nil, session.terminalInstanceID != normalizedInstanceID {
            session = TerminalSessionState(
                terminalID: event.terminalID,
                terminalInstanceID: normalizedInstanceID,
                projectID: event.projectID,
                projectName: event.projectName,
                projectPath: normalizedNonEmptyString(event.projectPath),
                sessionTitle: event.sessionTitle,
                tool: normalizedTool,
                aiSessionID: normalizedAISessionID,
                state: .idle,
                model: normalizedModel,
                baselineTotalTokens: 0,
                committedTotalTokens: 0,
                updatedAt: event.updatedAt,
                startedAt: event.updatedAt,
                wasInterrupted: false,
                hasCompletedTurn: false,
                transcriptPath: normalizedNonEmptyString(event.metadata?.transcriptPath),
                turnSequence: 0,
                notificationType: event.metadata?.notificationType,
                targetToolName: event.metadata?.targetToolName,
                interactionMessage: event.metadata?.message
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

        let previousLogicalKey = terminalSessionsByID[event.terminalID]?.logicalSessionKey
        let previousState = terminalSessionsByID[event.terminalID]

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
            session.state = .idle
            session.notificationType = nil
            session.targetToolName = nil
            session.interactionMessage = nil
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

    func applyRuntimeResolution(_ resolution: RuntimeResolution, source: String = "polling") -> Bool {
        guard var session = terminalSessionsByID[resolution.terminalID],
              canonicalToolName(session.tool) == "codex",
              session.state == .responding,
              session.turnSequence == resolution.turnSequence else {
            return false
        }

        let previousLogicalKey = session.logicalSessionKey
        let previousState = session
        session.state = .idle
        session.wasInterrupted = resolution.wasInterrupted
        session.hasCompletedTurn = resolution.hasCompletedTurn
        session.updatedAt = max(session.updatedAt, resolution.updatedAt)
        session.model = normalizedNonEmptyString(resolution.model) ?? session.model
        if let transcriptPath = normalizedNonEmptyString(resolution.transcriptPath) {
            session.transcriptPath = transcriptPath
        }
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
        session.committedTotalTokens = resolvedCommittedTotalTokens(
            for: session,
            incomingTotalTokens: resolution.totalTokens
        )

        terminalSessionsByID[resolution.terminalID] = session
        reconcileLogicalSession(for: session, previousLogicalKey: previousLogicalKey)
        let didChange = previousState != session
        if didChange {
            renderVersion &+= 1
            logger.log(
                "ai-session-store",
                "resolve source=\(source) terminal=\(resolution.terminalID.uuidString) tool=\(session.tool) interrupted=\(session.wasInterrupted) completed=\(session.hasCompletedTurn) total=\(session.committedTotalTokens) turn=\(session.turnSequence)"
            )
        }
        return didChange
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

    func runtimeContext(for terminalID: UUID) -> AIRuntimeContextSnapshot? {
        guard let session = terminalSessionsByID[terminalID] else {
            return nil
        }
        return AIRuntimeContextSnapshot(
            tool: session.tool,
            externalSessionID: session.aiSessionID,
            model: session.model,
            inputTokens: session.committedTotalTokens,
            outputTokens: 0,
            totalTokens: session.committedTotalTokens,
            updatedAt: session.updatedAt,
            responseState: session.state == .responding ? .responding : .idle,
            wasInterrupted: session.wasInterrupted,
            hasCompletedTurn: session.hasCompletedTurn,
            source: .hook
        )
    }

    func isRunning(terminalID: UUID) -> Bool {
        terminalSessionsByID[terminalID]?.state == .responding
    }

    func codexPollingTargets() -> [CodexPollingTarget] {
        terminalSessionsByID.values
            .filter {
                canonicalToolName($0.tool) == "codex"
                    && $0.state == .responding
                    && normalizedNonEmptyString($0.aiSessionID) != nil
                    && normalizedNonEmptyString($0.projectPath) != nil
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .compactMap { session in
                guard let projectPath = normalizedNonEmptyString(session.projectPath),
                      let aiSessionID = normalizedNonEmptyString(session.aiSessionID) else {
                    return nil
                }
                return CodexPollingTarget(
                    terminalID: session.terminalID,
                    projectPath: projectPath,
                    aiSessionID: aiSessionID,
                    transcriptPath: normalizedNonEmptyString(session.transcriptPath),
                    turnSequence: session.turnSequence,
                    updatedAt: session.updatedAt
                )
            }
    }

    func liveSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        terminalSessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
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
        if let responding = terminalSessionsByID.values
            .filter({ $0.projectID == projectID && $0.isLive })
            .sorted(by: { $0.updatedAt > $1.updatedAt })
            .first(where: { $0.state == .responding }) {
            return .running(tool: responding.tool)
        }
        if let needsInput = terminalSessionsByID.values
            .filter({ $0.projectID == projectID && $0.isLive })
            .sorted(by: { $0.updatedAt > $1.updatedAt })
            .first(where: { $0.state == .needsInput }) {
            return .waitingInput(tool: needsInput.tool)
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
        session.wasInterrupted = false
    }

    private func applyPromptSubmitted(_ session: inout TerminalSessionState, event: AIHookEvent) {
        let currentTotal = resolvedCommittedTotalTokens(for: session, incomingTotalTokens: event.totalTokens)
        session.state = .responding
        session.wasInterrupted = false
        session.hasCompletedTurn = false
        if canonicalToolName(event.tool) == "codex" {
            session.turnSequence &+= 1
        }
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
