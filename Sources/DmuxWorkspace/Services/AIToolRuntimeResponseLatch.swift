import Foundation

actor AIToolRuntimeResponseLatch {
    static let shared = AIToolRuntimeResponseLatch()

    private struct RuntimeKey: Hashable {
        var tool: String
        var runtimeSessionID: String
    }

    private struct ExternalKey: Hashable {
        var tool: String
        var externalSessionID: String
    }

    private struct LatchState {
        var lastRespondingAt: Double
        var externalSessionID: String?
        var pendingPromptCount: Int
    }

    private var stateByRuntimeSessionID: [RuntimeKey: LatchState] = [:]
    private var runtimeSessionIDsByExternalSessionID: [ExternalKey: Set<String>] = [:]

    func reset(tool: String, runtimeSessionID: String) {
        let runtimeKey = RuntimeKey(tool: tool, runtimeSessionID: runtimeSessionID)
        guard let existing = stateByRuntimeSessionID.removeValue(forKey: runtimeKey) else {
            return
        }
        guard let externalSessionID = existing.externalSessionID else {
            return
        }
        let externalKey = ExternalKey(tool: tool, externalSessionID: externalSessionID)
        runtimeSessionIDsByExternalSessionID[externalKey]?.remove(runtimeSessionID)
        if runtimeSessionIDsByExternalSessionID[externalKey]?.isEmpty == true {
            runtimeSessionIDsByExternalSessionID[externalKey] = nil
        }
    }

    func resetAll() {
        stateByRuntimeSessionID.removeAll()
        runtimeSessionIDsByExternalSessionID.removeAll()
    }

    func markResponding(tool: String, runtimeSessionID: String, externalSessionID: String?, updatedAt: Double) {
        let runtimeKey = RuntimeKey(tool: tool, runtimeSessionID: runtimeSessionID)
        let normalizedExternalSessionID = normalizedRuntimeSessionID(externalSessionID)
        let previousExternalSessionID = stateByRuntimeSessionID[runtimeKey]?.externalSessionID
        if let previousExternalSessionID, previousExternalSessionID != normalizedExternalSessionID {
            let previousExternalKey = ExternalKey(tool: tool, externalSessionID: previousExternalSessionID)
            runtimeSessionIDsByExternalSessionID[previousExternalKey]?.remove(runtimeSessionID)
            if runtimeSessionIDsByExternalSessionID[previousExternalKey]?.isEmpty == true {
                runtimeSessionIDsByExternalSessionID[previousExternalKey] = nil
            }
        }

        stateByRuntimeSessionID[runtimeKey] = LatchState(
            lastRespondingAt: updatedAt,
            externalSessionID: normalizedExternalSessionID,
            pendingPromptCount: max(1, (stateByRuntimeSessionID[runtimeKey]?.pendingPromptCount ?? 0) + 1)
        )
        if let normalizedExternalSessionID {
            let externalKey = ExternalKey(tool: tool, externalSessionID: normalizedExternalSessionID)
            runtimeSessionIDsByExternalSessionID[externalKey, default: []].insert(runtimeSessionID)
        }
    }

    func releaseIfDefinitiveStop(
        tool: String,
        runtimeSessionID: String,
        externalSessionID: String?,
        stopUpdatedAt: Double,
        wasInterrupted: Bool,
        hasCompletedTurn: Bool
    ) -> Bool {
        guard wasInterrupted || hasCompletedTurn else {
            return false
        }
        if hasNewerResponding(
            tool: tool,
            runtimeSessionID: runtimeSessionID,
            externalSessionID: externalSessionID,
            referenceUpdatedAt: stopUpdatedAt
        ) {
            return false
        }
        return consumePendingTurn(
            tool: tool,
            runtimeSessionID: runtimeSessionID,
            externalSessionID: externalSessionID,
            allowMissingPendingState: true
        ) ?? false
    }

    func releaseSettledIfPending(tool: String, runtimeSessionID: String, externalSessionID: String?) -> Bool? {
        consumePendingTurn(
            tool: tool,
            runtimeSessionID: runtimeSessionID,
            externalSessionID: externalSessionID,
            allowMissingPendingState: false
        )
    }

    func shouldIgnoreDefinitiveStop(
        tool: String,
        runtimeSessionID: String,
        externalSessionID: String?,
        stopUpdatedAt: Double
    ) -> Bool {
        hasNewerResponding(
            tool: tool,
            runtimeSessionID: runtimeSessionID,
            externalSessionID: externalSessionID,
            referenceUpdatedAt: stopUpdatedAt
        )
    }

    func shouldForceResponding(
        tool: String,
        runtimeSessionID: String,
        externalSessionID: String?,
        snapshotUpdatedAt: Double,
        wasInterrupted: Bool
    ) -> Bool {
        _ = snapshotUpdatedAt
        guard wasInterrupted == false else {
            return false
        }

        let runtimeKey = RuntimeKey(tool: tool, runtimeSessionID: runtimeSessionID)
        if let existing = stateByRuntimeSessionID[runtimeKey] {
            return existing.pendingPromptCount > 0
        }

        if let normalizedExternalSessionID = normalizedRuntimeSessionID(externalSessionID) {
            let externalKey = ExternalKey(tool: tool, externalSessionID: normalizedExternalSessionID)
            if let runtimeSessionIDs = runtimeSessionIDsByExternalSessionID[externalKey] {
                for id in runtimeSessionIDs {
                    let candidateKey = RuntimeKey(tool: tool, runtimeSessionID: id)
                    if let existing = stateByRuntimeSessionID[candidateKey],
                       existing.pendingPromptCount > 0 {
                        stateByRuntimeSessionID[runtimeKey] = LatchState(
                            lastRespondingAt: existing.lastRespondingAt,
                            externalSessionID: normalizedExternalSessionID,
                            pendingPromptCount: existing.pendingPromptCount
                        )
                        runtimeSessionIDsByExternalSessionID[externalKey, default: []].insert(runtimeSessionID)
                        return true
                    }
                }
            }
        }

        return false
    }

    private func hasNewerResponding(
        tool: String,
        runtimeSessionID: String,
        externalSessionID: String?,
        referenceUpdatedAt: Double
    ) -> Bool {
        let runtimeKey = RuntimeKey(tool: tool, runtimeSessionID: runtimeSessionID)
        if let existing = stateByRuntimeSessionID[runtimeKey],
           existing.pendingPromptCount > 0,
           existing.lastRespondingAt > referenceUpdatedAt {
            return true
        }

        guard let normalizedExternalSessionID = normalizedRuntimeSessionID(externalSessionID) else {
            return false
        }
        let externalKey = ExternalKey(tool: tool, externalSessionID: normalizedExternalSessionID)
        guard let runtimeSessionIDs = runtimeSessionIDsByExternalSessionID[externalKey] else {
            return false
        }

        for id in runtimeSessionIDs {
            let candidateKey = RuntimeKey(tool: tool, runtimeSessionID: id)
            guard let existing = stateByRuntimeSessionID[candidateKey],
                  existing.pendingPromptCount > 0,
                  existing.lastRespondingAt > referenceUpdatedAt else {
                continue
            }
            return true
        }

        return false
    }

    private func consumePendingTurn(
        tool: String,
        runtimeSessionID: String,
        externalSessionID: String?,
        allowMissingPendingState: Bool
    ) -> Bool? {
        let runtimeKey = RuntimeKey(tool: tool, runtimeSessionID: runtimeSessionID)
        if var existing = stateByRuntimeSessionID[runtimeKey] {
            existing.pendingPromptCount = max(0, existing.pendingPromptCount - 1)
            if existing.pendingPromptCount > 0 {
                stateByRuntimeSessionID[runtimeKey] = existing
                return false
            }
            reset(tool: tool, runtimeSessionID: runtimeSessionID)
            return true
        }

        if let normalizedExternalSessionID = normalizedRuntimeSessionID(externalSessionID) {
            let externalKey = ExternalKey(tool: tool, externalSessionID: normalizedExternalSessionID)
            if let runtimeSessionIDs = runtimeSessionIDsByExternalSessionID[externalKey] {
                for id in runtimeSessionIDs {
                    let candidateKey = RuntimeKey(tool: tool, runtimeSessionID: id)
                    guard var existing = stateByRuntimeSessionID[candidateKey],
                          existing.pendingPromptCount > 0 else {
                        continue
                    }
                    existing.pendingPromptCount = max(0, existing.pendingPromptCount - 1)
                    if existing.pendingPromptCount > 0 {
                        stateByRuntimeSessionID[candidateKey] = existing
                        return false
                    }
                    reset(tool: tool, runtimeSessionID: id)
                    return true
                }
            }
        }

        return allowMissingPendingState ? true : nil
    }
}

private func normalizedRuntimeSessionID(_ value: String?) -> String? {
    guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
          !value.isEmpty else {
        return nil
    }
    return value
}
