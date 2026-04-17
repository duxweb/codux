import Foundation
import Observation

@MainActor
@Observable
final class AIRuntimeStateStore {
    static let shared = AIRuntimeStateStore()
    private let logger = AppDebugLog.shared
    private let toolDriverFactory = AIToolDriverFactory.shared
    private let usageStore = AIUsageStore()

    struct LogicalSessionKey: Hashable {
        var tool: String
        var externalSessionID: String
    }

    struct LogicalSessionState: Equatable {
        var key: LogicalSessionKey
        var model: String?
        var inputTokens: Int
        var outputTokens: Int
        var totalTokens: Int
        var committedInputTokens: Int
        var committedOutputTokens: Int
        var committedTotalTokens: Int
        var baselineInputTokens: Int
        var baselineOutputTokens: Int
        var baselineTotalTokens: Int
        var contextWindow: Int?
        var contextUsedTokens: Int?
        var contextUsagePercent: Double?
        var updatedAt: Double
        var pendingBaselineSeed: Bool
        var allowsBaselineRebase: Bool
        var suppressHistoricalModelUntilProgress: Bool
    }

    struct PendingLogicalAttachment: Equatable {
        var key: LogicalSessionKey
        var origin: AIRuntimeSessionOrigin
        var indexedSummary: AISessionSummary?
    }

    struct TerminalBindingState: Equatable {
        var sessionID: UUID
        var sessionInstanceID: String?
        var invocationID: String?
        var projectID: UUID
        var projectName: String
        var sessionTitle: String
        var tool: String
        var status: String
        var responseState: AIResponseState?
        var updatedAt: Double
        var startedAt: Double?
        var interruptedAt: Double?
        var hasCompletedTurn: Bool
        var lastKnownExternalSessionID: String?
        var lastKnownModel: String?
        var logicalSessionKey: LogicalSessionKey?
        var pendingSessionOrigin: AIRuntimeSessionOrigin
    }

    struct RuntimeSnapshotApplyResult {
        var previousContext: AIRuntimeContextSnapshot?
        var currentContext: AIRuntimeContextSnapshot
        var didChangeDisplay: Bool
        var didAdvance: Bool
        var didSwitchExternalSession: Bool
        var ignored: Bool
    }

    var renderVersion: UInt64 = 0

    private(set) var terminalBindingsByID: [UUID: TerminalBindingState] = [:]
    private var logicalSessionsByKey: [LogicalSessionKey: LogicalSessionState] = [:]
    private var terminalIDsByLogicalSessionKey: [LogicalSessionKey: Set<UUID>] = [:]
    private var pendingLogicalAttachmentsByTerminalID: [UUID: PendingLogicalAttachment] = [:]
    private var runtimeContextsByTerminalID: [UUID: AIRuntimeContextSnapshot] = [:]

    func registerExpectedLogicalSession(
        sessionID: UUID,
        tool: String,
        externalSessionID: String,
        indexedSummary: AISessionSummary? = nil
    ) {
        let normalizedTool = canonicalToolName(tool)
        guard let normalizedExternalSessionID = normalizedExternalSessionID(externalSessionID) else {
            return
        }
        let key = LogicalSessionKey(tool: normalizedTool, externalSessionID: normalizedExternalSessionID)
        pendingLogicalAttachmentsByTerminalID[sessionID] = PendingLogicalAttachment(
            key: key,
            origin: .restored,
            indexedSummary: indexedSummary
        )
        logger.log(
            "runtime-store",
            "attach-register session=\(sessionID.uuidString) logical=\(key.tool):\(key.externalSessionID) origin=restored indexedTotal=\(indexedSummary?.totalTokens ?? 0) indexedModel=\(indexedSummary?.lastModel ?? "nil")"
        )
    }

    func clearExpectedLogicalSession(sessionID: UUID) {
        guard let attachment = pendingLogicalAttachmentsByTerminalID.removeValue(forKey: sessionID) else {
            return
        }
        logger.log(
            "runtime-store",
            "attach-clear session=\(sessionID.uuidString) logical=\(attachment.key.tool):\(attachment.key.externalSessionID) origin=\(attachment.origin.rawValue)"
        )
    }

    func expectedExternalSessionID(for sessionID: UUID, tool: String? = nil) -> String? {
        guard let attachment = pendingLogicalAttachmentsByTerminalID[sessionID] else {
            return nil
        }
        if let tool, canonicalToolName(tool) != attachment.key.tool {
            return nil
        }
        return attachment.key.externalSessionID
    }

    func runtimeProbeExternalSessionHint(for sessionID: UUID, tool: String) -> String? {
        if let expected = expectedExternalSessionID(for: sessionID, tool: tool) {
            return expected
        }
        let usesHistoricalHint =
            toolDriverFactory.driver(for: tool)?.usesHistoricalExternalSessionHintForRuntimeProbe ?? true
        guard usesHistoricalHint else {
            return nil
        }
        return normalizedExternalSessionID(runtimeContextsByTerminalID[sessionID]?.externalSessionID)
            ?? terminalBindingsByID[sessionID]?.lastKnownExternalSessionID
    }

    func runtimeContext(for sessionID: UUID) -> AIRuntimeContextSnapshot? {
        runtimeContextsByTerminalID[sessionID]
    }

    func externalSessionID(for sessionID: UUID) -> String? {
        terminalBindingsByID[sessionID]?.logicalSessionKey?.externalSessionID
            ?? terminalBindingsByID[sessionID]?.lastKnownExternalSessionID
    }

