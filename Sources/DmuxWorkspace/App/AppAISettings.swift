import Foundation

enum AppAIProviderKind: String, Codable, CaseIterable, Identifiable, Sendable {
    case openAICompatible
    case anthropic
    case localLlama

    var id: String { rawValue }

    var title: String {
        switch self {
        case .openAICompatible:
            return "OpenAI-Compatible API"
        case .anthropic:
            return "Claude API"
        case .localLlama:
            return String(
                localized: "settings.ai.provider.kind.local_llama",
                defaultValue: "Llama Model",
                bundle: .module
            )
        }
    }

    var defaultDisplayName: String {
        switch self {
        case .openAICompatible:
            return "OpenAI API"
        case .anthropic:
            return "Claude API"
        case .localLlama:
            return String(
                localized: "settings.ai.provider.default.local_llama",
                defaultValue: "Llama Model",
                bundle: .module
            )
        }
    }

    var defaultModel: String {
        switch self {
        case .openAICompatible:
            return "gpt-4.1-mini"
        case .anthropic:
            return "claude-3-5-haiku-latest"
        case .localLlama:
            return LocalLlamaModelCatalog.defaultModelID
        }
    }

    var defaultBaseURL: String {
        switch self {
        case .openAICompatible:
            return "https://api.openai.com/v1"
        case .anthropic:
            return "https://api.anthropic.com/v1"
        case .localLlama:
            return ""
        }
    }

    var supportsAPICompletion: Bool {
        switch self {
        case .openAICompatible, .anthropic:
            return true
        case .localLlama:
            return false
        }
    }

    var supportsMemoryExtraction: Bool {
        true
    }

    var memoryExtractionTranscriptTokenLimit: Int? {
        switch self {
        case .openAICompatible, .anthropic:
            return nil
        case .localLlama:
            return 1200
        }
    }

    var supportsPetSpeech: Bool {
        switch self {
        case .openAICompatible, .anthropic, .localLlama:
            return true
        }
    }

    var usesAPIConfiguration: Bool {
        supportsAPICompletion
    }

    var allowsUserDefinedChannels: Bool {
        switch self {
        case .openAICompatible, .anthropic:
            return true
        case .localLlama:
            return false
        }
    }

}

struct AIProviderTestState: Equatable, Sendable {
    enum Status: Equatable, Sendable {
        case idle
        case testing
        case succeeded
        case failed
    }

    var status: Status = .idle
    var message: String?
    var updatedAt = Date()

    var isTesting: Bool {
        status == .testing
    }
}

struct AppAIProviderConfiguration: Identifiable, Codable, Equatable, Sendable {
    var id: String
    var kind: AppAIProviderKind
    var displayName: String
    var isEnabled: Bool
    var model: String
    var baseURL: String
    var apiKey: String
    var useForMemoryExtraction: Bool
    var priority: Int

    init(
        id: String,
        kind: AppAIProviderKind,
        displayName: String,
        isEnabled: Bool = true,
        model: String = "",
        baseURL: String = "",
        apiKey: String = "",
        useForMemoryExtraction: Bool = true,
        priority: Int = 0
    ) {
        self.id = id
        self.kind = kind
        self.displayName = displayName
        self.isEnabled = isEnabled
        self.model = model
        self.baseURL = baseURL
        self.apiKey = apiKey
        self.useForMemoryExtraction = useForMemoryExtraction
        self.priority = priority
    }

    enum CodingKeys: String, CodingKey {
        case id
        case kind
        case displayName
        case isEnabled
        case model
        case baseURL
        case apiKey
        case useForMemoryExtraction
        case priority
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        kind = try container.decode(AppAIProviderKind.self, forKey: .kind)
        displayName = try container.decode(String.self, forKey: .displayName)
        isEnabled = try container.decodeIfPresent(Bool.self, forKey: .isEnabled) ?? true
        model = try container.decodeIfPresent(String.self, forKey: .model) ?? ""
        baseURL = try container.decodeIfPresent(String.self, forKey: .baseURL) ?? ""
        apiKey = try container.decodeIfPresent(String.self, forKey: .apiKey) ?? ""
        useForMemoryExtraction =
            try container.decodeIfPresent(Bool.self, forKey: .useForMemoryExtraction) ?? true
        priority = try container.decodeIfPresent(Int.self, forKey: .priority) ?? 0
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(kind, forKey: .kind)
        try container.encode(displayName, forKey: .displayName)
        try container.encode(isEnabled, forKey: .isEnabled)
        try container.encode(model, forKey: .model)
        try container.encode(baseURL, forKey: .baseURL)
        try container.encode(apiKey, forKey: .apiKey)
        try container.encode(useForMemoryExtraction, forKey: .useForMemoryExtraction)
        try container.encode(priority, forKey: .priority)
    }

