import Foundation

struct AIRuntimeBridgeService {
    private let fileManager = FileManager.default
    private let codexManagedHookStatusMessage = "dmux codex live"
    private let geminiManagedHookStatusMessage = "dmux gemini live"

    func claudeSessionMapDirectoryURL() -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let url = appSupport.appendingPathComponent("dmux/claude-session-map", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    func clearAllClaudeSessionMappings() {
        clearJSONFiles(in: claudeSessionMapDirectoryURL())
    }

    func clearLegacyLiveRuntimeState() {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        clearJSONFiles(in: appSupport.appendingPathComponent("dmux/ai-usage-live", isDirectory: true))
        clearJSONFiles(in: appSupport.appendingPathComponent("dmux/ai-response-live", isDirectory: true))
        clearJSONFiles(in: appSupport.appendingPathComponent("dmux/ai-usage-inbox", isDirectory: true))
        clearJSONFiles(in: appSupport.appendingPathComponent("dmux/ai-response-inbox", isDirectory: true))
        clearJSONFiles(in: appSupport.appendingPathComponent("dmux/codex-hook-inbox", isDirectory: true))
    }

    func runtimeEventSocketURL() -> URL {
        URL(fileURLWithPath: "/tmp/dmux-runtime-events.sock", isDirectory: false)
    }

    func environment(for session: TerminalSession) -> [(String, String)] {
        ensureCodexHooksInstalled()
        ensureGeminiHooksInstalled()

        let wrapperPath = wrapperBinURL().path
        let processEnvironment = ProcessInfo.processInfo.environment
        let originalPath = processEnvironment["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin"

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
        merged["DMUX_STATUS_DIR"] = statusDirectoryURL().path
        merged["DMUX_RUNTIME_SOCKET"] = runtimeEventSocketURL().path
        merged["DMUX_CLAUDE_SESSION_MAP_DIR"] = claudeSessionMapDirectoryURL().path
        merged["DMUX_LOG_FILE"] = AppDebugLog.shared.logFileURL().path
        merged["DMUX_PROJECT_ID"] = session.projectID.uuidString
        merged["DMUX_PROJECT_NAME"] = session.projectName
        merged["DMUX_PROJECT_PATH"] = session.cwd
        merged["DMUX_SESSION_ID"] = session.id.uuidString
        merged["DMUX_SESSION_TITLE"] = session.title
        merged["DMUX_SESSION_CWD"] = session.cwd
        merged["DMUX_ZSH_HOOK_SCRIPT"] = shellHookZshScriptURL().path
        merged["ZDOTDIR"] = shellHookZshDirectoryURL().path
        merged["TERM"] = "xterm-256color"
        merged["TERM_PROGRAM"] = "dmux"
        merged["LANG"] = merged["LANG"] ?? "en_US.UTF-8"
        merged["LC_CTYPE"] = merged["LC_CTYPE"] ?? merged["LANG"]

        AppDebugLog.shared.log(
            "terminal-env",
            "session=\(session.id.uuidString) shell=\(session.shell) cwd=\(session.cwd) zdotdir=\(merged["ZDOTDIR"] ?? "nil") wrapper=\(merged["DMUX_WRAPPER_BIN"] ?? "nil") pathPrefix=\(merged["PATH"]?.split(separator: ":").prefix(3).joined(separator: ":") ?? "nil")"
        )

        return merged.sorted { $0.key < $1.key }
    }

    private func wrapperBinURL() -> URL {
        WorkspacePaths.repositoryResourceURL("scripts/wrappers/bin")
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

    private func statusDirectoryURL() -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let url = appSupport.appendingPathComponent("dmux/agent-status", isDirectory: true)
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
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("dmux/shell-hooks", isDirectory: true)
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

    private func ensureCodexHooksInstalled() {
        let hooksFileURL = codexHooksFileURL()
        let hooksDirectoryURL = hooksFileURL.deletingLastPathComponent()
        let helperScriptURL = WorkspacePaths.repositoryResourceURL("scripts/wrappers/dmux-ai-state.sh")

        try? fileManager.createDirectory(at: hooksDirectoryURL, withIntermediateDirectories: true)

        let promptSubmitCommand = codexHookCommand(helperScriptURL: helperScriptURL, action: "codex-prompt-submit")
        let stopCommand = codexHookCommand(helperScriptURL: helperScriptURL, action: "codex-stop")

        var rootObject: [String: Any] = [:]
        if let existingData = try? Data(contentsOf: hooksFileURL),
           !existingData.isEmpty {
            guard let jsonObject = try? JSONSerialization.jsonObject(with: existingData),
                  let dictionary = jsonObject as? [String: Any] else {
                AppDebugLog.shared.log("codex-hook-config", "skip invalid hooks.json path=\(hooksFileURL.path)")
                return
            }
            rootObject = dictionary
        }

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
            AppDebugLog.shared.log("codex-hook-config", "failed to encode hooks.json path=\(hooksFileURL.path)")
            return
        }

        if let existingData = try? Data(contentsOf: hooksFileURL),
           existingData == data {
            return
        }

        do {
            try data.write(to: hooksFileURL, options: .atomic)
            AppDebugLog.shared.log("codex-hook-config", "installed hooks path=\(hooksFileURL.path)")
        } catch {
            AppDebugLog.shared.log("codex-hook-config", "write failed path=\(hooksFileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func ensureGeminiHooksInstalled() {
        let settingsFileURL = geminiSettingsFileURL()
        let settingsDirectoryURL = settingsFileURL.deletingLastPathComponent()
        let helperScriptURL = WorkspacePaths.repositoryResourceURL("scripts/wrappers/dmux-ai-state.sh")

        try? fileManager.createDirectory(at: settingsDirectoryURL, withIntermediateDirectories: true)

        let sessionStartCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "session-start")
        let beforeAgentCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "before-agent")
        let afterAgentCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "after-agent")
        let sessionEndCommand = geminiHookCommand(helperScriptURL: helperScriptURL, action: "session-end")

        var rootObject: [String: Any] = [:]
        if let existingData = try? Data(contentsOf: settingsFileURL),
           !existingData.isEmpty {
            guard let jsonObject = try? JSONSerialization.jsonObject(with: existingData),
                  let dictionary = jsonObject as? [String: Any] else {
                AppDebugLog.shared.log("gemini-hook-config", "skip invalid settings path=\(settingsFileURL.path)")
                return
            }
            rootObject = dictionary
        }

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
            AppDebugLog.shared.log("gemini-hook-config", "failed to encode settings path=\(settingsFileURL.path)")
            return
        }

        if let existingData = try? Data(contentsOf: settingsFileURL),
           existingData == data {
            return
        }

        do {
            try data.write(to: settingsFileURL, options: .atomic)
            AppDebugLog.shared.log("gemini-hook-config", "installed hooks path=\(settingsFileURL.path)")
        } catch {
            AppDebugLog.shared.log("gemini-hook-config", "write failed path=\(settingsFileURL.path) error=\(error.localizedDescription)")
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
        guard let contentData = try? Data(contentsOf: sourceURL) else {
            return
        }
        if let existingData = try? Data(contentsOf: destinationURL),
           existingData == contentData {
            return
        }
        try? fileManager.removeItem(at: destinationURL)
        try? contentData.write(to: destinationURL, options: .atomic)
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
}
