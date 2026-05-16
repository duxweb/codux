import Foundation

struct Project: Identifiable, Codable, Hashable, Sendable {
    var id: UUID
    var name: String
    var path: String
    var shell: String
    var defaultCommand: String
    var badgeText: String?
    var badgeSymbol: String?
    var badgeColorHex: String?
    var gitDefaultPushRemoteName: String?

    static func sample() -> Project {
        let path = WorkspacePaths.repositoryRoot().path
        return Project(
            id: UUID(),
            name: URL(fileURLWithPath: path).lastPathComponent.isEmpty ? "Workspace" : URL(fileURLWithPath: path).lastPathComponent,
            path: path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
    }

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case path
        case shell
        case defaultCommand
        case badgeText
        case badgeSymbol
        case badgeColorHex
        case gitDefaultPushRemoteName
    }

    init(id: UUID, name: String, path: String, shell: String, defaultCommand: String, badgeText: String?, badgeSymbol: String?, badgeColorHex: String?, gitDefaultPushRemoteName: String?) {
        self.id = id
        self.name = name
        self.path = path
        self.shell = shell
        self.defaultCommand = defaultCommand
        self.badgeText = badgeText
        self.badgeSymbol = badgeSymbol
        self.badgeColorHex = badgeColorHex
        self.gitDefaultPushRemoteName = gitDefaultPushRemoteName
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        path = try container.decode(String.self, forKey: .path)
        shell = try container.decode(String.self, forKey: .shell)
        defaultCommand = try container.decodeIfPresent(String.self, forKey: .defaultCommand) ?? ""
        badgeText = try container.decodeIfPresent(String.self, forKey: .badgeText)
        badgeSymbol = try container.decodeIfPresent(String.self, forKey: .badgeSymbol)
        badgeColorHex = try container.decodeIfPresent(String.self, forKey: .badgeColorHex)
        gitDefaultPushRemoteName = try container.decodeIfPresent(String.self, forKey: .gitDefaultPushRemoteName)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(path, forKey: .path)
        try container.encode(shell, forKey: .shell)
        try container.encode(defaultCommand, forKey: .defaultCommand)
        try container.encodeIfPresent(badgeText, forKey: .badgeText)
        try container.encodeIfPresent(badgeSymbol, forKey: .badgeSymbol)
        try container.encodeIfPresent(badgeColorHex, forKey: .badgeColorHex)
        try container.encodeIfPresent(gitDefaultPushRemoteName, forKey: .gitDefaultPushRemoteName)
    }
}

enum ProjectWorktreeTaskStatus: String, CaseIterable, Codable, Hashable, Sendable {
    case todo
    case planning
    case ready
    case running
    case waiting
    case review
    case blocked
    case done
    case merged
    case archived

    var visibleStatus: ProjectWorktreeTaskStatus {
        switch self {
        case .planning:
            return .running
        case .todo, .ready, .running, .waiting, .review, .blocked, .done, .merged, .archived:
            return self
        }
    }

    var displayName: String {
        switch self {
        case .todo:
            return String(localized: "worktree.status.todo", defaultValue: "Todo", bundle: .module)
        case .planning:
            return String(localized: "worktree.status.planning", defaultValue: "Planning", bundle: .module)
        case .ready:
            return String(localized: "worktree.status.ready", defaultValue: "Ready", bundle: .module)
        case .running:
            return String(localized: "worktree.status.running", defaultValue: "Running", bundle: .module)
        case .waiting:
            return String(localized: "worktree.status.waiting", defaultValue: "Waiting", bundle: .module)
        case .review:
            return String(localized: "worktree.status.review", defaultValue: "Review", bundle: .module)
        case .blocked:
            return String(localized: "worktree.status.blocked", defaultValue: "Blocked", bundle: .module)
        case .done:
            return String(localized: "worktree.status.done", defaultValue: "Pending Review", bundle: .module)
        case .merged:
            return String(localized: "worktree.status.merged", defaultValue: "Merged", bundle: .module)
        case .archived:
            return String(localized: "worktree.status.archived", defaultValue: "Archived", bundle: .module)
        }
    }
}

