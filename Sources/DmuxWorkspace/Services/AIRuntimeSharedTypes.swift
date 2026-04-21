import Foundation
import SQLite3

private let AIRuntimeSQLiteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

struct AIRuntimeSourceLocator {
    static func claudeProjectLogURLs() -> [URL] {
        let baseURL = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".claude/projects", isDirectory: true)
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

    static func claudeSessionLogURL(projectPath: String, externalSessionID: String) -> URL {
        let directoryName = projectPath
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ".", with: "-")
        return URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent(".claude/projects", isDirectory: true)
            .appendingPathComponent(directoryName, isDirectory: true)
            .appendingPathComponent("\(externalSessionID).jsonl", isDirectory: false)
    }

    static func codexDatabaseURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex/state_5.sqlite", isDirectory: false)
    }

    static func codexRolloutPath(projectPath: String, externalSessionID: String) -> URL? {
        let databaseURL = codexDatabaseURL()
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
        let modificationKey = URLResourceKey.contentModificationDateKey
        guard let chatsDirectoryURL = geminiChatsDirectoryURL(projectPath: projectPath),
              let fileURLs = try? FileManager.default.contentsOfDirectory(
                  at: chatsDirectoryURL,
                  includingPropertiesForKeys: [modificationKey],
                  options: [.skipsHiddenFiles]
              ) else {
            return []
        }

        return fileURLs
            .filter {
                $0.pathExtension == "json" && $0.lastPathComponent.hasPrefix("session-")
            }
            .sorted {
                let lhs = ((try? $0.resourceValues(forKeys: [modificationKey]))?.contentModificationDate ?? .distantPast)
                let rhs = ((try? $1.resourceValues(forKeys: [modificationKey]))?.contentModificationDate ?? .distantPast)
                return lhs > rhs
            }
    }
}

struct AIRuntimeContextSnapshot: Sendable {
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
    var source: AIRuntimeUpdateSource = .probe
}

struct AIManualInterruptEvent: Codable, Equatable, Sendable {
    var terminalID: UUID
    var updatedAt: Double
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
