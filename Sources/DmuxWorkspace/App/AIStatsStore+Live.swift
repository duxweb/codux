import Foundation

@MainActor
extension AIStatsStore {
    func refreshLiveState(
        project: Project,
        selectedSessionID: UUID?,
        reason: LiveRefreshReason
    ) {
        let liveContext = liveSnapshotContext(projectID: project.id, selectedSessionID: selectedSessionID)
        let status = projectIndexingStatus(
            projectID: project.id,
            fallback: .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))
        )
        let currentState = panelStateByProjectID[project.id] ?? cachedState(for: project.id) ?? .empty
        var nextState = aiUsageService.lightweightLivePanelState(
            from: currentState,
            project: project,
            liveSnapshots: liveContext.summary,
            currentSnapshot: liveContext.current,
            status: status
        )
        nextState.liveSnapshots = liveContext.display
        if nextState == currentState,
           restingRefreshState(projectID: project.id) == .idle {
            return
        }

        logger.log(
            "ai-live-refresh",
            "project=\(project.id.uuidString) reason=\(reason.rawValue) live=\(nextState.liveSnapshots.count) current=\(nextState.currentSnapshot?.sessionID.uuidString ?? "nil")"
        )
        storeState(nextState, refreshState: .idle, for: project.id, updateCurrent: true)
        syncCurrentAutomaticRefreshFlag()
    }

    func ingestRuntime(project: Project, projects: [Project], selectedSessionID: UUID?) -> [AITerminalSessionSnapshot] {
        runtimeIngressService.importRuntime(projects: projects)
        return resolveProjectLiveSnapshots(project: project, selectedSessionID: selectedSessionID)
    }

    func handleTerminalSessionClosed(sessionID: UUID, project: Project?, projects: [Project], selectedSessionID: UUID?) {
        aiSessionStore.removeTerminal(sessionID)
        guard let project else {
            return
        }
        let resolvedSelectedSessionID = effectiveSessionID(selectedSessionID)
        let liveContext = liveSnapshotContext(projectID: project.id, selectedSessionID: resolvedSelectedSessionID)
        let status = projectIndexingStatus(
            projectID: project.id,
            fallback: .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))
        )
        let currentState = panelStateByProjectID[project.id] ?? cachedState(for: project.id) ?? .empty
        var nextState = aiUsageService.lightweightLivePanelState(
            from: currentState,
            project: project,
            liveSnapshots: liveContext.summary,
            currentSnapshot: liveContext.current,
            status: status
        )
        nextState.liveSnapshots = liveContext.display
        if nextState == currentState,
           restingRefreshState(projectID: project.id) == .idle {
            return
        }
        storeState(nextState, refreshState: .idle, for: project.id, updateCurrent: true)
        syncCurrentAutomaticRefreshFlag()
    }

    func resolveProjectLiveSnapshots(
        project: Project,
        selectedSessionID: UUID?
    ) -> [AITerminalSessionSnapshot] {
        var resolved = aiSessionStore.liveSnapshots(projectID: project.id)

        if let selectedSessionID,
           resolved.contains(where: { $0.sessionID == selectedSessionID }) == false,
           let snapshot = aiSessionStore.currentDisplaySnapshot(projectID: project.id, selectedSessionID: selectedSessionID) {
            resolved.append(snapshot)
        }

        return resolved
            .filter { $0.status == "running" }
            .sorted { $0.updatedAt > $1.updatedAt }
    }
}
