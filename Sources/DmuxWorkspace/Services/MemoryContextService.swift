import Foundation

struct MemoryLaunchArtifacts: Sendable {
    var workspaceRootURL: URL
    var workspaceLinkURL: URL
    var promptFileURL: URL
}

struct MemoryContextService: Sendable {
    private let store: MemoryStore
    private let runtimeSupportRootURL: URL?

    init(
        store: MemoryStore = MemoryStore(),
        runtimeSupportRootURL: URL? = nil
    ) {
        self.store = store
        self.runtimeSupportRootURL = runtimeSupportRootURL
    }

    func prepareLaunchArtifacts(
        projectID: UUID,
        projectName: String,
        projectPath: String,
        settings: AppAISettings
    ) -> MemoryLaunchArtifacts? {
        let fileManager = FileManager.default
        let globalPrompt = normalizedNonEmptyString(settings.globalPrompt)
        let shouldInjectMemory = settings.memory.enabled && settings.memory.automaticInjectionEnabled
        guard (globalPrompt != nil || shouldInjectMemory),
              let runtimeRoot = runtimeSupportRootURL ?? AppRuntimePaths.runtimeSupportRootURL(fileManager: fileManager) else {
            return nil
        }

        let promptText = renderPromptText(
            projectID: projectID,
            projectName: projectName,
            tool: "claude",
            settings: settings
        )
        let claudeText = renderDocumentText(
            projectID: projectID,
            projectName: projectName,
            tool: "claude",
            settings: settings
        )
        let agentsText = renderDocumentText(
            projectID: projectID,
            projectName: projectName,
            tool: "codex",
            settings: settings
        )
        let geminiText = renderDocumentText(
            projectID: projectID,
            projectName: projectName,
            tool: "gemini",
            settings: settings
        )

        guard !promptText.isEmpty || !claudeText.isEmpty || !agentsText.isEmpty || !geminiText.isEmpty else {
            return nil
        }

        let rootURL = runtimeRoot
            .appendingPathComponent("memory-workspaces", isDirectory: true)
            .appendingPathComponent(projectID.uuidString, isDirectory: true)
        let workspaceLinkURL = rootURL.appendingPathComponent("workspace", isDirectory: true)
        let promptFileURL = rootURL.appendingPathComponent("memory-prompt.txt", isDirectory: false)

        do {
            try fileManager.createDirectory(at: rootURL, withIntermediateDirectories: true)
            if fileManager.fileExists(atPath: workspaceLinkURL.path) {
                try fileManager.removeItem(at: workspaceLinkURL)
            }
            try fileManager.createSymbolicLink(at: workspaceLinkURL, withDestinationURL: URL(fileURLWithPath: projectPath, isDirectory: true))
            try promptText.write(to: promptFileURL, atomically: true, encoding: .utf8)
            try claudeText.write(to: rootURL.appendingPathComponent("CLAUDE.md"), atomically: true, encoding: .utf8)
            try agentsText.write(to: rootURL.appendingPathComponent("AGENTS.md"), atomically: true, encoding: .utf8)
            try geminiText.write(to: rootURL.appendingPathComponent("GEMINI.md"), atomically: true, encoding: .utf8)
            return MemoryLaunchArtifacts(
                workspaceRootURL: rootURL,
                workspaceLinkURL: workspaceLinkURL,
                promptFileURL: promptFileURL
            )
        } catch {
            return nil
        }
    }

    private func renderDocumentText(
        projectID: UUID,
        projectName: String,
        tool: String,
        settings: AppAISettings
    ) -> String {
        let prompt = renderPromptText(
            projectID: projectID,
            projectName: projectName,
            tool: tool,
            settings: settings
        )
        guard !prompt.isEmpty else {
            return ""
        }
        return """
        # Dmux AI Memory

        \(prompt)
        """
    }

