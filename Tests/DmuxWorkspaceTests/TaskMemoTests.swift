import XCTest
@testable import DmuxWorkspace

@MainActor
final class TaskMemoTests: XCTestCase {
    func testTaskMemosAreScopedToProjectAndSession() throws {
        let project = makeProject()
        let sessionA = makeSession(project: project, id: UUID(uuidString: "10000000-0000-0000-0000-000000000001")!)
        let sessionB = makeSession(project: project, id: UUID(uuidString: "10000000-0000-0000-0000-000000000002")!)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        model.projects = [project]
        model.workspaces = [
            ProjectWorkspace(
                projectID: project.id,
                topSessionIDs: [sessionA.id, sessionB.id],
                topPaneRatios: [0.5, 0.5],
                bottomTabSessionIDs: [],
                bottomPaneHeight: 240,
                selectedSessionID: sessionA.id,
                selectedBottomTabSessionID: nil,
                sessions: [sessionA, sessionB]
            )
        ]
        model.selectedProjectID = project.id

        let queued = try XCTUnwrap(model.addTaskMemo(projectID: project.id, sessionID: sessionA.id, content: "first"))
        let waiting = try XCTUnwrap(model.addTaskMemo(projectID: project.id, sessionID: sessionA.id, content: "second", status: .waiting))
        let completed = try XCTUnwrap(model.addTaskMemo(projectID: project.id, sessionID: sessionA.id, content: "third", status: .completed))
        _ = model.addTaskMemo(projectID: project.id, sessionID: sessionB.id, content: "other")

        XCTAssertEqual(model.taskMemos(for: project.id, sessionID: sessionA.id).map(\.id), [queued.id, waiting.id, completed.id])
        XCTAssertEqual(model.taskMemoCounts(projectID: project.id, sessionID: sessionA.id).queued, 1)
        XCTAssertEqual(model.taskMemoCounts(projectID: project.id, sessionID: sessionA.id).waiting, 1)
        XCTAssertEqual(model.taskMemoCounts(projectID: project.id, sessionID: sessionA.id).completed, 1)

        model.setTaskMemoStatus(queued.id, status: .waiting)
        XCTAssertEqual(model.taskMemos.first(where: { $0.id == queued.id })?.status, .waiting)

        model.setTaskMemoStatus(queued.id, status: .completed)
        XCTAssertEqual(model.taskMemos.first(where: { $0.id == queued.id })?.status, .completed)

        model.requeueTaskMemo(queued.id)
        XCTAssertEqual(model.taskMemos.first(where: { $0.id == queued.id })?.status, .queued)
        XCTAssertNil(model.taskMemos.first(where: { $0.id == queued.id })?.lastSentAt)

        model.updateTaskMemo(waiting.id, content: "updated")
        XCTAssertEqual(model.taskMemos.first(where: { $0.id == waiting.id })?.content, "updated")

        model.deleteTaskMemo(waiting.id)
        XCTAssertNil(model.taskMemos.first(where: { $0.id == waiting.id }))
    }

    func testDuplicateContentCanBeQueuedAsSeparateMemos() throws {
        let project = makeProject()
        let session = makeSession(project: project, id: UUID(uuidString: "10000000-0000-0000-0000-000000000001")!)
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        model.projects = [project]
        model.workspaces = [
            ProjectWorkspace(
                projectID: project.id,
                topSessionIDs: [session.id],
                topPaneRatios: [1],
                bottomTabSessionIDs: [],
                bottomPaneHeight: 240,
                selectedSessionID: session.id,
                selectedBottomTabSessionID: nil,
                sessions: [session]
            )
        ]
        model.selectedProjectID = project.id

        let first = try XCTUnwrap(model.addTaskMemo(projectID: project.id, sessionID: session.id, content: "same"))
        let second = try XCTUnwrap(model.addTaskMemo(projectID: project.id, sessionID: session.id, content: "same"))

        XCTAssertNotEqual(first.id, second.id)
        XCTAssertEqual(model.taskMemos(for: project.id, sessionID: session.id).map(\.content), ["same", "same"])
        XCTAssertEqual(model.taskMemoCounts(projectID: project.id, sessionID: session.id).queued, 2)
    }

    func testCompletedMemoRequeueCanBeSelectedAgainEvenAfterSent() throws {
        let projectID = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
        let sessionID = UUID(uuidString: "10000000-0000-0000-0000-000000000001")!
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        let sentAt = Date()
        let memo = TaskMemoItem(
            id: UUID(uuidString: "20000000-0000-0000-0000-000000000001")!,
            projectID: projectID,
            sessionID: sessionID,
            content: "again",
            status: .completed,
            createdAt: sentAt,
            updatedAt: sentAt,
            lastSentAt: sentAt
        )
        model.taskMemos = [memo]

        model.requeueTaskMemo(memo.id)

        let requeued = try XCTUnwrap(model.taskMemos.first)
        XCTAssertEqual(requeued.status, .queued)
        XCTAssertNil(requeued.lastSentAt)
    }

    func testExecuteTaskMemoNowFailsWhenTerminalIsUnavailable() throws {
        let projectID = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
        let sessionID = UUID(uuidString: "10000000-0000-0000-0000-000000000001")!
        let model = AppModel(snapshot: nil, persistenceService: PersistenceService())
        let now = Date()
        let memo = TaskMemoItem(
            id: UUID(uuidString: "20000000-0000-0000-0000-000000000001")!,
            projectID: projectID,
            sessionID: sessionID,
            content: "send now",
            status: .queued,
            createdAt: now,
            updatedAt: now,
            lastSentAt: nil
        )
        model.taskMemos = [memo]

        XCTAssertFalse(model.executeTaskMemoNow(memo.id))
        XCTAssertEqual(model.taskMemos.first?.status, .queued)
        XCTAssertNil(model.taskMemos.first?.lastSentAt)
    }

    func testSnapshotDecodesWithoutTaskMemosForBackwardCompatibility() throws {
        let json = """
        {
          "projects": [],
          "workspaces": [],
          "selectedProjectID": null,
          "appSettings": null
        }
        """.data(using: .utf8)!

        let snapshot = try JSONDecoder().decode(AppSnapshot.self, from: json)

        XCTAssertNil(snapshot.taskMemos)
    }

    func testLegacyStashedStatusDecodesAsWaiting() throws {
        let json = #"""
        {
          "id": "20000000-0000-0000-0000-000000000001",
          "projectID": "00000000-0000-0000-0000-000000000001",
          "sessionID": "10000000-0000-0000-0000-000000000001",
          "content": "legacy",
          "status": "stashed",
          "createdAt": 735000000,
          "updatedAt": 735000000
        }
        """#.data(using: .utf8)!

        let item = try JSONDecoder().decode(TaskMemoItem.self, from: json)

        XCTAssertEqual(item.status, .waiting)
    }

    private func makeProject() -> Project {
        Project(
            id: UUID(uuidString: "00000000-0000-0000-0000-000000000001")!,
            name: "Project",
            path: "/tmp/project-task-memo",
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
    }

    private func makeSession(project: Project, id: UUID) -> TerminalSession {
        TerminalSession(
            id: id,
            projectID: project.id,
            projectName: project.name,
            title: "Terminal",
            tabTitle: nil,
            cwd: project.path,
            shell: project.shell,
            command: project.shell,
            previewLines: [],
            launchMode: .terminal,
            agentTool: nil
        )
    }
}
