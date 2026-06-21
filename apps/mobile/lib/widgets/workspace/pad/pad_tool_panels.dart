import 'package:flutter/material.dart';

import '../../../models/remote_models.dart';
import 'pad_file_list_item.dart';
import 'pad_theme.dart';

class PadSshToolPanel extends StatefulWidget {
  const PadSshToolPanel({super.key, required this.profiles});

  final List<RemoteSshProfile> profiles;

  @override
  State<PadSshToolPanel> createState() => _PadSshToolPanelState();
}

class _PadSshToolPanelState extends State<PadSshToolPanel> {
  String? _expandedId;

  @override
  Widget build(BuildContext context) {
    return PadPanelSurface(
      width: PadMetrics.rightColumnWidth,
      child: Column(
        children: [
          const _ToolHeader(title: 'SSH', actionIcon: Icons.add_rounded),
          Expanded(
            child: widget.profiles.isEmpty
                ? const Center(
                    child: Padding(
                      padding: EdgeInsets.all(24),
                      child: Text(
                        'No saved SSH profiles on this host',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: PadColors.textSubtle, fontSize: 13),
                      ),
                    ),
                  )
                : ListView(
                    physics: const BouncingScrollPhysics(),
                    padding: const EdgeInsets.fromLTRB(10, 10, 10, 12),
                    children: [
                      for (final profile in widget.profiles)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 8),
                          child: _SshProfileRow(
                            profile: profile,
                            expanded: profile.id == _expandedId,
                            onTap: () => setState(() {
                              _expandedId = _expandedId == profile.id
                                  ? null
                                  : profile.id;
                            }),
                          ),
                        ),
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

class PadGitToolPanel extends StatefulWidget {
  const PadGitToolPanel({
    super.key,
    required this.gitStatus,
    required this.projectRootName,
    required this.onAction,
    required this.onRefresh,
  });

  final RemoteGitStatusInfo? gitStatus;
  final String projectRootName;
  final void Function(String op, Map<String, dynamic> args) onAction;
  final VoidCallback onRefresh;

  @override
  State<PadGitToolPanel> createState() => _PadGitToolPanelState();
}

class _PadGitToolPanelState extends State<PadGitToolPanel> {
  String _section = 'changed';
  final Map<String, String> _currentPaths = {
    'staged': _gitRootPath,
    'changed': _gitRootPath,
    'untracked': _gitRootPath,
  };
  final Set<String> _selectedPaths = {};

  /// Maps real `git.status` changed files into the panel's section model.
  /// A partially-staged file appears in both `staged` and `changed`.
  List<_GitPreviewFile> _filesFromStatus() {
    final status = widget.gitStatus;
    if (status == null) return const [];
    final out = <_GitPreviewFile>[];
    for (final file in status.changedFiles) {
      final index = file.indexStatus.trim();
      final worktree = file.worktreeStatus.trim();
      if (index == '?' || worktree == '?') {
        out.add(_GitPreviewFile(section: 'untracked', status: '?', path: file.path));
        continue;
      }
      if (index.isNotEmpty) {
        out.add(_GitPreviewFile(section: 'staged', status: index, path: file.path));
      }
      if (worktree.isNotEmpty) {
        out.add(_GitPreviewFile(section: 'changed', status: worktree, path: file.path));
      }
    }
    return out;
  }

  @override
  Widget build(BuildContext context) {
    final allFiles = _filesFromStatus();
    final files = allFiles.where((file) => file.section == _section).toList();
    final currentPath = _currentPaths[_section] ?? _gitRootPath;
    final snapshot = _GitDirectorySnapshot.from(currentPath, files);
    final visibleFiles = snapshot.files;
    final scopedFiles = _gitFilesInScope(currentPath, files);
    final selectedSectionCount = files
        .where((file) => _selectedPaths.contains(file.path))
        .length;
    final allScopedSelected =
        scopedFiles.isNotEmpty &&
        scopedFiles.every((file) => _selectedPaths.contains(file.path));
    final parentPath = currentPath == _gitRootPath
        ? null
        : _parentToolPath(currentPath);

    return PadPanelSurface(
      width: PadMetrics.rightColumnWidth,
      child: Column(
        children: [
          const _ToolHeader(title: 'Git'),
          Expanded(
            child: RefreshIndicator(
              onRefresh: () async => widget.onRefresh(),
              color: Theme.of(context).colorScheme.secondary,
              backgroundColor: PadColors.card,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(
                  parent: BouncingScrollPhysics(),
                ),
                padding: const EdgeInsets.fromLTRB(10, 10, 10, 12),
                children: [
                _GitSummaryCard(
                  status: widget.gitStatus,
                  onAction: widget.onAction,
                ),
                const SizedBox(height: 10),
                _GitSectionTabs(
                  selected: _section,
                  onChanged: (value) => setState(() {
                    _section = value;
                    _currentPaths[value] ??= _gitRootPath;
                  }),
                ),
                const SizedBox(height: 8),
                if (parentPath != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: PadFileListItem(
                      icon: Icons.arrow_upward_rounded,
                      iconColor: Theme.of(context).colorScheme.secondary,
                      name: '返回上一级',
                      path: padRootRelativePath(
                        widget.projectRootName,
                        '$parentPath/.',
                      ),
                      onTap: () => setState(() {
                        _currentPaths[_section] = parentPath;
                      }),
                    ),
                  ),
                for (final folder in snapshot.folders)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: Builder(
                      builder: (context) {
                        final folderFiles = _gitFilesInScope(
                          folder.path,
                          files,
                        );
                        final folderSelected =
                            folderFiles.isNotEmpty &&
                            folderFiles.every(
                              (file) => _selectedPaths.contains(file.path),
                            );
                        return PadFileListItem(
                          icon: Icons.folder_rounded,
                          iconColor: Theme.of(context).colorScheme.secondary,
                          name: folder.name,
                          path: padRootRelativePath(
                            widget.projectRootName,
                            '${folder.path}/.',
                          ),
                          trailing: PadCountChip(label: '${folder.count}'),
                          selected: folderSelected,
                          onTap: () => setState(() {
                            _currentPaths[_section] = folder.path;
                          }),
                          onLongPress: () =>
                              setState(() => _toggleFiles(folderFiles)),
                        );
                      },
                    ),
                  ),
                for (final file in visibleFiles)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: PadFileListItem(
                      icon: _gitFileIcon(file.status),
                      iconColor: _selectedPaths.contains(file.path)
                          ? Theme.of(context).colorScheme.secondary
                          : PadColors.textMuted,
                      name: file.name,
                      path: padRootRelativePath(
                        widget.projectRootName,
                        file.path,
                      ),
                      trailing: PadStatusTag(
                        label: file.status,
                        color: _gitStatusColor(
                          file.status,
                          Theme.of(context).colorScheme.secondary,
                        ),
                      ),
                      selected: _selectedPaths.contains(file.path),
                      onTap: () => widget.onAction(
                        _section == 'staged' ? 'unstage' : 'stage',
                        {
                          'paths': [file.path],
                        },
                      ),
                      onLongPress: () => setState(() => _toggleFile(file)),
                    ),
                  ),
                ],
              ),
            ),
          ),
          _GitFooterBar(
            path: currentPath,
            selectedCount: selectedSectionCount,
            allSelected: allScopedSelected,
            onToggleAll: () => setState(() {
              if (allScopedSelected) {
                for (final file in scopedFiles) {
                  _selectedPaths.remove(file.path);
                }
                return;
              }
              for (final file in scopedFiles) {
                _selectedPaths.add(file.path);
              }
            }),
          ),
        ],
      ),
    );
  }

  void _toggleFile(_GitPreviewFile file) {
    if (!_selectedPaths.add(file.path)) {
      _selectedPaths.remove(file.path);
    }
  }

  void _toggleFiles(List<_GitPreviewFile> files) {
    if (files.isEmpty) return;
    final allSelected = files.every(
      (file) => _selectedPaths.contains(file.path),
    );
    for (final file in files) {
      if (allSelected) {
        _selectedPaths.remove(file.path);
      } else {
        _selectedPaths.add(file.path);
      }
    }
  }
}

class _GitPreviewFile {
  const _GitPreviewFile({
    required this.section,
    required this.status,
    required this.path,
  });

  final String section;
  final String status;
  final String path;

  String get name {
    final parts = path.split('/');
    return parts.isEmpty ? path : parts.last;
  }

  String get parent {
    final index = path.lastIndexOf('/');
    return index <= 0 ? '' : path.substring(0, index);
  }
}

const _gitRootPath = '';

class _GitDirectorySnapshot {
  const _GitDirectorySnapshot({required this.folders, required this.files});

  final List<_GitFolderNode> folders;
  final List<_GitPreviewFile> files;

  static _GitDirectorySnapshot from(
    String basePath,
    List<_GitPreviewFile> changes,
  ) {
    final folders = <String, _GitFolderNode>{};
    final files = <_GitPreviewFile>[];

    for (final change in changes) {
      final relativePath = _relativeToolPath(basePath, change.path);
      if (relativePath == null || relativePath.isEmpty) {
        continue;
      }
      final slashIndex = relativePath.indexOf('/');
      if (slashIndex < 0) {
        files.add(change);
        continue;
      }
      final folderName = relativePath.substring(0, slashIndex);
      final folderPath = _joinToolPath(basePath, folderName);
      folders
          .putIfAbsent(
            folderName,
            () => _GitFolderNode(name: folderName, path: folderPath),
          )
          .add(change);
    }

    final sortedFolders = folders.values.toList()
      ..sort((left, right) => left.name.compareTo(right.name));
    files.sort((left, right) => left.name.compareTo(right.name));
    return _GitDirectorySnapshot(folders: sortedFolders, files: files);
  }
}

class _GitFolderNode {
  _GitFolderNode({required this.name, required this.path});

  final String name;
  final String path;
  int count = 0;

  void add(_GitPreviewFile file) {
    count += 1;
  }
}

class _ToolHeader extends StatelessWidget {
  const _ToolHeader({required this.title, this.actionIcon});

  final String title;
  final IconData? actionIcon;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return Container(
      height: 48,
      color: PadColors.header,
      padding: const EdgeInsets.symmetric(horizontal: 14),
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
          if (actionIcon != null) ...[
            _ToolIconButton(icon: actionIcon!, color: accent),
            const SizedBox(width: 2),
          ],
          const _ToolIconButton(icon: Icons.more_horiz_rounded),
        ],
      ),
    );
  }
}

