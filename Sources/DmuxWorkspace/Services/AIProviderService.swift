import Darwin
import Foundation

struct AIProviderCompletionRequest: Sendable {
    var prompt: String
    var systemPrompt: String?
    var workingDirectory: String?
}

struct HeadlessToolInvocation: Sendable {
    var arguments: [String]
    var stdin: String?
    var environmentOverrides: [String: String] = [:]
    var injectGeminiPlaceholderKey = false
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
        case let .processFailure(message):
            return message
        case let .requestFailure(message):
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

struct AIProviderSelectionService: Sendable {
    func preferredMemoryExtractionProvider(in settings: AppAISettings, tool: String?) -> AppAIProviderConfiguration? {
        if settings.memory.defaultExtractorProviderID == AppMemorySettings.automaticExtractorProviderID {
            return settings.preferredExtractionProvider(forTool: tool)
        }

        if let selected = settings.provider(withID: settings.memory.defaultExtractorProviderID),
           selected.isEnabled,
           selected.useForMemoryExtraction {
            return selected
        }
        return settings.preferredExtractionProvider(forTool: tool)
    }
}

struct AIProviderFactory: Sendable {
    let credentialStore: AICredentialStore

    func client(for kind: AppAIProviderKind) -> AIProviderClient {
        switch kind {
        case .claude:
            return HeadlessToolProviderClient(
                binaryName: "claude",
                argumentBuilder: { request, configuration in
                    var args = ["--print"]
                    if !configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        args.append(contentsOf: ["--model", configuration.model])
                    }
                    if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
                        args.append(contentsOf: ["--append-system-prompt", systemPrompt])
                    }
                    args.append(request.prompt)
                    return HeadlessToolInvocation(arguments: args, stdin: nil)
                }
            )
        case .codex:
            return HeadlessToolProviderClient(
                binaryName: "codex",
                argumentBuilder: { request, configuration in
                    var args = [
                        "exec",
                        "--skip-git-repo-check",
                        "--dangerously-bypass-approvals-and-sandbox",
                        "--ephemeral",
                        "--color", "never",
                    ]
                    if let workingDirectory = normalizedNonEmptyString(request.workingDirectory) {
                        args.append(contentsOf: ["--cd", workingDirectory])
                    }
                    if !configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        args.append(contentsOf: ["--model", configuration.model])
                    }
                    let prompt = mergedPrompt(request: request)
                    args.append("-")
                    return HeadlessToolInvocation(arguments: args, stdin: prompt)
                }
            )
        case .gemini:
            return HeadlessToolProviderClient(
                binaryName: "gemini",
                argumentBuilder: { request, configuration in
                    var args = ["--prompt", mergedPrompt(request: request), "--output-format", "text"]
                    if !configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        args.append(contentsOf: ["--model", configuration.model])
                    }
                    return HeadlessToolInvocation(
                        arguments: args,
                        stdin: nil,
                        environmentOverrides: [:],
                        injectGeminiPlaceholderKey: true
                    )
                }
            )
        case .opencode:
            return HeadlessToolProviderClient(
                binaryName: "opencode",
                argumentBuilder: { request, configuration in
                    var args = ["run", "--format", "default"]
                    if !configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        args.append(contentsOf: ["--model", configuration.model])
                    }
                    if let workingDirectory = normalizedNonEmptyString(request.workingDirectory) {
                        args.append(contentsOf: ["--dir", workingDirectory])
                    }
                    args.append(mergedPrompt(request: request))
                    return HeadlessToolInvocation(arguments: args, stdin: nil)
                }
            )
        case .openAICompatible:
            return OpenAICompatibleProviderClient(credentialStore: credentialStore)
        }
    }

    private func mergedPrompt(request: AIProviderCompletionRequest) -> String {
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            return """
            <system>
            \(systemPrompt)
            </system>

            \(request.prompt)
            """
        }
        return request.prompt
    }

}

private struct HeadlessToolProviderClient: AIProviderClient {
    private static let timeoutSeconds: TimeInterval = 90

