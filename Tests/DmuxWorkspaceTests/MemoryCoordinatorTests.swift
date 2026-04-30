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

    func testAutomaticProviderSelectionUsesAPIProvidersOnlyByPriority() throws {
        let service = AIProviderSelectionService()
        var settings = AppAISettings()
        settings.providers = [
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
            ["Claude API", "OpenAI"])
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
        XCTAssertEqual(settings.providers.map(\.id), ["api-openai-compatible-test"])
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
}