class _ToolIconButton extends StatelessWidget {
  const _ToolIconButton({required this.icon, this.color});

  final IconData icon;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 32,
      height: 32,
      child: Icon(icon, size: 18, color: color ?? PadColors.textSubtle),
    );
  }
}

class _SshProfileRow extends StatelessWidget {
  const _SshProfileRow({
    required this.profile,
    required this.expanded,
    required this.onTap,
  });

  final RemoteSshProfile profile;
  final bool expanded;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return _ToolCard(
      selected: expanded,
      onTap: onTap,
      child: Column(
        children: [
          Row(
            children: [
              _ToolIconTile(
                icon: Icons.terminal_rounded,
                color: expanded ? accent : PadColors.textMuted,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: _ToolTitleBlock(
                  title: profile.name,
                  subtitle: profile.endpoint,
                ),
              ),
              const SizedBox(width: 8),
              Icon(
                expanded
                    ? Icons.keyboard_arrow_up_rounded
                    : Icons.keyboard_arrow_down_rounded,
                size: 20,
                color: PadColors.textSubtle,
              ),
            ],
          ),
          if (expanded) ...[
            const SizedBox(height: 12),
            _SshProfileDetail(profile: profile),
          ],
        ],
      ),
    );
  }
}

class _SshProfileDetail extends StatelessWidget {
  const _SshProfileDetail({required this.profile});

