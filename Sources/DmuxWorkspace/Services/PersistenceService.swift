import Foundation

enum PersistenceLoadIssue: Equatable {
    case invalidStateFile(backupFileName: String?)
    case sanitizedState
}

struct PersistenceLoadResult {
    var snapshot: AppSnapshot?
    var issues: [PersistenceLoadIssue]

    static let empty = PersistenceLoadResult(snapshot: nil, issues: [])
}

struct PersistenceService {
    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared

    func load() -> AppSnapshot? {
        loadWithRecovery().snapshot
    }

    func loadWithRecovery() -> PersistenceLoadResult {
        guard let fileURL = stateFileURL(),
              fileManager.fileExists(atPath: fileURL.path) else {
            return .empty
        }

        let data: Data
        do {
            data = try Data(contentsOf: fileURL)
        } catch {
            debugLog.log("persistence", "read failed path=\(fileURL.path) error=\(error.localizedDescription)")
            return .empty
        }

        guard !data.isEmpty else {
            do {
                try fileManager.removeItem(at: fileURL)
            } catch {
                debugLog.log("persistence", "remove empty state failed path=\(fileURL.path) error=\(error.localizedDescription)")
            }
            return PersistenceLoadResult(snapshot: nil, issues: [.invalidStateFile(backupFileName: nil)])
        }

        let decodedSnapshot: AppSnapshot
        do {
            decodedSnapshot = try JSONDecoder().decode(AppSnapshot.self, from: data)
        } catch {
            let backupURL = backupInvalidFile(at: fileURL)
            do {
                try fileManager.removeItem(at: fileURL)
            } catch {
                debugLog.log("persistence", "remove invalid state failed path=\(fileURL.path) error=\(error.localizedDescription)")
            }
            debugLog.log(
                "persistence",
                "recovered invalid state path=\(fileURL.path) backup=\(backupURL?.lastPathComponent ?? "nil") error=\(error.localizedDescription)"
            )
            return PersistenceLoadResult(
                snapshot: nil,
                issues: [.invalidStateFile(backupFileName: backupURL?.lastPathComponent)]
            )
        }

        let sanitized = sanitize(decodedSnapshot)
        if sanitized.didChange {
            save(sanitized.snapshot)
            debugLog.log(
                "persistence",
                "sanitized state path=\(fileURL.path) projects=\(sanitized.snapshot.projects.count) workspaces=\(sanitized.snapshot.workspaces.count)"
            )
        }

        var issues: [PersistenceLoadIssue] = []
        if sanitized.didChange {
            issues.append(.sanitizedState)
        }
        return PersistenceLoadResult(snapshot: sanitized.snapshot, issues: issues)
    }

    func loadStoredLanguagePreference() -> AppLanguage {
        guard let fileURL = stateFileURL(),
              fileManager.fileExists(atPath: fileURL.path),
              let data = try? Data(contentsOf: fileURL),
              !data.isEmpty,
              let jsonObject = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let settings = jsonObject["appSettings"] as? [String: Any],
              let rawValue = settings["language"] as? String,
              let language = AppLanguage(rawValue: rawValue) else {
            return .system
        }
        return language
    }

    func save(_ snapshot: AppSnapshot) {
        guard let directoryURL = appSupportDirectoryURL() else {
            return
        }
        let debugLog = debugLog

        Self.saveQueue.sync {
            Self.write(snapshot, to: directoryURL, debugLog: debugLog, failurePrefix: "save")
        }
    }

    func saveInBackground(_ snapshot: AppSnapshot) {
        guard let directoryURL = appSupportDirectoryURL() else {
            return
        }
        let debugLog = debugLog

        Self.saveQueue.async {
            Self.write(snapshot, to: directoryURL, debugLog: debugLog, failurePrefix: "background save")
        }
    }