struct ProjectWorktree: Identifiable, Codable, Hashable, Sendable {
    var id: UUID
    var projectID: UUID
    var name: String
    var branch: String
    var path: String
    var status: ProjectWorktreeTaskStatus
    var isDefault: Bool
    var createdAt: Date
    var updatedAt: Date

    static func defaultWorktree(for project: Project) -> ProjectWorktree {
        ProjectWorktree(
            id: project.id,
            projectID: project.id,
            name: String(localized: "worktree.default.name", defaultValue: "Default", bundle: .module),
            branch: "",
            path: project.path,
            status: .todo,
            isDefault: true,
            createdAt: Date(),
            updatedAt: Date()
        )
    }
}

struct ProjectWorktreeGitSummary: Equatable, Hashable, Sendable {
    var changes: Int = 0
    var incoming: Int = 0
    var outgoing: Int = 0

    static let empty = ProjectWorktreeGitSummary()
}

struct WorktreeTask: Identifiable, Codable, Hashable, Sendable {
    var id: UUID { worktreeID }
    var worktreeID: UUID
    var title: String
    var baseBranch: String
    var baseCommit: String?
    var status: ProjectWorktreeTaskStatus
    var createdAt: Date
    var updatedAt: Date
    var startedAt: Date?
    var completedAt: Date?
}

enum WorktreeReviewFileStatus: String, Codable, Hashable, Sendable {
    case added
    case modified
    case deleted
    case renamed
    case copied
    case typeChanged
    case unknown

    var displayName: String {
        switch self {
        case .added:
            return String(localized: "worktree.review.file.added", defaultValue: "Added", bundle: .module)
        case .modified:
            return String(localized: "worktree.review.file.modified", defaultValue: "Modified", bundle: .module)
        case .deleted:
            return String(localized: "worktree.review.file.deleted", defaultValue: "Deleted", bundle: .module)
        case .renamed:
            return String(localized: "worktree.review.file.renamed", defaultValue: "Renamed", bundle: .module)
        case .copied:
            return String(localized: "worktree.review.file.copied", defaultValue: "Copied", bundle: .module)
        case .typeChanged:
            return String(localized: "worktree.review.file.type_changed", defaultValue: "Type", bundle: .module)
        case .unknown:
            return String(localized: "worktree.review.file.unknown", defaultValue: "Changed", bundle: .module)
        }
    }
}

struct WorktreeReviewFileChange: Identifiable, Codable, Hashable, Sendable {
    var id: String { path }
    var path: String
    var oldPath: String?
    var status: WorktreeReviewFileStatus
    var additions: Int?
    var deletions: Int?
}

enum WorktreeReviewCheckSeverity: String, Codable, Hashable, Sendable {
    case ok
    case warning
    case blocking
}

struct WorktreeReviewCheck: Identifiable, Codable, Hashable, Sendable {
    var id: String
    var title: String
    var detail: String
    var severity: WorktreeReviewCheckSeverity
}

struct WorktreeReviewFileComparison: Equatable, Sendable {
    var file: WorktreeReviewFileChange
    var baseTitle: String
    var worktreeTitle: String
    var resultTitle: String
    var baseText: String
    var worktreeText: String
    var resultText: String
    var baseDeletesFile: Bool
    var worktreeDeletesFile: Bool
    var resultDeletesFile: Bool
    var message: String?
}

enum WorktreeReviewMode: String, Hashable, Sendable {
    case taskBranch
    case workingTreeAudit
}

struct WorktreeReviewSnapshot: Equatable, Sendable {
    var worktreeID: UUID
    var mode: WorktreeReviewMode
    var title: String
    var diffStat: String
    var files: [WorktreeReviewFileChange]
    var selectedFileID: String?
    var selectedFileComparison: WorktreeReviewFileComparison?
    var checks: [WorktreeReviewCheck]
    var refreshedAt: Date

