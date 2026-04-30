import Foundation

extension AppModel {
  func remoteProjects() -> [[String: String]] {
    projects.map { project in
      [
        "id": project.id.uuidString,
        "name": project.name,
        "path": project.path,
      ]
    }
  }

  func remoteTerminals() -> [[String: String]] {
    []
  }

  func remoteCreateTerminal(projectID: String?, command: String) -> TerminalSession? {
    let resolvedProject: Project?
    if let projectID, let uuid = UUID(uuidString: projectID) {
      resolvedProject = projects.first { $0.id == uuid }
    } else if let selectedProjectID {
      resolvedProject = projects.first { $0.id == selectedProjectID }
    } else {
      resolvedProject = projects.first
    }
    guard let project = resolvedProject else {
      return nil
    }
    return TerminalSession.make(project: project, command: command)
  }

  func remoteAddProject(path: String, name: String?) -> Project? {
    let normalizedPath = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.path
    if let existing = projects.first(where: {
      URL(fileURLWithPath: $0.path).standardizedFileURL.path == normalizedPath
    }) {
      updateSelectedProjectID(existing.id, source: "remoteAddProject.existing")
      return existing
    }
    let project = Project(
      id: UUID(),
      name: remoteProjectName(name: name, path: normalizedPath),
      path: normalizedPath,
      shell: appSettings.defaultTerminal.shellPath,
      defaultCommand: "",
      badgeText: nil,
      badgeSymbol: nil,
      badgeColorHex: nil,
      gitDefaultPushRemoteName: nil
    )
    projects.append(project)
    workspaces.append(ProjectWorkspace.sample(projectID: project.id, path: project.path))
    updateSelectedProjectID(project.id, source: "remoteAddProject.created")
    persist()
    refreshGitState()
    updateGitRemoteSyncPolling()
    refreshAIStatsIfNeeded()
    return project
  }

  func remoteEditProject(projectID: String, path: String, name: String?) -> Project? {
    guard let uuid = UUID(uuidString: projectID),
      let index = projects.firstIndex(where: { $0.id == uuid })
    else { return nil }
    let current = projects[index]
    let normalizedPath = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.path
    let updated = Project(
      id: current.id,
      name: remoteProjectName(name: name, path: normalizedPath),
      path: normalizedPath,
      shell: current.shell,
      defaultCommand: current.defaultCommand,
      badgeText: current.badgeText,
      badgeSymbol: current.badgeSymbol,
      badgeColorHex: current.badgeColorHex,
      gitDefaultPushRemoteName: current.gitDefaultPushRemoteName
    )
    projects[index] = updated
    if let workspaceIndex = workspaces.firstIndex(where: { $0.projectID == uuid }) {
      for sessionIndex in workspaces[workspaceIndex].sessions.indices {
        workspaces[workspaceIndex].sessions[sessionIndex].projectName = updated.name
        workspaces[workspaceIndex].sessions[sessionIndex].cwd = updated.path
      }
    }
    updateSelectedProjectID(updated.id, source: "remoteEditProject")
    persist()
    refreshGitState()
    updateGitRemoteSyncPolling()
    refreshAIStatsIfNeeded()
    return updated
  }

  func remoteRemoveProject(projectID: String) -> Bool {
    guard let uuid = UUID(uuidString: projectID),
      projects.contains(where: { $0.id == uuid })
    else { return false }
    petStore.forgetProjectBaseline(uuid)
    projects.removeAll { $0.id == uuid }
    workspaces.removeAll { $0.projectID == uuid }
    if selectedProjectID == uuid {
      updateSelectedProjectID(projects.first?.id, source: "remoteRemoveProject")
    }
    persist()
    refreshGitState()
    updateGitRemoteSyncPolling()
    refreshAIStatsIfNeeded()
    return true
  }

