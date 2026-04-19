import Foundation
import SQLite3

struct CodexToolDriver: AIToolDriver {
    let id = "codex"
    let aliases: Set<String> = ["codex"]
    let runtimeRefreshInterval: TimeInterval = 0.55
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let allowsRuntimeExternalSessionSwitch = true
    let appliesGenericResponsePayloads = false

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        []
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "codex-hook",
              let envelope = try? JSONDecoder().decode(CodexHookRuntimeEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.dmuxSessionId),
              let projectID = UUID(uuidString: envelope.dmuxProjectId),
              let payloadText = envelope.payload.data(using: .utf8),
              let payloadObject = try? JSONSerialization.jsonObject(with: payloadText) as? [String: Any] else {
            return nil
        }

        let dedupeKey = "codex|\(envelope.event)|\(sessionID.uuidString)|\(envelope.receivedAt)|\(payloadHash(envelope.payload))"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.2) else {
            AppDebugLog.shared.log(
                "codex-hook",
                "drop duplicate event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "codex-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "codex-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let externalSessionID = stringValue(in: payloadObject, key: "session_id")
            ?? existingSnapshot?.externalSessionID
            ?? liveEnvelope?.externalSessionID
        let model = stringValue(in: payloadObject, key: "model")
            ?? existingSnapshot?.model
            ?? liveEnvelope?.model
        let canReuseExistingTotals = shouldReuseExistingTotals(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot
        )
        let inheritedInputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
            : max(0, liveEnvelope?.inputTokens ?? 0)
        let inheritedOutputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
            : max(0, liveEnvelope?.outputTokens ?? 0)
        let inheritedTotalTokens = canReuseExistingTotals
            ? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)
            : max(0, liveEnvelope?.totalTokens ?? 0)
        let updatedAt = max(
            envelope.receivedAt,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        if let existingSnapshot,
           existingSnapshot.externalSessionID == externalSessionID,
           existingSnapshot.responseState == .responding,
           envelope.event == "UserPromptSubmit",
           updatedAt <= existingSnapshot.updatedAt {
            AppDebugLog.shared.log(
                "codex-hook",
                "drop stale event=\(envelope.event) session=\(sessionID.uuidString) updatedAt=\(updatedAt) existingAt=\(existingSnapshot.updatedAt)"
            )
            return nil
        }

        let runtimeSnapshot: AIRuntimeContextSnapshot
        let responsePayload: AIResponseStatePayload
        switch envelope.event {
        case "UserPromptSubmit":
            await AIToolRuntimeResponseLatch.shared.markResponding(
                tool: id,
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID,
                updatedAt: updatedAt
            )
            runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: model,
                inputTokens: inheritedInputTokens,
                outputTokens: inheritedOutputTokens,
                totalTokens: inheritedTotalTokens,
                updatedAt: updatedAt,
                responseState: .responding,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
            responsePayload = AIResponseStatePayload(
                sessionId: sessionID.uuidString,
                sessionInstanceId: nil,
                invocationId: nil,
                projectId: projectID.uuidString,
                projectPath: nil,
                tool: id,
                responseState: .responding,
                updatedAt: updatedAt,
                source: .hook
            )
        case "Stop":
            let transcriptPath = stringValue(in: payloadObject, key: "transcript_path")
            let parsedState = await resolveCodexStopRuntimeState(transcriptPath: transcriptPath)
            let currentResponseState = existingSnapshot?.responseState ?? liveEnvelope?.responseState
            let stopLooksSettled = (parsedState?.responseState == .idle)
                && (parsedState?.wasInterrupted != true)
            let explicitDefinitiveStop = (parsedState?.wasInterrupted == true)
                || (parsedState?.hasCompletedTurn == true)
            let hasCandidateDefinitiveStop = explicitDefinitiveStop || stopLooksSettled
            let stopSemanticUpdatedAt = resolveCodexDefinitiveStopReferenceUpdatedAt(
                parsedState: parsedState,
                fallbackUpdatedAt: envelope.receivedAt
            )
            let shouldIgnoreDefinitiveStop: Bool
            if hasCandidateDefinitiveStop {
                shouldIgnoreDefinitiveStop = await AIToolRuntimeResponseLatch.shared.shouldIgnoreDefinitiveStop(
                    tool: id,
                    runtimeSessionID: sessionID.uuidString,
                    externalSessionID: externalSessionID,
                    stopUpdatedAt: stopSemanticUpdatedAt
                )
            } else {
                shouldIgnoreDefinitiveStop = false
            }
            let resolution = resolveCodexHookStopResolution(
                parsedState: parsedState,
                currentResponseState: currentResponseState,
                shouldIgnoreDefinitiveStop: shouldIgnoreDefinitiveStop
            )
            let didReleaseDefinitiveStop = await AIToolRuntimeResponseLatch.shared.releaseIfDefinitiveStop(
                tool: id,
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID,
                stopUpdatedAt: stopSemanticUpdatedAt,
                wasInterrupted: resolution.effectiveWasInterrupted,
                hasCompletedTurn: resolution.effectiveHasCompletedTurn
            )
            if shouldIgnoreDefinitiveStop {
                AppDebugLog.shared.log(
                    "codex-hook",
                    "defer stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=stale-definitive-stop stopAt=\(stopSemanticUpdatedAt)"
                )
            } else if resolution.shouldEmitIdle && didReleaseDefinitiveStop == false {
                AppDebugLog.shared.log(
                    "codex-hook",
                    "defer stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=newer-responding-arrived-before-release stopAt=\(stopSemanticUpdatedAt)"
                )
                return AIToolRuntimeIngressUpdate(
                    runtimeSnapshotsBySessionID: [
                        sessionID: AIRuntimeContextSnapshot(
                            tool: id,
                            externalSessionID: externalSessionID,
                            model: parsedState?.model ?? model,
                            inputTokens: parsedState?.totalTokens ?? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                            outputTokens: 0,
                            totalTokens: parsedState?.totalTokens ?? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                            updatedAt: max(updatedAt, parsedState?.updatedAt ?? 0),
                            responseState: nil,
                            wasInterrupted: false,
                            hasCompletedTurn: false,
                            source: .hook
                        ),
                    ]
                )
            }
            AppDebugLog.shared.log(
                "codex-hook",
                "stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") transcript=\(transcriptPath ?? "nil") parsedModel=\(parsedState?.model ?? model ?? "nil") parsedTokens=\(parsedState?.totalTokens.map(String.init) ?? "nil") interrupted=\(parsedState?.wasInterrupted == true) completed=\(parsedState?.hasCompletedTurn == true)"
            )
            runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: parsedState?.model ?? model,
                inputTokens: parsedState?.totalTokens ?? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                outputTokens: 0,
                totalTokens: parsedState?.totalTokens ?? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                updatedAt: max(updatedAt, parsedState?.updatedAt ?? 0),
                responseState: resolution.shouldEmitIdle ? .idle : nil,
                wasInterrupted: resolution.effectiveWasInterrupted,
                hasCompletedTurn: resolution.effectiveHasCompletedTurn,
                source: .hook
            )
            if resolution.shouldEmitIdle {
                responsePayload = AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: projectID.uuidString,
                    projectPath: nil,
                    tool: id,
                    responseState: .idle,
                    updatedAt: runtimeSnapshot.updatedAt,
                    source: .hook
                )
            } else {
                AppDebugLog.shared.log(
                    "codex-hook",
                    "defer stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=\(shouldIgnoreDefinitiveStop ? "stale-definitive-stop" : (stopLooksSettled && currentResponseState == .responding ? "settled-during-responding" : "non-definitive"))"
                )
                return AIToolRuntimeIngressUpdate(
                    runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
                )
            }
        default:
            AppDebugLog.shared.log("codex-hook", "ignore event=\(envelope.event) session=\(sessionID.uuidString)")
            return nil
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: [responsePayload],
            runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
        )
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: true, canRemove: true)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        let sessionID = session.externalSessionID ?? session.sessionID.uuidString
        return "codex resume \(shellQuoted(sessionID))"
    }

    func renameSession(_ session: AISessionSummary, to title: String) throws {
        let sessionID = session.externalSessionID ?? session.sessionID.uuidString
        let databaseURL = AIRuntimeSourceLocator.codexDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            let sql = "UPDATE threads SET title = ? WHERE id = ?;"
            try executeSQLite(
                db: db,
                sql: sql,
                bindings: [
                    .text(title),
                    .text(sessionID),
                ]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    func removeSession(_ session: AISessionSummary) throws {
        let sessionID = session.externalSessionID ?? session.sessionID.uuidString
        let now = Int64(Date().timeIntervalSince1970)
        let databaseURL = AIRuntimeSourceLocator.codexDatabaseURL()
        try withSQLiteDatabase(path: databaseURL.path) { db in
            let sql = "UPDATE threads SET archived = 1, archived_at = ?, updated_at = ? WHERE id = ?;"
            try executeSQLite(
                db: db,
                sql: sql,
                bindings: [
                    .int64(now),
                    .int64(now),
                    .text(sessionID),
                ]
            )
            guard sqlite3_changes(db) > 0 else {
                throw AIToolSessionControlError.sessionNotFound
            }
        }
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func stringValue(in object: [String: Any]?, key: String) -> String? {
        guard let object else {
            return nil
        }
        guard let value = object[key] as? String, !value.isEmpty else {
            return nil
        }
        return value
    }

    private func payloadHash(_ payload: String) -> Int {
        var hasher = Hasher()
        hasher.combine(payload.count)
        hasher.combine(payload.prefix(160))
        return hasher.finalize()
    }

    private func shouldReuseExistingTotals(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?
    ) -> Bool {
        guard let externalSessionID, !externalSessionID.isEmpty else {
            return false
        }
        if liveEnvelope?.externalSessionID == externalSessionID {
            return true
        }
        if existingSnapshot?.externalSessionID == externalSessionID {
            return true
        }
        return false
    }
}