    func applyLiveEnvelope(_ envelope: AIToolUsageEnvelope) {
        guard let sessionID = UUID(uuidString: envelope.sessionId),
              let projectID = UUID(uuidString: envelope.projectId) else {
            return
        }

        let existing = terminalBindingsByID[sessionID]
        let existingUpdatedAt = existing?.updatedAt ?? 0
        let pendingAttachment = pendingLogicalAttachmentsByTerminalID[sessionID]
        let incomingTool = envelope.tool.isEmpty ? (existing?.tool ?? "") : envelope.tool
        let incomingInvocationID = normalizedInvocationID(envelope.invocationId)
        let incomingSessionInstanceID = normalizedInvocationID(envelope.sessionInstanceId)
        let didSwitchInvocationContext =
            invocationContextDidChange(previous: existing?.invocationID, incoming: incomingInvocationID)
            || invocationContextDidChange(previous: existing?.sessionInstanceID, incoming: incomingSessionInstanceID)
        let nextTool = resolvedTool(currentTool: existing?.tool ?? "", incomingTool: incomingTool)
        let nextInvocationID = incomingInvocationID ?? existing?.invocationID
        let nextSessionInstanceID = incomingSessionInstanceID ?? existing?.sessionInstanceID
        let nextExternalSessionID = resolvedLiveExternalSessionID(
            existing: existing,
            incomingExternalSessionID: envelope.externalSessionID,
            tool: nextTool,
            didSwitchInvocationContext: didSwitchInvocationContext
        )
        let incomingResponseState: AIResponseState? = {
            if envelope.updatedAt < existingUpdatedAt,
               existing?.responseState == .responding,
               envelope.responseState != .responding {
                return existing?.responseState
            }
            if didSwitchInvocationContext {
                return envelope.responseState
            }
            if let interruptedAt = existing?.interruptedAt,
               envelope.responseState == .responding,
               envelope.updatedAt <= interruptedAt {
                return existing?.responseState ?? .idle
            }
            return envelope.responseState ?? existing?.responseState
        }()
        let nextInterruptedAt: Double? = {
            guard let interruptedAt = existing?.interruptedAt else {
                return nil
            }
            if envelope.updatedAt > interruptedAt,
               envelope.responseState == .responding {
                return nil
            }
            return interruptedAt
        }()

        if didSwitchInvocationContext {
            detachTerminal(sessionID)
            runtimeContextsByTerminalID[sessionID] = nil
            logger.log(
                "runtime-store",
                "invocation-switch session=\(sessionID.uuidString) tool=\(nextTool) invocation=\(existing?.invocationID ?? "nil")->\(nextInvocationID ?? "nil") instance=\(existing?.sessionInstanceID ?? "nil")->\(nextSessionInstanceID ?? "nil")"
            )
        }

        var next = TerminalBindingState(
            sessionID: sessionID,
            sessionInstanceID: nextSessionInstanceID,
            invocationID: nextInvocationID,
            projectID: projectID,
            projectName: envelope.projectName,
            sessionTitle: envelope.sessionTitle,
            tool: nextTool,
            status: envelope.status,
            responseState: incomingResponseState,
            updatedAt: max(envelope.updatedAt, existing?.updatedAt ?? 0),
            startedAt: envelope.startedAt ?? (didSwitchInvocationContext ? nil : existing?.startedAt),
            interruptedAt: didSwitchInvocationContext ? nil : nextInterruptedAt,
            hasCompletedTurn: envelope.responseState == .responding ? false : (didSwitchInvocationContext ? false : (existing?.hasCompletedTurn ?? false)),
            lastKnownExternalSessionID: nextExternalSessionID,
            lastKnownModel: didSwitchInvocationContext ? nil : existing?.lastKnownModel,
            logicalSessionKey: didSwitchInvocationContext ? nil : existing?.logicalSessionKey,
            pendingSessionOrigin: didSwitchInvocationContext
                ? defaultPendingSessionOrigin(
                    tool: nextTool,
                    externalSessionID: nextExternalSessionID,
                    pendingAttachment: pendingAttachment
                )
                : (nextExternalSessionID == nil ? (existing?.pendingSessionOrigin ?? .unknown) : .unknown)
        )

        if next.pendingSessionOrigin != .fresh || max(0, envelope.totalTokens ?? 0) > 0 {
            next.lastKnownModel = envelope.model ?? next.lastKnownModel
        }

        if envelope.responseState == .responding {
            next.hasCompletedTurn = false
        }

        if let externalSessionID = nextExternalSessionID {
            let logicalKey = LogicalSessionKey(
                tool: canonicalToolName(nextTool),
                externalSessionID: externalSessionID
            )
            let indexedHistoricalSession =
                pendingLogicalAttachmentsByTerminalID[sessionID].flatMap { attachment in
                    attachment.key == logicalKey ? attachment.indexedSummary : nil
                }
            let attachmentOrigin = resolvedAttachmentOrigin(
                sessionID: sessionID,
                logicalKey: logicalKey,
                reportedOrigin: .unknown,
                pendingOrigin: next.pendingSessionOrigin
            )
            let shouldReuseExisting = shouldReuseLogicalSession(for: logicalKey, terminalSessionID: sessionID)
            let logicalState = upsertLogicalSession(
                projectID: projectID,
                key: logicalKey,
                model: envelope.model,
                inputTokens: max(0, envelope.inputTokens ?? 0),
                outputTokens: max(0, envelope.outputTokens ?? 0),
                totalTokens: max(0, envelope.totalTokens ?? 0),
                contextWindow: envelope.contextWindow,
                contextUsedTokens: envelope.contextUsedTokens,
                contextUsagePercent: envelope.contextUsagePercent,
                updatedAt: envelope.updatedAt,
                shouldReuseExisting: shouldReuseExisting,
                allowsHistoricalBaseline: incomingResponseState != .responding,
                attachmentOrigin: attachmentOrigin,
                indexedHistoricalSessionOverride: indexedHistoricalSession
            )
            bindTerminal(sessionID, to: logicalKey)
            next.logicalSessionKey = logicalState.key
            next.lastKnownModel = logicalState.model ?? next.lastKnownModel
            if attachmentOrigin != .unknown {
                next.pendingSessionOrigin = .unknown
            }
            clearPendingAttachmentIfMatched(sessionID: sessionID, logicalKey: logicalKey)
            if envelope.responseState == .responding {
                lockBaselineRebase(for: logicalKey, reason: "live-responding")
            }
        } else {
            detachTerminal(sessionID)
            next.logicalSessionKey = nil
        }

        let didChange = terminalBindingsByID[sessionID] != next
        apply(next, for: sessionID)
        if didChange {
            logger.log(
                "runtime-store",
                "live session=\(sessionID.uuidString) tool=\(next.tool) status=\(next.status) model=\(next.lastKnownModel ?? "nil") response=\(next.responseState?.rawValue ?? "nil") external=\(next.lastKnownExternalSessionID ?? "nil") instance=\(next.sessionInstanceID ?? "nil") pendingOrigin=\(next.pendingSessionOrigin.rawValue)"
            )
        }
    }

