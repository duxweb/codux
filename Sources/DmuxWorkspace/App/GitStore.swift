import Foundation
import Observation

@MainActor
@Observable
final class GitStore {
    enum RefreshPresentation {
        case fullScreen
        case preserveVisibleState
    }

    private struct CachedGitPanelEntry {
        var projectID: UUID
        var state: GitPanelState
        var updatedAt: Date
    }

    var panelState = GitPanelState.empty

    private var cachedPanels = RecentProjectCache<CachedGitPanelEntry>()
    private var remoteSyncTimer: Timer?
    private var remoteSyncInterval: TimeInterval = 60
    private var selectedProjectProvider: (@MainActor () -> Project?)?
    private var remoteSyncEnabledProvider: (@MainActor () -> Bool)?

    private func beginPanelOperation(allowsPreservingVisibleState: Bool = true) {
        if allowsPreservingVisibleState, panelState.gitState != nil {
            panelState.isGitLoading = false
            panelState.refreshState = .idle
        } else {
            panelState.isGitLoading = true
        }
    }

    func refresh(project: Project?, presentation: RefreshPresentation = .fullScreen) {
        guard let project else {
            panelState = .empty
            panelState.gitDiffText = String(localized: "git.empty.select_project", defaultValue: "Add or select a project to view Git status.", bundle: .module)
            return
        }

        let projectID = project.id
        let path = project.path
        let selectedIDsSnapshot = panelState.selectedGitEntryIDs
        let shouldShowBlockingLoading = presentation == .fullScreen || panelState.gitState == nil
        if shouldShowBlockingLoading {
            panelState.isGitLoading = true
        } else {
            panelState.isGitLoading = false
            panelState.refreshState = .idle
        }

        Task.detached {
            let service = GitService()
            do {
                let state = try service.repositoryState(at: path)
                await MainActor.run {
                    self.panelState.gitState = state
                    self.panelState.isGitLoading = false
                    self.panelState.refreshState = .idle
                    if state == nil {
                        self.panelState.gitDiffText = String(localized: "git.repository.not_repository", bundle: .module)
                        self.panelState.selectedGitEntryIDs.removeAll()
                        self.panelState.gitHistory = []
                        self.panelState.gitBranches = []
                        self.panelState.gitBranchUpstreams = [:]
                        self.panelState.gitRemoteBranches = []
                        self.panelState.gitRemotes = []
                        self.panelState.gitRemoteSyncState = .empty
                    } else {
                        self.panelState.gitDiffText = self.panelState.selectedGitEntry == nil ? String(localized: "git.diff.select_file", bundle: .module) : self.panelState.gitDiffText
                        let validIDs = Set((state?.staged ?? []).map(\.id) + (state?.changes ?? []).map(\.id) + (state?.untracked ?? []).map(\.id))
                        self.panelState.selectedGitEntryIDs = selectedIDsSnapshot.intersection(validIDs)
                        self.refreshHistory(projectPath: path, projectID: projectID)
                        self.refreshBranches(projectPath: path, projectID: projectID)
                        self.refreshRemoteSyncState(projectPath: path, projectID: projectID)
                    }
                    self.cacheState(for: projectID)
                }
            } catch {
                await MainActor.run {
                    self.panelState.gitState = nil
                    self.panelState.gitRemoteSyncState = .empty
                    self.panelState.isGitLoading = false
                    self.panelState.refreshState = .failed(error.localizedDescription)
                    self.panelState.gitDiffText = error.localizedDescription
                    self.panelState.gitHistory = []
                    self.panelState.gitBranches = []
                    self.panelState.gitBranchUpstreams = [:]
                    self.panelState.gitRemoteBranches = []
                    self.panelState.gitRemotes = []
                    self.cacheState(for: projectID)
                }
            }
        }
    }

