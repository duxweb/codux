import Foundation
import SQLite3

private let AIRuntimeSQLiteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

struct AIRuntimeSourceLocator {
    static func claudeProjectLogURLs(homeURL: URL? = nil) -> [URL] {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        let baseURL = homeURL.appendingPathComponent(".claude/projects", isDirectory: true)
        let enumerator = FileManager.default.enumerator(
            at: baseURL,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]
        )
        var urls: [URL] = []
        while let next = enumerator?.nextObject() as? URL {
            guard next.pathExtension == "jsonl" else {
                continue
            }
            urls.append(next)
        }
        return urls.sorted { $0.path < $1.path }
    }

    static func claudeProjectLogURLs(projectPath: String, homeURL: URL? = nil) -> [URL] {
        let directoryURL = claudeProjectDirectoryURL(projectPath: projectPath, homeURL: homeURL)
        let directURLs = (try? FileManager.default.contentsOfDirectory(
            at: directoryURL,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ))?
            .filter { $0.pathExtension == "jsonl" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent } ?? []
        if directURLs.isEmpty == false {
            return directURLs
        }

        return claudeProjectLogURLs(homeURL: homeURL).filter { fileURL in
            claudeLogFile(fileURL, belongsToProjectPath: projectPath)
        }
    }

    static func claudeSessionLogURL(projectPath: String, externalSessionID: String, homeURL: URL? = nil) -> URL {
        claudeProjectDirectoryURL(projectPath: projectPath, homeURL: homeURL)
            .appendingPathComponent("\(externalSessionID).jsonl", isDirectory: false)
    }

    static func codexDatabaseURL(homeURL: URL? = nil) -> URL {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL.appendingPathComponent(".codex/state_5.sqlite", isDirectory: false)
    }

    static func codexSessionsDirectoryURL(homeURL: URL? = nil) -> URL {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL.appendingPathComponent(".codex/sessions", isDirectory: true)
    }

    static func codexSessionFileURLs(projectPath: String, databaseURL: URL? = nil, homeURL: URL? = nil) -> [URL] {
        let databaseURL = databaseURL ?? codexDatabaseURL(homeURL: homeURL)
        let databaseMatches = codexSessionFileURLsFromDatabase(projectPath: projectPath, databaseURL: databaseURL)
        if databaseMatches.isEmpty == false {
            return databaseMatches
        }

        let sessionsDirectoryURL = codexSessionsDirectoryURL(homeURL: homeURL)
        return recursiveJSONLFileURLs(in: sessionsDirectoryURL).filter { fileURL in
            codexRolloutFile(fileURL, belongsToProjectPath: projectPath)
        }
    }

    static func codexSessionFileURLsFromDatabase(projectPath: String, databaseURL: URL) -> [URL] {
        guard FileManager.default.fileExists(atPath: databaseURL.path) else {
            return []
        }

        var db: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &db) == SQLITE_OK,
              let db else {
            if db != nil {
                sqlite3_close(db)
            }
            return []
        }
        defer { sqlite3_close(db) }

        let sql = """
        SELECT rollout_path
        FROM threads
        WHERE cwd = ?
          AND rollout_path IS NOT NULL
        ORDER BY updated_at DESC;
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return []
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, projectPath, -1, AIRuntimeSQLiteTransient)

        var urls: [URL] = []
        var seen = Set<String>()
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let rawPath = sqlite3_column_text(statement, 0) else {
                continue
            }
            let path = String(cString: rawPath)
            guard !path.isEmpty, !seen.contains(path) else {
                continue
            }
            seen.insert(path)
            let fileURL = URL(fileURLWithPath: path).standardizedFileURL
            guard FileManager.default.fileExists(atPath: fileURL.path) else {
                continue
            }
            urls.append(fileURL)
        }

        return urls
    }

    static func codexRolloutPath(projectPath: String, externalSessionID: String, databaseURL: URL? = nil, homeURL: URL? = nil) -> URL? {
        let databaseURL = databaseURL ?? codexDatabaseURL(homeURL: homeURL)
        guard FileManager.default.fileExists(atPath: databaseURL.path) else {
            return nil
        }

        var db: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &db) == SQLITE_OK,
              let db else {
            if db != nil {
                sqlite3_close(db)
            }
            return nil
        }
        defer { sqlite3_close(db) }

        let sql = "SELECT rollout_path FROM threads WHERE cwd = ? AND id = ? LIMIT 1;"
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            return nil
        }
        defer { sqlite3_finalize(statement) }

        sqlite3_bind_text(statement, 1, projectPath, -1, AIRuntimeSQLiteTransient)
        sqlite3_bind_text(statement, 2, externalSessionID, -1, AIRuntimeSQLiteTransient)

        guard sqlite3_step(statement) == SQLITE_ROW,
              let rawPath = sqlite3_column_text(statement, 0) else {
            return nil
        }

        return URL(fileURLWithPath: String(cString: rawPath)).standardizedFileURL
    }

    static func opencodeDatabaseURL(homeURL: URL? = nil) -> URL {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL.appendingPathComponent(".local/share/opencode/opencode.db", isDirectory: false)
    }

    static func opencodeLegacyMessagesDirectoryURL(homeURL: URL? = nil) -> URL {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL.appendingPathComponent(".local/share/opencode/storage/message", isDirectory: true)
    }

    static func opencodeLegacyMessageFileURLs(homeURL: URL? = nil) -> [URL] {
        let messagesDirectoryURL = opencodeLegacyMessagesDirectoryURL(homeURL: homeURL)
        guard let sessionDirectories = try? FileManager.default.contentsOfDirectory(
            at: messagesDirectoryURL,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else {
            return []
        }

        // Keep this imperative scan split into simple passes. The equivalent
        // chained functional version regressed compile time and hit Swift's
        // type-checker limits in CI on Xcode 16.
        let candidateDirectories = sessionDirectories.filter { entry in
            let isDirectory = (try? entry.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory ?? false
            return isDirectory && entry.lastPathComponent.hasPrefix("ses_")
        }

        var messageFiles: [URL] = []
        messageFiles.reserveCapacity(candidateDirectories.count)

        for directory in candidateDirectories {
            guard let entries = try? FileManager.default.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: [.contentModificationDateKey],
                options: [.skipsHiddenFiles]
            ) else {
                continue
            }

            for entry in entries where entry.pathExtension == "json" {
                messageFiles.append(entry)
            }
        }

        return messageFiles.sorted { $0.path < $1.path }
    }

    static func geminiProjectsURL(homeURL: URL? = nil) -> URL {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL.appendingPathComponent(".gemini/projects.json", isDirectory: false)
    }

    static func geminiTempDirectoryURL(homeURL: URL? = nil) -> URL {
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL.appendingPathComponent(".gemini/tmp", isDirectory: true)
    }

    static func geminiProjectTempDirectoryURL(projectPath: String, homeURL: URL? = nil) -> URL? {
        let projectsURL = geminiProjectsURL(homeURL: homeURL)
        if let data = try? Data(contentsOf: projectsURL),
           let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let projects = object["projects"] as? [String: Any],
           let directoryName = projects[projectPath] as? String,
           !directoryName.isEmpty {
            return geminiTempDirectoryURL(homeURL: homeURL).appendingPathComponent(directoryName, isDirectory: true)
        }

        let tempURL = geminiTempDirectoryURL(homeURL: homeURL)
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

    static func geminiChatsDirectoryURL(projectPath: String, homeURL: URL? = nil) -> URL? {
        geminiProjectTempDirectoryURL(projectPath: projectPath, homeURL: homeURL)?
            .appendingPathComponent("chats", isDirectory: true)
    }

    static func geminiSessionFileURLs(projectPath: String, homeURL: URL? = nil) -> [URL] {
        let modificationKey = URLResourceKey.contentModificationDateKey
        if let chatsDirectoryURL = geminiChatsDirectoryURL(projectPath: projectPath, homeURL: homeURL),
           let fileURLs = try? FileManager.default.contentsOfDirectory(
               at: chatsDirectoryURL,
               includingPropertiesForKeys: [modificationKey],
               options: [.skipsHiddenFiles]
           ) {
            let sessionFiles = fileURLs
                .filter {
                    $0.pathExtension == "json" && $0.lastPathComponent.hasPrefix("session-")
                }
                .sorted {
                    let lhs = ((try? $0.resourceValues(forKeys: [modificationKey]))?.contentModificationDate ?? .distantPast)
                    let rhs = ((try? $1.resourceValues(forKeys: [modificationKey]))?.contentModificationDate ?? .distantPast)
                    return lhs > rhs
                }
            if sessionFiles.isEmpty == false {
                return sessionFiles
            }
        }

        let tempURL = geminiTempDirectoryURL(homeURL: homeURL)
        guard let entries = try? FileManager.default.contentsOfDirectory(
            at: tempURL,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else {
            return []
        }

        return entries
            .filter { entry in
                let markerURL = entry.appendingPathComponent(".project_root", isDirectory: false)
                let markerValue = try? String(contentsOf: markerURL, encoding: .utf8)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return markerValue == projectPath
            }
            .flatMap { entry -> [URL] in
                let chatsURL = entry.appendingPathComponent("chats", isDirectory: true)
                let fileURLs = (try? FileManager.default.contentsOfDirectory(
                    at: chatsURL,
                    includingPropertiesForKeys: [modificationKey],
                    options: [.skipsHiddenFiles]
                )) ?? []
                return fileURLs.filter {
                    $0.pathExtension == "json" && $0.lastPathComponent.hasPrefix("session-")
                }
            }
            .sorted {
                let lhs = ((try? $0.resourceValues(forKeys: [modificationKey]))?.contentModificationDate ?? .distantPast)
                let rhs = ((try? $1.resourceValues(forKeys: [modificationKey]))?.contentModificationDate ?? .distantPast)
                return lhs > rhs
            }
    }

    private static func claudeProjectDirectoryURL(projectPath: String, homeURL: URL?) -> URL {
        let directoryName = projectPath
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ".", with: "-")
        let homeURL = homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
        return homeURL
            .appendingPathComponent(".claude/projects", isDirectory: true)
            .appendingPathComponent(directoryName, isDirectory: true)
    }

    private static func claudeLogFile(_ fileURL: URL, belongsToProjectPath projectPath: String) -> Bool {
        var lineCount = 0
        var matchesProject = false
        JSONLLineReader.forEachLine(in: fileURL) { lineData in
            lineCount += 1
            guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any] else {
                return lineCount < 12
            }
            if let cwd = row["cwd"] as? String {
                matchesProject = (cwd == projectPath)
                return false
            }
            return lineCount < 12
        }
        return matchesProject
    }

    private static func codexRolloutFile(_ fileURL: URL, belongsToProjectPath projectPath: String) -> Bool {
        var lineCount = 0
        var matchesProject = false
        JSONLLineReader.forEachLine(in: fileURL) { lineData in
            lineCount += 1
            guard let row = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any] else {
                return lineCount < 20
            }
            let rowType = row["type"] as? String
            let payload = row["payload"] as? [String: Any] ?? [:]
            if rowType == "session_meta" || rowType == "turn_context",
               let cwd = payload["cwd"] as? String {
                matchesProject = (cwd == projectPath)
                return false
            }
            return lineCount < 20
        }
        return matchesProject
    }

    private static func recursiveJSONLFileURLs(in directoryURL: URL) -> [URL] {
        let enumerator = FileManager.default.enumerator(
            at: directoryURL,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]
        )
        var urls: [URL] = []
        while let next = enumerator?.nextObject() as? URL {
            if next.pathExtension == "jsonl" {
                urls.append(next.standardizedFileURL)
            }
        }
        return urls.sorted { $0.path < $1.path }
    }
}

struct AIRuntimeContextSnapshot: Sendable {
    var tool: String
    var externalSessionID: String?
    var model: String?
    var inputTokens: Int
    var outputTokens: Int
    var cachedInputTokens: Int = 0
    var totalTokens: Int
    var updatedAt: Double
    var startedAt: Double?
    var completedAt: Double?
    var responseState: AIResponseState?
    var wasInterrupted: Bool = false
    var hasCompletedTurn: Bool = false
    var sessionOrigin: AIRuntimeSessionOrigin = .unknown
    var source: AIRuntimeUpdateSource = .probe
}

enum AIRuntimeSessionOrigin: String, Sendable {
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
