import Foundation

struct AIRuntimeBridgeService {
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

    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared
    private let codexManagedHookStatusMessage = "dmux codex live"
    private let geminiManagedHookStatusMessage = "dmux gemini live"

    func runtimeSupportRootURL(createIfNeeded: Bool = true) -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let url = appSupport.appendingPathComponent(runtimeSupportDirectoryName(), isDirectory: true)
        if createIfNeeded {
            try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        }
        return url
    }

    func claudeSessionMapDirectoryURL(createIfNeeded: Bool = true) -> URL {
        let url = runtimeSupportRootURL(createIfNeeded: createIfNeeded)
            .appendingPathComponent("claude-session-map", isDirectory: true)
        if createIfNeeded {
            try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        }
        return url
    }

    func clearAllClaudeSessionMappings() {
        clearJSONFiles(in: claudeSessionMapDirectoryURL())
    }

    func clearLegacyLiveRuntimeState() {
        let rootURL = runtimeSupportRootURL(createIfNeeded: false)
        clearJSONFiles(in: rootURL.appendingPathComponent("ai-usage-live", isDirectory: true))
        clearJSONFiles(in: rootURL.appendingPathComponent("ai-response-live", isDirectory: true))
        clearJSONFiles(in: rootURL.appendingPathComponent("ai-usage-inbox", isDirectory: true))
        clearJSONFiles(in: rootURL.appendingPathComponent("ai-response-inbox", isDirectory: true))
        clearJSONFiles(in: rootURL.appendingPathComponent("codex-hook-inbox", isDirectory: true))
    }

    func runtimeEventSocketURL() -> URL {
        URL(
            fileURLWithPath: "/tmp/\(runtimeSocketFileName())",
            isDirectory: false
        )
    }

    func environment(for session: TerminalSession) -> [(String, String)] {
        environmentResolution(for: session).pairs
    }

    func environmentResolution(for session: TerminalSession) -> EnvironmentResolution {
        scheduleManagedHookBootstrapIfNeeded()
        let wrapperPath = wrapperBinURL().path
        let processEnvironment = ProcessInfo.processInfo.environment
        let originalPath = processEnvironment["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin"
        let statusDirectoryPath = preparedStatusDirectoryPath()
        let claudeSessionMapDirectoryPath = preparedClaudeSessionMapDirectoryPath()
        let shellHookPaths = preparedShellHookPaths()
        let logFilePath = AppDebugLog.shared.logFileURL().path
        let signature = environmentCacheSignature(
            session: session,
            processEnvironment: processEnvironment,
            wrapperPath: wrapperPath,
            originalPath: originalPath,
            statusDirectoryPath: statusDirectoryPath,
            claudeSessionMapDirectoryPath: claudeSessionMapDirectoryPath,
            shellHookPaths: shellHookPaths,
            logFilePath: logFilePath
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

        let passthroughKeys = [
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

        var merged: [String: String] = [:]
        for key in passthroughKeys {
            if let value = processEnvironment[key], !value.isEmpty {
                merged[key] = value
            }
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
                "bootstrap namespace channel=\(service.runtimeChannel()) root=\(service.runtimeSupportRootURL().path) socket=\(service.runtimeEventSocketURL().path)"
            )
            service.debugLog.log("runtime-hooks", "bootstrap step=status-directory")
            _ = service.statusDirectoryURL()
            service.debugLog.log("runtime-hooks", "bootstrap step=claude-session-map")
            _ = service.claudeSessionMapDirectoryURL()
            service.debugLog.log("runtime-hooks", "bootstrap step=shell-hooks")
            service.ensureShellHooksStaged()
            service.debugLog.log("runtime-hooks", "bootstrap step=codex-hooks")
            service.ensureCodexHooksInstalled()
            service.debugLog.log("runtime-hooks", "bootstrap step=gemini-hooks")
            service.ensureGeminiHooksInstalled()
            service.debugLog.log("runtime-hooks", "bootstrap complete")
        }) else {
            return
        }

        debugLog.log("runtime-hooks", "bootstrap scheduled")
    }

    private func codexHooksFileURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent(".codex", isDirectory: true)
            .appendingPathComponent("hooks.json", isDirectory: false)
    }

    private func geminiSettingsFileURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent(".gemini", isDirectory: true)
            .appendingPathComponent("settings.json", isDirectory: false)
    }

    func statusDirectoryURL() -> URL {
        let url = runtimeSupportRootURL()
            .appendingPathComponent("agent-status", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func shellHookZshDirectoryURL() -> URL {
        ensureShellHooksStaged()
        return stagedShellHooksRootURL().appendingPathComponent("zsh", isDirectory: true)
    }

    private func shellHookZshScriptURL() -> URL {
        ensureShellHooksStaged()
        return stagedShellHooksRootURL().appendingPathComponent("dmux-ai-hook.zsh", isDirectory: false)
    }

    private func stagedShellHooksRootURL() -> URL {
        runtimeSupportRootURL().appendingPathComponent("shell-hooks", isDirectory: true)
    }

    private func ensureShellHooksStaged() {
        let rootURL = stagedShellHooksRootURL()
        let zshDirectoryURL = rootURL.appendingPathComponent("zsh", isDirectory: true)

        try? fileManager.createDirectory(at: rootURL, withIntermediateDirectories: true)
        try? fileManager.createDirectory(at: zshDirectoryURL, withIntermediateDirectories: true)

        stageShellHookResource("scripts/shell-hooks/zsh/.zshenv", to: zshDirectoryURL.appendingPathComponent(".zshenv"))
        stageShellHookResource("scripts/shell-hooks/zsh/.zprofile", to: zshDirectoryURL.appendingPathComponent(".zprofile"))
        stageShellHookResource("scripts/shell-hooks/zsh/.zshrc", to: zshDirectoryURL.appendingPathComponent(".zshrc"))
        stageShellHookResource("scripts/shell-hooks/zsh/.zlogin", to: zshDirectoryURL.appendingPathComponent(".zlogin"))
        stageShellHookResource("scripts/shell-hooks/dmux-ai-hook.zsh", to: rootURL.appendingPathComponent("dmux-ai-hook.zsh"))
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
        let url = runtimeSupportRootURL(createIfNeeded: false)
            .appendingPathComponent("agent-status", isDirectory: true)
        return fileManager.fileExists(atPath: url.path) ? url.path : nil
    }

    private func preparedClaudeSessionMapDirectoryPath() -> String? {
        let url = claudeSessionMapDirectoryURL(createIfNeeded: false)
        return fileManager.fileExists(atPath: url.path) ? url.path : nil
    }

    private func runtimeSupportDirectoryName() -> String {
        let channel = runtimeChannel()
        if channel == "release" {
            return "dmux"
        }
        return "dmux-\(channel)"
    }

    private func runtimeSocketFileName() -> String {
        let channel = runtimeChannel()
        if channel == "release" {
            return "dmux-runtime-events.sock"
        }
        return "dmux-runtime-events-\(channel).sock"
    }

    private func runtimeChannel() -> String {
        if let override = ProcessInfo.processInfo.environment["DMUX_RUNTIME_CHANNEL"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased(),
           !override.isEmpty {
            return sanitizeRuntimeChannel(override)
        }

        let bundleName = Bundle.main.bundleURL
            .deletingPathExtension()
            .lastPathComponent
            .lowercased()
        if bundleName.contains("dev") {
            return "dev"
        }
        if bundleName.contains("beta") {
            return "beta"
        }
        return "release"
    }

    private func sanitizeRuntimeChannel(_ value: String) -> String {
        let filtered = value.filter { $0.isLetter || $0.isNumber || $0 == "-" }
        return filtered.isEmpty ? "release" : filtered
    }

    private func environmentCacheSignature(
        session: TerminalSession,
        processEnvironment: [String: String],
        wrapperPath: String,
        originalPath: String,
        statusDirectoryPath: String?,
        claudeSessionMapDirectoryPath: String?,
        shellHookPaths: (zdotdirPath: String, scriptPath: String)?,
        logFilePath: String
    ) -> String {
        let passthroughKeys = [
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

        let processSignature = passthroughKeys
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
            runtimeEventSocketURL().path,
            processSignature,
        ].joined(separator: "\u{1F}")
    }

    private func ensureCodexHooksInstalled() {
        let hooksFileURL = codexHooksFileURL()
        let hooksDirectoryURL = hooksFileURL.deletingLastPathComponent()

        try? fileManager.createDirectory(at: hooksDirectoryURL, withIntermediateDirectories: true)

        var rootObject: [String: Any] = [:]
        if let existingData = try? Data(contentsOf: hooksFileURL),
           !existingData.isEmpty {
            guard let jsonObject = try? JSONSerialization.jsonObject(with: existingData),
                  let dictionary = jsonObject as? [String: Any] else {
                let backupURL = backupInvalidJSONFile(at: hooksFileURL)
                debugLog.log(
                    "codex-hook-config",
                    "recovered invalid hooks.json path=\(hooksFileURL.path) backup=\(backupURL?.lastPathComponent ?? "nil")"
                )
                rootObject = [:]
                installCodexHooks(&rootObject)
                return
            }
            rootObject = dictionary
        }

        installCodexHooks(&rootObject)
    }

    private func installCodexHooks(_ rootObject: inout [String: Any]) {
        let hooksFileURL = codexHooksFileURL()
        let helperScriptURL = WorkspacePaths.repositoryResourceURL("scripts/wrappers/dmux-ai-state.sh")
        let promptSubmitCommand = codexHookCommand(helperScriptURL: helperScriptURL, action: "codex-prompt-submit")
        let stopCommand = codexHookCommand(helperScriptURL: helperScriptURL, action: "codex-stop")

        var hooksObject = rootObject["hooks"] as? [String: Any] ?? [:]
        hooksObject["UserPromptSubmit"] = mergedCodexHookGroups(
            existingValue: hooksObject["UserPromptSubmit"],
            command: promptSubmitCommand,
            action: "codex-prompt-submit",
            helperScriptURL: helperScriptURL
        )
        hooksObject["Stop"] = mergedCodexHookGroups(
            existingValue: hooksObject["Stop"],
            command: stopCommand,
            action: "codex-stop",
            helperScriptURL: helperScriptURL
        )
        rootObject["hooks"] = hooksObject

        guard JSONSerialization.isValidJSONObject(rootObject),
              let data = try? JSONSerialization.data(withJSONObject: rootObject, options: [.prettyPrinted, .sortedKeys]) else {
            debugLog.log("codex-hook-config", "failed to encode hooks.json path=\(hooksFileURL.path)")
            return
        }

        if let existingData = try? Data(contentsOf: hooksFileURL),
           existingData == data {
            return
        }

        do {
            try data.write(to: hooksFileURL, options: .atomic)
            debugLog.log("codex-hook-config", "installed hooks path=\(hooksFileURL.path)")
        } catch {
            debugLog.log("codex-hook-config", "write failed path=\(hooksFileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func ensureGeminiHooksInstalled() {
        let settingsFileURL = geminiSettingsFileURL()
        let settingsDirectoryURL = settingsFileURL.deletingLastPathComponent()

        try? fileManager.createDirectory(at: settingsDirectoryURL, withIntermediateDirectories: true)

        var rootObject: [String: Any] = [:]
        if let existingData = try? Data(contentsOf: settingsFileURL),
           !existingData.isEmpty {
            guard let jsonObject = try? JSONSerialization.jsonObject(with: existingData),
                  let dictionary = jsonObject as? [String: Any] else {
                let backupURL = backupInvalidJSONFile(at: settingsFileURL)
                debugLog.log(
                    "gemini-hook-config",
                    "recovered invalid settings path=\(settingsFileURL.path) backup=\(backupURL?.lastPathComponent ?? "nil")"
                )
                rootObject = [:]
                installGeminiHooks(&rootObject)
                return
            }
            rootObject = dictionary
        }

        installGeminiHooks(&rootObject)
    }

    private func installGeminiHooks(_ rootObject: inout [String: Any]) {
        let settingsFileURL = geminiSettingsFileURL()
        let helperScriptURL = WorkspacePaths.repositoryResourceURL("scripts/wrappers/dmux-ai-state.sh")
        let sessionStartCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "session-start")
        let beforeAgentCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "before-agent")
        let afterAgentCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "after-agent")
        let sessionEndCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "session-end")

        var hooksObject = rootObject["hooks"] as? [String: Any] ?? [:]
        hooksObject["SessionStart"] = mergedGeminiHookGroups(
            existingValue: hooksObject["SessionStart"],
            command: sessionStartCommand,
            action: "session-start",
            helperScriptURL: helperScriptURL
        )
        hooksObject["BeforeAgent"] = mergedGeminiHookGroups(
            existingValue: hooksObject["BeforeAgent"],
            command: beforeAgentCommand,
            action: "before-agent",
            helperScriptURL: helperScriptURL
        )
        hooksObject["AfterAgent"] = mergedGeminiHookGroups(
            existingValue: hooksObject["AfterAgent"],
            command: afterAgentCommand,
            action: "after-agent",
            helperScriptURL: helperScriptURL
        )
        hooksObject["SessionEnd"] = mergedGeminiHookGroups(
            existingValue: hooksObject["SessionEnd"],
            command: sessionEndCommand,
            action: "session-end",
            helperScriptURL: helperScriptURL
        )
        rootObject["hooks"] = hooksObject

        guard JSONSerialization.isValidJSONObject(rootObject),
              let data = try? JSONSerialization.data(withJSONObject: rootObject, options: [.prettyPrinted, .sortedKeys]) else {
            debugLog.log("gemini-hook-config", "failed to encode settings path=\(settingsFileURL.path)")
            return
        }

        if let existingData = try? Data(contentsOf: settingsFileURL),
           existingData == data {
            return
        }

        do {
            try data.write(to: settingsFileURL, options: .atomic)
            debugLog.log("gemini-hook-config", "installed hooks path=\(settingsFileURL.path)")
        } catch {
            debugLog.log("gemini-hook-config", "write failed path=\(settingsFileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func mergedCodexHookGroups(
        existingValue: Any?,
        command: String,
        action: String,
        helperScriptURL: URL
    ) -> [[String: Any]] {
        let existingGroups = existingValue as? [[String: Any]] ?? []
        var nextGroups: [[String: Any]] = []

        for group in existingGroups {
            var nextGroup = group
            let hooks = group["hooks"] as? [[String: Any]] ?? []
            let filteredHooks = hooks.filter { hook in
                !isManagedCodexHook(
                    hook,
                    action: action,
                    helperScriptURL: helperScriptURL
                )
            }

            guard !filteredHooks.isEmpty else {
                continue
            }

            nextGroup["hooks"] = filteredHooks
            nextGroups.append(nextGroup)
        }

        nextGroups.append([
            "matcher": "",
            "hooks": [[
                "type": "command",
                "command": command,
                "timeout": 5000,
                "statusMessage": codexManagedHookStatusMessage,
            ]],
        ])

        return nextGroups
    }

    private func isManagedCodexHook(
        _ hook: [String: Any],
        action: String,
        helperScriptURL: URL
    ) -> Bool {
        if let statusMessage = hook["statusMessage"] as? String,
           statusMessage == codexManagedHookStatusMessage {
            return true
        }

        guard let type = hook["type"] as? String,
              type == "command",
              let command = hook["command"] as? String else {
            return false
        }

        return command.contains(helperScriptURL.path) && command.contains(action)
    }

    private func mergedGeminiHookGroups(
        existingValue: Any?,
        command: String,
        action: String,
        helperScriptURL: URL
    ) -> [[String: Any]] {
        let existingGroups = existingValue as? [[String: Any]] ?? []
        var nextGroups: [[String: Any]] = []

        for group in existingGroups {
            var nextGroup = group
            let hooks = group["hooks"] as? [[String: Any]] ?? []
            let filteredHooks = hooks.filter { hook in
                !isManagedGeminiHook(
                    hook,
                    action: action,
                    helperScriptURL: helperScriptURL
                )
            }

            guard !filteredHooks.isEmpty else {
                continue
            }

            nextGroup["hooks"] = filteredHooks
            nextGroups.append(nextGroup)
        }

        nextGroups.append([
            "matcher": "",
            "hooks": [[
                "type": "command",
                "command": command,
                "timeout": 5000,
                "statusMessage": geminiManagedHookStatusMessage,
            ]],
        ])

        return nextGroups
    }

    private func isManagedGeminiHook(
        _ hook: [String: Any],
        action: String,
        helperScriptURL: URL
    ) -> Bool {
        if let statusMessage = hook["statusMessage"] as? String,
           statusMessage == geminiManagedHookStatusMessage {
            return true
        }

        guard let type = hook["type"] as? String,
              type == "command",
              let command = hook["command"] as? String else {
            return false
        }

        return command.contains(helperScriptURL.path) && command.contains(action)
    }

    private func codexHookCommand(helperScriptURL: URL, action: String) -> String {
        [
            shellQuoted(helperScriptURL.path),
            shellQuoted(action),
            shellQuoted("codex"),
        ].joined(separator: " ")
    }

    private func geminiHookCommand(helperScriptURL: URL, action: String) -> String {
        [
            shellQuoted(helperScriptURL.path),
            shellQuoted(action),
            shellQuoted("gemini"),
        ].joined(separator: " ")
    }

    private func shellQuoted(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private func stageShellHookResource(_ relativePath: String, to destinationURL: URL) {
        let sourceURL = WorkspacePaths.repositoryResourceURL(relativePath)
        debugLog.log("runtime-hooks", "stage-shell-hook source=\(relativePath)")
        guard let contentData = try? Data(contentsOf: sourceURL) else {
            debugLog.log("runtime-hooks", "stage-shell-hook missing source=\(relativePath)")
            return
        }
        if let existingData = try? Data(contentsOf: destinationURL),
           existingData == contentData {
            debugLog.log("runtime-hooks", "stage-shell-hook unchanged source=\(relativePath)")
            return
        }
        try? fileManager.removeItem(at: destinationURL)
        try? contentData.write(to: destinationURL, options: .atomic)
        debugLog.log("runtime-hooks", "stage-shell-hook wrote source=\(relativePath)")
    }

    private func clearJSONFiles(in directory: URL) {
        try? fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        guard let fileURLs = try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) else {
            return
        }
        for fileURL in fileURLs where fileURL.pathExtension == "json" {
            try? fileManager.removeItem(at: fileURL)
        }
    }

    private func backupInvalidJSONFile(at fileURL: URL) -> URL? {
        let timestamp = Self.invalidFileDateFormatter.string(from: Date())
        let backupURL = fileURL
            .deletingLastPathComponent()
            .appendingPathComponent("\(fileURL.deletingPathExtension().lastPathComponent).invalid-\(timestamp).\(fileURL.pathExtension)")

        do {
            if fileManager.fileExists(atPath: backupURL.path) {
                try fileManager.removeItem(at: backupURL)
            }
            try fileManager.copyItem(at: fileURL, to: backupURL)
            return backupURL
        } catch {
            debugLog.log(
                "hook-config",
                "backup failed source=\(fileURL.path) target=\(backupURL.path) error=\(error.localizedDescription)"
            )
            return nil
        }
    }
}

private extension NSLock {
    func withLock<T>(_ work: () -> T) -> T {
        lock()
        defer { unlock() }
        return work()
    }
}

private extension AIRuntimeBridgeService {
    static let invalidFileDateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter
    }()
}