    private static func write(_ snapshot: AppSnapshot, to directoryURL: URL, debugLog: AppDebugLog, failurePrefix: String) {
        do {
            try FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
            let data = try JSONEncoder.pretty.encode(snapshot)
            try data.write(to: directoryURL.appendingPathComponent("state.json"), options: .atomic)
        } catch {
            debugLog.log("persistence", "\(failurePrefix) failed error=\(error.localizedDescription)")
        }
    }

    private func stateFileURL() -> URL? {
        appSupportDirectoryURL()?.appendingPathComponent("state.json")
    }

    private func appSupportDirectoryURL() -> URL? {
        AppRuntimePaths.appSupportRootURL(fileManager: fileManager)
    }

    private func backupInvalidFile(at fileURL: URL) -> URL? {
        let backupURL = invalidBackupURL(for: fileURL)
        do {
            if fileManager.fileExists(atPath: backupURL.path) {
                try fileManager.removeItem(at: backupURL)
            }
            try fileManager.copyItem(at: fileURL, to: backupURL)
            return backupURL
        } catch {
            debugLog.log(
                "persistence",
                "backup failed source=\(fileURL.path) target=\(backupURL.path) error=\(error.localizedDescription)"
            )
            return nil
        }
    }

    private func invalidBackupURL(for fileURL: URL) -> URL {
        let timestamp = Self.invalidFileDateFormatter.string(from: Date())
        let directoryURL = fileURL.deletingLastPathComponent()
        let baseName = fileURL.deletingPathExtension().lastPathComponent
        let pathExtension = fileURL.pathExtension
        let fileName = pathExtension.isEmpty
            ? "\(baseName).invalid-\(timestamp)"
            : "\(baseName).invalid-\(timestamp).\(pathExtension)"
        return directoryURL.appendingPathComponent(fileName, isDirectory: false)
    }