    static let localLlamaProviderID = "local-llama-memory"

    static let defaultConfigurations: [AppAIProviderConfiguration] = [
        AppAIProviderConfiguration(
            id: localLlamaProviderID,
            kind: .localLlama,
            displayName: AppAIProviderKind.localLlama.defaultDisplayName,
            isEnabled: true,
            model: AppAIProviderKind.localLlama.defaultModel,
            baseURL: "",
            apiKey: "",
            useForMemoryExtraction: true,
            priority: 0
        )
    ]

    var localizedDisplayName: String {
        if id == Self.localLlamaProviderID,
           kind == .localLlama,
           Self.isDefaultLocalLlamaDisplayName(displayName) {
            return kind.defaultDisplayName
        }
        return normalizedNonEmptyString(displayName) ?? kind.defaultDisplayName
    }

    static func customAPIChannel(
        kind: AppAIProviderKind,
        priority: Int,
        displayName: String? = nil,
        model: String? = nil,
        baseURL: String? = nil
    ) -> AppAIProviderConfiguration {
        AppAIProviderConfiguration(
            id: "api-\(kind.rawValue)-\(UUID().uuidString)",
            kind: kind,
            displayName: displayName ?? kind.defaultDisplayName,
            isEnabled: true,
            model: model ?? kind.defaultModel,
            baseURL: baseURL ?? kind.defaultBaseURL,
            apiKey: "",
            useForMemoryExtraction: true,
            priority: priority
        )
    }

    static func normalizedDefaultProviderDisplayName(
        existing: String?,
        defaultDisplayName: String,
        providerID: String,
        kind: AppAIProviderKind
    ) -> String {
        guard providerID == localLlamaProviderID,
              kind == .localLlama,
              isDefaultLocalLlamaDisplayName(existing) else {
            return normalizedNonEmptyString(existing) ?? defaultDisplayName
        }
        return defaultDisplayName
    }

    private static func isDefaultLocalLlamaDisplayName(_ value: String?) -> Bool {
        guard let normalized = normalizedNonEmptyString(value)?
            .lowercased()
            .replacingOccurrences(of: " ", with: "") else {
            return true
        }
        return [
            "localllamamemory",
            "localllama",
            "llamamodel",
            "llama模型",
        ].contains(normalized)
    }
}

struct AppMemorySettings: Codable, Equatable, Sendable {
    static let automaticExtractorProviderID = "automatic"

    var enabled = true
    var automaticInjectionEnabled = true
    var automaticExtractionEnabled = true
    var allowCrossProjectUserRecall = true
    var defaultExtractorProviderID = Self.automaticExtractorProviderID
    var maxInjectedUserWorkingMemories = 4
    var maxInjectedProjectWorkingMemories = 6
    var maxActiveWorkingEntries = 50
    var maxSummaryVersions = 10
    var summaryTargetTokenBudget = 900
    var maxInjectedSummaryTokens = 900
    var extractionIdleDelaySeconds = 120
    var sessionExtractionCooldownSeconds = 900
    var maxExtractionTranscriptLines = 80
    var maxExtractionTranscriptTokens = 8000

    init() {}

