import Foundation

enum AppAIProviderKind: String, Codable, CaseIterable, Identifiable, Sendable {
    case claude
    case codex
    case gemini
    case opencode
    case openAICompatible

    var id: String { rawValue }

    var title: String {
        switch self {
        case .claude:
            return "Claude"
        case .codex:
            return "Codex"
        case .gemini:
            return "Gemini"
        case .opencode:
            return "OpenCode"
        case .openAICompatible:
            return "OpenAI-Compatible API"
        }
    }

    var defaultDisplayName: String {
        title
    }

    var defaultModel: String {
        switch self {
        case .claude:
            return "sonnet"
        case .codex:
            return "gpt-5-codex"
        case .gemini:
            return "gemini-2.5-pro"
        case .opencode:
            return ""
        case .openAICompatible:
            return "gpt-4.1-mini"
        }
    }

    var builtInProviderID: String {
        switch self {
        case .claude:
            return "builtin-claude"
        case .codex:
            return "builtin-codex"
        case .gemini:
            return "builtin-gemini"
        case .opencode:
            return "builtin-opencode"
        case .openAICompatible:
            return "custom-openai-compatible"
        }
    }
}

struct AppAIProviderConfiguration: Identifiable, Codable, Equatable, Sendable {
    var id: String
    var kind: AppAIProviderKind
    var displayName: String
    var isEnabled: Bool
    var model: String
    var baseURL: String
    var apiKeyReference: String?
    var useForMemoryExtraction: Bool
    var priority: Int

    init(
        id: String,
        kind: AppAIProviderKind,
        displayName: String,
        isEnabled: Bool = true,
        model: String = "",
        baseURL: String = "",
        apiKeyReference: String? = nil,
        useForMemoryExtraction: Bool = true,
        priority: Int = 0
    ) {
        self.id = id
        self.kind = kind
        self.displayName = displayName
        self.isEnabled = isEnabled
        self.model = model
        self.baseURL = baseURL
        self.apiKeyReference = apiKeyReference
        self.useForMemoryExtraction = useForMemoryExtraction
        self.priority = priority
    }

    static func builtIn(_ kind: AppAIProviderKind, priority: Int) -> AppAIProviderConfiguration {
        AppAIProviderConfiguration(
            id: kind.builtInProviderID,
            kind: kind,
            displayName: kind.defaultDisplayName,
            isEnabled: kind != .opencode,
            model: kind.defaultModel,
            baseURL: "",
            apiKeyReference: nil,
            useForMemoryExtraction: kind != .opencode,
            priority: priority
        )
    }

    static let defaultConfigurations: [AppAIProviderConfiguration] = [
        .builtIn(.claude, priority: 0),
        .builtIn(.codex, priority: 1),
        .builtIn(.gemini, priority: 2),
        .builtIn(.opencode, priority: 3),
        AppAIProviderConfiguration(
            id: AppAIProviderKind.openAICompatible.builtInProviderID,
            kind: .openAICompatible,
            displayName: AppAIProviderKind.openAICompatible.defaultDisplayName,
            isEnabled: false,
            model: AppAIProviderKind.openAICompatible.defaultModel,
            baseURL: "https://api.openai.com/v1",
            apiKeyReference: nil,
            useForMemoryExtraction: false,
            priority: 4
        ),
    ]
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
        automaticInjectionEnabled = try container.decodeIfPresent(Bool.self, forKey: .automaticInjectionEnabled) ?? true
        automaticExtractionEnabled = try container.decodeIfPresent(Bool.self, forKey: .automaticExtractionEnabled) ?? true
        allowCrossProjectUserRecall = try container.decodeIfPresent(Bool.self, forKey: .allowCrossProjectUserRecall) ?? true
        defaultExtractorProviderID = try container.decodeIfPresent(String.self, forKey: .defaultExtractorProviderID) ?? Self.automaticExtractorProviderID
        maxInjectedUserWorkingMemories = max(0, min(24, try container.decodeIfPresent(Int.self, forKey: .maxInjectedUserWorkingMemories) ?? 8))
        maxInjectedProjectWorkingMemories = max(0, min(32, try container.decodeIfPresent(Int.self, forKey: .maxInjectedProjectWorkingMemories) ?? 12))
        maxActiveWorkingEntries = max(5, min(200, try container.decodeIfPresent(Int.self, forKey: .maxActiveWorkingEntries) ?? 50))
        maxSummaryVersions = max(1, min(50, try container.decodeIfPresent(Int.self, forKey: .maxSummaryVersions) ?? 10))
        summaryTargetTokenBudget = max(400, min(6000, try container.decodeIfPresent(Int.self, forKey: .summaryTargetTokenBudget) ?? 1800))
    }
}