  final RemoteSshProfile profile;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _MetaRow(label: 'Endpoint', value: profile.endpoint),
        _MetaRow(label: 'Credential', value: profile.credential),
      ],
    );
  }
}

class _GitSummaryCard extends StatelessWidget {
  const _GitSummaryCard({required this.status, required this.onAction});

  final RemoteGitStatusInfo? status;
  final void Function(String op, Map<String, dynamic> args) onAction;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final branch = status?.branch.trim().isNotEmpty == true
        ? status!.branch.trim()
        : '—';
    final upstream = status?.upstream?.trim() ?? '';
    final subtitleParts = <String>[
      if (upstream.isNotEmpty) upstream,
      if ((status?.ahead ?? 0) > 0) '${status!.ahead} ahead',
      if ((status?.behind ?? 0) > 0) '${status!.behind} behind',
    ];
    return _ToolCard(
      bordered: false,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              _ToolIconTile(icon: Icons.account_tree_rounded, color: accent),
              const SizedBox(width: 10),
              Expanded(
                child: _ToolTitleBlock(
                  title: branch,
                  subtitle: subtitleParts.isEmpty
                      ? 'no upstream'
                      : subtitleParts.join(' · '),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Container(
            height: 48,
            padding: const EdgeInsets.symmetric(horizontal: 8),
            decoration: BoxDecoration(
              color: const Color(0xFF111820),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Row(
              children: [
                _GitMetric(
                  icon: Icons.inventory_2_rounded,
                  value: '${status?.staged ?? 0}',
                  color: PadColors.chartB,
                ),
                const _GitMetricDivider(),
                _GitMetric(
                  icon: Icons.edit_note_rounded,
                  value: '${status?.unstaged ?? 0}',
                  color: PadColors.warning,
                ),
                const _GitMetricDivider(),
                _GitMetric(
                  icon: Icons.add_circle_outline_rounded,
                  value: '${status?.untracked ?? 0}',
                  color: PadColors.success,
                ),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: _MiniActionButton(
                  icon: Icons.check_rounded,
                  label: 'Commit',
                  onTap: () => _promptCommit(context),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: _MiniActionButton(
                  icon: Icons.sync_rounded,
                  label: 'Sync',
                  onTap: () => onAction('sync', const {}),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _promptCommit(BuildContext context) async {
    final controller = TextEditingController();
    final accent = Theme.of(context).colorScheme.secondary;
    final message = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: PadColors.panel,
        title: const Text(
          'Commit',
          style: TextStyle(color: PadColors.textPrimary, fontSize: 16),
        ),
        content: TextField(
          controller: controller,
          autofocus: true,
          maxLines: 3,
          style: const TextStyle(color: PadColors.textPrimary),
          decoration: const InputDecoration(
            hintText: 'Commit message',
            hintStyle: TextStyle(color: PadColors.textSubtle),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel', style: TextStyle(color: PadColors.textMuted)),
          ),
          TextButton(
            onPressed: () =>
                Navigator.of(dialogContext).pop(controller.text.trim()),
            child: Text('Commit', style: TextStyle(color: accent)),
          ),
        ],
      ),
    );
    controller.dispose();
    if (message != null && message.isNotEmpty) {
      onAction('commit', {'message': message});
    }
  }
}

class _GitSectionTabs extends StatelessWidget {
  const _GitSectionTabs({required this.selected, required this.onChanged});

  final String selected;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 36,
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: PadColors.panelTrack,
        borderRadius: BorderRadius.circular(18),
      ),
      child: Row(
        children: [
          _GitSectionTab(
            value: 'staged',
            label: '已暂存',
            selected: selected == 'staged',
            onTap: onChanged,
          ),
          _GitSectionTab(
            value: 'changed',
            label: '已修改',
            selected: selected == 'changed',
            onTap: onChanged,
          ),
          _GitSectionTab(
            value: 'untracked',
            label: '新增',
            selected: selected == 'untracked',
            onTap: onChanged,
          ),
        ],
      ),
    );
  }
}

class _GitSectionTab extends StatelessWidget {
  const _GitSectionTab({
    required this.value,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String value;
  final String label;
  final bool selected;
  final ValueChanged<String> onTap;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return Expanded(
      child: InkWell(
        borderRadius: BorderRadius.circular(15),
        onTap: () => onTap(value),
        child: Container(
          height: 30,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: selected ? PadColors.cardActive : Colors.transparent,
            borderRadius: BorderRadius.circular(15),
          ),
          child: Text(
            label,
            style: TextStyle(
              color: selected ? accent : PadColors.textMuted,
              fontSize: 11.5,
              fontWeight: FontWeight.w800,
            ),
          ),
        ),
      ),
    );
  }
}

class _GitPathStrip extends StatelessWidget {
  const _GitPathStrip({required this.path});

  final String path;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 32,
      color: PadColors.panelTrack,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      child: Row(
        children: [
          const Icon(
            Icons.account_tree_rounded,
            size: 15,
            color: PadColors.textMuted,
          ),
          const SizedBox(width: 7),
          Expanded(
            child: Text(
              path.isEmpty ? 'codux-gpui' : path,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(
                color: PadColors.textSecondary,
                fontSize: 11.5,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _GitFooterBar extends StatelessWidget {
  const _GitFooterBar({
    required this.path,
    required this.selectedCount,
    required this.allSelected,
    required this.onToggleAll,
  });

  final String path;
  final int selectedCount;
  final bool allSelected;
  final VoidCallback onToggleAll;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: const BoxDecoration(
        color: PadColors.header,
        border: Border(top: BorderSide(color: PadColors.border, width: 0.5)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (selectedCount > 0)
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 8, 10, 10),
              child: Row(
                children: [
                  SizedBox(
                    width: 34,
                    child: Text(
                      '$selectedCount',
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        color: PadColors.textMuted,
                        fontSize: 11.5,
                        fontWeight: FontWeight.w800,
                      ),
                    ),
                  ),
                  const SizedBox(width: 7),
                  Expanded(
                    child: _FooterActionButton(
                      icon: allSelected
                          ? Icons.remove_done_rounded
                          : Icons.select_all_rounded,
                      label: allSelected ? '取消' : '全选',
                      onTap: onToggleAll,
                    ),
                  ),
                  const SizedBox(width: 7),
                  const Expanded(
                    child: _FooterActionButton(
                      icon: Icons.add_task_rounded,
                      label: '暂存',
                    ),
                  ),
                  const SizedBox(width: 7),
                  const Expanded(
                    child: _FooterActionButton(
                      icon: Icons.undo_rounded,
                      label: '放弃',
                      danger: true,
                    ),
                  ),
                ],
              ),
            ),
          _GitPathStrip(path: path),
        ],
      ),
    );
  }
}

Color _gitStatusColor(String status, Color accent) {
  return switch (status) {
    'A' || '?' => PadColors.success,
    'D' => PadColors.danger,
    'R' => PadColors.warning,
    _ => accent,
  };
}

IconData _gitFileIcon(String status) {
  return switch (status) {
    'A' || '?' => Icons.note_add_rounded,
    'D' => Icons.note_alt_outlined,
    'R' => Icons.drive_file_rename_outline_rounded,
    _ => Icons.description_outlined,
  };
}

String? _parentToolPath(String path) {
  if (path.isEmpty) {
    return null;
  }
  final index = path.lastIndexOf('/');
  return index < 0 ? '' : path.substring(0, index);
}

String? _relativeToolPath(String basePath, String path) {
  if (basePath.isEmpty) {
    return path;
  }
  final prefix = '$basePath/';
  if (!path.startsWith(prefix)) {
    return null;
  }
  return path.substring(prefix.length);
}

String _joinToolPath(String basePath, String child) {
  return basePath.isEmpty ? child : '$basePath/$child';
}

List<_GitPreviewFile> _gitFilesInScope(
  String basePath,
  List<_GitPreviewFile> files,
) {
  if (basePath.isEmpty) {
    return files;
  }
  final prefix = '$basePath/';
  return files.where((file) => file.path.startsWith(prefix)).toList();
}

class _FooterActionButton extends StatelessWidget {
  const _FooterActionButton({
    required this.icon,
    required this.label,
    this.onTap,
    this.danger = false,
  });

  final IconData icon;
  final String label;
  final VoidCallback? onTap;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final accent = danger
        ? PadColors.danger
        : Theme.of(context).colorScheme.secondary;
    return InkWell(
      borderRadius: BorderRadius.circular(8),
      onTap: onTap,
      child: Container(
        height: 34,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: accent.withValues(alpha: 0.12),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 15, color: accent),
            const SizedBox(width: 5),
            Flexible(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: accent,
                  fontSize: 11.5,
                  fontWeight: FontWeight.w800,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ToolCard extends StatelessWidget {
  const _ToolCard({
    required this.child,
    this.selected = false,
    this.bordered = false,
    this.onTap,
  });

  final Widget child;
  final bool selected;
  final bool bordered;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final content = AnimatedContainer(
      duration: const Duration(milliseconds: 120),
      curve: Curves.easeOutCubic,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: selected ? PadColors.cardActive : PadColors.card,
        borderRadius: BorderRadius.circular(10),
        border: bordered
            ? Border.all(color: PadColors.border, width: 0.5)
            : null,
      ),
      child: child,
    );
    if (onTap == null) return content;
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: onTap,
        child: content,
      ),
    );
  }
}

class _ToolIconTile extends StatelessWidget {
  const _ToolIconTile({required this.icon, required this.color});

  final IconData icon;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 34,
      height: 34,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Icon(icon, size: 18, color: color),
    );
  }
}

class _ToolTitleBlock extends StatelessWidget {
  const _ToolTitleBlock({required this.title, required this.subtitle});

  final String title;
  final String subtitle;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          title,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(
            color: PadColors.textPrimary,
            fontSize: 13,
            fontWeight: FontWeight.w700,
          ),
        ),
        const SizedBox(height: 3),
        Text(
          subtitle,
          textDirection: TextDirection.rtl,
          textAlign: TextAlign.right,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: PadColors.textSubtle, fontSize: 11),
        ),
      ],
    );
  }
}