    private func sanitize(_ snapshot: AppSnapshot) -> SanitizedSnapshot {
        let defaultSettings = AppSettings()
        var didChange = false
        var sanitizedProjects: [Project] = []
        var seenProjectIDs = Set<UUID>()
        var seenProjectPaths = Set<String>()

        for project in snapshot.projects {
            let originalPath = project.path.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !originalPath.isEmpty else {
                didChange = true
                continue
            }

            let normalizedPath = normalizePath(originalPath)
            var isDirectory = ObjCBool(false)
            if fileManager.fileExists(atPath: normalizedPath, isDirectory: &isDirectory), !isDirectory.boolValue {
                didChange = true
                continue
            }

            if seenProjectIDs.insert(project.id).inserted == false {
                didChange = true
                continue
            }
            if seenProjectPaths.insert(normalizedPath).inserted == false {
                didChange = true
                continue
            }

            let derivedName = URL(fileURLWithPath: normalizedPath, isDirectory: true).lastPathComponent
            let nextName = project.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? (derivedName.isEmpty ? "Project" : derivedName)
                : project.name.trimmingCharacters(in: .whitespacesAndNewlines)
            let nextShell = project.shell.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? defaultSettings.defaultTerminal.shellPath
                : project.shell

            let sanitizedProject = Project(
                id: project.id,
                name: nextName,
                path: normalizedPath,
                shell: nextShell,
                defaultCommand: project.defaultCommand,
                badgeText: project.badgeText,
                badgeSymbol: project.badgeSymbol,
                badgeColorHex: project.badgeColorHex,
                gitDefaultPushRemoteName: project.gitDefaultPushRemoteName
            )

            if sanitizedProject != project {
                didChange = true
            }
            sanitizedProjects.append(sanitizedProject)
        }

        let sanitizedWorktrees = sanitizeWorktrees(
            snapshot.worktrees,
            projects: sanitizedProjects,
            didChange: &didChange
        )

        var workspaceByProjectID: [UUID: ProjectWorkspace] = [:]
        var emittedWorkspaceIDs = Set<UUID>()
        for workspace in snapshot.workspaces {
            guard emittedWorkspaceIDs.insert(workspace.projectID).inserted else {
                didChange = true
                continue
            }
            workspaceByProjectID[workspace.projectID] = workspace
        }

        var sanitizedWorkspaces: [ProjectWorkspace] = []
        for worktree in sanitizedWorktrees {
            guard let rootProject = sanitizedProjects.first(where: { $0.id == worktree.projectID }) else {
                didChange = true
                continue
            }
            let effectiveProject = Project(
                id: worktree.id,
                name: worktree.isDefault ? rootProject.name : "\(rootProject.name) · \(worktree.name)",
                path: worktree.path,
                shell: rootProject.shell,
                defaultCommand: rootProject.defaultCommand,
                badgeText: rootProject.badgeText,
                badgeSymbol: rootProject.badgeSymbol,
                badgeColorHex: rootProject.badgeColorHex,
                gitDefaultPushRemoteName: rootProject.gitDefaultPushRemoteName
            )
            let existingWorkspace = workspaceByProjectID[worktree.id]
            let sanitizedWorkspace = sanitizeWorkspace(existingWorkspace, for: effectiveProject)
            if sanitizedWorkspace.didChange {
                didChange = true
            }
            sanitizedWorkspaces.append(sanitizedWorkspace.workspace)
        }

        let sanitizedSelectedProjectID: UUID?
        if let selectedProjectID = snapshot.selectedProjectID,
           sanitizedProjects.contains(where: { $0.id == selectedProjectID }) {
            sanitizedSelectedProjectID = selectedProjectID
        } else {
            sanitizedSelectedProjectID = sanitizedProjects.first?.id
            if snapshot.selectedProjectID != sanitizedSelectedProjectID {
                didChange = true
            }
        }
        let sanitizedSelectedWorktreeID: UUID?
        if let selectedWorktreeID = snapshot.selectedWorktreeID,
           sanitizedWorktrees.contains(where: { $0.id == selectedWorktreeID }) {
            sanitizedSelectedWorktreeID = selectedWorktreeID
        } else if let sanitizedSelectedProjectID {
            sanitizedSelectedWorktreeID = sanitizedWorktrees.first(where: { $0.projectID == sanitizedSelectedProjectID && $0.isDefault })?.id
                ?? sanitizedWorktrees.first(where: { $0.projectID == sanitizedSelectedProjectID })?.id
            if snapshot.selectedWorktreeID != sanitizedSelectedWorktreeID {
                didChange = true
            }
        } else {
            sanitizedSelectedWorktreeID = nil
            if snapshot.selectedWorktreeID != nil {
                didChange = true
            }
        }

        let sanitizedSnapshot = AppSnapshot(
            projects: sanitizedProjects,
            worktrees: sanitizedWorktrees,
            worktreeTasks: sanitizeWorktreeTasks(
                snapshot.worktreeTasks ?? [],
                worktrees: sanitizedWorktrees,
                didChange: &didChange
            ),
            workspaces: sanitizedWorkspaces,
            selectedProjectID: sanitizedSelectedProjectID,
            selectedWorktreeID: sanitizedSelectedWorktreeID,
            workspaceContentStates: sanitizeWorkspaceContentStates(
                snapshot.workspaceContentStates ?? [],
                worktrees: sanitizedWorktrees,
                didChange: &didChange
            ),
            appSettings: snapshot.appSettings,
            taskMemos: sanitizeTaskMemos(snapshot.taskMemos ?? [], projects: sanitizedProjects, workspaces: sanitizedWorkspaces, didChange: &didChange),
            sshProfiles: sanitizeSSHProfiles(snapshot.sshProfiles ?? [], didChange: &didChange)
        )

        return SanitizedSnapshot(snapshot: sanitizedSnapshot, didChange: didChange)
    }

