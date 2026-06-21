import 'package:flutter/material.dart';

import '../../../i18n.dart';
import '../../../models/remote_models.dart';
import 'pad_file_list_item.dart';
import 'pad_theme.dart';
import 'pad_tool_panels.dart';
import '../../ai_stats_panel.dart';
import '../../project_files_panel.dart';

/// Contextual right column. A borderless, self-rounded surface with a unified
/// header on top (matching the sidebar header height) and a scrollable panel
/// below. Shows the file tree in "files" mode and AI stats in "stats" mode.
class PadRightColumn extends StatelessWidget {
  const PadRightColumn({
    super.key,
    required this.mode,
    required this.aiStats,
    required this.aiStatsLoading,
    required this.onShowStats,
    required this.gitStatus,
    required this.onGitAction,
    required this.onRefreshGit,
    required this.sshProfiles,
    required this.reviewSelectedPath,
    required this.onSelectReviewFile,
    required this.projectFilesPath,
    required this.projectFilesParent,
    required this.projectFileEntries,
    required this.projectFilesLoading,
    required this.onRequestProjectFiles,
    required this.onOpenProjectFile,
    required this.onOpenProjectHome,
    required this.onOpenProjectRoot,
    required this.onOpenProjectVolumes,
    required this.onRenameProjectFile,
    required this.onCopyProjectFilePath,
    required this.onDeleteProjectFile,
  });