    static func empty(worktreeID: UUID, title: String, mode: WorktreeReviewMode = .taskBranch) -> WorktreeReviewSnapshot {
        WorktreeReviewSnapshot(
            worktreeID: worktreeID,
            mode: mode,
            title: title,
            diffStat: "",
            files: [],
            selectedFileID: nil,
            selectedFileComparison: nil,
            checks: [],
            refreshedAt: Date()
        )
    }
}

struct WorkspaceFileTab: Identifiable, Hashable, Codable, Sendable {
    var id: String { fileURL.standardizedFileURL.path }
    var fileURL: URL
    var rootURL: URL
    var title: String
}

enum WorkspaceContentSelection: Hashable {
    case terminal
    case file(String)
}

enum WorkspacePrimaryViewMode: String, Codable, Hashable, Sendable {
    case terminal
    case files
    case review
}

struct WorkspaceContentState: Codable, Hashable, Sendable {
    var worktreeID: UUID
    var primaryViewMode: WorkspacePrimaryViewMode
    var selectedFileTabID: String?
    var fileTabs: [WorkspaceFileTab]
}

struct TerminalSession: Identifiable, Codable, Hashable, Sendable {
    enum LaunchMode: String, Codable, Hashable, Sendable {
        case terminal
        case agent
    }

    var id: UUID
    var projectID: UUID
    var projectName: String
    var title: String
    var tabTitle: String?
    var cwd: String
    var shell: String
    var command: String
    var previewLines: [String]
    var launchMode: LaunchMode
    var agentTool: String?

    enum CodingKeys: String, CodingKey {
        case id
        case projectID
        case projectName
        case title
        case tabTitle
        case cwd
        case shell
        case command
        case previewLines
        case launchMode
        case agentTool
    }

    static func make(project: Project, command: String) -> TerminalSession {
        let promptCommand = command.isEmpty ? project.shell : command
        return TerminalSession(
            id: UUID(),
            projectID: project.id,
            projectName: project.name,
            title: URL(fileURLWithPath: project.path).lastPathComponent,
            tabTitle: nil,
            cwd: project.path,
            shell: project.shell,
            command: promptCommand,
            previewLines: [
                "Launching \(project.shell) in \(project.path)",
                command.isEmpty ? "No default command configured." : "$ \(command)",
                "Codux terminal bridge is ready for native terminal embedding.",
            ],
            launchMode: .terminal,
            agentTool: nil
        )
    }

    static func makeAgent(project: Project, tool: AgentToolKind) -> TerminalSession {
        TerminalSession(
            id: UUID(),
            projectID: project.id,
            projectName: project.name,
            title: tool.displayName,
            tabTitle: tool.displayName,
            cwd: project.path,
            shell: project.shell,
            command: "",
            previewLines: [
                "Starting \(tool.displayName) agent in \(project.path)",
                "Codux agent bridge is ready for structured events.",
            ],
            launchMode: .agent,
            agentTool: tool.rawValue
        )
    }

    init(
        id: UUID,
        projectID: UUID,
        projectName: String,
        title: String,
        tabTitle: String?,
        cwd: String,
        shell: String,
        command: String,
        previewLines: [String],
        launchMode: LaunchMode,
        agentTool: String?
    ) {
        self.id = id
        self.projectID = projectID
        self.projectName = projectName
        self.title = title
        self.tabTitle = tabTitle
        self.cwd = cwd
        self.shell = shell
        self.command = command
        self.previewLines = previewLines
        self.launchMode = launchMode
        self.agentTool = agentTool
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        projectID = try container.decode(UUID.self, forKey: .projectID)
        projectName = try container.decode(String.self, forKey: .projectName)
        title = try container.decode(String.self, forKey: .title)
        tabTitle = try container.decodeIfPresent(String.self, forKey: .tabTitle)
        cwd = try container.decode(String.self, forKey: .cwd)
        shell = try container.decode(String.self, forKey: .shell)
        command = try container.decode(String.self, forKey: .command)
        previewLines = try container.decodeIfPresent([String].self, forKey: .previewLines) ?? []
        launchMode = try container.decodeIfPresent(LaunchMode.self, forKey: .launchMode) ?? .terminal
        agentTool = try container.decodeIfPresent(String.self, forKey: .agentTool)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(projectID, forKey: .projectID)
        try container.encode(projectName, forKey: .projectName)
        try container.encode(title, forKey: .title)
        try container.encodeIfPresent(tabTitle, forKey: .tabTitle)
        try container.encode(cwd, forKey: .cwd)
        try container.encode(shell, forKey: .shell)
        try container.encode(command, forKey: .command)
        try container.encode(previewLines, forKey: .previewLines)
        try container.encode(launchMode, forKey: .launchMode)
        try container.encodeIfPresent(agentTool, forKey: .agentTool)
    }
}

