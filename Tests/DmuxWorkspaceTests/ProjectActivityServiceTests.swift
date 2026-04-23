import XCTest
@testable import DmuxWorkspace

final class ProjectActivityServiceTests: XCTestCase {
    func testRunningPayloadExpiresToIdleAfterLifetime() {
        let service = ProjectActivityService()
        let payload = ProjectActivityPayload(
            tool: "claude",
            phase: "running",
            updatedAt: Date().timeIntervalSince1970 - 16,
            finishedAt: nil,
            exitCode: nil
        )

        XCTAssertEqual(service.phase(for: payload), .idle)
    }

    func testCompletedPayloadPersistsUntilExplicitlyCleared() {
        let service = ProjectActivityService()
        let payload = ProjectActivityPayload(
            tool: "codex",
            phase: "completed",
            updatedAt: Date().timeIntervalSince1970 - 21,
            finishedAt: Date().timeIntervalSince1970 - 21,
            exitCode: 0
        )

        guard case .completed(let tool, _, let exitCode) = service.phase(for: payload) else {
            return XCTFail("expected completed phase")
        }
        XCTAssertEqual(tool, "codex")
        XCTAssertEqual(exitCode, 0)
    }

    func testFreshCompletedPayloadPreservesToolAndExitCode() {
        let service = ProjectActivityService()
        let finishedAt = Date().timeIntervalSince1970 - 2
        let payload = ProjectActivityPayload(
            tool: "gemini",
            phase: "completed",
            updatedAt: finishedAt,
            finishedAt: finishedAt,
            exitCode: 7
        )

        guard case .completed(let tool, _, let exitCode) = service.phase(for: payload) else {
            return XCTFail("expected completed phase")
        }
        XCTAssertEqual(tool, "gemini")
        XCTAssertEqual(exitCode, 7)
    }

    func testCompletionTokenUsesToolTimestampAndExitCode() {
        let service = ProjectActivityService()
        let payload = ProjectActivityPayload(
            tool: "opencode",
            phase: "completed",
            updatedAt: 123.5,
            finishedAt: 124,
            exitCode: 2
        )

        XCTAssertEqual(service.completionToken(for: payload), "opencode-123.5-2")
    }
}
