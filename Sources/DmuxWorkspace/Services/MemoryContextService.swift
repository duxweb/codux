import Foundation

struct MemoryLaunchArtifacts: Sendable {
    var workspaceRootURL: URL
    var workspaceLinkURL: URL
    var promptFileURL: URL
    var indexFileURL: URL
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
        let shouldInjectMemory =
            settings.memory.enabled && settings.memory.automaticInjectionEnabled
        guard globalPrompt != nil || shouldInjectMemory,
            let runtimeRoot = runtimeSupportRootURL
                ?? AppRuntimePaths.runtimeSupportRootURL(fileManager: fileManager)
        else {
            return nil
        }

        let rootURL =
            runtimeRoot
            .appendingPathComponent("memory-workspaces", isDirectory: true)
            .appendingPathComponent(projectID.uuidString, isDirectory: true)
        let workspaceLinkURL = rootURL.appendingPathComponent("workspace", isDirectory: true)
        let promptFileURL = rootURL.appendingPathComponent("memory-prompt.txt", isDirectory: false)
        let indexFileURL = rootURL.appendingPathComponent("MEMORY.md", isDirectory: false)

        let claudeContext = collectMemoryContext(
            projectID: projectID,
            projectName: projectName,
            tool: "claude",
            settings: settings
        )
        let codexContext = collectMemoryContext(
            projectID: projectID,
            projectName: projectName,
            tool: "codex",
            settings: settings
        )
        let geminiContext = collectMemoryContext(
            projectID: projectID,
            projectName: projectName,
            tool: "gemini",
            settings: settings
        )
        let memoryContext = mergedMemoryContext([claudeContext, codexContext, geminiContext])

        let promptText = renderPromptText(context: claudeContext, rootURL: rootURL)
        let indexText = renderIndexText(context: memoryContext, rootURL: rootURL)
        let userMemoryText = renderUserMemoryText(context: memoryContext)
        let projectMemoryText = renderProjectMemoryText(context: memoryContext)
        let recentMemoryText = renderRecentMemoryText(context: memoryContext)
        let searchGuideText = renderSearchGuideText(context: memoryContext)
        let claudeText = renderToolLaunchText(
            projectID: projectID,
            projectName: projectName,
            tool: "claude",
            rootURL: rootURL,
            context: claudeContext
        )
        let agentsText = renderToolLaunchText(
            projectID: projectID,
            projectName: projectName,
            tool: "codex",
            rootURL: rootURL,
            context: codexContext
        )
        let geminiText = renderToolLaunchText(
            projectID: projectID,
            projectName: projectName,
            tool: "gemini",
            rootURL: rootURL,
            context: geminiContext
        )

        guard
            !promptText.isEmpty || !indexText.isEmpty || !claudeText.isEmpty || !agentsText.isEmpty
                || !geminiText.isEmpty
        else {
            return nil
        }

