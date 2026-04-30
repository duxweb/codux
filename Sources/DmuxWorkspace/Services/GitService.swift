import Foundation

struct GitCredential: Hashable {
    var username: String
    var password: String
}

struct GitRepositoryState: Hashable {
    var branch: String
    var staged: [GitFileEntry]
    var changes: [GitFileEntry]
    var untracked: [GitFileEntry]

    var hasStagedChanges: Bool {
        !staged.isEmpty
    }

    var totalChanges: Int {
        staged.count + changes.count + untracked.count
    }
}

struct GitCommitEntry: Identifiable, Hashable {
    var id: String { hash }
    var hash: String
    var graphPrefix: String
    var subject: String
    var author: String
    var relativeDate: String
    var decorations: [String]
}

struct GitRemoteSyncState: Hashable {
    var incomingCount: Int
    var outgoingCount: Int
    var hasUpstream: Bool

    static let empty = GitRemoteSyncState(incomingCount: 0, outgoingCount: 0, hasUpstream: false)
}

enum GitFileKind: String, Hashable {
    case staged
    case changed
    case untracked
}

struct GitFileEntry: Identifiable, Hashable {
    var id: String { "\(kind.rawValue):\(path)" }
    var path: String
    var kind: GitFileKind
}

enum GitFileDiffRowKind: Hashable {
    case context
    case added
    case removed
    case modified
}

struct GitFileDiffLine: Hashable {
    var number: Int?
    var text: String
}

struct GitFileDiffRow: Identifiable, Hashable {
    var id: Int
    var kind: GitFileDiffRowKind
    var newLine: GitFileDiffLine?
    var oldLine: GitFileDiffLine?
}

struct GitFileDiffPreview: Hashable {
    var entry: GitFileEntry
    var rows: [GitFileDiffRow]
    var newTitle: String
    var oldTitle: String
}

struct GitService {
    private enum GitTextSide {
        case new
        case old
    }

    private enum GitRawDiffEdit: Hashable {
        case context(newLine: GitFileDiffLine, oldLine: GitFileDiffLine)
        case added(GitFileDiffLine)
        case removed(GitFileDiffLine)
    }

    private let maxDiffPreviewBytes = 1_500_000
    private let maxPreciseDiffCells = 1_200_000