    private func sanitizeWorktrees(
        _ worktrees: [ProjectWorktree]?,
        projects: [Project],
        didChange: inout Bool
    ) -> [ProjectWorktree] {
        let projectIDs = Set(projects.map(\.id))
        var seenIDs = Set<UUID>()
        var seenPaths = Set<String>()
        var sanitizedWorktrees: [ProjectWorktree] = []

        for worktree in worktrees ?? [] {
            guard projectIDs.contains(worktree.projectID),
                  seenIDs.insert(worktree.id).inserted else {
                didChange = true
                continue
            }

            let normalizedPath = normalizePath(worktree.path)
            guard !normalizedPath.isEmpty,
                  seenPaths.insert(normalizedPath).inserted else {
                didChange = true
                continue
            }

            var sanitized = worktree
            sanitized.path = normalizedPath
            sanitized.name = worktree.name.trimmingCharacters(in: .whitespacesAndNewlines)
            if sanitized.name.isEmpty {
                sanitized.name = worktree.isDefault
                    ? String(localized: "worktree.default.name", defaultValue: "Default", bundle: .module)
                    : URL(fileURLWithPath: normalizedPath, isDirectory: true).lastPathComponent
            }
            sanitized.branch = worktree.branch.trimmingCharacters(in: .whitespacesAndNewlines)

            if sanitized != worktree {
                didChange = true
            }
            sanitizedWorktrees.append(sanitized)
        }

        for project in projects where sanitizedWorktrees.contains(where: { $0.projectID == project.id && $0.isDefault }) == false {
            let defaultWorktree = ProjectWorktree.defaultWorktree(for: project)
            if seenIDs.insert(defaultWorktree.id).inserted,
               seenPaths.insert(normalizePath(defaultWorktree.path)).inserted {
                sanitizedWorktrees.append(defaultWorktree)
                didChange = true
            }
        }

        let projectOrder = Dictionary(uniqueKeysWithValues: projects.enumerated().map { ($0.element.id, $0.offset) })
        return sanitizedWorktrees.sorted { lhs, rhs in
            let lhsProjectOrder = projectOrder[lhs.projectID] ?? Int.max
            let rhsProjectOrder = projectOrder[rhs.projectID] ?? Int.max
            if lhsProjectOrder != rhsProjectOrder {
                return lhsProjectOrder < rhsProjectOrder
            }
            if lhs.isDefault != rhs.isDefault {
                return lhs.isDefault
            }
            return lhs.updatedAt > rhs.updatedAt
        }
    }

    private func sanitizeWorktreeTasks(
        _ tasks: [WorktreeTask],
        worktrees: [ProjectWorktree],
        didChange: inout Bool
    ) -> [WorktreeTask] {
        let validWorktreeIDs = Set(worktrees.map(\.id))
        var seenWorktreeIDs = Set<UUID>()
        return tasks.compactMap { task in
            guard validWorktreeIDs.contains(task.worktreeID),
                  seenWorktreeIDs.insert(task.worktreeID).inserted else {
                didChange = true
                return nil
            }

            var sanitized = task
            sanitized.title = task.title.trimmingCharacters(in: .whitespacesAndNewlines)
            sanitized.baseBranch = task.baseBranch.trimmingCharacters(in: .whitespacesAndNewlines)
            if sanitized.title.isEmpty {
                sanitized.title = worktrees.first(where: { $0.id == task.worktreeID })?.name
                    ?? String(localized: "worktree.task.untitled", defaultValue: "Untitled Task", bundle: .module)
            }
            if sanitized != task {
                didChange = true
            }
            return sanitized
        }
    }