enum AgentToolKind: String, CaseIterable, Codable, Hashable, Sendable, Identifiable {
    case codex
    case claude
    case opencode

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .codex:
            return "Codex"
        case .claude:
            return "Claude Code"
        case .opencode:
            return "OpenCode"
        }
    }

    var symbolName: String {
        switch self {
        case .codex:
            return "sparkles"
        case .claude:
            return "wand.and.stars"
        case .opencode:
            return "curlybraces"
        }
    }

    var supportedAITool: AppSupportedAITool {
        switch self {
        case .codex:
            return .codex
        case .claude:
            return .claudeCode
        case .opencode:
            return .opencode
        }
    }

    var modelPresets: [String] {
        switch self {
        case .codex:
            return ["gpt-5.5", "gpt-5.5-codex", "gpt-5", "gpt-5-codex", "gpt-4.1"]
        case .claude:
            return ["sonnet", "opus", "haiku"]
        case .opencode:
            return []
        }
    }
}

enum TaskMemoStatus: String, CaseIterable, Codable, Hashable, Sendable {
    case queued
    case waiting
    case completed

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        switch rawValue {
        case Self.queued.rawValue:
            self = .queued
        case Self.waiting.rawValue, "stashed":
            self = .waiting
        case Self.completed.rawValue:
            self = .completed
        default:
            self = .queued
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

struct TaskMemoItem: Identifiable, Codable, Hashable, Sendable {
    var id: UUID
    var projectID: UUID
    var sessionID: UUID
    var content: String
    var status: TaskMemoStatus
    var createdAt: Date
    var updatedAt: Date
    var lastSentAt: Date?
}

enum SSHCredentialKind: String, Codable, CaseIterable, Hashable, Sendable {
    case none
    case password
    case privateKey
}

struct SSHCredentialSecrets: Equatable {
    var password: String
    var keyPassphrase: String
}

struct SSHConnectionProfile: Identifiable, Codable, Hashable, Sendable {
    var id: UUID
    var name: String
    var host: String
    var port: Int
    var username: String
    var credentialKind: SSHCredentialKind
    var privateKeyPath: String
    var updatedAt: Date
    var password: String? = nil
    var keyPassphrase: String? = nil

    var displayName: String {
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmedName.isEmpty {
            return trimmedName
        }
        return "\(username)@\(host)"
    }
}

struct RecentProjectCache<Value> {
    private struct Entry {
        var value: Value
        var updatedAt: Date
    }

    private var storage: [UUID: Entry] = [:]
    private var order: [UUID] = []
    private let maxEntries: Int?
    private let ttl: TimeInterval

    init(maxEntries: Int? = nil, ttl: TimeInterval = 30) {
        self.maxEntries = maxEntries
        self.ttl = ttl
    }

    var projectIDs: [UUID] {
        Array(storage.keys)
    }

    func peekValue(for projectID: UUID, now: Date = Date()) -> Value? {
        guard let entry = storage[projectID] else {
            return nil
        }
        guard now.timeIntervalSince(entry.updatedAt) <= ttl else {
            return nil
        }
        return entry.value
    }

    mutating func set(_ value: Value, for projectID: UUID) {
        storage[projectID] = Entry(value: value, updatedAt: Date())
        order.removeAll { $0 == projectID }
        order.append(projectID)

        if let maxEntries {
            while order.count > maxEntries {
                let evicted = order.removeFirst()
                storage[evicted] = nil
            }
        }
    }

    mutating func value(for projectID: UUID) -> Value? {
        guard let entry = storage[projectID] else {
            return nil
        }
        guard Date().timeIntervalSince(entry.updatedAt) <= ttl else {
            removeValue(for: projectID)
            return nil
        }
        order.removeAll { $0 == projectID }
        order.append(projectID)
        return entry.value
    }

    mutating func removeValue(for projectID: UUID) {
        storage[projectID] = nil
        order.removeAll { $0 == projectID }
    }
}

enum PanelRefreshState: Equatable {
    case idle
    case showingCached
    case refreshing
    case failed(String)

    var isShowingCached: Bool {
        if case .showingCached = self {
            return true
        }
        return false
    }
}

struct GitRemoteEntry: Hashable {
    var name: String
    var url: String
}

struct GitPanelState: Equatable {
    var gitState: GitRepositoryState?
    var selectedGitEntry: GitFileEntry?
    var selectedGitEntryIDs: Set<String>
    var gitHistory: [GitCommitEntry]
    var selectedGitCommitHash: String?
    var gitDiffText: String
    var gitBranches: [String]
    var gitBranchUpstreams: [String: String]
    var gitRemoteBranches: [String]
    var gitRemotes: [GitRemoteEntry]
    var gitRemoteSyncState: GitRemoteSyncState
    var isGitLoading: Bool
    var isGitDiffLoading: Bool
    var gitOperationStatusText: String?
    var gitOperationProgress: Double?
    var gitSelectionAnchorID: String?
    var activeGitRemoteOperation: GitRemoteOperation?
    var refreshState: PanelRefreshState

    static let empty = GitPanelState(
        gitState: nil,
        selectedGitEntry: nil,
        selectedGitEntryIDs: [],
        gitHistory: [],
        selectedGitCommitHash: nil,
        gitDiffText: "Select a file to preview its diff.",
        gitBranches: [],
        gitBranchUpstreams: [:],
        gitRemoteBranches: [],
        gitRemotes: [],
        gitRemoteSyncState: .empty,
        isGitLoading: false,
        isGitDiffLoading: false,
        gitOperationStatusText: nil,
        gitOperationProgress: nil,
        gitSelectionAnchorID: nil,
        activeGitRemoteOperation: nil,
        refreshState: .idle
    )
}

enum PaneAxis: String, Codable, Hashable {
    case horizontal
    case vertical
}

enum DetachedTerminalRegion: String, Hashable, Sendable {
    case top
    case bottom
}

struct DetachedTerminalPlacement: Equatable, Hashable, Sendable {
    var projectID: UUID
    var region: DetachedTerminalRegion
    var index: Int
    var topPaneRatios: [CGFloat]
}

struct ProjectWorkspace: Identifiable, Codable, Hashable {
    var id: UUID { projectID }
    var projectID: UUID
    var topSessionIDs: [UUID]
    var topPaneRatios: [CGFloat]
    var bottomTabSessionIDs: [UUID]
    var bottomPaneHeight: CGFloat
    var selectedSessionID: UUID
    var selectedBottomTabSessionID: UUID?
    var sessions: [TerminalSession]

    static let maxTopPanes = 6
    static let minimumTopPaneHeight: CGFloat = 220
    static let minimumBottomPaneHeight: CGFloat = 160
    static let defaultBottomPaneHeight: CGFloat = 240

    var hasBottomTabs: Bool {
        !bottomTabSessionIDs.isEmpty
    }

    var visibleSessionIDs: [UUID] {
        topSessionIDs + bottomTabSessionIDs
    }

    var visibleSessionCount: Int {
        visibleSessionIDs.count
    }

    func containsTopSession(_ sessionID: UUID) -> Bool {
        topSessionIDs.contains(sessionID)
    }

    func containsBottomSession(_ sessionID: UUID) -> Bool {
        bottomTabSessionIDs.contains(sessionID)
    }

    func containsVisibleSession(_ sessionID: UUID) -> Bool {
        containsTopSession(sessionID) || containsBottomSession(sessionID)
    }

    func containsSession(_ sessionID: UUID) -> Bool {
        sessions.contains { $0.id == sessionID }
    }

    func session(for sessionID: UUID) -> TerminalSession? {
        sessions.first { $0.id == sessionID }
    }

    func resolvedTopPaneRatios() -> [CGFloat] {
        guard !topSessionIDs.isEmpty else { return [] }

        let candidate = topPaneRatios.count == topSessionIDs.count
            ? topPaneRatios
            : Array(repeating: 1, count: topSessionIDs.count)

        let sum = candidate.reduce(0, +)
        guard sum > 0 else {
            let value = 1 / CGFloat(topSessionIDs.count)
            return Array(repeating: value, count: topSessionIDs.count)
        }

        return candidate.map { $0 / sum }
    }

    mutating func addTopSession(_ sessionID: UUID) -> Bool {
        guard topSessionIDs.count < Self.maxTopPanes else {
            return false
        }

        topSessionIDs.append(sessionID)
        let equalRatio = 1 / CGFloat(topSessionIDs.count)
        topPaneRatios = Array(repeating: equalRatio, count: topSessionIDs.count)
        selectedSessionID = sessionID
        return true
    }

    static func defaultBottomTabTitle(index: Int) -> String {
        String(
            format: String(localized: "workspace.tab_format", defaultValue: "Tab %@", bundle: .module),
            "\(index + 1)"
        )
    }

    mutating func addBottomTab(_ sessionID: UUID, title: String? = nil) {
        bottomTabSessionIDs.append(sessionID)
        setBottomTabTitleIfNeeded(
            sessionID,
            title: title ?? Self.defaultBottomTabTitle(index: bottomTabSessionIDs.count - 1)
        )
        selectedSessionID = sessionID
        selectedBottomTabSessionID = sessionID
    }

    mutating func ensureDefaultBottomTabTitles() -> Bool {
        var didChange = false
        for (index, sessionID) in bottomTabSessionIDs.enumerated() {
            didChange = setBottomTabTitleIfNeeded(
                sessionID,
                title: Self.defaultBottomTabTitle(index: index)
            ) || didChange
        }
        return didChange
    }

    mutating func moveBottomTab(_ sessionID: UUID, to targetSessionID: UUID) -> Bool {
        guard sessionID != targetSessionID,
              let sourceIndex = bottomTabSessionIDs.firstIndex(of: sessionID),
              let targetIndex = bottomTabSessionIDs.firstIndex(of: targetSessionID) else {
            return false
        }

        var updatedSessionIDs = bottomTabSessionIDs
        let movedSessionID = updatedSessionIDs.remove(at: sourceIndex)
        guard let adjustedTargetIndex = updatedSessionIDs.firstIndex(of: targetSessionID) else {
            return false
        }

        let insertionIndex = sourceIndex < targetIndex
            ? min(adjustedTargetIndex + 1, updatedSessionIDs.count)
            : adjustedTargetIndex
        updatedSessionIDs.insert(movedSessionID, at: insertionIndex)

        guard updatedSessionIDs != bottomTabSessionIDs else {
            return false
        }
        bottomTabSessionIDs = updatedSessionIDs
        return true
    }

    mutating func renameBottomTab(_ sessionID: UUID, to title: String) -> Bool {
        let trimmedTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedTitle.isEmpty,
              bottomTabSessionIDs.contains(sessionID),
              let sessionIndex = sessions.firstIndex(where: { $0.id == sessionID }) else {
            return false
        }

        guard sessions[sessionIndex].tabTitle != trimmedTitle else {
            return false
        }
        sessions[sessionIndex].tabTitle = trimmedTitle
        return true
    }

    @discardableResult
    private mutating func setBottomTabTitleIfNeeded(_ sessionID: UUID, title: String) -> Bool {
        guard bottomTabSessionIDs.contains(sessionID),
              let sessionIndex = sessions.firstIndex(where: { $0.id == sessionID }) else {
            return false
        }
        let trimmedTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedTitle.isEmpty else {
            return false
        }
        if let existingTitle = sessions[sessionIndex].tabTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !existingTitle.isEmpty {
            return false
        }
        sessions[sessionIndex].tabTitle = trimmedTitle
        return true
    }

    mutating func removeSession(_ sessionID: UUID) {
        if let index = topSessionIDs.firstIndex(of: sessionID) {
            topSessionIDs.remove(at: index)
            if index < topPaneRatios.count {
                topPaneRatios.remove(at: index)
            }
            topPaneRatios = resolvedTopPaneRatios()
        }
        bottomTabSessionIDs.removeAll(where: { $0 == sessionID })
        sessions.removeAll(where: { $0.id == sessionID })

        if selectedBottomTabSessionID == sessionID {
            selectedBottomTabSessionID = bottomTabSessionIDs.last
        }

        if selectedSessionID == sessionID {
            if let replacement = topSessionIDs.last ?? bottomTabSessionIDs.last ?? sessions.first?.id {
                selectedSessionID = replacement
            }
        }
    }

    mutating func detachVisibleSession(_ sessionID: UUID) -> DetachedTerminalPlacement? {
        let originalTopRatios = resolvedTopPaneRatios()

        if let index = topSessionIDs.firstIndex(of: sessionID) {
            topSessionIDs.remove(at: index)
            if index < topPaneRatios.count {
                topPaneRatios.remove(at: index)
            }
            topPaneRatios = resolvedTopPaneRatios()
            reconcileSelectionAfterVisibleRemoval(of: sessionID)
            return DetachedTerminalPlacement(
                projectID: projectID,
                region: .top,
                index: index,
                topPaneRatios: originalTopRatios
            )
        }

        if let index = bottomTabSessionIDs.firstIndex(of: sessionID) {
            bottomTabSessionIDs.remove(at: index)
            reconcileSelectionAfterVisibleRemoval(of: sessionID)
            return DetachedTerminalPlacement(
                projectID: projectID,
                region: .bottom,
                index: index,
                topPaneRatios: originalTopRatios
            )
        }

        return nil
    }

    mutating func restoreDetachedSession(_ sessionID: UUID, placement: DetachedTerminalPlacement) {
        guard containsSession(sessionID), containsVisibleSession(sessionID) == false else {
            return
        }

        switch placement.region {
        case .top:
            let insertIndex = min(max(placement.index, 0), topSessionIDs.count)
            topSessionIDs.insert(sessionID, at: insertIndex)
            if placement.topPaneRatios.count == topSessionIDs.count {
                topPaneRatios = placement.topPaneRatios
            } else {
                let equalRatio = 1 / CGFloat(topSessionIDs.count)
                topPaneRatios = Array(repeating: equalRatio, count: topSessionIDs.count)
            }
            topPaneRatios = resolvedTopPaneRatios()
            selectedSessionID = sessionID

        case .bottom:
            let insertIndex = min(max(placement.index, 0), bottomTabSessionIDs.count)
            bottomTabSessionIDs.insert(sessionID, at: insertIndex)
            selectedBottomTabSessionID = sessionID
            selectedSessionID = sessionID
        }
    }

    private mutating func reconcileSelectionAfterVisibleRemoval(of sessionID: UUID) {
        if selectedBottomTabSessionID == sessionID {
            selectedBottomTabSessionID = bottomTabSessionIDs.last
        }

        guard selectedSessionID == sessionID else {
            return
        }

        if let replacement = topSessionIDs.last ?? selectedBottomTabSessionID ?? bottomTabSessionIDs.last {
            selectedSessionID = replacement
        }
    }

    init(
        projectID: UUID,
        topSessionIDs: [UUID],
        topPaneRatios: [CGFloat] = [],
        bottomTabSessionIDs: [UUID],
        bottomPaneHeight: CGFloat = Self.defaultBottomPaneHeight,
        selectedSessionID: UUID,
        selectedBottomTabSessionID: UUID?,
        sessions: [TerminalSession]
    ) {
        self.projectID = projectID
        self.topSessionIDs = topSessionIDs
        self.topPaneRatios = topPaneRatios
        self.bottomTabSessionIDs = bottomTabSessionIDs
        self.bottomPaneHeight = bottomPaneHeight
        self.selectedSessionID = selectedSessionID
        self.selectedBottomTabSessionID = selectedBottomTabSessionID
        self.sessions = sessions
        self.topPaneRatios = resolvedTopPaneRatios()
    }

    static func sample(projectID: UUID, path: String) -> ProjectWorkspace {
        let project = Project(
            id: projectID,
            name: URL(fileURLWithPath: path).lastPathComponent,
            path: path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
        let session = TerminalSession.make(project: project, command: "")
        return ProjectWorkspace(
            projectID: projectID,
            topSessionIDs: [session.id],
            topPaneRatios: [1],
            bottomTabSessionIDs: [],
            bottomPaneHeight: Self.defaultBottomPaneHeight,
            selectedSessionID: session.id,
            selectedBottomTabSessionID: nil,
            sessions: [session]
        )
    }

    enum CodingKeys: String, CodingKey {
        case projectID
        case topSessionIDs
        case topPaneRatios
        case bottomTabSessionIDs
        case bottomPaneHeight
        case selectedSessionID
        case selectedBottomTabSessionID
        case sessions
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        projectID = try container.decode(UUID.self, forKey: .projectID)
        topSessionIDs = try container.decode([UUID].self, forKey: .topSessionIDs)
        _ = try container.decodeIfPresent([CGFloat].self, forKey: .topPaneRatios)
        topPaneRatios = []
        bottomTabSessionIDs = try container.decode([UUID].self, forKey: .bottomTabSessionIDs)
        _ = try container.decodeIfPresent(CGFloat.self, forKey: .bottomPaneHeight)
        bottomPaneHeight = Self.defaultBottomPaneHeight
        selectedSessionID = try container.decode(UUID.self, forKey: .selectedSessionID)
        selectedBottomTabSessionID = try container.decodeIfPresent(UUID.self, forKey: .selectedBottomTabSessionID)
        sessions = try container.decode([TerminalSession].self, forKey: .sessions)
        topPaneRatios = resolvedTopPaneRatios()
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(projectID, forKey: .projectID)
        try container.encode(topSessionIDs, forKey: .topSessionIDs)
        let equalTopPaneRatios = topSessionIDs.isEmpty
            ? []
            : Array(repeating: 1 / CGFloat(topSessionIDs.count), count: topSessionIDs.count)
        try container.encode(equalTopPaneRatios, forKey: .topPaneRatios)
        try container.encode(bottomTabSessionIDs, forKey: .bottomTabSessionIDs)
        try container.encode(Self.defaultBottomPaneHeight, forKey: .bottomPaneHeight)
        try container.encode(selectedSessionID, forKey: .selectedSessionID)
        try container.encodeIfPresent(selectedBottomTabSessionID, forKey: .selectedBottomTabSessionID)
        try container.encode(sessions, forKey: .sessions)
    }
}

struct AppSnapshot: Codable {
    var projects: [Project]
    var worktrees: [ProjectWorktree]?
    var worktreeTasks: [WorktreeTask]? = nil
    var workspaces: [ProjectWorkspace]
    var selectedProjectID: UUID?
    var selectedWorktreeID: UUID?
    var workspaceContentStates: [WorkspaceContentState]? = nil
    var appSettings: AppSettings?
    var taskMemos: [TaskMemoItem]?
    var sshProfiles: [SSHConnectionProfile]?
}
