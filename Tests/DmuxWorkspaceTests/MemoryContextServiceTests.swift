import XCTest
@testable import DmuxWorkspace

final class MemoryContextServiceTests: XCTestCase {
    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!
    private var runtimeRootURL: URL!

    override func setUpWithError() throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-memory-context-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: temporaryDirectoryURL, withIntermediateDirectories: true)
        databaseURL = temporaryDirectoryURL.appendingPathComponent("memory.sqlite3", isDirectory: false)
        runtimeRootURL = temporaryDirectoryURL.appendingPathComponent("runtime-support", isDirectory: true)
    }

    override func tearDownWithError() throws {
        if let temporaryDirectoryURL {
            try? FileManager.default.removeItem(at: temporaryDirectoryURL)
        }
        temporaryDirectoryURL = nil
        databaseURL = nil
        runtimeRootURL = nil
    }

    func testPrepareLaunchArtifactsRendersToolSpecificMemoryFiles() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()

        _ = try store.upsert(
            MemoryCandidate(
                scope: .user,
                projectID: nil,
                toolID: nil,
                tier: .core,
                kind: .preference,
                content: "Keep answers concise.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: "claude",
                tier: .working,
                kind: .decision,
                content: "Claude should summarize implementation tradeoffs.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: "codex",
                tier: .working,
                kind: .convention,
                content: "Codex should patch files with apply_patch only.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: "gemini",
                tier: .working,
                kind: .fact,
                content: "Gemini should prefer policy-driven approvals.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )

        let service = MemoryContextService(
            store: store,
            runtimeSupportRootURL: runtimeRootURL
        )
        let artifacts = service.prepareLaunchArtifacts(
            projectID: projectID,
            projectName: "Dmux",
            projectPath: temporaryDirectoryURL.path,
            settings: AppAISettings()
        )

        let resolvedArtifacts = try XCTUnwrap(artifacts)
        let claudeText = try String(contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("CLAUDE.md"))
        let agentsText = try String(contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("AGENTS.md"))
        let geminiText = try String(contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("GEMINI.md"))
        let promptText = try String(contentsOf: resolvedArtifacts.promptFileURL)

        XCTAssertTrue(claudeText.contains("Keep answers concise."))
        XCTAssertTrue(claudeText.contains("Claude should summarize implementation tradeoffs."))
        XCTAssertFalse(claudeText.contains("Codex should patch files with apply_patch only."))

        XCTAssertTrue(agentsText.contains("Keep answers concise."))
        XCTAssertTrue(agentsText.contains("Codex should patch files with apply_patch only."))
        XCTAssertFalse(agentsText.contains("Gemini should prefer policy-driven approvals."))

        XCTAssertTrue(geminiText.contains("Keep answers concise."))
        XCTAssertTrue(geminiText.contains("Gemini should prefer policy-driven approvals."))
        XCTAssertFalse(geminiText.contains("Claude should summarize implementation tradeoffs."))

        XCTAssertTrue(promptText.contains("Keep answers concise."))
        XCTAssertTrue(promptText.contains("Claude should summarize implementation tradeoffs."))
        let linkedDestination = try FileManager.default.destinationOfSymbolicLink(atPath: resolvedArtifacts.workspaceLinkURL.path)
        XCTAssertEqual(linkedDestination, temporaryDirectoryURL.path)
    }

    func testPrepareLaunchArtifactsPrefersSummaryAndLimitsRecentWorkingEntries() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()
        _ = try store.upsertSummary(
            scope: .user,
            content: "User total memory: prefer concise Chinese responses.",
            sourceEntryIDs: [],
            maxVersions: 3
        )
        _ = try store.upsertSummary(
            scope: .project,
            projectID: projectID,
            content: "Project total memory: inject summary before working notes.",
            sourceEntryIDs: [],
            maxVersions: 3
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .core,
                kind: .decision,
                content: "Old core item should not be injected when summary exists.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .working,
                kind: .fact,
                content: "Recent working note 1.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .working,
                kind: .fact,
                content: "Recent working note 2.",
                rationale: nil,
                sourceTool: nil,
                sourceSessionID: nil,
                sourceFingerprint: nil
            )
        )

        var settings = AppAISettings()
        settings.memory.maxInjectedProjectWorkingMemories = 1
        let service = MemoryContextService(store: store, runtimeSupportRootURL: runtimeRootURL)
        let artifacts = try XCTUnwrap(
            service.prepareLaunchArtifacts(
                projectID: projectID,
                projectName: "Dmux",
                projectPath: temporaryDirectoryURL.path,
                settings: settings
            )
        )
        let promptText = try String(contentsOf: artifacts.promptFileURL)

        XCTAssertTrue(promptText.contains("User total memory: prefer concise Chinese responses."))
        XCTAssertTrue(promptText.contains("Project total memory: inject summary before working notes."))
        XCTAssertFalse(promptText.contains("Old core item should not be injected when summary exists."))
        let injectedWorkingCount = ["Recent working note 1.", "Recent working note 2."]
            .filter { promptText.contains($0) }
            .count
        XCTAssertEqual(injectedWorkingCount, 1)
    }
}