    let binaryName: String
    let argumentBuilder: @Sendable (AIProviderCompletionRequest, AppAIProviderConfiguration) -> HeadlessToolInvocation

    func complete(
        _ request: AIProviderCompletionRequest,
        configuration: AppAIProviderConfiguration
    ) async throws -> String {
        try await Task.detached(priority: .utility) {
            let process = Process()
            let stdoutPipe = Pipe()
            let stderrPipe = Pipe()
            let stdinPipe = Pipe()
            var invocation = argumentBuilder(request, configuration)
            let outputFileURL: URL?
            if binaryName == "codex" {
                let fileManager = FileManager.default
                let directoryURL = fileManager.temporaryDirectory
                    .appendingPathComponent("codux-memory-provider", isDirectory: true)
                try? fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
                let fileURL = directoryURL.appendingPathComponent("codex-output-\(UUID().uuidString).txt", isDirectory: false)
                invocation.arguments.append(contentsOf: ["--output-last-message", fileURL.path])
                outputFileURL = fileURL
            } else {
                outputFileURL = nil
            }
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = [binaryName] + invocation.arguments
            var environment = AIToolEnvironmentService().mergedEnvironment(
                includeBundledWrappers: false,
                includeGeminiPlaceholder: invocation.injectGeminiPlaceholderKey
            )
            for (key, value) in invocation.environmentOverrides where environment[key]?.isEmpty ?? true {
                environment[key] = value
            }
            process.environment = environment
            if let workingDirectory = normalizedNonEmptyString(request.workingDirectory) {
                process.currentDirectoryURL = URL(fileURLWithPath: workingDirectory, isDirectory: true)
            }
            process.standardOutput = stdoutPipe
            process.standardError = stderrPipe
            if invocation.stdin != nil {
                process.standardInput = stdinPipe
            }
            try process.run()
            if let stdin = invocation.stdin {
                try Self.writeStdin(stdin, to: stdinPipe, binaryName: binaryName)
            }
            let stdoutHandle = stdoutPipe.fileHandleForReading
            let stderrHandle = stderrPipe.fileHandleForReading
            let stdoutTask = Task.detached(priority: .utility) {
                stdoutHandle.readDataToEndOfFile()
            }
            let stderrTask = Task.detached(priority: .utility) {
                stderrHandle.readDataToEndOfFile()
            }
            let timeoutState = LockedBool(false)
            let timeoutWorkItem = DispatchWorkItem {
                guard process.isRunning else {
                    return
                }
                timeoutState.set(true)
                process.terminate()
            }
            DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + Self.timeoutSeconds, execute: timeoutWorkItem)
            process.waitUntilExit()
            timeoutWorkItem.cancel()

            let stdoutData = await stdoutTask.value
            let stderrData = await stderrTask.value
            let stdoutPreview = outputPreview(stdoutData)
            let stderrPreview = outputPreview(stderrData)
            if timeoutState.value {
                let suffix = stderrPreview.isEmpty && stdoutPreview.isEmpty
                    ? ""
                    : " stderr=\(stderrPreview) stdout=\(stdoutPreview)"
                throw AIProviderError.processFailure("\(binaryName) timed out after \(Int(Self.timeoutSeconds)) seconds.\(suffix)")
            }
            guard process.terminationStatus == 0 else {
                throw AIProviderError.processFailure(
                    processFailureMessage(
                        binaryName: binaryName,
                        terminationStatus: process.terminationStatus,
                        stderrPreview: stderrPreview,
                        stdoutPreview: stdoutPreview
                    )
                )
            }

            let outputData: Data
            if let outputFileURL,
               let fileData = try? Data(contentsOf: outputFileURL),
               !fileData.isEmpty {
                outputData = fileData
                try? FileManager.default.removeItem(at: outputFileURL)
            } else {
                outputData = stdoutData
            }
            let output = String(data: outputData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !output.isEmpty else {
                throw AIProviderError.emptyResponse
            }
            return output
        }.value
    }