    func originURL(at path: String) throws -> String {
        try runGit(["config", "--get", "remote.origin.url"], at: path, allowFailure: true, allowEmptyOutput: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func initializeRepository(at path: String) throws {
        _ = try runGit(["init"], at: path)
    }

    func clone(_ remoteURL: String, into path: String, credential: GitCredential? = nil, progress: (@Sendable (String, Double?) -> Void)? = nil) throws {
        let parentURL = URL(fileURLWithPath: path).deletingLastPathComponent()
        let folderName = URL(fileURLWithPath: path).lastPathComponent
        let process = Process()
        process.currentDirectoryURL = parentURL
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["git", "clone", "--progress", remoteURL, folderName]

        var environment = ProcessInfo.processInfo.environment
        environment["GIT_TERMINAL_PROMPT"] = "0"

        var askPassURL: URL?
        if let credential {
            let askPassScript = "#!/bin/sh\nprompt=\"$1\"\ncase \"$prompt\" in\n  *Username*|*username*) printf '%s\\n' \"$GHOSTTYWORKSPACE_GIT_USERNAME\" ;;&\n  *Password*|*password*) printf '%s\\n' \"$GHOSTTYWORKSPACE_GIT_PASSWORD\" ;;&\n  *) printf '\\n' ;;&\nesac\n"
            let temporaryURL = FileManager.default.temporaryDirectory.appendingPathComponent("dmux-git-askpass-\(UUID().uuidString)")
            try askPassScript.write(to: temporaryURL, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: temporaryURL.path)
            environment["GIT_ASKPASS"] = temporaryURL.path
            environment["SSH_ASKPASS"] = temporaryURL.path
            environment["GHOSTTYWORKSPACE_GIT_USERNAME"] = credential.username
            environment["GHOSTTYWORKSPACE_GIT_PASSWORD"] = credential.password
            environment["DISPLAY"] = environment["DISPLAY"] ?? "1"
            askPassURL = temporaryURL
        }

        process.environment = environment
        defer {
            if let askPassURL {
                try? FileManager.default.removeItem(at: askPassURL)
            }
        }

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()
        let errorHandle = stderr.fileHandleForReading
        var stderrData = Data()
        while process.isRunning {
            let chunk = errorHandle.availableData
            if !chunk.isEmpty {
                stderrData.append(chunk)
                while let newlineRange = stderrData.range(of: Data([0x0A])) {
                    let lineData = stderrData.subdata(in: 0..<newlineRange.lowerBound)
                    stderrData.removeSubrange(0...newlineRange.lowerBound)
                    guard let line = String(data: lineData, encoding: .utf8) else { continue }
                    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                    if trimmed.isEmpty { continue }
                    progress?(trimmed, Self.cloneProgressValue(from: trimmed))
                }
            }
            Thread.sleep(forTimeInterval: 0.03)
        }
        process.waitUntilExit()
        let tail = errorHandle.readDataToEndOfFile()
        if !tail.isEmpty {
            stderrData.append(tail)
        }

        let output = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let errorOutput = String(data: stderrData, encoding: .utf8) ?? ""
        let combined = [output, errorOutput]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")

        guard process.terminationStatus == 0 else {
            let failureMessage = combined.isEmpty ? "Git clone failed." : combined
            if Self.isAuthenticationFailure(failureMessage) {
                throw GitServiceError.authenticationRequired(failureMessage)
            }
            throw GitServiceError.commandFailed(failureMessage)
        }
    }

    private static func cloneProgressValue(from line: String) -> Double? {
        let patterns = [
            "Receiving objects:",
            "Resolving deltas:",
            "Compressing objects:",
            "Finding sources:"
        ]
        guard patterns.contains(where: { line.contains($0) }) else {
            return nil
        }

        let scanner = Scanner(string: line)
        while !scanner.isAtEnd {
            _ = scanner.scanUpToCharacters(from: .decimalDigits)
            if let value = scanner.scanDouble(), scanner.scanString("%") != nil {
                return min(max(value / 100.0, 0), 1)
            }
        }
        return nil
    }

    func repositoryState(at path: String) throws -> GitRepositoryState? {
        guard try isGitRepository(at: path) else {
            return nil
        }

        let branch = try currentBranch(at: path)
        let statusOutput = try runGit(["status", "--porcelain=v1"], at: path)

        var staged: [GitFileEntry] = []
        var changes: [GitFileEntry] = []
        var untracked: [GitFileEntry] = []

        for line in statusOutput.split(whereSeparator: \ .isNewline) {
            let entry = parseStatusLine(String(line))
            guard let entry else {
                continue
            }

            switch entry {
            case .staged(let path):
                staged.append(GitFileEntry(path: path, kind: .staged))
            case .changed(let path):
                changes.append(GitFileEntry(path: path, kind: .changed))
            case .untracked(let path):
                untracked.append(GitFileEntry(path: path, kind: .untracked))
            case .stagedAndChanged(let path):
                staged.append(GitFileEntry(path: path, kind: .staged))
                changes.append(GitFileEntry(path: path, kind: .changed))
            }
        }

        return GitRepositoryState(branch: branch, staged: staged, changes: changes, untracked: untracked)
    }

    func diff(for entry: GitFileEntry, at path: String) throws -> String {
        switch entry.kind {
        case .staged:
            return try runGit(["diff", "--cached", "--", entry.path], at: path, allowEmptyOutput: true)
        case .changed:
            return try runGit(["diff", "--", entry.path], at: path, allowEmptyOutput: true)
        case .untracked:
            return "Untracked file: \(entry.path)\n\nStage the file to include it in the next commit."
        }
    }

    func sideBySideDiff(for entry: GitFileEntry, at path: String) throws -> GitFileDiffPreview {
        let oldText = try fileText(for: entry, at: path, side: .old)
        let newText = try fileText(for: entry, at: path, side: .new)
        return GitFileDiffPreview(
            entry: entry,
            rows: diffRows(newText: newText, oldText: oldText),
            newTitle: String(localized: "git.diff.new_file", defaultValue: "New File", bundle: .module),
            oldTitle: String(localized: "git.diff.old_file", defaultValue: "Old File", bundle: .module)
        )
    }

    func stage(_ filePath: String, at path: String) throws {
        _ = try runGit(["add", "--", filePath], at: path)
    }

    func stage(_ filePaths: [String], at path: String) throws {
        guard !filePaths.isEmpty else { return }
        _ = try runGit(["add", "--"] + filePaths, at: path)
    }

    func unstage(_ filePath: String, at path: String) throws {
        try unstage([filePath], at: path)
    }

    func unstage(_ filePaths: [String], at path: String) throws {
        guard !filePaths.isEmpty else { return }
        if try hasResolvableHEAD(at: path) {
            _ = try runGit(["reset", "HEAD", "--"] + filePaths, at: path)
        } else {
            _ = try runGit(["rm", "--cached", "-r", "--"] + filePaths, at: path, allowEmptyOutput: true)
        }
    }

    func commit(message: String, at path: String) throws {
        _ = try runGit(["commit", "-m", message], at: path)
    }

    func amendLastCommitMessage(_ message: String, at path: String) throws {
        _ = try runGit(["commit", "--amend", "-m", message], at: path)
    }

    func undoLastCommit(at path: String) throws {
        _ = try runGit(["reset", "--soft", "HEAD~1"], at: path)
    }

    func lastCommitMessage(at path: String) throws -> String {
        try runGit(["log", "-1", "--pretty=%s"], at: path)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func isHeadCommitPushed(at path: String) throws -> Bool {
        let upstream = try runGit(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"], at: path, allowFailure: true, allowEmptyOutput: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)

        guard !upstream.isEmpty, !upstream.contains("fatal:") else {
            return false
        }

        let output = try runGit(["branch", "-r", "--contains", "HEAD"], at: path, allowEmptyOutput: true)
            .split(whereSeparator: \ .isNewline)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }

        return output.contains(upstream)
    }

    func push(at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["push"], at: path, credential: credential)
    }

    func push(branch: String, to remote: String, at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["push", "-u", remote, branch], at: path, credential: credential)
    }

    func push(localBranch: String, to remote: String, remoteBranch: String, at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["push", remote, "\(localBranch):\(remoteBranch)"], at: path, credential: credential)
    }

