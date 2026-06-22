import 'package:flutter/material.dart';

import '../../models/remote_models.dart';
import '../../services/remote_protocol.dart';
import '../../services/log_service.dart';
import '../../services/remote_runtime_store.dart';
import '../../services/worktree_utils.dart';
import '../../widgets/components/worktree_create_dialog.dart';

class HomeWorktreeActions {
  const HomeWorktreeActions({
    required this.context,
    required this.t,
    required this.selectedProject,
    required this.selectedProjectId,
    required this.selectedWorktreeId,
    required this.terminalDataVisible,
    required this.terminalListLoaded,
    required this.preferredBaseBranch,
    required this.worktreeBaseBranches,
    required this.worktreesForProject,
    required this.showToast,
    required this.flushTerminalInput,
    required this.closeTerminalSwitcher,
    required this.markPendingSwitch,
    required this.clearPendingSwitch,
    required this.setWorktreeListLoading,
    required this.setCreatingWorktree,
    required this.syncRuntimeViewState,
    required this.showTerminalWorkspace,
    required this.sendEnvelope,
    required this.applyRuntimePlan,
    required this.runtime,
    required this.confirmAction,
    required this.worktreeTitle,
    required this.selectEnvelope,
    required this.createEnvelope,
    required this.deleteEnvelope,
    required this.mergeEnvelope,
  });

  final BuildContext context;
  final String Function(String key, {Map<String, String>? params}) t;
  final ProjectInfo? selectedProject;
  final String? selectedProjectId;
  final String? selectedWorktreeId;
  final bool terminalDataVisible;
  final bool terminalListLoaded;
  final String preferredBaseBranch;
  final List<String> worktreeBaseBranches;
  final List<RemoteWorktreeInfo> Function(String projectId) worktreesForProject;
  final void Function(String message) showToast;
  final VoidCallback flushTerminalInput;
  final VoidCallback closeTerminalSwitcher;
  final void Function(String projectId, String worktreeId) markPendingSwitch;
  final VoidCallback clearPendingSwitch;
  final void Function(bool loading) setWorktreeListLoading;
  final void Function(bool creating) setCreatingWorktree;
  final VoidCallback syncRuntimeViewState;
  final VoidCallback showTerminalWorkspace;
  final bool Function(RelayEnvelope envelope) sendEnvelope;
  final void Function(RemoteRuntimePlan plan, {required String reason})
      applyRuntimePlan;
  final RemoteRuntimeStore runtime;
  final Future<bool> Function({
    required String title,
    required String message,
    required bool destructive,
  }) confirmAction;
  final String Function(RemoteWorktreeInfo worktree) worktreeTitle;
  final RelayEnvelope Function(ProjectInfo project, RemoteWorktreeInfo worktree)
      selectEnvelope;
  final RelayEnvelope Function({
    required ProjectInfo project,
    required String baseBranch,
    required String name,
  }) createEnvelope;
  final RelayEnvelope Function(ProjectInfo project, RemoteWorktreeInfo worktree)
      deleteEnvelope;
  final RelayEnvelope Function(ProjectInfo project, RemoteWorktreeInfo worktree)
      mergeEnvelope;

  void selectWorktree(RemoteWorktreeInfo worktree, {bool force = false}) {
    final project = selectedProject;
    if (project == null) {
      showToast(t('project.selectFirst'));
      return;
    }
    if (worktree.projectId != project.id) {
      CoduxLog.warn(
        '[codux-flutter-worktree] ignore select project=${project.id} worktree=${worktree.id} worktreeProject=${worktree.projectId}',
      );
      return;
    }
    // `force` (used right after creating a worktree) re-runs the bind even when
    // the runtime already shows it selected, so the host actually serves the new
    // worktree's terminal instead of leaving a blank screen.
    if (!force && worktree.id == selectedWorktreeId) {
      closeTerminalSwitcher();
      return;
    }
    flushTerminalInput();
    markPendingSwitch(project.id, worktree.id);
    final plan = runtime.worktreeSelected(
      projectId: project.id,
      worktreeId: worktree.id,
      terminalVisible: terminalDataVisible,
      terminalListLoaded: terminalListLoaded,
    );
    showTerminalWorkspace();
    syncRuntimeViewState();
    setWorktreeListLoading(true);
    final sent = sendEnvelope(selectEnvelope(project, worktree));
    if (!sent) {
      clearPendingSwitch();
      setWorktreeListLoading(false);
      return;
    }
    applyRuntimePlan(plan, reason: 'worktree-local-select');
  }

  Future<void> createWorktree() async {
    final project = selectedProject;
    if (project == null || project.path == null || project.path!.isEmpty) {
      showToast(t('project.selectPathFirst'));
      return;
    }
    final branchOptions = worktreeBranchOptions(
      defaultBaseBranch: preferredBaseBranch,
      baseBranches: worktreeBaseBranches,
      worktrees: selectedProjectId == null
          ? const []
          : worktreesForProject(selectedProjectId!),
    );
    final request = await showDialog<WorktreeCreateDraft>(
      context: context,
      builder: (ctx) => WorktreeCreateDialog(
        title: t('worktree.new'),
        baseBranchLabel: t('worktree.baseBranch'),
        nameLabel: t('worktree.name'),
        cancelLabel: t('app.cancel'),
        createLabel: t('common.create'),
        branchOptions: branchOptions,
        initialBaseBranch: defaultWorktreeBaseBranch(
          preferred: preferredBaseBranch,
          options: branchOptions,
        ),
        initialName: defaultWorktreeName(),
      ),
    );
    if (request == null) return;
    if (request.baseBranch.isEmpty) {
      showToast(t('worktree.baseBranchRequired'));
      return;
    }
    if (request.name.isEmpty) {
      showToast(t('worktree.nameRequired'));
      return;
    }
    setWorktreeListLoading(true);
    setCreatingWorktree(true);
    sendEnvelope(
      createEnvelope(
        project: project,
        baseBranch: request.baseBranch,
        name: request.name,
      ),
    );
  }

  Future<void> mergeWorktree(RemoteWorktreeInfo worktree) async {
    final confirmed = await confirmAction(
      title: t('worktree.merge'),
      message: t('worktree.mergeConfirm', params: {'name': worktreeTitle(worktree)}),
      destructive: false,
    );
    if (!confirmed) return;
    sendWorktreeOperation(RemoteMessageType.worktreeMerge, worktree);
  }

  Future<void> deleteWorktree(RemoteWorktreeInfo worktree) async {
    final confirmed = await confirmAction(
      title: t('worktree.delete'),
      message: t('worktree.deleteConfirm', params: {'name': worktreeTitle(worktree)}),
      destructive: true,
    );
    if (!confirmed) return;
    sendWorktreeOperation(RemoteMessageType.worktreeDelete, worktree);
  }

  void sendWorktreeOperation(String type, RemoteWorktreeInfo worktree) {
    final project = selectedProject;
    if (project == null || project.path == null || project.path!.isEmpty) {
      showToast(t('project.selectPathFirst'));
      return;
    }
    setWorktreeListLoading(true);
    final envelope = type == RemoteMessageType.worktreeDelete
        ? deleteEnvelope(project, worktree)
        : mergeEnvelope(project, worktree);
    sendEnvelope(envelope);
  }
}
