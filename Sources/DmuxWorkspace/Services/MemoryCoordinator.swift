import CryptoKit
import Foundation
import SQLite3

extension Notification.Name {
    static let dmuxMemoryExtractionStatusDidChange = Notification.Name(
        "dmux.memoryExtractionStatusDidChange")
}

actor MemoryCoordinator {
    private let store: MemoryStore
    private let credentialStore: AICredentialStore
    private let providerSelectionService = AIProviderSelectionService()
    private let debugLog = AppDebugLog.shared
    private var isProcessingQueue = false

    init(
        store: MemoryStore = MemoryStore(),
        credentialStore: AICredentialStore = AICredentialStore()
    ) {
        self.store = store
        self.credentialStore = credentialStore
    }

    func currentStatusSnapshot() async -> MemoryExtractionStatusSnapshot {
        statusSnapshot(fallback: .idle)
    }

    func recoverInterruptedExtractions() async {
        do {
            let count = try store.resetRunningExtractionTasks(
                reason: "Recovered after app restart before completion.")
            if count > 0 {
                debugLog.log("memory-extraction", "recovered running tasks count=\(count)")
            }
            publishStatus(count > 0 ? .queued : .idle)
        } catch {
            debugLog.log(
                "memory-extraction", "recover failed error=\(error.localizedDescription)",
                level: .error)
            publishStatus(.failed)
        }
    }

    func handleSessionSnapshots(
        _ sessions: [AISessionStore.TerminalSessionState],
        settings: AppAISettings,
        projects: [Project]
    ) async {
        guard settings.memory.enabled, settings.memory.automaticExtractionEnabled else {
            return
        }

        let projectByID = Dictionary(uniqueKeysWithValues: projects.map { ($0.id, $0) })
        for session in sessions {
            guard session.state == .idle,
                session.hasCompletedTurn,
                let project = projectByID[session.projectID],
                let source = resolveTranscriptSource(for: session, project: project)
            else {
                continue
            }
            do {
                let fingerprint = source.fingerprint
                let didEnqueue = try store.enqueueExtractionIfNeeded(
                    projectID: project.id,
                    tool: session.tool,
                    sessionID: normalizedNonEmptyString(session.aiSessionID)
                        ?? session.terminalID.uuidString,
                    transcriptPath: source.location,
                    sourceFingerprint: fingerprint
                )
                if didEnqueue {
                    debugLog.log(
                        "memory-extraction",
                        "queued project=\(project.name) projectID=\(project.id.uuidString) tool=\(session.tool) session=\(normalizedNonEmptyString(session.aiSessionID) ?? session.terminalID.uuidString) transcript=\(source.location)"
                    )
                    publishStatus(.queued)
                }
            } catch {
                continue
            }
        }
        await processQueueIfNeeded(settings: settings, projectsByID: projectByID)
    }

    private func processQueueIfNeeded(settings: AppAISettings, projectsByID: [UUID: Project]) async
    {
        guard !isProcessingQueue else {
            return
        }
        isProcessingQueue = true
        publishStatus(.queued)
        defer { isProcessingQueue = false }

        let providerFactory = AIProviderFactory(credentialStore: credentialStore)
        while true {
            let task: MemoryExtractionTask
            do {
                guard let nextTask = try nextExtractionTask() else {
                    break
                }
                task = nextTask
            } catch {
                break
            }

            do {
                try store.markExtractionTaskRunning(task.id)
                debugLog.log(
                    "memory-extraction",
                    "start task=\(task.id.uuidString) projectID=\(task.projectID.uuidString) tool=\(task.tool) session=\(task.sessionID) transcript=\(task.transcriptPath)"
                )
                publishStatus(.processing)
                guard let project = projectsByID[task.projectID] else {
                    try store.markExtractionTaskDone(task.id)
                    debugLog.log(
                        "memory-extraction",
                        "drop task=\(task.id.uuidString) reason=missing-project")
                    publishStatus(.idle)
                    continue
                }

                let extractionProviders =
                    providerSelectionService.candidateMemoryExtractionProviders(
                        in: settings, tool: task.tool)
                guard !extractionProviders.isEmpty else {
                    try store.markExtractionTaskFailed(
                        task.id, error: AIProviderError.unavailableProvider.localizedDescription)
                    debugLog.log(
                        "memory-extraction", "failed task=\(task.id.uuidString) reason=no-provider",
                        level: .error)
                    publishStatus(.failed)
                    continue
                }

                let transcript = try resolveTranscriptForTask(task, project: project)
                let userSummary = try? store.currentSummary(scope: .user)
                let projectSummary = try? store.currentSummary(
                    scope: .project, projectID: project.id)
                let existingUserMemories =
                    (try? store.listEntries(
                        scope: .user, tiers: [.working],
                        limit: settings.memory.maxInjectedUserWorkingMemories))
                    ?? []
                let existingProjectMemories =
                    (try? store.listEntries(
                        scope: .project, projectID: project.id, tiers: [.working],
                        limit: settings.memory.maxInjectedProjectWorkingMemories)) ?? []
                let prompt = makeExtractionPrompt(
                    transcript: transcript,
                    userSummary: userSummary,
                    projectSummary: projectSummary,
                    userMemories: existingUserMemories,
                    projectMemories: existingProjectMemories,
                    projectName: project.name,
                    settings: settings
                )
                let response = try await extractMemoryResponse(
                    prompt: prompt,
                    projectPath: project.path,
                    providers: extractionProviders,
                    providerFactory: providerFactory,
                    taskID: task.id
                )
                try apply(response: response, task: task, settings: settings)
                try store.markExtractionTaskDone(task.id)
                debugLog.log(
                    "memory-extraction",
                    "done task=\(task.id.uuidString) userSummary=\(normalizedNonEmptyString(response.userSummary) != nil) projectSummary=\(normalizedNonEmptyString(response.projectSummary) != nil) workingAdd=\(response.workingAdd.count)"
                )
                publishStatus(.idle)
            } catch {
                try? store.markExtractionTaskFailed(task.id, error: error.localizedDescription)
                debugLog.log(
                    "memory-extraction",
                    "failed task=\(task.id.uuidString) error=\(error.localizedDescription)",
                    level: .error)
                publishStatus(.failed)
            }
        }
        publishStatus(.idle)
    }

    private func extractMemoryResponse(
        prompt: String,
        projectPath: String,
        providers: [AppAIProviderConfiguration],
        providerFactory: AIProviderFactory,
        taskID: UUID
    ) async throws -> MemoryExtractionResponse {
        var failures: [String] = []
        for provider in providers {
            do {
                let responseText = try await providerFactory.client(for: provider.kind).complete(
                    AIProviderCompletionRequest(
                        prompt: prompt,
                        systemPrompt: extractionSystemPrompt(),
                        workingDirectory: projectPath
                    ),
                    configuration: provider
                )
                return try decodeExtractionResponse(from: responseText)
            } catch {
                let message = error.localizedDescription
                failures.append("\(provider.displayName): \(message)")
                debugLog.log(
                    "memory-extraction",
                    "provider failed task=\(taskID.uuidString) provider=\(provider.displayName) error=\(message)",
                    level: .error
                )
            }
        }

        throw AIProviderError.requestFailure(
            "All memory extraction providers failed. \(failures.joined(separator: " | "))")
    }

    private func publishStatus(_ fallback: MemoryExtractionStatus) {
        let snapshot = statusSnapshot(fallback: fallback)
        Task { @MainActor in
            NotificationCenter.default.post(
                name: .dmuxMemoryExtractionStatusDidChange,
                object: nil,
                userInfo: ["snapshot": snapshot]
            )
        }
    }

    private func nextExtractionTask() throws -> MemoryExtractionTask? {
        if let nextTask = try store.nextPendingExtractionTask() {
            return nextTask
        }
        guard let retryTask = try store.retryableFailedExtractionTask() else {
            return nil
        }
        try store.resetExtractionTaskForRetry(retryTask.id)
        debugLog.log(
            "memory-extraction",
            "retry task=\(retryTask.id.uuidString) attempts=\(retryTask.attempts) error=\(retryTask.error ?? "")"
        )
        return retryTask
    }

    private func statusSnapshot(fallback: MemoryExtractionStatus) -> MemoryExtractionStatusSnapshot
    {
        (try? store.extractionStatusSnapshot()) ?? fallbackStatusSnapshot(for: fallback)
    }

    private func fallbackStatusSnapshot(for status: MemoryExtractionStatus)
        -> MemoryExtractionStatusSnapshot
    {
        MemoryExtractionStatusSnapshot(
            status: status,
            pendingCount: status == .queued ? 1 : 0,
            runningCount: status == .processing ? 1 : 0,
            lastError: nil,
            updatedAt: Date()
        )
    }

    private func apply(
        response: MemoryExtractionResponse, task: MemoryExtractionTask, settings: AppAISettings
    ) throws {
        var newWorkingIDs: [UUID] = []
        for item in response.workingAdd {
            let scope = item.scope ?? .project
            let entry = try store.upsert(
                MemoryCandidate(
                    scope: scope,
                    projectID: scope == .project ? task.projectID : nil,
                    toolID: nil,
                    tier: .working,
                    kind: item.kind,
                    content: item.content,
                    rationale: item.rationale,
                    sourceTool: task.tool,
                    sourceSessionID: task.sessionID,
                    sourceFingerprint: task.sourceFingerprint
                )
            )
            newWorkingIDs.append(entry.id)
        }

        var mergedIDs = response.mergedEntryIDs.compactMap { UUID(uuidString: $0) }
        mergedIDs.append(contentsOf: newWorkingIDs)

        if let content = normalizedNonEmptyString(response.userSummary) {
            let summary = try store.upsertSummary(
                scope: .user,
                content: content,
                sourceEntryIDs: mergedIDs,
                maxVersions: settings.memory.maxSummaryVersions
            )
            try store.markEntriesMerged(mergedIDs, summaryID: summary.id)
        }

        if let content = normalizedNonEmptyString(response.projectSummary) {
            let summary = try store.upsertSummary(
                scope: .project,
                projectID: task.projectID,
                content: content,
                sourceEntryIDs: mergedIDs,
                maxVersions: settings.memory.maxSummaryVersions
            )
            try store.markEntriesMerged(mergedIDs, summaryID: summary.id)
        }

        let archiveIDs = response.workingArchive.compactMap { UUID(uuidString: $0) }
        try store.archiveEntries(archiveIDs)
        try store.trimWorkingEntries(
            scope: .user, maxActive: settings.memory.maxActiveWorkingEntries)
        try store.trimWorkingEntries(
            scope: .project, projectID: task.projectID,
            maxActive: settings.memory.maxActiveWorkingEntries
        )
    }

    private func extractionSystemPrompt() -> String {
        """
        You extract and compact durable software-engineering memory from AI coding sessions.

        Return JSON only.
        Do not include markdown fences.
        Do not call tools, request scans, browse files, or infer facts outside the provided transcript and existing memory.
        Treat this as a deterministic memory compaction job, not a chat response.
        """
    }

    private func makeExtractionPrompt(
        transcript: String,
        userSummary: MemorySummary?,
        projectSummary: MemorySummary?,
        userMemories: [MemoryEntry],
        projectMemories: [MemoryEntry],
        projectName: String,
        settings: AppAISettings
    ) -> String {
        """
        Memory extraction schema version: dmux-memory-v2

        Project: \(projectName)

        Existing user summary:
        \(renderExistingSummary(userSummary))

        Existing project summary:
        \(renderExistingSummary(projectSummary))

        Recent user working entries:
        \(renderExistingMemories(userMemories))

        Recent project working entries:
        \(renderExistingMemories(projectMemories))

        Transcript:
        <transcript>
        \(transcript)
        </transcript>

        Return JSON with this exact shape and no extra keys:
        {
          "user_summary": "merged durable user memory, or empty string to keep unchanged",
          "project_summary": "merged durable project memory, or empty string to keep unchanged",
          "working_add": [{"scope":"user|project","kind":"preference|convention|decision|fact|bug_lesson","content":"...","rationale":"..."}],
          "working_archive": ["uuid"],
          "merged_entry_ids": ["uuid"]
        }

        Stable extraction keywords and categories:
        - preference: explicit user preferences, communication style, review style, workflow style, tool choices, permission/confirmation preferences.
        - convention: stable coding standards, repository conventions, naming/path rules, testing/build commands, localization or documentation rules.
        - decision: accepted architectural or product decisions that should guide future implementation.
        - fact: durable repository facts discovered from the session, such as source-of-truth paths, runtime data locations, feature boundaries, or known command surfaces.
        - bug_lesson: reproducible bug cause, fix pattern, regression guard, or diagnostic chain that should prevent repeated debugging.

        Positive signals:
        - The user corrects behavior or states a durable rule, for example "以后都...", "不要...", "用...", "路径统一是...", "这个是固定的".
        - The session establishes a repo-specific source of truth, command, path, schema, lifecycle, or bug fix.
        - A decision is likely to matter after this session ends.

        Negative signals:
        - Do not store greetings, progress updates, temporary todo items, one-off command output, timestamps, broad explanations, or generic programming knowledge.
        - Do not store full transcript text or raw logs.
        - Do not store facts that are only true during this immediate run unless they describe a reusable bug lesson.
        - Do not invent preferences from assistant wording; user-stated rules and confirmed repo facts have priority.

        Compaction rules:
        - Merge old summary + useful transcript facts into a concise total summary; do not append a changelog.
        - user_summary contains only durable cross-project developer habits and preferences.
        - project_summary contains only durable repository-specific memory for this project.
        - working_add is only for fresh short-lived facts that may be useful in the next few sessions before compaction.
        - Do not mention or request file scans, shell commands, or external tools; use only the transcript and existing memory above.
        - Keep each summary under about \(settings.memory.summaryTargetTokenBudget) tokens.
        - If a summary should stay unchanged, return an empty string for that summary.

        Examples:
        - Input meaning: user says "README 里的日志路径统一改成 /Users/me/Library/Application Support/Codux". Output category: project fact/convention.
        - Input meaning: user asks "好了么" repeatedly during a long build. Output: no memory.
        - Input meaning: debugging proves loading renewal must use runtime poll time when responding snapshot timestamps are stale. Output category: project bug_lesson.
        - Input meaning: assistant says a generic Swift explanation without user confirmation. Output: no memory.
        """
    }

    private func renderExistingSummary(_ summary: MemorySummary?) -> String {
        guard let summary, let content = normalizedNonEmptyString(summary.content) else {
            return "(none)"
        }
        return "version=\(summary.version)\n\(content)"
    }

    private func renderExistingMemories(_ entries: [MemoryEntry]) -> String {
        guard !entries.isEmpty else {
            return "(none)"
        }
        return entries.map { entry in
            if let rationale = normalizedNonEmptyString(entry.rationale) {
                return
                    "- id=\(entry.id.uuidString) [\(entry.kind.rawValue)] \(entry.content) (context: \(rationale))"
            }
            return "- id=\(entry.id.uuidString) [\(entry.kind.rawValue)] \(entry.content)"
        }.joined(separator: "\n")
    }

    private func decodeExtractionResponse(from rawText: String) throws -> MemoryExtractionResponse {
        let decoder = JSONDecoder()
        var lastError: Error?
        for candidate in MemoryExtractionResponseDecoder.jsonObjectCandidates(from: rawText) {
            do {
                return try decoder.decode(MemoryExtractionResponse.self, from: Data(candidate.utf8))
            } catch {
                lastError = error
            }
        }
        if let lastError {
            throw lastError
        }
        throw AIProviderError.requestFailure(
            "Memory extraction provider did not return a valid JSON object.")
    }

    private func resolveTranscriptForTask(_ task: MemoryExtractionTask, project: Project) throws
        -> String
    {
        guard let tool = normalizedNonEmptyString(task.tool)?.lowercased() else {
            throw AIProviderError.requestFailure("Missing tool for transcript extraction.")
        }
        if FileManager.default.fileExists(atPath: task.transcriptPath) {
            if tool == "opencode", task.transcriptPath.hasSuffix(".db") {
                if let transcript = fetchOpenCodeTranscript(
                    projectPath: project.path, externalSessionID: task.sessionID,
                    databasePath: task.transcriptPath)
                {
                    return transcript
                }
            } else if let transcript = readTranscriptFile(at: task.transcriptPath) {
                return transcript
            }
        }

        switch tool {
        case "claude":
            if let sessionID = normalizedNonEmptyString(task.sessionID) {
                let path = AIRuntimeSourceLocator.claudeSessionLogURL(
                    projectPath: project.path, externalSessionID: sessionID
                ).path
                if let transcript = readTranscriptFile(at: path) {
                    return transcript
                }
            }
        case "codex":
            if let fileURL = AIRuntimeSourceLocator.codexRolloutPath(
                projectPath: project.path, externalSessionID: task.sessionID)
            {
                if let transcript = readTranscriptFile(at: fileURL.path) {
                    return transcript
                }
            }
        case "gemini":
            let files = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: project.path)
            if let matching = files.first(where: { $0.lastPathComponent.contains(task.sessionID) }),
                let transcript = readTranscriptFile(at: matching.path)
            {
                return transcript
            }
            if let transcript = files.first.flatMap({ readTranscriptFile(at: $0.path) }) {
                return transcript
            }
        case "opencode":
            if let transcript = fetchOpenCodeTranscript(
                projectPath: project.path,
                externalSessionID: task.sessionID,
                databasePath: AIRuntimeSourceLocator.opencodeDatabaseURL().path
            ) {
                return transcript
            }
        default:
            break
        }
        throw AIProviderError.requestFailure("Unable to resolve transcript for memory extraction.")
    }

    private func resolveTranscriptSource(
        for session: AISessionStore.TerminalSessionState,
        project: Project
    ) -> (location: String, fingerprint: String)? {
        let tool = normalizedNonEmptyString(session.tool)?.lowercased() ?? session.tool.lowercased()
        let sessionID =
            normalizedNonEmptyString(session.aiSessionID) ?? session.terminalID.uuidString

        if let transcriptPath = normalizedNonEmptyString(session.transcriptPath),
            let source = transcriptSourceIfReadable(
                path: transcriptPath, tool: tool, sessionID: sessionID)
        {
            return source
        }

        switch tool {
        case "claude":
            guard let aiSessionID = normalizedNonEmptyString(session.aiSessionID) else {
                return nil
            }
            let path = AIRuntimeSourceLocator.claudeSessionLogURL(
                projectPath: project.path, externalSessionID: aiSessionID
            ).path
            return transcriptSourceIfReadable(path: path, tool: tool, sessionID: aiSessionID)

        case "codex":
            guard let aiSessionID = normalizedNonEmptyString(session.aiSessionID),
                let fileURL = AIRuntimeSourceLocator.codexRolloutPath(
                    projectPath: project.path, externalSessionID: aiSessionID)
            else {
                return nil
            }
            return transcriptSourceIfReadable(
                path: fileURL.path, tool: tool, sessionID: aiSessionID)

        case "gemini":
            let files = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: project.path)
            if let aiSessionID = normalizedNonEmptyString(session.aiSessionID),
                let matching = files.first(where: { $0.lastPathComponent.contains(aiSessionID) })
            {
                return transcriptSourceIfReadable(
                    path: matching.path, tool: tool, sessionID: aiSessionID)
            }
            guard let latest = files.first else { return nil }
            return transcriptSourceIfReadable(path: latest.path, tool: tool, sessionID: sessionID)

        case "opencode":
            let databasePath = AIRuntimeSourceLocator.opencodeDatabaseURL().path
            return transcriptSourceIfReadable(
                path: databasePath, tool: tool, sessionID: sessionID, allowDatabase: true)

        default:
            return nil
        }
    }

    private func transcriptSourceIfReadable(
        path: String,
        tool: String,
        sessionID: String,
        allowDatabase: Bool = false
    ) -> (location: String, fingerprint: String)? {
        var isDirectory = ObjCBool(false)
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
            isDirectory.boolValue == false,
            let attributes = try? FileManager.default.attributesOfItem(atPath: path)
        else {
            return nil
        }
        if allowDatabase == false,
            readTranscriptFile(at: path) == nil
        {
            return nil
        }
        let size = (attributes[.size] as? NSNumber)?.intValue ?? 0
        guard size > 0 else { return nil }
        let modifiedAt = (attributes[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
        return (
            location: path,
            fingerprint: sha256("\(tool)|\(sessionID)|\(path)|\(size)|\(modifiedAt)")
        )
    }

    private func readTranscriptFile(at path: String) -> String? {
        guard let text = try? String(contentsOfFile: path, encoding: .utf8) else {
            return nil
        }
        let lines = text.components(separatedBy: .newlines).suffix(160)
        let trimmed = lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private func fetchOpenCodeTranscript(
        projectPath: String,
        externalSessionID: String,
        databasePath: String
    ) -> String? {
        var db: OpaquePointer?
        guard sqlite3_open(databasePath, &db) == SQLITE_OK,
            let db
        else {
            if db != nil {
                sqlite3_close(db)
            }
            return nil
        }
        defer { sqlite3_close(db) }

        let sql = """
            SELECT json_extract(m.data, '$.role') AS role,
                   COALESCE(json_extract(m.data, '$.time.created'), '') AS created_at,
                   COALESCE(json_extract(m.data, '$.content'), json_extract(p.data, '$.text'), json_extract(p.data, '$.state.output'), '') AS content,
                   COALESCE(json_extract(m.data, '$.path.root'), s.directory, '') AS root_path,
                   COALESCE(json_extract(p.data, '$.type'), '') AS part_type,
                   COALESCE(json_extract(p.data, '$.tool'), '') AS tool_name
            FROM session s
            JOIN message m ON m.session_id = s.id
            LEFT JOIN part p ON p.message_id = m.id
            WHERE s.id = ?
              AND s.time_archived IS NULL
            ORDER BY m.time_created ASC, p.time_created ASC;
            """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
            let statement
        else {
            return nil
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, externalSessionID, -1, SQLITE_TRANSIENT_SESSION)
        var lines: [String] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            let rootPath = sqlite3_column_text(statement, 3).map { String(cString: $0) }
            guard pathsEquivalent(rootPath, projectPath) else {
                continue
            }
            let role = sqlite3_column_text(statement, 0).map { String(cString: $0) } ?? "assistant"
            let createdAt = sqlite3_column_text(statement, 1).map { String(cString: $0) } ?? ""
            let content = sqlite3_column_text(statement, 2).map { String(cString: $0) } ?? ""
            let partType = sqlite3_column_text(statement, 4).map { String(cString: $0) } ?? ""
            let toolName = sqlite3_column_text(statement, 5).map { String(cString: $0) } ?? ""
            if !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let prefix =
                    partType == "tool" && toolName.isEmpty == false
                    ? "\(role).tool[\(toolName)]" : role
                lines.append("[\(createdAt)] \(prefix): \(content)")
            }
        }
        let text = lines.suffix(120).joined(separator: "\n").trimmingCharacters(
            in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    private func sha256(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

private struct MemoryExtractionResponse: Decodable {
    struct Item: Decodable {
        var scope: MemoryScope?
        var kind: MemoryKind
        var content: String
        var rationale: String?
    }

    var userSummary: String?
    var projectSummary: String?
    var workingAdd: [Item]
    var workingArchive: [String]
    var mergedEntryIDs: [String]

    enum CodingKeys: String, CodingKey {
        case userSummary = "user_summary"
        case projectSummary = "project_summary"
        case workingAdd = "working_add"
        case workingArchive = "working_archive"
        case mergedEntryIDs = "merged_entry_ids"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        userSummary = try container.decodeIfPresent(String.self, forKey: .userSummary)
        projectSummary = try container.decodeIfPresent(String.self, forKey: .projectSummary)
        workingAdd = try container.decodeIfPresent([Item].self, forKey: .workingAdd) ?? []
        workingArchive = try container.decodeIfPresent([String].self, forKey: .workingArchive) ?? []
        mergedEntryIDs = try container.decodeIfPresent([String].self, forKey: .mergedEntryIDs) ?? []
    }
}

enum MemoryExtractionResponseDecoder {
    static func jsonObjectCandidates(from rawText: String) -> [String] {
        let trimmed = rawText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return []
        }

        var candidates: [String] = []
        if trimmed.hasPrefix("{") {
            candidates.append(trimmed)
        }
        for fenced in fencedCodeBlockBodies(from: trimmed) {
            candidates.append(contentsOf: balancedJSONObjects(in: fenced))
            let fencedTrimmed = fenced.trimmingCharacters(in: .whitespacesAndNewlines)
            if fencedTrimmed.hasPrefix("{") {
                candidates.append(fencedTrimmed)
            }
        }
        candidates.append(contentsOf: balancedJSONObjects(in: trimmed))

        var seen = Set<String>()
        return candidates.compactMap { candidate in
            let value = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !value.isEmpty, seen.insert(value).inserted else {
                return nil
            }
            return value
        }
    }

    private static func fencedCodeBlockBodies(from text: String) -> [String] {
        var bodies: [String] = []
        var searchStart = text.startIndex
        while let fenceStart = text.range(of: "```", range: searchStart..<text.endIndex) {
            let afterOpeningFence = fenceStart.upperBound
            guard let fenceEnd = text.range(of: "```", range: afterOpeningFence..<text.endIndex)
            else {
                break
            }
            let rawBody = String(text[afterOpeningFence..<fenceEnd.lowerBound])
            bodies.append(stripFenceLanguageLine(rawBody))
            searchStart = fenceEnd.upperBound
        }
        return bodies
    }

    private static func stripFenceLanguageLine(_ body: String) -> String {
        var text = body
        if text.hasPrefix("\n") {
            text.removeFirst()
            return text
        }
        guard let newline = text.firstIndex(of: "\n") else {
            return text
        }
        let firstLine = text[..<newline].trimmingCharacters(in: .whitespacesAndNewlines)
        guard firstLine.range(of: #"^[A-Za-z0-9_-]+$"#, options: .regularExpression) != nil else {
            return text
        }
        return String(text[text.index(after: newline)...])
    }

    private static func balancedJSONObjects(in text: String) -> [String] {
        var objects: [String] = []
        var objectStart: String.Index?
        var depth = 0
        var isInsideString = false
        var isEscaped = false

        var index = text.startIndex
        while index < text.endIndex {
            let character = text[index]
            if isInsideString {
                if isEscaped {
                    isEscaped = false
                } else if character == "\\" {
                    isEscaped = true
                } else if character == "\"" {
                    isInsideString = false
                }
                index = text.index(after: index)
                continue
            }

            if character == "\"" {
                isInsideString = true
            } else if character == "{" {
                if depth == 0 {
                    objectStart = index
                }
                depth += 1
            } else if character == "}", depth > 0 {
                depth -= 1
                if depth == 0, let start = objectStart {
                    objects.append(String(text[start...index]))
                    objectStart = nil
                }
            }
            index = text.index(after: index)
        }
        return objects
    }
}
