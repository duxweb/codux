import Foundation

@MainActor
extension AIStatsStore {
    func startTimers(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        panelVisibilityProvider = isPanelVisible
        selectedProjectProvider = selectedProject
        selectedSessionIDProvider = selectedSessionID
        projectsProvider = projects

        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: max(30, automaticRefreshInterval), repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self, isPanelVisible(), let project = selectedProject() else {
                    return
                }
                let sessionID = self.effectiveSessionID(selectedSessionID())
                self.setRefreshFlags(projectID: project.id, automatic: true, manual: false)
                self.syncCurrentAutomaticRefreshFlag()
                self.refresh(
                    project: project,
                    projects: projects(),
                    selectedSessionID: sessionID,
                    force: false,
                    trigger: .automatic
                )
            }
        }

        backgroundRefreshTimer?.invalidate()
        backgroundRefreshTimer = Timer.scheduledTimer(withTimeInterval: backgroundRefreshInterval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else {
                    return
                }
                self.runBackgroundRefreshTick(
                    isPanelVisible: isPanelVisible,
                    selectedProject: selectedProject,
                    selectedSessionID: selectedSessionID,
                    projects: projects
                )
            }
        }
        startRuntimeBridgeObserver()

        startTerminalFocusObserver(
            isPanelVisible: isPanelVisible,
            selectedProject: selectedProject,
            selectedSessionID: selectedSessionID
        )
    }

    func configureIntervals(automatic: TimeInterval, background: TimeInterval) {
        automaticRefreshInterval = max(30, automatic)
        backgroundRefreshInterval = max(60, background)
        if let panelVisibilityProvider, let selectedProjectProvider, let selectedSessionIDProvider, let projectsProvider {
            startTimers(
                isPanelVisible: panelVisibilityProvider,
                selectedProject: selectedProjectProvider,
                selectedSessionID: selectedSessionIDProvider,
                projects: projectsProvider
            )
        }
    }

    func refreshLocalizedStatusTexts() {
        let projectIDs = Set(indexingStatusByProjectID.keys)
            .union(panelStateByProjectID.keys)
            .union(cachedPanels.projectIDs)

        for projectID in projectIDs {
            if let status = indexingStatusByProjectID[projectID] {
                indexingStatusByProjectID[projectID] = relocalizedStatus(status)
            }
            if var panelState = panelStateByProjectID[projectID] {
                panelState.indexingStatus = relocalizedStatus(panelState.indexingStatus)
                let nextRefreshState = refreshStateByProjectID[projectID] ?? .idle
                storeState(panelState, refreshState: nextRefreshState, for: projectID, updateCurrent: currentProjectID == projectID)
            }
        }
    }

    func refreshIfNeeded(project: Project?, projects: [Project], selectedSessionID: UUID?) {
        let selectedSessionID = effectiveSessionID(selectedSessionID)
        updateSelectionContext(project: project, projects: projects, selectedSessionID: selectedSessionID)
        syncCurrentAutomaticRefreshFlag()
        guard let project else {
            clearCurrentState()
            return
        }

        _ = ingestRuntime(project: project, projects: projects, selectedSessionID: currentSelectedSessionID)
        let liveContext = liveSnapshotContext(projectID: project.id, selectedSessionID: selectedSessionID)
        let persistedIndexedSnapshot = aiUsageStore.indexedProjectSnapshot(projectID: project.id)
        if cachedState(for: project.id) == nil, let persistedIndexedSnapshot {
            logger.log(
                "history-refresh",
                "hydrate persisted project=\(project.id.uuidString) indexedAt=\(persistedIndexedSnapshot.indexedAt.timeIntervalSince1970) projectTotal=\(persistedIndexedSnapshot.projectSummary.projectTotalTokens) todayTotal=\(persistedIndexedSnapshot.projectSummary.todayTotalTokens) sessions=\(persistedIndexedSnapshot.sessions.count)"
            )
        }
        let cachedState = cachedState(for: project.id) ?? persistedIndexedSnapshot.map { _ in
            aiUsageService.snapshotBackedPanelState(
                project: project,
                liveSnapshots: liveContext.summary,
                currentSnapshot: liveContext.current,
                status: .completed(detail: String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module))
            )
        }
        let isFirstOpenThisLaunch = openedProjectIDsThisLaunch.insert(project.id).inserted
        let refreshDecision = automaticRefreshDecision(
            projectID: project.id,
            state: cachedState,
            interval: automaticRefreshInterval,
            forceOnFirstOpen: isFirstOpenThisLaunch
        )
        let shouldRefresh = refreshDecision.shouldRefresh
        let trigger: RefreshTrigger? = if shouldRefresh {
            if isFirstOpenThisLaunch || persistedIndexedSnapshot == nil && cachedState == nil {
                .initial
            } else {
                .automatic
            }
        } else {
            nil
        }

        let projectRefreshState: PanelRefreshState
        if refreshTasks[project.id] != nil {
            projectRefreshState = isAutomaticRefreshInProgress(projectID: project.id) ? .showingCached : .refreshing
        } else if shouldRefresh {
            projectRefreshState = trigger == .automatic && cachedState != nil ? .showingCached : .refreshing
        } else {
            projectRefreshState = restingRefreshState(projectID: project.id)
        }
        if let cachedState {
            let status = projectIndexingStatus(projectID: project.id, fallback: cachedState.indexingStatus)
            var nextState = aiUsageService.snapshotBackedPanelState(
                project: project,
                liveSnapshots: liveContext.summary,
                currentSnapshot: liveContext.current,
                status: status
            )
            nextState.liveSnapshots = liveContext.display
            nextState.indexingStatus = status
            storeState(nextState, refreshState: projectRefreshState, for: project.id, updateCurrent: true)
        } else {
            var emptyState = aiUsageService.fastPanelState(
                project: project,
                liveSnapshots: liveContext.summary,
                currentSnapshot: liveContext.current
            )
            emptyState.liveSnapshots = liveContext.display
            storeState(emptyState, refreshState: .refreshing, for: project.id, updateCurrent: true)
        }

        if shouldRefresh {
            guard let trigger else { return }
            if trigger == .automatic {
                automaticRefreshInProgressByProjectID[project.id] = true
                manualRefreshInProgressByProjectID[project.id] = false
                syncCurrentAutomaticRefreshFlag()
            }
            refresh(
                project: project,
                projects: projects,
                selectedSessionID: selectedSessionID,
                force: false,
                trigger: trigger
            )
        }
    }

    func refreshCurrent(project: Project?, projects: [Project], selectedSessionID: UUID?) {
        guard let project else { return }
        let selectedSessionID = effectiveSessionID(selectedSessionID)
        updateSelectionContext(project: project, projects: projects, selectedSessionID: selectedSessionID)
        setRefreshFlags(projectID: project.id, automatic: false, manual: true)
        syncCurrentAutomaticRefreshFlag()
        refresh(
            project: project,
            projects: projects,
            selectedSessionID: selectedSessionID,
            force: true,
            trigger: .manual
        )
    }

    func runBackgroundRefreshTick(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?,
        projects: @escaping @MainActor () -> [Project]
    ) {
        if isPanelVisible() {
            return
        }

        guard let project = selectedProject() else {
            return
        }

        let currentProjects = projects()
        let sessionID = effectiveSessionID(selectedSessionID())
        let decision = automaticRefreshDecision(
            projectID: project.id,
            state: cachedState(for: project.id),
            interval: backgroundRefreshInterval,
            forceOnFirstOpen: false
        )
        guard decision.shouldRefresh else {
            logger.log(
                "history-refresh",
                "skip trigger=background project=\(project.id.uuidString) reason=\(decision.reason)"
            )
            return
        }

        logger.log(
            "history-refresh",
            "queue trigger=background project=\(project.id.uuidString) reason=\(decision.reason)"
        )
        refresh(
            project: project,
            projects: currentProjects,
            selectedSessionID: sessionID,
            force: false,
            trigger: .background,
            updateCurrentSelectionContext: false
        )
    }

    func startRuntimeBridgeObserver() {
        if let runtimeBridgeObserver {
            NotificationCenter.default.removeObserver(runtimeBridgeObserver)
        }
        runtimeBridgeObserver = NotificationCenter.default.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self else {
                return
            }
            let kind = notification.userInfo?["kind"] as? String
            let reason: LiveRefreshReason = kind == "runtime-poll" ? .runtimePoll : .runtimeBridge
            Task { @MainActor [weak self] in
                self?.scheduleLiveRefresh(reason: reason)
            }
        }
    }

    func scheduleLiveRefresh(reason: LiveRefreshReason) {
        pendingLiveRefreshReason = mergedLiveRefreshReason(
            pendingLiveRefreshReason,
            reason
        )

        guard pendingLiveRefreshTask == nil else {
            return
        }

        pendingLiveRefreshTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(120))
            guard let self else {
                return
            }
            self.pendingLiveRefreshTask = nil
            let resolvedReason = self.pendingLiveRefreshReason ?? reason
            self.pendingLiveRefreshReason = nil

            guard let isPanelVisible = self.panelVisibilityProvider,
                  let selectedProject = self.selectedProjectProvider,
                  let selectedSessionID = self.selectedSessionIDProvider,
                  let projects = self.projectsProvider,
                  isPanelVisible(),
                  let project = selectedProject() else {
                return
            }

            let currentProjects = projects()
            let sessionID = self.effectiveSessionID(selectedSessionID())
            _ = self.ingestRuntime(project: project, projects: currentProjects, selectedSessionID: sessionID)
            self.refreshLiveState(
                project: project,
                selectedSessionID: sessionID,
                reason: resolvedReason
            )
        }
    }

    func mergedLiveRefreshReason(
        _ existing: LiveRefreshReason?,
        _ incoming: LiveRefreshReason
    ) -> LiveRefreshReason {
        switch (existing, incoming) {
        case (.runtimeBridge, _), (_, .runtimeBridge):
            return .runtimeBridge
        case (.runtimePoll, _), (_, .runtimePoll):
            return .runtimePoll
        case (.terminalFocus, _), (_, .terminalFocus):
            return .terminalFocus
        case (.none, _):
            return incoming
        }
    }

    func startTerminalFocusObserver(
        isPanelVisible: @escaping @MainActor () -> Bool,
        selectedProject: @escaping @MainActor () -> Project?,
        selectedSessionID: @escaping @MainActor () -> UUID?
    ) {
        if let terminalFocusObserver {
            NotificationCenter.default.removeObserver(terminalFocusObserver)
        }

        terminalFocusObserver = NotificationCenter.default.addObserver(
            forName: .dmuxTerminalFocusDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            guard let self else {
                return
            }
            Task { @MainActor in
                guard isPanelVisible(), let project = selectedProject() else {
                    return
                }
                let sessionID = self.effectiveSessionID(selectedSessionID())
                self.refreshLiveState(
                    project: project,
                    selectedSessionID: sessionID,
                    reason: .terminalFocus
                )
            }
        }
    }
}
