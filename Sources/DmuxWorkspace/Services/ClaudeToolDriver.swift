import Foundation

struct ClaudeToolDriver: AIToolDriver {
    let id = "claude"
    let aliases: Set<String> = ["claude", "claude-code"]
    let runtimeRefreshInterval: TimeInterval = 0.9
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let allowsRuntimeExternalSessionSwitch = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        AIRuntimeSourceLocator.claudeProjectLogURLs().map {
            AIToolRuntimeSourceDescriptor(path: $0.path, watchKind: .file)
        }
    }

    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate? {
        guard descriptor.watchKind == .file else {
            return nil
        }

        let fileURL = URL(fileURLWithPath: descriptor.path)
        let interruptEvents = await ClaudeRuntimeInterruptWatchCache.shared.process(
            fileURL: fileURL,
            projectPath: Optional<String>.none
        )
        guard !interruptEvents.isEmpty else {
            return nil
        }

        var responsePayloads: [AIResponseStatePayload] = []
        var runtimeSnapshotsBySessionID: [UUID: AIRuntimeContextSnapshot] = [:]

        for interruptEvent in interruptEvents {
            guard let envelope = liveEnvelopes.first(where: {
                canonicalTool($0.tool) == id
                    && $0.externalSessionID == interruptEvent.externalSessionID
                    && UUID(uuidString: $0.projectId).flatMap { projectID in
                        projects.first(where: { $0.id == projectID })
                    } != nil
            }),
                  let sessionID = UUID(uuidString: envelope.sessionId),
                  let projectID = UUID(uuidString: envelope.projectId) else {
                continue
            }

            AppDebugLog.shared.log(
                "claude-watcher",
                "interrupt session=\(sessionID.uuidString) external=\(interruptEvent.externalSessionID) updatedAt=\(interruptEvent.updatedAt)"
            )

            responsePayloads.append(
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: envelope.sessionInstanceId,
                    invocationId: envelope.invocationId,
                    projectId: projectID.uuidString,
                    projectPath: envelope.projectPath,
                    tool: id,
                    responseState: .idle,
                    updatedAt: interruptEvent.updatedAt,
                    source: .watcher
                )
            )

            runtimeSnapshotsBySessionID[sessionID] = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: interruptEvent.externalSessionID,
                model: envelope.model,
                inputTokens: max(0, envelope.inputTokens ?? 0),
                outputTokens: max(0, envelope.outputTokens ?? 0),
                totalTokens: max(0, envelope.totalTokens ?? 0),
                updatedAt: interruptEvent.updatedAt,
                responseState: .idle,
                wasInterrupted: true,
                hasCompletedTurn: false,
                source: .watcher
            )
        }

        guard !responsePayloads.isEmpty || !runtimeSnapshotsBySessionID.isEmpty else {
            return nil
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: runtimeSnapshotsBySessionID
        )
    }

    func handleRuntimeSocketEvent(
        kind: String,
        payloadData: Data,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) async -> AIToolRuntimeIngressUpdate? {
        _ = projects
        guard kind == "claude-hook",
              let envelope = try? JSONDecoder().decode(ClaudeHookRuntimeEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.dmuxSessionId) else {
            return nil
        }

        let dedupeKey = "claude|\(envelope.event)|\(sessionID.uuidString)|\(envelope.receivedAt)|\(payloadHash(envelope.payload))"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.0) else {
            AppDebugLog.shared.log(
                "claude-hook",
                "drop duplicate event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "claude-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "claude-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let payloadObject: [String: Any]? = envelope.payload.data(using: .utf8)
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let externalSessionID = stringValue(in: payloadObject, key: "session_id")
            ?? existingSnapshot?.externalSessionID
            ?? liveEnvelope?.externalSessionID
        let projectPath = envelope.dmuxProjectPath
            ?? liveEnvelope?.projectPath
        let projectID = UUID(uuidString: envelope.dmuxProjectId)
        let updatedAt = max(envelope.receivedAt, existingSnapshot?.updatedAt ?? 0, liveEnvelope?.updatedAt ?? 0)
        let model = existingSnapshot?.model ?? liveEnvelope?.model
        let inputTokens = max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
        let outputTokens = max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
        let totalTokens = max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)

        switch envelope.event {
        case "UserPromptSubmit":
            await AIToolRuntimeResponseLatch.shared.markResponding(
                tool: id,
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID,
                updatedAt: updatedAt
            )
            if let projectPath, let externalSessionID, !projectPath.isEmpty, !externalSessionID.isEmpty {
                let fileURL = AIRuntimeSourceLocator.claudeSessionLogURL(
                    projectPath: projectPath,
                    externalSessionID: externalSessionID
                )
                await ClaudeRuntimeInterruptWatchCache.shared.prime(
                    fileURL: fileURL,
                    externalSessionID: externalSessionID
                )
                AppDebugLog.shared.log(
                    "claude-hook",
                    "prime interrupt watcher session=\(sessionID.uuidString) external=\(externalSessionID) file=\(fileURL.lastPathComponent)"
                )
            } else {
                AppDebugLog.shared.log(
                    "claude-hook",
                    "skip prime session=\(sessionID.uuidString) reason=missing-path-or-external"
                )
            }
            let runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: model,
                inputTokens: inputTokens,
                outputTokens: outputTokens,
                totalTokens: totalTokens,
                updatedAt: updatedAt,
                responseState: .responding,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
            let responsePayload = projectID.map {
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: $0.uuidString,
                    projectPath: nil,
                    tool: id,
                    responseState: .responding,
                    updatedAt: updatedAt,
                    source: .hook
                )
            }
            return AIToolRuntimeIngressUpdate(
                responsePayloads: responsePayload.map { [$0] } ?? [],
                runtimeSnapshotsBySessionID: [sessionID: runtimeSnapshot]
            )
        case "Notification":
            let notificationType = stringValue(in: payloadObject, key: "notification_type") ?? "unknown"
            AppDebugLog.shared.log(
                "claude-hook",
                "notification session=\(sessionID.uuidString) type=\(notificationType)"
            )
        case "Stop":
            let didReleaseDefinitiveStop = await AIToolRuntimeResponseLatch.shared.releaseIfDefinitiveStop(
                tool: id,
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID,
                stopUpdatedAt: updatedAt,
                wasInterrupted: false,
                hasCompletedTurn: true
            )
            AppDebugLog.shared.log(
                "claude-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
            let settledSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: model,
                inputTokens: inputTokens,
                outputTokens: outputTokens,
                totalTokens: totalTokens,
                updatedAt: updatedAt,
                responseState: didReleaseDefinitiveStop ? .idle : nil,
                wasInterrupted: false,
                hasCompletedTurn: didReleaseDefinitiveStop,
                source: .hook
            )
            if didReleaseDefinitiveStop == false {
                AppDebugLog.shared.log(
                    "claude-hook",
                    "defer stop session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=pending-queued-prompt"
                )
                return AIToolRuntimeIngressUpdate(
                    runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
                )
            }
            let responsePayload = projectID.map {
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: $0.uuidString,
                    projectPath: nil,
                    tool: id,
                    responseState: .idle,
                    updatedAt: updatedAt,
                    source: .hook
                )
            }
            return AIToolRuntimeIngressUpdate(
                responsePayloads: responsePayload.map { [$0] } ?? [],
                runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
            )
        case "Idle", "SessionEnd":
            let didReleaseSettledStop = await AIToolRuntimeResponseLatch.shared.releaseSettledIfPending(
                tool: id,
                runtimeSessionID: sessionID.uuidString,
                externalSessionID: externalSessionID
            )
            AppDebugLog.shared.log(
                "claude-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
            guard let didReleaseSettledStop else {
                let currentResponseState = existingSnapshot?.responseState ?? liveEnvelope?.responseState
                guard currentResponseState == .responding else {
                    AppDebugLog.shared.log(
                        "claude-hook",
                        "ignore settled session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=no-pending-prompt"
                    )
                    return nil
                }
                let settledSnapshot = AIRuntimeContextSnapshot(
                    tool: id,
                    externalSessionID: externalSessionID,
                    model: model,
                    inputTokens: inputTokens,
                    outputTokens: outputTokens,
                    totalTokens: totalTokens,
                    updatedAt: updatedAt,
                    responseState: .idle,
                    wasInterrupted: false,
                    hasCompletedTurn: false,
                    source: .hook
                )
                let responsePayload = projectID.map {
                    AIResponseStatePayload(
                        sessionId: sessionID.uuidString,
                        sessionInstanceId: nil,
                        invocationId: nil,
                        projectId: $0.uuidString,
                        projectPath: nil,
                        tool: id,
                        responseState: .idle,
                        updatedAt: updatedAt,
                        source: .hook
                    )
                }
                AppDebugLog.shared.log(
                    "claude-hook",
                    "fallback settled session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=responding-without-pending-state"
                )
                return AIToolRuntimeIngressUpdate(
                    responsePayloads: responsePayload.map { [$0] } ?? [],
                    runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
                )
            }
            let settledSnapshot = AIRuntimeContextSnapshot(
                tool: id,
                externalSessionID: externalSessionID,
                model: model,
                inputTokens: inputTokens,
                outputTokens: outputTokens,
                totalTokens: totalTokens,
                updatedAt: updatedAt,
                responseState: didReleaseSettledStop ? .idle : nil,
                wasInterrupted: false,
                hasCompletedTurn: false,
                source: .hook
            )
            if didReleaseSettledStop == false {
                AppDebugLog.shared.log(
                    "claude-hook",
                    "defer settled session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil") reason=pending-queued-prompt"
                )
                return AIToolRuntimeIngressUpdate(
                    runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
                )
            }
            let responsePayload = projectID.map {
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: $0.uuidString,
                    projectPath: nil,
                    tool: id,
                    responseState: .idle,
                    updatedAt: updatedAt,
                    source: .hook
                )
            }
            return AIToolRuntimeIngressUpdate(
                responsePayloads: responsePayload.map { [$0] } ?? [],
                runtimeSnapshotsBySessionID: [sessionID: settledSnapshot]
            )
        case "StopFailure", "SessionStart", "PreToolUse", "PostToolUse", "PermissionRequest":
            AppDebugLog.shared.log(
                "claude-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
        default:
            AppDebugLog.shared.log(
                "claude-hook",
                "ignore event=\(envelope.event) session=\(sessionID.uuidString)"
            )
        }

        return nil
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        _ = session
        return AIToolSessionCapabilities(canOpen: true, canRename: false, canRemove: true)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            return nil
        }
        return "claude --resume \(shellQuoted(sessionID))"
    }

    func removeSession(_ session: AISessionSummary) throws {
        let targetSessionID = session.externalSessionID ?? session.sessionID.uuidString
        let candidates = AIRuntimeSourceLocator.claudeProjectLogURLs().filter { fileURL in
            if fileURL.lastPathComponent == "\(targetSessionID).jsonl" {
                return true
            }
            guard let text = try? String(contentsOf: fileURL, encoding: .utf8) else {
                return false
            }
            return text.contains("\"sessionId\":\"\(targetSessionID)\"")
        }
        guard !candidates.isEmpty else {
            throw AIToolSessionControlError.sessionNotFound
        }

        let fileManager = FileManager.default
        for fileURL in candidates {
            try fileManager.removeItem(at: fileURL)
        }
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func stringValue(in object: [String: Any]?, key: String) -> String? {
        guard let object,
              let value = object[key] as? String,
              !value.isEmpty else {
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
}