    private func outputPreview(_ data: Data) -> String {
        let output = String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard output.count > 600 else {
            return output
        }
        return "\(output.prefix(600))..."
    }

    private func processFailureMessage(
        binaryName: String,
        terminationStatus: Int32,
        stderrPreview: String,
        stdoutPreview: String
    ) -> String {
        if stderrPreview.contains("env: \(binaryName): No such file or directory") {
            return "\(displayName(for: binaryName)) CLI was not found in the application environment PATH."
        }
        if stderrPreview.contains("No such file or directory") && stderrPreview.contains(binaryName) {
            return "\(displayName(for: binaryName)) CLI was not found in the application environment PATH."
        }
        if stderrPreview.isEmpty {
            return "\(displayName(for: binaryName)) exited with code \(terminationStatus). stdout=\(stdoutPreview)"
        }
        return stderrPreview
    }

    private func displayName(for binaryName: String) -> String {
        switch binaryName {
        case "claude":
            return "Claude"
        case "codex":
            return "Codex"
        case "gemini":
            return "Gemini"
        case "opencode":
            return "OpenCode"
        default:
            return binaryName
        }
    }

    private static func writeStdin(_ stdin: String, to pipe: Pipe, binaryName: String) throws {
        let data = Data(stdin.utf8)
        let fileDescriptor = pipe.fileHandleForWriting.fileDescriptor
        defer {
            try? pipe.fileHandleForWriting.close()
        }

        try data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress else {
                return
            }
            var offset = 0
            while offset < data.count {
                let written = Darwin.write(fileDescriptor, baseAddress.advanced(by: offset), data.count - offset)
                if written > 0 {
                    offset += written
                    continue
                }
                if written == 0 || errno == EPIPE {
                    throw AIProviderError.processFailure("\(binaryName) closed stdin before reading input.")
                }
                if errno == EINTR {
                    continue
                }
                throw AIProviderError.processFailure("Failed to write stdin to \(binaryName): \(String(cString: strerror(errno))).")
            }
        }
    }
}

private final class LockedBool: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: Bool

    init(_ value: Bool) {
        storage = value
    }

    var value: Bool {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }

    func set(_ value: Bool) {
        lock.lock()
        storage = value
        lock.unlock()
    }
}

private struct OpenAICompatibleProviderClient: AIProviderClient {
    let credentialStore: AICredentialStore

    func complete(
        _ request: AIProviderCompletionRequest,
        configuration: AppAIProviderConfiguration
    ) async throws -> String {
        guard let apiKey = credentialStore.apiKey(for: configuration.apiKeyReference) else {
            throw AIProviderError.missingAPIKey
        }
        let baseURLString = configuration.baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: baseURLString.isEmpty ? "https://api.openai.com/v1/chat/completions" : normalizedEndpointURL(from: baseURLString)) else {
            throw AIProviderError.invalidBaseURL
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
        urlRequest.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        let payload = OpenAIChatCompletionRequest(
            model: configuration.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? AppAIProviderKind.openAICompatible.defaultModel : configuration.model,
            messages: makeMessages(for: request)
        )
        urlRequest.httpBody = try JSONEncoder().encode(payload)

        let (data, response) = try await URLSession.shared.data(for: urlRequest)
        if let httpResponse = response as? HTTPURLResponse,
           !(200..<300).contains(httpResponse.statusCode) {
            let body = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            throw AIProviderError.requestFailure(body.isEmpty ? "Provider returned HTTP \(httpResponse.statusCode)." : body)
        }

        let decoded = try JSONDecoder().decode(OpenAIChatCompletionResponse.self, from: data)
        guard let content = decoded.choices.first?.message.content?.trimmingCharacters(in: .whitespacesAndNewlines),
              !content.isEmpty else {
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

    private func makeMessages(for request: AIProviderCompletionRequest) -> [OpenAIChatCompletionMessage] {
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