    func applyResponsePayload(_ payload: AIResponseStatePayload) {
        guard let sessionID = UUID(uuidString: payload.sessionId),
              var existing = terminalBindingsByID[sessionID] else {
            return
        }
        if shouldIgnoreIncomingTool(existing: existing, incomingTool: payload.tool) {
            logger.log(
                "runtime-store",
                "ignore response session=\(sessionID.uuidString) existingTool=\(existing.tool) incomingTool=\(payload.tool) state=\(payload.responseState.rawValue)"
            )
            return
        }

        let nextTool = resolvedTool(currentTool: existing.tool, incomingTool: payload.tool)
        let isStaleResponseTransition =
            payload.updatedAt < existing.updatedAt
            && existing.responseState != nil
            && existing.responseState != payload.responseState
        if isStaleResponseTransition {
            logger.log(
                "runtime-store",
                "ignore stale response session=\(sessionID.uuidString) tool=\(nextTool) state=\(payload.responseState.rawValue) payloadAt=\(payload.updatedAt) existingAt=\(existing.updatedAt) existingState=\(existing.responseState?.rawValue ?? "nil")"
            )
            return
        }

        let nextResponseState: AIResponseState? = {
            if let interruptedAt = existing.interruptedAt,
               payload.responseState == .responding,
               payload.updatedAt <= interruptedAt {
                return existing.responseState ?? .idle
            }
            return payload.responseState
        }()
        let nextInterruptedAt: Double? = {
            guard let interruptedAt = existing.interruptedAt else {
                return nil
            }
            if payload.updatedAt > interruptedAt,
               payload.responseState == .responding {
                return nil
            }
            return interruptedAt
        }()

        let didChange = existing.tool != nextTool
            || existing.responseState != nextResponseState
            || existing.interruptedAt != nextInterruptedAt
            || existing.invocationID != normalizedInvocationID(payload.invocationId)
            || existing.sessionInstanceID != normalizedInvocationID(payload.sessionInstanceId)
        guard didChange else {
            return
        }

        existing.tool = nextTool
        existing.invocationID = normalizedInvocationID(payload.invocationId) ?? existing.invocationID
        existing.sessionInstanceID = normalizedInvocationID(payload.sessionInstanceId) ?? existing.sessionInstanceID
        existing.responseState = nextResponseState
        existing.updatedAt = max(existing.updatedAt, payload.updatedAt)
        existing.interruptedAt = nextInterruptedAt
        if payload.responseState == .responding {
            existing.hasCompletedTurn = false
            if let logicalKey = existing.logicalSessionKey {
                lockBaselineRebase(for: logicalKey, reason: "response-responding")
            }
        }
        apply(existing, for: sessionID)
        logger.log(
            "runtime-store",
            "response session=\(sessionID.uuidString) tool=\(existing.tool) state=\(existing.responseState?.rawValue ?? "nil") updatedAt=\(existing.updatedAt)"
        )
    }