struct AppAISettings: Codable, Equatable, Sendable {
    var runtimeTools = AppAIToolPermissionSettings()
    var memory = AppMemorySettings()
    var providers = AppAIProviderConfiguration.defaultConfigurations

    init() {}

    enum CodingKeys: String, CodingKey {
        case runtimeTools
        case memory
        case providers
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        runtimeTools = try container.decodeIfPresent(AppAIToolPermissionSettings.self, forKey: .runtimeTools) ?? .init()
        memory = try container.decodeIfPresent(AppMemorySettings.self, forKey: .memory) ?? .init()
        providers = try container.decodeIfPresent([AppAIProviderConfiguration].self, forKey: .providers) ?? AppAIProviderConfiguration.defaultConfigurations
        migrateMissingDefaultProviders()
    }

    mutating func migrateMissingDefaultProviders() {
        var existingByID = Dictionary(uniqueKeysWithValues: providers.map { ($0.id, $0) })
        for (index, provider) in AppAIProviderConfiguration.defaultConfigurations.enumerated() {
            if existingByID[provider.id] == nil {
                existingByID[provider.id] = provider
            } else if existingByID[provider.id]?.priority == 0 && provider.priority != 0 && provider.id != AppAIProviderKind.claude.builtInProviderID {
                existingByID[provider.id]?.priority = provider.priority
            }
            _ = index
        }
        providers = existingByID.values.sorted {
            if $0.priority == $1.priority {
                return $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
            }
            return $0.priority < $1.priority
        }
        if memory.defaultExtractorProviderID != AppMemorySettings.automaticExtractorProviderID,
           providers.contains(where: { $0.id == memory.defaultExtractorProviderID && $0.useForMemoryExtraction && $0.isEnabled }) == false {
            memory.defaultExtractorProviderID = AppMemorySettings.automaticExtractorProviderID
        }
    }

    func provider(withID id: String) -> AppAIProviderConfiguration? {
        providers.first(where: { $0.id == id })
    }

    func preferredExtractionProviderID() -> String? {
        providers
            .filter { $0.isEnabled && $0.useForMemoryExtraction }
            .sorted { lhs, rhs in
                if lhs.priority == rhs.priority {
                    return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
                }
                return lhs.priority < rhs.priority
            }
            .first?
            .id
    }

    func preferredExtractionProvider(forTool tool: String?) -> AppAIProviderConfiguration? {
        if let tool,
           let kind = AppAIProviderKind(rawValue: Self.canonicalProviderToolName(tool)),
           let provider = provider(withID: kind.builtInProviderID),
           provider.isEnabled,
           provider.useForMemoryExtraction {
            return provider
        }

        return providers
            .filter { $0.isEnabled && $0.useForMemoryExtraction }
            .sorted { lhs, rhs in
                if lhs.priority == rhs.priority {
                    return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
                }
                return lhs.priority < rhs.priority
            }
            .first
    }

    private static func canonicalProviderToolName(_ tool: String) -> String {
        switch tool.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "claude", "claude-code":
            return AppAIProviderKind.claude.rawValue
        case "codex":
            return AppAIProviderKind.codex.rawValue
        case "gemini":
            return AppAIProviderKind.gemini.rawValue
        case "opencode":
            return AppAIProviderKind.opencode.rawValue
        default:
            return tool.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        }
    }
}
