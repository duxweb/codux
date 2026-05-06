import Foundation

#if canImport(llama)
@preconcurrency import llama
#endif

struct LocalLlamaProviderClient: AIProviderClient {
    func complete(
        _ request: AIProviderCompletionRequest,
        configuration: AppAIProviderConfiguration
    ) async throws -> String {
        guard let descriptor = LocalLlamaModelCatalog.descriptor(for: configuration) else {
            throw AIProviderError.requestFailure("No local llama model is configured.")
        }
        let modelURL = try LocalLlamaModelStore().installedModelURL(for: descriptor)

        #if canImport(llama)
        let runtimeConfig = descriptor.recommendedConfig["memory"]
        return try await LocalLlamaCompletionRuntime.shared.complete(
            request,
            modelURL: modelURL,
            contextTokens: Int32(runtimeConfig?.contextTokens ?? descriptor.contextLength),
            maxPredictionTokens: Int32(runtimeConfig?.maxPredictionTokens ?? 768),
            chatTemplate: descriptor.chatTemplate
        )
        #else
        _ = request
        _ = modelURL
        throw AIProviderError.unavailableProvider
        #endif
    }
}

#if canImport(llama)
private actor LocalLlamaCompletionRuntime {
    static let shared = LocalLlamaCompletionRuntime()

    private var didInitializeBackend = false

    func complete(
        _ request: AIProviderCompletionRequest,
        modelURL: URL,
        contextTokens: Int32,
        maxPredictionTokens: Int32,
        chatTemplate: String
    ) async throws -> String {
        if didInitializeBackend == false {
            llama_backend_init()
            didInitializeBackend = true
        }

        let formattedPrompt = LocalLlamaPromptFormatter.format(request, chatTemplate: chatTemplate)
        return try LocalLlamaSession(
            modelURL: modelURL,
            contextTokens: contextTokens,
            maxPredictionTokens: maxPredictionTokens
        )
        .generate(formattedPrompt)
    }
}

private struct LocalLlamaSession {
    private let modelURL: URL
    private let contextTokens: Int32
    private let maxPredictionTokens: Int32
    private let promptBatchSize: Int32 = 256

    init(modelURL: URL, contextTokens: Int32, maxPredictionTokens: Int32) {
        self.modelURL = modelURL
        self.contextTokens = contextTokens
        self.maxPredictionTokens = maxPredictionTokens
    }

    func generate(_ prompt: String) throws -> String {
        var modelParams = llama_model_default_params()
        modelParams.n_gpu_layers = 99
        modelParams.use_mmap = true

        let model = modelURL.path.withCString { path in
            llama_model_load_from_file(path, modelParams)
        }
        guard let model else {
            throw AIProviderError.requestFailure("Failed to load local llama model.")
        }
        defer { llama_model_free(model) }

        var contextParams = llama_context_default_params()
        contextParams.n_ctx = UInt32(contextTokens)
        contextParams.n_batch = UInt32(promptBatchSize)
        contextParams.n_ubatch = UInt32(promptBatchSize)
        contextParams.n_threads = defaultThreadCount()
        contextParams.n_threads_batch = defaultThreadCount()
        contextParams.no_perf = true

        guard let context = llama_init_from_model(model, contextParams) else {
            throw AIProviderError.requestFailure("Failed to initialize local llama context.")
        }
        defer { llama_free(context) }

        guard let vocab = llama_model_get_vocab(model) else {
            throw AIProviderError.requestFailure("Failed to read local llama vocabulary.")
        }

        let promptTokens = try tokenize(prompt, vocab: vocab)
        guard promptTokens.count + Int(maxPredictionTokens) <= Int(contextTokens) else {
            throw AIProviderError.requestFailure(
                "Local model prompt exceeds the configured context window.")
        }

        try decode(promptTokens, context: context)

        var samplerParams = llama_sampler_chain_default_params()
        samplerParams.no_perf = true
        guard let sampler = llama_sampler_chain_init(samplerParams) else {
            throw AIProviderError.requestFailure("Failed to initialize local llama sampler.")
        }
        defer { llama_sampler_free(sampler) }
        llama_sampler_chain_add(sampler, llama_sampler_init_greedy())

        var outputData = Data()
        for _ in 0..<maxPredictionTokens {
            let token = llama_sampler_sample(sampler, context, -1)
            if llama_vocab_is_eog(vocab, token) {
                break
            }

            llama_sampler_accept(sampler, token)
            outputData.append(try pieceData(for: token, vocab: vocab))

            var nextToken = token
            let decodeResult = withUnsafeMutablePointer(to: &nextToken) { tokenPointer in
                let batch = llama_batch_get_one(tokenPointer, 1)
                return llama_decode(context, batch)
            }
            guard decodeResult == 0 else {
                throw AIProviderError.requestFailure(
                    "Local llama generation failed with code \(decodeResult).")
            }

            if let partial = String(data: outputData, encoding: .utf8),
               containsStopSequence(partial) {
                break
            }
        }

        guard let output = String(data: outputData, encoding: .utf8) else {
            throw AIProviderError.emptyResponse
        }
        let cleaned = cleanOutput(output)
        guard !cleaned.isEmpty else {
            throw AIProviderError.emptyResponse
        }
        return cleaned
    }

