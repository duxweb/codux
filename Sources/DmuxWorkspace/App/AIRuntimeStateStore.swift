import Foundation
import Observation

@MainActor
@Observable
final class AIRuntimeStateStore {
    static let shared = AIRuntimeStateStore()
    private let logger = AppDebugLog.shared

    struct SessionState: Equatable {
        var sessionID: UUID
        var sessionInstanceID: String?
        var projectID: UUID
        var projectName: String
        var sessionTitle: String
        var tool: String
        var externalSessionID: String?
        var model: String?
        var status: String
        var responseState: AIResponseState?
        var updatedAt: Double
        var startedAt: Double?
        var inputTokens: Int
        var outputTokens: Int
        var totalTokens: Int
        var contextWindow: Int?
        var contextUsedTokens: Int?
        var contextUsagePercent: Double?
    }

    var renderVersion: UInt64 = 0

    private(set) var sessionsByID: [UUID: SessionState] = [:]

    func applyLiveEnvelope(_ envelope: AIToolUsageEnvelope) {
        guard let sessionID = UUID(uuidString: envelope.sessionId),
              let projectID = UUID(uuidString: envelope.projectId) else {
            return
        }

        let existing = sessionsByID[sessionID]
        let existingUpdatedAt = existing?.updatedAt ?? 0
        let isNewInstance = {
            guard let incoming = envelope.sessionInstanceId, !incoming.isEmpty else {
                return false
            }
            return existing?.sessionInstanceID != incoming
        }()
        let incomingResponseState: AIResponseState? = {
            if envelope.tool == "codex" {
                // Codex response activity is hook-driven. Ignore the shell live file's
                // static idle state so it does not overwrite an in-memory responding state.
                return existing?.responseState
            }
            if envelope.updatedAt < existingUpdatedAt,
               existing?.responseState == .responding,
               envelope.responseState != .responding {
                return existing?.responseState
            }
            return envelope.responseState ?? (isNewInstance ? nil : existing?.responseState)
        }()
        let next = SessionState(
            sessionID: sessionID,
            sessionInstanceID: envelope.sessionInstanceId ?? existing?.sessionInstanceID,
            projectID: projectID,
            projectName: envelope.projectName,
            sessionTitle: envelope.sessionTitle,
            tool: envelope.tool.isEmpty ? ((isNewInstance ? nil : existing?.tool) ?? "") : envelope.tool,
            externalSessionID: envelope.externalSessionID ?? (isNewInstance ? nil : existing?.externalSessionID),
            model: envelope.model ?? (isNewInstance ? nil : existing?.model),
            status: envelope.status,
            responseState: incomingResponseState,
            updatedAt: max(envelope.updatedAt, existing?.updatedAt ?? 0),
            startedAt: envelope.startedAt ?? existing?.startedAt,
            inputTokens: isNewInstance ? max(0, envelope.inputTokens ?? 0) : max(envelope.inputTokens ?? 0, existing?.inputTokens ?? 0),
            outputTokens: isNewInstance ? max(0, envelope.outputTokens ?? 0) : max(envelope.outputTokens ?? 0, existing?.outputTokens ?? 0),
            totalTokens: isNewInstance ? max(0, envelope.totalTokens ?? 0) : max(envelope.totalTokens ?? 0, existing?.totalTokens ?? 0),
            contextWindow: isNewInstance ? envelope.contextWindow : (envelope.contextWindow ?? existing?.contextWindow),
            contextUsedTokens: isNewInstance ? envelope.contextUsedTokens : (envelope.contextUsedTokens ?? existing?.contextUsedTokens),
            contextUsagePercent: isNewInstance ? envelope.contextUsagePercent : (envelope.contextUsagePercent ?? existing?.contextUsagePercent)
        )
        let didChange = sessionsByID[sessionID] != next
        apply(next, for: sessionID)
        if didChange {
            logger.log(
                "runtime-store",
                "live session=\(sessionID.uuidString) tool=\(next.tool) status=\(next.status) model=\(next.model ?? "nil") response=\(next.responseState?.rawValue ?? "nil") total=\(next.totalTokens) external=\(next.externalSessionID ?? "nil") instance=\(next.sessionInstanceID ?? "nil")"
            )
        }
    }

    func applyResponsePayload(_ payload: AIResponseStatePayload) {
        guard let sessionID = UUID(uuidString: payload.sessionId) else {
            return
        }
        guard var existing = sessionsByID[sessionID] else {
            return
        }
        let didChange = existing.tool != payload.tool || existing.responseState != payload.responseState
        guard didChange else {
            return
        }
        existing.tool = payload.tool
        existing.responseState = payload.responseState
        existing.updatedAt = max(existing.updatedAt, payload.updatedAt)
        apply(existing, for: sessionID)
        logger.log(
            "runtime-store",
            "response session=\(sessionID.uuidString) tool=\(existing.tool) state=\(existing.responseState?.rawValue ?? "nil") updatedAt=\(existing.updatedAt)"
        )
    }

