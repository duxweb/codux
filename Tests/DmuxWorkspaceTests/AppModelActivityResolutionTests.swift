import XCTest
@testable import DmuxWorkspace

final class AppModelActivityResolutionTests: XCTestCase {
    func testResolveDisplayedActivityPhaseUsesRuntimeOnlyForRealtimeCachedPhase() {
        let resolved = AppModel.resolveDisplayedActivityPhase(
            runtimePhase: .idle,
            cachedPhase: .completed(tool: "codex", finishedAt: .init(timeIntervalSince1970: 100), exitCode: nil),
            completionPhase: .idle,
            cachedPayloadTool: nil,
            hasLiveRuntimeSessions: false,
            isRealtimeTool: { ["codex", "claude", "gemini", "opencode"].contains($0) }
        )

        XCTAssertEqual(resolved, .idle)
    }

    func testResolveDisplayedActivityPhaseFallsBackToCachedForNonRealtimePayload() {
        let finishedAt = Date(timeIntervalSince1970: 100)
        let resolved = AppModel.resolveDisplayedActivityPhase(
            runtimePhase: .idle,
            cachedPhase: .completed(tool: "buildkite", finishedAt: finishedAt, exitCode: 0),
            completionPhase: .idle,
            cachedPayloadTool: "buildkite",
            hasLiveRuntimeSessions: false,
            isRealtimeTool: { ["codex", "claude", "gemini", "opencode"].contains($0) }
        )

        XCTAssertEqual(resolved, .completed(tool: "buildkite", finishedAt: finishedAt, exitCode: 0))
    }
}
