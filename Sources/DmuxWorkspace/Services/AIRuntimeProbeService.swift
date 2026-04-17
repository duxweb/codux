import Foundation

struct AIRuntimeSourceLocator {
    static func claudeProjectLogURLs() -> [URL] {
        let baseURL = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".claude/projects", isDirectory: true)
        let enumerator = FileManager.default.enumerator(at: baseURL, includingPropertiesForKeys: [.isRegularFileKey], options: [.skipsHiddenFiles])
        var urls: [URL] = []
        while let next = enumerator?.nextObject() as? URL {
            guard next.pathExtension == "jsonl" else {
                continue
            }
            urls.append(next)
        }
        return urls.sorted { $0.path < $1.path }
    }

    static func codexDatabaseURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex/state_5.sqlite", isDirectory: false)
    }

    static func opencodeDatabaseURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".local/share/opencode/opencode.db", isDirectory: false)
    }

    static func geminiProjectsURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".gemini/projects.json", isDirectory: false)
    }

    static func geminiTempDirectoryURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".gemini/tmp", isDirectory: true)
    }

    static func geminiProjectTempDirectoryURL(projectPath: String) -> URL? {
        let projectsURL = geminiProjectsURL()
        if let data = try? Data(contentsOf: projectsURL),
           let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let projects = object["projects"] as? [String: Any],
           let directoryName = projects[projectPath] as? String,
           !directoryName.isEmpty {
            return geminiTempDirectoryURL().appendingPathComponent(directoryName, isDirectory: true)
        }

        let tempURL = geminiTempDirectoryURL()
        guard let entries = try? FileManager.default.contentsOfDirectory(
            at: tempURL,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ) else {
            return nil
        }

        for entry in entries {
            let rootMarker = entry.appendingPathComponent(".project_root", isDirectory: false)
            guard let value = try? String(contentsOf: rootMarker, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines),
                  value == projectPath else {
                continue
            }
            return entry
        }
        return nil
    }

    static func geminiChatsDirectoryURL(projectPath: String) -> URL? {
        geminiProjectTempDirectoryURL(projectPath: projectPath)?
            .appendingPathComponent("chats", isDirectory: true)
    }

    static func geminiSessionFileURLs(projectPath: String) -> [URL] {
        guard let chatsDirectoryURL = geminiChatsDirectoryURL(projectPath: projectPath),
              let fileURLs = try? FileManager.default.contentsOfDirectory(
                  at: chatsDirectoryURL,
                  includingPropertiesForKeys: [.contentModificationDateKey],
                  options: [.skipsHiddenFiles]
              ) else {
            return []
        }

        return fileURLs
            .filter {
                $0.pathExtension == "json" && $0.lastPathComponent.hasPrefix("session-")
            }
            .sorted {
                let lhs = ((try? $0.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
                let rhs = ((try? $1.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast)
                return lhs > rhs
            }
    }
}

struct AIRuntimeContextSnapshot {
    var tool: String
    var externalSessionID: String?
    var model: String?
    var inputTokens: Int
    var outputTokens: Int
    var totalTokens: Int
    var updatedAt: Double
    var responseState: AIResponseState?
    var wasInterrupted: Bool = false
    var hasCompletedTurn: Bool = false
    var sessionOrigin: AIRuntimeSessionOrigin = .unknown
}

enum AIRuntimeSessionOrigin: String {
    case unknown
    case fresh
    case restored
}

func parseCodexISO8601Date(_ value: String) -> Date? {
    let formatterWithFractional = ISO8601DateFormatter()
    formatterWithFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = formatterWithFractional.date(from: value) {
        return date
    }

    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: value)
}

actor AIRuntimeContextProbe {
    private let codexRuntimeProbe = CodexRuntimeProbeService()
    private let claudeRuntimeProbe = ClaudeRuntimeProbeService()
    private let geminiRuntimeProbe = GeminiRuntimeProbeService()
    private let opencodeRuntimeProbe = OpenCodeRuntimeProbeService()

    func snapshot(for tool: String, runtimeSessionID: String, projectPath: String, startedAt: Double) async -> AIRuntimeContextSnapshot? {
        switch normalize(tool: tool) {
        case "codex":
            return await codexRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: nil
            )
        case "claude":
            return await claudeRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                knownExternalSessionID: nil
            )
        case "opencode":
            return await opencodeRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: nil
            )
        case "gemini":
            return await geminiRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: nil
            )
        default:
            return AIRuntimeContextSnapshot(
                tool: tool,
                externalSessionID: nil,
                model: nil,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                updatedAt: Date().timeIntervalSince1970,
                responseState: nil
            )
        }
    }

    private func normalize(tool: String) -> String {
        switch tool {
        case "claude-code":
            return "claude"
        default:
            return tool
        }
    }

    func snapshot(
        for tool: String,
        runtimeSessionID: String,
        projectPath: String,
        startedAt: Double,
        knownExternalSessionID: String?
    ) async -> AIRuntimeContextSnapshot? {
        switch normalize(tool: tool) {
        case "codex":
            return await codexRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: knownExternalSessionID
            )
        case "claude":
            return await claudeRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                knownExternalSessionID: knownExternalSessionID
            )
        case "opencode":
            return await opencodeRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: knownExternalSessionID
            )
        case "gemini":
            return await geminiRuntimeProbe.snapshot(
                runtimeSessionID: runtimeSessionID,
                projectPath: projectPath,
                startedAt: startedAt,
                knownExternalSessionID: knownExternalSessionID
            )
        default:
            return AIRuntimeContextSnapshot(
                tool: tool,
                externalSessionID: nil,
                model: nil,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                updatedAt: Date().timeIntervalSince1970,
                responseState: nil,
                hasCompletedTurn: false
            )
        }
    }

    func reset(for tool: String?, runtimeSessionID: String) async {
        guard let tool else {
            return
        }
        switch normalize(tool: tool) {
        case "codex":
            await codexRuntimeProbe.reset(runtimeSessionID: runtimeSessionID)
        case "claude":
            await claudeRuntimeProbe.reset(runtimeSessionID: runtimeSessionID)
        case "opencode":
            await opencodeRuntimeProbe.reset(runtimeSessionID: runtimeSessionID)
        case "gemini":
            await geminiRuntimeProbe.reset(runtimeSessionID: runtimeSessionID)
        default:
            break
        }
    }

}

