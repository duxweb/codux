import Foundation

struct AIRuntimeBridgeService {
    struct ManagedHookSpec {
        var eventKey: String
        var action: String
        var command: String
        var statusMessage: String
        var timeout: Int
        var async: Bool = false
    }

    struct EnvironmentResolution {
        let pairs: [(String, String)]
        let isCacheHit: Bool
    }

    private final class ManagedHookBootstrapCoordinator: @unchecked Sendable {
        enum State {
            case idle
            case running
            case finished
        }

        private let queue = DispatchQueue(label: "dmux.runtime-hooks.bootstrap", qos: .utility)
        private let lock = NSLock()
        private var state: State = .idle

        func schedule(_ work: @escaping @Sendable () -> Void) -> Bool {
            let shouldSchedule = lock.withLock { () -> Bool in
                guard state == .idle else {
                    return false
                }
                state = .running
                return true
            }

            guard shouldSchedule else {
                return false
            }

            queue.async { [weak self] in
                work()
                self?.lock.withLock {
                    self?.state = .finished
                }
            }
            return true
        }
    }

    private final class EnvironmentCacheCoordinator: @unchecked Sendable {
        struct Entry {
            let signature: String
            let pairs: [(String, String)]
        }

        private let lock = NSLock()
        private var storage: [UUID: Entry] = [:]
        private var order: [UUID] = []
        private let maxEntries = 48

        func value(for sessionID: UUID, signature: String) -> [(String, String)]? {
            lock.withLock {
                guard let entry = storage[sessionID], entry.signature == signature else {
                    return nil
                }
                order.removeAll { $0 == sessionID }
                order.append(sessionID)
                return entry.pairs
            }
        }

        func set(_ pairs: [(String, String)], for sessionID: UUID, signature: String) {
            lock.withLock {
                storage[sessionID] = Entry(signature: signature, pairs: pairs)
                order.removeAll { $0 == sessionID }
                order.append(sessionID)

                while order.count > maxEntries {
                    let evicted = order.removeFirst()
                    storage[evicted] = nil
                }
            }
        }
    }

    private static let managedHookBootstrapCoordinator = ManagedHookBootstrapCoordinator()
    private static let environmentCacheCoordinator = EnvironmentCacheCoordinator()
    private static let passthroughEnvironmentKeys = [
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "TMPDIR",
        "PWD",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "LC_MESSAGES",
        "LC_COLLATE",
        "LC_NUMERIC",
        "LC_TIME",
        "LC_MONETARY",
        "LC_MEASUREMENT",
        "LC_IDENTIFICATION",
        "LC_PAPER",
        "LC_NAME",
        "LC_ADDRESS",
        "LC_TELEPHONE",
        "LC_RESPONSETIME",
        "SSH_AUTH_SOCK",
        "__CF_USER_TEXT_ENCODING",
    ]

    let fileManager = FileManager.default
    let debugLog = AppDebugLog.shared
    private let aiToolEnvironmentService = AIToolEnvironmentService()

    func runtimeSupportRootURL(createIfNeeded: Bool = true) -> URL {
        let url = AppRuntimePaths.runtimeSupportRootURL(fileManager: fileManager)!
        if createIfNeeded {
            try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        }
        return url
    }

    func claudeSessionMapDirectoryURL(createIfNeeded: Bool = true) -> URL {
        let url = runtimeTemporaryRootURL(createIfNeeded: createIfNeeded)
            .appendingPathComponent("claude-session-map", isDirectory: true)
        if createIfNeeded {
            try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        }
        return url
    }

    func clearAllClaudeSessionMappings() {
        clearJSONFiles(in: claudeSessionMapDirectoryURL())
    }

    func runtimeEventSocketURL() -> URL {
        let rootURL = AppRuntimePaths.temporaryRootURL(fileManager: fileManager)
        try? fileManager.createDirectory(at: rootURL, withIntermediateDirectories: true)
        return rootURL.appendingPathComponent("runtime-events.sock", isDirectory: false)
    }

