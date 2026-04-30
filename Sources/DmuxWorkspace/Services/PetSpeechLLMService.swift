import Foundation

struct PetSpeechAuditPrompt: Equatable, Sendable {
    var systemPrompt: String
    var userPrompt: String
}

actor PetSpeechLLMService {
    private let credentialStore: AICredentialStore
    private let providerSelection = AIProviderSelectionService()

    init(credentialStore: AICredentialStore) {
        self.credentialStore = credentialStore
    }

    func generateLine(
        event: PetSpeechEvent,
        mode: PetSpeechMode,
        settings: AppAISettings
    ) async -> String? {
        guard settings.pet.speechLLMEnabled,
              event.tier >= .rhythm,
              let provider = providerSelection.preferredPetSpeechProvider(in: settings) else {
            return nil
        }

        let prompt = Self.auditPrompt(event: event, mode: mode)
        let providerFactory = AIProviderFactory(credentialStore: credentialStore)
        do {
            let response = try await withThrowingTaskGroup(of: String.self) { group in
                group.addTask {
                    try await providerFactory.client(for: provider.kind)
                        .complete(
                            AIProviderCompletionRequest(
                                prompt: prompt.userPrompt,
                                systemPrompt: prompt.systemPrompt,
                                workingDirectory: WorkspacePaths.repositoryRoot().path
                            ),
                            configuration: provider
                        )
                }
                group.addTask {
                    try await Task.sleep(for: .seconds(3))
                    throw CancellationError()
                }
                guard let result = try await group.next() else {
                    throw AIProviderError.emptyResponse
                }
                group.cancelAll()
                return result
            }
            return Self.sanitizedLine(response)
        } catch {
            return nil
        }
    }

    static func auditPrompt(event: PetSpeechEvent, mode: PetSpeechMode) -> PetSpeechAuditPrompt {
        let payload = event.payload
        let resolvedMode = mode == .mixed ? .encourage : mode
        let petName = payload["petName"]?.isEmpty == false
            ? payload["petName"]!
            : petSpeechL("pet.speech.payload.pet_name", "Little One")
        let systemPrompt = String(
            format: petSpeechL(
                "pet.speech.llm.system_prompt_format",
                "You are a desktop pixel pet named %@. Personality: %@. Reply to the event metadata below with one line under 30 characters. Do not repeat the data. Be expressive. Do not explain. Output only the line."
            ),
            petName,
            modeDescriptor(resolvedMode)
        )
        let userPrompt = String(
            format: petSpeechL(
                "pet.speech.llm.user_prompt_format",
                "Event: %@\nTool: %@ / Model: %@\nTokens this turn: %@, Duration: %@ minutes\nProject: %@"
            ),
            event.kind.rawValue,
            payload["tool"] ?? petSpeechL("pet.speech.payload.tool", "you"),
            payload["model"] ?? "AI",
            payload["tokens"] ?? "0",
            payload["durationMin"] ?? "0",
            payload["project"] ?? petSpeechL("pet.speech.payload.project", "this task")
        )
        return PetSpeechAuditPrompt(systemPrompt: systemPrompt, userPrompt: userPrompt)
    }

    private static func modeDescriptor(_ mode: PetSpeechMode) -> String {
        switch mode {
        case .roast:
            return petSpeechL("pet.speech.llm.mode.roast", "sarcastic, argumentative, sharp but not cruel")
        case .encourage:
            return petSpeechL("pet.speech.llm.mode.encourage", "warm, specific, coach-like")
        case .flirty:
            return petSpeechL("pet.speech.llm.mode.flirty", "playful, witty, tasteful")
        case .chuunibyou:
            return petSpeechL("pet.speech.llm.mode.chuunibyou", "dramatic, self-mythologizing, fantasy-styled")
        case .off, .mixed:
            return petSpeechL("pet.speech.llm.mode.encourage", "warm, specific, coach-like")
        }
    }

    private static func sanitizedLine(_ text: String) -> String? {
        var line = text
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "\"'“”‘’"))
        guard !line.isEmpty else {
            return nil
        }
        if line.count > 36 {
            let endIndex = line.index(line.startIndex, offsetBy: 35)
            line = String(line[..<endIndex]) + "…"
        }
        return line
    }

}
