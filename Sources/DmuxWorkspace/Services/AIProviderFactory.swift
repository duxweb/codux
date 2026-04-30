import Foundation

struct AIProviderFactory: Sendable {
    func client(for kind: AppAIProviderKind) -> AIProviderClient {
        switch kind {
        case .openAICompatible:
            return OpenAICompatibleProviderClient()
        case .anthropic:
            return AnthropicProviderClient()
        }
    }
}