    func environmentResolution(for session: TerminalSession, aiSettings: AppAISettings? = nil) -> EnvironmentResolution {
        scheduleManagedHookBootstrapIfNeeded()
        let wrapperPath = wrapperBinURL().path
        let processEnvironment = ProcessInfo.processInfo.environment
        let originalPath = processEnvironment["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin"
        let statusDirectoryPath = preparedStatusDirectoryPath()
        let claudeSessionMapDirectoryPath = preparedClaudeSessionMapDirectoryPath()
        let shellHookPaths = preparedShellHookPaths()
        let logFilePath = AppDebugLog.shared.logFileURL().path
        let runtimeOwner = runtimeOwnerID()
        let toolPermissionSettingsFilePath = toolPermissionSettingsFileURL().path
        let memoryArtifacts = aiSettings.flatMap {
            MemoryContextService().prepareLaunchArtifacts(
                projectID: session.projectID,
                projectName: session.projectName,
                projectPath: session.cwd,
                settings: $0
            )
        }
        let signature = environmentCacheSignature(
            session: session,
            processEnvironment: processEnvironment,
            wrapperPath: wrapperPath,
            originalPath: originalPath,
            statusDirectoryPath: statusDirectoryPath,
            claudeSessionMapDirectoryPath: claudeSessionMapDirectoryPath,
            shellHookPaths: shellHookPaths,
            logFilePath: logFilePath,
            runtimeOwner: runtimeOwner,
            toolPermissionSettingsFilePath: toolPermissionSettingsFilePath,
            memoryWorkspaceRootPath: memoryArtifacts?.workspaceRootURL.path,
            memoryWorkspaceLinkPath: memoryArtifacts?.workspaceLinkURL.path,
            memoryPromptFilePath: memoryArtifacts?.promptFileURL.path
        )

        if let cached = Self.environmentCacheCoordinator.value(for: session.id, signature: signature) {
            return EnvironmentResolution(pairs: cached, isCacheHit: true)
        }

        debugLog.log(
            "startup-ui",
            "terminal-env begin session=\(session.id.uuidString) project=\(session.projectID.uuidString)"
        )
        debugLog.log("startup-ui", "terminal-env step=session-wrapper session=\(session.id.uuidString)")
        debugLog.log("startup-ui", "terminal-env step=process-environment session=\(session.id.uuidString) count=\(processEnvironment.count)")

        var merged: [String: String] = [:]
        for key in Self.passthroughEnvironmentKeys {
            if let value = processEnvironment[key], !value.isEmpty {
                merged[key] = value
            }
        }
        for (key, value) in aiToolEnvironmentService.configuredEnvironment() where merged[key]?.isEmpty ?? true {
            merged[key] = value
        }

        merged["PATH"] = wrapperPath + ":" + originalPath
        merged["DMUX_WRAPPER_BIN"] = wrapperPath
        merged["DMUX_ORIGINAL_PATH"] = originalPath
        debugLog.log("startup-ui", "terminal-env step=path-ready session=\(session.id.uuidString)")
        if let statusDirectoryPath {
            merged["DMUX_STATUS_DIR"] = statusDirectoryPath
        }
        merged["DMUX_RUNTIME_SOCKET"] = runtimeEventSocketURL().path
        if let claudeSessionMapDirectoryPath {
            merged["DMUX_CLAUDE_SESSION_MAP_DIR"] = claudeSessionMapDirectoryPath
        }
        debugLog.log("startup-ui", "terminal-env step=runtime-paths session=\(session.id.uuidString)")
        merged["DMUX_LOG_FILE"] = logFilePath
        merged["DMUX_RUNTIME_OWNER"] = runtimeOwner
        merged["DMUX_TOOL_PERMISSION_SETTINGS_FILE"] = toolPermissionSettingsFilePath
        merged["DMUX_PROJECT_ID"] = session.projectID.uuidString
        merged["DMUX_PROJECT_NAME"] = session.projectName
        merged["DMUX_PROJECT_PATH"] = session.cwd
        merged["DMUX_SESSION_ID"] = session.id.uuidString
        merged["DMUX_SESSION_TITLE"] = session.title
        merged["DMUX_SESSION_CWD"] = session.cwd
        debugLog.log("startup-ui", "terminal-env step=session-metadata session=\(session.id.uuidString)")
        if let shellHookPaths {
            merged["DMUX_ZSH_HOOK_SCRIPT"] = shellHookPaths.scriptPath
            merged["ZDOTDIR"] = shellHookPaths.zdotdirPath
        }
        if let memoryArtifacts {
            merged["DMUX_AI_MEMORY_WORKSPACE_ROOT"] = memoryArtifacts.workspaceRootURL.path
            merged["DMUX_AI_MEMORY_WORKSPACE_LINK"] = memoryArtifacts.workspaceLinkURL.path
            merged["DMUX_AI_MEMORY_PROMPT_FILE"] = memoryArtifacts.promptFileURL.path
        }
        debugLog.log("startup-ui", "terminal-env step=hooks-ready session=\(session.id.uuidString) enabled=\(merged["ZDOTDIR"] != nil)")
        merged["TERM"] = "xterm-256color"
        merged["TERM_PROGRAM"] = "dmux"
        merged["LANG"] = merged["LANG"] ?? "en_US.UTF-8"
        merged["LC_CTYPE"] = merged["LC_CTYPE"] ?? merged["LANG"]

        AppDebugLog.shared.log(
            "terminal-env",
            "session=\(session.id.uuidString) shell=\(session.shell) cwd=\(session.cwd) zdotdir=\(merged["ZDOTDIR"] ?? "nil") wrapper=\(merged["DMUX_WRAPPER_BIN"] ?? "nil") pathPrefix=\(merged["PATH"]?.split(separator: ":").prefix(3).joined(separator: ":") ?? "nil")"
        )
        debugLog.log(
            "startup-ui",
            "terminal-env complete session=\(session.id.uuidString) project=\(session.projectID.uuidString) hasHooks=\(merged["ZDOTDIR"] != nil)"
        )

        let pairs = merged.sorted { $0.key < $1.key }
        Self.environmentCacheCoordinator.set(pairs, for: session.id, signature: signature)
        return EnvironmentResolution(pairs: pairs, isCacheHit: false)
    }

    func prepareManagedRuntimeSupportIfNeeded() {
        scheduleManagedHookBootstrapIfNeeded()
    }

    private func wrapperBinURL() -> URL {
        WorkspacePaths.repositoryResourceURL("scripts/wrappers/bin")
    }

    private func scheduleManagedHookBootstrapIfNeeded() {
        guard Self.managedHookBootstrapCoordinator.schedule({
            let service = AIRuntimeBridgeService()
            service.debugLog.log("runtime-hooks", "bootstrap start")
            service.debugLog.log(
                "runtime-hooks",
                "bootstrap namespace owner=\(service.runtimeOwnerID()) supportRoot=\(service.runtimeSupportRootURL().path) tempRoot=\(service.runtimeTemporaryRootURL().path) socket=\(service.runtimeEventSocketURL().path)"
            )
            service.debugLog.log("runtime-hooks", "bootstrap step=status-directory")
            _ = service.statusDirectoryURL()
            service.debugLog.log("runtime-hooks", "bootstrap step=claude-session-map")
            _ = service.claudeSessionMapDirectoryURL()
            service.debugLog.log("runtime-hooks", "bootstrap step=shell-hooks")
            service.ensureShellHooksStaged()
            service.debugLog.log("runtime-hooks", "bootstrap step=managed-helper")
            _ = service.managedRuntimeHookHelperURL()
            service.debugLog.log("runtime-hooks", "bootstrap step=claude-hooks")
            service.ensureManagedHookConfig(
                at: service.claudeSettingsFileURL(),
                category: "claude-hook-config",
                invalidDescription: "settings",
                install: service.installClaudeHooks
            )
            service.debugLog.log("runtime-hooks", "bootstrap step=codex-hooks")
            service.ensureManagedHookConfig(
                at: service.codexHooksFileURL(),
                category: "codex-hook-config",
                invalidDescription: "hooks.json",
                install: service.installCodexHooks
            )
            service.debugLog.log("runtime-hooks", "bootstrap step=codex-config")
            service.ensureCodexConfigInstalled()
            service.debugLog.log("runtime-hooks", "bootstrap step=gemini-hooks")
            service.ensureManagedHookConfig(
                at: service.geminiSettingsFileURL(),
                category: "gemini-hook-config",
                invalidDescription: "settings",
                install: service.installGeminiHooks
            )
            service.debugLog.log("runtime-hooks", "bootstrap complete")
        }) else {
            return
        }

        debugLog.log("runtime-hooks", "bootstrap scheduled")
    }

    func statusDirectoryURL() -> URL {
        let url = runtimeTemporaryRootURL()
            .appendingPathComponent("agent-status", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func stagedShellHooksRootURL() -> URL {
        runtimeSupportRootURL().appendingPathComponent("shell-hooks", isDirectory: true)
    }

    private func ensureShellHooksStaged() {
        let rootURL = stagedShellHooksRootURL()
        let zshDirectoryURL = rootURL.appendingPathComponent("zsh", isDirectory: true)

        try? fileManager.createDirectory(at: rootURL, withIntermediateDirectories: true)
        try? fileManager.createDirectory(at: zshDirectoryURL, withIntermediateDirectories: true)

        stageResource("scripts/shell-hooks/zsh/.zshenv", to: zshDirectoryURL.appendingPathComponent(".zshenv"), logLabel: "shell-hook")
        stageResource("scripts/shell-hooks/zsh/.zprofile", to: zshDirectoryURL.appendingPathComponent(".zprofile"), logLabel: "shell-hook")
        stageResource("scripts/shell-hooks/zsh/.zshrc", to: zshDirectoryURL.appendingPathComponent(".zshrc"), logLabel: "shell-hook")
        stageResource("scripts/shell-hooks/zsh/.zlogin", to: zshDirectoryURL.appendingPathComponent(".zlogin"), logLabel: "shell-hook")
        stageResource("scripts/shell-hooks/dmux-ai-hook.zsh", to: rootURL.appendingPathComponent("dmux-ai-hook.zsh"), logLabel: "shell-hook")
    }

    private func preparedShellHookPaths() -> (zdotdirPath: String, scriptPath: String)? {
        let zdotdirURL = stagedShellHooksRootURL().appendingPathComponent("zsh", isDirectory: true)
        let scriptURL = stagedShellHooksRootURL().appendingPathComponent("dmux-ai-hook.zsh", isDirectory: false)
        guard fileManager.fileExists(atPath: zdotdirURL.path),
              fileManager.fileExists(atPath: scriptURL.path) else {
            return nil
        }
        return (zdotdirURL.path, scriptURL.path)
    }

    private func preparedStatusDirectoryPath() -> String? {
        optionalExistingDirectoryPath(
            runtimeTemporaryRootURL(createIfNeeded: false)
                .appendingPathComponent("agent-status", isDirectory: true)
        )
    }

    private func preparedClaudeSessionMapDirectoryPath() -> String? {
        optionalExistingDirectoryPath(claudeSessionMapDirectoryURL(createIfNeeded: false))
    }

    private func runtimeTemporaryRootURL(createIfNeeded: Bool = true) -> URL {
        let url = AppRuntimePaths.temporaryRootURL(fileManager: fileManager)
        if createIfNeeded {
            try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        }
        return url
    }

    private func runtimeOwnerID() -> String {
        AppRuntimePaths.runtimeOwnerID()
    }

    private func toolPermissionSettingsFileURL() -> URL {
        AppRuntimePaths.appSupportRootURL(fileManager: fileManager)!
            .appendingPathComponent("tool-permissions.json", isDirectory: false)
    }

    private func environmentCacheSignature(
        session: TerminalSession,
        processEnvironment: [String: String],
        wrapperPath: String,
        originalPath: String,
        statusDirectoryPath: String?,
        claudeSessionMapDirectoryPath: String?,
        shellHookPaths: (zdotdirPath: String, scriptPath: String)?,
        logFilePath: String,
        runtimeOwner: String,
        toolPermissionSettingsFilePath: String,
        memoryWorkspaceRootPath: String?,
        memoryWorkspaceLinkPath: String?,
        memoryPromptFilePath: String?
    ) -> String {
        let processSignature = Self.passthroughEnvironmentKeys
            .map { key in "\(key)=\(processEnvironment[key] ?? "")" }
            .joined(separator: "|")

        return [
            session.id.uuidString,
            session.projectID.uuidString,
            session.projectName,
            session.title,
            session.cwd,
            session.shell,
            wrapperPath,
            originalPath,
            statusDirectoryPath ?? "",
            claudeSessionMapDirectoryPath ?? "",
            shellHookPaths?.zdotdirPath ?? "",
            shellHookPaths?.scriptPath ?? "",
            logFilePath,
            runtimeOwner,
            toolPermissionSettingsFilePath,
            memoryWorkspaceRootPath ?? "",
            memoryWorkspaceLinkPath ?? "",
            memoryPromptFilePath ?? "",
            runtimeEventSocketURL().path,
            processSignature,
        ].joined(separator: "\u{1F}")
    }

    private func optionalExistingDirectoryPath(_ url: URL) -> String? {
        fileManager.fileExists(atPath: url.path) ? url.path : nil
    }
}

private extension NSLock {
    func withLock<T>(_ work: () -> T) -> T {
        lock()
        defer { unlock() }
        return work()
    }
}
