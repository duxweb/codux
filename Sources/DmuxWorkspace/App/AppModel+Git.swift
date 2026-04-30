import AppKit
import Foundation

extension AppModel {
    func isGitEntrySelected(_ entry: GitFileEntry) -> Bool {
        gitStore.isEntrySelected(entry)
    }

    func toggleGitEntrySelection(_ entry: GitFileEntry) {
        gitStore.toggleEntrySelection(entry)
    }

    func selectGitEntry(_ entry: GitFileEntry, extendingRange: Bool) {
        gitStore.selectEntry(entry, in: gitState, extendingRange: extendingRange)
    }

    func prepareGitEntryContextMenu(_ entry: GitFileEntry) {
        if selectedGitEntryIDs.contains(entry.id) {
            gitStore.prepareEntryContextMenu(entry)
            loadDiff(for: entry)
            return
        }

        selectGitEntry(entry, extendingRange: false)
        loadDiff(for: entry)
    }

    func prepareGitCommitContextMenu(_ commit: GitCommitEntry) {
        gitStore.selectCommit(commit)
    }

    var selectedGitEntriesForContextMenu: [GitFileEntry] {
        guard let gitState else { return [] }
        let all = flatGitEntries(from: gitState)
        return all.filter { selectedGitEntryIDs.contains($0.id) }
    }

    func copyGitPaths(_ entries: [GitFileEntry]) {
        let text = entries.map(\.path).joined(separator: "\n")
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        statusMessage = entries.count > 1 ? String(localized: "git.files.copy_selected_paths.success", defaultValue: "Copied selected file paths.", bundle: .module) : String(localized: "git.files.copy_path.success", defaultValue: "Copied file path.", bundle: .module)
    }

    func openGitDiffWindow(for entry: GitFileEntry) {
        guard let project = selectedProject else { return }
        GitFileDiffWindowPresenter.show(entry: entry, project: project)
    }

    func addGitEntriesToIgnore(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }

        let repositoryPath = project.path
        isGitLoading = true