    func applyRuntimeSnapshot(sessionID: UUID, snapshot: AIRuntimeContextSnapshot) {
        guard var existing = sessionsByID[sessionID] else {
            return
        }
        existing.tool = snapshot.tool
        existing.externalSessionID = snapshot.externalSessionID ?? existing.externalSessionID
        existing.model = snapshot.model ?? existing.model
        existing.inputTokens = snapshot.inputTokens
        existing.outputTokens = snapshot.outputTokens
        existing.totalTokens = snapshot.totalTokens
        existing.updatedAt = max(existing.updatedAt, snapshot.updatedAt)
        existing.responseState = snapshot.responseState ?? existing.responseState
        let didChange = sessionsByID[sessionID] != existing
        apply(existing, for: sessionID)
        if didChange {
            logger.log(
                "runtime-store",
                "snapshot session=\(sessionID.uuidString) tool=\(existing.tool) model=\(existing.model ?? "nil") response=\(existing.responseState?.rawValue ?? "nil") total=\(existing.totalTokens) external=\(existing.externalSessionID ?? "nil")"
            )
        }
    }

    func clearSession(_ sessionID: UUID) {
        if sessionsByID.removeValue(forKey: sessionID) != nil {
            renderVersion &+= 1
            logger.log("runtime-store", "clear session=\(sessionID.uuidString)")
        }
    }

    func reset() {
        guard !sessionsByID.isEmpty else {
            return
        }
        sessionsByID.removeAll()
        renderVersion &+= 1
        logger.log("runtime-store", "reset all")
    }

    func prune(projectID: UUID, liveSessionIDs: Set<UUID>) {
        let stale = sessionsByID.values
            .filter { $0.projectID == projectID && !liveSessionIDs.contains($0.sessionID) }
            .map(\.sessionID)
        guard !stale.isEmpty else {
            return
        }
        for sessionID in stale {
            sessionsByID[sessionID] = nil
        }
        renderVersion &+= 1
        logger.log("runtime-store", "prune project=\(projectID.uuidString) removed=\(stale.count)")
    }

    func projectPhase(projectID: UUID) -> ProjectActivityPhase {
        let sessions = sessionsByID.values
            .filter { $0.projectID == projectID && $0.status == "running" }
            .sorted { $0.updatedAt > $1.updatedAt }

        if let responding = sessions.first(where: { $0.responseState == .responding }) {
            return .running(tool: responding.tool)
        }
        return .idle
    }

    func liveSnapshots(projectID: UUID) -> [AITerminalSessionSnapshot] {
        sessionsByID.values
            .filter { $0.projectID == projectID && $0.status == "running" }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map(snapshot(from:))
    }

    func currentSnapshot(projectID: UUID, selectedSessionID: UUID?) -> AITerminalSessionSnapshot? {
        let snapshots = liveSnapshots(projectID: projectID)
        if let selectedSessionID,
           let selected = snapshots.first(where: { $0.sessionID == selectedSessionID }) {
            return selected
        }
        return snapshots.first
    }

    func sessionTitle(for sessionID: UUID) -> String? {
        sessionsByID[sessionID]?.sessionTitle
    }

    private func apply(_ state: SessionState, for sessionID: UUID) {
        if sessionsByID[sessionID] != state {
            sessionsByID[sessionID] = state
            renderVersion &+= 1
        }
    }

    private func snapshot(from state: SessionState) -> AITerminalSessionSnapshot {
        AITerminalSessionSnapshot(
            sessionID: state.sessionID,
            externalSessionID: state.externalSessionID,
            projectID: state.projectID,
            projectName: state.projectName,
            sessionTitle: state.sessionTitle,
            tool: state.tool,
            model: state.model,
            status: state.status,
            responseState: state.responseState,
            startedAt: state.startedAt.map { Date(timeIntervalSince1970: $0) },
            updatedAt: Date(timeIntervalSince1970: state.updatedAt),
            currentInputTokens: state.inputTokens,
            currentOutputTokens: state.outputTokens,
            currentTotalTokens: state.totalTokens,
            currentContextWindow: state.contextWindow,
            currentContextUsedTokens: state.contextUsedTokens,
            currentContextUsagePercent: state.contextUsagePercent
        )
    }
}