    enum CodingKeys: String, CodingKey {
        case enabled
        case automaticInjectionEnabled
        case automaticExtractionEnabled
        case allowCrossProjectUserRecall
        case defaultExtractorProviderID
        case maxInjectedUserWorkingMemories
        case maxInjectedProjectWorkingMemories
        case maxActiveWorkingEntries
        case maxSummaryVersions
        case summaryTargetTokenBudget
        case maxInjectedSummaryTokens
        case extractionIdleDelaySeconds
        case sessionExtractionCooldownSeconds
        case maxExtractionTranscriptLines
        case maxExtractionTranscriptTokens
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try container.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        automaticInjectionEnabled =
            try container.decodeIfPresent(Bool.self, forKey: .automaticInjectionEnabled) ?? true
        automaticExtractionEnabled =
            try container.decodeIfPresent(Bool.self, forKey: .automaticExtractionEnabled) ?? true
        allowCrossProjectUserRecall =
            try container.decodeIfPresent(Bool.self, forKey: .allowCrossProjectUserRecall) ?? true
        defaultExtractorProviderID =
            try container.decodeIfPresent(String.self, forKey: .defaultExtractorProviderID)
            ?? Self.automaticExtractorProviderID
        let decodedUserWorkingLimit =
            try container.decodeIfPresent(Int.self, forKey: .maxInjectedUserWorkingMemories)
        maxInjectedUserWorkingMemories = max(
            0,
            min(
                24,
                decodedUserWorkingLimit == 8 ? 4 : decodedUserWorkingLimit ?? 4)
        )
        let decodedProjectWorkingLimit =
            try container.decodeIfPresent(Int.self, forKey: .maxInjectedProjectWorkingMemories)
        maxInjectedProjectWorkingMemories = max(
            0,
            min(
                32,
                decodedProjectWorkingLimit == 12 ? 6 : decodedProjectWorkingLimit ?? 6))
        maxActiveWorkingEntries = max(
            5,
            min(
                200, try container.decodeIfPresent(Int.self, forKey: .maxActiveWorkingEntries) ?? 50
            ))
        maxSummaryVersions = max(
            1, min(50, try container.decodeIfPresent(Int.self, forKey: .maxSummaryVersions) ?? 10))
        let decodedSummaryTarget =
            try container.decodeIfPresent(Int.self, forKey: .summaryTargetTokenBudget)
        summaryTargetTokenBudget = max(
            400,
            min(
                3000,
                decodedSummaryTarget == 1800 ? 900 : decodedSummaryTarget ?? 900))
        maxInjectedSummaryTokens = max(
            200,
            min(
                2000,
                try container.decodeIfPresent(Int.self, forKey: .maxInjectedSummaryTokens) ?? 900))
        extractionIdleDelaySeconds = max(
            0,
            min(
                900,
                try container.decodeIfPresent(Int.self, forKey: .extractionIdleDelaySeconds) ?? 120
            ))
        sessionExtractionCooldownSeconds = max(
            0,
            min(
                7200,
                try container.decodeIfPresent(Int.self, forKey: .sessionExtractionCooldownSeconds)
                    ?? 900
            ))
        maxExtractionTranscriptLines = max(
            20,
            min(
                200,
                try container.decodeIfPresent(Int.self, forKey: .maxExtractionTranscriptLines)
                    ?? 80
            ))
        maxExtractionTranscriptTokens = max(
            2000,
            min(
                20000,
                try container.decodeIfPresent(Int.self, forKey: .maxExtractionTranscriptTokens)
                    ?? 8000
            ))
    }
}

struct AppAISettings: Codable, Equatable, Sendable {
    var runtimeTools = AppAIToolPermissionSettings()
    var globalPrompt = ""
    var memory = AppMemorySettings()
    var pet = AppAIPetSettings()
    var providers = AppAIProviderConfiguration.defaultConfigurations
    var localLlamaDownloadRoute = LocalLlamaModelDownloadRoute.china

    init() {}