class _MetaRow extends StatelessWidget {
  const _MetaRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 7),
      child: Row(
        children: [
          Expanded(
            child: Text(
              label,
              style: const TextStyle(
                color: PadColors.textMuted,
                fontSize: 11.5,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          Text(
            value,
            style: const TextStyle(
              color: PadColors.textSecondary,
              fontSize: 11.5,
              fontWeight: FontWeight.w700,
            ),
          ),
        ],
      ),
    );
  }
}

class _MiniActionButton extends StatelessWidget {
  const _MiniActionButton({required this.icon, required this.label, this.onTap});

  final IconData icon;
  final String label;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Container(
        height: 34,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: accent.withValues(alpha: 0.12),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 15, color: accent),
            const SizedBox(width: 6),
            Text(
              label,
              style: TextStyle(
                color: accent,
                fontSize: 11.5,
                fontWeight: FontWeight.w800,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _GitMetric extends StatelessWidget {
  const _GitMetric({
    required this.icon,
    required this.value,
    required this.color,
  });

  final IconData icon;
  final String value;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Expanded(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 4),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Container(
              width: 24,
              height: 24,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: color.withValues(alpha: 0.13),
                borderRadius: BorderRadius.circular(7),
              ),
              child: Icon(icon, size: 14, color: color),
            ),
            const SizedBox(width: 8),
            Text(
              value,
              style: const TextStyle(
                color: PadColors.textPrimary,
                fontSize: 16,
                fontWeight: FontWeight.w900,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _GitMetricDivider extends StatelessWidget {
  const _GitMetricDivider();

  @override
  Widget build(BuildContext context) {
    return Container(width: 0.5, height: 24, color: PadColors.border);
  }
}
