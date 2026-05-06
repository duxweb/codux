import XCTest

@testable import DmuxWorkspace

final class MemoryCoordinatorTests: XCTestCase {
    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!

    override func setUpWithError() throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(
                "dmux-memory-coordinator-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: temporaryDirectoryURL, withIntermediateDirectories: true)
        databaseURL = temporaryDirectoryURL.appendingPathComponent(
            "memory.sqlite3", isDirectory: false)
    }

    override func tearDownWithError() throws {
        if let temporaryDirectoryURL {
            try? FileManager.default.removeItem(at: temporaryDirectoryURL)
        }
        temporaryDirectoryURL = nil
        databaseURL = nil
    }

    func testCurrentStatusSnapshotReflectsUnderlyingQueueState() async throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let coordinator = MemoryCoordinator(store: store)
        let projectID = UUID()

        let initial = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(initial.status, .idle)
        XCTAssertEqual(initial.pendingCount, 0)
        XCTAssertEqual(initial.runningCount, 0)
        XCTAssertNil(initial.lastError)

        XCTAssertTrue(
            try store.enqueueExtractionIfNeeded(
                projectID: projectID,
                tool: "codex",
                sessionID: "session-1",
                transcriptPath: "/tmp/transcript.jsonl",
                sourceFingerprint: "fp-1"
            )
        )

        let queued = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(queued.status, .queued)
        XCTAssertEqual(queued.pendingCount, 1)
        XCTAssertEqual(queued.runningCount, 0)
        XCTAssertNil(queued.lastError)

        let task = try XCTUnwrap(store.nextPendingExtractionTask())
        try store.markExtractionTaskRunning(task.id)

        let processing = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(processing.status, .processing)
        XCTAssertEqual(processing.pendingCount, 0)
        XCTAssertEqual(processing.runningCount, 1)
        XCTAssertNil(processing.lastError)

        try store.markExtractionTaskFailed(task.id, error: "provider unavailable")

