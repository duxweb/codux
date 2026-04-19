import CoreServices
import XCTest
@testable import DmuxWorkspace

final class GitWatcherTests: XCTestCase {
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
}
