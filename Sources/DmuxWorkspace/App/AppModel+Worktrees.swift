import AppKit
import Foundation
import UniformTypeIdentifiers

extension AppModel {
    func createWorktree(for projectID: UUID) {
        guard let project = projects.first(where: { $0.id == projectID }) else {
            statusMessage = String(localized: "project.not_found", defaultValue: "Project not found.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let baseBranch = (try? gitService.branchName(at: project.path))
            .flatMap { $0 == "detached HEAD" ? nil : $0 }
            ?? "main"
        let localBranches = (try? gitService.localBranches(at: project.path)) ?? []
        let baseBranches = Array(Set(localBranches + [baseBranch]))
            .filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
            .sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
        let branchSeed = "task/\(Self.worktreeTimestampSlug())"
        let dialog = WorktreeTaskDialogState(
            title: String(localized: "worktree.create.title", defaultValue: "New Worktree", bundle: .module),
            message: String(
                format: String(localized: "worktree.task.create.message_format", defaultValue: "Create a task workspace for %@.", bundle: .module),
                project.name
            ),
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            baseBranches: baseBranches,
            baseBranch: baseBranch,
            branchName: branchSeed,
            taskTitle: String(localized: "worktree.task.default_title", defaultValue: "New Task", bundle: .module)
        )

        WorktreeTaskPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, let result else {
                return
            }
            self.createWorktree(for: projectID, request: result)
        }
    }

    func createWorktree(for projectID: UUID, branchName rawBranchName: String) {
        let request = WorktreeTaskDialogResult(
            baseBranch: "",
            branchName: rawBranchName,
            taskTitle: worktreeDisplayName(forBranch: rawBranchName)
        )
        createWorktree(for: projectID, request: request)
    }

    func createWorktree(for projectID: UUID, request: WorktreeTaskDialogResult) {
        guard let project = projects.first(where: { $0.id == projectID }) else {
            statusMessage = String(localized: "project.not_found", defaultValue: "Project not found.", bundle: .module)
            return
        }

        let branchName = request.branchName.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseBranch = request.baseBranch.trimmingCharacters(in: .whitespacesAndNewlines)
        let taskTitle = request.taskTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !branchName.isEmpty else {
            statusMessage = String(localized: "worktree.branch.empty", defaultValue: "Branch name cannot be empty.", bundle: .module)
            return
        }
        guard worktrees.contains(where: { $0.projectID == projectID && $0.branch == branchName }) == false else {
            statusMessage = String(localized: "worktree.branch.exists", defaultValue: "This branch already has a worktree.", bundle: .module)
            return
        }

        let destinationURL = managedWorktreeURL(project: project, branchName: branchName)
        let repositoryPath = project.path
        statusMessage = String(localized: "worktree.create.running", defaultValue: "Creating worktree.", bundle: .module)

        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                let parentURL = destinationURL.deletingLastPathComponent()
                try FileManager.default.createDirectory(at: parentURL, withIntermediateDirectories: true)
                let baseCommit = try? service.commitHash(ref: baseBranch, at: repositoryPath)
                try service.createWorktree(branch: branchName, destinationPath: destinationURL.path, baseRef: baseBranch, at: repositoryPath)
                let worktreePath = destinationURL.standardizedFileURL.path
                await MainActor.run {
                    self.registerCreatedWorktree(
                        project: project,
                        branchName: branchName,
                        path: worktreePath,
                        taskTitle: taskTitle,
                        baseBranch: baseBranch,
                        baseCommit: baseCommit
                    )
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                }
            }
        }
    }

    func importDroppedProjectDirectories(_ urls: [URL]) {
        for url in urls {
            importDroppedProjectDirectory(url)
        }
    }

    func importDroppedProjectDirectory(_ url: URL) {
        let normalizedPath = url.standardizedFileURL.path
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: normalizedPath, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            return
        }

        Task.detached(priority: .userInitiated) {
            let service = GitService()
            let droppedOrigin = (try? service.originURL(at: normalizedPath))?.trimmingCharacters(in: .whitespacesAndNewlines)
            let droppedBranch = (try? service.branchName(at: normalizedPath)) ?? ""
            await MainActor.run {
                if let matchingProject = self.projects.first(where: { project in
                    guard let droppedOrigin, !droppedOrigin.isEmpty else {
                        return false
                    }
                    guard let projectOrigin = try? self.gitService.originURL(at: project.path) else {
                        return false
                    }
                    let normalizedProjectOrigin = projectOrigin.trimmingCharacters(in: .whitespacesAndNewlines)
                    return !normalizedProjectOrigin.isEmpty && normalizedProjectOrigin == droppedOrigin
                }) {
                    self.importExistingWorktree(
                        projectID: matchingProject.id,
                        path: normalizedPath,
                        branchName: droppedBranch
                    )
                } else {
                    self.importProject(
                        name: url.lastPathComponent,
                        path: normalizedPath,
                        badgeText: "",
                        badgeSymbol: nil,
                        badgeColorHex: systemAccentHexString()
                    )
                }
            }
        }
    }

    func openWorktreeDirectory(_ worktreeID: UUID) {
        guard let worktree = worktrees.first(where: { $0.id == worktreeID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        NSWorkspace.shared.open(URL(fileURLWithPath: worktree.path, isDirectory: true))
        statusMessage = String(localized: "project.open.folder.success", defaultValue: "Opened project folder.", bundle: .module)
    }

    func updateWorktreeStatus(_ worktreeID: UUID, status: ProjectWorktreeTaskStatus) {
        guard setWorktreeStatus(worktreeID, status: status, now: Date()) else {
            return
        }
        persist()
    }

    func removeWorktree(_ worktreeID: UUID) {
        guard let worktree = worktrees.first(where: { $0.id == worktreeID }),
              let project = projects.first(where: { $0.id == worktree.projectID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        guard !worktree.isDefault else {
            statusMessage = String(localized: "worktree.default.remove_denied", defaultValue: "The default worktree cannot be removed.", bundle: .module)
            return
        }

        let hasChanges = (try? gitService.hasUncommittedChanges(at: worktree.path)) ?? false
        guard !hasChanges else {
            statusMessage = String(localized: "worktree.remove.dirty_denied", defaultValue: "This worktree has uncommitted changes. Clean it before removing.", bundle: .module)
            return
        }

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "worktree.remove.title", defaultValue: "Remove Worktree", bundle: .module),
            message: String(
                format: String(localized: "worktree.remove.message_format", defaultValue: "Remove %@ from Codux and the Git worktree list? The branch will not be deleted.", bundle: .module),
                worktree.name
            ),
            icon: "trash",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.remove", defaultValue: "Remove", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else {
                return
            }
            self.performRemoveWorktree(worktree, rootProject: project)
        }
    }

    func worktreeGitSummary(_ worktree: ProjectWorktree) -> ProjectWorktreeGitSummary {
        worktreeGitSummaries[worktree.id] ?? .empty
    }

    func refreshWorktreeGitSummaries() {
        let visibleWorktrees = selectedProjectWorktrees
        pendingWorktreeGitSummaryRefreshTask?.cancel()

        guard visibleWorktrees.isEmpty == false else {
            worktreeGitSummaries = [:]
            pendingWorktreeGitSummaryRefreshTask = nil
            return
        }

        pendingWorktreeGitSummaryRefreshTask = Task.detached(priority: .utility) {
            let service = GitService()
            var summaries: [UUID: ProjectWorktreeGitSummary] = [:]

            for worktree in visibleWorktrees {
                guard Task.isCancelled == false else {
                    return
                }
                let state = try? service.repositoryState(at: worktree.path)
                guard Task.isCancelled == false else {
                    return
                }
                let remote = try? service.remoteSyncState(at: worktree.path)
                guard Task.isCancelled == false else {
                    return
                }
                summaries[worktree.id] = ProjectWorktreeGitSummary(
                    changes: state?.totalChanges ?? 0,
                    incoming: remote?.incomingCount ?? 0,
                    outgoing: remote?.outgoingCount ?? 0
                )
            }

            guard Task.isCancelled == false else {
                return
            }

            await MainActor.run {
                let visibleIDs = Set(visibleWorktrees.map(\.id))
                let existingIDs = Set(self.worktrees.map(\.id))
                var nextSummaries = self.worktreeGitSummaries.filter { id, _ in
                    existingIDs.contains(id) && visibleIDs.contains(id) == false
                }
                for (id, summary) in summaries where existingIDs.contains(id) {
                    nextSummaries[id] = summary
                }
                self.worktreeGitSummaries = nextSummaries
                self.pendingWorktreeGitSummaryRefreshTask = nil
            }
        }
    }

    private func registerCreatedWorktree(
        project: Project,
        branchName: String,
        path: String,
        taskTitle: String,
        baseBranch: String,
        baseCommit: String?
    ) {
        let name = worktreeDisplayName(forBranch: branchName)
        let resolvedTitle = taskTitle.isEmpty ? name : taskTitle
        let resolvedBaseBranch = baseBranch.isEmpty
            ? ((try? gitService.branchName(at: project.path)).flatMap { $0 == "detached HEAD" ? nil : $0 } ?? "")
            : baseBranch
        let worktree = ProjectWorktree(
            id: UUID(),
            projectID: project.id,
            name: resolvedTitle,
            branch: branchName,
            path: path,
            status: .todo,
            isDefault: false,
            createdAt: Date(),
            updatedAt: Date()
        )
        let task = WorktreeTask(
            worktreeID: worktree.id,
            title: resolvedTitle,
            baseBranch: resolvedBaseBranch,
            baseCommit: baseCommit,
            status: .todo,
            createdAt: Date(),
            updatedAt: Date(),
            startedAt: nil,
            completedAt: nil
        )
        worktrees.append(worktree)
        workspaces.append(ProjectWorkspace.sample(projectID: worktree.id, path: worktree.path))
        worktreeTasks.append(task)
        updateSelectedProjectID(project.id, source: "createWorktree")
        selectedWorktreeID = worktree.id
        statusMessage = String(
            format: String(localized: "worktree.create.success_format", defaultValue: "Created worktree %@.", bundle: .module),
            resolvedTitle
        )
        persist()
        refreshGitState()
        refreshAIStatsIfNeeded()
    }

    private func importExistingWorktree(projectID: UUID, path: String, branchName: String) {
        let normalizedPath = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.path
        if let existing = worktrees.first(where: { URL(fileURLWithPath: $0.path).standardizedFileURL.path == normalizedPath }) {
            selectWorktree(existing.id)
            statusMessage = String(localized: "worktree.exists.switched", defaultValue: "Worktree already exists. Switched to it.", bundle: .module)
            return
        }

        let rootProject = projects.first(where: { $0.id == projectID })
        let baseBranch = rootProject
            .flatMap { try? gitService.branchName(at: $0.path) }
            .flatMap { $0 == "detached HEAD" ? nil : $0 }
            ?? ""
        let baseCommit = rootProject.flatMap { project in
            baseBranch.isEmpty ? nil : (try? gitService.commitHash(ref: baseBranch, at: project.path))
        } ?? nil
        let name = branchName.isEmpty
            ? URL(fileURLWithPath: normalizedPath, isDirectory: true).lastPathComponent
            : worktreeDisplayName(forBranch: branchName)
        let worktree = ProjectWorktree(
            id: UUID(),
            projectID: projectID,
            name: name,
            branch: branchName,
            path: normalizedPath,
            status: .todo,
            isDefault: false,
            createdAt: Date(),
            updatedAt: Date()
        )
        let task = WorktreeTask(
            worktreeID: worktree.id,
            title: name,
            baseBranch: baseBranch,
            baseCommit: baseCommit,
            status: .todo,
            createdAt: Date(),
            updatedAt: Date(),
            startedAt: nil,
            completedAt: nil
        )
        worktrees.append(worktree)
        workspaces.append(ProjectWorkspace.sample(projectID: worktree.id, path: worktree.path))
        worktreeTasks.append(task)
        updateSelectedProjectID(projectID, source: "importWorktree")
        selectedWorktreeID = worktree.id
        persist()
        refreshGitState()
        refreshAIStatsIfNeeded()
        statusMessage = String(localized: "worktree.import.success", defaultValue: "Imported worktree.", bundle: .module)
    }

    private func performRemoveWorktree(_ worktree: ProjectWorktree, rootProject: Project) {
        statusMessage = String(localized: "worktree.remove.running", defaultValue: "Removing worktree.", bundle: .module)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                try service.removeWorktree(path: worktree.path, at: rootProject.path)
                await MainActor.run {
                    self.finishRemoveWorktree(worktree)
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                }
            }
        }
    }

    private func finishRemoveWorktree(_ worktree: ProjectWorktree) {
        worktrees.removeAll { $0.id == worktree.id }
        workspaces.removeAll { $0.projectID == worktree.id }
        workspaceFileTabsByWorktreeID[worktree.id] = nil
        selectedWorkspaceContentByWorktreeID[worktree.id] = nil
        workspacePrimaryViewModeByWorktreeID[worktree.id] = nil
        worktreeGitSummaries[worktree.id] = nil
        worktreeTasks.removeAll { $0.worktreeID == worktree.id }
        if selectedWorktreeReviewID == worktree.id {
            selectedWorktreeReviewID = nil
            selectedWorktreeReviewFileID = nil
            worktreeReviewSnapshot = nil
        }
        if selectedWorktreeID == worktree.id {
            selectedWorktreeID = worktrees.first(where: { $0.projectID == worktree.projectID && $0.isDefault })?.id
                ?? worktrees.first(where: { $0.projectID == worktree.projectID })?.id
        }
        statusMessage = String(localized: "worktree.remove.success", defaultValue: "Removed worktree.", bundle: .module)
        persist()
        refreshGitState()
        refreshAIStatsIfNeeded()
    }

    func worktreeTask(_ worktreeID: UUID) -> WorktreeTask? {
        worktreeTasks.first(where: { $0.worktreeID == worktreeID })
    }

    func effectiveWorktreeTaskStatus(for worktree: ProjectWorktree) -> ProjectWorktreeTaskStatus {
        let persistedStatus = worktreeTask(worktree.id)?.status ?? worktree.status
        if persistedStatus == .merged || persistedStatus == .archived {
            return persistedStatus
        }

        switch activityPhase(for: worktree.id) {
        case .running, .loading:
            return .running
        case .waitingInput:
            return .waiting
        case .completed:
            return persistedStatus == .review ? .review : .done
        case .idle:
            return persistedStatus
        }
    }

    func isWorktreeAIActive(for worktree: ProjectWorktree) -> Bool {
        switch activityPhase(for: worktree.id) {
        case .running, .loading:
            return true
        case .idle, .waitingInput, .completed:
            return false
        }
    }

    func worktreeStatusSummary(for projectID: UUID) -> String? {
        let projectWorktrees = worktrees.filter { $0.projectID == projectID && !$0.isDefault }
        guard !projectWorktrees.isEmpty else {
            return nil
        }
        var counts: [ProjectWorktreeTaskStatus: Int] = [:]
        for worktree in projectWorktrees {
            let status = effectiveWorktreeTaskStatus(for: worktree).visibleStatus
            counts[status, default: 0] += 1
        }
        let ordered: [ProjectWorktreeTaskStatus] = [.running, .waiting, .ready, .review, .blocked, .done, .merged, .todo, .archived]
        let parts = ordered.compactMap { status -> String? in
            guard let count = counts[status], count > 0 else { return nil }
            return "\(count) \(status.displayName)"
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    func syncWorktreeTaskStatusesFromRuntime() {
        var didChange = false

        for task in worktreeTasks {
            guard task.status != .merged,
                  task.status != .archived,
                  task.status != .review,
                  let nextStatus = runtimeTaskStatus(for: task.worktreeID) else {
                continue
            }
            didChange = setWorktreeStatus(task.worktreeID, status: nextStatus, now: Date()) || didChange
        }

        if didChange {
            persist()
        }
    }

    func openWorktreeReview(_ worktreeID: UUID) {
        guard worktrees.contains(where: { $0.id == worktreeID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        selectWorktree(worktreeID)
        selectedWorktreeReviewID = worktreeID
        selectedWorktreeReviewFileID = nil
        selectWorkspaceReview()
        let isDefaultWorktree = worktrees.first(where: { $0.id == worktreeID })?.isDefault == true
        if !isDefaultWorktree, let task = worktreeTask(worktreeID), task.status == .ready || task.status == .done {
            updateWorktreeStatus(worktreeID, status: .review)
        }
    }

    func selectWorktreeReviewFile(_ fileID: String) {
        guard selectedWorktreeReviewFileID != fileID else {
            return
        }
        selectedWorktreeReviewFileID = fileID
        refreshWorktreeReview()
    }

    func saveSelectedWorktreeReviewResult(text: String, deletesFile: Bool) {
        guard let worktreeID = selectedWorktreeReviewID,
              let worktree = worktrees.first(where: { $0.id == worktreeID }),
              let file = worktreeReviewSnapshot?.selectedFileComparison?.file else {
            statusMessage = String(localized: "worktree.review.select_file", defaultValue: "Select a changed file to compare.", bundle: .module)
            return
        }

        let isAudit = worktree.isDefault || worktreeReviewSnapshot?.mode == .workingTreeAudit
        statusMessage = isAudit
            ? String(localized: "worktree.review.audit_save_result.running", defaultValue: "Saving audit result.", bundle: .module)
            : String(localized: "worktree.review.save_result.running", defaultValue: "Saving merge result.", bundle: .module)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                try service.writeWorktreeReviewResult(
                    text,
                    deletesFile: deletesFile,
                    for: file,
                    at: worktree.path
                )
                await MainActor.run {
                    self.statusMessage = isAudit
                        ? String(localized: "worktree.review.audit_save_result.success", defaultValue: "Saved audit result.", bundle: .module)
                        : String(localized: "worktree.review.save_result.success", defaultValue: "Saved merge result.", bundle: .module)
                    self.refreshWorktreeGitSummaries()
                    self.refreshWorktreeReview()
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                    self.refreshWorktreeReview()
                }
            }
        }
    }

    func refreshWorktreeReview() {
        guard let worktreeID = selectedWorktreeReviewID,
              let worktree = worktrees.first(where: { $0.id == worktreeID }),
              let rootProject = projects.first(where: { $0.id == worktree.projectID }) else {
            worktreeReviewSnapshot = nil
            selectedWorktreeReviewFileID = nil
            isLoadingWorktreeReview = false
            return
        }
        let task = worktreeTask(worktreeID)
        let baseBranch = task?.baseBranch.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let title = task?.title ?? worktree.name
        if worktree.isDefault {
            refreshWorkingTreeAuditReview(worktree: worktree)
            return
        }
        guard !baseBranch.isEmpty, !worktree.branch.isEmpty else {
            worktreeReviewSnapshot = .empty(worktreeID: worktreeID, title: title)
            selectedWorktreeReviewFileID = nil
            isLoadingWorktreeReview = false
            return
        }

        isLoadingWorktreeReview = true
        let requestedFileID = selectedWorktreeReviewFileID
        let baseCommit = task?.baseCommit
        Task.detached(priority: .utility) {
            let service = GitService()
            let diffStat = (try? service.diffStat(from: baseBranch, to: worktree.branch, at: worktree.path)) ?? ""
            let files = (try? service.worktreeReviewFiles(from: baseBranch, at: worktree.path)) ?? []
            let selectedFileID = requestedFileID.flatMap { requested in
                files.contains(where: { $0.id == requested }) ? requested : nil
            } ?? files.first?.id
            let selectedFile = selectedFileID.flatMap { id in
                files.first(where: { $0.id == id })
            }
            let comparison = selectedFile.flatMap { file in
                try? service.worktreeReviewComparison(for: file, baseRef: baseBranch, at: worktree.path)
            }
            let checks = Self.worktreeReviewChecks(
                service: service,
                rootProjectPath: rootProject.path,
                worktreePath: worktree.path,
                baseBranch: baseBranch,
                taskBranch: worktree.branch,
                baseCommit: baseCommit,
                hasDiff: !files.isEmpty
            )
            await MainActor.run {
                guard self.selectedWorktreeReviewID == worktreeID else {
                    return
                }
                self.selectedWorktreeReviewFileID = selectedFileID
                self.worktreeReviewSnapshot = WorktreeReviewSnapshot(
                    worktreeID: worktreeID,
                    mode: .taskBranch,
                    title: title,
                    diffStat: diffStat,
                    files: files,
                    selectedFileID: selectedFileID,
                    selectedFileComparison: comparison,
                    checks: checks,
                    refreshedAt: Date()
                )
                self.isLoadingWorktreeReview = false
            }
        }
    }

    private func refreshWorkingTreeAuditReview(worktree: ProjectWorktree) {
        isLoadingWorktreeReview = true
        let requestedFileID = selectedWorktreeReviewFileID
        let title = String(localized: "worktree.review.audit_title", defaultValue: "Uncommitted Audit", bundle: .module)
        Task.detached(priority: .utility) {
            let service = GitService()
            let diffStat = (try? service.workingTreeAuditDiffStat(at: worktree.path)) ?? ""
            let files = (try? service.workingTreeAuditFiles(at: worktree.path)) ?? []
            let selectedFileID = requestedFileID.flatMap { requested in
                files.contains(where: { $0.id == requested }) ? requested : nil
            } ?? files.first?.id
            let selectedFile = selectedFileID.flatMap { id in
                files.first(where: { $0.id == id })
            }
            let comparison = selectedFile.flatMap { file in
                try? service.workingTreeAuditComparison(for: file, at: worktree.path)
            }
            let checks = Self.workingTreeAuditChecks(
                service: service,
                worktreePath: worktree.path,
                changeCount: files.count
            )
            await MainActor.run {
                guard self.selectedWorktreeReviewID == worktree.id else {
                    return
                }
                self.selectedWorktreeReviewFileID = selectedFileID
                self.worktreeReviewSnapshot = WorktreeReviewSnapshot(
                    worktreeID: worktree.id,
                    mode: .workingTreeAudit,
                    title: title,
                    diffStat: diffStat,
                    files: files,
                    selectedFileID: selectedFileID,
                    selectedFileComparison: comparison,
                    checks: checks,
                    refreshedAt: Date()
                )
                self.isLoadingWorktreeReview = false
            }
        }
    }

    func mergeReviewedWorktree(removeAfterMerge: Bool) {
        mergeReviewedWorktree(removeAfterMerge: removeAfterMerge, deleteBranchAfterMerge: false)
    }

    func discardSelectedReviewFile() {
        guard let worktreeID = selectedWorktreeReviewID,
              let worktree = worktrees.first(where: { $0.id == worktreeID }),
              let filePath = selectedWorktreeReviewFileID else {
            statusMessage = String(localized: "worktree.review.select_file", defaultValue: "Select a changed file to compare.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "agent.discard_file.title", defaultValue: "Discard File Change", bundle: .module),
            message: String(
                format: String(localized: "agent.discard_file.message_format", defaultValue: "Discard uncommitted changes in %@?", bundle: .module),
                filePath
            ),
            icon: "arrow.uturn.backward.circle",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.performDiscardReviewFile(filePath, worktree: worktree)
        }
    }

    func discardAllReviewChanges() {
        guard let worktreeID = selectedWorktreeReviewID,
              let worktree = worktrees.first(where: { $0.id == worktreeID }) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "agent.discard_all.title", defaultValue: "Discard All Changes", bundle: .module),
            message: String(localized: "agent.discard_all.message", defaultValue: "Discard all uncommitted changes in this worktree?", bundle: .module),
            icon: "arrow.uturn.backward.circle",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.files.discard_all", defaultValue: "Discard All", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.performDiscardAllReviewChanges(worktree: worktree)
        }
    }

    func mergeReviewedWorktree(removeAfterMerge: Bool, deleteBranchAfterMerge: Bool) {
        guard let worktreeID = selectedWorktreeReviewID,
              let worktree = worktrees.first(where: { $0.id == worktreeID }),
              let rootProject = projects.first(where: { $0.id == worktree.projectID }),
              let task = worktreeTask(worktreeID) else {
            statusMessage = String(localized: "worktree.not_found", defaultValue: "Worktree not found.", bundle: .module)
            return
        }
        guard !worktree.isDefault else {
            statusMessage = String(localized: "worktree.default.merge_denied", defaultValue: "The default worktree cannot be merged into itself.", bundle: .module)
            return
        }
        guard !task.baseBranch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            statusMessage = String(localized: "worktree.merge.base_missing", defaultValue: "This worktree has no base branch.", bundle: .module)
            return
        }
        guard isWorktreeMergeCandidateStatus(effectiveWorktreeTaskStatus(for: worktree)) else {
            statusMessage = String(localized: "worktree.merge.not_ready", defaultValue: "This worktree is not ready to merge.", bundle: .module)
            return
        }
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: removeAfterMerge
                ? String(localized: "worktree.merge_cleanup.title", defaultValue: "Merge and Remove Worktree", bundle: .module)
                : String(localized: "worktree.merge.title", defaultValue: "Merge Worktree", bundle: .module),
            message: String(
                format: removeAfterMerge
                    ? String(localized: "worktree.merge_cleanup.message_format", defaultValue: "Stage and commit any task changes, squash merge %@ into %@, then remove the worktree.", bundle: .module)
                    : String(localized: "worktree.merge.message_format", defaultValue: "Stage and commit any task changes, squash merge %@ into %@, then mark the task as merged.", bundle: .module),
                worktree.branch,
                task.baseBranch
            ),
            icon: "arrow.triangle.merge",
            iconColor: AppTheme.success,
            primaryTitle: removeAfterMerge
                ? String(localized: "worktree.review.merge_cleanup", defaultValue: "Merge & Remove", bundle: .module)
                : String(localized: "common.merge", defaultValue: "Merge", bundle: .module),
            primaryTint: AppTheme.success,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module),
            option: removeAfterMerge
                ? ConfirmDialogOptionState(
                    title: String(localized: "worktree.merge.delete_branch_option", defaultValue: "Delete the local task branch after merge", bundle: .module),
                    isOn: deleteBranchAfterMerge
                )
                : nil
        )

        if removeAfterMerge {
            ConfirmDialogPresenter.presentWithOption(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
                guard let self, result?.action == .primary else {
                    return
                }
                self.performMergeWorktree(
                    worktree,
                    task: task,
                    rootProject: rootProject,
                    removeAfterMerge: true,
                    deleteBranchAfterMerge: result?.isOptionEnabled == true
                )
            }
        } else {
            ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
                guard let self, result == .primary else {
                    return
                }
                self.performMergeWorktree(
                    worktree,
                    task: task,
                    rootProject: rootProject,
                    removeAfterMerge: false,
                    deleteBranchAfterMerge: false
                )
            }
        }
    }

    private func performDiscardReviewFile(_ filePath: String, worktree: ProjectWorktree) {
        statusMessage = String(localized: "agent.discard.running", defaultValue: "Discarding changes.", bundle: .module)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                try service.discardWorktreeReviewFile(filePath, at: worktree.path)
                await MainActor.run {
                    self.refreshGitState()
                    self.refreshWorktreeGitSummaries()
                    self.refreshWorktreeReview()
                    self.statusMessage = String(localized: "agent.discard_file.success", defaultValue: "Discarded file change.", bundle: .module)
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                    self.refreshWorktreeReview()
                }
            }
        }
    }

    private func performDiscardAllReviewChanges(worktree: ProjectWorktree) {
        statusMessage = String(localized: "agent.discard.running", defaultValue: "Discarding changes.", bundle: .module)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                try service.discardWorkingTreeChanges(at: worktree.path)
                await MainActor.run {
                    self.refreshGitState()
                    self.refreshWorktreeGitSummaries()
                    self.refreshWorktreeReview()
                    self.statusMessage = String(localized: "agent.discard_all.success", defaultValue: "Discarded all changes.", bundle: .module)
                }
            } catch {
                await MainActor.run {
                    self.statusMessage = error.localizedDescription
                    self.refreshWorktreeReview()
                }
            }
        }
    }

    private func performMergeWorktree(
        _ worktree: ProjectWorktree,
        task: WorktreeTask,
        rootProject: Project,
        removeAfterMerge: Bool,
        deleteBranchAfterMerge: Bool
    ) {
        statusMessage = String(localized: "worktree.merge.running", defaultValue: "Merging worktree.", bundle: .module)
        let taskCommitMessage = Self.worktreeTaskCommitMessage(task)
        let mergeCommitMessage = Self.worktreeMergeCommitMessage(task)
        Task.detached(priority: .userInitiated) {
            let service = GitService()
            do {
                if try service.hasUncommittedChanges(at: rootProject.path) {
                    throw WorktreeMergeError.dirtyBase
                }
                if try service.hasUncommittedChanges(at: worktree.path) {
                    try service.stageAll(at: worktree.path)
                    try service.commit(message: taskCommitMessage, at: worktree.path)
                }
                let conflictPaths = try service.mergeConflictPaths(from: task.baseBranch, to: worktree.branch, at: rootProject.path)
                if !conflictPaths.isEmpty {
                    throw WorktreeMergeError.mergeConflicts(conflictPaths)
                }
                let hasBranchDiff = try service.hasDiff(from: task.baseBranch, to: worktree.branch, at: rootProject.path)
                if hasBranchDiff {
                    try service.checkout(branch: task.baseBranch, at: rootProject.path)
                    try service.squashMerge(branch: worktree.branch, intoCurrentBranchAt: rootProject.path)
                    try service.commit(message: mergeCommitMessage, at: rootProject.path)
                }
                if removeAfterMerge {
                    try service.removeWorktree(path: worktree.path, at: rootProject.path)
                }
                if deleteBranchAfterMerge {
                    try service.deleteBranch(worktree.branch, force: true, at: rootProject.path)
                }
                await MainActor.run {
                    if removeAfterMerge {
                        self.finishRemoveWorktree(worktree)
                    } else {
                        self.updateWorktreeStatus(worktree.id, status: .merged)
                    }
                    self.refreshGitState()
                    self.refreshWorktreeGitSummaries()
                    self.refreshWorktreeReview()
                    self.statusMessage = hasBranchDiff
                        ? String(localized: "worktree.merge.success", defaultValue: "Merged worktree.", bundle: .module)
                        : String(localized: "worktree.merge.no_changes", defaultValue: "No task commits to merge.", bundle: .module)
                }
            } catch {
                await MainActor.run {
                    self.updateWorktreeStatus(worktree.id, status: .blocked)
                    self.statusMessage = error.localizedDescription
                    self.refreshWorktreeReview()
                }
            }
        }
    }

    private func managedWorktreeURL(project: Project, branchName: String) -> URL {
        let root = AppRuntimePaths.appSupportRootURL() ?? FileManager.default.temporaryDirectory
        let slug = worktreeSlug(branchName)
        return root
            .appendingPathComponent("worktrees", isDirectory: true)
            .appendingPathComponent(project.id.uuidString, isDirectory: true)
            .appendingPathComponent(slug, isDirectory: true)
    }

    private func worktreeSlug(_ branchName: String) -> String {
        let mapped = branchName.lowercased().map { character -> Character in
            if character.isLetter || character.isNumber {
                return character
            }
            return "-"
        }
        let slug = String(mapped)
            .replacingOccurrences(of: "-+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return slug.isEmpty ? "worktree-\(UUID().uuidString.prefix(8))" : slug
    }

    private func worktreeDisplayName(forBranch branchName: String) -> String {
        let trimmed = branchName.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.split(separator: "/").last.map(String.init) ?? trimmed
    }

    private static func worktreeTimestampSlug() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter.string(from: Date())
    }

    private static func worktreeTaskCommitMessage(_ task: WorktreeTask) -> String {
        "Task: \(task.title)"
    }

    private static func worktreeMergeCommitMessage(_ task: WorktreeTask) -> String {
        "Merge task: \(task.title)"
    }

    nonisolated private static func workingTreeAuditChecks(
        service: GitService,
        worktreePath: String,
        changeCount: Int
    ) -> [WorktreeReviewCheck] {
        let state = (try? service.repositoryState(at: worktreePath)) ?? nil
        let branch = state?.branch.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let hasDiff = changeCount > 0

        return [
            WorktreeReviewCheck(
                id: "audit-branch",
                title: String(localized: "worktree.review.check.audit_branch", defaultValue: "Branch", bundle: .module),
                detail: branch.isEmpty
                    ? String(localized: "worktree.branch.current", defaultValue: "current branch", bundle: .module)
                    : branch,
                severity: .ok
            ),
            WorktreeReviewCheck(
                id: "audit-changes",
                title: String(localized: "worktree.review.check.audit_changes", defaultValue: "Uncommitted Changes", bundle: .module),
                detail: hasDiff
                    ? String(
                        format: String(localized: "worktree.review.check.audit_changes_found_format", defaultValue: "%@ file(s) need review.", bundle: .module),
                        "\(changeCount)"
                    )
                    : String(localized: "worktree.review.check.audit_changes_clean", defaultValue: "Working tree is clean.", bundle: .module),
                severity: hasDiff ? .ok : .warning
            )
        ]
    }

    nonisolated private static func worktreeReviewChecks(
        service: GitService,
        rootProjectPath: String,
        worktreePath: String,
        baseBranch: String,
        taskBranch: String,
        baseCommit: String?,
        hasDiff: Bool
    ) -> [WorktreeReviewCheck] {
        var checks: [WorktreeReviewCheck] = []

        let baseDirty = (try? service.hasUncommittedChanges(at: rootProjectPath)) ?? false
        checks.append(
            WorktreeReviewCheck(
                id: "base-dirty",
                title: String(localized: "worktree.review.check.base_clean", defaultValue: "Base workspace", bundle: .module),
                detail: baseDirty
                    ? String(localized: "worktree.review.check.base_dirty", defaultValue: "Base workspace has uncommitted changes.", bundle: .module)
                    : String(localized: "worktree.review.check.base_clean.detail", defaultValue: "Base workspace is clean.", bundle: .module),
                severity: baseDirty ? .blocking : .ok
            )
        )

        let worktreeDirty = (try? service.hasUncommittedChanges(at: worktreePath)) ?? false
        checks.append(
            WorktreeReviewCheck(
                id: "worktree-dirty",
                title: String(localized: "worktree.review.check.worktree_changes", defaultValue: "Task changes", bundle: .module),
                detail: worktreeDirty
                    ? String(localized: "worktree.review.check.worktree_dirty", defaultValue: "Worktree has uncommitted changes that will be committed before merge.", bundle: .module)
                    : String(localized: "worktree.review.check.worktree_clean", defaultValue: "No uncommitted worktree changes.", bundle: .module),
                severity: worktreeDirty ? .warning : .ok
            )
        )

        let conflictPaths = (try? service.mergeConflictPaths(from: baseBranch, to: taskBranch, at: rootProjectPath)) ?? []
        checks.append(
            WorktreeReviewCheck(
                id: "merge-conflicts",
                title: String(localized: "worktree.review.check.conflicts", defaultValue: "Conflicts", bundle: .module),
                detail: conflictPaths.isEmpty
                    ? String(localized: "worktree.review.check.conflicts_clean", defaultValue: "No committed merge conflicts detected.", bundle: .module)
                    : String(
                        format: String(localized: "worktree.review.check.conflicts_found_format", defaultValue: "%@ conflict path(s) detected before merge.", bundle: .module),
                        "\(conflictPaths.count)"
                    ),
                severity: conflictPaths.isEmpty ? .ok : .blocking
            )
        )

        checks.append(
            WorktreeReviewCheck(
                id: "diff",
                title: String(localized: "worktree.review.check.diff", defaultValue: "Diff", bundle: .module),
                detail: hasDiff
                    ? String(localized: "worktree.review.check.diff_present", defaultValue: "Changes are available for review.", bundle: .module)
                    : String(localized: "worktree.review.check.diff_empty", defaultValue: "No changes relative to the base branch.", bundle: .module),
                severity: hasDiff ? .ok : .warning
            )
        )

        if let baseCommit,
           let currentBaseCommit = (try? service.commitHash(ref: baseBranch, at: rootProjectPath)) ?? nil,
           currentBaseCommit != baseCommit {
            checks.append(
                WorktreeReviewCheck(
                    id: "base-drift",
                    title: String(localized: "worktree.review.check.base_drift", defaultValue: "Base commit", bundle: .module),
                    detail: String(localized: "worktree.review.check.base_drift.detail", defaultValue: "Base branch moved after this worktree was created.", bundle: .module),
                    severity: .warning
                )
            )
        }

        let upstream = (try? service.currentBranchUpstream(at: worktreePath)) ?? nil
        let remote = (try? service.remoteSyncState(at: worktreePath)) ?? .empty
        if upstream == nil {
            checks.append(
                WorktreeReviewCheck(
                    id: "branch-upstream",
                    title: String(localized: "worktree.review.check.remote", defaultValue: "Remote", bundle: .module),
                    detail: String(localized: "worktree.review.check.remote_missing", defaultValue: "Task branch has no upstream remote branch.", bundle: .module),
                    severity: .warning
                )
            )
        } else if remote.outgoingCount > 0 {
            checks.append(
                WorktreeReviewCheck(
                    id: "branch-unpushed",
                    title: String(localized: "worktree.review.check.remote", defaultValue: "Remote", bundle: .module),
                    detail: String(
                        format: String(localized: "worktree.review.check.remote_unpushed_format", defaultValue: "%@ local commit(s) have not been pushed.", bundle: .module),
                        "\(remote.outgoingCount)"
                    ),
                    severity: .warning
                )
            )
        } else {
            checks.append(
                WorktreeReviewCheck(
                    id: "branch-upstream",
                    title: String(localized: "worktree.review.check.remote", defaultValue: "Remote", bundle: .module),
                    detail: String(localized: "worktree.review.check.remote_clean", defaultValue: "Task branch is aligned with its upstream.", bundle: .module),
                    severity: .ok
                )
            )
        }

        return checks
    }

    @discardableResult
    private func setWorktreeStatus(_ worktreeID: UUID, status: ProjectWorktreeTaskStatus, now: Date) -> Bool {
        guard let index = worktrees.firstIndex(where: { $0.id == worktreeID }) else {
            return false
        }

        var worktreeChanged = false
        var taskChanged = false
        if worktrees[index].status != status {
            worktrees[index].status = status
            worktreeChanged = true
        }

        if let taskIndex = worktreeTasks.firstIndex(where: { $0.worktreeID == worktreeID }) {
            if worktreeTasks[taskIndex].status != status {
                worktreeTasks[taskIndex].status = status
                taskChanged = true
            }
            switch status {
            case .ready, .done, .merged:
                if worktreeTasks[taskIndex].completedAt == nil {
                    worktreeTasks[taskIndex].completedAt = now
                    taskChanged = true
                }
            case .planning, .running:
                if worktreeTasks[taskIndex].startedAt == nil {
                    worktreeTasks[taskIndex].startedAt = now
                    taskChanged = true
                }
                if worktreeTasks[taskIndex].completedAt != nil {
                    worktreeTasks[taskIndex].completedAt = nil
                    taskChanged = true
                }
            case .todo, .waiting, .review, .blocked, .archived:
                break
            }
            if taskChanged || worktreeChanged {
                worktreeTasks[taskIndex].updatedAt = now
            }
        }

        if worktreeChanged || taskChanged {
            worktrees[index].updatedAt = now
        }
        return worktreeChanged || taskChanged
    }

    private func runtimeTaskStatus(for worktreeID: UUID) -> ProjectWorktreeTaskStatus? {
        switch aiSessionStore.projectPhase(projectID: worktreeID) {
        case .running, .loading:
            return .running
        case .waitingInput:
            return .waiting
        case .completed(_, _, let exitCode):
            return exitCode == nil ? .done : .blocked
        case .idle:
            guard let completedPhase = aiSessionStore.completedPhase(projectID: worktreeID) else {
                return nil
            }
            if case .completed(_, _, let exitCode) = completedPhase {
                return exitCode == nil ? .done : .blocked
            } else {
                return nil
            }
        }
    }

    func isWorktreeMergeCandidateStatus(_ status: ProjectWorktreeTaskStatus) -> Bool {
        switch status.visibleStatus {
        case .ready, .review, .done, .blocked:
            return true
        case .todo, .planning, .running, .waiting, .merged, .archived:
            return false
        }
    }

}

private enum WorktreeMergeError: LocalizedError {
    case dirtyBase
    case mergeConflicts([String])

    var errorDescription: String? {
        switch self {
        case .dirtyBase:
            return String(localized: "worktree.merge.base_dirty", defaultValue: "The base worktree has uncommitted changes. Clean it before merging.", bundle: .module)
        case .mergeConflicts(let paths):
            let pathList = paths.prefix(6).joined(separator: ", ")
            return String(
                format: String(localized: "worktree.merge.conflicts_format", defaultValue: "Merge conflicts detected before merge: %@.", bundle: .module),
                pathList
            )
        }
    }
}
