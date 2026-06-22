import 'package:flutter/material.dart';

import '../../models/remote_models.dart';
import '../../models/workspace_mode.dart';

/// Shared workspace data + actions bundle.
///
/// Bundles the workspace view-model (lists, selections, editor state) together
/// with the action callbacks so the home page wires it once and the workspace
/// shells (pad single-view, phone) consume a single object instead of threading
/// dozens of positional parameters.
class WorkspaceController {
  const WorkspaceController({
    required this.topInset,
    required this.workspaceMode,
    required this.onBack,
    required this.connected,
    required this.latencyMs,
    required this.deviceName,
    required this.projects,
    required this.selectedProjectId,
    required this.worktrees,
    required this.selectedWorktreeId,
    required this.terminals,
    required this.activeTerminalId,
    required this.aiStats,
    required this.aiStatsLoading,
    required this.gitStatus,
    required this.onGitAction,
    required this.onRefreshGit,
    required this.onSshUpsert,
    required this.onSshRemove,
    required this.aiSessions,
    required this.onOpenSession,
    required this.onRenameSession,
    required this.onDeleteSession,
    required this.sshProfiles,
    required this.gitDiff,
    required this.reviewSelectedPath,
    required this.onSelectReviewFile,
    required this.editingFilePath,
    required this.fileEditorController,
    required this.fileEditorLoading,
    required this.fileEditorSaving,
    required this.fileEditorEditing,
    required this.fileEditorEditable,
    required this.onEditFile,
    required this.onSaveFile,
    required this.onCancelFileEdit,
    required this.onCloseFileEditor,
    required this.projectFilesPath,
    required this.projectFilesParent,
    required this.projectFileEntries,
    required this.projectFilesLoading,
    required this.terminalBody,
    required this.onShowTerminal,
    required this.onShowStats,
    required this.onShowFiles,
    required this.onShowReview,
    required this.onShowSsh,
    required this.onShowGit,
    required this.onEditProject,
    required this.onAddProject,
    required this.onRemoveProject,
    required this.onSelectProject,
    required this.onSelectWorktree,
    required this.onCreateWorktree,
    required this.onMergeWorktree,
    required this.onDeleteWorktree,
    required this.onSelectTerminal,
    required this.onCreateTerminal,
    required this.onCloseTerminal,
    required this.onRequestProjectFiles,
    required this.onOpenProjectFile,
    required this.onOpenProjectHome,
    required this.onOpenProjectRoot,
    required this.onOpenProjectVolumes,
    required this.onRenameProjectFile,
    required this.onCopyProjectFilePath,
    required this.onDeleteProjectFile,
  });

  final double topInset;
  final WorkspaceMode workspaceMode;
  final VoidCallback onBack;
  final bool connected;
  final int? latencyMs;
  final String deviceName;
  final List<ProjectInfo> projects;
  final String? selectedProjectId;
  final List<RemoteWorktreeInfo> worktrees;
  final String? selectedWorktreeId;
  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final AIStatsInfo? aiStats;
  final bool aiStatsLoading;
  final RemoteGitStatusInfo? gitStatus;
  final void Function(String op, Map<String, dynamic> args) onGitAction;
  final VoidCallback onRefreshGit;
  final void Function(Map<String, dynamic> fields) onSshUpsert;
  final ValueChanged<String> onSshRemove;
  final List<AISessionRecord> aiSessions;
  final ValueChanged<AISessionRecord> onOpenSession;
  final ValueChanged<AISessionRecord> onRenameSession;
  final ValueChanged<AISessionRecord> onDeleteSession;
  final List<RemoteSshProfile> sshProfiles;
  final RemoteGitDiff? gitDiff;
  final String? reviewSelectedPath;
  final ValueChanged<String> onSelectReviewFile;
  final String? editingFilePath;
  final TextEditingController fileEditorController;
  final bool fileEditorLoading;
  final bool fileEditorSaving;
  final bool fileEditorEditing;
  final bool fileEditorEditable;
  final VoidCallback onEditFile;
  final VoidCallback onSaveFile;
  final VoidCallback onCancelFileEdit;
  final VoidCallback onCloseFileEditor;
  final String projectFilesPath;
  final String? projectFilesParent;
  final List<RemoteFileEntry> projectFileEntries;
  final bool projectFilesLoading;
  final Widget terminalBody;
  final VoidCallback onShowTerminal;
  final VoidCallback onShowStats;
  final VoidCallback onShowFiles;
  final VoidCallback onShowReview;
  final VoidCallback onShowSsh;
  final VoidCallback onShowGit;
  final VoidCallback onEditProject;
  final VoidCallback onAddProject;
  final VoidCallback onRemoveProject;
  final ValueChanged<ProjectInfo> onSelectProject;
  final ValueChanged<RemoteWorktreeInfo> onSelectWorktree;
  final VoidCallback onCreateWorktree;
  final ValueChanged<RemoteWorktreeInfo> onMergeWorktree;
  final ValueChanged<RemoteWorktreeInfo> onDeleteWorktree;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onCreateTerminal;
  final ValueChanged<TerminalInfo> onCloseTerminal;
  final ValueChanged<String> onRequestProjectFiles;
  final ValueChanged<RemoteFileEntry> onOpenProjectFile;
  final VoidCallback onOpenProjectHome;
  final VoidCallback onOpenProjectRoot;
  final VoidCallback onOpenProjectVolumes;
  final ValueChanged<RemoteFileEntry> onRenameProjectFile;
  final ValueChanged<RemoteFileEntry> onCopyProjectFilePath;
  final ValueChanged<RemoteFileEntry> onDeleteProjectFile;
}