    private func tokenize(_ text: String, vocab: OpaquePointer) throws -> [llama_token] {
        let byteCount = Int32(text.utf8.count)
        var tokens = [llama_token](repeating: 0, count: max(16, Int(byteCount) + 8))
        let count = text.withCString { rawText in
            tokens.withUnsafeMutableBufferPointer { buffer in
                llama_tokenize(
                    vocab,
                    rawText,
                    byteCount,
                    buffer.baseAddress,
                    Int32(buffer.count),
                    true,
                    true
                )
            }
        }

        if count >= 0 {
            return Array(tokens.prefix(Int(count)))
        }

        tokens = [llama_token](repeating: 0, count: Int(-count))
        let retryCount = text.withCString { rawText in
            tokens.withUnsafeMutableBufferPointer { buffer in
                llama_tokenize(
                    vocab,
                    rawText,
                    byteCount,
                    buffer.baseAddress,
                    Int32(buffer.count),
                    true,
                    true
                )
            }
        }
        guard retryCount >= 0 else {
            throw AIProviderError.requestFailure("Local llama tokenization failed.")
        }
        return Array(tokens.prefix(Int(retryCount)))
    }

    private func decode(_ tokens: [llama_token], context: OpaquePointer) throws {
        var offset = 0
        while offset < tokens.count {
            let end = min(tokens.count, offset + Int(promptBatchSize))
            var chunk = Array(tokens[offset..<end])
            let result = chunk.withUnsafeMutableBufferPointer { buffer in
                let batch = llama_batch_get_one(buffer.baseAddress, Int32(buffer.count))
                return llama_decode(context, batch)
            }
            guard result == 0 else {
                throw AIProviderError.requestFailure(
                    "Local llama prompt evaluation failed with code \(result).")
            }
            offset = end
        }
    }

    private func pieceData(for token: llama_token, vocab: OpaquePointer) throws -> Data {
        var buffer = [CChar](repeating: 0, count: 128)
        let count = buffer.withUnsafeMutableBufferPointer { rawBuffer in
            llama_token_to_piece(vocab, token, rawBuffer.baseAddress, Int32(rawBuffer.count), 0, false)
        }

        if count >= 0 {
            return Data(bytes: buffer, count: Int(count))
        }

        buffer = [CChar](repeating: 0, count: Int(-count))
        let retryCount = buffer.withUnsafeMutableBufferPointer { rawBuffer in
            llama_token_to_piece(vocab, token, rawBuffer.baseAddress, Int32(rawBuffer.count), 0, false)
        }
        guard retryCount >= 0 else {
            throw AIProviderError.requestFailure("Local llama detokenization failed.")
        }
        return Data(bytes: buffer, count: Int(retryCount))
    }

    private func defaultThreadCount() -> Int32 {
        let activeCount = ProcessInfo.processInfo.activeProcessorCount
        return Int32(max(2, min(4, max(1, activeCount / 3))))
    }

    private func containsStopSequence(_ text: String) -> Bool {
        ["<|im_end|>", "<end_of_turn>", "<|endoftext|>", "</s>", "<|eot_id|>", "<|end|>", "<｜end▁of▁sentence｜>"].contains {
            text.contains($0)
        }
    }