  final String mode;
  final AIStatsInfo? aiStats;
  final bool aiStatsLoading;
  final VoidCallback onShowStats;
  final RemoteGitStatusInfo? gitStatus;
  final void Function(String op, Map<String, dynamic> args) onGitAction;
  final VoidCallback onRefreshGit;
  final List<RemoteSshProfile> sshProfiles;
  final String? reviewSelectedPath;
  final ValueChanged<String> onSelectReviewFile;
  final String projectFilesPath;
  final String? projectFilesParent;
  final List<RemoteFileEntry> projectFileEntries;
  final bool projectFilesLoading;
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
    final prefs = AppPreferences.of(context);
    if (mode == 'stats') {
      return SizedBox(
        width: PadMetrics.rightColumnWidth,
        child: AIStatsPanel(
          stats: aiStats,
          loading: aiStatsLoading,
          onRefresh: onShowStats,
          title: prefs.t('workspace.stats'),
          contentPadding: EdgeInsets.zero,
          cardBordered: true,
          colors: PadColors.statsPanel,
        ),
      );
    }
    if (mode == 'review') {
      final changes = [
        for (final file in gitStatus?.changedFiles ?? const <RemoteGitFileStatus>[])
          _ReviewChangeEntry(
            _reviewStatusCode(file),
            file.path,
            0,
            0,
          ),
      ];
      return PadPanelSurface(
        width: PadMetrics.rightColumnWidth,
        child: Column(
          children: [
            _ColumnHeader(title: prefs.t('workspace.review')),
            Expanded(
              child: _ReviewFileTree(
                changes: changes,
                selectedPath: reviewSelectedPath,
                onSelect: onSelectReviewFile,
                onRefresh: onRefreshGit,
              ),
            ),
            _ReviewFooter(status: gitStatus, onAction: onGitAction),
          ],
        ),
      );
    }
    if (mode == 'ssh') {
      return PadSshToolPanel(profiles: sshProfiles);
    }
    if (mode == 'git') {
      return PadGitToolPanel(
        gitStatus: gitStatus,
        onAction: onGitAction,
        onRefresh: onRefreshGit,
      );
    }
    return PadPanelSurface(
      width: PadMetrics.rightColumnWidth,
      child: Column(
        children: [
          _ColumnHeader(
            title: prefs.t('workspace.files'),
            trailing: ProjectFilesPanelActions(
              onOpenHome: onOpenProjectHome,
              onOpenRoot: onOpenProjectRoot,
              onOpenVolumes: onOpenProjectVolumes,
              dense: true,
              menuColor: PadColors.panel,
              plain: true,
            ),
          ),
          Expanded(child: _filesCardList(context)),
        ],
      ),
    );
  }

  /// File browser as card rows (matching the git/review lists): leading icon,
  /// name, and the path relative to the project root (no trailing slash). Files
  /// open on tap and expose rename/copy/delete on long-press.
  Widget _filesCardList(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    if (projectFilesLoading && projectFileEntries.isEmpty) {
      return ColoredBox(
        color: PadColors.panel,
        child: Center(child: CircularProgressIndicator(color: accent)),
      );
    }
    final rows = <Widget>[
      if (projectFilesParent != null)
        PadFileListItem(
          icon: Icons.arrow_upward_rounded,
          iconColor: accent,
          name: '返回上一级',
          path: _fileLabel(projectFilesParent!),
          onTap: () => onRequestProjectFiles(projectFilesParent!),
        ),
      for (final entry in projectFileEntries)
        PadFileListItem(
          icon: entry.isDirectory
              ? Icons.folder_rounded
              : Icons.description_outlined,
          iconColor: entry.isDirectory ? accent : PadColors.textMuted,
          name: entry.name,
          path: _fileLabel(entry.path),
          onTap: entry.isDirectory
              ? () => onRequestProjectFiles(entry.path)
              : () => onOpenProjectFile(entry),
          onLongPress: () => _showFileActions(context, entry),
        ),
    ];
    return ColoredBox(
      color: PadColors.panel,
      child: RefreshIndicator(
        onRefresh: () async => onRequestProjectFiles(projectFilesPath),
        color: accent,
        backgroundColor: PadColors.card,
        child: ListView.separated(
          physics: const AlwaysScrollableScrollPhysics(
            parent: BouncingScrollPhysics(),
          ),
          padding: const EdgeInsets.fromLTRB(10, 8, 10, 12),
          itemCount: rows.length,
          separatorBuilder: (_, _) => const SizedBox(height: 6),
          itemBuilder: (context, index) => rows[index],
        ),
      ),
    );
  }

  /// Path label for a browser entry, rooted at the current directory (`/`).
  /// Every visible item is a direct child of the current directory, so its
  /// location reads as `/` (the current directory is the root of this view).
  String _fileLabel(String absolutePath) {
    final base = projectFilesPath.trim();
    if (base.isNotEmpty && absolutePath.startsWith('$base/')) {
      final rel = absolutePath.substring(base.length + 1);
      final slash = rel.lastIndexOf('/');
      final dir = slash <= 0 ? '' : rel.substring(0, slash);
      return dir.isEmpty ? '/' : '/$dir';
    }
    return '/';
  }

  void _showFileActions(BuildContext context, RemoteFileEntry entry) {
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: PadColors.panel,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (sheetContext) {
        final accent = Theme.of(sheetContext).colorScheme.secondary;
        Widget item(IconData icon, String label, VoidCallback onTap,
            {bool danger = false}) {
          final color = danger ? PadColors.danger : accent;
          return InkWell(
            onTap: () {
              Navigator.of(sheetContext).pop();
              onTap();
            },
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
              child: Row(
                children: [
                  Icon(icon, size: 18, color: color),
                  const SizedBox(width: 12),
                  Text(
                    label,
                    style: TextStyle(
                      color: danger ? PadColors.danger : PadColors.textPrimary,
                      fontSize: 14,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
            ),
          );
        }

        return SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const SizedBox(height: 8),
              item(Icons.drive_file_rename_outline_rounded, '重命名',
                  () => onRenameProjectFile(entry)),
              item(Icons.link_rounded, '复制路径',
                  () => onCopyProjectFilePath(entry)),
              item(Icons.delete_outline_rounded, '删除',
                  () => onDeleteProjectFile(entry), danger: true),
              const SizedBox(height: 8),
            ],
          ),
        );
      },
    );
  }
}

class _ReviewFileTree extends StatefulWidget {
  const _ReviewFileTree({
    required this.changes,
    required this.selectedPath,
    required this.onSelect,
    required this.onRefresh,
  });

  final List<_ReviewChangeEntry> changes;
  final String? selectedPath;
  final ValueChanged<String> onSelect;
  final VoidCallback onRefresh;