  func remoteAIStats(projectID: String) -> [String: Any]? {
    guard let uuid = UUID(uuidString: projectID),
      let project = projects.first(where: { $0.id == uuid })
    else { return nil }
    let liveContext = aiStatsStore.liveSnapshotContext(
      projectID: uuid, selectedSessionID: selectedSessionID)
    let state =
      aiStatsStore.cachedState(for: uuid)
      ?? aiStatsStore.aiUsageService.snapshotBackedPanelState(
        project: project,
        liveSnapshots: liveContext.summary,
        currentSnapshot: liveContext.current,
        status: .idle
      )
    let summary = state.projectSummary
    let requestCount = state.sessions.reduce(0) { $0 + $1.requestCount }
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let liveSessions = state.liveSnapshots.map { snapshot -> [String: Any] in
      var item: [String: Any] = [
        "sessionId": snapshot.sessionID.uuidString,
        "title": snapshot.sessionTitle,
        "projectId": snapshot.projectID.uuidString,
        "projectName": snapshot.projectName,
        "status": snapshot.status,
        "isRunning": snapshot.isRunning,
        "inputTokens": snapshot.currentInputTokens,
        "outputTokens": snapshot.currentOutputTokens,
        "totalTokens": snapshot.currentTotalTokens,
        "cachedInputTokens": snapshot.currentCachedInputTokens,
        "updatedAt": formatter.string(from: snapshot.updatedAt),
      ]
      if let externalSessionID = snapshot.externalSessionID {
        item["externalSessionId"] = externalSessionID
      }
      if let tool = snapshot.tool { item["tool"] = tool }
      if let model = snapshot.model { item["model"] = model }
      if let usage = snapshot.currentContextUsagePercent { item["contextUsagePercent"] = usage }
      return item
    }
    let todayTimeBuckets = state.todayTimeBuckets.map { bucket -> [String: Any] in
      [
        "start": formatter.string(from: bucket.start),
        "end": formatter.string(from: bucket.end),
        "totalTokens": bucket.totalTokens,
        "cachedInputTokens": bucket.cachedInputTokens,
        "requestCount": bucket.requestCount,
      ]
    }
    let heatmap = state.heatmap.map { day -> [String: Any] in
      [
        "day": formatter.string(from: day.day),
        "totalTokens": day.totalTokens,
        "cachedInputTokens": day.cachedInputTokens,
        "requestCount": day.requestCount,
      ]
    }
    let toolBreakdown = state.toolBreakdown.map { item -> [String: Any] in
      [
        "key": item.key,
        "totalTokens": item.totalTokens,
        "cachedInputTokens": item.cachedInputTokens,
        "requestCount": item.requestCount,
      ]
    }
    let modelBreakdown = state.modelBreakdown.map { item -> [String: Any] in
      [
        "key": item.key,
        "totalTokens": item.totalTokens,
        "cachedInputTokens": item.cachedInputTokens,
        "requestCount": item.requestCount,
      ]
    }
    var payload: [String: Any] = [
      "projectId": project.id.uuidString,
      "projectName": summary?.projectName ?? project.name,
      "todayTokens": summary?.todayTotalTokens ?? 0,
      "totalTokens": summary?.projectTotalTokens ?? 0,
      "currentSessionTokens": summary?.currentSessionTokens ?? 0,
      "currentSessionCachedInputTokens": summary?.currentSessionCachedInputTokens ?? 0,
      "projectTotalTokens": summary?.projectTotalTokens ?? 0,
      "projectCachedInputTokens": summary?.projectCachedInputTokens ?? 0,
      "todayTotalTokens": summary?.todayTotalTokens ?? 0,
      "todayCachedInputTokens": summary?.todayCachedInputTokens ?? 0,
      "requestCount": requestCount,
      "currentSessions": liveSessions,
      "todayTimeBuckets": todayTimeBuckets,
      "heatmap": heatmap,
      "toolBreakdown": toolBreakdown,
      "modelBreakdown": modelBreakdown,
    ]
    if let currentTool = summary?.currentTool { payload["currentTool"] = currentTool }
    if let currentModel = summary?.currentModel { payload["currentModel"] = currentModel }
    if let contextUsagePercent = summary?.currentContextUsagePercent {
      payload["contextUsagePercent"] = contextUsagePercent
    }
    if let updatedAt = summary?.currentSessionUpdatedAt {
      payload["updatedAt"] = ISO8601DateFormatter().string(from: updatedAt)
    }
    return payload
  }

  private func remoteProjectName(name: String?, path: String) -> String {
    let trimmed = name?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if trimmed.isEmpty == false { return trimmed }
    let fallback = URL(fileURLWithPath: path).lastPathComponent
    return fallback.isEmpty ? "Project" : fallback
  }

  func remoteCloseTerminal(sessionID: UUID) -> Bool {
    guard let workspaceIndex = workspaces.firstIndex(where: { $0.containsSession(sessionID) })
    else {
      return false
    }
    noteTerminalLoadingState(sessionID, isLoading: false)
    var updatedWorkspaces = workspaces
    updatedWorkspaces[workspaceIndex].removeSession(sessionID)
    workspaces = updatedWorkspaces
    runtimeIngressService.clearLiveState(sessionID: sessionID)
    aiStatsStore.handleTerminalSessionClosed(
      sessionID: sessionID,
      project: selectedProject,
      projects: projects,
      selectedSessionID: selectedSessionID
    )
    DmuxTerminalBackend.shared.registry.release(sessionID: sessionID)
    clearTerminalRecoveryState(for: sessionID)
    persist()
    refreshAIStatsIfNeeded()
    return true
  }
}
