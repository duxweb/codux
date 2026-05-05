import XCTest
@testable import DmuxWorkspace

@MainActor
final class WorkspacePaneLayoutTests: XCTestCase {
    func testTerminalStartupLoadingDoesNotBecomeProjectActivityLoading() {
        let projectID = UUID(uuidString: "00000000-0000-0000-0000-000000000020")!
        let sessionID = UUID(uuidString: "40000000-0000-0000-0000-000000000001")!
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        model.projects = [
            Project(
                id: projectID,
                name: "Project",
                path: "/tmp/project-\(projectID.uuidString)",
                shell: "/bin/zsh",
                defaultCommand: "",
                badgeText: nil,
                badgeSymbol: nil,
                badgeColorHex: nil,
                gitDefaultPushRemoteName: nil
            )
        ]
        model.workspaces = [makeWorkspace(projectID: projectID, sessionIDs: [sessionID])]

        model.noteTerminalLoadingState(sessionID, isLoading: true)

        XCTAssertEqual(model.resolvedProjectActivityPhase(projectID: projectID), ProjectActivityPhase.idle)
    }

    func testTopPaneDistributionDoesNotResetWhenSwitchingProjects() {
        let currentWorkspace = makeWorkspace(
            projectID: UUID(uuidString: "00000000-0000-0000-0000-000000000001")!,
            sessionIDs: [
                UUID(uuidString: "10000000-0000-0000-0000-000000000001")!,
                UUID(uuidString: "10000000-0000-0000-0000-000000000002")!,
            ]
        )
        let nextWorkspace = makeWorkspace(
            projectID: UUID(uuidString: "00000000-0000-0000-0000-000000000002")!,
            sessionIDs: [
                UUID(uuidString: "20000000-0000-0000-0000-000000000001")!,
                UUID(uuidString: "20000000-0000-0000-0000-000000000002")!,
            ]
        )

        XCTAssertFalse(
            TopPaneSplitController.shouldResetTopPaneDistribution(
                from: currentWorkspace,
                to: nextWorkspace
            )
        )
    }

    func testTopPaneDistributionResetsForSameProjectStructureChange() {
        let projectID = UUID(uuidString: "00000000-0000-0000-0000-000000000010")!
        let currentWorkspace = makeWorkspace(
            projectID: projectID,
            sessionIDs: [
                UUID(uuidString: "30000000-0000-0000-0000-000000000001")!,
            ]
        )
        let nextWorkspace = makeWorkspace(
            projectID: projectID,
            sessionIDs: [
                UUID(uuidString: "30000000-0000-0000-0000-000000000001")!,
                UUID(uuidString: "30000000-0000-0000-0000-000000000002")!,
            ]
        )

        XCTAssertTrue(
            TopPaneSplitController.shouldResetTopPaneDistribution(
                from: currentWorkspace,
                to: nextWorkspace
            )
        )
    }

    private func makeWorkspace(projectID: UUID, sessionIDs: [UUID]) -> ProjectWorkspace {
        let project = Project(
            id: projectID,
            name: "Project",
            path: "/tmp/project-\(projectID.uuidString)",
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
        let sessions = sessionIDs.map { sessionID in
            var session = TerminalSession.make(project: project, command: "")
            session.id = sessionID
            return session
        }
        return ProjectWorkspace(
            projectID: projectID,
            topSessionIDs: sessionIDs,
            topPaneRatios: Array(repeating: 1, count: sessionIDs.count),
            bottomTabSessionIDs: [],
            bottomPaneHeight: 240,
            selectedSessionID: sessionIDs[0],
            selectedBottomTabSessionID: nil,
            sessions: sessions
        )
    }
}
