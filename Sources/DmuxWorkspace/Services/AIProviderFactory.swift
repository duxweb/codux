import Foundation

struct AIProviderFactory: Sendable {
    let credentialStore: AICredentialStore

    func client(for kind: AppAIProviderKind) -> AIProviderClient {
        switch kind {
        case .openAICompatible:
            return OpenAICompatibleProviderClient(credentialStore: credentialStore)
        case .anthropic:
            return AnthropicProviderClient(credentialStore: credentialStore)
        }
    }
}