  @override
  State<_ReviewFileTree> createState() => _ReviewFileTreeState();
}

class _ReviewFileTreeState extends State<_ReviewFileTree> {
  String _currentPath = '';

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    final snapshot = _ReviewDirectorySnapshot.from(
      _currentPath,
      widget.changes,
    );
    final parentPath = _currentPath.isEmpty
        ? null
        : _parentReviewPath(_currentPath);
    final rows = <Widget>[
      if (parentPath != null)
        PadFileListItem(
          icon: Icons.arrow_upward_rounded,
          iconColor: accent,
          name: prefs.t('project.parentDir'),
          path: padRootRelativePath('$parentPath/.'),
          onTap: () => setState(() => _currentPath = parentPath),
        ),
      for (final folder in snapshot.folders)
        PadFileListItem(
          icon: Icons.folder_rounded,
          iconColor: accent,
          name: folder.name,
          path: padRootRelativePath('${folder.path}/.'),
          trailing: PadCountChip(label: '${folder.count}'),
          onTap: () => setState(() => _currentPath = folder.path),
        ),
      for (final file in snapshot.files)
        PadFileListItem(
          icon: _reviewFileIcon(file.status),
          iconColor: widget.selectedPath == file.path
              ? accent
              : PadColors.textMuted,
          name: file.name,
          path: padRootRelativePath(file.path),
          trailing: PadStatusTag(
            label: file.status,
            color: _reviewStatusColor(file.status, accent),
          ),
          selected: widget.selectedPath == file.path,
          onTap: () => widget.onSelect(file.path),
        ),
    ];

    return ColoredBox(
      color: PadColors.panel,
      child: RefreshIndicator(
        onRefresh: () async => widget.onRefresh(),
        color: accent,
        backgroundColor: PadColors.card,
        child: ListView.separated(
          physics: const AlwaysScrollableScrollPhysics(
            parent: BouncingScrollPhysics(),
          ),
          padding: const EdgeInsets.fromLTRB(10, 8, 10, 12),
          itemCount: rows.length,
          separatorBuilder: (_, _) => const SizedBox(height: 6),
          itemBuilder: (context, index) => rows[index],
        ),
      ),
    );
  }
}

/// Single-letter status from a git file's index/worktree status codes.
String _reviewStatusCode(RemoteGitFileStatus file) {
  final index = file.indexStatus.trim();
  final worktree = file.worktreeStatus.trim();
  if (index == '?' || worktree == '?') return 'A';
  final code = index.isNotEmpty ? index : worktree;
  return code.isEmpty ? 'M' : code;
}

class _ReviewDirectorySnapshot {
  const _ReviewDirectorySnapshot({required this.folders, required this.files});

  final List<_ReviewFolderNode> folders;
  final List<_ReviewChangeEntry> files;

  static _ReviewDirectorySnapshot from(
    String basePath,
    List<_ReviewChangeEntry> changes,
  ) {
    final folders = <String, _ReviewFolderNode>{};
    final files = <_ReviewChangeEntry>[];

    for (final change in changes) {
      final relativePath = _relativeReviewPath(basePath, change.path);
      if (relativePath == null || relativePath.isEmpty) {
        continue;
      }
      final slashIndex = relativePath.indexOf('/');
      if (slashIndex < 0) {
        files.add(change);
        continue;
      }
      final folderName = relativePath.substring(0, slashIndex);
      final folderPath = _joinReviewPath(basePath, folderName);
      folders
          .putIfAbsent(
            folderName,
            () => _ReviewFolderNode(name: folderName, path: folderPath),
          )
          .add(change);
    }

    final sortedFolders = folders.values.toList()
      ..sort((left, right) => left.name.compareTo(right.name));
    files.sort((left, right) => left.name.compareTo(right.name));
    return _ReviewDirectorySnapshot(folders: sortedFolders, files: files);
  }
}

class _ReviewFolderNode {
  _ReviewFolderNode({required this.name, required this.path});

  final String name;
  final String path;
  int count = 0;
  int additions = 0;
  int deletions = 0;

  void add(_ReviewChangeEntry change) {
    count += 1;
    additions += change.additions;
    deletions += change.deletions;
  }
}

