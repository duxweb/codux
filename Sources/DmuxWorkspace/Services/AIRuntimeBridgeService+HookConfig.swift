import CryptoKit
import Foundation

extension AIRuntimeBridgeService {
    struct CodexHookTrustState: Equatable {
        var key: String
        var trustedHash: String
    }

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
        let updatedText = updatedCodexConfigText(
            from: existingText,
            codexHookTrustStates: managedCodexHookTrustStates()
        )

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

    func updatedCodexConfigText(
        from existingText: String,
        codexHookTrustStates: [CodexHookTrustState] = []
    ) -> String {
        let targetLine = "suppress_unstable_features_warning = true"
        let featuresHeader = "[features]"
        let hooksFeatureLine = "hooks = true"

        func normalized(_ line: String) -> String {
            line.trimmingCharacters(in: .whitespaces)
        }

        func isSuppressLine(_ line: String) -> Bool {
            normalized(line).hasPrefix("suppress_unstable_features_warning")
        }

        func isTableHeader(_ line: String) -> Bool {
            let trimmed = normalized(line)
            return trimmed.hasPrefix("[") && trimmed.hasSuffix("]")
        }

        func isFeaturesHeader(_ line: String) -> Bool {
            normalized(line) == featuresHeader
        }

        func keyName(_ line: String) -> String? {
            let trimmed = normalized(line)
            guard trimmed.isEmpty == false,
                  trimmed.hasPrefix("#") == false,
                  let equalsIndex = trimmed.firstIndex(of: "=") else {
                return nil
            }
            return trimmed[..<equalsIndex].trimmingCharacters(in: .whitespaces)
        }

        func featuresSectionEnd(in lines: [String], start: Int) -> Int {
            var index = start + 1
            while index < lines.count, isTableHeader(lines[index]) == false {
                index += 1
            }
            return index
        }

        func ensureHooksFeatureFlag(in lines: inout [String]) {
            guard let featuresIndex = lines.firstIndex(where: isFeaturesHeader) else {
                if lines.isEmpty == false, normalized(lines.last ?? "").isEmpty == false {
                    lines.append("")
                }
                lines.append(featuresHeader)
                lines.append(hooksFeatureLine)
                return
            }

            let sectionEnd = featuresSectionEnd(in: lines, start: featuresIndex)
            var hooksIndex: Int?
            var legacyHooksIndex: Int?
            var removalIndices: [Int] = []

            for index in (featuresIndex + 1)..<sectionEnd {
                switch keyName(lines[index]) {
                case "hooks":
                    if hooksIndex == nil {
                        hooksIndex = index
                    } else {
                        removalIndices.append(index)
                    }
                case "codex_hooks":
                    if legacyHooksIndex == nil {
                        legacyHooksIndex = index
                    } else {
                        removalIndices.append(index)
                    }
                default:
                    break
                }
            }

            if let hooksIndex {
                lines[hooksIndex] = hooksFeatureLine
                if let legacyHooksIndex {
                    removalIndices.append(legacyHooksIndex)
                }
            } else if let legacyHooksIndex {
                lines[legacyHooksIndex] = hooksFeatureLine
            } else {
                var insertionIndex = sectionEnd
                while insertionIndex > featuresIndex + 1,
                      normalized(lines[insertionIndex - 1]).isEmpty {
                    insertionIndex -= 1
                }
                lines.insert(hooksFeatureLine, at: insertionIndex)
            }

            for index in removalIndices.sorted(by: >) {
                lines.remove(at: index)
            }
        }

        func hookStateKey(fromHeader line: String) -> String? {
            let trimmed = normalized(line)
            let prefix = "[hooks.state.\""
            let suffix = "\"]"
            guard trimmed.hasPrefix(prefix), trimmed.hasSuffix(suffix) else {
                return nil
            }
            let start = trimmed.index(trimmed.startIndex, offsetBy: prefix.count)
            let end = trimmed.index(trimmed.endIndex, offsetBy: -suffix.count)
            return String(trimmed[start..<end])
        }

        func removeManagedHookTrustStateBlocks(from lines: [String], keys: Set<String>) -> [String] {
            guard keys.isEmpty == false else {
                return lines
            }

            var result: [String] = []
            var index = 0
            while index < lines.count {
                if let key = hookStateKey(fromHeader: lines[index]), keys.contains(key) {
                    index += 1
                    while index < lines.count, isTableHeader(lines[index]) == false {
                        index += 1
                    }
                    continue
                }

                result.append(lines[index])
                index += 1
            }
            return result
        }

        func tomlQuotedString(_ value: String) -> String {
            var output = "\""
            for scalar in value.unicodeScalars {
                switch scalar {
                case "\"":
                    output += "\\\""
                case "\\":
                    output += "\\\\"
                case "\n":
                    output += "\\n"
                case "\r":
                    output += "\\r"
                case "\t":
                    output += "\\t"
                default:
                    output.unicodeScalars.append(scalar)
                }
            }
            output += "\""
            return output
        }

        func appendManagedHookTrustStates(_ states: [CodexHookTrustState], to lines: inout [String]) {
            guard states.isEmpty == false else {
                return
            }

            if lines.contains(where: { normalized($0) == "[hooks.state]" }) == false {
                if lines.isEmpty == false, normalized(lines.last ?? "").isEmpty == false {
                    lines.append("")
                }
                lines.append("[hooks.state]")
            }

            for state in states {
                if lines.isEmpty == false, normalized(lines.last ?? "").isEmpty == false {
                    lines.append("")
                }
                lines.append("[hooks.state.\(tomlQuotedString(state.key))]")
                lines.append("trusted_hash = \(tomlQuotedString(state.trustedHash))")
            }
        }

        var lines = existingText
            .replacingOccurrences(of: "\r\n", with: "\n")
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
            .filter { isSuppressLine($0) == false }

        while lines.last?.isEmpty == true {
            lines.removeLast()
        }

        if lines.isEmpty {
            lines = [targetLine]
        } else {
            let firstTableIndex = lines.firstIndex(where: isTableHeader)
            var insertionIndex = firstTableIndex ?? lines.count

            while insertionIndex > 0,
                  normalized(lines[insertionIndex - 1]).isEmpty {
                insertionIndex -= 1
            }

            lines.insert(targetLine, at: insertionIndex)

            if let firstTableIndex,
               insertionIndex < firstTableIndex,
               insertionIndex + 1 < lines.count,
               normalized(lines[insertionIndex + 1]).isEmpty == false {
                lines.insert("", at: insertionIndex + 1)
            }
        }

        ensureHooksFeatureFlag(in: &lines)

        let sortedTrustStates = codexHookTrustStates.sorted { $0.key < $1.key }
        let trustKeys = Set(sortedTrustStates.map(\.key))
        lines = removeManagedHookTrustStateBlocks(from: lines, keys: trustKeys)
        appendManagedHookTrustStates(sortedTrustStates, to: &lines)

        return lines.joined(separator: "\n") + "\n"
    }