        do {
            try fileManager.createDirectory(at: rootURL, withIntermediateDirectories: true)
            if fileManager.fileExists(atPath: workspaceLinkURL.path) {
                try fileManager.removeItem(at: workspaceLinkURL)
            }
            try fileManager.createSymbolicLink(
                at: workspaceLinkURL,
                withDestinationURL: URL(fileURLWithPath: projectPath, isDirectory: true))
            try promptText.write(to: promptFileURL, atomically: true, encoding: .utf8)
            try indexText.write(to: indexFileURL, atomically: true, encoding: .utf8)
            try userMemoryText.write(
                to: rootURL.appendingPathComponent("memory-user.md"), atomically: true,
                encoding: .utf8)
            try projectMemoryText.write(
                to: rootURL.appendingPathComponent("memory-project.md"), atomically: true,
                encoding: .utf8)
            try recentMemoryText.write(
                to: rootURL.appendingPathComponent("memory-recent.md"), atomically: true,
                encoding: .utf8)
            try searchGuideText.write(
                to: rootURL.appendingPathComponent("memory-search.md"), atomically: true,
                encoding: .utf8)
            try claudeText.write(
                to: rootURL.appendingPathComponent("CLAUDE.md"), atomically: true, encoding: .utf8)
            try agentsText.write(
                to: rootURL.appendingPathComponent("AGENTS.md"), atomically: true, encoding: .utf8)
            try geminiText.write(
                to: rootURL.appendingPathComponent("GEMINI.md"), atomically: true, encoding: .utf8)
            return MemoryLaunchArtifacts(
                workspaceRootURL: rootURL,
                workspaceLinkURL: workspaceLinkURL,
                promptFileURL: promptFileURL,
                indexFileURL: indexFileURL
            )
        } catch {
            return nil
        }
    }

    private func renderToolLaunchText(
        projectID: UUID,
        projectName: String,
        tool: String,
        rootURL: URL,
        context: MemoryContextPayload
    ) -> String {
        let prompt = renderIndexText(context: context, rootURL: rootURL)
        guard !prompt.isEmpty else {
            return ""
        }
        return """
            Launch context for \(documentToolName(tool)).
            Start with MEMORY.md, then open topic files only when relevant to the task.
            Prefer current repository state over stale memory.

            \(prompt)
            """
    }

    private func collectMemoryContext(
        projectID: UUID,
        projectName: String,
        tool: String,
        settings: AppAISettings
    ) -> MemoryContextPayload {
        let shouldInjectMemory =
            settings.memory.enabled && settings.memory.automaticInjectionEnabled
        let globalPrompt = normalizedNonEmptyString(settings.globalPrompt)

        let userSummary =
            shouldInjectMemory && settings.memory.allowCrossProjectUserRecall
            ? try? store.currentSummary(scope: .user)
            : nil
        let projectSummary =
            shouldInjectMemory
            ? try? store.currentSummary(scope: .project, projectID: projectID) : nil
        let userWorking =
            shouldInjectMemory && settings.memory.allowCrossProjectUserRecall
            ? ((try? store.listEntries(
                scope: .user, toolID: tool, tiers: [.working],
                limit: settings.memory.maxInjectedUserWorkingMemories)) ?? [])
            : []
        let projectWorking =
            shouldInjectMemory
            ? ((try? store.listEntries(
                scope: .project, projectID: projectID, toolID: tool, tiers: [.working],
                limit: settings.memory.maxInjectedProjectWorkingMemories)) ?? [])
            : []

        let userCoreFallback: [MemoryEntry]
        if shouldInjectMemory, userSummary == nil, settings.memory.allowCrossProjectUserRecall {
            userCoreFallback =
                (try? store.listEntries(scope: .user, toolID: tool, tiers: [.core], limit: 4)) ?? []
        } else {
            userCoreFallback = []
        }

        let projectCoreFallback: [MemoryEntry]
        if shouldInjectMemory, projectSummary == nil {
            projectCoreFallback =
                (try? store.listEntries(
                    scope: .project, projectID: projectID, toolID: tool, tiers: [.core], limit: 6))
                ?? []
        } else {
            projectCoreFallback = []
        }

        let uniqueEntries = uniqueOrderedEntries(
            userCoreFallback + userWorking + projectCoreFallback + projectWorking)
        try? store.bumpAccess(for: uniqueEntries.map(\.id))

        return MemoryContextPayload(
            projectName: projectName,
            globalPrompt: globalPrompt,
            userSummary: normalizedNonEmptyString(userSummary?.content),
            projectSummary: normalizedNonEmptyString(projectSummary?.content),
            userCoreFallback: uniqueOrderedEntries(userCoreFallback),
            projectCoreFallback: uniqueOrderedEntries(projectCoreFallback),
            userWorking: uniqueOrderedEntries(userWorking),
            projectWorking: uniqueOrderedEntries(projectWorking),
            userWorkingLimit: settings.memory.maxInjectedUserWorkingMemories,
            projectWorkingLimit: settings.memory.maxInjectedProjectWorkingMemories,
            memoryEnabled: shouldInjectMemory
        )
    }

    private func renderPromptText(context: MemoryContextPayload, rootURL: URL) -> String {
        renderIndexText(context: context, rootURL: rootURL)
    }

    private func renderIndexText(context: MemoryContextPayload, rootURL: URL) -> String {
        var sections: [String] = []
        if let globalPrompt = context.globalPrompt {
            sections.append(
                renderSummarySection(title: "Global instructions", content: globalPrompt))
        }
        guard context.hasMemory else {
            return sections.joined(separator: "\n\n")
        }
        sections.append(
            """
            # MEMORY.md

            Project context: \(context.projectName)
            Apply relevant memory as guidance, not as source of truth.
            Prefer current repository state and user instructions over stale memory.

            ## Load order
            1. Use this index first.
            2. Open topic files only when they are relevant to the current task.
            3. Full transcripts are not injected; use memory search only when history is needed.

            ## Topic files
            - `memory-user.md`: cross-project user preferences and habits.
            - `memory-project.md`: project-specific decisions, conventions, and facts.
            - `memory-recent.md`: fresh working notes from recent sessions.
            - `memory-search.md`: search-only memory guidance and current injection limits.

            Memory workspace: \(rootURL.path)
            Project workspace symlink: `workspace/`
            """)
        if let userSummary = context.userSummary {
            sections.append(renderSummarySection(title: "User summary", content: userSummary))
        } else if !context.userCoreFallback.isEmpty {
            sections.append(
                renderIndexEntryList(title: "User notes index", entries: context.userCoreFallback))
        }
        if let projectSummary = context.projectSummary {
            sections.append(renderSummarySection(title: "Project summary", content: projectSummary))
        } else if !context.projectCoreFallback.isEmpty {
            sections.append(
                renderIndexEntryList(
                    title: "Project notes index", entries: context.projectCoreFallback))
        }
        if !context.userWorking.isEmpty || !context.projectWorking.isEmpty {
            sections.append(
                """
                [Recent notes index]
                - User working notes: \(context.userWorking.count)
                - Project working notes: \(context.projectWorking.count)
                """)
        }
        return trimIndexLines(sections.joined(separator: "\n\n"), maxLines: 200)
    }

    private func renderUserMemoryText(context: MemoryContextPayload) -> String {
        var sections = [
            "# User Memory\n\nUse this only when cross-project user preferences matter."
        ]
        if let userSummary = context.userSummary {
            sections.append(renderSummarySection(title: "User summary", content: userSummary))
        }
        if !context.userCoreFallback.isEmpty {
            sections.append(
                renderSection(title: "User core notes", entries: context.userCoreFallback))
        }
        if !context.userWorking.isEmpty {
            sections.append(renderSection(title: "Recent user notes", entries: context.userWorking))
        }
        return sections.joined(separator: "\n\n")
    }

    private func renderProjectMemoryText(context: MemoryContextPayload) -> String {
        var sections = [
            "# Project Memory\n\nUse this only when project-specific decisions, conventions, or facts matter."
        ]
        if let projectSummary = context.projectSummary {
            sections.append(renderSummarySection(title: "Project summary", content: projectSummary))
        }
        if !context.projectCoreFallback.isEmpty {
            sections.append(
                renderSection(title: "Project core notes", entries: context.projectCoreFallback))
        }
        if !context.projectWorking.isEmpty {
            sections.append(
                renderSection(title: "Recent project notes", entries: context.projectWorking))
        }
        return sections.joined(separator: "\n\n")
    }

    private func renderRecentMemoryText(context: MemoryContextPayload) -> String {
        var sections = [
            "# Recent Working Memory\n\nThese notes are short-lived and should not override current repository evidence."
        ]
        if !context.userWorking.isEmpty {
            sections.append(renderSection(title: "Recent user notes", entries: context.userWorking))
        }
        if !context.projectWorking.isEmpty {
            sections.append(
                renderSection(title: "Recent project notes", entries: context.projectWorking))
        }
        return sections.joined(separator: "\n\n")
    }

    private func renderSearchGuideText(context: MemoryContextPayload) -> String {
        """
        # Search-Only Memory

        Full historical transcripts are not loaded into launch context.
        Use current repository files first. Search memory only when prior decisions,
        previous debugging chains, or older project context are directly relevant.

        Current injected limits:
        - User working notes: \(context.userWorking.count)/\(context.userWorkingLimit)
        - Project working notes: \(context.projectWorking.count)/\(context.projectWorkingLimit)
        """
    }

    private func uniqueOrderedEntries(_ entries: [MemoryEntry]) -> [MemoryEntry] {
        var seen = Set<UUID>()
        return entries.filter { entry in
            seen.insert(entry.id).inserted
        }
    }

    private func mergedMemoryContext(_ contexts: [MemoryContextPayload]) -> MemoryContextPayload {
        guard var first = contexts.first else {
            return MemoryContextPayload(
                projectName: "",
                globalPrompt: nil,
                userSummary: nil,
                projectSummary: nil,
                userCoreFallback: [],
                projectCoreFallback: [],
                userWorking: [],
                projectWorking: [],
                userWorkingLimit: 0,
                projectWorkingLimit: 0,
                memoryEnabled: false
            )
        }
        first.userCoreFallback = uniqueOrderedEntries(contexts.flatMap(\.userCoreFallback))
        first.projectCoreFallback = uniqueOrderedEntries(contexts.flatMap(\.projectCoreFallback))
        first.userWorking = uniqueOrderedEntries(contexts.flatMap(\.userWorking))
        first.projectWorking = uniqueOrderedEntries(contexts.flatMap(\.projectWorking))
        first.memoryEnabled = contexts.contains { $0.memoryEnabled }
        return first
    }

    private func renderSection(title: String, entries: [MemoryEntry]) -> String {
        let lines = entries.map { entry in
            if let rationale = normalizedNonEmptyString(entry.rationale) {
                return "- \(entry.content) [\(entry.kind.rawValue); \(rationale)]"
            }
            return "- \(entry.content) [\(entry.kind.rawValue)]"
        }
        return """
            [\(title)]
            \(lines.joined(separator: "\n"))
            """
    }

    private func renderIndexEntryList(title: String, entries: [MemoryEntry]) -> String {
        let lines = entries.prefix(8).map { entry in
            "- \(entry.kind.rawValue): \(entry.content)"
        }
        return """
            [\(title)]
            \(lines.joined(separator: "\n"))
            """
    }

    private func renderSummarySection(title: String, content: String) -> String {
        """
        [\(title)]
        \(content)
        """
    }

    private func trimIndexLines(_ text: String, maxLines: Int) -> String {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count > maxLines else {
            return text
        }
        return lines.prefix(maxLines - 1).joined(separator: "\n") + "\n[Memory index truncated]"
    }

    private func documentToolName(_ tool: String) -> String {
        switch tool.lowercased() {
        case "codex":
            return "Codex"
        case "claude", "claude-code":
            return "Claude Code"
        case "gemini":
            return "Gemini"
        default:
            return tool
        }
    }
}

private struct MemoryContextPayload {
    var projectName: String
    var globalPrompt: String?
    var userSummary: String?
    var projectSummary: String?
    var userCoreFallback: [MemoryEntry]
    var projectCoreFallback: [MemoryEntry]
    var userWorking: [MemoryEntry]
    var projectWorking: [MemoryEntry]
    var userWorkingLimit: Int
    var projectWorkingLimit: Int
    var memoryEnabled: Bool

    var hasMemory: Bool {
        memoryEnabled
            && (userSummary != nil || projectSummary != nil || !userCoreFallback.isEmpty
                || !projectCoreFallback.isEmpty || !userWorking.isEmpty || !projectWorking.isEmpty)
    }
}
