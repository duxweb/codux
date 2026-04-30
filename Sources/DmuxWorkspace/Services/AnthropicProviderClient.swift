import Foundation

struct AnthropicProviderClient: AIProviderClient {
    func complete(
        _ request: AIProviderCompletionRequest,
        configuration: AppAIProviderConfiguration
    ) async throws -> String {
        let apiKey = configuration.apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !apiKey.isEmpty else {
            throw AIProviderError.missingAPIKey
        }
        let baseURLString = configuration.baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard
            let url = URL(
                string: baseURLString.isEmpty
                    ? "https://api.anthropic.com/v1/messages"
                    : normalizedEndpointURL(from: baseURLString))
        else {
            throw AIProviderError.invalidBaseURL
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
        urlRequest.setValue(apiKey, forHTTPHeaderField: "x-api-key")
        urlRequest.setValue("2023-06-01", forHTTPHeaderField: "anthropic-version")
        let payload = AnthropicMessagesRequest(
            model: configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? AppAIProviderKind.anthropic.defaultModel : configuration.model,
            maxTokens: 4096,
            system: normalizedNonEmptyString(request.systemPrompt),
            messages: [
                AnthropicMessage(role: "user", content: request.prompt)
            ]
        )
        urlRequest.httpBody = try JSONEncoder().encode(payload)

        let (data, response) = try await URLSession.shared.data(for: urlRequest)
        if let httpResponse = response as? HTTPURLResponse,
            !(200..<300).contains(httpResponse.statusCode)
        {
            let body =
                String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
                ?? ""
            throw AIProviderError.requestFailure(
                body.isEmpty ? "Provider returned HTTP \(httpResponse.statusCode)." : body)
        }

        let decoded = try JSONDecoder().decode(AnthropicMessagesResponse.self, from: data)
        let content = decoded.content
            .compactMap { $0.text?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty else {
            throw AIProviderError.emptyResponse
        }
        return content
    }

    private func normalizedEndpointURL(from baseURL: String) -> String {
        let trimmed = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasSuffix("/messages") {
            return trimmed
        }
        if trimmed.hasSuffix("/") {
            return "\(trimmed)v1/messages"
        }
        if trimmed.hasSuffix("/v1") {
            return "\(trimmed)/messages"
        }
        return "\(trimmed)/v1/messages"
    }
}

private struct AnthropicMessagesRequest: Encodable {
    enum CodingKeys: String, CodingKey {
        case model
        case maxTokens = "max_tokens"
        case system
        case messages
    }

    var model: String
    var maxTokens: Int
    var system: String?
    var messages: [AnthropicMessage]
}

private struct AnthropicMessage: Codable {
    var role: String
    var content: String
}

private struct AnthropicMessagesResponse: Decodable {
    struct ContentBlock: Decodable {
        var type: String?
        var text: String?
    }

    var content: [ContentBlock]
}
