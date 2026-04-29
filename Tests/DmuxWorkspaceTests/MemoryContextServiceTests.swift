import XCTest

@testable import DmuxWorkspace

final class MemoryContextServiceTests: XCTestCase {
    private var temporaryDirectoryURL: URL!
    private var databaseURL: URL!
    private var runtimeRootURL: URL!

    override func setUpWithError() throws {
        temporaryDirectoryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(
                "dmux-memory-context-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: temporaryDirectoryURL, withIntermediateDirectories: true)
        databaseURL = temporaryDirectoryURL.appendingPathComponent(
            "memory.sqlite3", isDirectory: false)
        runtimeRootURL = temporaryDirectoryURL.appendingPathComponent(
            "runtime-support", isDirectory: true)
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
        let claudeText = try String(
            contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("CLAUDE.md"))
        let agentsText = try String(
            contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("AGENTS.md"))
        let geminiText = try String(
            contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("GEMINI.md"))
        let promptText = try String(contentsOf: resolvedArtifacts.promptFileURL)
        let indexText = try String(contentsOf: resolvedArtifacts.indexFileURL)
        let userMemoryText = try String(
            contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent("memory-user.md"))
        let recentMemoryText = try String(
            contentsOf: resolvedArtifacts.workspaceRootURL.appendingPathComponent(
                "memory-recent.md"))

        XCTAssertTrue(claudeText.contains("Launch context for Claude Code."))
        XCTAssertTrue(claudeText.contains("Keep answers concise."))
        XCTAssertTrue(claudeText.contains("Recent notes index"))
        XCTAssertFalse(claudeText.contains("Claude should summarize implementation tradeoffs."))
        XCTAssertFalse(claudeText.contains("Codex should patch files with apply_patch only."))

        XCTAssertTrue(agentsText.contains("Launch context for Codex."))
        XCTAssertTrue(agentsText.contains("Keep answers concise."))
        XCTAssertTrue(agentsText.contains("Recent notes index"))
        XCTAssertFalse(agentsText.contains("Codex should patch files with apply_patch only."))
        XCTAssertFalse(agentsText.contains("Gemini should prefer policy-driven approvals."))

        XCTAssertTrue(geminiText.contains("Launch context for Gemini."))
        XCTAssertTrue(geminiText.contains("Keep answers concise."))
        XCTAssertTrue(geminiText.contains("Recent notes index"))
        XCTAssertFalse(geminiText.contains("Gemini should prefer policy-driven approvals."))
        XCTAssertFalse(geminiText.contains("Claude should summarize implementation tradeoffs."))

        XCTAssertTrue(promptText.contains("# MEMORY.md"))
        XCTAssertTrue(promptText.contains("Keep answers concise."))
        XCTAssertFalse(promptText.contains("Claude should summarize implementation tradeoffs."))
        XCTAssertTrue(indexText.contains("Project working notes: 3"))
        XCTAssertLessThanOrEqual(
            indexText.split(separator: "\n", omittingEmptySubsequences: false).count, 200)
        XCTAssertTrue(userMemoryText.contains("Keep answers concise."))
        XCTAssertTrue(
            recentMemoryText.contains("Claude should summarize implementation tradeoffs."))
        XCTAssertTrue(recentMemoryText.contains("Codex should patch files with apply_patch only."))
        XCTAssertTrue(recentMemoryText.contains("Gemini should prefer policy-driven approvals."))
        let linkedDestination = try FileManager.default.destinationOfSymbolicLink(
            atPath: resolvedArtifacts.workspaceLinkURL.path)
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
        let projectMemoryText = try String(
            contentsOf: artifacts.workspaceRootURL.appendingPathComponent("memory-project.md"))

        XCTAssertTrue(promptText.contains("User total memory: prefer concise Chinese responses."))
        XCTAssertTrue(
            promptText.contains("Project total memory: inject summary before working notes."))
        XCTAssertFalse(
            promptText.contains("Old core item should not be injected when summary exists."))
        XCTAssertFalse(promptText.contains("Recent working note 1."))
        XCTAssertFalse(promptText.contains("Recent working note 2."))
        let injectedWorkingCount = ["Recent working note 1.", "Recent working note 2."]
            .filter { projectMemoryText.contains($0) }
            .count
        XCTAssertEqual(injectedWorkingCount, 1)
    }

    func testPrepareLaunchArtifactsWritesWikiStyleLayeredFiles() throws {
        let store = MemoryStore(databaseURL: databaseURL)
        let projectID = UUID()
        _ = try store.upsertSummary(
            scope: .project,
            projectID: projectID,
            content: "Project summary for the index.",
            sourceEntryIDs: [],
            maxVersions: 3
        )
        _ = try store.upsert(
            MemoryCandidate(
                scope: .project,
                projectID: projectID,
                toolID: nil,
                tier: .working,
                kind: .bugLesson,
                content: "Search old transcript only when debugging repeated failures.",
                rationale: "Avoid loading transcript by default",
                sourceTool: "codex",
                sourceSessionID: "session-3",
                sourceFingerprint: "fp-3"
            )
        )

        let service = MemoryContextService(store: store, runtimeSupportRootURL: runtimeRootURL)
        let artifacts = try XCTUnwrap(
            service.prepareLaunchArtifacts(
                projectID: projectID,
                projectName: "Dmux",
                projectPath: temporaryDirectoryURL.path,
                settings: AppAISettings()
            )
        )
        let indexText = try String(
            contentsOf: artifacts.workspaceRootURL.appendingPathComponent("MEMORY.md"))
        let searchText = try String(
            contentsOf: artifacts.workspaceRootURL.appendingPathComponent("memory-search.md"))
        let promptText = try String(contentsOf: artifacts.promptFileURL)

        XCTAssertTrue(indexText.contains("## Load order"))
        XCTAssertTrue(indexText.contains("memory-user.md"))
        XCTAssertTrue(indexText.contains("memory-project.md"))
        XCTAssertTrue(indexText.contains("memory-recent.md"))
        XCTAssertTrue(indexText.contains("memory-search.md"))
        XCTAssertLessThanOrEqual(
            indexText.split(separator: "\n", omittingEmptySubsequences: false).count, 200)
        XCTAssertTrue(searchText.contains("Full historical transcripts are not loaded"))
        XCTAssertFalse(
            promptText.contains("Search old transcript only when debugging repeated failures."))
        XCTAssertTrue(
            try FileManager.default.destinationOfSymbolicLink(
                atPath: artifacts.workspaceLinkURL.path)
                == temporaryDirectoryURL.path)
    }
}