class _ReviewChangeEntry {
  const _ReviewChangeEntry(
    this.status,
    this.path,
    this.additions,
    this.deletions,
  );

  final String status;
  final String path;
  final int additions;
  final int deletions;

  String get name {
    final parts = path.split('/');
    return parts.isEmpty ? path : parts.last;
  }

  String get parent {
    final index = path.lastIndexOf('/');
    return index <= 0 ? '' : path.substring(0, index);
  }
}

String? _parentReviewPath(String path) {
  if (path.isEmpty) {
    return null;
  }
  final index = path.lastIndexOf('/');
  return index < 0 ? '' : path.substring(0, index);
}

String? _relativeReviewPath(String basePath, String path) {
  if (basePath.isEmpty) {
    return path;
  }
  final prefix = '$basePath/';
  if (!path.startsWith(prefix)) {
    return null;
  }
  return path.substring(prefix.length);
}

String _joinReviewPath(String basePath, String child) {
  return basePath.isEmpty ? child : '$basePath/$child';
}

Color _reviewStatusColor(String status, Color accent) {
  return switch (status) {
    'A' => PadColors.success,
    'D' => PadColors.danger,
    'R' => PadColors.warning,
    _ => accent,
  };
}

IconData _reviewFileIcon(String status) {
  return switch (status) {
    'A' => Icons.note_add_rounded,
    'D' => Icons.note_alt_outlined,
    'R' => Icons.drive_file_rename_outline_rounded,
    _ => Icons.description_outlined,
  };
}

/// Unified column header — same height as the sidebar header bars.
class _ColumnHeader extends StatelessWidget {
  const _ColumnHeader({required this.title, this.trailing});

  final String title;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 48,
      color: PadColors.header,
      padding: const EdgeInsets.symmetric(horizontal: 14),
      alignment: Alignment.centerLeft,
      child: Row(
        children: [
          Expanded(
            child: Text(
              title,
              style: const TextStyle(
                color: PadColors.textPrimary,
                fontSize: 15,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          ?trailing,
        ],
      ),
    );
  }
}

/// Review panel footer: one-tap commit&push / commit&merge for the current
/// project changes. The merge action prompts for a target branch.
class _ReviewFooter extends StatefulWidget {
  const _ReviewFooter({required this.status, required this.onAction});

  final RemoteGitStatusInfo? status;
  final void Function(String op, Map<String, dynamic> args) onAction;

  @override
  State<_ReviewFooter> createState() => _ReviewFooterState();
}

class _ReviewFooterState extends State<_ReviewFooter> {
  bool _busy = false;

  @override
  void didUpdateWidget(covariant _ReviewFooter oldWidget) {
    super.didUpdateWidget(oldWidget);
    // A fresh git.status reply (new object) means the last action settled.
    if (!identical(widget.status, oldWidget.status) && _busy) {
      setState(() => _busy = false);
    }
  }