    @discardableResult
    func applyRuntimeSnapshot(sessionID: UUID, snapshot: AIRuntimeContextSnapshot) -> RuntimeSnapshotApplyResult? {
        guard var existing = terminalBindingsByID[sessionID] else {
            return nil
        }
        if shouldIgnoreIncomingTool(existing: existing, incomingTool: snapshot.tool) {
            logger.log(
                "runtime-store",
                "ignore snapshot session=\(sessionID.uuidString) existingTool=\(existing.tool) incomingTool=\(snapshot.tool) response=\(snapshot.responseState?.rawValue ?? "nil") total=\(snapshot.totalTokens)"
            )
            return RuntimeSnapshotApplyResult(
                previousContext: runtimeContextsByTerminalID[sessionID],
                currentContext: snapshot,
                didChangeDisplay: false,
                didAdvance: false,
                didSwitchExternalSession: false,
                ignored: true
            )
        }

        let previousRuntime = runtimeContextsByTerminalID[sessionID]
        guard let mergedRuntime = mergedRuntimeSnapshot(
            previous: previousRuntime,
            incoming: snapshot,
            existingTool: existing.tool
        ) else {
            logger.log(
                "runtime-store",
                "ignore snapshot session=\(sessionID.uuidString) tool=\(canonicalToolName(snapshot.tool)) reason=external-mismatch previous=\(previousRuntime?.externalSessionID ?? "nil") incoming=\(snapshot.externalSessionID ?? "nil")"
            )
            return RuntimeSnapshotApplyResult(
                previousContext: previousRuntime,
                currentContext: snapshot,
                didChangeDisplay: false,
                didAdvance: false,
                didSwitchExternalSession: false,
                ignored: true
            )
        }

        runtimeContextsByTerminalID[sessionID] = mergedRuntime

        let prefersHookDrivenResponseState = toolDriverFactory.prefersHookDrivenResponseState(for: mergedRuntime.tool)
        let shouldPreserveHookRespondingState = prefersHookDrivenResponseState
            && existing.responseState == .responding
            && mergedRuntime.responseState == .idle
            && mergedRuntime.wasInterrupted == false
            && mergedRuntime.hasCompletedTurn == false

        let previousBinding = existing
        existing.tool = resolvedTool(currentTool: existing.tool, incomingTool: mergedRuntime.tool)
        existing.lastKnownExternalSessionID = normalizedExternalSessionID(mergedRuntime.externalSessionID) ?? existing.lastKnownExternalSessionID
        if existing.pendingSessionOrigin != .fresh || mergedRuntime.totalTokens > 0 {
            existing.lastKnownModel = mergedRuntime.model ?? existing.lastKnownModel
        }
        existing.updatedAt = max(existing.updatedAt, mergedRuntime.updatedAt)

        if mergedRuntime.wasInterrupted {
            existing.interruptedAt = max(existing.interruptedAt ?? 0, mergedRuntime.updatedAt)
        }
        if mergedRuntime.hasCompletedTurn {
            existing.hasCompletedTurn = true
        }

        let canApplySnapshotResponseState = prefersHookDrivenResponseState == false
            || existing.responseState == nil
            || mergedRuntime.responseState == .idle
        if canApplySnapshotResponseState {
            if shouldPreserveHookRespondingState {
                existing.responseState = .responding
            } else if let interruptedAt = existing.interruptedAt,
                      mergedRuntime.responseState == .responding,
                      mergedRuntime.updatedAt <= interruptedAt {
                existing.responseState = existing.responseState ?? .idle
            } else {
                existing.responseState = mergedRuntime.responseState ?? existing.responseState
                if let interruptedAt = existing.interruptedAt,
                   mergedRuntime.updatedAt > interruptedAt,
                   mergedRuntime.responseState == .responding {
                    existing.interruptedAt = nil
                }
            }
        }
        if mergedRuntime.responseState == .responding || mergedRuntime.wasInterrupted {
            existing.hasCompletedTurn = false
        }

        if let externalSessionID = normalizedExternalSessionID(mergedRuntime.externalSessionID) {
            let logicalKey = LogicalSessionKey(
                tool: canonicalToolName(existing.tool),
                externalSessionID: externalSessionID
            )
            let indexedHistoricalSession =
                pendingLogicalAttachmentsByTerminalID[sessionID].flatMap { attachment in
                    attachment.key == logicalKey ? attachment.indexedSummary : nil
                }
            let attachmentOrigin = resolvedAttachmentOrigin(
                sessionID: sessionID,
                logicalKey: logicalKey,
                reportedOrigin: mergedRuntime.sessionOrigin,
                pendingOrigin: existing.pendingSessionOrigin
            )
            let shouldReuseExisting = shouldReuseLogicalSession(for: logicalKey, terminalSessionID: sessionID)
            let logicalState = upsertLogicalSession(
                projectID: existing.projectID,
                key: logicalKey,
                model: mergedRuntime.model,
                inputTokens: max(0, mergedRuntime.inputTokens),
                outputTokens: max(0, mergedRuntime.outputTokens),
                totalTokens: max(0, mergedRuntime.totalTokens),
                contextWindow: nil,
                contextUsedTokens: nil,
                contextUsagePercent: nil,
                updatedAt: mergedRuntime.updatedAt,
                shouldReuseExisting: shouldReuseExisting,
                allowsHistoricalBaseline: existing.responseState != .responding,
                attachmentOrigin: attachmentOrigin,
                indexedHistoricalSessionOverride: indexedHistoricalSession
            )
            bindTerminal(sessionID, to: logicalKey)
            existing.logicalSessionKey = logicalState.key
            existing.lastKnownModel = logicalState.model ?? existing.lastKnownModel
            if attachmentOrigin != .unknown {
                existing.pendingSessionOrigin = .unknown
            }
            clearPendingAttachmentIfMatched(sessionID: sessionID, logicalKey: logicalKey)
            if mergedRuntime.responseState == .responding {
                lockBaselineRebase(for: logicalKey, reason: "snapshot-responding")
            }
        } else if existing.logicalSessionKey == nil {
            detachTerminal(sessionID)
        }

        let runtimeChanged = runtimeDisplayDidChange(previous: previousRuntime, current: mergedRuntime)
        let bindingChanged = previousBinding != existing
        let didChangeDisplay = runtimeChanged || bindingChanged
        let didAdvance = runtimeDidAdvance(previous: previousRuntime, current: mergedRuntime)
        let didSwitchExternalSession =
            toolDriverFactory.allowsRuntimeExternalSessionSwitch(for: mergedRuntime.tool)
            && externalSessionIDDidChange(
                previous: previousRuntime?.externalSessionID,
                incoming: mergedRuntime.externalSessionID
            )

        apply(existing, for: sessionID)
        if didChangeDisplay {
            let projected = self.snapshot(from: existing)
            logger.log(
                "runtime-store",
                "snapshot session=\(sessionID.uuidString) tool=\(existing.tool) model=\(projected.model ?? "nil") response=\(existing.responseState?.rawValue ?? "nil") total=\(projected.currentTotalTokens) external=\(projected.externalSessionID ?? "nil") origin=\(mergedRuntime.sessionOrigin.rawValue)"
            )
        }

        return RuntimeSnapshotApplyResult(
            previousContext: previousRuntime,
            currentContext: mergedRuntime,
            didChangeDisplay: didChangeDisplay,
            didAdvance: didAdvance,
            didSwitchExternalSession: didSwitchExternalSession,
            ignored: false
        )
    }

    func clearSession(_ sessionID: UUID) {
        let previousLogicalKey = terminalBindingsByID[sessionID]?.logicalSessionKey
        detachTerminal(sessionID)
        pendingLogicalAttachmentsByTerminalID[sessionID] = nil
        runtimeContextsByTerminalID[sessionID] = nil
        if terminalBindingsByID.removeValue(forKey: sessionID) != nil {
            renderVersion &+= 1
            logger.log(
                "runtime-store",
                "clear session=\(sessionID.uuidString) logical=\(previousLogicalKey.map { "\($0.tool):\($0.externalSessionID)" } ?? "nil")"
            )
        }
    }

    func reset() {
        guard !terminalBindingsByID.isEmpty
            || !logicalSessionsByKey.isEmpty
            || !runtimeContextsByTerminalID.isEmpty else {
            return
        }
        terminalBindingsByID.removeAll()
        logicalSessionsByKey.removeAll()
        terminalIDsByLogicalSessionKey.removeAll()
        pendingLogicalAttachmentsByTerminalID.removeAll()
        runtimeContextsByTerminalID.removeAll()
        renderVersion &+= 1
        logger.log("runtime-store", "reset all")
    }

    func prune(projectID: UUID, liveSessionIDs: Set<UUID>) {
        let stale = terminalBindingsByID.values
            .filter { $0.projectID == projectID && !liveSessionIDs.contains($0.sessionID) }
            .map(\.sessionID)
        guard !stale.isEmpty else {
            return
        }
        for sessionID in stale {
            detachTerminal(sessionID)
            terminalBindingsByID[sessionID] = nil
            pendingLogicalAttachmentsByTerminalID[sessionID] = nil
            runtimeContextsByTerminalID[sessionID] = nil
        }
        renderVersion &+= 1
        logger.log("runtime-store", "prune project=\(projectID.uuidString) removed=\(stale.count)")
    }

    func projectPhase(projectID: UUID) -> ProjectActivityPhase {
        let bindings = terminalBindingsByID.values
            .filter { $0.projectID == projectID && $0.status == "running" }
            .sorted { $0.updatedAt > $1.updatedAt }

        if let responding = bindings.first(where: { $0.responseState == .responding }) {
            return .running(tool: responding.tool)
        }
        return .idle
    }