    func fetch(at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["fetch"], at: path, allowEmptyOutput: true, credential: credential)
    }

    func pull(at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["pull", "--rebase"], at: path, credential: credential)
    }

    func sync(at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["pull", "--rebase"], at: path, credential: credential)
        _ = try runGit(["push"], at: path, credential: credential)
    }

    func createBranch(_ branch: String, at path: String) throws {
        _ = try runGit(["checkout", "-b", branch], at: path)
    }

    func createBranch(_ branch: String, from commit: String, at path: String) throws {
        _ = try runGit(["checkout", "-b", branch, commit], at: path)
    }

    func checkout(commit: String, at path: String) throws {
        _ = try runGit(["checkout", commit], at: path)
    }

    func revert(commit: String, at path: String) throws {
        _ = try runGit(["revert", "--no-edit", commit], at: path)
    }

    func resetCurrentBranch(to commit: String, at path: String) throws {
        _ = try runGit(["reset", "--hard", commit], at: path)
    }

    func forcePush(at path: String, credential: GitCredential? = nil) throws {
        _ = try runGit(["push", "--force-with-lease"], at: path, credential: credential)
    }

    func discard(_ entry: GitFileEntry, at path: String) throws {
        switch entry.kind {
        case .changed:
            _ = try runGit(["restore", "--", entry.path], at: path)
        case .untracked:
            _ = try runGit(["clean", "-f", "--", entry.path], at: path)
        case .staged:
            _ = try runGit(["restore", "--staged", "--worktree", "--", entry.path], at: path)
        }
    }

    func discard(_ entries: [GitFileEntry], at path: String) throws {
        for entry in entries {
            try discard(entry, at: path)
        }
    }

    func appendToGitignore(_ paths: [String], at repositoryPath: String) throws {
        guard !paths.isEmpty else { return }
        let gitignoreURL = URL(fileURLWithPath: repositoryPath).appendingPathComponent(".gitignore")

        let existing = (try? String(contentsOf: gitignoreURL, encoding: .utf8)) ?? ""
        let existingLines = Set(existing.split(whereSeparator: \ .isNewline).map { $0.trimmingCharacters(in: .whitespaces) })

        let additions = paths
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && !existingLines.contains($0) }

        guard !additions.isEmpty else { return }

        let prefix = existing.isEmpty || existing.hasSuffix("\n") ? existing : existing + "\n"
        let content = prefix + additions.joined(separator: "\n") + "\n"
        try content.write(to: gitignoreURL, atomically: true, encoding: .utf8)
    }

    func history(at path: String, limit: Int = 20) throws -> [GitCommitEntry] {
        let format = "%x09%H%x1f%s%x1f%an%x1f%ar%x1f%d"
        let output = try runGit(["log", "--graph", "--decorate=short", "--date=relative", "-n", String(limit), "--pretty=format:\(format)"], at: path, allowEmptyOutput: true)
        return output.split(whereSeparator: \ .isNewline).compactMap { line in
            let raw = String(line)
            let sections = raw.split(separator: "\t", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
            guard sections.count == 2 else { return nil }

            let graphPrefix = sections[0]
            let parts = sections[1].split(separator: "\u{1f}", omittingEmptySubsequences: false).map(String.init)
            guard parts.count == 5 else { return nil }

            let decorations = parseDecorations(parts[4])
            return GitCommitEntry(
                hash: parts[0],
                graphPrefix: graphPrefix,
                subject: parts[1],
                author: parts[2],
                relativeDate: parts[3],
                decorations: decorations
            )
        }
    }

    func localBranches(at path: String) throws -> [String] {
        let output = try runGit(["for-each-ref", "--format=%(refname:short)", "refs/heads"], at: path, allowEmptyOutput: true)
        return output
            .split(whereSeparator: \ .isNewline)
            .map(String.init)
            .filter { !$0.isEmpty }
    }

    func localBranchUpstreams(at path: String) throws -> [String: String] {
        let output = try runGit(["for-each-ref", "--format=%(refname:short)%x1f%(upstream:short)", "refs/heads"], at: path, allowEmptyOutput: true)
        var mapping: [String: String] = [:]

        for line in output.split(whereSeparator: \ .isNewline) {
            let parts = String(line).split(separator: "\u{1f}", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
            guard let branch = parts.first?.trimmingCharacters(in: .whitespacesAndNewlines), !branch.isEmpty else { continue }
            let upstream = parts.count > 1 ? parts[1].trimmingCharacters(in: .whitespacesAndNewlines) : ""
            if !upstream.isEmpty {
                mapping[branch] = upstream
            }
        }

        return mapping
    }

    func remoteBranches(at path: String) throws -> [String] {
        let output = try runGit(["for-each-ref", "--format=%(refname:short)", "refs/remotes"], at: path, allowEmptyOutput: true)
        return output
            .split(whereSeparator: \ .isNewline)
            .map(String.init)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && !$0.contains("HEAD ->") && $0.contains("/") }
    }

    func remotes(at path: String) throws -> [GitRemoteEntry] {
        let output = try runGit(["remote", "-v"], at: path, allowEmptyOutput: true)
        var remotes: [GitRemoteEntry] = []
        var seen = Set<String>()

        for line in output.split(whereSeparator: \.isNewline) {
            let parts = line.split(omittingEmptySubsequences: true, whereSeparator: \.isWhitespace).map(String.init)
            guard parts.count >= 2 else { continue }
            let name = parts[0]
            let url = parts[1]
            guard !seen.contains(name) else { continue }
            seen.insert(name)
            remotes.append(GitRemoteEntry(name: name, url: url))
        }

        return remotes.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    func remoteURL(named remote: String, at path: String) throws -> String {
        try runGit(["remote", "get-url", remote], at: path, allowFailure: true, allowEmptyOutput: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func addRemote(name: String, url: String, at path: String) throws {
        _ = try runGit(["remote", "add", name, url], at: path)
    }

    func removeRemote(name: String, at path: String) throws {
        _ = try runGit(["remote", "remove", name], at: path)
    }

    func checkout(branch: String, at path: String) throws {
        _ = try runGit(["checkout", branch], at: path)
    }

    func checkoutRemoteBranch(_ remoteBranch: String, at path: String) throws -> String {
        let localName = remoteBranch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).dropFirst().first.map(String.init) ?? remoteBranch
        _ = try runGit(["checkout", "-b", localName, "--track", remoteBranch], at: path)
        return localName
    }

    func merge(branch: String, intoCurrentBranchAt path: String) throws {
        _ = try runGit(["merge", branch], at: path)
    }

    func remoteSyncState(at path: String) throws -> GitRemoteSyncState {
        let upstream = try runGit(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"], at: path, allowFailure: true, allowEmptyOutput: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)

        guard !upstream.isEmpty, !upstream.contains("fatal:") else {
            return .empty
        }

        let counts = try runGit(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"], at: path)
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .split(whereSeparator: \ .isWhitespace)

        guard counts.count == 2,
              let incoming = Int(counts[0]),
              let outgoing = Int(counts[1]) else {
            return .empty
        }

        return GitRemoteSyncState(incomingCount: incoming, outgoingCount: outgoing, hasUpstream: true)
    }

    func stageAll(at path: String) throws {
        _ = try runGit(["add", "-A"], at: path)
    }

    func unstageAll(at path: String) throws {
        if try hasResolvableHEAD(at: path) {
            _ = try runGit(["reset", "HEAD", "--", "."], at: path)
        } else {
            _ = try runGit(["rm", "--cached", "-r", "."], at: path, allowEmptyOutput: true)
        }
    }

    private func isGitRepository(at path: String) throws -> Bool {
        let output = try runGit(["rev-parse", "--is-inside-work-tree"], at: path, allowFailure: true)
        return output.trimmingCharacters(in: .whitespacesAndNewlines) == "true"
    }

    private func currentBranch(at path: String) throws -> String {
        let branch = try runGit(["branch", "--show-current"], at: path, allowFailure: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)

        return branch.isEmpty ? "detached HEAD" : branch
    }

    private func fileText(for entry: GitFileEntry, at path: String, side: GitTextSide) throws -> String {
        switch (entry.kind, side) {
        case (.untracked, .old):
            return ""
        case (.untracked, .new):
            return try workingTreeText(relativePath: entry.path, at: path)
        case (.staged, .old):
            return try headContains(entry.path, at: path) ? gitObjectText("HEAD:\(entry.path)", at: path) : ""
        case (.staged, .new):
            return try indexContains(entry.path, at: path) ? gitObjectText(":\(entry.path)", at: path) : ""
        case (.changed, .old):
            if try indexContains(entry.path, at: path) {
                return try gitObjectText(":\(entry.path)", at: path)
            }
            return try headContains(entry.path, at: path) ? gitObjectText("HEAD:\(entry.path)", at: path) : ""
        case (.changed, .new):
            let fileURL = URL(fileURLWithPath: path).appendingPathComponent(entry.path)
            guard FileManager.default.fileExists(atPath: fileURL.path) else {
                return ""
            }
            return try workingTreeText(relativePath: entry.path, at: path)
        }
    }

    private func workingTreeText(relativePath: String, at path: String) throws -> String {
        let fileURL = URL(fileURLWithPath: path).appendingPathComponent(relativePath)
        let values = try fileURL.resourceValues(forKeys: [.fileSizeKey, .isDirectoryKey])
        guard values.isDirectory != true else {
            return ""
        }
        let byteCount = UInt64(values.fileSize ?? 0)
        guard byteCount <= maxDiffPreviewBytes else {
            throw GitServiceError.commandFailed(
                String(localized: "git.diff.too_large", defaultValue: "This file is too large to compare safely.", bundle: .module)
            )
        }
        let data = try Data(contentsOf: fileURL)
        guard data.contains(0) == false,
              let text = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .utf16) else {
            throw GitServiceError.commandFailed(
                String(localized: "git.diff.binary", defaultValue: "Binary files cannot be compared here.", bundle: .module)
            )
        }
        return text
    }

    private func gitObjectText(_ object: String, at path: String) throws -> String {
        try runGit(["show", object], at: path, allowEmptyOutput: true)
    }

    private func indexContains(_ filePath: String, at path: String) throws -> Bool {
        let output = try runGit(["ls-files", "--stage", "--", filePath], at: path, allowEmptyOutput: true)
        return output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
    }

    private func headContains(_ filePath: String, at path: String) throws -> Bool {
        guard try hasResolvableHEAD(at: path) else {
            return false
        }
        let output = try runGit(["ls-tree", "-r", "--name-only", "HEAD", "--", filePath], at: path, allowEmptyOutput: true)
        return output.split(whereSeparator: \.isNewline).contains { $0 == filePath }
    }

    private func diffRows(newText: String, oldText: String) -> [GitFileDiffRow] {
        let newLines = splitLines(newText)
        let oldLines = splitLines(oldText)
        let edits: [GitRawDiffEdit]
        if newLines.count * oldLines.count > maxPreciseDiffCells {
            edits = fallbackEdits(newLines: newLines, oldLines: oldLines)
        } else {
            edits = preciseEdits(newLines: newLines, oldLines: oldLines)
        }
        return coalescedRows(from: edits)
    }

    private func splitLines(_ text: String) -> [String] {
        guard text.isEmpty == false else {
            return []
        }
        var lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        if text.hasSuffix("\n") {
            lines.removeLast()
        }
        return lines
    }

    private func preciseEdits(newLines: [String], oldLines: [String]) -> [GitRawDiffEdit] {
        let newCount = newLines.count
        let oldCount = oldLines.count
        var table = Array(repeating: 0, count: (newCount + 1) * (oldCount + 1))

        func index(_ newIndex: Int, _ oldIndex: Int) -> Int {
            newIndex * (oldCount + 1) + oldIndex
        }

        if newCount > 0 && oldCount > 0 {
            for newIndex in stride(from: newCount - 1, through: 0, by: -1) {
                for oldIndex in stride(from: oldCount - 1, through: 0, by: -1) {
                    if newLines[newIndex] == oldLines[oldIndex] {
                        table[index(newIndex, oldIndex)] = table[index(newIndex + 1, oldIndex + 1)] + 1
                    } else {
                        table[index(newIndex, oldIndex)] = max(table[index(newIndex + 1, oldIndex)], table[index(newIndex, oldIndex + 1)])
                    }
                }
            }
        }

        var edits: [GitRawDiffEdit] = []
        var newIndex = 0
        var oldIndex = 0
        while newIndex < newCount || oldIndex < oldCount {
            if newIndex < newCount, oldIndex < oldCount, newLines[newIndex] == oldLines[oldIndex] {
                edits.append(.context(
                    newLine: GitFileDiffLine(number: newIndex + 1, text: newLines[newIndex]),
                    oldLine: GitFileDiffLine(number: oldIndex + 1, text: oldLines[oldIndex])
                ))
                newIndex += 1
                oldIndex += 1
            } else if newIndex < newCount, (oldIndex == oldCount || table[index(newIndex + 1, oldIndex)] >= table[index(newIndex, oldIndex + 1)]) {
                edits.append(.added(GitFileDiffLine(number: newIndex + 1, text: newLines[newIndex])))
                newIndex += 1
            } else if oldIndex < oldCount {
                edits.append(.removed(GitFileDiffLine(number: oldIndex + 1, text: oldLines[oldIndex])))
                oldIndex += 1
            }
        }
        return edits
    }

    private func fallbackEdits(newLines: [String], oldLines: [String]) -> [GitRawDiffEdit] {
        let maxCount = max(newLines.count, oldLines.count)
        return (0..<maxCount).flatMap { index -> [GitRawDiffEdit] in
            let newLine = index < newLines.count ? GitFileDiffLine(number: index + 1, text: newLines[index]) : nil
            let oldLine = index < oldLines.count ? GitFileDiffLine(number: index + 1, text: oldLines[index]) : nil
            switch (newLine, oldLine) {
            case let (.some(newLine), .some(oldLine)) where newLine.text == oldLine.text:
                return [.context(newLine: newLine, oldLine: oldLine)]
            case let (.some(newLine), .some(oldLine)):
                return [.added(newLine), .removed(oldLine)]
            case let (.some(newLine), .none):
                return [.added(newLine)]
            case let (.none, .some(oldLine)):
                return [.removed(oldLine)]
            case (.none, .none):
                return []
            }
        }
    }

    private func coalescedRows(from edits: [GitRawDiffEdit]) -> [GitFileDiffRow] {
        var rows: [GitFileDiffRow] = []
        var rowID = 0
        var index = 0

        func append(kind: GitFileDiffRowKind, newLine: GitFileDiffLine?, oldLine: GitFileDiffLine?) {
            rows.append(GitFileDiffRow(id: rowID, kind: kind, newLine: newLine, oldLine: oldLine))
            rowID += 1
        }

        while index < edits.count {
            switch edits[index] {
            case let .context(newLine, oldLine):
                append(kind: .context, newLine: newLine, oldLine: oldLine)
                index += 1
            case .added, .removed:
                var added: [GitFileDiffLine] = []
                var removed: [GitFileDiffLine] = []
                while index < edits.count {
                    switch edits[index] {
                    case let .added(line):
                        added.append(line)
                    case let .removed(line):
                        removed.append(line)
                    case .context:
                        break
                    }
                    if case .context = edits[index] {
                        break
                    }
                    index += 1
                }

                let pairedCount = min(added.count, removed.count)
                for pairIndex in 0..<pairedCount {
                    append(kind: .modified, newLine: added[pairIndex], oldLine: removed[pairIndex])
                }
                for line in added.dropFirst(pairedCount) {
                    append(kind: .added, newLine: line, oldLine: nil)
                }
                for line in removed.dropFirst(pairedCount) {
                    append(kind: .removed, newLine: nil, oldLine: line)
                }
            }
        }

        return rows
    }

    private func hasResolvableHEAD(at path: String) throws -> Bool {
        let output = try runGit(["rev-parse", "--verify", "HEAD"], at: path, allowFailure: true, allowEmptyOutput: true)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return !output.isEmpty && !output.contains("fatal:")
    }

    private func runGit(
        _ arguments: [String],
        at path: String,
        allowFailure: Bool = false,
        allowEmptyOutput: Bool = false,
        credential: GitCredential? = nil
    ) throws -> String {
        let process = Process()
        process.currentDirectoryURL = URL(fileURLWithPath: path)
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["git"] + arguments

        var environment = ProcessInfo.processInfo.environment
        environment["GIT_TERMINAL_PROMPT"] = "0"

        var askPassURL: URL?
        if let credential {
            let askPassScript = "#!/bin/sh\nprompt=\"$1\"\ncase \"$prompt\" in\n  *Username*|*username*) printf '%s\\n' \"$GHOSTTYWORKSPACE_GIT_USERNAME\" ;;&\n  *Password*|*password*) printf '%s\\n' \"$GHOSTTYWORKSPACE_GIT_PASSWORD\" ;;&\n  *) printf '\\n' ;;&\nesac\n"
            let temporaryURL = FileManager.default.temporaryDirectory.appendingPathComponent("dmux-git-askpass-\(UUID().uuidString)")
            try askPassScript.write(to: temporaryURL, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: temporaryURL.path)

            environment["GIT_ASKPASS"] = temporaryURL.path
            environment["SSH_ASKPASS"] = temporaryURL.path
            environment["GHOSTTYWORKSPACE_GIT_USERNAME"] = credential.username
            environment["GHOSTTYWORKSPACE_GIT_PASSWORD"] = credential.password
            environment["DISPLAY"] = environment["DISPLAY"] ?? "1"
            askPassURL = temporaryURL
        }

        process.environment = environment
        defer {
            if let askPassURL {
                try? FileManager.default.removeItem(at: askPassURL)
            }
        }

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()
        process.waitUntilExit()

        let stdoutData = stdout.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderr.fileHandleForReading.readDataToEndOfFile()

        let output = String(data: stdoutData, encoding: .utf8) ?? ""
        let errorOutput = String(data: stderrData, encoding: .utf8) ?? ""

        let failureMessage = errorOutput.isEmpty ? output : errorOutput

        if process.terminationStatus != 0 && !allowFailure {
            if Self.isAuthenticationFailure(failureMessage) {
                throw GitServiceError.authenticationRequired(failureMessage)
            }
            throw GitServiceError.commandFailed(failureMessage)
        }

        if !allowEmptyOutput && output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !errorOutput.isEmpty {
            return errorOutput
        }

        return output
    }

    private static func isAuthenticationFailure(_ message: String) -> Bool {
        let normalized = message.lowercased()
        return normalized.contains("authentication failed")
            || normalized.contains("could not read username")
            || normalized.contains("could not read password")
            || normalized.contains("terminal prompts disabled")
            || normalized.contains("invalid username or password")
            || normalized.contains("authentication required")
    }

    private func parseStatusLine(_ line: String) -> ParsedStatusEntry? {
        guard line.count >= 4 else {
            return nil
        }

        let characters = Array(line)
        let indexCode = characters[0]
        let workTreeCode = characters[1]
        let path = String(line.dropFirst(3))

        if indexCode == "?" || workTreeCode == "?" {
            return .untracked(path)
        }

        let hasIndexChange = indexCode != " "
        let hasWorkTreeChange = workTreeCode != " "

        switch (hasIndexChange, hasWorkTreeChange) {
        case (true, true):
            return .stagedAndChanged(path)
        case (true, false):
            return .staged(path)
        case (false, true):
            return .changed(path)
        case (false, false):
            return nil
        }
    }

    private func parseDecorations(_ raw: String) -> [String] {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("("), trimmed.hasSuffix(")") else { return [] }
        let content = String(trimmed.dropFirst().dropLast())
        return content
            .split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }
}

private enum ParsedStatusEntry {
    case staged(String)
    case changed(String)
    case untracked(String)
    case stagedAndChanged(String)
}

enum GitServiceError: LocalizedError {
    case commandFailed(String)
    case authenticationRequired(String)

    var errorDescription: String? {
        switch self {
        case .commandFailed(let message):
            return message.trimmingCharacters(in: .whitespacesAndNewlines)
        case .authenticationRequired(let message):
            return message.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }
}
