import 'package:flutter/foundation.dart';

import '../../models/remote_models.dart';
import '../../models/workspace_mode.dart';
import '../../services/remote_project_controller.dart';
import '../../services/remote_project_file_controller.dart';
import '../../services/remote_protocol.dart';
import '../../services/remote_resource_subscription_coordinator.dart';
import '../../services/remote_runtime_store.dart';

class HomeWorkspaceModeActions {
  const HomeWorkspaceModeActions({
    required this.remoteProtocolReady,
    required this.workspaceMode,
    required this.terminalDataVisible,
    required this.terminalListLoaded,
    required this.selectedProject,
    required this.selectedWorktreeId,
    required this.projectFilesPath,
    required this.releaseTerminalViewport,
    required this.showToast,
    required this.setModeState,
    required this.setProjectFilesState,
    required this.focusTerminalViewSoon,
    required this.mountVisibleTerminal,
    required this.sendEnvelope,
    required this.applyRuntimePlan,
    required this.runtime,
    required this.projectController,
    required this.projectFileController,
    required this.resourceSubscriptions,
  });

  final bool remoteProtocolReady;
  final WorkspaceMode workspaceMode;
  final bool terminalDataVisible;
  final bool terminalListLoaded;
  final ProjectInfo? selectedProject;
  final String? selectedWorktreeId;
  final String projectFilesPath;
  final VoidCallback releaseTerminalViewport;
  final void Function(String message) showToast;
  final void Function(
    WorkspaceMode mode, {
    bool terminalReady,
    bool aiStatsLoading,
  })
  setModeState;
  final void Function(String path, {required bool loading})
  setProjectFilesState;
  final VoidCallback focusTerminalViewSoon;
  final void Function({required String reason}) mountVisibleTerminal;
  final bool Function(RelayEnvelope envelope) sendEnvelope;
  final void Function(RemoteRuntimePlan plan, {required String reason})
  applyRuntimePlan;
  final RemoteRuntimeStore runtime;
  final RemoteProjectController projectController;
  final RemoteProjectFileController projectFileController;
  final RemoteResourceSubscriptionCoordinator resourceSubscriptions;

  void requestAIStats(String selectProjectMessage) {
    final project = selectedProject;
    if (project == null) {
      showToast(selectProjectMessage);
      return;
    }
    if (workspaceMode == WorkspaceMode.terminal) {
      releaseTerminalViewport();
    }
    setModeState(WorkspaceMode.stats, aiStatsLoading: true);
    final fallback = projectController.aiStatsEnvelope(
      project,
      worktreeId: selectedWorktreeId,
    );
    resourceSubscriptions.requestProject(
      resource: RemoteResourceType.aiStats,
      projectId: project.id,
      fallback: fallback,
      extraPayload: {
        'worktreeId': selectedWorktreeId,
        'projectPath': project.path,
      },
    );
  }

  void refreshAIStats() {
    final project = selectedProject;
    if (!remoteProtocolReady || project == null) return;
    final fallback = projectController.aiStatsEnvelope(
      project,
      worktreeId: selectedWorktreeId,
    );
    resourceSubscriptions.requestProject(
      resource: RemoteResourceType.aiStats,
      projectId: project.id,
      fallback: fallback,
      extraPayload: {
        'worktreeId': selectedWorktreeId,
        'projectPath': project.path,
      },
    );
  }

  void requestGitStatus() {
    final project = selectedProject;
    if (!remoteProtocolReady || project == null) return;
    resourceSubscriptions.requestProject(
      resource: RemoteResourceType.gitStatus,
      projectId: project.id,
      fallback: projectController.gitStatusEnvelope(project),
      extraPayload: {'projectPath': project.path},
    );
  }

  /// Run a git mutation (stage/unstage/discard/commit/push/...). The host
  /// replies with a refreshed git.status, so the panel updates automatically.
  void gitInvoke(String op, {Map<String, dynamic> args = const {}}) {
    final project = selectedProject;
    if (!remoteProtocolReady || project == null) return;
    sendEnvelope(projectController.gitInvokeEnvelope(project, op, args: args));
  }

  /// Request the unified diff for one path (review/diff view).
  void requestGitDiff(String path, {String? baseBranch}) {
    final project = selectedProject;
    if (!remoteProtocolReady || project == null) return;
    sendEnvelope(
      projectController.gitReadEnvelope(
        project,
        'diff',
        args: {'filePath': path, 'baseBranch': ?baseBranch},
      ),
    );
  }

  void syncTerminalToSelectedProject({bool requestListIfMissing = true}) {
    if (!terminalDataVisible) return;
    final plan = runtime.ensureTerminalForSelectedProject(
      terminalVisible: terminalDataVisible,
      terminalListLoaded: requestListIfMissing && terminalListLoaded,
    );
    applyRuntimePlan(plan, reason: 'missing-terminal');
  }

  void showTerminalMode() {
    setModeState(WorkspaceMode.terminal, terminalReady: false);
    syncTerminalToSelectedProject();
    mountVisibleTerminal(reason: 'mode');
    requestGitStatus();
    focusTerminalViewSoon();
  }

  void showFilesMode(String selectProjectMessage, String currentNoDirMessage) {
    final project = selectedProject;
    if (project == null) {
      showToast(selectProjectMessage);
      return;
    }
    final targetPath = projectFileController.pathForProject(
      project,
      currentPath: projectFilesPath,
    );
    if (workspaceMode == WorkspaceMode.terminal) {
      releaseTerminalViewport();
    }
    setModeState(WorkspaceMode.files);
    requestGitStatus();
    requestProjectFiles(currentNoDirMessage, path: targetPath);
  }

  void requestProjectFiles(String currentNoDirMessage, {String? path}) {
    final project = selectedProject;
    final target = path ?? project?.path;
    if (target == null || target.isEmpty) {
      showToast(currentNoDirMessage);
      return;
    }
    setProjectFilesState(target, loading: true);
    if (project != null) {
      projectFileController.remember(projectId: project.id, path: target);
    }
    sendEnvelope(projectFileController.listEnvelope(target));
  }
}
