import Foundation

struct CodexHookRuntimeEnvelope: Decodable, Sendable {
    var event: String
    var tool: String
    var dmuxSessionId: String
    var dmuxProjectId: String
    var dmuxProjectPath: String?
    var receivedAt: Double
    var payload: String
}

actor CodexHookRuntimeService {
    static let shared = CodexHookRuntimeService()
    private let logger = AppDebugLog.shared

    func handleIPCEnvelope(
        _ envelope: CodexHookRuntimeEnvelope,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot] = [:]
    ) async -> AIToolRuntimeIngressUpdate? {
        guard envelope.tool == "codex",
              let update = process(
                envelope: envelope,
                projects: projects,
                liveEnvelopes: liveEnvelopes,
                existingRuntime: existingRuntime
              ) else {
            return nil
        }

        return AIToolRuntimeIngressUpdate(
            responsePayloads: [update.responsePayload],
            runtimeSnapshotsBySessionID: [update.sessionID: update.runtimeSnapshot]
        )
    }

    private func process(
        envelope: CodexHookRuntimeEnvelope,
        projects: [Project],
        liveEnvelopes: [AIToolUsageEnvelope],
        existingRuntime: [UUID: AIRuntimeContextSnapshot]
    ) -> (sessionID: UUID, responsePayload: AIResponseStatePayload, runtimeSnapshot: AIRuntimeContextSnapshot)? {
        guard let sessionID = UUID(uuidString: envelope.dmuxSessionId),
              let projectID = UUID(uuidString: envelope.dmuxProjectId),
              let payloadData = envelope.payload.data(using: .utf8),
              let payloadObject = try? JSONSerialization.jsonObject(with: payloadData) as? [String: Any] else {
            logger.log("codex-hook", "invalid envelope event=\(envelope.event) session=\(envelope.dmuxSessionId)")
            return nil
        }

        let liveEnvelope = liveEnvelopes.first { UUID(uuidString: $0.sessionId) == sessionID }
        let existingSnapshot = existingRuntime[sessionID]
        let externalSessionID = stringValue(in: payloadObject, key: "session_id")
            ?? existingSnapshot?.externalSessionID
            ?? liveEnvelope?.externalSessionID
        let model = stringValue(in: payloadObject, key: "model")
            ?? existingSnapshot?.model
            ?? liveEnvelope?.model

        let updatedAt = max(
            envelope.receivedAt,
            liveEnvelope?.updatedAt ?? 0,
            existingSnapshot?.updatedAt ?? 0
        )

        switch envelope.event {
        case "UserPromptSubmit":
            let runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: model,
                inputTokens: max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                outputTokens: max(liveEnvelope?.outputTokens ?? 0, existingSnapshot?.outputTokens ?? 0),
                totalTokens: max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                updatedAt: updatedAt,
                responseState: .responding
            )
            return (
                sessionID,
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: projectID.uuidString,
                    projectPath: nil,
                    tool: "codex",
                    responseState: .responding,
                    updatedAt: updatedAt
                ),
                runtimeSnapshot
            )

        case "Stop":
            let transcriptPath = stringValue(in: payloadObject, key: "transcript_path")
            let parsedState = parseCodexRolloutRuntimeState(fileURL: transcriptPath.map(URL.init(fileURLWithPath:)))
            logger.log(
                "codex-hook",
                "stop session=\(sessionID.uuidString) transcript=\(transcriptPath ?? "nil") parsedModel=\(parsedState?.model ?? "nil") parsedTokens=\(parsedState?.totalTokens.map(String.init) ?? "nil")"
            )
            let runtimeSnapshot = AIRuntimeContextSnapshot(
                tool: "codex",
                externalSessionID: externalSessionID,
                model: parsedState?.model ?? model,
                inputTokens: parsedState?.totalTokens ?? max(liveEnvelope?.inputTokens ?? 0, existingSnapshot?.inputTokens ?? 0),
                outputTokens: 0,
                totalTokens: parsedState?.totalTokens ?? max(liveEnvelope?.totalTokens ?? 0, existingSnapshot?.totalTokens ?? 0),
                updatedAt: max(updatedAt, parsedState?.updatedAt ?? 0),
                responseState: .idle
            )
            return (
                sessionID,
                AIResponseStatePayload(
                    sessionId: sessionID.uuidString,
                    sessionInstanceId: nil,
                    invocationId: nil,
                    projectId: projectID.uuidString,
                    projectPath: nil,
                    tool: "codex",
                    responseState: .idle,
                    updatedAt: runtimeSnapshot.updatedAt
                ),
                runtimeSnapshot
            )

        default:
            _ = projects
            logger.log("codex-hook", "ignore event=\(envelope.event) session=\(sessionID.uuidString)")
            return nil
        }
    }

    private func stringValue(in object: [String: Any], key: String) -> String? {
        guard let value = object[key] as? String, !value.isEmpty else {
            return nil
        }
        return value
    }
}