    private func cleanOutput(_ text: String) -> String {
        var value = text
        for stopSequence in [
            "<|im_end|>",
            "<end_of_turn>",
            "<|endoftext|>",
            "</s>",
            "<|eot_id|>",
            "<|end|>",
            "<｜end▁of▁sentence｜>",
        ] {
            if let range = value.range(of: stopSequence) {
                value = String(value[..<range.lowerBound])
            }
        }
        return strippedThinkBlocks(from: value).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func strippedThinkBlocks(from text: String) -> String {
        var value = text
        while let start = value.range(of: "<think>") {
            guard let end = value.range(of: "</think>", range: start.upperBound..<value.endIndex) else {
                value.removeSubrange(start.lowerBound..<value.endIndex)
                break
            }
            value.removeSubrange(start.lowerBound..<end.upperBound)
        }
        return value
    }
}

private enum LocalLlamaPromptFormatter {
    static func format(_ request: AIProviderCompletionRequest, chatTemplate: String) -> String {
        switch chatTemplate.lowercased() {
        case "deepseek-r1", "deepseek":
            return deepSeekR1Format(request)
        case "gemma", "gemma3":
            return gemmaFormat(request)
        case "llama3", "llama-3", "llama":
            return llama3Format(request)
        case "mistral", "mistral-small":
            return mistralSmallFormat(request)
        case "phi3", "phi-3":
            return phi3Format(request)
        case "phi4", "phi-4":
            return phi4Format(request)
        case "qwen", "qwen2", "qwen2.5", "qwen3":
            return qwenFormat(request)
        default:
            return qwenFormat(request)
        }
    }

    private static func qwenFormat(_ request: AIProviderCompletionRequest) -> String {
        var pieces: [String] = []
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            pieces.append(
                """
                <|im_start|>system
                \(systemPrompt)<|im_end|>
                """
            )
        }
        pieces.append(
            """
            <|im_start|>user
            \(request.prompt)<|im_end|>
            <|im_start|>assistant
            """
        )
        return pieces.joined(separator: "\n")
    }

    private static func gemmaFormat(_ request: AIProviderCompletionRequest) -> String {
        let prompt: String
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            prompt = "\(systemPrompt)\n\n\(request.prompt)"
        } else {
            prompt = request.prompt
        }

        return """
            <start_of_turn>user
            \(prompt)<end_of_turn>
            <start_of_turn>model
            """
    }

    private static func deepSeekR1Format(_ request: AIProviderCompletionRequest) -> String {
        var prompt = "<｜begin▁of▁sentence｜>"
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            prompt += systemPrompt
        }
        prompt += "<｜User｜>\(request.prompt)<｜Assistant｜><think>\n"
        return prompt
    }

    private static func llama3Format(_ request: AIProviderCompletionRequest) -> String {
        var pieces = ["<|begin_of_text|>"]
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            pieces.append(
                """
                <|start_header_id|>system<|end_header_id|>

                \(systemPrompt)<|eot_id|>
                """
            )
        }
        pieces.append(
            """
            <|start_header_id|>user<|end_header_id|>

            \(request.prompt)<|eot_id|><|start_header_id|>assistant<|end_header_id|>

            """
        )
        return pieces.joined(separator: "")
    }

    private static func mistralSmallFormat(_ request: AIProviderCompletionRequest) -> String {
        let systemPrompt = normalizedNonEmptyString(request.systemPrompt)
            ?? "You are a helpful assistant."
        return "<s>[SYSTEM_PROMPT]\(systemPrompt)[/SYSTEM_PROMPT][INST]\(request.prompt)[/INST]"
    }

    private static func phi3Format(_ request: AIProviderCompletionRequest) -> String {
        var pieces: [String] = []
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            pieces.append(
                """
                <|system|>
                \(systemPrompt)<|end|>
                """
            )
        }
        pieces.append(
            """
            <|user|>
            \(request.prompt)<|end|>
            <|assistant|>
            """
        )
        return pieces.joined(separator: "\n")
    }

    private static func phi4Format(_ request: AIProviderCompletionRequest) -> String {
        var pieces: [String] = []
        if let systemPrompt = normalizedNonEmptyString(request.systemPrompt) {
            pieces.append("<|im_start|>system<|im_sep|>\(systemPrompt)<|im_end|>")
        }
        pieces.append("<|im_start|>user<|im_sep|>\(request.prompt)<|im_end|><|im_start|>assistant<|im_sep|>")
        return pieces.joined()
    }
}
#endif
