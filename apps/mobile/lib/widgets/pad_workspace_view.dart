import 'package:flutter/material.dart';

import '../models/remote_models.dart';
import 'workspace/pad/pad_theme.dart';
import 'workspace/pad/pad_right_column.dart';
import 'workspace/pad/pad_top_bar.dart';
import 'workspace/pad/pad_workspace_main_pane.dart';
import 'workspace/pad/pad_workspace_shared.dart';
import 'workspace/pad/pad_workspace_sidebar.dart';
import 'workspace/workspace_controller.dart';

class PadWorkspaceView extends StatelessWidget {
  const PadWorkspaceView({super.key, required this.controller});

  final WorkspaceController controller;

  // Delegating getters keep the build() body referencing bare names while the
  // workspace state + actions live in a single shared controller object.
  double get topInset => controller.topInset;
  String get workspaceMode => controller.workspaceMode;
  bool get connected => controller.connected;
  int? get latencyMs => controller.latencyMs;
  String get deviceName => controller.deviceName;
  List<ProjectInfo> get projects => controller.projects;
  String? get selectedProjectId => controller.selectedProjectId;
  List<RemoteWorktreeInfo> get worktrees => controller.worktrees;
  String? get selectedWorktreeId => controller.selectedWorktreeId;
  List<TerminalInfo> get terminals => controller.terminals;
  String? get activeTerminalId => controller.activeTerminalId;
  AIStatsInfo? get aiStats => controller.aiStats;
  bool get aiStatsLoading => controller.aiStatsLoading;
  RemoteGitStatusInfo? get gitStatus => controller.gitStatus;
  void Function(String op, Map<String, dynamic> args) get onGitAction =>
      controller.onGitAction;
  VoidCallback get onRefreshGit => controller.onRefreshGit;
  List<AISessionRecord> get aiSessions => controller.aiSessions;
  List<RemoteSshProfile> get sshProfiles => controller.sshProfiles;
  RemoteGitDiff? get gitDiff => controller.gitDiff;
  String? get reviewSelectedPath => controller.reviewSelectedPath;
  ValueChanged<String> get onSelectReviewFile => controller.onSelectReviewFile;
  String? get editingFilePath => controller.editingFilePath;
  TextEditingController get fileEditorController =>
      controller.fileEditorController;
  bool get fileEditorLoading => controller.fileEditorLoading;
  bool get fileEditorSaving => controller.fileEditorSaving;
  bool get fileEditorEditing => controller.fileEditorEditing;
  bool get fileEditorEditable => controller.fileEditorEditable;
  VoidCallback get onEditFile => controller.onEditFile;
  VoidCallback get onSaveFile => controller.onSaveFile;
  VoidCallback get onCancelFileEdit => controller.onCancelFileEdit;
  VoidCallback get onCloseFileEditor => controller.onCloseFileEditor;
  String get projectFilesPath => controller.projectFilesPath;
  String? get projectFilesParent => controller.projectFilesParent;
  List<RemoteFileEntry> get projectFileEntries => controller.projectFileEntries;
  bool get projectFilesLoading => controller.projectFilesLoading;
  Widget get terminalBody => controller.terminalBody;
  VoidCallback get onShowTerminal => controller.onShowTerminal;
  VoidCallback get onShowStats => controller.onShowStats;
  VoidCallback get onShowFiles => controller.onShowFiles;
  VoidCallback get onShowReview => controller.onShowReview;
  VoidCallback get onShowSsh => controller.onShowSsh;
  VoidCallback get onShowGit => controller.onShowGit;
  VoidCallback get onEditProject => controller.onEditProject;
  VoidCallback get onAddProject => controller.onAddProject;
  VoidCallback get onRemoveProject => controller.onRemoveProject;
  ValueChanged<ProjectInfo> get onSelectProject => controller.onSelectProject;
  ValueChanged<RemoteWorktreeInfo> get onSelectWorktree =>
      controller.onSelectWorktree;
  VoidCallback get onCreateWorktree => controller.onCreateWorktree;
  ValueChanged<TerminalInfo> get onSelectTerminal =>
      controller.onSelectTerminal;
  VoidCallback get onCreateTerminal => controller.onCreateTerminal;
  ValueChanged<TerminalInfo> get onCloseTerminal => controller.onCloseTerminal;
  ValueChanged<TerminalInfo> get onRenameSession => controller.onRenameSession;
  ValueChanged<String> get onRequestProjectFiles =>
      controller.onRequestProjectFiles;
  ValueChanged<RemoteFileEntry> get onOpenProjectFile =>
      controller.onOpenProjectFile;
  VoidCallback get onOpenProjectHome => controller.onOpenProjectHome;
  VoidCallback get onOpenProjectRoot => controller.onOpenProjectRoot;
  VoidCallback get onOpenProjectVolumes => controller.onOpenProjectVolumes;
  ValueChanged<RemoteFileEntry> get onRenameProjectFile =>
      controller.onRenameProjectFile;
  ValueChanged<RemoteFileEntry> get onCopyProjectFilePath =>
      controller.onCopyProjectFilePath;
  ValueChanged<RemoteFileEntry> get onDeleteProjectFile =>
      controller.onDeleteProjectFile;

