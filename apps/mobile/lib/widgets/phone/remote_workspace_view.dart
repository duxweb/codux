import 'package:flutter/material.dart';

import '../../models/remote_models.dart';
import '../../models/workspace_mode.dart';
import '../components/ai_stats_panel.dart';
import '../components/project_files_panel.dart';
import 'project_tab_bar.dart';
import 'terminal_header.dart';

class RemoteWorkspaceView extends StatelessWidget {
  const RemoteWorkspaceView({
    super.key,
    required this.topInset,
    required this.workspaceMode,
    required this.connected,
    required this.latencyMs,
    required this.projects,
    required this.selectedProjectId,
    required this.projectListLoaded,
    required this.terminals,
    required this.activeTerminalId,
    required this.hasCurrentTerminal,
    required this.aiStats,
    required this.aiStatsLoading,
    required this.projectFilesPath,
    required this.projectFilesParent,
    required this.projectFileEntries,
    required this.projectFilesLoading,
    required this.terminalBody,
    required this.onShowTerminal,
    required this.onShowStats,
    required this.onShowFiles,
    required this.onBack,
    required this.onEditProject,
    required this.onAddProject,
    required this.onRemoveProject,
    required this.onSelectProject,
    required this.onSelectTerminal,
    required this.onRefreshLists,
    required this.onCreateTerminal,
    required this.onCloseCurrentTerminal,
    required this.onRebuildTerminal,
    required this.onOpenTerminalSwitcher,
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
  final bool connected;
  final int? latencyMs;
  final List<ProjectInfo> projects;
  final String? selectedProjectId;
  final bool projectListLoaded;
  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final bool hasCurrentTerminal;
  final AIStatsInfo? aiStats;
  final bool aiStatsLoading;
  final String projectFilesPath;
  final String? projectFilesParent;
  final List<RemoteFileEntry> projectFileEntries;
  final bool projectFilesLoading;
  final Widget terminalBody;
  final VoidCallback onShowTerminal;
  final VoidCallback onShowStats;
  final VoidCallback onShowFiles;
  final VoidCallback onBack;
  final VoidCallback onEditProject;
  final VoidCallback onAddProject;
  final VoidCallback onRemoveProject;
  final ValueChanged<ProjectInfo> onSelectProject;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onRefreshLists;
  final VoidCallback onCreateTerminal;
  final VoidCallback onCloseCurrentTerminal;
  final VoidCallback onRebuildTerminal;
  final VoidCallback onOpenTerminalSwitcher;
  final ValueChanged<String> onRequestProjectFiles;
  final ValueChanged<RemoteFileEntry> onOpenProjectFile;
  final VoidCallback onOpenProjectHome;
  final VoidCallback onOpenProjectRoot;
  final VoidCallback onOpenProjectVolumes;
  final ValueChanged<RemoteFileEntry> onRenameProjectFile;
  final ValueChanged<RemoteFileEntry> onCopyProjectFilePath;
  final ValueChanged<RemoteFileEntry> onDeleteProjectFile;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        TerminalHeader(
          topInset: topInset,
          latencyMs: connected ? latencyMs : null,
          connected: connected,
          activeMode: workspaceMode,
          onTerminal: onShowTerminal,
          onStats: onShowStats,
          onFiles: onShowFiles,
          onBack: onBack,
          onEditProject: onEditProject,
          onAddProject: onAddProject,
          onRemoveProject: onRemoveProject,
        ),
        ProjectTabBar(
          projects: projects,
          selectedId: selectedProjectId,
          loading: connected && !projectListLoaded,
          terminals: terminals,
          activeTerminalId: activeTerminalId,
          onSelect: onSelectProject,
          onSelectTerminal: onSelectTerminal,
          onRefresh: onRefreshLists,
          onCreateTerminal: onCreateTerminal,
          onCloseTerminal: hasCurrentTerminal ? onCloseCurrentTerminal : null,
          onRebuild: onRebuildTerminal,
          onOpenSwitcher: onOpenTerminalSwitcher,
        ),
        Expanded(child: _buildBody()),
      ],
    );
  }

  Widget _buildBody() {
    if (workspaceMode == WorkspaceMode.stats) {
      return AIStatsPanel(
        stats: aiStats,
        loading: aiStatsLoading,
        onRefresh: onShowStats,
      );
    }
    if (workspaceMode == WorkspaceMode.files) {
      return ProjectFilesPanel(
        path: projectFilesPath,
        parent: projectFilesParent,
        entries: projectFileEntries,
        loading: projectFilesLoading,
        onOpenPath: onRequestProjectFiles,
        onOpenFile: onOpenProjectFile,
        onRefresh: () => onRequestProjectFiles(projectFilesPath),
        onOpenHome: onOpenProjectHome,
        onOpenRoot: onOpenProjectRoot,
        onOpenVolumes: onOpenProjectVolumes,
        onRename: onRenameProjectFile,
        onCopyPath: onCopyProjectFilePath,
        onDelete: onDeleteProjectFile,
      );
    }
    return terminalBody;
  }
}
