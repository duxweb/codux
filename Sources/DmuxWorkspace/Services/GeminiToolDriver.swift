import Foundation

struct GeminiToolDriver: AIToolDriver {
    let id = "gemini"
    let aliases: Set<String> = ["gemini"]
    let runtimeRefreshInterval: TimeInterval = 0.75
    let isRealtimeTool = true
    let prefersHookDrivenResponseState = true
    let freezesDisplayTokensWhileResponding = true
    let allowsRuntimeExternalSessionSwitch = true
    let seedsObservedBaselineOnFreshLaunch = true

    func matches(tool: String) -> Bool {
        aliases.contains(tool)
    }

    func runtimeSourceDescriptors(project: Project, envelope: AIToolUsageEnvelope?) -> [AIToolRuntimeSourceDescriptor] {
        let projectPath = envelope?.projectPath ?? project.path
        guard let chatsDirectoryURL = AIRuntimeSourceLocator.geminiChatsDirectoryURL(projectPath: projectPath),
              FileManager.default.fileExists(atPath: chatsDirectoryURL.path) else {
            return []
        }
        return [AIToolRuntimeSourceDescriptor(path: chatsDirectoryURL.path, watchKind: .directory)]
    }

    func handleRuntimeIngressEvent(
        descriptor: AIToolRuntimeSourceDescriptor,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope]
    ) async -> AIToolRuntimeIngressUpdate? {
        guard descriptor.watchKind == .directory else {
            return nil
        }

        let matchingProjectPaths = Set(projects.compactMap { project -> String? in
            guard AIRuntimeSourceLocator.geminiChatsDirectoryURL(projectPath: project.path)?.path == descriptor.path else {
                return nil
            }
            return project.path
        })
        guard !matchingProjectPaths.isEmpty else {
            return nil
        }

        var responsePayloads: [AIResponseStatePayload] = []
        var runtimeSnapshotsBySessionID: [UUID: AIRuntimeContextSnapshot] = [:]

        for envelope in liveEnvelopes {
            guard canonicalTool(envelope.tool) == id,
                  let sessionID = UUID(uuidString: envelope.sessionId),
                  let projectID = UUID(uuidString: envelope.projectId),
                  let projectPath = envelope.projectPath,
                  matchingProjectPaths.contains(projectPath),
                  let snapshot = resolvedSnapshot(
                      projectPath: projectPath,
                      liveEnvelope: envelope,
                      existingSnapshot: nil,
                      responseStateOverride: nil,
                      updatedAt: envelope.updatedAt,
                      marksCompletedTurn: envelope.responseState == .idle,
                      source: .probe
                  ) else {
                continue
            }

            runtimeSnapshotsBySessionID[sessionID] = snapshot
            if let responseState = snapshot.responseState {
                responsePayloads.append(
                    AIResponseStatePayload(
                        sessionId: sessionID.uuidString,
                        sessionInstanceId: envelope.sessionInstanceId,
                        invocationId: envelope.invocationId,
                        projectId: projectID.uuidString,
                        projectPath: projectPath,
                        tool: id,
                        responseState: responseState,
                        updatedAt: snapshot.updatedAt,
                        source: .probe
                    )
                )
            }
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
        guard kind == "gemini-hook",
              let envelope = try? JSONDecoder().decode(GeminiHookRuntimeEnvelope.self, from: payloadData),
              let sessionID = UUID(uuidString: envelope.dmuxSessionId),
              let projectID = UUID(uuidString: envelope.dmuxProjectId) else {
            return nil
        }

        let dedupeKey = "gemini|\(envelope.event)|\(sessionID.uuidString)|\(payloadHash(envelope.payload))"
        guard await AIToolRuntimeEventDeduper.shared.shouldAccept(key: dedupeKey, ttl: 1.0) else {
            AppDebugLog.shared.log(
                "gemini-hook",
                "drop duplicate event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        if let liveEnvelope,
           canonicalTool(liveEnvelope.tool) != id {
            AppDebugLog.shared.log(
                "gemini-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) liveTool=\(liveEnvelope.tool)"
            )
            return nil
        }
        if liveEnvelope == nil,
           let existingSnapshot,
           canonicalTool(existingSnapshot.tool) != id {
            AppDebugLog.shared.log(
                "gemini-hook",
                "ignore stale event=\(envelope.event) session=\(sessionID.uuidString) runtimeTool=\(existingSnapshot.tool)"
            )
            return nil
        }

        let payloadObject: [String: Any]? = envelope.payload.data(using: .utf8)
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let externalSessionID = extractedSessionID(from: payloadObject)
            ?? existingSnapshot?.externalSessionID
            ?? normalizedSessionID(liveEnvelope?.externalSessionID)
        let projectPath = normalizedSessionID(envelope.dmuxProjectPath)
            ?? normalizedSessionID(liveEnvelope?.projectPath)
        let updatedAt = max(
            envelope.receivedAt,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        let responseStateOverride: AIResponseState?
        switch envelope.event {
        case "SessionStart":
            responseStateOverride = .idle
        case "BeforeAgent":
            responseStateOverride = .responding
        case "AfterAgent":
            responseStateOverride = .idle
        case "SessionEnd":
            AppDebugLog.shared.log(
                "gemini-hook",
                "event=\(envelope.event) session=\(sessionID.uuidString) external=\(externalSessionID ?? "nil")"
            )
            return nil
        default:
            AppDebugLog.shared.log(
                "gemini-hook",
                "ignore event=\(envelope.event) session=\(sessionID.uuidString)"
            )
            return nil
        }

        let runtimeSnapshot = resolvedSnapshot(
            projectPath: projectPath,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot,
            preferredExternalSessionID: externalSessionID,
            responseStateOverride: responseStateOverride,
            updatedAt: updatedAt,
            marksCompletedTurn: false,
            source: .hook
        ) ?? fallbackSnapshot(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot,
            responseStateOverride: responseStateOverride,
            updatedAt: updatedAt,
            marksCompletedTurn: false
        )

        let marksCompletedTurn: Bool = {
            guard envelope.event == "AfterAgent" else {
                return false
            }
            let previousTotal = max(
                liveEnvelope?.totalTokens ?? 0,
                existingSnapshot?.totalTokens ?? 0
            )
            return runtimeSnapshot.totalTokens > previousTotal
        }()

        let effectiveSnapshot: AIRuntimeContextSnapshot = {
            guard marksCompletedTurn else {
                return runtimeSnapshot
            }
            var next = runtimeSnapshot
            next.hasCompletedTurn = true
            return next
        }()

        AppDebugLog.shared.log(
            "gemini-hook",
            "event=\(envelope.event) session=\(sessionID.uuidString) external=\(effectiveSnapshot.externalSessionID ?? "nil") response=\(effectiveSnapshot.responseState?.rawValue ?? "nil") total=\(effectiveSnapshot.totalTokens) completed=\(marksCompletedTurn)"
        )

        let responsePayloads: [AIResponseStatePayload]
        if let responseState = effectiveSnapshot.responseState {
            responsePayloads = [
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: liveEnvelope?.sessionInstanceId,
                    invocationId: liveEnvelope?.invocationId,
                    projectId: projectID.uuidString,
                    projectPath: projectPath,
                    tool: id,
                    responseState: responseState,
                    updatedAt: effectiveSnapshot.updatedAt,
                    source: .hook
                ),
            ]
        } else {
            responsePayloads = []
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: responsePayloads,
            runtimeSnapshotsBySessionID: [sessionID: effectiveSnapshot]
        )
    }

    func sessionCapabilities(for session: AISessionSummary) -> AIToolSessionCapabilities {
        let canOpen = !(session.externalSessionID?.isEmpty ?? true)
        return AIToolSessionCapabilities(canOpen: canOpen, canRename: false, canRemove: false)
    }

    func resumeCommand(for session: AISessionSummary) -> String? {
        guard let sessionID = session.externalSessionID, !sessionID.isEmpty else {
            return nil
        }
        return "gemini --resume \(shellQuoted(sessionID))"
    }

    private func canonicalTool(_ tool: String) -> String {
        aliases.contains(tool) ? id : tool
    }

    private func normalizedSessionID(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func extractedSessionID(from object: [String: Any]?) -> String? {
        firstString(in: object, keys: ["session_id", "sessionId", "id"])
    }

    private func firstString(in root: Any?, keys: [String]) -> String? {
        var stack: [Any] = []
        if let root {
            stack.append(root)
        }

        while let current = stack.popLast() {
            if let dictionary = current as? [String: Any] {
                for key in keys {
                    if let value = dictionary[key] as? String,
                       !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        return value
                    }
                }
                stack.append(contentsOf: dictionary.values)
                continue
            }
            if let array = current as? [Any] {
                stack.append(contentsOf: array)
            }
        }

        return nil
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
        if normalizedSessionID(liveEnvelope?.externalSessionID) == externalSessionID {
            return true
        }
        if existingSnapshot?.externalSessionID == externalSessionID {
            return true
        }
        return false
    }

    private func resolvedSnapshot(
        projectPath: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?,
        preferredExternalSessionID: String? = nil,
        responseStateOverride: AIResponseState?,
        updatedAt: Double,
        marksCompletedTurn: Bool,
        source: AIRuntimeUpdateSource
    ) -> AIRuntimeContextSnapshot? {
        guard let projectPath = normalizedSessionID(projectPath) else {
            return nil
        }

        let externalSessionID = normalizedSessionID(preferredExternalSessionID)
            ?? normalizedSessionID(liveEnvelope?.externalSessionID)
            ?? existingSnapshot?.externalSessionID
        let startedAt = liveEnvelope?.startedAt ?? updatedAt
        let parsedState = parseGeminiSessionRuntimeState(
            projectPath: projectPath,
            startedAt: startedAt,
            preferredSessionID: externalSessionID,
            preferredSessionIsAuthoritative: externalSessionID != nil
        )
        guard let parsedState else {
            return nil
        }

        return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: parsedState.externalSessionID,
            model: parsedState.model
                ?? existingSnapshot?.model
                ?? normalizedSessionID(liveEnvelope?.model),
            inputTokens: parsedState.inputTokens,
            outputTokens: parsedState.outputTokens,
            totalTokens: parsedState.totalTokens,
            updatedAt: max(updatedAt, parsedState.updatedAt),
            responseState: responseStateOverride ?? parsedState.responseState,
            wasInterrupted: false,
            hasCompletedTurn: marksCompletedTurn,
            sessionOrigin: parsedState.origin,
            source: source
        )
    }

    private func fallbackSnapshot(
        externalSessionID: String?,
        liveEnvelope: AIToolUsageEnvelope?,
        existingSnapshot: AIRuntimeContextSnapshot?,
        responseStateOverride: AIResponseState?,
        updatedAt: Double,
        marksCompletedTurn: Bool
    ) -> AIRuntimeContextSnapshot {
        let canReuseExistingTotals = shouldReuseExistingTotals(
            externalSessionID: externalSessionID,
            liveEnvelope: liveEnvelope,
            existingSnapshot: existingSnapshot
        )
        let inputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0)
            : 0
        let outputTokens = canReuseExistingTotals
            ? max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0)
            : 0
        let totalTokens = canReuseExistingTotals
            ? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0)
            : 0

        return AIRuntimeContextSnapshot(
            tool: id,
            externalSessionID: externalSessionID,
            model: existingSnapshot?.model ?? normalizedSessionID(liveEnvelope?.model),
            inputTokens: inputTokens,
            outputTokens: outputTokens,
            totalTokens: totalTokens,
            updatedAt: updatedAt,
            responseState: responseStateOverride,
            wasInterrupted: false,
            hasCompletedTurn: marksCompletedTurn,
            source: .hook
        )
    }
}