  Widget _sidebarBottomBar() {
    final ok = connected && latencyMs != null;
    final color = ok ? PadColors.success : PadColors.textSubtle;
    return SizedBox(
      height: 40,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 6),
        child: Row(
          children: [
            Expanded(
              child: Text(
                deviceName.isEmpty ? '—' : deviceName,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: PadColors.textSecondary,
                  fontSize: 12.5,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 7,
                  height: 7,
                  decoration: BoxDecoration(
                    color: color,
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: 6),
                Text(
                  ok ? '${latencyMs}ms' : '-- ms',
                  style: const TextStyle(
                    color: PadColors.textSecondary,
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    // "文件" is a center view (editor + browser sidebar); review shows the diff;
    // stats/ssh/git are right-column-only and keep the terminal centered. The
    // open file persists in state across view switches, so returning to "文件"
    // shows it again.
    final primaryWorkspaceMode = switch (workspaceMode) {
      'files' || 'review' => workspaceMode,
      _ => 'terminal',
    };
    final showRightColumn =
        workspaceMode == 'files' ||
        workspaceMode == 'stats' ||
        workspaceMode == 'review' ||
        workspaceMode == 'ssh' ||
        workspaceMode == 'git';
    return ColoredBox(
      color: PadColors.bg,
      child: Padding(
        padding: EdgeInsets.only(top: topInset),
        child: Column(
          children: [
            PadTopBar(
              workspaceMode: primaryWorkspaceMode,
              toolMode: workspaceMode,
              onShowTerminal: onShowTerminal,
              onShowStats: onShowStats,
              onShowFiles: onShowFiles,
              onShowReview: onShowReview,
              onShowSsh: onShowSsh,
              onShowGit: onShowGit,
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(12, 2, 12, 12),
                child: Row(
                  children: [
                    SizedBox(
                      width: PadMetrics.leftColumnWidth,
                      child: Column(
                        children: [
                          Expanded(
                            child: PadPanelSurface(
                              child: PadWorkspaceSidebar(
                                project: selectedProjectOf(
                                  projects,
                                  selectedProjectId,
                                ),
                                projects: projects,
                                selectedProjectId: selectedProjectId,
                                worktrees: worktrees,
                                selectedWorktreeId: selectedWorktreeId,
                                terminals: terminals,
                                activeTerminalId: activeTerminalId,
                                aiSessions: aiSessions,
                                onSelectProject: onSelectProject,
                                onEditProject: onEditProject,
                                onAddProject: onAddProject,
                                onRemoveProject: onRemoveProject,
                                onSelectWorktree: onSelectWorktree,
                                onCreateWorktree: onCreateWorktree,
                                onSelectTerminal: onSelectTerminal,
                                onCreateTerminal: onCreateTerminal,
                                onCloseTerminal: onCloseTerminal,
                                onRenameSession: onRenameSession,
                              ),
                            ),
                          ),
                          _sidebarBottomBar(),
                        ],
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: PadPanelSurface(
                        child: PadWorkspaceMainPane(
                          terminals: terminals,
                          activeTerminalId: activeTerminalId,
                          workspaceMode: primaryWorkspaceMode,
                          terminalBody: terminalBody,
                          gitDiff: gitDiff,
                          reviewSelectedPath: reviewSelectedPath,
                          editingFilePath: editingFilePath,
                          fileEditorController: fileEditorController,
                          fileEditorLoading: fileEditorLoading,
                          fileEditorSaving: fileEditorSaving,
                          fileEditorEditing: fileEditorEditing,
                          fileEditorEditable: fileEditorEditable,
                          onEditFile: onEditFile,
                          onSaveFile: onSaveFile,
                          onCancelFileEdit: onCancelFileEdit,
                          onCloseFileEditor: onCloseFileEditor,
                          onSelectTerminal: onSelectTerminal,
                          onCreateTerminal: onCreateTerminal,
                          onCloseTerminal: onCloseTerminal,
                        ),
                      ),
                    ),
                    if (showRightColumn) ...[
                      const SizedBox(width: 12),
                      PadRightColumn(
                        mode: workspaceMode,
                        aiStats: aiStats,
                        aiStatsLoading: aiStatsLoading,
                        onShowStats: onShowStats,
                        gitStatus: gitStatus,
                        onGitAction: onGitAction,
                        onRefreshGit: onRefreshGit,
                        sshProfiles: sshProfiles,
                        reviewSelectedPath: reviewSelectedPath,
                        onSelectReviewFile: onSelectReviewFile,
                        projectFilesPath: projectFilesPath,
                        projectFilesParent: projectFilesParent,
                        projectFileEntries: projectFileEntries,
                        projectFilesLoading: projectFilesLoading,
                        onRequestProjectFiles: onRequestProjectFiles,
                        onOpenProjectFile: onOpenProjectFile,
                        onOpenProjectHome: onOpenProjectHome,
                        onOpenProjectRoot: onOpenProjectRoot,
                        onOpenProjectVolumes: onOpenProjectVolumes,
                        onRenameProjectFile: onRenameProjectFile,
                        onCopyProjectFilePath: onCopyProjectFilePath,
                        onDeleteProjectFile: onDeleteProjectFile,
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
