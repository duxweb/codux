import Foundation

struct AIProviderCompletionRequest: Sendable {
    var prompt: String
    var systemPrompt: String?
    var workingDirectory: String?
}

enum AIProviderError: LocalizedError {
    case unavailableProvider
    case missingAPIKey
    case invalidBaseURL
    case emptyResponse
    case processFailure(String)
    case requestFailure(String)

    var errorDescription: String? {
        switch self {
        case .unavailableProvider:
            return "No available AI provider is configured for memory extraction."
        case .missingAPIKey:
            return "The selected AI provider is missing an API key."
        case .invalidBaseURL:
            return "The selected AI provider has an invalid base URL."
        case .emptyResponse:
            return "The AI provider returned an empty response."
        case .processFailure(let message):
            return message
        case .requestFailure(let message):
            return message
        }
    }
}

protocol AIProviderClient: Sendable {
    func complete(
        _ request: AIProviderCompletionRequest,
        configuration: AppAIProviderConfiguration
    ) async throws -> String
}