    private func sanitizeTaskMemos(
        _ taskMemos: [TaskMemoItem],
        projects: [Project],
        workspaces: [ProjectWorkspace],
        didChange: inout Bool
    ) -> [TaskMemoItem] {
        let projectIDs = Set(workspaces.map(\.projectID))
        let sessionIDsByProjectID = Dictionary(
            uniqueKeysWithValues: workspaces.map { workspace in
                (workspace.projectID, Set(workspace.sessions.map(\.id)))
            }
        )
        var seenIDs = Set<UUID>()
        var sanitizedItems: [TaskMemoItem] = []

        for item in taskMemos {
            guard projectIDs.contains(item.projectID),
                  sessionIDsByProjectID[item.projectID]?.contains(item.sessionID) == true,
                  seenIDs.insert(item.id).inserted else {
                didChange = true
                continue
            }

            var sanitized = item
            sanitized.content = item.content.trimmingCharacters(in: .whitespacesAndNewlines)
            if sanitized.content.isEmpty {
                didChange = true
                continue
            }
            if sanitized.updatedAt < sanitized.createdAt {
                sanitized.updatedAt = sanitized.createdAt
                didChange = true
            }
            if sanitized != item {
                didChange = true
            }
            sanitizedItems.append(sanitized)
        }

        return sanitizedItems.sorted { lhs, rhs in
            lhs.createdAt < rhs.createdAt
        }
    }

    private func sanitizeSSHProfiles(_ profiles: [SSHConnectionProfile], didChange: inout Bool) -> [SSHConnectionProfile] {
        var seenIDs = Set<UUID>()
        var sanitizedProfiles: [SSHConnectionProfile] = []

        for profile in profiles {
            let host = profile.host.trimmingCharacters(in: .whitespacesAndNewlines)
            let username = profile.username.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !host.isEmpty, !username.isEmpty, seenIDs.insert(profile.id).inserted else {
                didChange = true
                continue
            }

            let sanitized = SSHConnectionProfile(
                id: profile.id,
                name: profile.name.trimmingCharacters(in: .whitespacesAndNewlines),
                host: host,
                port: min(max(profile.port, 1), 65535),
                username: username,
                credentialKind: profile.credentialKind,
                privateKeyPath: profile.privateKeyPath.trimmingCharacters(in: .whitespacesAndNewlines),
                updatedAt: profile.updatedAt,
                password: profile.credentialKind == .password ? profile.password : nil,
                keyPassphrase: profile.credentialKind == .privateKey ? profile.keyPassphrase : nil
            )
            if sanitized != profile {
                didChange = true
            }
            sanitizedProfiles.append(sanitized)
        }

        return sanitizedProfiles.sorted { lhs, rhs in
            lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
        }
    }

    private func sanitizeWorkspaceContentStates(
        _ states: [WorkspaceContentState],
        worktrees: [ProjectWorktree],
        didChange: inout Bool
    ) -> [WorkspaceContentState]? {
        let validWorktreeIDs = Set(worktrees.map(\.id))
        var seenWorktreeIDs = Set<UUID>()
        var sanitizedStates: [WorkspaceContentState] = []

        for state in states {
            guard validWorktreeIDs.contains(state.worktreeID),
                  seenWorktreeIDs.insert(state.worktreeID).inserted else {
                didChange = true
                continue
            }

            var seenFileTabIDs = Set<String>()
            var sanitizedTabs: [WorkspaceFileTab] = []
            for tab in state.fileTabs {
                var sanitizedTab = tab
                sanitizedTab.fileURL = tab.fileURL.standardizedFileURL
                sanitizedTab.rootURL = tab.rootURL.standardizedFileURL
                let fallbackTitle = sanitizedTab.fileURL.lastPathComponent
                let title = sanitizedTab.title.trimmingCharacters(in: .whitespacesAndNewlines)
                sanitizedTab.title = title.isEmpty ? fallbackTitle : title

                guard !sanitizedTab.id.isEmpty,
                      seenFileTabIDs.insert(sanitizedTab.id).inserted else {
                    didChange = true
                    continue
                }

                if sanitizedTab != tab {
                    didChange = true
                }
                sanitizedTabs.append(sanitizedTab)
            }

            let selectedFileTabID: String?
            if let candidate = state.selectedFileTabID,
               sanitizedTabs.contains(where: { $0.id == candidate }) {
                selectedFileTabID = candidate
            } else {
                selectedFileTabID = nil
                if state.selectedFileTabID != nil {
                    didChange = true
                }
            }

            let sanitizedState = WorkspaceContentState(
                worktreeID: state.worktreeID,
                primaryViewMode: state.primaryViewMode,
                selectedFileTabID: selectedFileTabID,
                fileTabs: sanitizedTabs
            )

            guard sanitizedState.primaryViewMode != .terminal
                || !sanitizedState.fileTabs.isEmpty
                || sanitizedState.selectedFileTabID != nil else {
                didChange = true
                continue
            }

            sanitizedStates.append(sanitizedState)
        }

        return sanitizedStates.isEmpty ? nil : sanitizedStates
    }

