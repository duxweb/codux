import Foundation

actor AIResponseRuntimeSyncService {
    static let shared = AIResponseRuntimeSyncService()

    func responseStateUpdates(
        liveEnvelopes: [AIToolUsageEnvelope],
        projects: [Project],
        toolFilter: String? = nil
    ) async -> [AIResponseStatePayload] {
        _ = liveEnvelopes
        _ = projects
        _ = toolFilter
        return []
    }
}
