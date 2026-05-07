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

struct TerminalSession: Identifiable, Codable, Hashable, Sendable {
    var id: UUID
    var projectID: UUID
    var projectName: String
    var title: String
    var cwd: String
    var shell: String
    var command: String
    var previewLines: [String]

    static func make(project: Project, command: String) -> TerminalSession {
        let promptCommand = command.isEmpty ? project.shell : command
        return TerminalSession(
            id: UUID(),
            projectID: project.id,
            projectName: project.name,
            title: URL(fileURLWithPath: project.path).lastPathComponent,
            cwd: project.path,
            shell: project.shell,
            command: promptCommand,
            previewLines: [
                "Launching \(project.shell) in \(project.path)",
                command.isEmpty ? "No default command configured." : "$ \(command)",
                "Codux terminal bridge is ready for native terminal embedding.",
            ]
        )
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
    static let minimumBottomPaneHeight: CGFloat = 160

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

    mutating func addBottomTab(_ sessionID: UUID) {
        bottomTabSessionIDs.append(sessionID)
        selectedSessionID = sessionID
        selectedBottomTabSessionID = sessionID
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
        bottomPaneHeight: CGFloat = 240,
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
            bottomPaneHeight: 240,
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
        topPaneRatios = try container.decodeIfPresent([CGFloat].self, forKey: .topPaneRatios) ?? []
        bottomTabSessionIDs = try container.decode([UUID].self, forKey: .bottomTabSessionIDs)
        bottomPaneHeight = try container.decodeIfPresent(CGFloat.self, forKey: .bottomPaneHeight) ?? 240
        selectedSessionID = try container.decode(UUID.self, forKey: .selectedSessionID)
        selectedBottomTabSessionID = try container.decodeIfPresent(UUID.self, forKey: .selectedBottomTabSessionID)
        sessions = try container.decode([TerminalSession].self, forKey: .sessions)
        topPaneRatios = resolvedTopPaneRatios()
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(projectID, forKey: .projectID)
        try container.encode(topSessionIDs, forKey: .topSessionIDs)
        try container.encode(resolvedTopPaneRatios(), forKey: .topPaneRatios)
        try container.encode(bottomTabSessionIDs, forKey: .bottomTabSessionIDs)
        try container.encode(bottomPaneHeight, forKey: .bottomPaneHeight)
        try container.encode(selectedSessionID, forKey: .selectedSessionID)
        try container.encodeIfPresent(selectedBottomTabSessionID, forKey: .selectedBottomTabSessionID)
        try container.encode(sessions, forKey: .sessions)
    }
}

struct AppSnapshot: Codable {
    var projects: [Project]
    var workspaces: [ProjectWorkspace]
    var selectedProjectID: UUID?
    var appSettings: AppSettings?
    var taskMemos: [TaskMemoItem]?
    var sshProfiles: [SSHConnectionProfile]?
}