    private func sanitizeWorkspace(_ workspace: ProjectWorkspace?, for project: Project) -> SanitizedWorkspace {
        guard var workspace else {
            return SanitizedWorkspace(workspace: ProjectWorkspace.sample(projectID: project.id, path: project.path), didChange: true)
        }

        var didChange = false
        if workspace.projectID != project.id {
            didChange = true
        }
        workspace.projectID = project.id

        var sessions: [TerminalSession] = []
        var seenSessionIDs = Set<UUID>()

        for session in workspace.sessions {
            guard seenSessionIDs.insert(session.id).inserted else {
                didChange = true
                continue
            }

            let sanitizedSession = sanitizeSession(session, for: project)
            if sanitizedSession != session {
                didChange = true
            }
            sessions.append(sanitizedSession)
        }

        if sessions.isEmpty {
            sessions = [TerminalSession.make(project: project, command: project.defaultCommand)]
            didChange = true
        }

        let sessionIDs = Set(sessions.map(\.id))
        var consumedSessionIDs = Set<UUID>()

        let topSessionIDs = sanitizeSessionIDList(
            workspace.topSessionIDs,
            validIDs: sessionIDs,
            consumedIDs: &consumedSessionIDs,
            maxCount: ProjectWorkspace.maxTopPanes,
            didChange: &didChange
        )
        let bottomSessionIDs = sanitizeSessionIDList(
            workspace.bottomTabSessionIDs,
            validIDs: sessionIDs,
            consumedIDs: &consumedSessionIDs,
            maxCount: nil,
            didChange: &didChange
        )

        var finalTopSessionIDs = topSessionIDs
        var finalBottomSessionIDs = bottomSessionIDs

        let remainingSessionIDs = sessions.map(\.id).filter { consumedSessionIDs.contains($0) == false }
        if !remainingSessionIDs.isEmpty {
            didChange = true
        }
        for sessionID in remainingSessionIDs {
            if finalTopSessionIDs.count < ProjectWorkspace.maxTopPanes {
                finalTopSessionIDs.append(sessionID)
            } else {
                finalBottomSessionIDs.append(sessionID)
            }
        }

        if finalTopSessionIDs.isEmpty, let firstBottomSessionID = finalBottomSessionIDs.first {
            finalTopSessionIDs = [firstBottomSessionID]
            finalBottomSessionIDs.removeFirst()
            didChange = true
        }

        if finalTopSessionIDs.isEmpty, finalBottomSessionIDs.isEmpty {
            let recoverySession = TerminalSession.make(project: project, command: project.defaultCommand)
            sessions = [recoverySession]
            finalTopSessionIDs = [recoverySession.id]
            didChange = true
        }

        let allVisibleSessionIDs = Set(finalTopSessionIDs + finalBottomSessionIDs)
        let selectedSessionID: UUID
        if allVisibleSessionIDs.contains(workspace.selectedSessionID) {
            selectedSessionID = workspace.selectedSessionID
        } else {
            selectedSessionID = finalTopSessionIDs.first ?? finalBottomSessionIDs.first ?? sessions[0].id
            didChange = true
        }

        let selectedBottomTabSessionID: UUID?
        if let existingSelectedBottomTabSessionID = workspace.selectedBottomTabSessionID,
           finalBottomSessionIDs.contains(existingSelectedBottomTabSessionID) {
            selectedBottomTabSessionID = existingSelectedBottomTabSessionID
        } else {
            selectedBottomTabSessionID = finalBottomSessionIDs.last
            if workspace.selectedBottomTabSessionID != selectedBottomTabSessionID {
                didChange = true
            }
        }

        let sanitizedWorkspace = ProjectWorkspace(
            projectID: project.id,
            topSessionIDs: finalTopSessionIDs,
            topPaneRatios: [],
            bottomTabSessionIDs: finalBottomSessionIDs,
            bottomPaneHeight: ProjectWorkspace.defaultBottomPaneHeight,
            selectedSessionID: selectedSessionID,
            selectedBottomTabSessionID: selectedBottomTabSessionID,
            sessions: sessions
        )

        var titledWorkspace = sanitizedWorkspace
        if titledWorkspace.ensureDefaultBottomTabTitles() {
            didChange = true
        }

        if titledWorkspace != workspace {
            didChange = true
        }

        return SanitizedWorkspace(workspace: titledWorkspace, didChange: didChange)
    }

