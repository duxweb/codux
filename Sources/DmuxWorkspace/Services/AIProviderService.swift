import Foundation

struct AIProviderSelectionService: Sendable {
    func preferredMemoryExtractionProvider(in settings: AppAISettings, tool: String?)
        -> AppAIProviderConfiguration?
    {
        candidateMemoryExtractionProviders(in: settings, tool: tool).first
    }

    func candidateMemoryExtractionProviders(in settings: AppAISettings, tool: String?)
        -> [AppAIProviderConfiguration]
    {
        _ = tool
        let enabledProviders = settings.providers
            .filter { $0.isEnabled && $0.useForMemoryExtraction && $0.kind.supportsAPICompletion }
            .sorted { lhs, rhs in
                if lhs.priority == rhs.priority {
                    return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName)
                        == .orderedAscending
                }
                return lhs.priority < rhs.priority
            }

        if settings.memory.defaultExtractorProviderID
            != AppMemorySettings.automaticExtractorProviderID
        {
            if let selected = settings.provider(withID: settings.memory.defaultExtractorProviderID),
                selected.isEnabled,
                selected.useForMemoryExtraction,
                selected.kind.supportsAPICompletion
            {
                return [selected]
            }
            return enabledProviders
        }

        return enabledProviders
    }
}