        Task.detached {
            let service = GitService()
            do {
                try service.appendToGitignore(paths, at: repositoryPath)
                await MainActor.run {
                    self.statusMessage = paths.count > 1 ? String(localized: "git.ignore.added", defaultValue: "Added to .gitignore.", bundle: .module) : String(localized: "git.ignore.added", defaultValue: "Added to .gitignore.", bundle: .module)
                    self.refreshGitState()
                }
            } catch {
                await MainActor.run {
                    self.isGitLoading = false
                    self.statusMessage = error.localizedDescription
                }
            }
        }
    }

    func revealGitEntriesInFinder(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let urls = entries.map { URL(fileURLWithPath: project.path).appendingPathComponent($0.path) }
        guard !urls.isEmpty else { return }
        NSWorkspace.shared.activateFileViewerSelecting(urls)
    }

    func mergeBranchIntoCurrent(_ branch: String) {
        guard let project = selectedProject else { return }
        let currentBranch = gitState?.branch ?? String(localized: "git.branch.current_label", defaultValue: "Current Branch", bundle: .module)
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "git.branch.merge.title", defaultValue: "Merge Branch", bundle: .module),
            message: String(
                format: String(localized: "git.branch.merge.message_format", defaultValue: "Merge %@ into %@.", bundle: .module),
                branch,
                currentBranch
            ),
            icon: "arrow.merge",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.merge", defaultValue: "Merge", bundle: .module),
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            let path = project.path
            self.isGitLoading = true

            Task.detached {
                let service = GitService()
                do {
                    try service.merge(branch: branch, intoCurrentBranchAt: path)
                    await MainActor.run {
                        self.statusMessage = String(
                            format: String(localized: "git.branch.merge.success_format", defaultValue: "Merged branch %@.", bundle: .module),
                            branch
                        )
                        self.refreshGitState()
                    }
                } catch {
                    await MainActor.run {
                        self.isGitLoading = false
                        self.statusMessage = error.localizedDescription
                    }
                }
            }
        }
    }

    func syncMainBranchIntoCurrent() {
        let mainBranch = gitBranches.contains("main") ? "main" : (gitBranches.contains("master") ? "master" : nil)
        guard let mainBranch else {
            statusMessage = String(localized: "git.main_branch.not_found", defaultValue: "Main branch not found.", bundle: .module)
            return
        }

        if gitState?.branch == mainBranch {
            statusMessage = String(localized: "git.main_branch.already_on", defaultValue: "Already on the main branch.", bundle: .module)
            return
        }

        let currentBranch = gitState?.branch ?? String(localized: "git.branch.current_label", defaultValue: "Current Branch", bundle: .module)
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = ConfirmDialogState(
            title: String(localized: "git.main_branch.sync.title", defaultValue: "Sync Main Branch", bundle: .module),
            message: String(
                format: String(localized: "git.main_branch.sync.message_format", defaultValue: "Merge the latest changes from %@ into %@.", bundle: .module),
                mainBranch,
                currentBranch
            ),
            icon: "arrow.triangle.merge",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.sync", defaultValue: "Sync", bundle: .module),
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.mergeBranchIntoCurrent(mainBranch)
        }
    }

    func refreshGitState(
        presentation: GitStore.RefreshPresentation = .fullScreen,
        includesRemoteSync: Bool = true
    ) {
        gitStore.refresh(
            project: selectedProject,
            presentation: presentation,
            includesRemoteSync: includesRemoteSync
        )
    }

    func initializeGitRepository() {
        guard let project = selectedProject else { return }
        gitStore.initializeRepository(project: project) { self.statusMessage = $0 }
    }

    func restoreCachedGitPanelIfAvailable(for projectID: UUID) {
        gitStore.restoreCachedState(for: projectID)
    }

    func checkoutGitBranch(_ branch: String) {
        guard let project = selectedProject else { return }
        gitStore.checkoutBranch(branch, project: project) { self.statusMessage = $0 }
    }

    func checkoutRemoteGitBranch(_ remoteBranch: String) {
        guard let project = selectedProject else { return }
        gitStore.checkoutRemoteBranch(remoteBranch, project: project) { self.statusMessage = $0 }
    }

    func selectGitCommit(_ commit: GitCommitEntry) {
        gitStore.selectCommit(commit)
    }

    func copyGitCommitHash(_ commit: GitCommitEntry) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(commit.hash, forType: .string)
        statusMessage = String(localized: "git.commit.hash.copied", defaultValue: "Copied commit hash.", bundle: .module)
    }

    func checkoutGitCommit(_ commit: GitCommitEntry) {
        guard let project = selectedProject else { return }
        gitStore.checkoutCommit(commit, project: project) { self.statusMessage = $0 }
    }

    func revertGitCommit(_ commit: GitCommitEntry) {
        guard let project = selectedProject else { return }
        gitStore.revertCommit(commit, project: project) { self.statusMessage = $0 }
    }

    func createBranch(from commit: GitCommitEntry) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = GitInputDialogState(
            kind: .createBranchFromCommit(commit.hash),
            title: String(localized: "git.branch.create_from_commit.title", defaultValue: "Create Branch from Commit", bundle: .module),
            message: String(localized: "git.branch.new.message", defaultValue: "Enter a new branch name.", bundle: .module),
            placeholder: "feature/from-commit",
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            value: "",
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] branchName in
            guard let self, let branchName else { return }
            self.createGitBranch(named: branchName, from: commit.hash)
        }
    }

    func restoreGitCommit(_ commit: GitCommitEntry, forceRemote: Bool) {
        guard let project = selectedProject else { return }

        let dialog = ConfirmDialogState(
            title: forceRemote ? String(localized: "git.history.restore_remote", defaultValue: "Restore This Revision Remotely", bundle: .module) : String(localized: "git.history.restore_local", defaultValue: "Restore This Revision Locally", bundle: .module),
            message: forceRemote
                ? String(localized: "git.history.restore_remote.message", defaultValue: "Reset the current branch to this revision and overwrite the remote branch.", bundle: .module)
                : String(localized: "git.history.restore_local.message", defaultValue: "Reset the current branch to this revision locally only.", bundle: .module),
            icon: forceRemote ? "arrow.uturn.backward.circle.fill" : "clock.arrow.circlepath",
            iconColor: AppTheme.warning,
            primaryTitle: forceRemote ? String(localized: "git.history.restore_remote.action", defaultValue: "Remote Restore", bundle: .module) : String(localized: "git.history.restore_local.action", defaultValue: "Local Restore", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.gitStore.restoreCommit(commit, forceRemote: forceRemote, project: project) { self.statusMessage = $0 }
        }
    }

    func revealRepositoryInFinder() {
        guard let project = selectedProject else { return }
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: project.path)])
    }

    func undoLastGitCommit() {
        guard let project = selectedProject else { return }
        let path = project.path
        let headCommitPushed = (try? gitService.isHeadCommitPushed(at: path)) ?? false

        let dialog = ConfirmDialogState(
            title: String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module),
            message: headCommitPushed
                ? String(localized: "git.history.undo_last_commit.remote_notice", defaultValue: "Undo the last commit but keep the file changes and staging state. This commit may already be on the remote, so you might need to force push later.", bundle: .module)
                : String(localized: "git.history.undo_last_commit.local_notice", defaultValue: "Undo the last commit but keep the file changes and staging state.", bundle: .module),
            icon: "arrow.uturn.backward",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.undo", defaultValue: "Undo", bundle: .module),
            primaryTint: AppTheme.warning,
            cancelTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self, result == .primary else { return }
            self.gitStore.undoLastCommit(headCommitPushed: headCommitPushed, project: project, onStatus: { self.statusMessage = $0 }, onRewriteWarning: {
                self.presentPostCommitRewriteAlert(actionTitle: String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module), repositoryPath: path)
            })
        }
    }

    func editLastGitCommitMessage() {
        guard let project = selectedProject else { return }
        let path = project.path
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        let currentMessage = (try? gitService.lastCommitMessage(at: path)) ?? ""
        let headCommitPushed = (try? gitService.isHeadCommitPushed(at: path)) ?? false

        let dialog = GitInputDialogState(
            kind: .editLastCommitMessage(headCommitPushed: headCommitPushed),
            title: String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module),
            message: headCommitPushed
                ? String(localized: "git.commit.edit_last_message.remote_notice", defaultValue: "Only change the latest commit message. This commit may already be on the remote, so you might need to force push after editing it.", bundle: .module)
                : String(localized: "git.commit.edit_last_message.notice", defaultValue: "Only change the latest commit message.", bundle: .module),
            placeholder: String(localized: "git.commit.edit_last_message.placeholder", defaultValue: "Enter a new commit message", bundle: .module),
            confirmTitle: String(localized: "common.save", defaultValue: "Save", bundle: .module),
            value: currentMessage,
            isMultiline: true
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] message in
            guard let self, let message else { return }
            self.amendLastGitCommitMessage(message, headCommitPushed: headCommitPushed)
        }
    }

    func createGitBranch() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }
        let dialog = GitInputDialogState(
            kind: .createBranch,
            title: String(localized: "git.branch.new", defaultValue: "New Branch", bundle: .module),
            message: String(localized: "git.branch.new.message", defaultValue: "Enter a new branch name.", bundle: .module),
            placeholder: "feature/xxx",
            confirmTitle: String(localized: "common.create", defaultValue: "Create", bundle: .module),
            value: "",
            isMultiline: false
        )

        GitInputPanelPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] branchName in
            guard let self, let branchName else { return }
            self.createGitBranch(named: branchName)
        }
    }

    func discard(_ entry: GitFileEntry) {
        guard let project = selectedProject else { return }
        gitStore.discardEntry(entry, project: project) { self.statusMessage = $0 }
    }

    func discardSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind != .staged }
        guard !entries.isEmpty else { return }
        gitStore.discardEntries(entries, project: project, successMessage: String(localized: "git.discard.selected.success", defaultValue: "Discarded selected changes.", bundle: .module)) { self.statusMessage = $0 }
    }

    func stageEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.filter { $0.kind != .staged }.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.stagePaths(paths, project: project, successMessage: String(localized: "git.stage.section.success", defaultValue: "Staged files in this section.", bundle: .module)) { self.statusMessage = $0 }
    }

    func unstageEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        let paths = Array(Set(entries.filter { $0.kind == .staged }.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.unstagePaths(paths, project: project, successMessage: String(localized: "git.unstage.section.success", defaultValue: "Unstaged files in this section.", bundle: .module)) { self.statusMessage = $0 }
    }

    func discardEntries(_ entries: [GitFileEntry]) {
        guard let project = selectedProject else { return }
        guard !entries.isEmpty else { return }
        gitStore.discardEntries(entries, project: project, successMessage: String(localized: "git.discard.section.success", defaultValue: "Discarded changes in this section.", bundle: .module)) { self.statusMessage = $0 }
    }

    func loadDiff(for entry: GitFileEntry) {
        gitStore.loadDiff(for: entry, project: selectedProject)
    }

    func stageAllChanges() {
        guard let project = selectedProject else {
            return
        }
        gitStore.stageAll(project: project) { self.statusMessage = $0 }
    }

    func unstageAllChanges() {
        guard let project = selectedProject else {
            return
        }
        gitStore.unstageAll(project: project) { self.statusMessage = $0 }
    }

    func stageSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind == .changed || $0.kind == .untracked }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.stagePaths(paths, project: project, successMessage: String(localized: "git.stage.selected.success", defaultValue: "Staged selected files.", bundle: .module)) { self.statusMessage = $0 }
    }

    func unstageSelectedChanges() {
        guard let project = selectedProject, let gitState else { return }
        let entries = selectedEntries(in: gitState).filter { $0.kind == .staged }
        let paths = Array(Set(entries.map(\.path))).sorted()
        guard !paths.isEmpty else { return }
        gitStore.unstagePaths(paths, project: project, successMessage: String(localized: "git.unstage.selected.success", defaultValue: "Unstaged selected files.", bundle: .module)) { self.statusMessage = $0 }
    }

    func stage(_ entry: GitFileEntry) {
        guard let project = selectedProject else {
            return
        }
        gitStore.stagePaths([entry.path], project: project, successMessage: String(format: String(localized: "git.stage.file.success_format", defaultValue: "Staged %@.", bundle: .module), entry.path)) { self.statusMessage = $0 }
    }

    func unstage(_ entry: GitFileEntry) {
        guard let project = selectedProject else {
            return
        }
        gitStore.unstagePaths([entry.path], project: project, successMessage: String(format: String(localized: "git.unstage.file.success_format", defaultValue: "Unstaged %@.", bundle: .module), entry.path)) { self.statusMessage = $0 }
    }

    func commitChanges() {
        performCommitAction(.commit)
    }

    func performCommitAction(_ action: GitCommitAction) {
        guard let project = selectedProject else {
            return
        }

        let trimmed = commitMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            statusMessage = String(localized: "git.commit.message.empty", defaultValue: "Commit message cannot be empty.", bundle: .module)
            return
        }

        gitStore.commit(message: trimmed, action: action, project: project, onStatus: { self.statusMessage = $0 }, onSuccess: {
            self.commitMessage = ""
            self.selectedGitEntry = nil
            self.selectedGitEntryIDs.removeAll()
            self.gitDiffText = String(localized: "git.commit.completed", defaultValue: "Commit completed.", bundle: .module)
        })
    }

    func generateCommitMessage() {
        guard let gitState else {
            statusMessage = String(localized: "git.commit.generate.unavailable", defaultValue: "There are no changes available to generate a commit message.", bundle: .module)
            return
        }

        let fileNames = Array(Set((gitState.staged + gitState.changes + gitState.untracked).map(\.path))).sorted()
        guard !fileNames.isEmpty else {
            statusMessage = String(localized: "git.commit.generate.unavailable", defaultValue: "There are no changes available to generate a commit message.", bundle: .module)
            return
        }

        isGeneratingCommitMessage = true
        let summary = fileNames.prefix(3).joined(separator: ", ")
        let suffix = fileNames.count > 3
            ? String(format: String(localized: "git.commit.generate.more_files_format", defaultValue: " and %@ more files", bundle: .module), "\(fileNames.count)")
            : ""
        commitMessage = String(format: String(localized: "git.commit.generate.summary_format", defaultValue: "Update %@%@ Git workspace interaction", bundle: .module), summary, suffix)
        isGeneratingCommitMessage = false
        statusMessage = String(localized: "git.commit.generate.success", defaultValue: "Generated commit message.", bundle: .module)
    }

    func updateGitRemoteSyncPolling() {
        updateGitStatusAutoRefresh()
        guard NSApplication.shared.isActive, rightPanel == .git, selectedProject != nil else {
            stopGitRemoteSyncPolling()
            return
        }
        startGitRemoteSyncPolling()
    }

    private func createGitBranch(named branchName: String, from commitHash: String? = nil) {
        guard let project = selectedProject else { return }
        gitStore.createBranch(branchName, from: commitHash, project: project) { self.statusMessage = $0 }
    }

    private func amendLastGitCommitMessage(_ message: String, headCommitPushed: Bool) {
        guard let project = selectedProject else { return }
        let path = project.path
        gitStore.amendLastCommitMessage(message, headCommitPushed: headCommitPushed, project: project, onStatus: { self.statusMessage = $0 }, onRewriteWarning: {
            self.presentPostCommitRewriteAlert(actionTitle: String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module), repositoryPath: path)
        })
    }

    private func entries(for kind: GitFileKind, in state: GitRepositoryState) -> [GitFileEntry] {
        switch kind {
        case .staged:
            return state.staged
        case .changed:
            return state.changes
        case .untracked:
            return state.untracked
        }
    }

    private func selectedEntries(in state: GitRepositoryState) -> [GitFileEntry] {
        let all = state.staged + state.changes + state.untracked
        return all.filter { selectedGitEntryIDs.contains($0.id) }
    }

    private func flatGitEntries(from state: GitRepositoryState) -> [GitFileEntry] {
        state.staged + state.changes + state.untracked
    }

    private func presentPostCommitRewriteAlert(actionTitle: String, repositoryPath: String) {
        let dialog = ConfirmDialogState(
            title: actionTitle,
            message: String(localized: "git.history.rewrite_warning.message", defaultValue: "Local history has changed. If the remote still contains the old commits, you may need to force push to sync it.", bundle: .module),
            icon: "arrow.trianglehead.2.clockwise.rotate.90",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "git.history.rewrite_warning.defer", defaultValue: "Handle Later", bundle: .module),
            secondaryTitle: String(localized: "git.history.rewrite_warning.force_push", defaultValue: "Force Push Now", bundle: .module)
        )

        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            return
        }

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else { return }
            if result == .secondary {
                guard let project = self.selectedProject else { return }
                let savedCredential = self.credentialForSelectedProjectRemote()
                self.gitStore.performRemoteAction(GitRemoteAction.forcePush, project: project, credential: savedCredential, onStatus: { self.statusMessage = $0 }, onAuthRequired: { completion in self.promptForGitCredential(completion: completion) }, onAuthSucceeded: { credential in
                    if let remoteURL = try? self.gitService.originURL(at: project.path), !remoteURL.isEmpty {
                        self.gitCredentialStore.save(credential, for: remoteURL)
                    }
                }, onConflict: {
                    self.presentRemoteSyncConflictAlert(repositoryPath: repositoryPath)
                })
            } else {
                self.refreshGitState()
            }
        }
    }

    private func refreshGitRemoteSyncState(projectPath: String, projectID: UUID) {
        gitStore.refreshRemoteSyncState(projectPath: projectPath, projectID: projectID)
    }

    private func startGitRemoteSyncPolling() {
        gitStore.startRemoteSyncPolling(
            selectedProject: { self.selectedProject },
            isEnabled: { NSApp.isActive && self.rightPanel == .git }
        )
    }

    private func startGitStatusAutoRefresh() {
        gitStore.startStatusAutoRefresh(
            selectedProject: { self.selectedProject },
            isEnabled: { self.rightPanel == .git }
        )
    }

    private func stopGitRemoteSyncPolling() {
        gitStore.stopRemoteSyncPolling()
    }

    private func stopGitStatusAutoRefresh() {
        gitStore.stopStatusAutoRefresh()
    }

    private func updateGitStatusAutoRefresh() {
        guard rightPanel == .git, selectedProject != nil else {
            stopGitStatusAutoRefresh()
            return
        }
        startGitStatusAutoRefresh()
    }

}