        let failed = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(failed.status, .failed)
        XCTAssertEqual(failed.pendingCount, 0)
        XCTAssertEqual(failed.runningCount, 0)
        XCTAssertEqual(failed.lastError, "provider unavailable")
    }

    func testRecoverInterruptedExtractionsRequeuesRunningTasks() async throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let coordinator = MemoryCoordinator(store: store)

        XCTAssertTrue(
            try store.enqueueExtractionIfNeeded(
                projectID: UUID(),
                tool: "codex",
                sessionID: "session-2",
                transcriptPath: "/tmp/transcript-2.jsonl",
                sourceFingerprint: "fp-2"
            )
        )

        let task = try XCTUnwrap(store.nextPendingExtractionTask())
        try store.markExtractionTaskRunning(task.id)

        let running = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(running.status, .processing)
        XCTAssertEqual(running.runningCount, 1)

        await coordinator.recoverInterruptedExtractions()

        let recovered = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(recovered.status, .queued)
        XCTAssertEqual(recovered.pendingCount, 1)
        XCTAssertEqual(recovered.runningCount, 0)
        XCTAssertNil(recovered.lastError)
    }

    func testProviderConfigurationRefreshClearsUnavailableProviderFailure() async throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let coordinator = MemoryCoordinator(store: store)
        let projectID = UUID()

        XCTAssertTrue(
            try store.enqueueExtractionIfNeeded(
                projectID: projectID,
                tool: "codex",
                sessionID: "session-provider-refresh",
                transcriptPath: "/tmp/provider-refresh.jsonl",
                sourceFingerprint: "fp-provider-refresh"
            )
        )

        let task = try XCTUnwrap(store.nextPendingExtractionTask())
        try store.markExtractionTaskRunning(task.id)
        try store.markExtractionTaskFailed(
            task.id,
            error: AIProviderError.unavailableProvider.localizedDescription
        )

        let failed = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(failed.status, .failed)

        await coordinator.refreshAfterProviderConfigurationChanged(
            settings: AppAISettings(),
            projects: []
        )

        let refreshed = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(refreshed.status, .idle)
        XCTAssertEqual(refreshed.pendingCount, 0)
        XCTAssertEqual(refreshed.runningCount, 0)
        XCTAssertNil(refreshed.lastError)
    }

    func testManualExtractionBypassesAutomaticToggleAndIdleDelay() async throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let coordinator = MemoryCoordinator(store: store)
        let projectID = UUID()
        let projectURL = temporaryDirectoryURL.appendingPathComponent("project", isDirectory: true)
        try FileManager.default.createDirectory(at: projectURL, withIntermediateDirectories: true)
        let transcriptURL = temporaryDirectoryURL.appendingPathComponent("manual-transcript.jsonl")
        try """
        {"role":"user","content":"Remember that this project keeps memory extraction manual when needed."}
        {"role":"assistant","content":"Implementation completed."}
        """.write(to: transcriptURL, atomically: true, encoding: .utf8)
        let session = AISessionStore.TerminalSessionState(
            terminalID: UUID(),
            projectID: projectID,
            projectName: "Memory Project",
            projectPath: projectURL.path,
            sessionTitle: "Codex",
            tool: "codex",
            aiSessionID: "manual-session",
            state: .idle,
            updatedAt: Date().timeIntervalSince1970,
            wasInterrupted: false,
            hasCompletedTurn: true,
            transcriptPath: transcriptURL.path
        )
        var settings = AppAISettings()
        settings.providers = []
        settings.memory.automaticExtractionEnabled = false
        settings.memory.extractionIdleDelaySeconds = 3600
        let project = Project(
            id: projectID,
            name: "Memory Project",
            path: projectURL.path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )

        await coordinator.handleSessionSnapshots(
            [session],
            settings: settings,
            projects: [project]
        )

        let automaticSnapshot = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(automaticSnapshot.status, .idle)

        await coordinator.handleSessionSnapshots(
            [session],
            settings: settings,
            projects: [project],
            mode: .manual
        )

        let manualSnapshot = await coordinator.currentStatusSnapshot()
        XCTAssertEqual(manualSnapshot.status, .failed)
        XCTAssertEqual(manualSnapshot.lastError, AIProviderError.unavailableProvider.localizedDescription)
    }

    func testDefaultProviderSelectionUsesLocalLlamaForMemoryExtraction() throws {
        let service = AIProviderSelectionService()
        let settings = AppAISettings()

        let providers = service.candidateMemoryExtractionProviders(in: settings, tool: "codex")

        XCTAssertEqual(providers.map(\.id), [AppAIProviderConfiguration.localLlamaProviderID])
        XCTAssertEqual(providers.first?.kind, .localLlama)
        XCTAssertEqual(providers.first?.model, LocalLlamaModelCatalog.defaultModelID)
        XCTAssertEqual(
            service.preferredPetSpeechProvider(in: settings)?.id,
            AppAIProviderConfiguration.localLlamaProviderID
        )
    }

    func testAutomaticProviderSelectionUsesMemoryProvidersByPriority() throws {
        let service = AIProviderSelectionService()
        var settings = AppAISettings()
        settings.providers = [
            AppAIProviderConfiguration(
                id: AppAIProviderConfiguration.localLlamaProviderID,
                kind: .localLlama,
                displayName: "Llama Model",
                priority: 0
            ),
            AppAIProviderConfiguration.customAPIChannel(
                kind: .openAICompatible,
                priority: 2,
                displayName: "OpenAI"
            ),
            AppAIProviderConfiguration.customAPIChannel(
                kind: .anthropic,
                priority: 1,
                displayName: "Claude API"
            ),
        ]

        let providers = service.candidateMemoryExtractionProviders(in: settings, tool: "codex")

        XCTAssertEqual(
            providers.map(\.displayName),
            ["Llama Model", "Claude API", "OpenAI"])
    }

    func testExplicitProviderSelectionDoesNotFallbackToOtherProviders() throws {
        let service = AIProviderSelectionService()
        var settings = AppAISettings()
        let preferred = AppAIProviderConfiguration.customAPIChannel(
            kind: .openAICompatible,
            priority: 1,
            displayName: "Preferred API"
        )
        settings.providers = [
            preferred,
            AppAIProviderConfiguration.customAPIChannel(
                kind: .anthropic,
                priority: 0,
                displayName: "Fallback API"
            ),
        ]
        settings.memory.defaultExtractorProviderID = preferred.id

        let providers = service.candidateMemoryExtractionProviders(in: settings, tool: "codex")

        XCTAssertEqual(providers.map(\.id), [preferred.id])
    }

    func testSettingsMigrationRemovesLegacyCliExtractionProviders() throws {
        let data = Data(
            """
            {
              "memory": {
                "defaultExtractorProviderID": "builtin-claude"
              },
              "providers": [
                {
                  "id": "builtin-claude",
                  "kind": "claude",
                  "displayName": "Claude",
                  "isEnabled": true,
                  "model": "",
                  "baseURL": "",
                  "apiKeyReference": null,
                  "useForMemoryExtraction": true,
                  "priority": 0
                },
                {
                  "id": "api-openai",
                  "kind": "openAICompatible",
                  "displayName": "API",
                  "isEnabled": true,
                  "model": "gpt-4.1-mini",
                  "baseURL": "https://api.openai.com/v1",
                  "apiKeyReference": null,
                  "useForMemoryExtraction": true,
                  "priority": 1
                }
              ]
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppAISettings.self, from: data)

        XCTAssertFalse(settings.providers.contains { $0.id == "builtin-claude" })
        XCTAssertTrue(settings.providers.contains { $0.kind == .localLlama })
        XCTAssertTrue(settings.providers.contains { $0.kind == .openAICompatible })
        XCTAssertEqual(
            settings.memory.defaultExtractorProviderID,
            AppMemorySettings.automaticExtractorProviderID
        )
    }

    func testSettingsMigrationRemovesLegacyBuiltInAPIChannel() throws {
        let data = Data(
            """
            {
              "memory": {
                "defaultExtractorProviderID": "custom-openai-compatible"
              },
              "providers": [
                {
                  "id": "custom-openai-compatible",
                  "kind": "openAICompatible",
                  "displayName": "OpenAI-Compatible API",
                  "isEnabled": false,
                  "model": "gpt-4.1-mini",
                  "baseURL": "https://api.openai.com/v1",
                  "apiKeyReference": null,
                  "useForMemoryExtraction": false,
                  "priority": 0
                },
                {
                  "id": "api-openai-compatible-test",
                  "kind": "openAICompatible",
                  "displayName": "OpenAI API",
                  "isEnabled": true,
                  "model": "gpt-4.1-mini",
                  "baseURL": "https://api.openai.com/v1",
                  "apiKeyReference": null,
                  "useForMemoryExtraction": true,
                  "priority": 1
                }
              ]
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppAISettings.self, from: data)

        XCTAssertFalse(settings.providers.contains { $0.id == "custom-openai-compatible" })
        XCTAssertEqual(
            settings.providers.map(\.id),
            [AppAIProviderConfiguration.localLlamaProviderID, "api-openai-compatible-test"]
        )
        XCTAssertEqual(
            settings.memory.defaultExtractorProviderID,
            AppMemorySettings.automaticExtractorProviderID
        )
    }

    func testExtractionResponseDecoderAcceptsMarkdownFencedJSON() throws {
        let candidates = MemoryExtractionResponseDecoder.jsonObjectCandidates(
            from: """
                Here is the memory update:

                ```json
                {
                  "user_summary": "",
                  "project_summary": "Use wiki-style memory layers.",
                  "working_add": [],
                  "working_archive": [],
                  "merged_entry_ids": []
                }
                ```
                """
        )

        XCTAssertEqual(candidates.count, 1)
        XCTAssertTrue(candidates[0].contains("\"project_summary\""))
    }

    func testExtractionResponseDecoderFindsBalancedJSONInsidePromptEcho() throws {
        let candidates = MemoryExtractionResponseDecoder.jsonObjectCandidates(
            from: """
                OpenAI Codex
                --------
                user
                Treat this as a deterministic memory compaction job.
                This sentence has braces that are not JSON: {not-json}
                {
                  "user_summary": "",
                  "project_summary": "",
                  "working_add": [
                    {
                      "scope": "project",
                      "kind": "bug_lesson",
                      "content": "Parser tolerates braces like {value} inside JSON strings.",
                      "rationale": "CLI output can include prompt echoes"
                    }
                  ],
                  "working_archive": [],
                  "merged_entry_ids": []
                }
                trailing text
                """
        )

        XCTAssertTrue(
            candidates.contains { candidate in
                candidate.contains("\"working_add\"")
                    && candidate.contains(
                        "Parser tolerates braces like {value} inside JSON strings.")
            })
    }

    func testExtractionResponseDecodeAcceptsModelSchemaAliases() throws {
        let raw = """
            {
              "userSummary": "",
              "projectSummary": "Keep terminal resize work off the hot drag path.",
              "memories": [
                {
                  "target": "repo",
                  "stability": "stable",
                  "category": "fix pattern",
                  "text": "Coalesce terminal geometry updates during split dragging.",
                  "reason": "Small local models may emit alias keys."
                },
                {
                  "scope": "global",
                  "tier": "recent",
                  "type": "style",
                  "memory": "Answer from repo evidence."
                }
              ],
              "archive_ids": ["\(UUID().uuidString)"],
              "merged_ids": ["\(UUID().uuidString)"]
            }
            """

        let response = try JSONDecoder().decode(
            MemoryExtractionResponse.self,
            from: Data(raw.utf8)
        )

        XCTAssertEqual(response.projectSummary, "Keep terminal resize work off the hot drag path.")
        XCTAssertEqual(response.workingAdd.count, 2)
        XCTAssertEqual(response.workingAdd[0].scope, .project)
        XCTAssertEqual(response.workingAdd[0].tier, .core)
        XCTAssertEqual(response.workingAdd[0].kind, .bugLesson)
        XCTAssertEqual(response.workingAdd[1].scope, .user)
        XCTAssertEqual(response.workingAdd[1].tier, .working)
        XCTAssertEqual(response.workingAdd[1].kind, .preference)
        XCTAssertEqual(response.workingArchive.count, 1)
        XCTAssertEqual(response.mergedEntryIDs.count, 1)
    }

    func testExtractionResponseDecodeTreatsUnknownKindAsFact() throws {
        let raw = """
            {
              "working_add": [
                {"scope":"project","tier":"working","kind":"path-note","content":"Use runtime.log for dev diagnostics."}
              ]
            }
            """

        let response = try JSONDecoder().decode(
            MemoryExtractionResponse.self,
            from: Data(raw.utf8)
        )

        XCTAssertEqual(response.workingAdd.count, 1)
        XCTAssertEqual(response.workingAdd[0].kind, .fact)
        XCTAssertEqual(response.workingAdd[0].content, "Use runtime.log for dev diagnostics.")
    }

    func testMemorySettingsDefaultsFavorCompactExtractionAndInjection() throws {
        let settings = AppMemorySettings()

        XCTAssertEqual(settings.maxInjectedUserWorkingMemories, 4)
        XCTAssertEqual(settings.maxInjectedProjectWorkingMemories, 6)
        XCTAssertEqual(settings.summaryTargetTokenBudget, 900)
        XCTAssertEqual(settings.maxInjectedSummaryTokens, 900)
        XCTAssertEqual(settings.extractionIdleDelaySeconds, 120)
        XCTAssertEqual(settings.sessionExtractionCooldownSeconds, 900)
        XCTAssertEqual(settings.maxExtractionTranscriptLines, 80)
        XCTAssertEqual(settings.maxExtractionTranscriptTokens, 8000)
    }

    func testMemorySettingsMigratesOldNoisyDefaultsToCompactDefaults() throws {
        let data = Data(
            """
            {
              "maxInjectedUserWorkingMemories": 8,
              "maxInjectedProjectWorkingMemories": 12,
              "summaryTargetTokenBudget": 1800
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppMemorySettings.self, from: data)

        XCTAssertEqual(settings.maxInjectedUserWorkingMemories, 4)
        XCTAssertEqual(settings.maxInjectedProjectWorkingMemories, 6)
        XCTAssertEqual(settings.summaryTargetTokenBudget, 900)
    }

    func testMemorySettingsPreservesUserCustomizedBudgets() throws {
        let data = Data(
            """
            {
              "maxInjectedUserWorkingMemories": 3,
              "maxInjectedProjectWorkingMemories": 5,
              "summaryTargetTokenBudget": 1200
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppMemorySettings.self, from: data)

        XCTAssertEqual(settings.maxInjectedUserWorkingMemories, 3)
        XCTAssertEqual(settings.maxInjectedProjectWorkingMemories, 5)
        XCTAssertEqual(settings.summaryTargetTokenBudget, 1200)
    }

    func testSettingsMigrationDropsAppleFoundationModelsProvider() throws {
        let data = Data(
            """
            {
              "providers": [
                {
                  "id": "local-apple-foundation-models",
                  "kind": "appleFoundationModels",
                  "displayName": "Apple Intelligence",
                  "isEnabled": true,
                  "model": "system-language-model",
                  "baseURL": "",
                  "apiKey": "",
                  "useForMemoryExtraction": true,
                  "priority": 0
                }
              ]
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppAISettings.self, from: data)

        XCTAssertFalse(settings.providers.contains { $0.id == "local-apple-foundation-models" })
        XCTAssertEqual(settings.providers.map(\.id), [AppAIProviderConfiguration.localLlamaProviderID])
        XCTAssertEqual(settings.providers.first?.kind, .localLlama)
    }

    func testSettingsMigrationRenamesLegacyLocalLlamaDisplayName() throws {
        let data = Data(
            """
            {
              "providers": [
                {
                  "id": "local-llama-memory",
                  "kind": "localLlama",
                  "displayName": "Local Llama Memory",
                  "isEnabled": true,
                  "model": "\(LocalLlamaModelCatalog.defaultModelID)",
                  "baseURL": "",
                  "apiKey": "",
                  "useForMemoryExtraction": true,
                  "priority": 0
                }
              ]
            }
            """.utf8
        )

        let settings = try JSONDecoder().decode(AppAISettings.self, from: data)

        XCTAssertEqual(settings.providers.first?.displayName, AppAIProviderKind.localLlama.defaultDisplayName)
        XCTAssertEqual(settings.providers.first?.localizedDisplayName, AppAIProviderKind.localLlama.defaultDisplayName)
    }
}
