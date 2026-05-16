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

    func testGitServiceDetectsTreeDiffBetweenBranches() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "one\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try runGit(["checkout", "-b", "task/demo"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["commit", "-am", "Task change"], at: root)

        XCTAssertTrue(try GitService().hasDiff(from: "main", to: "task/demo", at: root.path))

        try runGit(["checkout", "main"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["commit", "-am", "Squashed task change"], at: root)

        XCTAssertFalse(try GitService().hasDiff(from: "main", to: "task/demo", at: root.path))
    }

    func testWorktreeReviewFilesAndComparisonUseBaseAndWorktreeContent() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "one\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try runGit(["checkout", "-b", "task/demo"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try "new\n".write(to: root.appendingPathComponent("New.txt"), atomically: true, encoding: .utf8)

        let service = GitService()
        let files = try service.worktreeReviewFiles(from: "main", at: root.path)

        XCTAssertEqual(files.map(\.path), ["Demo.txt", "New.txt"])
        XCTAssertEqual(files.first(where: { $0.path == "Demo.txt" })?.status, .modified)
        XCTAssertEqual(files.first(where: { $0.path == "New.txt" })?.status, .added)

        let demo = try XCTUnwrap(files.first(where: { $0.path == "Demo.txt" }))
        let comparison = try service.worktreeReviewComparison(for: demo, baseRef: "main", at: root.path)

        XCTAssertEqual(comparison.baseText, "one\n")
        XCTAssertEqual(comparison.worktreeText, "two\n")
        XCTAssertEqual(comparison.resultText, "two\n")
        XCTAssertFalse(comparison.baseDeletesFile)
        XCTAssertFalse(comparison.worktreeDeletesFile)
        XCTAssertFalse(comparison.resultDeletesFile)
    }

    func testWorkingTreeAuditFilesAndComparisonUseHeadAndWorkingTreeContent() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "one\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try "new\n".write(to: root.appendingPathComponent("New.txt"), atomically: true, encoding: .utf8)

        let service = GitService()
        let files = try service.workingTreeAuditFiles(at: root.path)

        XCTAssertEqual(files.map(\.path), ["Demo.txt", "New.txt"])
        XCTAssertEqual(files.first(where: { $0.path == "Demo.txt" })?.status, .modified)
        XCTAssertEqual(files.first(where: { $0.path == "New.txt" })?.status, .added)

        let demo = try XCTUnwrap(files.first(where: { $0.path == "Demo.txt" }))
        let comparison = try service.workingTreeAuditComparison(for: demo, at: root.path)

        XCTAssertEqual(comparison.baseTitle, "HEAD")
        XCTAssertEqual(comparison.worktreeTitle, "Working Tree")
        XCTAssertEqual(comparison.resultTitle, "Audit Result")
        XCTAssertEqual(comparison.baseText, "one\n")
        XCTAssertEqual(comparison.worktreeText, "two\n")
        XCTAssertEqual(comparison.resultText, "two\n")
    }

    func testWorktreeReviewResultCanWriteAndDeleteFile() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        let nestedURL = root.appendingPathComponent("Sources/Demo.txt")
        let change = WorktreeReviewFileChange(
            path: "Sources/Demo.txt",
            oldPath: nil,
            status: .modified,
            additions: nil,
            deletions: nil
        )
        let service = GitService()

        try service.writeWorktreeReviewResult("merged\n", deletesFile: false, for: change, at: root.path)
        XCTAssertEqual(try String(contentsOf: nestedURL, encoding: .utf8), "merged\n")

        try service.writeWorktreeReviewResult("", deletesFile: true, for: change, at: root.path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: nestedURL.path))
    }

    func testDiscardWorktreeReviewFileDiscardsTrackedAndUntrackedChanges() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "one\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try "new\n".write(to: root.appendingPathComponent("New.txt"), atomically: true, encoding: .utf8)

        let service = GitService()
        try service.discardWorktreeReviewFile("Demo.txt", at: root.path)
        try service.discardWorktreeReviewFile("New.txt", at: root.path)

        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("Demo.txt"), encoding: .utf8), "one\n")
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("New.txt").path))
        XCTAssertEqual(try service.repositoryState(at: root.path)?.totalChanges, 0)
    }

    func testDiscardWorkingTreeChangesDiscardsAllTrackedAndUntrackedChanges() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "one\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try "two\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try "new\n".write(to: root.appendingPathComponent("New.txt"), atomically: true, encoding: .utf8)

        let service = GitService()
        try service.discardWorkingTreeChanges(at: root.path)

        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("Demo.txt"), encoding: .utf8), "one\n")
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("New.txt").path))
        XCTAssertEqual(try service.repositoryState(at: root.path)?.totalChanges, 0)
    }

    func testWorktreeMergeConflictPreflightFindsConflictingPath() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try runGit(["init"], at: root)
        try runGit(["config", "user.name", "Codux Tests"], at: root)
        try runGit(["config", "user.email", "codux-tests@example.com"], at: root)
        try runGit(["checkout", "-b", "main"], at: root)
        try "base\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["add", "Demo.txt"], at: root)
        try runGit(["commit", "-m", "Initial"], at: root)
        try runGit(["checkout", "-b", "task/demo"], at: root)
        try "task\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["commit", "-am", "Task"], at: root)
        try runGit(["checkout", "main"], at: root)
        try "main\n".write(to: root.appendingPathComponent("Demo.txt"), atomically: true, encoding: .utf8)
        try runGit(["commit", "-am", "Main"], at: root)

        let conflicts = try GitService().mergeConflictPaths(from: "main", to: "task/demo", at: root.path)

        XCTAssertTrue(conflicts.contains("Demo.txt"), "Expected Demo.txt conflict, got \(conflicts)")
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