    private func sanitizeSession(_ session: TerminalSession, for project: Project) -> TerminalSession {
        let sanitizedCWD: String
        let trimmedCWD = session.cwd.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedCWD.isEmpty {
            sanitizedCWD = project.path
        } else {
            let normalizedCWD = normalizePath(trimmedCWD)
            var isDirectory = ObjCBool(false)
            if fileManager.fileExists(atPath: normalizedCWD, isDirectory: &isDirectory), !isDirectory.boolValue {
                sanitizedCWD = project.path
            } else {
                sanitizedCWD = normalizedCWD
            }
        }

        let sanitizedShell = session.shell.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? project.shell
            : session.shell
        let sanitizedTitle = session.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? project.name
            : session.title

        return TerminalSession(
            id: session.id,
            projectID: project.id,
            projectName: project.name,
            title: sanitizedTitle,
            tabTitle: sanitizedTabTitle(session.tabTitle),
            cwd: sanitizedCWD,
            shell: sanitizedShell,
            command: session.command,
            previewLines: session.previewLines,
            launchMode: session.launchMode,
            agentTool: session.agentTool
        )
    }

    private func sanitizedTabTitle(_ title: String?) -> String? {
        guard let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmedTitle.isEmpty else {
            return nil
        }
        return trimmedTitle
    }

    private func sanitizeSessionIDList(
        _ ids: [UUID],
        validIDs: Set<UUID>,
        consumedIDs: inout Set<UUID>,
        maxCount: Int?,
        didChange: inout Bool
    ) -> [UUID] {
        var result: [UUID] = []
        for id in ids {
            guard validIDs.contains(id), consumedIDs.contains(id) == false else {
                didChange = true
                continue
            }
            result.append(id)
            consumedIDs.insert(id)
            if let maxCount, result.count >= maxCount {
                if ids.count > result.count {
                    didChange = true
                }
                break
            }
        }
        return result
    }

    private func normalizePath(_ path: String) -> String {
        URL(fileURLWithPath: (path as NSString).expandingTildeInPath, isDirectory: true)
            .standardizedFileURL
            .path
    }

    private struct SanitizedSnapshot {
        let snapshot: AppSnapshot
        let didChange: Bool
    }

    private struct SanitizedWorkspace {
        let workspace: ProjectWorkspace
        let didChange: Bool
    }

}

private extension JSONEncoder {
    static var pretty: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return encoder
    }
}

private extension PersistenceService {
    static let saveQueue = DispatchQueue(label: "codux.persistence.save", qos: .utility)

    static let invalidFileDateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter
    }()
}
