import Foundation

extension AIRuntimeBridgeService {
    func codexHooksFileURL() -> URL {
        toolConfigFileURL(directoryName: ".codex", filename: "hooks.json")
    }

    func codexConfigFileURL() -> URL {
        toolConfigFileURL(directoryName: ".codex", filename: "config.toml")
    }

    func claudeSettingsFileURL() -> URL {
        toolConfigFileURL(directoryName: ".claude", filename: "settings.json")
    }

    func geminiSettingsFileURL() -> URL {
        toolConfigFileURL(directoryName: ".gemini", filename: "settings.json")
    }

    func managedHooksDirectoryURL() -> URL {
        let url = runtimeSupportRootURL().appendingPathComponent("runtime-hooks", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    func managedRuntimeHookHelperURL() -> URL {
        let destinationURL = managedHooksDirectoryURL().appendingPathComponent("dmux-ai-state.sh", isDirectory: false)
        stageResource(
            "scripts/wrappers/dmux-ai-state.sh",
            to: destinationURL,
            logLabel: "runtime-hook",
            executable: true
        )
        return destinationURL
    }

    func ensureCodexConfigInstalled() {
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

    func installClaudeHooks(_ rootObject: inout [String: Any]) {
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

    func installCodexHooks(_ rootObject: inout [String: Any]) {
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

    func ensureManagedHookConfig(
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

    func installGeminiHooks(_ rootObject: inout [String: Any]) {
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

    func installManagedHooks(
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

    func applyManagedHookSpecs(
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

    func managedHookSpecs(
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

    func mergedManagedHookGroups(
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

    func strippedManagedHookGroups(
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

    func isManagedHook(
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

    func hookCommand(helperScriptURL: URL, action: String, tool: String) -> String {
        [
            shellQuoted(helperScriptURL.path),
            shellQuoted(action),
            shellQuoted(tool),
        ].joined(separator: " ")
    }

    func toolConfigFileURL(directoryName: String, filename: String) -> URL {
        URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent(directoryName, isDirectory: true)
            .appendingPathComponent(filename, isDirectory: false)
    }

    func loadJSONObjectConfig(
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

    func writeJSONObjectConfig(
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

    func shellQuoted(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    func stageResource(
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

    func clearJSONFiles(in directory: URL) {
        try? fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        guard let fileURLs = try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) else {
            return
        }
        for fileURL in fileURLs where fileURL.pathExtension == "json" {
            try? fileManager.removeItem(at: fileURL)
        }
    }

    func backupInvalidJSONFile(at fileURL: URL) -> URL? {
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

private extension AIRuntimeBridgeService {
    static let invalidFileDateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter
    }()
}
