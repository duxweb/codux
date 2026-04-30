import Foundation

struct OpenAICompatibleProviderClient: AIProviderClient {
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
                    ? "https://api.openai.com/v1/chat/completions"
                    : normalizedEndpointURL(from: baseURLString))
        else {
            throw AIProviderError.invalidBaseURL
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
        urlRequest.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        let payload = OpenAIChatCompletionRequest(
            model: configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? AppAIProviderKind.openAICompatible.defaultModel : configuration.model,
            messages: makeMessages(for: request)
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

        let decoded = try JSONDecoder().decode(OpenAIChatCompletionResponse.self, from: data)
        guard
            let content = decoded.choices.first?.message.content?.trimmingCharacters(
                in: .whitespacesAndNewlines),
            !content.isEmpty
        else {
            throw AIProviderError.emptyResponse
        }
        return content
    }

    private func normalizedEndpointURL(from baseURL: String) -> String {
        let trimmed = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasSuffix("/chat/completions") {
            return trimmed
        }
        if trimmed.hasSuffix("/") {
            return "\(trimmed)v1/chat/completions"
        }
        if trimmed.hasSuffix("/v1") {
            return "\(trimmed)/chat/completions"
        }
        return "\(trimmed)/v1/chat/completions"
    }

    private func makeMessages(for request: AIProviderCompletionRequest)
        -> [OpenAIChatCompletionMessage]
    {
        var messages: [OpenAIChatCompletionMessage] = []
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            messages.append(OpenAIChatCompletionMessage(role: "system", content: systemPrompt))
        }
        messages.append(OpenAIChatCompletionMessage(role: "user", content: request.prompt))
        return messages
    }
}

private struct OpenAIChatCompletionRequest: Encodable {
    var model: String
    var messages: [OpenAIChatCompletionMessage]
}

private struct OpenAIChatCompletionMessage: Codable {
    var role: String
    var content: String
}

private struct OpenAIChatCompletionResponse: Decodable {
    struct Choice: Decodable {
        struct Message: Decodable {
            var content: String?
        }

        var message: Message
    }

    var choices: [Choice]
}