    func initializeRepository(project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation(allowsPreservingVisibleState: false)
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.initializeRepository(at: path)
                await MainActor.run {
                    onStatus(String(localized: "git.repository.initialized", bundle: .module))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func cloneRepository(project: Project, remoteURL: String, credential: GitCredential? = nil, onStatus: @escaping @MainActor (String) -> Void, onAuthRequired: @escaping @MainActor (@escaping (GitCredential?) -> Void) -> Void, onAuthSucceeded: @escaping @MainActor (GitCredential) -> Void) {
        beginPanelOperation(allowsPreservingVisibleState: false)
        panelState.gitOperationStatusText = String(localized: "git.clone.preparing", defaultValue: "Preparing to clone the repository...", bundle: .module)
        panelState.gitOperationProgress = 0
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.clone(remoteURL, into: path, credential: credential) { line, progressValue in
                    Task { @MainActor in
                        self.panelState.gitOperationStatusText = line
                        if let progressValue {
                            self.panelState.gitOperationProgress = progressValue
                        }
                    }
                }
                await MainActor.run {
                    if let credential {
                        onAuthSucceeded(credential)
                    }
                    self.panelState.gitOperationStatusText = nil
                    self.panelState.gitOperationProgress = nil
                    onStatus(String(localized: "git.repository.cloned", bundle: .module))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    if case GitServiceError.authenticationRequired = error,
                       credential == nil {
                        onAuthRequired { credential in
                            guard let credential else {
                                self.panelState.isGitLoading = false
                                self.panelState.gitOperationStatusText = String(localized: "git.auth.cancelled", bundle: .module)
                                self.panelState.gitOperationProgress = nil
                                onStatus(String(localized: "git.auth.cancelled", bundle: .module))
                                return
                            }
                            self.cloneRepository(project: project, remoteURL: remoteURL, credential: credential, onStatus: onStatus, onAuthRequired: onAuthRequired, onAuthSucceeded: onAuthSucceeded)
                        }
                        return
                    }
                    self.panelState.isGitLoading = false
                    self.panelState.gitOperationStatusText = error.localizedDescription
                    self.panelState.gitOperationProgress = nil
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func restoreCachedState(for projectID: UUID) {
        guard let entry = cachedPanels.value(for: projectID) else {
            panelState.refreshState = .refreshing
            panelState.selectedGitEntry = nil
            panelState.selectedGitCommitHash = nil
            panelState.gitDiffText = String(localized: "git.diff.select_file", bundle: .module)
            panelState.selectedGitEntryIDs.removeAll()
            panelState.gitHistory = []
            panelState.gitBranches = []
            panelState.gitRemoteBranches = []
            panelState.gitRemotes = []
            panelState.gitRemoteSyncState = .empty
            return
        }

        panelState = entry.state
        panelState.refreshState = .showingCached
    }

    func updateRemoteOperation(_ operation: GitRemoteOperation?) {
        panelState.activeGitRemoteOperation = operation
    }

    func updateSelectionState(entry: GitFileEntry?, entryIDs: Set<String>, commitHash: String?, anchorID: String?) {
        panelState.selectedGitEntry = entry
        panelState.selectedGitEntryIDs = entryIDs
        panelState.selectedGitCommitHash = commitHash
        panelState.gitSelectionAnchorID = anchorID
    }

    func isEntrySelected(_ entry: GitFileEntry) -> Bool {
        panelState.selectedGitEntryIDs.contains(entry.id)
    }

    func toggleEntrySelection(_ entry: GitFileEntry) {
        if panelState.selectedGitEntryIDs.contains(entry.id) {
            panelState.selectedGitEntryIDs.remove(entry.id)
        } else {
            panelState.selectedGitEntryIDs.insert(entry.id)
        }
        panelState.selectedGitEntry = entry
        panelState.gitSelectionAnchorID = entry.id
    }

    func selectEntry(_ entry: GitFileEntry, in gitState: GitRepositoryState?, extendingRange: Bool) {
        guard let gitState else {
            panelState.selectedGitEntry = entry
            panelState.selectedGitEntryIDs = [entry.id]
            panelState.gitSelectionAnchorID = entry.id
            return
        }

        panelState.selectedGitEntry = entry
        let allEntries = gitState.staged + gitState.changes + gitState.untracked
        if extendingRange,
           let anchorID = panelState.gitSelectionAnchorID,
           let anchorIndex = allEntries.firstIndex(where: { $0.id == anchorID }),
           let targetIndex = allEntries.firstIndex(where: { $0.id == entry.id }) {
            let lower = min(anchorIndex, targetIndex)
            let upper = max(anchorIndex, targetIndex)
            panelState.selectedGitEntryIDs = Set(allEntries[lower...upper].map(\.id))
        } else {
            panelState.selectedGitEntryIDs = [entry.id]
            panelState.gitSelectionAnchorID = entry.id
        }
    }

    func prepareEntryContextMenu(_ entry: GitFileEntry) {
        panelState.selectedGitEntry = entry
        panelState.gitSelectionAnchorID = entry.id
    }

    func selectCommit(_ commit: GitCommitEntry) {
        panelState.selectedGitCommitHash = commit.hash
    }

    func updateDiffState(text: String, isLoading: Bool) {
        panelState.gitDiffText = text
        panelState.isGitDiffLoading = isLoading
    }

    func loadDiff(for entry: GitFileEntry, project: Project?) {
        guard let project else {
            return
        }

        panelState.selectedGitEntry = entry
        let projectID = project.id
        let path = project.path
        panelState.isGitDiffLoading = true
        panelState.gitDiffText = String(localized: "git.diff.loading", defaultValue: "Loading diff...", bundle: .module)

        Task.detached {
            let service = GitService()
            do {
                let diff = try service.diff(for: entry, at: path)
                await MainActor.run {
                    guard self.panelState.selectedGitEntry?.id == entry.id else { return }
                    self.panelState.gitDiffText = diff
                    self.panelState.isGitDiffLoading = false
                    self.cacheState(for: projectID)
                }
            } catch {
                await MainActor.run {
                    guard self.panelState.selectedGitEntry?.id == entry.id else { return }
                    self.panelState.gitDiffText = error.localizedDescription
                    self.panelState.isGitDiffLoading = false
                    self.cacheState(for: projectID)
                }
            }
        }
    }

    func startRemoteSyncPolling(selectedProject: @escaping @MainActor () -> Project?, isEnabled: @escaping @MainActor () -> Bool) {
        selectedProjectProvider = selectedProject
        remoteSyncEnabledProvider = isEnabled
        remoteSyncTimer?.invalidate()
        remoteSyncTimer = Timer.scheduledTimer(withTimeInterval: remoteSyncInterval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, isEnabled(), self.panelState.activeGitRemoteOperation == nil, let project = selectedProject() else {
                    return
                }
                self.refreshRemoteSyncState(projectPath: project.path, projectID: project.id)
            }
        }
    }

    func configureRemoteSyncInterval(_ interval: TimeInterval) {
        remoteSyncInterval = max(15, interval)
        if let selectedProjectProvider, let remoteSyncEnabledProvider {
            startRemoteSyncPolling(selectedProject: selectedProjectProvider, isEnabled: remoteSyncEnabledProvider)
        }
    }

    func stopRemoteSyncPolling() {
        remoteSyncTimer?.invalidate()
        remoteSyncTimer = nil
    }

    private func cacheState(for projectID: UUID) {
        cachedPanels.set(CachedGitPanelEntry(projectID: projectID, state: panelState, updatedAt: Date()), for: projectID)
    }

    private func refreshHistory(projectPath: String, projectID: UUID) {
        Task.detached {
            let service = GitService()
            do {
                let history = try service.history(at: projectPath)
                await MainActor.run {
                    self.panelState.gitHistory = history
                    self.cacheState(for: projectID)
                }
            } catch {
                await MainActor.run {
                    self.panelState.gitHistory = []
                    self.cacheState(for: projectID)
                }
            }
        }
    }

    private func refreshBranches(projectPath: String, projectID: UUID) {
        Task.detached {
            let service = GitService()
            do {
                let branches = try service.localBranches(at: projectPath)
                let branchUpstreams = try service.localBranchUpstreams(at: projectPath)
                let remoteBranches = try service.remoteBranches(at: projectPath)
                let remotes = try service.remotes(at: projectPath)
                await MainActor.run {
                    self.panelState.gitBranches = branches
                    self.panelState.gitBranchUpstreams = branchUpstreams
                    self.panelState.gitRemoteBranches = remoteBranches
                    self.panelState.gitRemotes = remotes
                    self.cacheState(for: projectID)
                }
            } catch {
                await MainActor.run {
                    self.panelState.gitBranches = []
                    self.panelState.gitBranchUpstreams = [:]
                    self.panelState.gitRemoteBranches = []
                    self.panelState.gitRemotes = []
                    self.cacheState(for: projectID)
                }
            }
        }
    }

    func refreshRemoteSyncState(projectPath: String, projectID: UUID) {
        Task.detached {
            let service = GitService()
            do {
                try service.fetch(at: projectPath)
                let syncState = try service.remoteSyncState(at: projectPath)
                let remotes = try service.remotes(at: projectPath)
                await MainActor.run {
                    self.panelState.gitRemoteSyncState = syncState
                    self.panelState.gitRemotes = remotes
                    self.cacheState(for: projectID)
                }
            } catch {
            }
        }
    }

    func checkoutBranch(_ branch: String, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.checkout(branch: branch, at: path)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.branch.switch.success_format", bundle: .module), branch))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func checkoutRemoteBranch(_ remoteBranch: String, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                let localName = try service.checkoutRemoteBranch(remoteBranch, at: path)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.remote.branch.checkout_tracking_format", bundle: .module), localName))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func checkoutCommit(_ commit: GitCommitEntry, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.checkout(commit: commit.hash, at: path)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.history.checkout.success_format", bundle: .module), String(commit.hash.prefix(7))))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func revertCommit(_ commit: GitCommitEntry, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.revert(commit: commit.hash, at: path)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.history.revert.success_format", bundle: .module), String(commit.hash.prefix(7))))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func restoreCommit(_ commit: GitCommitEntry, forceRemote: Bool, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.resetCurrentBranch(to: commit.hash, at: path)
                if forceRemote {
                    try service.forcePush(at: path)
                }
                await MainActor.run {
                    onStatus(forceRemote ? String(localized: "git.history.restore.remote_success", bundle: .module) : String(localized: "git.history.restore.local_success", bundle: .module))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func createBranch(_ branchName: String, from commitHash: String? = nil, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                if let commitHash {
                    try service.createBranch(branchName, from: commitHash, at: path)
                } else {
                    try service.createBranch(branchName, at: path)
                }
                await MainActor.run {
                    onStatus(commitHash == nil
                        ? String(format: String(localized: "git.branch.create_and_switch.success_format", bundle: .module), branchName)
                        : String(format: String(localized: "git.branch.create_from_commit.success_format", bundle: .module), branchName))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func stageAll(project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.stageAll(at: path)
                await MainActor.run {
                    onStatus(String(localized: "git.files.stage_all.success", bundle: .module))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func unstageAll(project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.unstageAll(at: path)
                await MainActor.run {
                    onStatus(String(localized: "git.files.unstage_all.success", bundle: .module))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func stagePaths(_ paths: [String], project: Project, successMessage: String, onStatus: @escaping @MainActor (String) -> Void) {
        guard !paths.isEmpty else { return }
        beginPanelOperation()
        let repositoryPath = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.stage(paths, at: repositoryPath)
                await MainActor.run {
                    onStatus(successMessage)
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func unstagePaths(_ paths: [String], project: Project, successMessage: String, onStatus: @escaping @MainActor (String) -> Void) {
        guard !paths.isEmpty else { return }
        beginPanelOperation()
        let repositoryPath = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.unstage(paths, at: repositoryPath)
                await MainActor.run {
                    onStatus(successMessage)
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func discardEntries(_ entries: [GitFileEntry], project: Project, successMessage: String, onStatus: @escaping @MainActor (String) -> Void) {
        guard !entries.isEmpty else { return }
        beginPanelOperation()
        let repositoryPath = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.discard(entries, at: repositoryPath)
                await MainActor.run {
                    onStatus(successMessage)
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func discardEntry(_ entry: GitFileEntry, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let repositoryPath = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.discard(entry, at: repositoryPath)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.files.discard.success_format", bundle: .module), entry.path))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func commit(message: String, action: GitCommitAction, project: Project, onStatus: @escaping @MainActor (String) -> Void, onSuccess: @escaping @MainActor () -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.commit(message: message, at: path)
                switch action {
                case .commit:
                    break
                case .commitAndPush:
                    try service.push(at: path)
                case .commitAndSync:
                    try service.sync(at: path)
                }

                await MainActor.run {
                    onSuccess()
                    onStatus(action.successMessage)
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func amendLastCommitMessage(_ message: String, headCommitPushed: Bool, project: Project, onStatus: @escaping @MainActor (String) -> Void, onRewriteWarning: @escaping @MainActor () -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.amendLastCommitMessage(message, at: path)
                await MainActor.run {
                    onStatus(headCommitPushed
                        ? String(localized: "git.commit.edit_last_message.remote_success", bundle: .module)
                        : String(localized: "git.commit.edit_last_message.success", bundle: .module))
                    if headCommitPushed {
                        onRewriteWarning()
                    } else {
                        self.refresh(project: project, presentation: .preserveVisibleState)
                    }
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func undoLastCommit(headCommitPushed: Bool, project: Project, onStatus: @escaping @MainActor (String) -> Void, onRewriteWarning: @escaping @MainActor () -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.undoLastCommit(at: path)
                await MainActor.run {
                    onStatus(headCommitPushed
                        ? String(localized: "git.history.undo_last_commit.remote_success", bundle: .module)
                        : String(localized: "git.history.undo_last_commit.success", bundle: .module))
                    if headCommitPushed {
                        onRewriteWarning()
                    } else {
                        self.refresh(project: project, presentation: .preserveVisibleState)
                    }
                }
            } catch {
                await MainActor.run {
                    self.panelState.isGitLoading = false
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func refreshRemoteBranches(project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        let path = project.path
        let projectID = project.id
        panelState.activeGitRemoteOperation = .fetch
        let startedAt = Date()

        Task.detached {
            let service = GitService()
            do {
                try service.fetch(at: path)
                let branches = try service.localBranches(at: path)
                let remoteBranches = try service.remoteBranches(at: path)
                let remotes = try service.remotes(at: path)
                let remoteState = try service.remoteSyncState(at: path)
                let remainingDelay = max(0, 1.2 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    self.panelState.activeGitRemoteOperation = nil
                    self.panelState.gitBranches = branches
                    self.panelState.gitRemoteBranches = remoteBranches
                    self.panelState.gitRemotes = remotes
                    self.panelState.gitRemoteSyncState = remoteState
                    self.cacheState(for: projectID)
                    onStatus(String(localized: "git.remote.branches.refresh_success", bundle: .module))
                }
            } catch {
                let remainingDelay = max(0, 1.2 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    self.panelState.activeGitRemoteOperation = nil
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func pushBranch(_ branch: String, to remote: GitRemoteEntry, project: Project, credential: GitCredential? = nil, onStatus: @escaping @MainActor (String) -> Void, onAuthRequired: @escaping @MainActor (@escaping (GitCredential?) -> Void) -> Void, onAuthSucceeded: @escaping @MainActor (GitCredential) -> Void) {
        panelState.activeGitRemoteOperation = .push
        let startedAt = Date()
        let path = project.path

        Task.detached {
            let service = GitService()
            do {
                try service.push(branch: branch, to: remote.name, at: path, credential: credential)
                let remainingDelay = max(0, 1.0 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    if let credential {
                        onAuthSucceeded(credential)
                    }
                    self.panelState.activeGitRemoteOperation = nil
                    onStatus(String(format: String(localized: "git.remote.push.success_format", bundle: .module), remote.name))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                let remainingDelay = max(0, 1.0 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    if case GitServiceError.authenticationRequired = error,
                       credential == nil {
                        onAuthRequired { credential in
                            guard let credential else {
                                self.panelState.activeGitRemoteOperation = nil
                                onStatus(String(localized: "git.auth.cancelled", bundle: .module))
                                return
                            }
                            self.pushBranch(branch, to: remote, project: project, credential: credential, onStatus: onStatus, onAuthRequired: onAuthRequired, onAuthSucceeded: onAuthSucceeded)
                        }
                        return
                    }
                    self.panelState.activeGitRemoteOperation = nil
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func pushLocalBranch(_ localBranch: String, to remote: GitRemoteEntry, remoteBranch: String, project: Project, credential: GitCredential? = nil, onStatus: @escaping @MainActor (String) -> Void, onAuthRequired: @escaping @MainActor (@escaping (GitCredential?) -> Void) -> Void, onAuthSucceeded: @escaping @MainActor (GitCredential) -> Void) {
        panelState.activeGitRemoteOperation = .push
        let startedAt = Date()
        let path = project.path

        Task.detached {
            let service = GitService()
            do {
                try service.push(localBranch: localBranch, to: remote.name, remoteBranch: remoteBranch, at: path, credential: credential)
                let remainingDelay = max(0, 1.0 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    if let credential {
                        onAuthSucceeded(credential)
                    }
                    self.panelState.activeGitRemoteOperation = nil
                    onStatus(String(format: String(localized: "git.remote.push.branch_success_format", bundle: .module), localBranch, remote.name, remoteBranch))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                let remainingDelay = max(0, 1.0 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    if case GitServiceError.authenticationRequired = error,
                       credential == nil {
                        onAuthRequired { credential in
                            guard let credential else {
                                self.panelState.activeGitRemoteOperation = nil
                                onStatus(String(localized: "git.auth.cancelled", bundle: .module))
                                return
                            }
                            self.pushLocalBranch(localBranch, to: remote, remoteBranch: remoteBranch, project: project, credential: credential, onStatus: onStatus, onAuthRequired: onAuthRequired, onAuthSucceeded: onAuthSucceeded)
                        }
                        return
                    }
                    self.panelState.activeGitRemoteOperation = nil
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func addRemote(name: String, url: String, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.addRemote(name: name, url: url, at: path)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.remote.add.success_format", bundle: .module), name))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func removeRemote(_ remote: GitRemoteEntry, project: Project, onStatus: @escaping @MainActor (String) -> Void) {
        beginPanelOperation()
        let path = project.path
        Task.detached {
            let service = GitService()
            do {
                try service.removeRemote(name: remote.name, at: path)
                await MainActor.run {
                    onStatus(String(format: String(localized: "git.remote.remove.success_format", bundle: .module), remote.name))
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                await MainActor.run {
                    onStatus(error.localizedDescription)
                }
            }
        }
    }

    func performRemoteAction(_ action: GitRemoteAction, project: Project, credential: GitCredential? = nil, onStatus: @escaping @MainActor (String) -> Void, onAuthRequired: @escaping @MainActor (@escaping (GitCredential?) -> Void) -> Void, onAuthSucceeded: @escaping @MainActor (GitCredential) -> Void, onConflict: @escaping @MainActor () -> Void) {
        panelState.activeGitRemoteOperation = remoteOperation(for: action)
        let startedAt = Date()
        let path = project.path

        Task.detached {
            let service = GitService()
            do {
                let message = try Self.runRemoteAction(action, service: service, path: path, credential: credential)
                let remainingDelay = max(0, 1.2 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    if let credential {
                        onAuthSucceeded(credential)
                    }
                    self.panelState.activeGitRemoteOperation = nil
                    onStatus(message)
                    self.refresh(project: project, presentation: .preserveVisibleState)
                }
            } catch {
                let remainingDelay = max(0, 1.2 - Date().timeIntervalSince(startedAt))
                if remainingDelay > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remainingDelay * 1_000_000_000))
                }
                await MainActor.run {
                    if case GitServiceError.authenticationRequired = error,
                       credential == nil {
                        onAuthRequired { credential in
                            guard let credential else {
                                self.panelState.activeGitRemoteOperation = nil
                                onStatus(String(localized: "git.auth.cancelled", bundle: .module))
                                return
                            }
                            self.performRemoteAction(action, project: project, credential: credential, onStatus: onStatus, onAuthRequired: onAuthRequired, onAuthSucceeded: onAuthSucceeded, onConflict: onConflict)
                        }
                        return
                    }
                    if Self.looksLikeConflict(error.localizedDescription), action == .sync {
                        self.panelState.activeGitRemoteOperation = nil
                        onConflict()
                    } else {
                        self.panelState.activeGitRemoteOperation = nil
                        onStatus(error.localizedDescription)
                    }
                }
            }
        }
    }

    private static func looksLikeConflict(_ message: String) -> Bool {
        let normalized = message.lowercased()
        return normalized.contains("conflict")
            || normalized.contains("merge conflict")
            || normalized.contains("could not apply")
            || normalized.contains("resolve all conflicts")
    }

    private nonisolated static func runRemoteAction(_ action: GitRemoteAction, service: GitService, path: String, credential: GitCredential?) throws -> String {
        switch action {
        case .fetch:
            try service.fetch(at: path, credential: credential)
            return String(localized: "git.remote.fetch.success", defaultValue: "Fetched remote updates.", bundle: .module)
        case .pull:
            try service.pull(at: path, credential: credential)
            return String(localized: "git.remote.pull.success", defaultValue: "Pulled remote updates.", bundle: .module)
        case .push:
            try service.push(at: path, credential: credential)
            return String(localized: "git.remote.push.current_success", defaultValue: "Pushed the current branch.", bundle: .module)
        case .forcePush:
            try service.forcePush(at: path, credential: credential)
            return String(localized: "git.remote.force_push.current_success", defaultValue: "Force pushed the current branch.", bundle: .module)
        case .sync:
            try service.fetch(at: path, credential: credential)
            let remoteState = try service.remoteSyncState(at: path)
            if remoteState.incomingCount > 0 {
                try service.pull(at: path, credential: credential)
            }
            let postPullState = try service.remoteSyncState(at: path)
            if postPullState.outgoingCount > 0 {
                try service.push(at: path, credential: credential)
            }
            if remoteState.incomingCount > 0 && postPullState.outgoingCount > 0 {
                return String(
                    format: String(localized: "git.remote.sync.pull_push_format", defaultValue: "Pulled %@ updates and pushed %@ commits.", bundle: .module),
                    "\(remoteState.incomingCount)",
                    "\(postPullState.outgoingCount > 0 ? postPullState.outgoingCount : remoteState.outgoingCount)"
                )
            }
            if remoteState.incomingCount > 0 {
                return String(format: String(localized: "git.remote.sync.pull_only_format", defaultValue: "Pulled %@ updates.", bundle: .module), "\(remoteState.incomingCount)")
            }
            if postPullState.outgoingCount > 0 || remoteState.outgoingCount > 0 {
                return String(format: String(localized: "git.remote.sync.push_only_format", defaultValue: "Pushed %@ commits.", bundle: .module), "\(postPullState.outgoingCount > 0 ? postPullState.outgoingCount : remoteState.outgoingCount)")
            }
            return String(localized: "git.remote.sync.synced", defaultValue: "Remote is synced.", bundle: .module)
        }
    }

    private nonisolated func remoteOperation(for action: GitRemoteAction) -> GitRemoteOperation? {
        switch action {
        case .fetch:
            return .fetch
        case .pull:
            return .pull
        case .push:
            return .push
        case .forcePush:
            return .forcePush
        case .sync:
            return nil
        }
    }
}
