import Foundation

struct AIRuntimeBridgeService {
    private struct ManagedHookSpec {
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

    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared

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

    func runtimeEventSocketURL() -> URL {
        URL(
            fileURLWithPath: "/tmp/\(runtimeSocketFileName())",
            isDirectory: false
        )
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

        var merged: [String: String] = [:]
        for key in Self.passthroughEnvironmentKeys {
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

    private func codexHooksFileURL() -> URL {
        toolConfigFileURL(directoryName: ".codex", filename: "hooks.json")
    }

    private func codexConfigFileURL() -> URL {
        toolConfigFileURL(directoryName: ".codex", filename: "config.toml")
    }

    private func claudeSettingsFileURL() -> URL {
        toolConfigFileURL(directoryName: ".claude", filename: "settings.json")
    }

    private func geminiSettingsFileURL() -> URL {
        toolConfigFileURL(directoryName: ".gemini", filename: "settings.json")
    }

    func statusDirectoryURL() -> URL {
        let url = runtimeSupportRootURL()
            .appendingPathComponent("agent-status", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func managedHooksDirectoryURL() -> URL {
        let url = runtimeSupportRootURL().appendingPathComponent("runtime-hooks", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func managedRuntimeHookHelperURL() -> URL {
        let destinationURL = managedHooksDirectoryURL().appendingPathComponent("dmux-ai-state.sh", isDirectory: false)
        stageResource(
            "scripts/wrappers/dmux-ai-state.sh",
            to: destinationURL,
            logLabel: "runtime-hook",
            executable: true
        )
        return destinationURL
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
            runtimeSupportRootURL(createIfNeeded: false)
                .appendingPathComponent("agent-status", isDirectory: true)
        )
    }

    private func preparedClaudeSessionMapDirectoryPath() -> String? {
        optionalExistingDirectoryPath(claudeSessionMapDirectoryURL(createIfNeeded: false))
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
            runtimeEventSocketURL().path,
            processSignature,
        ].joined(separator: "\u{1F}")
    }

    private func ensureCodexConfigInstalled() {
        let configFileURL = codexConfigFileURL()
        let configDirectoryURL = configFileURL.deletingLastPathComponent()
        try? fileManager.createDirectory(at: configDirectoryURL, withIntermediateDirectories: true)

        let existingText = (try? String(contentsOf: configFileURL, encoding: .utf8)) ?? ""
        let targetLine = "suppress_unstable_features_warning = true"
        let updatedText: String

        if existingText.range(
            of: #"(?m)^\s*suppress_unstable_features_warning\s*=\s*.+$"#,
            options: .regularExpression
        ) != nil {
            updatedText = existingText.replacingOccurrences(
                of: #"(?m)^\s*suppress_unstable_features_warning\s*=\s*.+$"#,
                with: targetLine,
                options: .regularExpression
            )
        } else if existingText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            updatedText = "\(targetLine)\n"
        } else {
            let suffix = existingText.hasSuffix("\n") ? "" : "\n"
            updatedText = "\(existingText)\(suffix)\(targetLine)\n"
        }

        guard updatedText != existingText else {
            return
        }

        do {
            try updatedText.write(to: configFileURL, atomically: true, encoding: .utf8)
            debugLog.log("codex-hook-config", "updated config path=\(configFileURL.path)")
        } catch {
            debugLog.log("codex-hook-config", "config write failed path=\(configFileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func installClaudeHooks(_ rootObject: inout [String: Any]) {
        installManagedHooks(
            &rootObject,
            fileURL: claudeSettingsFileURL(),
            tool: "claude",
            category: "claude-hook-config",
            description: "settings",
            definitions: [
                ("SessionStart", "session-start", 10, false),
                ("UserPromptSubmit", "prompt-submit", 10, false),
                ("Stop", "stop", 10, false),
                ("StopFailure", "stop-failure", 10, false),
                ("SessionEnd", "session-end", 1, false),
                ("PreToolUse", "pre-tool-use", 5, true),
                ("PostToolUse", "post-tool-use", 5, true),
                ("PostToolUseFailure", "post-tool-use-failure", 5, true),
                ("PermissionRequest", "permission-request", 5, true),
                ("PermissionDenied", "permission-denied", 5, true),
                ("Elicitation", "elicitation", 10, false),
                ("ElicitationResult", "elicitation-result", 10, false),
            ],
            notificationActionToStrip: "notification"
        )
    }

    private func installCodexHooks(_ rootObject: inout [String: Any]) {
        installManagedHooks(
            &rootObject,
            fileURL: codexHooksFileURL(),
            tool: "codex",
            category: "codex-hook-config",
            description: "hooks.json",
            definitions: [
                ("SessionStart", "codex-session-start", 1000, false),
                ("UserPromptSubmit", "codex-prompt-submit", 1000, false),
                ("PreToolUse", "codex-pre-tool-use", 1000, false),
                ("PostToolUse", "codex-post-tool-use", 1000, false),
                ("Stop", "codex-stop", 1000, false),
            ]
        )
    }

    private func ensureManagedHookConfig(
        at fileURL: URL,
        category: String,
        invalidDescription: String,
        install: (inout [String: Any]) -> Void
    ) {
        try? fileManager.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        var rootObject = loadJSONObjectConfig(
            at: fileURL,
            category: category,
            invalidDescription: invalidDescription
        )
        install(&rootObject)
    }

    private func installGeminiHooks(_ rootObject: inout [String: Any]) {
        installManagedHooks(
            &rootObject,
            fileURL: geminiSettingsFileURL(),
            tool: "gemini",
            category: "gemini-hook-config",
            description: "settings",
            definitions: [
                ("SessionStart", "session-start", 5000, false),
                ("BeforeAgent", "before-agent", 5000, false),
                ("AfterAgent", "after-agent", 5000, false),
                ("Notification", "notification", 5000, false),
                ("SessionEnd", "session-end", 5000, false),
            ]
        )
    }

    private func installManagedHooks(
        _ rootObject: inout [String: Any],
        fileURL: URL,
        tool: String,
        category: String,
        description: String,
        definitions: [(eventKey: String, action: String, timeout: Int, async: Bool)],
        notificationActionToStrip: String? = nil
    ) {
        let helperScriptURL = managedRuntimeHookHelperURL()
        let statusMessage = "dmux \(tool) live"
        var hooksObject = rootObject["hooks"] as? [String: Any] ?? [:]
        let specs = managedHookSpecs(
            tool: tool,
            statusMessage: statusMessage,
            helperScriptURL: helperScriptURL,
            definitions: definitions
        )
        applyManagedHookSpecs(specs, to: &hooksObject, helperScriptURL: helperScriptURL)

        if let notificationActionToStrip {
            let notificationHookGroups = strippedManagedHookGroups(
                existingValue: hooksObject["Notification"],
                action: notificationActionToStrip,
                helperScriptURL: helperScriptURL,
                statusMessage: statusMessage
            )
            if notificationHookGroups.isEmpty {
                hooksObject.removeValue(forKey: "Notification")
            } else {
                hooksObject["Notification"] = notificationHookGroups
            }
        }

        rootObject["hooks"] = hooksObject
        writeJSONObjectConfig(rootObject, to: fileURL, category: category, description: description)
    }

    private func applyManagedHookSpecs(
        _ specs: [ManagedHookSpec],
        to hooksObject: inout [String: Any],
        helperScriptURL: URL
    ) {
        for spec in specs {
            hooksObject[spec.eventKey] = mergedManagedHookGroups(
                existingValue: hooksObject[spec.eventKey],
                spec: spec,
                helperScriptURL: helperScriptURL
            )
        }
    }

    private func managedHookSpecs(
        tool: String,
        statusMessage: String,
        helperScriptURL: URL,
        definitions: [(eventKey: String, action: String, timeout: Int, async: Bool)]
    ) -> [ManagedHookSpec] {
        definitions.map { definition in
            ManagedHookSpec(
                eventKey: definition.eventKey,
                action: definition.action,
                command: hookCommand(
                    helperScriptURL: helperScriptURL,
                    action: definition.action,
                    tool: tool
                ),
                statusMessage: statusMessage,
                timeout: definition.timeout,
                async: definition.async
            )
        }
    }

    private func mergedManagedHookGroups(
        existingValue: Any?,
        spec: ManagedHookSpec,
        helperScriptURL: URL
    ) -> [[String: Any]] {
        let nextGroups = strippedManagedHookGroups(
            existingValue: existingValue,
            action: spec.action,
            helperScriptURL: helperScriptURL,
            statusMessage: spec.statusMessage
        )

        var hook: [String: Any] = [
            "type": "command",
            "command": spec.command,
            "timeout": spec.timeout,
            "statusMessage": spec.statusMessage,
        ]
        if spec.async {
            hook["async"] = true
        }

        return nextGroups + [[
            "matcher": "",
            "hooks": [hook],
        ]]
    }

    private func strippedManagedHookGroups(
        existingValue: Any?,
        action: String,
        helperScriptURL: URL,
        statusMessage: String
    ) -> [[String: Any]] {
        let existingGroups = existingValue as? [[String: Any]] ?? []
        return existingGroups.compactMap { group in
            let hooks = group["hooks"] as? [[String: Any]] ?? []
            let filteredHooks = hooks.filter { hook in
                !isManagedHook(
                    hook,
                    action: action,
                    helperScriptURL: helperScriptURL,
                    statusMessage: statusMessage
                )
            }

            guard !filteredHooks.isEmpty else {
                return nil
            }

            var nextGroup = group
            nextGroup["hooks"] = filteredHooks
            return nextGroup
        }
    }

    private func isManagedHook(
        _ hook: [String: Any],
        action: String,
        helperScriptURL: URL,
        statusMessage expectedStatusMessage: String
    ) -> Bool {
        if let statusMessage = hook["statusMessage"] as? String,
           statusMessage == expectedStatusMessage {
            return true
        }

        guard let type = hook["type"] as? String,
              type == "command",
              let command = hook["command"] as? String else {
            return false
        }

        return command.contains(helperScriptURL.path) && command.contains(action)
    }

    private func hookCommand(helperScriptURL: URL, action: String, tool: String) -> String {
        [
            shellQuoted(helperScriptURL.path),
            shellQuoted(action),
            shellQuoted(tool),
        ].joined(separator: " ")
    }

    private func toolConfigFileURL(directoryName: String, filename: String) -> URL {
        URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent(directoryName, isDirectory: true)
            .appendingPathComponent(filename, isDirectory: false)
    }

    private func loadJSONObjectConfig(
        at fileURL: URL,
        category: String,
        invalidDescription: String
    ) -> [String: Any] {
        guard let existingData = try? Data(contentsOf: fileURL),
              !existingData.isEmpty else {
            return [:]
        }

        guard let jsonObject = try? JSONSerialization.jsonObject(with: existingData),
              let dictionary = jsonObject as? [String: Any] else {
            let backupURL = backupInvalidJSONFile(at: fileURL)
            debugLog.log(
                category,
                "recovered invalid \(invalidDescription) path=\(fileURL.path) backup=\(backupURL?.lastPathComponent ?? "nil")"
            )
            return [:]
        }

        return dictionary
    }

    private func writeJSONObjectConfig(
        _ rootObject: [String: Any],
        to fileURL: URL,
        category: String,
        description: String
    ) {
        guard JSONSerialization.isValidJSONObject(rootObject),
              let data = try? JSONSerialization.data(withJSONObject: rootObject, options: [.prettyPrinted, .sortedKeys]) else {
            debugLog.log(category, "failed to encode \(description) path=\(fileURL.path)")
            return
        }

        if let existingData = try? Data(contentsOf: fileURL),
           existingData == data {
            return
        }

        do {
            try data.write(to: fileURL, options: .atomic)
            debugLog.log(category, "installed hooks path=\(fileURL.path)")
        } catch {
            debugLog.log(category, "write failed path=\(fileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func shellQuoted(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private func optionalExistingDirectoryPath(_ url: URL) -> String? {
        fileManager.fileExists(atPath: url.path) ? url.path : nil
    }

    private func stageResource(
        _ relativePath: String,
        to destinationURL: URL,
        logLabel: String,
        executable: Bool = false
    ) {
        let sourceURL = WorkspacePaths.repositoryResourceURL(relativePath)
        debugLog.log("runtime-hooks", "stage-\(logLabel) source=\(relativePath)")
        guard let contentData = try? Data(contentsOf: sourceURL) else {
            debugLog.log("runtime-hooks", "stage-\(logLabel) missing source=\(relativePath)")
            return
        }
        if let existingData = try? Data(contentsOf: destinationURL),
           existingData == contentData {
            if executable, fileManager.isExecutableFile(atPath: destinationURL.path) == false {
                try? fileManager.setAttributes([.posixPermissions: 0o755], ofItemAtPath: destinationURL.path)
            }
            debugLog.log("runtime-hooks", "stage-\(logLabel) unchanged source=\(relativePath)")
            return
        }
        try? fileManager.removeItem(at: destinationURL)
        try? contentData.write(to: destinationURL, options: .atomic)
        if executable {
            try? fileManager.setAttributes([.posixPermissions: 0o755], ofItemAtPath: destinationURL.path)
        }
        debugLog.log("runtime-hooks", "stage-\(logLabel) wrote source=\(relativePath)")
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