    enum CodingKeys: String, CodingKey {
        case runtimeTools
        case globalPrompt
        case memory
        case pet
        case providers
        case localLlamaDownloadRoute
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        runtimeTools =
            try container.decodeIfPresent(AppAIToolPermissionSettings.self, forKey: .runtimeTools)
            ?? .init()
        globalPrompt = try container.decodeIfPresent(String.self, forKey: .globalPrompt) ?? ""
        memory = try container.decodeIfPresent(AppMemorySettings.self, forKey: .memory) ?? .init()
        pet = try container.decodeIfPresent(AppAIPetSettings.self, forKey: .pet) ?? .init()
        localLlamaDownloadRoute =
            try container.decodeIfPresent(
                LocalLlamaModelDownloadRoute.self,
                forKey: .localLlamaDownloadRoute
            ) ?? .china
        if let decodedProviders = try? container.decode(
            [LossyAppAIProviderConfiguration].self,
            forKey: .providers
        ) {
            providers = decodedProviders.compactMap(\.value)
        } else {
            providers = AppAIProviderConfiguration.defaultConfigurations
        }
        migrateMissingDefaultProviders()
    }

    mutating func migrateMissingDefaultProviders() {
        let decodedProviders = providers
        var migratedByID: [String: AppAIProviderConfiguration] = [:]

        for defaultProvider in AppAIProviderConfiguration.defaultConfigurations {
            var provider = defaultProvider
            if let existing = decodedProviders.first(where: {
                $0.id == defaultProvider.id && $0.kind == defaultProvider.kind
            }) {
                provider.displayName = AppAIProviderConfiguration
                    .normalizedDefaultProviderDisplayName(
                        existing: existing.displayName,
                        defaultDisplayName: defaultProvider.displayName,
                        providerID: defaultProvider.id,
                        kind: defaultProvider.kind
                    )
                provider.isEnabled = existing.isEnabled
                provider.model = normalizedNonEmptyString(existing.model) ?? defaultProvider.model
                provider.baseURL = defaultProvider.baseURL
                provider.apiKey = ""
                provider.useForMemoryExtraction = existing.useForMemoryExtraction
                provider.priority = existing.priority
            }
            migratedByID[provider.id] = provider
        }

        for provider in decodedProviders
        where provider.kind.allowsUserDefinedChannels && provider.id.hasPrefix("api-") {
            migratedByID[provider.id] = provider
        }

        providers = migratedByID.values.sorted {
            if $0.priority == $1.priority {
                return $0.displayName.localizedCaseInsensitiveCompare($1.displayName)
                    == .orderedAscending
            }
            return $0.priority < $1.priority
        }
        if memory.defaultExtractorProviderID != AppMemorySettings.automaticExtractorProviderID,
            providers.contains(where: {
                $0.id == memory.defaultExtractorProviderID && $0.useForMemoryExtraction
                    && $0.isEnabled && $0.kind.supportsMemoryExtraction
            }) == false
        {
            memory.defaultExtractorProviderID = AppMemorySettings.automaticExtractorProviderID
        }
        if pet.speechProviderID != AppAIPetSettings.automaticSpeechProviderID,
           providers.contains(where: {
               $0.id == pet.speechProviderID && $0.isEnabled && $0.kind.supportsPetSpeech
           }) == false {
            pet.speechProviderID = AppAIPetSettings.automaticSpeechProviderID
        }
    }

    func provider(withID id: String) -> AppAIProviderConfiguration? {
        providers.first(where: { $0.id == id })
    }

    func preferredExtractionProviderID() -> String? {
        providers
            .filter { $0.isEnabled && $0.useForMemoryExtraction && $0.kind.supportsMemoryExtraction }
            .sorted { lhs, rhs in
                if lhs.priority == rhs.priority {
                    return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName)
                        == .orderedAscending
                }
                return lhs.priority < rhs.priority
            }
            .first?
            .id
    }

    func preferredExtractionProvider() -> AppAIProviderConfiguration? {
        return
            providers
            .filter { $0.isEnabled && $0.useForMemoryExtraction && $0.kind.supportsMemoryExtraction }
            .sorted { lhs, rhs in
                if lhs.priority == rhs.priority {
                    return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName)
                        == .orderedAscending
                }
                return lhs.priority < rhs.priority
            }
            .first
    }
}

private struct LossyAppAIProviderConfiguration: Decodable {
    var value: AppAIProviderConfiguration?

    init(from decoder: Decoder) throws {
        value = try? AppAIProviderConfiguration(from: decoder)
    }
}