    func liveSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        terminalBindingsByID.values
            .filter { $0.projectID == projectID && $0.status == "running" }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map(snapshot(from:))
    }

    func liveDisplaySnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        liveSnapshots(projectID: projectID).map(displaySnapshot(from:))
    }

    func liveAggregationSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        var snapshotsByLogicalKey: [LogicalSessionKey: AITerminalSessionSnapshot] = [:]
        var fallbackSnapshotsBySessionID: [UUID: AITerminalSessionSnapshot] = [:]

        for binding in terminalBindingsByID.values
            .filter({ $0.projectID == projectID && $0.status == "running" })
            .sorted(by: { $0.updatedAt > $1.updatedAt }) {
            let snapshot = snapshot(from: binding)
            if let logicalKey = binding.logicalSessionKey {
                if let existing = snapshotsByLogicalKey[logicalKey] {
                    if snapshot.updatedAt > existing.updatedAt {
                        snapshotsByLogicalKey[logicalKey] = snapshot
                    }
                } else {
                    snapshotsByLogicalKey[logicalKey] = snapshot
                }
            } else {
                fallbackSnapshotsBySessionID[binding.sessionID] = snapshot
            }
        }

        return (Array(snapshotsByLogicalKey.values) + Array(fallbackSnapshotsBySessionID.values))
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    func currentSnapshot(projectID: UUID, selectedSessionID: UUID?) -> AITerminalSessionSnapshot? {
        let snapshots = liveSnapshots(projectID: projectID)
        if let selectedSessionID,
           let selected = snapshots.first(where: { $0.sessionID == selectedSessionID }) {
            return selected
        }
        return snapshots.first
    }

    func currentDisplaySnapshot(projectID: UUID, selectedSessionID: UUID?) -> AITerminalSessionSnapshot? {
        let snapshots = liveDisplaySnapshots(projectID: projectID)
        if let selectedSessionID,
           let selected = snapshots.first(where: { $0.sessionID == selectedSessionID }) {
            return selected
        }
        return snapshots.first
    }

    func sessionTitle(for sessionID: UUID) -> String? {
        terminalBindingsByID[sessionID]?.sessionTitle
    }

    func responseState(for sessionID: UUID) -> AIResponseState? {
        terminalBindingsByID[sessionID]?.responseState
    }

    func tool(for sessionID: UUID) -> String? {
        terminalBindingsByID[sessionID]?.tool
    }

    func debugSummary(projectID: UUID) -> String {
        let bindings = terminalBindingsByID.values
            .filter { $0.projectID == projectID }
            .sorted { $0.updatedAt > $1.updatedAt }

        guard !bindings.isEmpty else {
            return "none"
        }

        return bindings.map { binding in
            let updatedAt = String(format: "%.3f", binding.updatedAt)
            let key = binding.logicalSessionKey.map { "\($0.tool):\($0.externalSessionID)" } ?? "pending"
            return "session=\(binding.sessionID.uuidString) tool=\(binding.tool) status=\(binding.status) response=\(binding.responseState?.rawValue ?? "nil") logical=\(key) pendingOrigin=\(binding.pendingSessionOrigin.rawValue) updatedAt=\(updatedAt)"
        }
        .joined(separator: " | ")
    }

    private func apply(_ state: TerminalBindingState, for sessionID: UUID) {
        if terminalBindingsByID[sessionID] != state {
            terminalBindingsByID[sessionID] = state
            renderVersion &+= 1
        }
    }

    private func snapshot(from binding: TerminalBindingState) -> AITerminalSessionSnapshot {
        let logical = binding.logicalSessionKey.flatMap { logicalSessionsByKey[$0] }
        let runtime = runtimeContextsByTerminalID[binding.sessionID]
        let updatedAt = max(binding.updatedAt, logical?.updatedAt ?? 0, runtime?.updatedAt ?? 0)
        let visibleModel: String? = {
            if logical?.suppressHistoricalModelUntilProgress == true {
                return nil
            }
            return logical?.model ?? binding.lastKnownModel
        }()
        let suppressesDisplayTokensUntilFirstTurn =
            toolDriverFactory.freezesDisplayTokensWhileResponding(for: binding.tool)
            && binding.hasCompletedTurn == false
        let freezesDisplayTokensWhileResponding =
            binding.responseState == .responding
            && toolDriverFactory.freezesDisplayTokensWhileResponding(for: binding.tool)
        let visibleInputTokens =
            (suppressesDisplayTokensUntilFirstTurn || freezesDisplayTokensWhileResponding)
            ? (logical?.committedInputTokens ?? logical?.baselineInputTokens ?? max(0, runtime?.inputTokens ?? 0))
            : (logical?.inputTokens ?? max(0, runtime?.inputTokens ?? 0))
        let visibleOutputTokens =
            (suppressesDisplayTokensUntilFirstTurn || freezesDisplayTokensWhileResponding)
            ? (logical?.committedOutputTokens ?? logical?.baselineOutputTokens ?? max(0, runtime?.outputTokens ?? 0))
            : (logical?.outputTokens ?? max(0, runtime?.outputTokens ?? 0))
        let visibleTotalTokens =
            (suppressesDisplayTokensUntilFirstTurn || freezesDisplayTokensWhileResponding)
            ? (logical?.committedTotalTokens ?? logical?.baselineTotalTokens ?? max(0, runtime?.totalTokens ?? 0))
            : (logical?.totalTokens ?? max(0, runtime?.totalTokens ?? 0))
        return AITerminalSessionSnapshot(
            sessionID: binding.sessionID,
            externalSessionID: logical?.key.externalSessionID ?? binding.lastKnownExternalSessionID,
            projectID: binding.projectID,
            projectName: binding.projectName,
            sessionTitle: binding.sessionTitle,
            tool: binding.tool,
            model: visibleModel,
            status: binding.status,
            responseState: binding.responseState,
            startedAt: binding.startedAt.map { Date(timeIntervalSince1970: $0) },
            updatedAt: Date(timeIntervalSince1970: updatedAt),
            currentInputTokens: visibleInputTokens,
            currentOutputTokens: visibleOutputTokens,
            currentTotalTokens: visibleTotalTokens,
            baselineInputTokens: logical?.baselineInputTokens ?? 0,
            baselineOutputTokens: logical?.baselineOutputTokens ?? 0,
            baselineTotalTokens: logical?.baselineTotalTokens ?? 0,
            currentContextWindow: logical?.contextWindow,
            currentContextUsedTokens: logical?.contextUsedTokens,
            currentContextUsagePercent: logical?.contextUsagePercent,
            wasInterrupted: binding.interruptedAt != nil && binding.responseState != .responding,
            hasCompletedTurn: binding.hasCompletedTurn && binding.responseState != .responding
        )
    }

    private func displaySnapshot(from snapshot: AITerminalSessionSnapshot) -> AITerminalSessionSnapshot {
        var next = snapshot
        next.currentInputTokens = max(0, snapshot.currentInputTokens - snapshot.baselineInputTokens)
        next.currentOutputTokens = max(0, snapshot.currentOutputTokens - snapshot.baselineOutputTokens)
        next.currentTotalTokens = max(0, snapshot.currentTotalTokens - snapshot.baselineTotalTokens)
        return next
    }

    private func shouldReuseLogicalSession(for key: LogicalSessionKey, terminalSessionID: UUID) -> Bool {
        guard logicalSessionsByKey[key] != nil else {
            return false
        }
        if terminalBindingsByID[terminalSessionID]?.logicalSessionKey == key {
            return true
        }
        return !(terminalIDsByLogicalSessionKey[key] ?? []).isEmpty
    }

    @discardableResult
    private func upsertLogicalSession(
        projectID: UUID,
        key: LogicalSessionKey,
        model: String?,
        inputTokens: Int,
        outputTokens: Int,
        totalTokens: Int,
        contextWindow: Int?,
        contextUsedTokens: Int?,
        contextUsagePercent: Double?,
        updatedAt: Double,
        shouldReuseExisting: Bool,
        allowsHistoricalBaseline: Bool,
        attachmentOrigin: AIRuntimeSessionOrigin,
        indexedHistoricalSessionOverride: AISessionSummary? = nil
    ) -> LogicalSessionState {
        let freezesDisplayTokensWhileResponding =
            toolDriverFactory.freezesDisplayTokensWhileResponding(for: key.tool)
        if shouldReuseExisting == false || logicalSessionsByKey[key] == nil {
            let indexedHistoricalSession = indexedHistoricalSessionOverride ?? usageStore.indexedSessionSummary(
                projectID: projectID,
                tool: key.tool,
                externalSessionID: key.externalSessionID
            )
            let indexedHistoricalTotal = indexedHistoricalSession?.totalTokens
            let hasIndexedHistoricalSession = (indexedHistoricalTotal ?? 0) > 0
            let shouldAdoptObservedHistoricalBaseline =
                attachmentOrigin == .unknown
                && hasIndexedHistoricalSession
                && totalTokens > (indexedHistoricalTotal ?? 0)
                && allowsHistoricalBaseline
            let shouldSeedObservedBaselineOnFirstProgress =
                totalTokens <= 0
                && (
                    attachmentOrigin == .restored
                    || (attachmentOrigin == .unknown && hasIndexedHistoricalSession)
                )
            let usesObservedBaseline =
                (attachmentOrigin == .restored && hasIndexedHistoricalSession == false)
                || shouldAdoptObservedHistoricalBaseline
            let pendingBaselineSeed =
                shouldSeedObservedBaselineOnFirstProgress
                || (
                    attachmentOrigin == .restored
                    && hasIndexedHistoricalSession == false
                    && totalTokens <= 0
                )
            let baselineInputTokens =
                usesObservedBaseline && totalTokens > 0 ? inputTokens
                : (indexedHistoricalSession?.totalInputTokens ?? 0)
            let baselineOutputTokens =
                usesObservedBaseline && totalTokens > 0 ? outputTokens
                : (indexedHistoricalSession?.totalOutputTokens ?? 0)
            let baselineTotalTokens =
                usesObservedBaseline && totalTokens > 0 ? totalTokens
                : (indexedHistoricalTotal ?? 0)
            let committedInputTokens =
                freezesDisplayTokensWhileResponding && allowsHistoricalBaseline == false
                ? baselineInputTokens
                : inputTokens
            let committedOutputTokens =
                freezesDisplayTokensWhileResponding && allowsHistoricalBaseline == false
                ? baselineOutputTokens
                : outputTokens
            let committedTotalTokens =
                freezesDisplayTokensWhileResponding && allowsHistoricalBaseline == false
                ? baselineTotalTokens
                : totalTokens
            let next = LogicalSessionState(
                key: key,
                model: resolvedInitialLogicalModel(
                    indexedHistoricalModel: indexedHistoricalSession?.lastModel,
                    observedModel: model,
                    attachmentOrigin: attachmentOrigin,
                    totalTokens: totalTokens
                ),
                inputTokens: inputTokens,
                outputTokens: outputTokens,
                totalTokens: totalTokens,
                committedInputTokens: committedInputTokens,
                committedOutputTokens: committedOutputTokens,
                committedTotalTokens: committedTotalTokens,
                baselineInputTokens: baselineInputTokens,
                baselineOutputTokens: baselineOutputTokens,
                baselineTotalTokens: baselineTotalTokens,
                contextWindow: contextWindow,
                contextUsedTokens: contextUsedTokens,
                contextUsagePercent: contextUsagePercent,
                updatedAt: updatedAt,
                pendingBaselineSeed: pendingBaselineSeed,
                allowsBaselineRebase: attachmentOrigin == .unknown
                    && hasIndexedHistoricalSession
                    && allowsHistoricalBaseline
                    && pendingBaselineSeed == false
                    && usesObservedBaseline == false,
                suppressHistoricalModelUntilProgress: attachmentOrigin == .fresh && totalTokens <= 0
            )
            logicalSessionsByKey[key] = next
            logger.log(
                "runtime-store",
                "logical create key=\(key.tool):\(key.externalSessionID) baseline=\(baselineTotalTokens) total=\(totalTokens) indexed=\(hasIndexedHistoricalSession) indexedTotal=\(indexedHistoricalTotal ?? 0) origin=\(attachmentOrigin.rawValue) observedBaseline=\(usesObservedBaseline) pending=\(next.pendingBaselineSeed) rebase=\(next.allowsBaselineRebase)"
            )
            return next
        }

        var next = logicalSessionsByKey[key]!
        if attachmentOrigin != .fresh && next.pendingBaselineSeed && totalTokens > 0 {
            next.baselineInputTokens = inputTokens
            next.baselineOutputTokens = outputTokens
            next.baselineTotalTokens = totalTokens
            next.pendingBaselineSeed = false
            next.allowsBaselineRebase = false
            logger.log(
                "runtime-store",
                "logical seed key=\(key.tool):\(key.externalSessionID) baseline=\(totalTokens) input=\(inputTokens) output=\(outputTokens) origin=\(attachmentOrigin.rawValue)"
            )
            if next.suppressHistoricalModelUntilProgress, model != nil {
                next.suppressHistoricalModelUntilProgress = false
                logger.log(
                    "runtime-store",
                    "logical model-unlock key=\(key.tool):\(key.externalSessionID) reason=seed"
                )
            }
        }
        if next.allowsBaselineRebase && allowsHistoricalBaseline && totalTokens > next.baselineTotalTokens {
            next.baselineInputTokens = inputTokens
            next.baselineOutputTokens = outputTokens
            next.baselineTotalTokens = totalTokens
            next.allowsBaselineRebase = false
            logger.log(
                "runtime-store",
                "logical rebase key=\(key.tool):\(key.externalSessionID) baseline=\(totalTokens) input=\(inputTokens) output=\(outputTokens)"
            )
        }
        if allowsHistoricalBaseline == false && next.allowsBaselineRebase {
            next.allowsBaselineRebase = false
            logger.log(
                "runtime-store",
                "logical rebase-lock key=\(key.tool):\(key.externalSessionID) reason=historical-window-closed"
            )
        }
        let hasProgressBeyondBaseline =
            totalTokens > next.baselineTotalTokens
            || inputTokens > next.baselineInputTokens
            || outputTokens > next.baselineOutputTokens
        if next.suppressHistoricalModelUntilProgress && hasProgressBeyondBaseline {
            next.suppressHistoricalModelUntilProgress = false
            logger.log(
                "runtime-store",
                "logical model-unlock key=\(key.tool):\(key.externalSessionID) total=\(totalTokens) baseline=\(next.baselineTotalTokens)"
            )
        }
        if next.suppressHistoricalModelUntilProgress == false {
            next.model = model ?? next.model
        }
        next.inputTokens = max(next.inputTokens, inputTokens)
        next.outputTokens = max(next.outputTokens, outputTokens)
        next.totalTokens = max(next.totalTokens, totalTokens)
        if freezesDisplayTokensWhileResponding == false || allowsHistoricalBaseline {
            let previousCommittedTotalTokens = next.committedTotalTokens
            next.committedInputTokens = next.inputTokens
            next.committedOutputTokens = next.outputTokens
            next.committedTotalTokens = next.totalTokens
            if next.committedTotalTokens != previousCommittedTotalTokens {
                logger.log(
                    "runtime-store",
                    "logical commit key=\(key.tool):\(key.externalSessionID) total=\(next.committedTotalTokens) responseWindow=\(allowsHistoricalBaseline ? "idle" : "responding")"
                )
            }
        }
        next.contextWindow = contextWindow ?? next.contextWindow
        next.contextUsedTokens = contextUsedTokens ?? next.contextUsedTokens
        next.contextUsagePercent = contextUsagePercent ?? next.contextUsagePercent
        next.updatedAt = max(next.updatedAt, updatedAt)
        logicalSessionsByKey[key] = next
        return next
    }

    private func bindTerminal(_ sessionID: UUID, to key: LogicalSessionKey) {
        if terminalBindingsByID[sessionID]?.logicalSessionKey == key {
            return
        }
        detachTerminal(sessionID)
        var terminals = terminalIDsByLogicalSessionKey[key] ?? []
        terminals.insert(sessionID)
        terminalIDsByLogicalSessionKey[key] = terminals
        logger.log(
            "runtime-store",
            "bind session=\(sessionID.uuidString) logical=\(key.tool):\(key.externalSessionID) terminals=\(terminals.count)"
        )
    }

    private func detachTerminal(_ sessionID: UUID) {
        guard let key = terminalBindingsByID[sessionID]?.logicalSessionKey else {
            return
        }
        var terminals = terminalIDsByLogicalSessionKey[key] ?? []
        terminals.remove(sessionID)
        if terminals.isEmpty {
            terminalIDsByLogicalSessionKey[key] = nil
            logicalSessionsByKey[key] = nil
            logger.log(
                "runtime-store",
                "logical remove key=\(key.tool):\(key.externalSessionID) reason=last-terminal-detached"
            )
        } else {
            terminalIDsByLogicalSessionKey[key] = terminals
            logger.log(
                "runtime-store",
                "unbind session=\(sessionID.uuidString) logical=\(key.tool):\(key.externalSessionID) terminals=\(terminals.count)"
            )
        }
        if var binding = terminalBindingsByID[sessionID] {
            binding.logicalSessionKey = nil
            terminalBindingsByID[sessionID] = binding
        }
    }

    private func clearPendingAttachmentIfMatched(sessionID: UUID, logicalKey: LogicalSessionKey) {
        guard let attachment = pendingLogicalAttachmentsByTerminalID[sessionID],
              attachment.key == logicalKey else {
            return
        }
        pendingLogicalAttachmentsByTerminalID[sessionID] = nil
        logger.log(
            "runtime-store",
            "attach-consume session=\(sessionID.uuidString) logical=\(logicalKey.tool):\(logicalKey.externalSessionID) origin=\(attachment.origin.rawValue)"
        )
    }

    private func lockBaselineRebase(for key: LogicalSessionKey, reason: String) {
        guard var logical = logicalSessionsByKey[key],
              logical.allowsBaselineRebase else {
            return
        }
        logical.allowsBaselineRebase = false
        logicalSessionsByKey[key] = logical
        logger.log(
            "runtime-store",
            "logical rebase-lock key=\(key.tool):\(key.externalSessionID) reason=\(reason)"
        )
    }

    private func resolvedAttachmentOrigin(
        sessionID: UUID,
        logicalKey: LogicalSessionKey,
        reportedOrigin: AIRuntimeSessionOrigin,
        pendingOrigin: AIRuntimeSessionOrigin
    ) -> AIRuntimeSessionOrigin {
        if reportedOrigin != .unknown {
            return reportedOrigin
        }
        if let pendingAttachment = pendingLogicalAttachmentsByTerminalID[sessionID],
           pendingAttachment.key == logicalKey {
            return pendingAttachment.origin
        }
        if pendingOrigin != .unknown {
            return pendingOrigin
        }
        return .unknown
    }

    private func defaultPendingSessionOrigin(
        tool: String,
        externalSessionID: String?,
        pendingAttachment: PendingLogicalAttachment?
    ) -> AIRuntimeSessionOrigin {
        if pendingAttachment != nil {
            return .unknown
        }
        if toolDriverFactory.seedsObservedBaselineOnFreshLaunch(for: tool),
           externalSessionID == nil {
            return .fresh
        }
        return .unknown
    }

    private func resolvedLiveExternalSessionID(
        existing: TerminalBindingState?,
        incomingExternalSessionID: String?,
        tool: String,
        didSwitchInvocationContext: Bool
    ) -> String? {
        let normalizedIncoming = normalizedExternalSessionID(incomingExternalSessionID)
        if didSwitchInvocationContext {
            return normalizedIncoming
        }
        if toolDriverFactory.allowsRuntimeExternalSessionSwitch(for: tool),
           let current = existing?.lastKnownExternalSessionID,
           let normalizedIncoming,
           current != normalizedIncoming {
            return current
        }
        return normalizedIncoming ?? existing?.lastKnownExternalSessionID
    }

    private func mergedRuntimeSnapshot(
        previous: AIRuntimeContextSnapshot?,
        incoming: AIRuntimeContextSnapshot,
        existingTool: String
    ) -> AIRuntimeContextSnapshot? {
        let resolvedToolName = resolvedTool(currentTool: existingTool, incomingTool: incoming.tool)
        if shouldIgnoreRuntimeSnapshot(previous: previous, incoming: incoming, tool: resolvedToolName) {
            return nil
        }

        var merged = incoming
        merged.tool = resolvedToolName
        let prefersHookDrivenResponseState = toolDriverFactory.prefersHookDrivenResponseState(for: resolvedToolName)
        if prefersHookDrivenResponseState,
           let existingResponseState = previous?.responseState {
            if existingResponseState == .responding,
               merged.responseState == .idle,
               merged.wasInterrupted == false,
               merged.hasCompletedTurn == false {
                merged.responseState = .responding
            } else if merged.responseState != .idle {
                merged.responseState = existingResponseState
            }
        }

        let externalSessionDidSwitch =
            toolDriverFactory.allowsRuntimeExternalSessionSwitch(for: resolvedToolName)
            && externalSessionIDDidChange(
                previous: previous?.externalSessionID,
                incoming: merged.externalSessionID
            )

        if let previous, !externalSessionDidSwitch {
            merged.externalSessionID = normalizedExternalSessionID(merged.externalSessionID) ?? normalizedExternalSessionID(previous.externalSessionID)
            merged.model = merged.model ?? previous.model
            merged.inputTokens = max(merged.inputTokens, previous.inputTokens)
            merged.outputTokens = max(merged.outputTokens, previous.outputTokens)
            merged.totalTokens = max(merged.totalTokens, previous.totalTokens)
            merged.updatedAt = max(merged.updatedAt, previous.updatedAt)
            if merged.sessionOrigin == .unknown {
                merged.sessionOrigin = previous.sessionOrigin
            }
        }

        return merged
    }

    private func shouldIgnoreRuntimeSnapshot(
        previous: AIRuntimeContextSnapshot?,
        incoming: AIRuntimeContextSnapshot,
        tool: String
    ) -> Bool {
        if toolDriverFactory.allowsRuntimeExternalSessionSwitch(for: tool) {
            return false
        }
        guard toolDriverFactory.prefersHookDrivenResponseState(for: tool),
              let previous,
              let previousExternalSessionID = normalizedExternalSessionID(previous.externalSessionID),
              let incomingExternalSessionID = normalizedExternalSessionID(incoming.externalSessionID) else {
            return false
        }
        return previousExternalSessionID != incomingExternalSessionID
    }

    private func runtimeDidAdvance(previous: AIRuntimeContextSnapshot?, current: AIRuntimeContextSnapshot) -> Bool {
        guard let previous else {
            return current.totalTokens > 0
                || current.outputTokens > 0
                || current.responseState != nil
        }
        if current.totalTokens > previous.totalTokens {
            return true
        }
        if current.outputTokens > previous.outputTokens {
            return true
        }
        if current.responseState != previous.responseState {
            return true
        }
        return false
    }

    private func runtimeDisplayDidChange(previous: AIRuntimeContextSnapshot?, current: AIRuntimeContextSnapshot) -> Bool {
        guard let previous else {
            return true
        }
        return previous.tool != current.tool
            || previous.externalSessionID != current.externalSessionID
            || previous.model != current.model
            || previous.inputTokens != current.inputTokens
            || previous.outputTokens != current.outputTokens
            || previous.totalTokens != current.totalTokens
            || previous.responseState != current.responseState
            || previous.sessionOrigin != current.sessionOrigin
    }

    private func resolvedInitialLogicalModel(
        indexedHistoricalModel: String?,
        observedModel: String?,
        attachmentOrigin: AIRuntimeSessionOrigin,
        totalTokens: Int
    ) -> String? {
        switch attachmentOrigin {
        case .fresh:
            if totalTokens > 0 {
                return observedModel
            }
            return nil
        case .restored:
            return observedModel
        case .unknown:
            return observedModel ?? indexedHistoricalModel
        }
    }

    private func shouldIgnoreIncomingTool(existing: TerminalBindingState, incomingTool: String) -> Bool {
        guard existing.status == "running" else {
            return false
        }
        let currentTool = canonicalToolName(existing.tool)
        let nextTool = canonicalToolName(incomingTool)
        guard !currentTool.isEmpty, !nextTool.isEmpty else {
            return false
        }
        return currentTool != nextTool
    }

    private func resolvedTool(currentTool: String, incomingTool: String) -> String {
        guard !incomingTool.isEmpty else {
            return currentTool
        }
        let currentCanonical = canonicalToolName(currentTool)
        let incomingCanonical = canonicalToolName(incomingTool)
        if currentCanonical.isEmpty || incomingCanonical.isEmpty || currentCanonical == incomingCanonical {
            return incomingTool
        }
        return currentTool
    }

    private func canonicalToolName(_ tool: String) -> String {
        toolDriverFactory.canonicalToolName(tool)
    }

    private func normalizedExternalSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func normalizedInvocationID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func externalSessionIDDidChange(previous: String?, incoming: String?) -> Bool {
        guard let previous = normalizedExternalSessionID(previous),
              let incoming = normalizedExternalSessionID(incoming) else {
            return false
        }
        return previous != incoming
    }

    private func invocationContextDidChange(previous: String?, incoming: String?) -> Bool {
        guard let incoming else {
            return false
        }
        return normalizedInvocationID(previous) != incoming
    }
}
