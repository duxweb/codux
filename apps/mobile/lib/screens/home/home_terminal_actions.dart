import 'package:flutter/material.dart';

import '../../models/remote_models.dart';
import '../../models/workspace_mode.dart';
import '../../services/remote_runtime_store.dart';
import '../../services/terminal_input_batcher.dart';

class HomeTerminalActions {
  const HomeTerminalActions({
    required this.context,
    required this.t,
    required this.mounted,
    required this.selectedProjectId,
    required this.workspaceMode,
    required this.showToast,
    required this.showTerminalWorkspace,
    required this.focusTerminalViewSoon,
    required this.releaseTerminalViewport,
    required this.ensureSelectedProjectWorktrees,
    required this.pushTerminalSwitcher,
    required this.hideTerminalSwitcher,
    required this.mountVisibleTerminal,
    required this.currentTerminal,
    required this.currentProjectTerminals,
    required this.isAccessibleTerminal,
    required this.runtime,
    required this.inputBatcher,
    required this.applyRuntimePlan,
    required this.sendTerminalClose,
    required this.setTerminalReady,
  });

  final BuildContext context;
  final String Function(String key, {Map<String, String>? params}) t;
  final bool mounted;
  final String? selectedProjectId;
  final WorkspaceMode workspaceMode;
  final void Function(String message) showToast;
  final VoidCallback showTerminalWorkspace;
  final VoidCallback focusTerminalViewSoon;
  final VoidCallback releaseTerminalViewport;
  final void Function({bool loading}) ensureSelectedProjectWorktrees;
  final Future<void> Function(VoidCallback mutate) pushTerminalSwitcher;
  final VoidCallback hideTerminalSwitcher;
  final void Function({required String reason}) mountVisibleTerminal;
  final TerminalInfo? Function() currentTerminal;
  final List<TerminalInfo> Function() currentProjectTerminals;
  final bool Function(TerminalInfo? terminal) isAccessibleTerminal;
  final RemoteRuntimeStore runtime;
  final TerminalInputBatcher inputBatcher;
  final void Function(RemoteRuntimePlan plan, {required String reason})
      applyRuntimePlan;
  final void Function(TerminalInfo terminal) sendTerminalClose;
  final void Function(bool ready) setTerminalReady;

  void selectTerminal(TerminalInfo terminal) {
    if (!isAccessibleTerminal(terminal)) return;
    inputBatcher.flush();
    showTerminalWorkspace();
    final plan = runtime.selectTerminal(terminal);
    applyRuntimePlan(plan, reason: 'select-terminal');
    focusTerminalViewSoon();
  }

  void createTerminalForSelectedProject(
    void Function(String projectId, [String layoutKind]) createTerminal, {
    String? layoutKind,
  }) {
    final projectId = requireSelectedProjectId();
    if (projectId == null) return;
    showTerminalWorkspace();
    if (layoutKind == null) {
      createTerminal(projectId);
    } else {
      createTerminal(projectId, layoutKind);
    }
  }

  String? requireSelectedProjectId() {
    final projectId = selectedProjectId;
    if (projectId != null) return projectId;
    showToast(t('project.selectFirst'));
    return null;
  }

  void closeCurrentTerminal() {
    final terminal = currentTerminal();
    if (terminal == null || !isAccessibleTerminal(terminal)) return;
    closeTerminal(terminal);
  }

  void closeTerminal(TerminalInfo terminal) {
    if (!isAccessibleTerminal(terminal)) return;
    if (isLastRemainingTerminal(terminal)) {
      showToast(t('terminal.keepOne'));
      return;
    }
    final plan = runtime.removeTerminal(terminal.id);
    applyRuntimePlan(plan, reason: 'close-terminal');
    sendTerminalClose(terminal);
  }

  bool isLastRemainingTerminal(TerminalInfo terminal) {
    final scopedTerminals = currentProjectTerminals();
    return scopedTerminals.length <= 1 &&
        scopedTerminals.any((item) => item.id == terminal.id);
  }

  Future<void> openTerminalSwitcher(bool alreadyShown) async {
    if (alreadyShown) return;
    if (workspaceMode == WorkspaceMode.terminal) {
      releaseTerminalViewport();
    }
    ensureSelectedProjectWorktrees(loading: true);
    await pushTerminalSwitcher(() {
      setTerminalReady(false);
    });
  }

  void closeTerminalSwitcher(Future<void> Function(VoidCallback mutate) popPage) {
    popPage(hideTerminalSwitcher).then((_) {
      if (mounted) mountVisibleTerminal(reason: 'switcher-close');
    });
  }
}