    func managedCodexHookTrustStates() -> [CodexHookTrustState] {
        guard let data = try? Data(contentsOf: codexHooksFileURL()),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return []
        }
        return managedCodexHookTrustStates(from: root)
    }

    func managedCodexHookTrustStates(from rootObject: [String: Any]) -> [CodexHookTrustState] {
        guard let hooksObject = rootObject["hooks"] as? [String: Any] else {
            return []
        }

        let expectedActionsByEvent = codexManagedHookActionsByEvent()
        var states: [CodexHookTrustState] = []

        for (eventKey, eventLabel) in codexHookEventLabelsByKey() {
            guard let expectedAction = expectedActionsByEvent[eventKey],
                  let groups = hooksObject[eventKey] as? [[String: Any]] else {
                continue
            }

            for (groupIndex, group) in groups.enumerated() {
                let hooks = group["hooks"] as? [[String: Any]] ?? []
                for (handlerIndex, hook) in hooks.enumerated() {
                    guard (hook["type"] as? String) == "command",
                          let command = hook["command"] as? String,
                          isCodexManagedHookCommand(command, action: expectedAction) else {
                        continue
                    }

                    let statusMessage = hook["statusMessage"] as? String
                    let timeout = hook["timeout"] as? Int ?? 600
                    let matcher = codexHookMatcherForTrustHash(eventKey: eventKey, group: group)
                    let hash = codexCommandHookTrustHash(
                        eventLabel: eventLabel,
                        matcher: matcher,
                        command: command,
                        timeout: max(timeout, 1),
                        statusMessage: statusMessage
                    )
                    states.append(
                        CodexHookTrustState(
                            key: "\(codexHooksFileURL().path):\(eventLabel):\(groupIndex):\(handlerIndex)",
                            trustedHash: hash
                        )
                    )

                    debugLog.log(
                        "codex-hook-config",
                        "prepared trusted hook action=\(expectedAction) key=\(codexHooksFileURL().path):\(eventLabel):\(groupIndex):\(handlerIndex)"
                    )
                }
            }
        }

        return states
    }

    func codexManagedHookActionsByEvent() -> [String: String] {
        [
            "SessionStart": "codex-session-start",
            "UserPromptSubmit": "codex-prompt-submit",
            "PermissionRequest": "codex-permission-request",
            "Stop": "codex-stop",
        ]
    }

    func codexManagedHookCommandsByEvent() -> [String: String] {
        let helperScriptURL = managedRuntimeHookHelperURL()
        let owner = AppRuntimePaths.runtimeOwnerID()
        return codexManagedHookActionsByEvent().mapValues { action in
            hookCommand(
                helperScriptURL: helperScriptURL,
                action: action,
                owner: owner,
                tool: "codex"
            )
        }
    }

    func isCodexManagedHookCommand(_ command: String, action: String) -> Bool {
        command.contains("dmux-ai-state.sh")
            && commandContainsShellArgument(command, argument: action)
            && commandContainsShellArgument(command, argument: "codex")
    }

    func codexHookEventLabelsByKey() -> [String: String] {
        [
            "PermissionRequest": "permission_request",
            "SessionStart": "session_start",
            "Stop": "stop",
            "UserPromptSubmit": "user_prompt_submit",
        ]
    }

    func codexHookMatcherForTrustHash(eventKey: String, group: [String: Any]) -> String? {
        switch eventKey {
        case "UserPromptSubmit", "Stop":
            return nil
        default:
            return group["matcher"] as? String
        }
    }

    func codexCommandHookTrustHash(
        eventLabel: String,
        matcher: String?,
        command: String,
        timeout: Int,
        statusMessage: String?
    ) -> String {
        let statusJSON = statusMessage.map(jsonStringLiteral) ?? "null"
        let hookJSON = """
        "hooks":[{"async":false,"command":\(jsonStringLiteral(command)),"statusMessage":\(statusJSON),"timeout":\(timeout),"type":"command"}]
        """
        let canonicalJSON: String
        if let matcher {
            canonicalJSON = """
            {"event_name":\(jsonStringLiteral(eventLabel)),\(hookJSON),"matcher":\(jsonStringLiteral(matcher))}
            """
        } else {
            canonicalJSON = """
            {"event_name":\(jsonStringLiteral(eventLabel)),\(hookJSON)}
            """
        }
        let digest = SHA256.hash(data: Data(canonicalJSON.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return "sha256:\(digest)"
    }

    func jsonStringLiteral(_ value: String) -> String {
        var output = "\""
        for scalar in value.unicodeScalars {
            switch scalar.value {
            case 0x08:
                output += "\\b"
            case 0x09:
                output += "\\t"
            case 0x0A:
                output += "\\n"
            case 0x0C:
                output += "\\f"
            case 0x0D:
                output += "\\r"
            case 0x22:
                output += "\\\""
            case 0x5C:
                output += "\\\\"
            case 0x00...0x1F:
                output += String(format: "\\u%04x", scalar.value)
            default:
                output.unicodeScalars.append(scalar)
            }
        }
        output += "\""
        return output
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
                ("PermissionRequest", "permission-request", 5, true),
                ("PermissionDenied", "permission-denied", 5, true),
                ("Elicitation", "elicitation", 10, false),
                ("ElicitationResult", "elicitation-result", 10, false),
            ],
            removedDefinitions: [
                ("PreToolUse", "pre-tool-use"),
                ("PostToolUse", "post-tool-use"),
                ("PostToolUseFailure", "post-tool-use-failure"),
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
                ("PermissionRequest", "codex-permission-request", 1000, false),
                ("Stop", "codex-stop", 1000, false),
            ],
            removedDefinitions: [
                ("PreToolUse", "codex-pre-tool-use"),
                ("PostToolUse", "codex-post-tool-use"),
                ("SessionEnd", "codex-session-end"),
            ],
            stripAnyManagedHookForAction: true
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
            ],
            stripAnyManagedHookForAction: true
        )
    }

    func installManagedHooks(
        _ rootObject: inout [String: Any],
        fileURL: URL,
        tool: String,
        category: String,
        description: String,
        definitions: [(eventKey: String, action: String, timeout: Int, async: Bool)],
        removedDefinitions: [(eventKey: String, action: String)] = [],
        notificationActionToStrip: String? = nil,
        stripAnyManagedHookForAction: Bool = false
    ) {
        let helperScriptURL = managedRuntimeHookHelperURL()
        let statusMessage = "dmux \(tool) live"
        var hooksObject = rootObject["hooks"] as? [String: Any] ?? [:]
        stripManagedHookDefinitions(
            removedDefinitions,
            from: &hooksObject,
            owner: AppRuntimePaths.runtimeOwnerID(),
            helperScriptURL: helperScriptURL,
            statusMessage: statusMessage,
            tool: tool,
            stripAnyManagedHookForAction: stripAnyManagedHookForAction,
            preserveOtherManagedOwners: false
        )
        let specs = managedHookSpecs(
            tool: tool,
            statusMessage: statusMessage,
            helperScriptURL: helperScriptURL,
            definitions: definitions
        )
        applyManagedHookSpecs(
            specs,
            to: &hooksObject,
            helperScriptURL: helperScriptURL,
            stripAnyManagedHookForAction: stripAnyManagedHookForAction
        )

        if let notificationActionToStrip {
            let notificationHookGroups = strippedManagedHookGroups(
                existingValue: hooksObject["Notification"],
                action: notificationActionToStrip,
                owner: AppRuntimePaths.runtimeOwnerID(),
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

    func stripManagedHookDefinitions(
        _ definitions: [(eventKey: String, action: String)],
        from hooksObject: inout [String: Any],
        owner: String,
        helperScriptURL: URL,
        statusMessage: String,
        tool: String? = nil,
        stripAnyManagedHookForAction: Bool = false,
        preserveOtherManagedOwners: Bool = true
    ) {
        for definition in definitions {
            let strippedGroups = strippedManagedHookGroups(
                existingValue: hooksObject[definition.eventKey],
                action: definition.action,
                tool: tool,
                owner: owner,
                helperScriptURL: helperScriptURL,
                statusMessage: statusMessage,
                stripAnyManagedHookForAction: stripAnyManagedHookForAction,
                preserveOtherManagedOwners: preserveOtherManagedOwners
            )

            if strippedGroups.isEmpty {
                hooksObject.removeValue(forKey: definition.eventKey)
            } else {
                hooksObject[definition.eventKey] = strippedGroups
            }
        }
    }

    func applyManagedHookSpecs(
        _ specs: [ManagedHookSpec],
        to hooksObject: inout [String: Any],
        helperScriptURL: URL,
        stripAnyManagedHookForAction: Bool = false
    ) {
        for spec in specs {
            hooksObject[spec.eventKey] = upsertManagedHookGroups(
                existingValue: hooksObject[spec.eventKey],
                spec: spec,
                helperScriptURL: helperScriptURL,
                stripAnyManagedHookForAction: stripAnyManagedHookForAction
            )
        }
    }

    func managedHookSpecs(
        tool: String,
        statusMessage: String,
        helperScriptURL: URL,
        definitions: [(eventKey: String, action: String, timeout: Int, async: Bool)]
    ) -> [ManagedHookSpec] {
        let owner = AppRuntimePaths.runtimeOwnerID()
        return definitions.map { definition in
            ManagedHookSpec(
                eventKey: definition.eventKey,
                action: definition.action,
                tool: tool,
                command: hookCommand(
                    helperScriptURL: helperScriptURL,
                    action: definition.action,
                    owner: owner,
                    tool: tool
                ),
                statusMessage: statusMessage,
                timeout: definition.timeout,
                async: definition.async
            )
        }
    }

    func upsertManagedHookGroups(
        existingValue: Any?,
        spec: ManagedHookSpec,
        helperScriptURL: URL,
        stripAnyManagedHookForAction: Bool = false,
        preserveOtherManagedOwners: Bool = true
    ) -> [[String: Any]] {
        let owner = AppRuntimePaths.runtimeOwnerID()
        let replacementHook = managedHookDictionary(for: spec)
        let existingGroups = existingValue as? [[String: Any]] ?? []
        var didReplace = false
        var result: [[String: Any]] = []

        for group in existingGroups {
            let hooks = group["hooks"] as? [[String: Any]] ?? []
            var nextHooks: [[String: Any]] = []

            for hook in hooks {
                if isManagedHook(
                    hook,
                    action: spec.action,
                    tool: spec.tool,
                    owner: owner,
                    helperScriptURL: helperScriptURL,
                    statusMessage: spec.statusMessage,
                    stripAnyManagedHookForAction: stripAnyManagedHookForAction,
                    preserveOtherManagedOwners: preserveOtherManagedOwners
                ) {
                    if didReplace == false {
                        nextHooks.append(replacementHook)
                        didReplace = true
                    }
                    continue
                }

                nextHooks.append(hook)
            }

            guard nextHooks.isEmpty == false else {
                continue
            }

            var nextGroup = group
            nextGroup["hooks"] = nextHooks
            result.append(nextGroup)
        }

        if didReplace == false {
            result.append([
                "matcher": "",
                "hooks": [replacementHook],
            ])
        }

        return result
    }

    func managedHookDictionary(for spec: ManagedHookSpec) -> [String: Any] {
        var hook: [String: Any] = [
            "type": "command",
            "command": spec.command,
            "timeout": spec.timeout,
            "statusMessage": spec.statusMessage,
        ]
        if spec.async {
            hook["async"] = true
        }
        return hook
    }

    func strippedManagedHookGroups(
        existingValue: Any?,
        action: String,
        tool: String? = nil,
        owner: String,
        helperScriptURL: URL,
        statusMessage: String,
        stripAnyManagedHookForAction: Bool = false,
        preserveOtherManagedOwners: Bool = true
    ) -> [[String: Any]] {
        let existingGroups = existingValue as? [[String: Any]] ?? []
        return existingGroups.compactMap { group in
            let hooks = group["hooks"] as? [[String: Any]] ?? []
            let filteredHooks = hooks.filter { hook in
                !isManagedHook(
                    hook,
                    action: action,
                    tool: tool,
                    owner: owner,
                    helperScriptURL: helperScriptURL,
                    statusMessage: statusMessage,
                    stripAnyManagedHookForAction: stripAnyManagedHookForAction,
                    preserveOtherManagedOwners: preserveOtherManagedOwners
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
        tool: String? = nil,
        owner: String,
        helperScriptURL: URL,
        statusMessage expectedStatusMessage: String,
        stripAnyManagedHookForAction: Bool = false,
        preserveOtherManagedOwners: Bool = true
    ) -> Bool {
        guard let type = hook["type"] as? String,
              type == "command",
              let command = hook["command"] as? String else {
            return false
        }

        guard commandContainsShellArgument(command, argument: action) else {
            return false
        }

        if stripAnyManagedHookForAction,
           command.contains("dmux-ai-state.sh"),
           hook["statusMessage"] as? String == expectedStatusMessage,
           let tool,
           commandContainsShellArgument(command, argument: tool) {
            if commandContainsShellArgument(command, argument: owner) {
                return true
            }
            if preserveOtherManagedOwners,
               commandHasManagedOwnerArgument(command, action: action, tool: tool) {
                return false
            }
            return true
        }

        guard command.contains(helperScriptURL.path) else {
            return false
        }

        if commandContainsShellArgument(command, argument: owner) {
            return true
        }

        return false
    }

    func commandHasManagedOwnerArgument(_ command: String, action: String, tool: String) -> Bool {
        let actionToken = shellQuoted(action)
        let toolToken = shellQuoted(tool)
        guard let actionRange = command.range(of: actionToken) else {
            return false
        }
        let tail = command[actionRange.upperBound...].trimmingCharacters(in: .whitespaces)
        guard tail.isEmpty == false else {
            return false
        }
        if tail == toolToken || tail.hasPrefix("\(toolToken) ") {
            return false
        }
        return tail.contains(toolToken)
    }

    func commandContainsShellArgument(_ command: String, argument: String) -> Bool {
        let token = shellQuoted(argument)
        return command.contains(" \(token) ")
            || command.hasPrefix("\(token) ")
            || command.hasSuffix(" \(token)")
            || command == token
    }

    func hookCommand(helperScriptURL: URL, action: String, owner: String, tool: String) -> String {
        [
            shellQuoted(helperScriptURL.path),
            shellQuoted(action),
            shellQuoted(owner),
            shellQuoted(tool),
        ].joined(separator: " ")
    }

    func toolConfigFileURL(directoryName: String, filename: String) -> URL {
        let rootURL: URL
        if AppRuntimePaths.isRunningUnderXCTest() {
            rootURL = AppRuntimePaths
                .temporaryRootURL(
                    fileManager: fileManager,
                    appDisplayName: "CoduxTests",
                    bundleIdentifier: "com.duxweb.codux.tests"
                )
                .appendingPathComponent("external-tool-configs", isDirectory: true)
        } else {
            rootURL = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        }
        return rootURL
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