  bool get _hasChanges => (widget.status?.changes ?? 0) > 0;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final enabled = _hasChanges && !_busy;
    return Container(
      decoration: const BoxDecoration(
        color: PadColors.header,
        border: Border(top: BorderSide(color: PadColors.border, width: 0.5)),
      ),
      padding: const EdgeInsets.fromLTRB(10, 8, 10, 10),
      child: Row(
        children: [
          Expanded(
            child: _ReviewFooterButton(
              icon: Icons.cloud_upload_rounded,
              label: '提交推送',
              accent: accent,
              busy: _busy,
              onTap: enabled ? _commitPush : null,
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: _ReviewFooterButton(
              icon: Icons.merge_rounded,
              label: '提交合并',
              accent: accent,
              busy: _busy,
              onTap: enabled ? _commitMerge : null,
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _commitPush() async {
    final message = await _promptMessage(context, '提交并推送');
    if (message == null || message.isEmpty) return;
    setState(() => _busy = true);
    widget.onAction('commit_push', {'message': message});
  }

  Future<void> _commitMerge() async {
    final branches = (widget.status?.branches ?? const <RemoteGitBranch>[])
        .where((branch) => !branch.isCurrent)
        .map((branch) => branch.name)
        .toList();
    if (branches.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('没有可合并的目标分支')),
      );
      return;
    }
    final result = await _promptMerge(context, branches);
    if (result == null) return;
    setState(() => _busy = true);
    widget.onAction('commit_merge', {
      'message': result.$1,
      'target': result.$2,
    });
  }
}

class _ReviewFooterButton extends StatelessWidget {
  const _ReviewFooterButton({
    required this.icon,
    required this.label,
    required this.accent,
    required this.busy,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final Color accent;
  final bool busy;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final enabled = onTap != null;
    return InkWell(
      borderRadius: BorderRadius.circular(8),
      onTap: onTap,
      child: Container(
        height: 36,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: accent.withValues(alpha: enabled ? 0.14 : 0.06),
          borderRadius: BorderRadius.circular(8),
        ),
        child: busy
            ? SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2, color: accent),
              )
            : Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(
                    icon,
                    size: 16,
                    color: enabled ? accent : PadColors.textSubtle,
                  ),
                  const SizedBox(width: 6),
                  Text(
                    label,
                    style: TextStyle(
                      color: enabled ? accent : PadColors.textSubtle,
                      fontSize: 12.5,
                      fontWeight: FontWeight.w800,
                    ),
                  ),
                ],
              ),
      ),
    );
  }
}

Future<String?> _promptMessage(BuildContext context, String title) async {
  final controller = TextEditingController();
  final accent = Theme.of(context).colorScheme.secondary;
  final message = await showDialog<String>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      backgroundColor: PadColors.panel,
      title: Text(
        title,
        style: const TextStyle(color: PadColors.textPrimary, fontSize: 16),
      ),
      content: TextField(
        controller: controller,
        autofocus: true,
        maxLines: 3,
        style: const TextStyle(color: PadColors.textPrimary),
        decoration: const InputDecoration(
          hintText: '提交说明',
          hintStyle: TextStyle(color: PadColors.textSubtle),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(),
          child: const Text('取消', style: TextStyle(color: PadColors.textMuted)),
        ),
        TextButton(
          onPressed: () =>
              Navigator.of(dialogContext).pop(controller.text.trim()),
          child: Text('确定', style: TextStyle(color: accent)),
        ),
      ],
    ),
  );
  controller.dispose();
  return message;
}

Future<(String, String)?> _promptMerge(
  BuildContext context,
  List<String> branches,
) async {
  final controller = TextEditingController();
  final accent = Theme.of(context).colorScheme.secondary;
  String target = branches.first;
  final result = await showDialog<(String, String)>(
    context: context,
    builder: (dialogContext) => StatefulBuilder(
      builder: (dialogContext, setLocal) => AlertDialog(
        backgroundColor: PadColors.panel,
        title: const Text(
          '提交并合并',
          style: TextStyle(color: PadColors.textPrimary, fontSize: 16),
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: controller,
              autofocus: true,
              maxLines: 2,
              style: const TextStyle(color: PadColors.textPrimary),
              decoration: const InputDecoration(
                hintText: '提交说明',
                hintStyle: TextStyle(color: PadColors.textSubtle),
              ),
            ),
            const SizedBox(height: 14),
            const Text(
              '合并到目标分支',
              style: TextStyle(color: PadColors.textSubtle, fontSize: 12),
            ),
            const SizedBox(height: 6),
            DropdownButton<String>(
              value: target,
              isExpanded: true,
              dropdownColor: PadColors.panel,
              style: const TextStyle(color: PadColors.textPrimary),
              items: [
                for (final branch in branches)
                  DropdownMenuItem(value: branch, child: Text(branch)),
              ],
              onChanged: (value) =>
                  setLocal(() => target = value ?? target),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('取消', style: TextStyle(color: PadColors.textMuted)),
          ),
          TextButton(
            onPressed: () {
              final message = controller.text.trim();
              if (message.isEmpty) return;
              Navigator.of(dialogContext).pop((message, target));
            },
            child: Text('确定', style: TextStyle(color: accent)),
          ),
        ],
      ),
    ),
  );
  controller.dispose();
  return result;
}
