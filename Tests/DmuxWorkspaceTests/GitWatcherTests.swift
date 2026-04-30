import CoreServices
import XCTest
@testable import DmuxWorkspace

final class GitWatcherTests: XCTestCase {
    func testSideBySideDiffLabelsAddedLinesOnTheNewSide() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try "one\ntwo\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)

        let preview = try GitService().sideBySideDiff(
            for: GitFileEntry(path: "Demo.txt", kind: .staged),
            at: root.path
        )

        XCTAssertEqual(preview.newTitle, "New File")
        XCTAssertEqual(preview.oldTitle, "Old File")
        XCTAssertTrue(preview.rows.contains { $0.kind == .added && $0.newLine?.text == "one" && $0.oldLine == nil })
    }

    func testWorktreeFileEventsStillRefreshGitSidebar() {
        XCTAssertTrue(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: "/tmp/repo",
                path: "/tmp/repo/Sources/App.swift",
                flags: 0
            )
        )
    }

    func testGitMetadataEventsThatAffectStatusAreForwarded() {
        let repositoryPath = "/tmp/repo"

        XCTAssertTrue(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: repositoryPath,
                path: "/tmp/repo/.git/index",
                flags: 0
            )
        )
        XCTAssertTrue(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: repositoryPath,
                path: "/tmp/repo/.git/HEAD",
                flags: 0
            )
        )
        XCTAssertTrue(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: repositoryPath,
                path: "/tmp/repo/.git/refs/heads/main",
                flags: 0
            )
        )
    }

    func testIrrelevantGitDirectoryEventsStayFiltered() {
        XCTAssertFalse(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: "/tmp/repo",
                path: "/tmp/repo/.git",
                flags: 0
            )
        )
        XCTAssertFalse(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: "/tmp/repo",
                path: "/tmp/repo/.git/objects/ab/cdef",
                flags: 0
            )
        )
        XCTAssertFalse(
            GitRepositoryWatchFilter.shouldForward(
                repositoryPath: "/tmp/repo",
                path: "/tmp/repo/.git/config",
                flags: FSEventStreamEventFlags(kFSEventStreamEventFlagHistoryDone)
            )
        )
    }

    private func runGit(_ arguments: [String], at url: URL) throws {
        let process = Process()
        process.currentDirectoryURL = url
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["git"] + arguments
        let stderr = Pipe()
        process.standardError = stderr
        try process.run()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            let message = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "git failed"
            XCTFail(message)
        }
    }
}
