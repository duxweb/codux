import Foundation
import Observation

@MainActor
@Observable
final class AISessionStore {
    static let shared = AISessionStore()
    let runningPhaseLifetime: TimeInterval = 30

    private enum PromptSubmittedSource {
        case userInput
        case toolUse
    }

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
        var inputTokens: Int
        var outputTokens: Int
        var cachedInputTokens: Int
        var totalTokens: Int
        var updatedAt: Double
        var hasCompletedTurn: Bool
    }

    struct ExpectedLogicalSession: Equatable, Sendable {
        var tool: String
        var aiSessionID: String
    }

    struct RuntimeTokenObservation: Equatable, Sendable {
        var inputTokens: Int
        var outputTokens: Int
        var cachedInputTokens: Int
        var totalTokens: Int
        var observedAt: Double
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
        var baselineInputTokens: Int
        var committedInputTokens: Int
        var baselineOutputTokens: Int
        var committedOutputTokens: Int
        var baselineCachedInputTokens: Int
        var committedCachedInputTokens: Int
        var baselineTotalTokens: Int
        var committedTotalTokens: Int
        var baselineResolved: Bool
        var updatedAt: Double
        var attachedAt: Double
        var startedAt: Double?
        var activeTurnStartedAt: Double?
        var runtimeTurnStartedAt: Double?
        var needsCachedBaseline: Bool
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

        init(
            terminalID: UUID,
            terminalInstanceID: String? = nil,
            projectID: UUID,
            projectName: String,
            projectPath: String? = nil,
            sessionTitle: String,
            tool: String,
            aiSessionID: String? = nil,
            state: State,
            model: String? = nil,
            baselineInputTokens: Int = 0,
            committedInputTokens: Int = 0,
            baselineOutputTokens: Int = 0,
            committedOutputTokens: Int = 0,
            baselineCachedInputTokens: Int = 0,
            committedCachedInputTokens: Int = 0,
            baselineTotalTokens: Int = 0,
            committedTotalTokens: Int = 0,
            baselineResolved: Bool = false,
            updatedAt: Double,
            attachedAt: Double? = nil,
            startedAt: Double? = nil,
            activeTurnStartedAt: Double? = nil,
            runtimeTurnStartedAt: Double? = nil,
            needsCachedBaseline: Bool = false,
            wasInterrupted: Bool,
            hasCompletedTurn: Bool,
            transcriptPath: String? = nil,
            notificationType: String? = nil,
            targetToolName: String? = nil,
            interactionMessage: String? = nil
        ) {
            self.terminalID = terminalID
            self.terminalInstanceID = terminalInstanceID
            self.projectID = projectID
            self.projectName = projectName
            self.projectPath = projectPath
            self.sessionTitle = sessionTitle
            self.tool = tool
            self.aiSessionID = aiSessionID
            self.state = state
            self.model = model
            self.baselineInputTokens = baselineInputTokens
            self.committedInputTokens = committedInputTokens
            self.baselineOutputTokens = baselineOutputTokens
            self.committedOutputTokens = committedOutputTokens
            self.baselineCachedInputTokens = baselineCachedInputTokens
            self.committedCachedInputTokens = committedCachedInputTokens
            self.baselineTotalTokens = baselineTotalTokens
            self.committedTotalTokens = committedTotalTokens
            self.baselineResolved = baselineResolved
            self.updatedAt = updatedAt
            self.attachedAt = attachedAt ?? updatedAt
            self.startedAt = startedAt
            self.activeTurnStartedAt = activeTurnStartedAt
            self.runtimeTurnStartedAt = runtimeTurnStartedAt
            self.needsCachedBaseline = needsCachedBaseline
            self.wasInterrupted = wasInterrupted
            self.hasCompletedTurn = hasCompletedTurn
            self.transcriptPath = transcriptPath
            self.notificationType = notificationType
            self.targetToolName = targetToolName
            self.interactionMessage = interactionMessage
        }
    }

    private let logger = AppDebugLog.shared
    private let toolDriverFactory = AIToolDriverFactory.shared

    private(set) var terminalSessionsByID: [UUID: TerminalSessionState] = [:]
    private(set) var logicalSessionsByKey: [LogicalSessionKey: LogicalSessionState] = [:]
    private var expectedLogicalSessionsByTerminalID: [UUID: ExpectedLogicalSession] = [:]
    private var runtimeTokenObservationsByLogicalKey: [LogicalSessionKey: RuntimeTokenObservation] = [:]
    var renderVersion: UInt64 = 0 {
        didSet {
            onRenderVersionChange?()
        }
    }
    var onRenderVersionChange: (@MainActor () -> Void)?

    private init() {}

    func reset() {
        terminalSessionsByID.removeAll()
        logicalSessionsByKey.removeAll()
        expectedLogicalSessionsByTerminalID.removeAll()
        runtimeTokenObservationsByLogicalKey.removeAll()
        renderVersion &+= 1
        logger.log("ai-session-store", "reset")
    }

    func registerExpectedLogicalSession(
        terminalID: UUID,
        tool: String,
        aiSessionID: String
    ) {
        let normalizedTool = canonicalToolName(tool)
        let normalizedSessionID = normalizedNonEmptyString(aiSessionID)
        guard let normalizedSessionID else {
            return
        }
        expectedLogicalSessionsByTerminalID[terminalID] = ExpectedLogicalSession(
            tool: normalizedTool,
            aiSessionID: normalizedSessionID
        )
    }

    func clearExpectedLogicalSession(terminalID: UUID) {
        expectedLogicalSessionsByTerminalID[terminalID] = nil
    }

    func apply(_ event: AIHookEvent) -> Bool {
        let normalizedTool = canonicalToolName(event.tool)
        let directAISessionID = normalizedNonEmptyString(event.aiSessionID)
        let expectedAISessionID = resolvedExpectedLogicalSessionID(for: event.terminalID, tool: normalizedTool)
        let normalizedAISessionID = directAISessionID ?? expectedAISessionID
        let needsCachedBaseline = directAISessionID == nil && expectedAISessionID != nil
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

        if shouldIgnoreInternalCodexEvent(event: event) {
            logger.log(
                "ai-session-store",
                "ignore terminal=\(event.terminalID.uuidString) tool=\(normalizedTool) kind=\(event.kind.rawValue) reason=internal-codex-memory-session"
            )
            return false
        }

        if shouldIgnoreToolActivityEvent(event: event) {
            logger.log(
                "ai-session-store",
                "ignore terminal=\(event.terminalID.uuidString) tool=\(normalizedTool) kind=\(event.kind.rawValue) reason=tool-activity-without-loading"
            )
            return false
        }

        var session = terminalSessionsByID[event.terminalID] ?? makeFreshSessionState(
            event: event,
            tool: normalizedTool,
            aiSessionID: normalizedAISessionID,
            model: normalizedModel,
            terminalInstanceID: normalizedInstanceID,
            needsCachedBaseline: needsCachedBaseline
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
                terminalInstanceID: normalizedInstanceID,
                needsCachedBaseline: needsCachedBaseline
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
        session.needsCachedBaseline = session.needsCachedBaseline || needsCachedBaseline
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
            applySessionStarted(&session)
        case .promptSubmitted:
            applyPromptSubmitted(&session, event: event)
        case .needsInput:
            session.state = .needsInput
            session.wasInterrupted = false
        case .turnCompleted:
            applyTurnCompleted(&session, event: event)
        case .sessionEnded:
            if shouldRetainCompletedSessionOnEnd(session) {
                terminalSessionsByID[event.terminalID] = session
                expectedLogicalSessionsByTerminalID[event.terminalID] = nil
                reconcileLogicalSession(for: session, previousLogicalKey: previousLogicalKey)
                pruneLogicalSessionIfUnused(previousLogicalKey)
                let didChange = previousState != session
                if didChange {
                    renderVersion &+= 1
                    logger.log(
                        "ai-session-store",
                        "retain-completed-on-end terminal=\(event.terminalID.uuidString) tool=\(normalizedTool) external=\(normalizedAISessionID ?? "nil")"
                    )
                }
                return didChange
            }
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
                inputTokens: envelope.inputTokens,
                outputTokens: envelope.outputTokens,
                cachedInputTokens: envelope.cachedInputTokens,
                totalTokens: envelope.totalTokens,
                updatedAt: envelope.updatedAt,
                metadata: nil
            )
        )
    }

    func removeTerminal(_ terminalID: UUID) {
        let previousLogicalKey = terminalSessionsByID[terminalID]?.logicalSessionKey
        terminalSessionsByID[terminalID] = nil
        expectedLogicalSessionsByTerminalID[terminalID] = nil
        pruneLogicalSessionIfUnused(previousLogicalKey)
        renderVersion &+= 1
        logger.log("ai-session-store", "remove terminal=\(terminalID.uuidString)")
    }

    func removeMissingManagedTerminalSessions(liveInstanceIDs: Set<String>) -> [UUID] {
        var removedTerminalIDs: [UUID] = []

        for (terminalID, session) in terminalSessionsByID {
            guard let instanceID = normalizedNonEmptyString(session.terminalInstanceID),
                  liveInstanceIDs.contains(instanceID) == false else {
                continue
            }
            removedTerminalIDs.append(terminalID)
        }

        guard !removedTerminalIDs.isEmpty else {
            return []
        }

        for terminalID in removedTerminalIDs {
            let previousLogicalKey = terminalSessionsByID[terminalID]?.logicalSessionKey
            terminalSessionsByID[terminalID] = nil
            expectedLogicalSessionsByTerminalID[terminalID] = nil
            pruneLogicalSessionIfUnused(previousLogicalKey)
        }

        renderVersion &+= 1
        let removedTerminalList = removedTerminalIDs.map(\.uuidString).joined(separator: ",")
        logger.log(
            "ai-session-store",
            "remove-missing terminals=\(removedTerminalList)"
        )
        return removedTerminalIDs
    }

    private func shouldRetainCompletedSessionOnEnd(_ session: TerminalSessionState) -> Bool {
        session.state == .idle && session.wasInterrupted == false && session.hasCompletedTurn
    }

    func session(for terminalID: UUID) -> TerminalSessionState? {
        terminalSessionsByID[terminalID]
    }

    func tool(for terminalID: UUID) -> String? {
        terminalSessionsByID[terminalID]?.tool
    }

    func isRunning(terminalID: UUID) -> Bool {
        guard let session = terminalSessionsByID[terminalID],
              session.state == .responding else {
            return false
        }
        let now = Date().timeIntervalSince1970
        return max(0, now - session.updatedAt) <= runningPhaseLifetime
    }

    func clearCompleted(projectID: UUID) -> Bool {
        var didChange = false

        for terminalID in terminalSessionsByID.keys {
            guard var session = terminalSessionsByID[terminalID],
                  session.projectID == projectID,
                  session.state == .idle,
                  session.wasInterrupted == false,
                  session.hasCompletedTurn else {
                continue
            }

            session.hasCompletedTurn = false
            terminalSessionsByID[terminalID] = session
            reconcileLogicalSession(for: session, previousLogicalKey: session.logicalSessionKey)
            didChange = true
        }

        if didChange {
            renderVersion &+= 1
            logger.log("ai-session-store", "clear-completed project=\(projectID.uuidString)")
        }

        return didChange
    }

    struct WaitingInputContext: Equatable, Sendable {
        var tool: String
        var updatedAt: Double
        var notificationType: String?
        var targetToolName: String?
        var message: String?
    }

    func applyRuntimeSnapshot(
        terminalID: UUID,
        snapshot: AIRuntimeContextSnapshot
    ) -> Bool {
        guard let seedSession = terminalSessionsByID[terminalID] else {
            return false
        }

        let normalizedSessionID = normalizedNonEmptyString(snapshot.externalSessionID)
        let normalizedModel = normalizedNonEmptyString(snapshot.model)
        let targetLogicalKey = normalizedSessionID.map {
            LogicalSessionKey(tool: canonicalToolName(snapshot.tool), aiSessionID: $0)
        }

        let targetTerminalIDs: [UUID]
        if let targetLogicalKey {
            targetTerminalIDs = terminalSessionsByID.compactMap { terminalID, session in
                if terminalID == seedSession.terminalID {
                    return terminalID
                }
                return session.logicalSessionKey == targetLogicalKey ? terminalID : nil
            }
        } else {
            targetTerminalIDs = [terminalID]
        }

        var didChange = false
        for targetTerminalID in targetTerminalIDs {
            guard var session = terminalSessionsByID[targetTerminalID] else {
                continue
            }

            let previousLogicalKey = session.logicalSessionKey
            let previousState = session

            if let normalizedSessionID {
                session.aiSessionID = normalizedSessionID
            }
            if let normalizedModel {
                session.model = normalizedModel
            }
            resolveBaselineIfNeeded(&session, snapshot: snapshot)
            applyRuntimeLifecycle(&session, snapshot: snapshot, previousState: previousState)
            session.committedInputTokens = max(session.committedInputTokens, snapshot.inputTokens)
            session.committedOutputTokens = max(session.committedOutputTokens, snapshot.outputTokens)
            session.committedCachedInputTokens = max(session.committedCachedInputTokens, snapshot.cachedInputTokens)
            session.committedTotalTokens = max(session.committedTotalTokens, snapshot.totalTokens)

            terminalSessionsByID[targetTerminalID] = session
            recordRuntimeObservation(for: session, snapshot: snapshot)
            reconcileLogicalSession(for: session, previousLogicalKey: previousLogicalKey)
            pruneLogicalSessionIfUnused(previousLogicalKey)

            if previousState != session {
                didChange = true
            }
        }

        if didChange {
            renderVersion &+= 1
            logger.log(
                "ai-session-store",
                "runtime terminal=\(terminalID.uuidString) tool=\(snapshot.tool) external=\(snapshot.externalSessionID ?? "nil") total=\(snapshot.totalTokens)"
            )
        }
        return didChange
    }

    private func applySessionStarted(_ session: inout TerminalSessionState) {
        session.state = .idle
        session.activeTurnStartedAt = nil
        session.runtimeTurnStartedAt = nil
        session.wasInterrupted = false
        session.hasCompletedTurn = false
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
    }

    private func applyPromptSubmitted(_ session: inout TerminalSessionState, event: AIHookEvent) {
        session.state = .responding
        session.activeTurnStartedAt = event.updatedAt
        session.runtimeTurnStartedAt = nil
        session.wasInterrupted = false
        session.hasCompletedTurn = false
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
    }

    private func applyTurnCompleted(_ session: inout TerminalSessionState, event: AIHookEvent) {
        let wasInterrupted = event.metadata?.wasInterrupted == true
        session.state = .idle
        session.activeTurnStartedAt = nil
        session.runtimeTurnStartedAt = nil
        session.wasInterrupted = wasInterrupted
        session.hasCompletedTurn = event.metadata?.hasCompletedTurn ?? !wasInterrupted
        session.notificationType = nil
        session.targetToolName = nil
        session.interactionMessage = nil
    }

    private func applyRuntimeLifecycle(
        _ session: inout TerminalSessionState,
        snapshot: AIRuntimeContextSnapshot,
        previousState: TerminalSessionState
    ) {
        guard let responseState = snapshot.responseState else {
            return
        }

        let snapshotIsNewer = snapshot.updatedAt > previousState.updatedAt
        let promptTurnStartedAt = previousState.activeTurnStartedAt ?? previousState.updatedAt
        let runtimeTurnStartedAt = {
            guard responseState == .responding else {
                return snapshot.startedAt
            }
            if let startedAt = snapshot.startedAt,
               startedAt >= promptTurnStartedAt {
                return startedAt
            }
            return snapshot.updatedAt
        }()
        let turnCompletedAt = snapshot.completedAt ?? (
            snapshot.wasInterrupted || snapshot.hasCompletedTurn ? snapshot.updatedAt : nil
        )

        switch responseState {
        case .responding:
            let canPromoteToResponding =
                previousState.state == .responding
                || (
                    previousState.hasCompletedTurn == false
                    && previousState.wasInterrupted == false
                    && (snapshotIsNewer || previousState.state == .idle)
                )

            guard canPromoteToResponding else {
                return
            }

            if snapshotIsNewer {
                session.updatedAt = snapshot.updatedAt
            }
            if let runtimeTurnStartedAt {
                session.runtimeTurnStartedAt = runtimeTurnStartedAt
            } else if session.runtimeTurnStartedAt == nil {
                session.runtimeTurnStartedAt = promptTurnStartedAt
            }
            if previousState.state != .responding || previousState.wasInterrupted || previousState.hasCompletedTurn {
                session.state = .responding
                session.startedAt = runtimeTurnStartedAt ?? snapshot.updatedAt
                session.activeTurnStartedAt = runtimeTurnStartedAt ?? snapshot.updatedAt
                if let runtimeTurnStartedAt {
                    session.startedAt = runtimeTurnStartedAt
                }
                session.wasInterrupted = false
                session.hasCompletedTurn = false
                session.notificationType = nil
                session.targetToolName = nil
                session.interactionMessage = nil
            }

        case .idle:
            let idleSnapshotCanResolveTurn =
                if snapshot.wasInterrupted || snapshot.hasCompletedTurn {
                    if let turnCompletedAt {
                        turnCompletedAt >= promptTurnStartedAt
                    } else {
                        false
                    }
                } else if previousState.state == .needsInput {
                    true
                } else if let observedTurnStartedAt = previousState.runtimeTurnStartedAt {
                    observedTurnStartedAt >= promptTurnStartedAt && snapshot.updatedAt >= observedTurnStartedAt
                } else {
                    false
                }

            let shouldResolveToIdle =
                idleSnapshotCanResolveTurn
                && (
                    previousState.state == .responding
                    || previousState.state == .needsInput
                    || snapshot.wasInterrupted
                    || snapshot.hasCompletedTurn
                )

            guard shouldResolveToIdle else {
                return
            }

            if snapshotIsNewer {
                session.updatedAt = snapshot.updatedAt
            }

            if snapshot.wasInterrupted {
                session.state = .idle
                session.activeTurnStartedAt = nil
                session.runtimeTurnStartedAt = nil
                session.wasInterrupted = true
                session.hasCompletedTurn = false
            } else if snapshot.hasCompletedTurn {
                session.state = .idle
                session.activeTurnStartedAt = nil
                session.runtimeTurnStartedAt = nil
                session.wasInterrupted = false
                session.hasCompletedTurn = true
            } else if previousState.state == .responding || previousState.state == .needsInput {
                session.state = .idle
                session.activeTurnStartedAt = nil
                session.runtimeTurnStartedAt = nil
                session.wasInterrupted = false
                session.hasCompletedTurn = false
            }

            session.notificationType = nil
            session.targetToolName = nil
            session.interactionMessage = nil
        }
    }

    private func resolveBaselineIfNeeded(
        _ session: inout TerminalSessionState,
        snapshot: AIRuntimeContextSnapshot
    ) {
        guard session.baselineResolved == false else {
            return
        }

        if let observation = runtimeObservationForBaseline(of: session, snapshot: snapshot) {
            session.baselineInputTokens = observation.inputTokens
            session.baselineOutputTokens = observation.outputTokens
            session.baselineCachedInputTokens = observation.cachedInputTokens
            session.baselineTotalTokens = observation.totalTokens
        } else if session.needsCachedBaseline || snapshot.sessionOrigin == .restored {
            session.baselineInputTokens = snapshot.inputTokens
            session.baselineOutputTokens = snapshot.outputTokens
            session.baselineCachedInputTokens = snapshot.cachedInputTokens
            session.baselineTotalTokens = snapshot.totalTokens
        } else {
            session.baselineInputTokens = 0
            session.baselineOutputTokens = 0
            session.baselineCachedInputTokens = 0
            session.baselineTotalTokens = 0
        }

        session.baselineResolved = true
    }

    private func runtimeObservationForBaseline(
        of session: TerminalSessionState,
        snapshot: AIRuntimeContextSnapshot
    ) -> RuntimeTokenObservation? {
        guard let logicalKey = runtimeLogicalKey(for: session, snapshot: snapshot),
              let observation = runtimeTokenObservationsByLogicalKey[logicalKey] else {
            return nil
        }
        return observation.observedAt <= session.attachedAt ? observation : nil
    }

    private func recordRuntimeObservation(
        for session: TerminalSessionState,
        snapshot: AIRuntimeContextSnapshot
    ) {
        guard let logicalKey = runtimeLogicalKey(for: session, snapshot: snapshot) else {
            return
        }

        let observation = RuntimeTokenObservation(
            inputTokens: snapshot.inputTokens,
            outputTokens: snapshot.outputTokens,
            cachedInputTokens: snapshot.cachedInputTokens,
            totalTokens: snapshot.totalTokens,
            observedAt: snapshot.updatedAt
        )

        if let existing = runtimeTokenObservationsByLogicalKey[logicalKey],
           existing.observedAt > observation.observedAt {
            return
        }

        runtimeTokenObservationsByLogicalKey[logicalKey] = observation
    }

    private func runtimeLogicalKey(
        for session: TerminalSessionState,
        snapshot: AIRuntimeContextSnapshot
    ) -> LogicalSessionKey? {
        guard let aiSessionID = normalizedNonEmptyString(snapshot.externalSessionID ?? session.aiSessionID) else {
            return nil
        }
        return LogicalSessionKey(
            tool: canonicalToolName(snapshot.tool.isEmpty ? session.tool : snapshot.tool),
            aiSessionID: aiSessionID
        )
    }

    private func reconcileLogicalSession(for session: TerminalSessionState, previousLogicalKey: LogicalSessionKey?) {
        guard let logicalKey = session.logicalSessionKey else {
            return
        }
        let nextState = LogicalSessionState(
            key: logicalKey,
            model: session.model,
            inputTokens: session.committedInputTokens,
            outputTokens: session.committedOutputTokens,
            cachedInputTokens: session.committedCachedInputTokens,
            totalTokens: session.committedTotalTokens,
            updatedAt: session.updatedAt,
            hasCompletedTurn: session.hasCompletedTurn
        )
        if let existing = logicalSessionsByKey[logicalKey] {
            logicalSessionsByKey[logicalKey] = LogicalSessionState(
                key: logicalKey,
                model: nextState.model ?? existing.model,
                inputTokens: max(existing.inputTokens, nextState.inputTokens),
                outputTokens: max(existing.outputTokens, nextState.outputTokens),
                cachedInputTokens: max(existing.cachedInputTokens, nextState.cachedInputTokens),
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

    private func shouldIgnoreToolActivityEvent(event: AIHookEvent) -> Bool {
        guard promptSubmittedSource(for: event) == .toolUse else {
            return false
        }
        return true
    }

    private func shouldIgnoreInternalCodexEvent(event: AIHookEvent) -> Bool {
        guard normalizedNonEmptyString(event.tool)?.lowercased() == "codex",
              let projectPath = normalizedProjectPath(event.projectPath) else {
            return false
        }

        let memoriesPath = codexMemoriesRootPath
        return projectPath == memoriesPath || projectPath.hasPrefix(memoriesPath + "/")
    }

    private func promptSubmittedSource(for event: AIHookEvent) -> PromptSubmittedSource {
        switch normalizedNonEmptyString(event.metadata?.source) {
        case "tool-use", "pre-tool-use", "post-tool-use", "post-tool-use-failure":
            return .toolUse
        default:
            return .userInput
        }
    }

    private func makeFreshSessionState(
        event: AIHookEvent,
        tool: String,
        aiSessionID: String?,
        model: String?,
        terminalInstanceID: String?,
        needsCachedBaseline: Bool
    ) -> TerminalSessionState {
        return TerminalSessionState(
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
            baselineInputTokens: 0,
            committedInputTokens: 0,
            baselineOutputTokens: 0,
            committedOutputTokens: 0,
            baselineCachedInputTokens: 0,
            committedCachedInputTokens: 0,
            baselineTotalTokens: 0,
            committedTotalTokens: 0,
            baselineResolved: false,
            updatedAt: event.updatedAt,
            attachedAt: event.updatedAt,
            startedAt: event.updatedAt,
            activeTurnStartedAt: nil,
            runtimeTurnStartedAt: nil,
            needsCachedBaseline: needsCachedBaseline,
            wasInterrupted: false,
            hasCompletedTurn: false,
            transcriptPath: normalizedNonEmptyString(event.metadata?.transcriptPath),
            notificationType: event.metadata?.notificationType,
            targetToolName: event.metadata?.targetToolName,
            interactionMessage: event.metadata?.message
        )
    }

    private var codexMemoriesRootPath: String {
        URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent(".codex/memories", isDirectory: true)
            .standardizedFileURL
            .path
    }

    private func normalizedProjectPath(_ path: String?) -> String? {
        normalizedComparablePath(path)
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

}
