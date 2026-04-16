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

        do {
            try fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
            let data = try JSONEncoder.pretty.encode(snapshot)
            try data.write(to: directoryURL.appendingPathComponent("state.json"), options: .atomic)
        } catch {
            debugLog.log("persistence", "save failed error=\(error.localizedDescription)")
        }
    }

    private func stateFileURL() -> URL? {
        appSupportDirectoryURL()?.appendingPathComponent("state.json")
    }

    private func appSupportDirectoryURL() -> URL? {
        fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?.appendingPathComponent("dmux")
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
        for project in sanitizedProjects {
            let existingWorkspace = workspaceByProjectID[project.id]
            let sanitizedWorkspace = sanitizeWorkspace(existingWorkspace, for: project)
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

        let sanitizedSnapshot = AppSnapshot(
            projects: sanitizedProjects,
            workspaces: sanitizedWorkspaces,
            selectedProjectID: sanitizedSelectedProjectID,
            appSettings: snapshot.appSettings
        )

        return SanitizedSnapshot(snapshot: sanitizedSnapshot, didChange: didChange)
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
            topPaneRatios: workspace.topPaneRatios,
            bottomTabSessionIDs: finalBottomSessionIDs,
            bottomPaneHeight: max(180, workspace.bottomPaneHeight),
            selectedSessionID: selectedSessionID,
            selectedBottomTabSessionID: selectedBottomTabSessionID,
            sessions: sessions
        )

        if sanitizedWorkspace != workspace {
            didChange = true
        }

        return SanitizedWorkspace(workspace: sanitizedWorkspace, didChange: didChange)
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
            cwd: sanitizedCWD,
            shell: sanitizedShell,
            command: session.command,
            previewLines: session.previewLines
        )
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
    static let invalidFileDateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter
    }()
}
