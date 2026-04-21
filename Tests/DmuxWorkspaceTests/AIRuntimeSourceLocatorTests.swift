import XCTest
import SQLite3
@testable import DmuxWorkspace

final class AIRuntimeSourceLocatorTests: XCTestCase {
    func testClaudeProjectLogURLsOnlyReadsCurrentProjectDirectory() throws {
        let homeURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-airuntime-locator-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: homeURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: homeURL) }

        let projectPath = "/Volumes/Web/codux"
        let otherProjectPath = "/Volumes/Web/other"

        let currentFile = AIRuntimeSourceLocator.claudeSessionLogURL(
            projectPath: projectPath,
            externalSessionID: "current",
            homeURL: homeURL
        )
        let otherFile = AIRuntimeSourceLocator.claudeSessionLogURL(
            projectPath: otherProjectPath,
            externalSessionID: "other",
            homeURL: homeURL
        )
        try FileManager.default.createDirectory(at: currentFile.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: otherFile.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "{}\n".write(to: currentFile, atomically: true, encoding: .utf8)
        try "{}\n".write(to: otherFile, atomically: true, encoding: .utf8)

        let urls = AIRuntimeSourceLocator.claudeProjectLogURLs(projectPath: projectPath, homeURL: homeURL)

        XCTAssertEqual(
            urls.map { $0.resolvingSymlinksInPath().standardizedFileURL },
            [currentFile.resolvingSymlinksInPath().standardizedFileURL]
        )
    }

    func testClaudeProjectLogURLsFallsBackToScanningWhenDirectoryNameDoesNotMatchProjectPath() throws {
        let homeURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-airuntime-locator-fallback-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: homeURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: homeURL) }

        let projectPath = "/Volumes/Web/未命名文件夹"
        let baseURL = homeURL.appendingPathComponent(".claude/projects", isDirectory: true)
        let actualDirectory = baseURL.appendingPathComponent("-Volumes-Web-------", isDirectory: true)
        try FileManager.default.createDirectory(at: actualDirectory, withIntermediateDirectories: true)

        let matchingFile = actualDirectory.appendingPathComponent("match.jsonl", isDirectory: false)
        let otherFile = actualDirectory.appendingPathComponent("skip.jsonl", isDirectory: false)
        try """
        {"type":"user","cwd":"\(projectPath)","sessionId":"session-match","timestamp":"2026-04-21T10:21:22.206Z"}
        """.appending("\n").write(to: matchingFile, atomically: true, encoding: .utf8)
        try """
        {"type":"user","cwd":"/Volumes/Web/other","sessionId":"session-skip","timestamp":"2026-04-21T10:21:22.206Z"}
        """.appending("\n").write(to: otherFile, atomically: true, encoding: .utf8)

        let urls = AIRuntimeSourceLocator.claudeProjectLogURLs(projectPath: projectPath, homeURL: homeURL)

        XCTAssertEqual(
            urls.map { $0.resolvingSymlinksInPath().standardizedFileURL },
            [matchingFile.resolvingSymlinksInPath().standardizedFileURL]
        )
    }

    func testCodexSessionFileURLsUsesStateDatabaseForProjectFilter() throws {
        let rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-codex-locator-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: rootURL) }

        let rolloutURL = rootURL.appendingPathComponent("session-a.jsonl", isDirectory: false)
        let otherRolloutURL = rootURL.appendingPathComponent("session-b.jsonl", isDirectory: false)
        try "{}\n".write(to: rolloutURL, atomically: true, encoding: .utf8)
        try "{}\n".write(to: otherRolloutURL, atomically: true, encoding: .utf8)

        let databaseURL = rootURL.appendingPathComponent("state_5.sqlite", isDirectory: false)
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(databaseURL.path, &db), SQLITE_OK)
        guard let db else {
            XCTFail("failed to open sqlite db")
            return
        }
        defer { sqlite3_close(db) }

        XCTAssertEqual(
            sqlite3_exec(
                db,
                """
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    cwd TEXT,
                    rollout_path TEXT,
                    updated_at INTEGER
                );
                """,
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )

        XCTAssertEqual(
            sqlite3_exec(
                db,
                """
                INSERT INTO threads (id, cwd, rollout_path, updated_at) VALUES
                ('match-a', '/Volumes/Web/codux', '\(rolloutURL.path)', 2),
                ('skip-b', '/Volumes/Web/other', '\(otherRolloutURL.path)', 1),
                ('match-c', '/Volumes/Web/codux', '\(rolloutURL.path)', 3);
                """,
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )

        let urls = AIRuntimeSourceLocator.codexSessionFileURLs(
            projectPath: "/Volumes/Web/codux",
            databaseURL: databaseURL
        )

        XCTAssertEqual(urls, [rolloutURL.standardizedFileURL])
    }

    func testGeminiSessionFileURLsFallsBackToProjectRootMarkerScan() throws {
        let homeURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-gemini-locator-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: homeURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: homeURL) }

        let projectPath = "/Volumes/Web/current-project"
        let matchedRoot = homeURL.appendingPathComponent(".gemini/tmp/matched", isDirectory: true)
        let otherRoot = homeURL.appendingPathComponent(".gemini/tmp/other", isDirectory: true)
        try FileManager.default.createDirectory(at: matchedRoot.appendingPathComponent("chats", isDirectory: true), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: otherRoot.appendingPathComponent("chats", isDirectory: true), withIntermediateDirectories: true)
        try "\(projectPath)\n".write(to: matchedRoot.appendingPathComponent(".project_root"), atomically: true, encoding: .utf8)
        try "/Volumes/Web/other\n".write(to: otherRoot.appendingPathComponent(".project_root"), atomically: true, encoding: .utf8)

        let matchedFile = matchedRoot.appendingPathComponent("chats/session-a.json")
        let otherFile = otherRoot.appendingPathComponent("chats/session-b.json")
        try "{}".write(to: matchedFile, atomically: true, encoding: .utf8)
        try "{}".write(to: otherFile, atomically: true, encoding: .utf8)

        let urls = AIRuntimeSourceLocator.geminiSessionFileURLs(projectPath: projectPath, homeURL: homeURL)

        XCTAssertEqual(
            urls.map { $0.standardizedFileURL },
            [matchedFile.standardizedFileURL]
        )
    }

    func testCodexSessionFileURLsFallsBackToSessionsDirectoryScan() throws {
        let homeURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmux-codex-session-scan-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: homeURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: homeURL) }

        let matchedFile = homeURL
            .appendingPathComponent(".codex/sessions/2026/04/21/rollout-a.jsonl", isDirectory: false)
        let otherFile = homeURL
            .appendingPathComponent(".codex/sessions/2026/04/21/rollout-b.jsonl", isDirectory: false)
        try FileManager.default.createDirectory(at: matchedFile.deletingLastPathComponent(), withIntermediateDirectories: true)
        try """
        {"timestamp":"2026-04-21T10:00:00Z","type":"session_meta","payload":{"cwd":"/Volumes/Web/current-project","id":"thread-a"}}
        """.appending("\n").write(to: matchedFile, atomically: true, encoding: .utf8)
        try """
        {"timestamp":"2026-04-21T10:00:00Z","type":"session_meta","payload":{"cwd":"/Volumes/Web/other-project","id":"thread-b"}}
        """.appending("\n").write(to: otherFile, atomically: true, encoding: .utf8)

        let urls = AIRuntimeSourceLocator.codexSessionFileURLs(
            projectPath: "/Volumes/Web/current-project",
            homeURL: homeURL
        )

        XCTAssertEqual(urls, [matchedFile.standardizedFileURL])
    }
}