    private func renderPromptText(
        projectID: UUID,
        projectName: String,
        tool: String,
        settings: AppAISettings
    ) -> String {
        let shouldInjectMemory = settings.memory.enabled && settings.memory.automaticInjectionEnabled
        var sections: [String] = []
        if let globalPrompt = normalizedNonEmptyString(settings.globalPrompt) {
            sections.append(renderSummarySection(title: "Global user prompt", content: globalPrompt))
        }
        guard shouldInjectMemory else {
            return sections.joined(separator: "\n\n")
        }

        let userSummary = settings.memory.allowCrossProjectUserRecall
            ? try? store.currentSummary(scope: .user)
            : nil
        let projectSummary = try? store.currentSummary(scope: .project, projectID: projectID)
        let userWorking = settings.memory.allowCrossProjectUserRecall
            ? ((try? store.listEntries(scope: .user, toolID: tool, tiers: [.working], limit: settings.memory.maxInjectedUserWorkingMemories)) ?? [])
            : []
        let projectWorking = (try? store.listEntries(scope: .project, projectID: projectID, toolID: tool, tiers: [.working], limit: settings.memory.maxInjectedProjectWorkingMemories)) ?? []

        let userCoreFallback: [MemoryEntry]
        if userSummary == nil, settings.memory.allowCrossProjectUserRecall {
            userCoreFallback = (try? store.listEntries(scope: .user, toolID: tool, tiers: [.core], limit: 4)) ?? []
        } else {
            userCoreFallback = []
        }

        let projectCoreFallback: [MemoryEntry]
        if projectSummary == nil {
            projectCoreFallback = (try? store.listEntries(scope: .project, projectID: projectID, toolID: tool, tiers: [.core], limit: 6)) ?? []
        } else {
            projectCoreFallback = []
        }

        let uniqueEntries = uniqueOrderedEntries(userCoreFallback + userWorking + projectCoreFallback + projectWorking)
        guard userSummary != nil || projectSummary != nil || !uniqueEntries.isEmpty else {
            return sections.joined(separator: "\n\n")
        }
        try? store.bumpAccess(for: uniqueEntries.map(\.id))

        let userWorkingUnique = uniqueOrderedEntries(userWorking)
        let projectWorkingUnique = uniqueOrderedEntries(projectWorking)

        sections.append("You are working inside project '\(projectName)'. Use the following persistent memory as operating guidance.")
        if let userSummary, let content = normalizedNonEmptyString(userSummary.content) {
            sections.append(renderSummarySection(title: "User summary memory", content: content))
        } else if !userCoreFallback.isEmpty {
            sections.append(renderSection(title: "User fallback memory", entries: uniqueOrderedEntries(userCoreFallback)))
        }
        if let projectSummary, let content = normalizedNonEmptyString(projectSummary.content) {
            sections.append(renderSummarySection(title: "Project summary memory", content: content))
        } else if !projectCoreFallback.isEmpty {
            sections.append(renderSection(title: "Project fallback memory", entries: uniqueOrderedEntries(projectCoreFallback)))
        }
        if !userWorkingUnique.isEmpty {
            sections.append(renderSection(title: "Recent user working memory", entries: userWorkingUnique))
        }
        if !projectWorkingUnique.isEmpty {
            sections.append(renderSection(title: "Recent project working memory", entries: projectWorkingUnique))
        }
        return sections.joined(separator: "\n\n")
    }

    private func uniqueOrderedEntries(_ entries: [MemoryEntry]) -> [MemoryEntry] {
        var seen = Set<UUID>()
        return entries.filter { entry in
            seen.insert(entry.id).inserted
        }
    }

    private func renderSection(title: String, entries: [MemoryEntry]) -> String {
        let lines = entries.map { entry in
            if let rationale = normalizedNonEmptyString(entry.rationale) {
                return "- [\(entry.kind.rawValue)] \(entry.content) (context: \(rationale))"
            }
            return "- [\(entry.kind.rawValue)] \(entry.content)"
        }
        return """
        ## \(title)
        \(lines.joined(separator: "\n"))
        """
    }

    private func renderSummarySection(title: String, content: String) -> String {
        """
        ## \(title)
        \(content)
        """
    }
}