struct TerminalProcessInspector: Sendable {
    func activeTool(forShellPID shellPID: Int32) -> String? {
        let snapshot = processSnapshot()
        guard !snapshot.isEmpty else {
            return nil
        }

        let rowsByPID = Dictionary(uniqueKeysWithValues: snapshot.map { ($0.pid, $0) })
        var childrenByParent: [Int32: [ProcessInfoRow]] = [:]
        for row in snapshot {
            childrenByParent[row.ppid, default: []].append(row)
        }

        let candidateRoots = candidateShellRoots(startingAt: shellPID, rowsByPID: rowsByPID)

        for rootPID in candidateRoots {
            if let tool = deepestToolMatch(rootPID: rootPID, childrenByParent: childrenByParent) {
                return tool
            }
        }

        return nil
    }

    func hasActiveCommand(forShellPID shellPID: Int32) -> Bool {
        let snapshot = processSnapshot()
        guard !snapshot.isEmpty else {
            return false
        }

        let rowsByPID = Dictionary(uniqueKeysWithValues: snapshot.map { ($0.pid, $0) })
        var childrenByParent: [Int32: [ProcessInfoRow]] = [:]
        for row in snapshot {
            childrenByParent[row.ppid, default: []].append(row)
        }

        let candidateRoots = candidateShellRoots(startingAt: shellPID, rowsByPID: rowsByPID)
        for rootPID in candidateRoots {
            if containsNonShellDescendant(rootPID: rootPID, childrenByParent: childrenByParent) {
                return true
            }
        }
        return false
    }

