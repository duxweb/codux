import Foundation

enum AppAIProviderKind: String, Codable, CaseIterable, Identifiable, Sendable {
    case openAICompatible
    case anthropic

    var id: String { rawValue }

    var title: String {
        switch self {
        case .openAICompatible:
            return "OpenAI-Compatible API"
        case .anthropic:
            return "Claude API"
        }
    }

    var defaultDisplayName: String {
        switch self {
        case .openAICompatible:
            return "OpenAI API"
        case .anthropic:
            return "Claude API"
        }
    }

    var defaultModel: String {
        switch self {
        case .openAICompatible:
            return "gpt-4.1-mini"
        case .anthropic:
            return "claude-3-5-haiku-latest"
        }
    }

    var defaultBaseURL: String {
        switch self {
        case .openAICompatible:
            return "https://api.openai.com/v1"
        case .anthropic:
            return "https://api.anthropic.com/v1"
        }
    }

    var supportsAPICompletion: Bool {
        true
    }

    var allowsUserDefinedChannels: Bool {
        true
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

    static let defaultConfigurations: [AppAIProviderConfiguration] = []

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
}

struct AppMemorySettings: Codable, Equatable, Sendable {
    static let automaticExtractorProviderID = "automatic"

    var enabled = true
    var automaticInjectionEnabled = true
    var automaticExtractionEnabled = true
    var allowCrossProjectUserRecall = true
    var defaultExtractorProviderID = Self.automaticExtractorProviderID
    var maxInjectedUserWorkingMemories = 8
    var maxInjectedProjectWorkingMemories = 12
    var maxActiveWorkingEntries = 50
    var maxSummaryVersions = 10
    var summaryTargetTokenBudget = 1800

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
        maxInjectedUserWorkingMemories = max(
            0,
            min(
                24,
                try container.decodeIfPresent(Int.self, forKey: .maxInjectedUserWorkingMemories)
                    ?? 8)
        )
        maxInjectedProjectWorkingMemories = max(
            0,
            min(
                32,
                try container.decodeIfPresent(Int.self, forKey: .maxInjectedProjectWorkingMemories)
                    ?? 12))
        maxActiveWorkingEntries = max(
            5,
            min(
                200, try container.decodeIfPresent(Int.self, forKey: .maxActiveWorkingEntries) ?? 50
            ))
        maxSummaryVersions = max(
            1, min(50, try container.decodeIfPresent(Int.self, forKey: .maxSummaryVersions) ?? 10))
        summaryTargetTokenBudget = max(
            400,
            min(
                6000,
                try container.decodeIfPresent(Int.self, forKey: .summaryTargetTokenBudget) ?? 1800))
    }
}

struct AppAISettings: Codable, Equatable, Sendable {
    var runtimeTools = AppAIToolPermissionSettings()
    var globalPrompt = ""
    var memory = AppMemorySettings()
    var pet = AppAIPetSettings()
    var providers = AppAIProviderConfiguration.defaultConfigurations

    init() {}

    enum CodingKeys: String, CodingKey {
        case runtimeTools
        case globalPrompt
        case memory
        case pet
        case providers
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        runtimeTools =
            try container.decodeIfPresent(AppAIToolPermissionSettings.self, forKey: .runtimeTools)
            ?? .init()
        globalPrompt = try container.decodeIfPresent(String.self, forKey: .globalPrompt) ?? ""
        memory = try container.decodeIfPresent(AppMemorySettings.self, forKey: .memory) ?? .init()
        pet = try container.decodeIfPresent(AppAIPetSettings.self, forKey: .pet) ?? .init()
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
        var existingByID: [String: AppAIProviderConfiguration] = [:]
        for provider in providers
        where provider.kind.supportsAPICompletion && provider.id.hasPrefix("api-") {
            existingByID[provider.id] = provider
        }
        providers = existingByID.values.sorted {
            if $0.priority == $1.priority {
                return $0.displayName.localizedCaseInsensitiveCompare($1.displayName)
                    == .orderedAscending
            }
            return $0.priority < $1.priority
        }
        if memory.defaultExtractorProviderID != AppMemorySettings.automaticExtractorProviderID,
            providers.contains(where: {
                $0.id == memory.defaultExtractorProviderID && $0.useForMemoryExtraction
                    && $0.isEnabled && $0.kind.supportsAPICompletion
            }) == false
        {
            memory.defaultExtractorProviderID = AppMemorySettings.automaticExtractorProviderID
        }
        if pet.speechProviderID != AppAIPetSettings.automaticSpeechProviderID,
           providers.contains(where: {
               $0.id == pet.speechProviderID && $0.isEnabled && $0.kind.supportsAPICompletion
           }) == false {
            pet.speechProviderID = AppAIPetSettings.automaticSpeechProviderID
        }
    }

    func provider(withID id: String) -> AppAIProviderConfiguration? {
        providers.first(where: { $0.id == id })
    }

    func preferredExtractionProviderID() -> String? {
        providers
            .filter { $0.isEnabled && $0.useForMemoryExtraction && $0.kind.supportsAPICompletion }
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
            .filter { $0.isEnabled && $0.useForMemoryExtraction && $0.kind.supportsAPICompletion }
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
