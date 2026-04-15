import Foundation

actor AIResponseRuntimeSyncService {
    static let shared = AIResponseRuntimeSyncService()

    private let toolDriverFactory = AIToolDriverFactory.shared
    private let openCodeGlobalEventService = OpenCodeGlobalEventService.shared
    private let session = URLSession(configuration: .ephemeral)
    private var lastPayloadBySessionID: [String: AIResponseStatePayload] = [:]

    func responseStateUpdates(
        liveEnvelopes: [AIToolUsageEnvelope],
        projects: [Project],
        toolFilter: String? = nil
    ) async -> [AIResponseStatePayload] {
        var updates: [AIResponseStatePayload] = []

        let filteredEnvelopes = liveEnvelopes.filter { envelope in
            guard toolDriverFactory.isRealtimeTool(envelope.tool) else {
                return false
            }
            if let toolFilter {
                return toolDriverFactory.canonicalToolName(envelope.tool) == toolFilter
            }
            return true
        }

        updates.append(contentsOf: await opencodeUpdates(from: filteredEnvelopes, projects: projects))

        return updates
    }

    private func opencodeUpdates(from envelopes: [AIToolUsageEnvelope], projects: [Project]) async -> [AIResponseStatePayload] {
        let opencodeEnvelopes = envelopes.filter { toolDriverFactory.canonicalToolName($0.tool) == "opencode" }
        guard !opencodeEnvelopes.isEmpty else {
            return []
        }

        var grouped: [String: [AIToolUsageEnvelope]] = [:]
        for envelope in opencodeEnvelopes {
            guard let projectID = UUID(uuidString: envelope.projectId),
                  let project = projects.first(where: { $0.id == projectID }) else {
                continue
            }
            grouped[project.path, default: []].append(envelope)
        }

        var updates: [AIResponseStatePayload] = []
        for (projectPath, groupedEnvelopes) in grouped {
            let statuses = if let cached = await openCodeGlobalEventService.cachedStatuses(directory: projectPath) {
                cached
            } else {
                await fetchOpenCodeSessionStatuses(directory: projectPath)
            }
            guard let statuses else {
                continue
            }

            for envelope in groupedEnvelopes {
                guard let sessionID = UUID(uuidString: envelope.sessionId),
                      let projectID = UUID(uuidString: envelope.projectId) else {
                    continue
                }

                let matchedState: AIResponseState? = {
                    if let externalSessionID = envelope.externalSessionID,
                       let state = statuses[externalSessionID] {
                        return state
                    }
                    if groupedEnvelopes.count == 1, let state = statuses.values.first {
                        return state
                    }
                    return nil
                }()

                guard let responseState = matchedState else {
                    continue
                }

                if let payload = deduplicatedPayload(
                    sessionID: sessionID,
                    projectID: projectID,
                    tool: "opencode",
                    responseState: responseState,
                    updatedAt: Date().timeIntervalSince1970
                ) {
                    updates.append(payload)
                }
            }
        }

        return updates
    }

    private func fetchOpenCodeSessionStatuses(directory: String) async -> [String: AIResponseState]? {
        var components = URLComponents(string: "http://127.0.0.1:4096/session/status")
        components?.queryItems = [
            URLQueryItem(name: "directory", value: directory),
        ]
        guard let url = components?.url else {
            return nil
        }

        var request = URLRequest(url: url)
        request.timeoutInterval = 0.18

        guard let (data, response) = try? await session.data(for: request),
              let httpResponse = response as? HTTPURLResponse,
              (200 ..< 300).contains(httpResponse.statusCode),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }

        var result: [String: AIResponseState] = [:]
        for (sessionID, rawStatus) in object {
            guard let rawStatus = rawStatus as? [String: Any],
                  let type = rawStatus["type"] as? String else {
                continue
            }
            switch type {
            case "busy":
                result[sessionID] = .responding
            case "idle":
                result[sessionID] = .idle
            default:
                continue
            }
        }
        return result.isEmpty ? nil : result
    }

    private func deduplicatedPayload(
        sessionID: UUID,
        projectID: UUID,
        tool: String,
        responseState: AIResponseState,
        updatedAt: Double
    ) -> AIResponseStatePayload? {
        let payload = AIResponseStatePayload(
            sessionId: sessionID.uuidString,
            sessionInstanceId: nil,
            invocationId: nil,
            projectId: projectID.uuidString,
            projectPath: nil,
            tool: tool,
            responseState: responseState,
            updatedAt: updatedAt
        )

        if let existing = lastPayloadBySessionID[sessionID.uuidString],
           existing.tool == payload.tool,
           existing.responseState == payload.responseState,
           existing.updatedAt >= payload.updatedAt {
            return nil
        }

        lastPayloadBySessionID[sessionID.uuidString] = payload
        return payload
    }
}