    private func deepestToolMatch(rootPID: Int32, childrenByParent: [Int32: [ProcessInfoRow]]) -> String? {
        var stack = childrenByParent[rootPID] ?? []
        var matches: [(depth: Int, tool: String)] = []
        var depthByPID: [Int32: Int] = [rootPID: 0]

        while let row = stack.popLast() {
            let parentDepth = depthByPID[row.ppid] ?? 0
            let depth = parentDepth + 1
            depthByPID[row.pid] = depth

            if let tool = detectTool(in: row.command) {
                matches.append((depth, tool))
            }

            stack.append(contentsOf: childrenByParent[row.pid] ?? [])
        }

        return matches.sorted { lhs, rhs in
            if lhs.depth == rhs.depth {
                return lhs.tool < rhs.tool
            }
            return lhs.depth > rhs.depth
        }.first?.tool
    }

    private func containsNonShellDescendant(rootPID: Int32, childrenByParent: [Int32: [ProcessInfoRow]]) -> Bool {
        var stack = childrenByParent[rootPID] ?? []

        while let row = stack.popLast() {
            if isShellCommand(row.command) || isLoginCommand(row.command) {
                stack.append(contentsOf: childrenByParent[row.pid] ?? [])
                continue
            }
            return true
        }

        return false
    }

    private func candidateShellRoots(startingAt shellPID: Int32, rowsByPID: [Int32: ProcessInfoRow]) -> [Int32] {
        var roots: [Int32] = []
        var currentPID: Int32? = shellPID
        var visited = Set<Int32>()

        while let pid = currentPID, pid > 0, visited.insert(pid).inserted {
            guard let row = rowsByPID[pid] else {
                break
            }
            if isShellCommand(row.command) {
                roots.append(pid)
            }

            guard let parent = rowsByPID[row.ppid] else {
                break
            }
            if isShellCommand(parent.command) || isLoginCommand(parent.command) {
                currentPID = parent.pid
            } else {
                break
            }
        }

        if roots.isEmpty {
            roots.append(shellPID)
        }
        return roots
    }

    private func detectTool(in command: String) -> String? {
        let normalized = command.lowercased()
        if normalized.contains("tool-wrapper.sh")
            || normalized.contains("/scripts/wrappers/bin/")
            || normalized.contains("/scripts/wrappers/tool-wrapper.sh")
        {
            return nil
        }
        let tools = ["claude-code", "claude", "codex", "opencode", "gemini"]
        for tool in tools {
            if normalized.contains("/\(tool)")
                || normalized.contains(" \(tool)")
                || normalized.contains(" \"\(tool)\"")
                || normalized.contains(" '\(tool)'")
                || normalized.hasPrefix("\(tool) ")
                || normalized == tool
            {
                return tool
            }
        }
        return nil
    }

    private func isShellCommand(_ command: String) -> Bool {
        let normalized = command.lowercased()
        let shells = ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish", " -/bin/zsh", " -/bin/bash", " -/bin/sh"]
        return shells.contains(where: normalized.contains) || normalized.hasPrefix("-/bin/")
    }

    private func isLoginCommand(_ command: String) -> Bool {
        command.lowercased().contains("/usr/bin/login")
    }

    private func processSnapshot() -> [ProcessInfoRow] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = ["-wwaxo", "pid=,ppid=,command="]

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
        } catch {
            return []
        }

        let outputData = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0,
              let output = String(data: outputData, encoding: .utf8) else {
            return []
        }

        return output
            .split(separator: "\n")
            .compactMap { line in
                let trimmed = line.trimmingCharacters(in: .whitespaces)
                guard !trimmed.isEmpty else { return nil }
                let parts = trimmed.split(maxSplits: 2, whereSeparator: \.isWhitespace)
                guard parts.count == 3,
                      let pid = Int32(parts[0]),
                      let ppid = Int32(parts[1]) else {
                    return nil
                }
                return ProcessInfoRow(pid: pid, ppid: ppid, command: String(parts[2]))
            }
    }

    private struct ProcessInfoRow {
        var pid: Int32
        var ppid: Int32
        var command: String
    }
}
