import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../models/remote_models.dart';
import 'pad_file_list_item.dart';
import 'pad_file_tree.dart';
import 'pad_theme.dart';

class PadSshToolPanel extends StatefulWidget {
  const PadSshToolPanel({
    super.key,
    required this.profiles,
    required this.onUpsert,
    required this.onRemove,
  });

  final List<RemoteSshProfile> profiles;
  final void Function(Map<String, dynamic> fields) onUpsert;
  final ValueChanged<String> onRemove;

  @override
  State<PadSshToolPanel> createState() => _PadSshToolPanelState();
}

class _PadSshToolPanelState extends State<PadSshToolPanel> {
  String? _expandedId;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return PadPanelSurface(
      width: PadMetrics.rightColumnWidth,
      child: Column(
        children: [
          Container(
            height: 48,
            color: PadColors.header,
            padding: const EdgeInsets.symmetric(horizontal: 14),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'SSH',
                    style: TextStyle(
                      color: PadColors.textPrimary,
                      fontSize: 15,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
                _GitHeaderButton(
                  icon: Icons.add_rounded,
                  color: accent,
                  onTap: () => _openForm(),
                ),
              ],
            ),
          ),
          Expanded(
            child: widget.profiles.isEmpty
                ? Center(
                    child: Padding(
                      padding: EdgeInsets.all(24),
                      child: Text(
                        '暂无 SSH 连接,点右上角 + 添加',
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
                            onEdit: () => _openForm(profile),
                            onDelete: () => _confirmDelete(profile),
                          ),
                        ),
                    ],
                  ),
          ),
        ],
      ),
    );
  }

  void _openForm([RemoteSshProfile? existing]) {
    showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      backgroundColor: PadColors.panel,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (sheetContext) => _SshFormSheet(
        existing: existing,
        onSubmit: (fields) {
          Navigator.of(sheetContext).pop();
          widget.onUpsert(fields);
        },
      ),
    );
  }

  Future<void> _confirmDelete(RemoteSshProfile profile) async {
    final accent = Theme.of(context).colorScheme.secondary;
    final ok = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: PadColors.panel,
        title: Text(
          '删除 SSH 连接',
          style: TextStyle(color: PadColors.textPrimary, fontSize: 16),
        ),
        content: Text(
          '确定删除「${profile.name}」?',
          style: TextStyle(color: PadColors.textSecondary),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text('取消', style: TextStyle(color: PadColors.textMuted)),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text('删除', style: TextStyle(color: accent)),
          ),
        ],
      ),
    );
    if (ok == true) widget.onRemove(profile.id);
  }
}

class PadGitToolPanel extends StatefulWidget {
  const PadGitToolPanel({
    super.key,
    required this.gitStatus,
    required this.onAction,
    required this.onRefresh,
    required this.onOpenFile,
  });

  final RemoteGitStatusInfo? gitStatus;
  final void Function(String op, Map<String, dynamic> args) onAction;
  final VoidCallback onRefresh;

  /// Open a changed file's diff (switches the center to the review view).
  final ValueChanged<String> onOpenFile;

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
  final TextEditingController _commitController = TextEditingController();
  bool _syncing = false;

  @override
  void initState() {
    super.initState();
    // Rebuild as the user types so the commit button can enable/disable itself.
    _commitController.addListener(_onCommitMessageChanged);
  }

  void _onCommitMessageChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _commitController.removeListener(_onCommitMessageChanged);
    _commitController.dispose();
    super.dispose();
  }

  void _sync() {
    if (_syncing) return;
    setState(() => _syncing = true);
    widget.onAction('sync', const {});
    // Keep the spinner up for a beat so the sync reads as an action, even when
    // the host's git.status reply comes back almost instantly.
    Future.delayed(const Duration(seconds: 3), () {
      if (mounted) setState(() => _syncing = false);
    });
  }

  /// Commit the staged changes via `op` (commit / commit_push / commit_sync),
  /// then clear the message field.
  void _commit(String op) {
    final message = _commitController.text.trim();
    if (message.isEmpty) return;
    _runGitAction(op, {'message': message});
    _commitController.clear();
  }

  /// Dispatch a git op and surface a short toast so the user gets feedback that
  /// the action fired (the host replies asynchronously with a fresh status).
  void _runGitAction(String op, Map<String, dynamic> args) {
    widget.onAction(op, args);
    final label = _opToast(op);
    if (label != null) _toast(label);
  }

  void _toast(String message) {
    final messenger = ScaffoldMessenger.maybeOf(context);
    if (messenger == null) return;
    messenger.clearSnackBars();
    messenger.showSnackBar(
      SnackBar(
        content: Text(message),
        duration: const Duration(milliseconds: 1400),
        behavior: SnackBarBehavior.floating,
        backgroundColor: PadColors.card,
      ),
    );
  }

  String? _opToast(String op) {
    switch (op) {
      case 'fetch':
        return '正在获取…';
      case 'pull':
        return '正在拉取…';
      case 'push':
      case 'push_remote':
      case 'push_remote_branch':
        return '正在推送…';
      case 'force_push':
        return '正在强制推送…';
      case 'sync':
        return '正在同步…';
      case 'undo_last_commit':
        return '已撤销最近一次提交';
      case 'amend':
        return '已修改提交说明';
      case 'checkout_branch':
      case 'checkout_remote_branch':
        return '正在切换分支…';
      case 'merge_branch':
        return '正在合并…';
      case 'delete_branch':
        return '已删除分支';
      case 'create_branch':
        return '已创建分支';
      case 'add_remote':
        return '已添加远程仓库';
      case 'remove_remote':
        return '已移除远程仓库';
      case 'commit':
        return '已提交';
      case 'commit_push':
        return '已提交并推送';
      case 'commit_sync':
        return '已提交并同步';
      default:
        return null;
    }
  }

  /// Git panel header: title, a sync shortcut, and the branch/actions menu.
  Widget _buildGitHeader(BuildContext context) {
    return Container(
      height: 48,
      color: PadColors.header,
      padding: const EdgeInsets.symmetric(horizontal: 14),
      child: Row(
        children: [
          Expanded(
            child: Text(
              'Git',
              style: TextStyle(
                color: PadColors.textPrimary,
                fontSize: 15,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          if (_syncing)
            SizedBox(
              width: 32,
              height: 32,
              child: Center(
                child: SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(
                    strokeWidth: 2,
                    color: PadColors.textSubtle,
                  ),
                ),
              ),
            )
          else
            _GitHeaderButton(
              icon: Icons.sync_rounded,
              onTap: _sync,
            ),
          const SizedBox(width: 2),
          _GitHeaderButton(
            icon: Icons.more_horiz_rounded,
            onTap: () => _openBranchMenu(context),
          ),
        ],
      ),
    );
  }

  /// Branch name + commit message field + commit button group (commit / commit
  /// & push / commit & sync).
  Widget _buildBranchCommitCard(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final branch = widget.gitStatus?.branch.trim().isNotEmpty == true
        ? widget.gitStatus!.branch.trim()
        : '—';
    // Commit only when there's a message and something to commit; otherwise the
    // button greys out instead of silently swallowing the tap.
    final canCommit =
        _commitController.text.trim().isNotEmpty && _filesFromStatus().isNotEmpty;
    return _ToolCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            branch,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              color: PadColors.textPrimary,
              fontSize: 14,
              fontWeight: FontWeight.w800,
            ),
          ),
          const SizedBox(height: 12),
          Container(
            decoration: BoxDecoration(
              color: PadColors.panelTrack,
              borderRadius: BorderRadius.circular(10),
            ),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
            child: TextField(
              controller: _commitController,
              minLines: 2,
              maxLines: 4,
              style: TextStyle(
                color: PadColors.textPrimary,
                fontSize: 13,
              ),
              decoration: InputDecoration(
                isCollapsed: true,
                contentPadding: EdgeInsets.symmetric(vertical: 8),
                border: InputBorder.none,
                hintText: '提交说明',
                hintStyle: TextStyle(color: PadColors.textSubtle, fontSize: 13),
              ),
            ),
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: _MiniActionButton(
                  icon: Icons.check_rounded,
                  label: '提交',
                  onTap: canCommit ? () => _commit('commit') : null,
                ),
              ),
              const SizedBox(width: 8),
              _CommitMenuButton(
                accent: accent,
                enabled: canCommit,
                onCommitPush: () => _commit('commit_push'),
                onCommitSync: () => _commit('commit_sync'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  void _openBranchMenu(BuildContext context) {
    final status = widget.gitStatus;
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: PadColors.panel,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (sheetContext) => _GitBranchMenu(
        status: status,
        onAction: (op, args) {
          Navigator.of(sheetContext).pop();
          _runGitAction(op, args);
        },
        onCreateBranch: () {
          Navigator.of(sheetContext).pop();
          // Defer so the sheet finishes popping before the dialog opens
          // (showing a dialog mid-pop throws a navigator-lock red screen).
          Future.microtask(_promptCreateBranch);
        },
        onAmend: () {
          Navigator.of(sheetContext).pop();
          Future.microtask(_promptAmend);
        },
        onAddRemote: () {
          Navigator.of(sheetContext).pop();
          Future.microtask(_promptAddRemote);
        },
        onCopyRemoteUrl: (url) {
          Navigator.of(sheetContext).pop();
          Clipboard.setData(ClipboardData(text: url));
          _toast('已复制远程地址');
        },
      ),
    );
  }

  Future<void> _promptAddRemote() async {
    if (!mounted) return;
    final nameController = TextEditingController(text: 'origin');
    final urlController = TextEditingController();
    final accent = Theme.of(context).colorScheme.secondary;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: PadColors.panel,
        title: Text(
          '添加远程仓库',
          style: TextStyle(color: PadColors.textPrimary, fontSize: 16),
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: nameController,
              autofocus: true,
              style: TextStyle(color: PadColors.textPrimary),
              decoration: InputDecoration(
                hintText: '名称 (如 origin)',
                hintStyle: TextStyle(color: PadColors.textSubtle),
              ),
            ),
            const SizedBox(height: 8),
            TextField(
              controller: urlController,
              style: TextStyle(color: PadColors.textPrimary),
              decoration: InputDecoration(
                hintText: '远程地址 URL',
                hintStyle: TextStyle(color: PadColors.textSubtle),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text('取消', style: TextStyle(color: PadColors.textMuted)),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text('添加', style: TextStyle(color: accent)),
          ),
        ],
      ),
    );
    final name = nameController.text.trim();
    final url = urlController.text.trim();
    Future.delayed(const Duration(milliseconds: 400), () {
      nameController.dispose();
      urlController.dispose();
    });
    if (!mounted) return;
    if (confirmed == true && name.isNotEmpty && url.isNotEmpty) {
      widget.onAction('add_remote', {'name': name, 'url': url});
    }
  }

  Future<void> _promptAmend() async {
    if (!mounted) return;
    final controller = TextEditingController();
    final accent = Theme.of(context).colorScheme.secondary;
    final message = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: PadColors.panel,
        title: Text(
          '修改最近一次提交说明',
          style: TextStyle(color: PadColors.textPrimary, fontSize: 16),
        ),
        content: TextField(
          controller: controller,
          autofocus: true,
          maxLines: 3,
          style: TextStyle(color: PadColors.textPrimary),
          decoration: InputDecoration(
            hintText: '新的提交说明',
            hintStyle: TextStyle(color: PadColors.textSubtle),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text('取消', style: TextStyle(color: PadColors.textMuted)),
          ),
          TextButton(
            onPressed: () =>
                Navigator.of(dialogContext).pop(controller.text.trim()),
            child: Text('确定', style: TextStyle(color: accent)),
          ),
        ],
      ),
    );
    // Dispose after the dialog's exit animation so the TextField (still painting
    // while it animates out) never touches a disposed controller.
    Future.delayed(const Duration(milliseconds: 400), controller.dispose);
    if (!mounted) return;
    if (message != null && message.isNotEmpty) {
      widget.onAction('amend', {'message': message});
    }
  }

  Future<void> _promptCreateBranch() async {
    if (!mounted) return;
    final controller = TextEditingController();
    final accent = Theme.of(context).colorScheme.secondary;
    final name = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: PadColors.panel,
        title: Text(
          '新建分支',
          style: TextStyle(color: PadColors.textPrimary, fontSize: 16),
        ),
        content: TextField(
          controller: controller,
          autofocus: true,
          style: TextStyle(color: PadColors.textPrimary),
          decoration: InputDecoration(
            hintText: '分支名称',
            hintStyle: TextStyle(color: PadColors.textSubtle),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text('取消', style: TextStyle(color: PadColors.textMuted)),
          ),
          TextButton(
            onPressed: () =>
                Navigator.of(dialogContext).pop(controller.text.trim()),
            child: Text('创建', style: TextStyle(color: accent)),
          ),
        ],
      ),
    );
    Future.delayed(const Duration(milliseconds: 400), controller.dispose);
    if (!mounted) return;
    if (name != null && name.isNotEmpty) {
      widget.onAction('create_branch', {'branch': name, 'checkout': true});
    }
  }

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
    final snapshot = PadDirectorySnapshot<_GitPreviewFile>.from(
      currentPath,
      files,
    );
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
        : padParentPath(currentPath);

    return PadPanelSurface(
      width: PadMetrics.rightColumnWidth,
      child: Column(
        children: [
          _buildGitHeader(context),
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
                _buildBranchCommitCard(context),
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
                      path: padCurrentDirPath(currentPath, parentPath),
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
                          path: padCurrentDirPath(currentPath, folder.path),
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
                      icon: padFileIcon(file.status),
                      iconColor: _selectedPaths.contains(file.path)
                          ? Theme.of(context).colorScheme.secondary
                          : PadColors.textMuted,
                      name: file.name,
                      path: padCurrentDirPath(currentPath, file.path),
                      trailing: PadStatusTag(
                        label: file.status,
                        color: padStatusColor(
                          file.status,
                          Theme.of(context).colorScheme.secondary,
                        ),
                      ),
                      selected: _selectedPaths.contains(file.path),
                      // Tap opens the file's diff; stage/unstage is via
                      // long-press selection + the footer actions.
                      onTap: () => widget.onOpenFile(file.path),
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

class _GitPreviewFile implements PadTreeEntry {
  const _GitPreviewFile({
    required this.section,
    required this.status,
    required this.path,
  });

  final String section;
  @override
  final String status;
  @override
  final String path;
}

const _gitRootPath = '';

class _SshProfileRow extends StatelessWidget {
  const _SshProfileRow({
    required this.profile,
    required this.expanded,
    required this.onTap,
    required this.onEdit,
    required this.onDelete,
  });

  final RemoteSshProfile profile;
  final bool expanded;
  final VoidCallback onTap;
  final VoidCallback onEdit;
  final VoidCallback onDelete;

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
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: _MiniActionButton(
                    icon: Icons.edit_rounded,
                    label: '编辑',
                    onTap: onEdit,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: _MiniActionButton(
                    icon: Icons.delete_outline_rounded,
                    label: '删除',
                    danger: true,
                    onTap: onDelete,
                  ),
                ),
              ],
            ),
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
          Icon(
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
              style: TextStyle(
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
      decoration: BoxDecoration(
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
                      style: TextStyle(
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
    this.onTap,
  });

  final Widget child;
  final bool selected;
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
          style: TextStyle(
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
          style: TextStyle(color: PadColors.textSubtle, fontSize: 11),
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
              style: TextStyle(
                color: PadColors.textMuted,
                fontSize: 11.5,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          Text(
            value,
            style: TextStyle(
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
  const _MiniActionButton({
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
    final enabled = onTap != null;
    final color = !enabled
        ? PadColors.textSubtle
        : danger
        ? PadColors.danger
        : Theme.of(context).colorScheme.secondary;
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Container(
        height: 34,
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: color.withValues(alpha: enabled ? 0.12 : 0.08),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 15, color: color),
            const SizedBox(width: 6),
            Text(
              label,
              style: TextStyle(
                color: color,
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

/// Tappable icon button used in the git panel header (sync / branch menu).
class _GitHeaderButton extends StatelessWidget {
  const _GitHeaderButton({required this.icon, required this.onTap, this.color});

  final IconData icon;
  final VoidCallback onTap;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      borderRadius: BorderRadius.circular(8),
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTap: onTap,
        child: SizedBox(
          width: 32,
          height: 32,
          child: Icon(icon, size: 18, color: color ?? PadColors.textSubtle),
        ),
      ),
    );
  }
}

/// The dropdown half of the commit button group: commit & push / commit & sync.
class _CommitMenuButton extends StatelessWidget {
  const _CommitMenuButton({
    required this.accent,
    required this.onCommitPush,
    required this.onCommitSync,
    this.enabled = true,
  });

  final Color accent;
  final VoidCallback onCommitPush;
  final VoidCallback onCommitSync;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final color = enabled ? accent : PadColors.textSubtle;
    return Container(
      height: 34,
      width: 38,
      decoration: BoxDecoration(
        color: color.withValues(alpha: enabled ? 0.12 : 0.08),
        borderRadius: BorderRadius.circular(8),
      ),
      child: PopupMenuButton<String>(
        padding: EdgeInsets.zero,
        position: PopupMenuPosition.under,
        color: PadColors.panel,
        enabled: enabled,
        icon: Icon(Icons.arrow_drop_down_rounded, color: color, size: 22),
        onSelected: (value) =>
            value == 'push' ? onCommitPush() : onCommitSync(),
        itemBuilder: (context) => [
          PopupMenuItem(
            value: 'push',
            child: Text(
              '提交并推送',
              style: TextStyle(color: PadColors.textPrimary, fontSize: 13),
            ),
          ),
          PopupMenuItem(
            value: 'sync',
            child: Text(
              '提交并同步',
              style: TextStyle(color: PadColors.textPrimary, fontSize: 13),
            ),
          ),
        ],
      ),
    );
  }
}

/// Branch / actions menu opened from the git panel header "...".
/// One row in a git menu panel: a leaf action (`onTap`), a drill-in (`child`),
/// a divider, or an empty-state note. Mirrors a desktop menu item / submenu.
class _GitMenuEntry {
  _GitMenuEntry({
    this.icon,
    this.label,
    this.onTap,
    this.child,
    this.danger = false,
    this.current = false,
    this.enabled = true,
    this.isDivider = false,
    this.note,
  });

  _GitMenuEntry.divider() : this(isDivider: true);
  _GitMenuEntry.note(String text) : this(note: text);

  final IconData? icon;
  final String? label;
  final VoidCallback? onTap;
  final _GitMenuPanel? child;
  final bool danger;
  final bool current;
  final bool enabled;
  final bool isDivider;
  final String? note;
}

/// A single level of the git menu — a title plus its rows.
class _GitMenuPanel {
  _GitMenuPanel({this.title, required this.entries});

  final String? title;
  final List<_GitMenuEntry> entries;
}

/// Branch / actions menu opened from the git panel header "...". A drill-down
/// bottom sheet that mirrors the desktop git menu 1:1 (本地分支 / 合并到当前分支 /
/// 远程仓库 / 远程分支 / 推送到… as submenus, each with its per-item actions).
class _GitBranchMenu extends StatefulWidget {
  const _GitBranchMenu({
    required this.status,
    required this.onAction,
    required this.onCreateBranch,
    required this.onAmend,
    required this.onAddRemote,
    required this.onCopyRemoteUrl,
  });

  final RemoteGitStatusInfo? status;
  final void Function(String op, Map<String, dynamic> args) onAction;
  final VoidCallback onCreateBranch;
  final VoidCallback onAmend;
  final VoidCallback onAddRemote;
  final ValueChanged<String> onCopyRemoteUrl;

  @override
  State<_GitBranchMenu> createState() => _GitBranchMenuState();
}

class _GitBranchMenuState extends State<_GitBranchMenu> {
  // Navigation path of drill-in indices from the root panel (empty = root).
  final List<int> _path = [];

  void _act(String op, Map<String, dynamic> args) => widget.onAction(op, args);

  /// Build the whole menu tree from the current status. Rebuilt identically on
  /// every frame, so `_path` indices stay stable while we drill in / out.
  _GitMenuPanel _buildRoot() {
    final status = widget.status;
    final all = status?.branches ?? const <RemoteGitBranch>[];
    final nonCurrent = all.where((branch) => !branch.isCurrent).toList();
    final remoteBranches = status?.remoteBranches ?? const <String>[];
    final remoteRepos = status?.remotes ?? const <RemoteGitRemote>[];

    _GitMenuPanel branchActions(RemoteGitBranch branch) {
      final usable = !branch.isCurrent;
      return _GitMenuPanel(
        title: branch.name,
        entries: [
          _GitMenuEntry(
            icon: Icons.check_rounded,
            label: '切换分支',
            enabled: usable,
            onTap: () => _act('checkout_branch', {'branch': branch.name}),
          ),
          _GitMenuEntry.divider(),
          _GitMenuEntry(
            icon: Icons.merge_rounded,
            label: '合并到当前分支',
            enabled: usable,
            onTap: () => _act('merge_branch', {'branch': branch.name}),
          ),
          _GitMenuEntry(
            icon: Icons.call_merge_rounded,
            label: '压缩合并分支',
            enabled: usable,
            onTap: () => _act('merge_branch', {
              'branch': branch.name,
              'squash': true,
            }),
          ),
          _GitMenuEntry.divider(),
          _GitMenuEntry(
            icon: Icons.delete_outline_rounded,
            label: '删除本地分支',
            danger: true,
            enabled: usable,
            onTap: () => _act('delete_branch', {'branch': branch.name}),
          ),
        ],
      );
    }

    final localPanel = _GitMenuPanel(
      title: '本地分支',
      entries: all.isEmpty
          ? [_GitMenuEntry.note('没有本地分支')]
          : [
              for (final branch in all)
                _GitMenuEntry(
                  icon: branch.isCurrent
                      ? Icons.check_rounded
                      : Icons.alt_route_rounded,
                  label: branch.name,
                  current: branch.isCurrent,
                  child: branchActions(branch),
                ),
            ],
    );

    final mergePanel = _GitMenuPanel(
      title: '合并到当前分支',
      entries: nonCurrent.isEmpty
          ? [_GitMenuEntry.note('没有可合并分支')]
          : [
              for (final branch in nonCurrent)
                _GitMenuEntry(
                  icon: Icons.merge_rounded,
                  label: branch.name,
                  onTap: () => _act('merge_branch', {'branch': branch.name}),
                ),
            ],
    );

    _GitMenuPanel remoteActions(RemoteGitRemote remote) => _GitMenuPanel(
      title: remote.name,
      entries: [
        _GitMenuEntry(
          icon: Icons.link_rounded,
          label: '复制 URL',
          onTap: () => widget.onCopyRemoteUrl(remote.url),
        ),
        _GitMenuEntry.divider(),
        _GitMenuEntry(
          icon: Icons.delete_outline_rounded,
          label: '移除远程仓库',
          danger: true,
          onTap: () => _act('remove_remote', {'name': remote.name}),
        ),
      ],
    );

    final remotesPanel = _GitMenuPanel(
      title: '远程仓库',
      entries: [
        _GitMenuEntry(
          icon: Icons.add_rounded,
          label: '添加远程仓库',
          onTap: widget.onAddRemote,
        ),
        if (remoteRepos.isEmpty)
          _GitMenuEntry.note('没有远程仓库')
        else ...[
          _GitMenuEntry.divider(),
          for (final remote in remoteRepos)
            _GitMenuEntry(
              icon: Icons.dns_rounded,
              label: remote.name,
              child: remoteActions(remote),
            ),
        ],
      ],
    );

    _GitMenuPanel remoteBranchActions(String branch) => _GitMenuPanel(
      title: branch,
      entries: [
        _GitMenuEntry(
          icon: Icons.south_rounded,
          label: '检出为本地分支',
          onTap: () => _act('checkout_remote_branch', {'remoteBranch': branch}),
        ),
        _GitMenuEntry(
          icon: Icons.north_rounded,
          label: '推送到该分支',
          onTap: () =>
              _act('push_remote_branch', {'remoteBranch': branch}),
        ),
      ],
    );

    final remoteBranchesPanel = _GitMenuPanel(
      title: '远程分支',
      entries: [
        _GitMenuEntry(
          icon: Icons.refresh_rounded,
          label: '刷新远程分支',
          onTap: () => _act('fetch', const {}),
        ),
        if (remoteBranches.isEmpty)
          _GitMenuEntry.note('没有远程分支')
        else ...[
          _GitMenuEntry.divider(),
          for (final branch in remoteBranches)
            _GitMenuEntry(
              icon: Icons.cloud_outlined,
              label: branch,
              child: remoteBranchActions(branch),
            ),
        ],
      ],
    );

    final pushToPanel = _GitMenuPanel(
      title: '推送到…',
      entries: remoteRepos.isEmpty
          ? [_GitMenuEntry.note('没有远程仓库')]
          : [
              for (final remote in remoteRepos)
                _GitMenuEntry(
                  icon: Icons.north_east_rounded,
                  label: remote.name,
                  onTap: () => _act('push_remote', {'remote': remote.name}),
                ),
            ],
    );

    return _GitMenuPanel(
      entries: [
        _GitMenuEntry(
          icon: Icons.add_rounded,
          label: '新建分支',
          onTap: widget.onCreateBranch,
        ),
        _GitMenuEntry.divider(),
        _GitMenuEntry(
          icon: Icons.account_tree_rounded,
          label: '本地分支',
          child: localPanel,
        ),
        _GitMenuEntry(
          icon: Icons.merge_rounded,
          label: '合并到当前分支',
          child: mergePanel,
        ),
        _GitMenuEntry(
          icon: Icons.cloud_rounded,
          label: '远程仓库',
          child: remotesPanel,
        ),
        _GitMenuEntry(
          icon: Icons.cloud_outlined,
          label: '远程分支',
          child: remoteBranchesPanel,
        ),
        _GitMenuEntry.divider(),
        _GitMenuEntry(
          icon: Icons.download_rounded,
          label: '获取',
          onTap: () => _act('fetch', const {}),
        ),
        _GitMenuEntry(
          icon: Icons.south_rounded,
          label: '拉取',
          onTap: () => _act('pull', const {}),
        ),
        _GitMenuEntry(
          icon: Icons.north_rounded,
          label: '推送',
          onTap: () => _act('push', const {}),
        ),
        _GitMenuEntry(
          icon: Icons.north_east_rounded,
          label: '推送到…',
          child: pushToPanel,
        ),
        _GitMenuEntry.divider(),
        _GitMenuEntry(
          icon: Icons.warning_amber_rounded,
          label: '强制推送',
          onTap: () => _act('force_push', const {}),
        ),
        _GitMenuEntry(
          icon: Icons.undo_rounded,
          label: '撤销最近一次提交',
          onTap: () => _act('undo_last_commit', const {}),
        ),
        _GitMenuEntry(
          icon: Icons.edit_note_rounded,
          label: '修改最近一次提交说明',
          onTap: widget.onAmend,
        ),
      ],
    );
  }

  /// Walk `_path` from the root to the panel currently on screen.
  _GitMenuPanel _currentPanel(_GitMenuPanel root) {
    var panel = root;
    for (final index in _path) {
      final entry = panel.entries[index];
      final child = entry.child;
      if (child == null) break;
      panel = child;
    }
    return panel;
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final root = _buildRoot();
    final panel = _currentPanel(root);
    final atRoot = _path.isEmpty;

    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (atRoot)
            Center(
              child: Container(
                margin: const EdgeInsets.symmetric(vertical: 10),
                width: 36,
                height: 4,
                decoration: BoxDecoration(
                  color: PadColors.border,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
            )
          else
            InkWell(
              onTap: () => setState(_path.removeLast),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(8, 8, 16, 8),
                child: Row(
                  children: [
                    Icon(
                      Icons.chevron_left_rounded,
                      size: 22,
                      color: PadColors.textSecondary,
                    ),
                    const SizedBox(width: 2),
                    Expanded(
                      child: Text(
                        panel.title ?? '',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          color: PadColors.textPrimary,
                          fontSize: 14,
                          fontWeight: FontWeight.w800,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          Flexible(
            child: ListView(
              shrinkWrap: true,
              padding: const EdgeInsets.only(bottom: 8),
              children: [
                for (var index = 0; index < panel.entries.length; index++)
                  _buildEntryRow(panel.entries[index], index, accent),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildEntryRow(_GitMenuEntry entry, int index, Color accent) {
    if (entry.isDivider) {
      return Divider(
        height: 9,
        thickness: 0.5,
        color: PadColors.border,
        indent: 16,
        endIndent: 16,
      );
    }
    if (entry.note != null) {
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 16),
        child: Text(
          entry.note!,
          textAlign: TextAlign.center,
          style: TextStyle(color: PadColors.textSubtle, fontSize: 13),
        ),
      );
    }
    final isDrill = entry.child != null;
    final color = entry.danger ? PadColors.danger : accent;
    final enabled = entry.enabled && (isDrill || entry.onTap != null);
    final labelColor = !enabled
        ? PadColors.textSubtle
        : (entry.danger ? PadColors.danger : PadColors.textPrimary);
    return InkWell(
      onTap: !enabled
          ? null
          : () {
              if (isDrill) {
                setState(() => _path.add(index));
              } else {
                entry.onTap!();
              }
            },
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Row(
          children: [
            Icon(
              entry.icon,
              size: 18,
              color: enabled ? color : PadColors.textSubtle,
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                entry.label ?? '',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: labelColor,
                  fontSize: 14,
                  fontWeight: entry.current ? FontWeight.w800 : FontWeight.w600,
                ),
              ),
            ),
            if (isDrill)
              Icon(
                Icons.chevron_right_rounded,
                size: 20,
                color: PadColors.textSubtle,
              ),
          ],
        ),
      ),
    );
  }
}

/// Add / edit form for a saved SSH profile. Secrets are never pre-filled (the
/// host doesn't expose them); on edit, re-enter the credential to change it.
class _SshFormSheet extends StatefulWidget {
  const _SshFormSheet({required this.existing, required this.onSubmit});

  final RemoteSshProfile? existing;
  final void Function(Map<String, dynamic> fields) onSubmit;

  @override
  State<_SshFormSheet> createState() => _SshFormSheetState();
}

class _SshFormSheetState extends State<_SshFormSheet> {
  late final TextEditingController _name;
  late final TextEditingController _host;
  late final TextEditingController _port;
  late final TextEditingController _user;
  late final TextEditingController _password;
  late final TextEditingController _keyPath;
  late final TextEditingController _passphrase;
  String _kind = 'password';

  @override
  void initState() {
    super.initState();
    final existing = widget.existing;
    // endpoint is "username@host:port" — parse it back for editing.
    var user = '';
    var host = '';
    var port = '22';
    if (existing != null) {
      var rest = existing.endpoint;
      final at = rest.indexOf('@');
      if (at >= 0) {
        user = rest.substring(0, at);
        rest = rest.substring(at + 1);
      }
      final colon = rest.lastIndexOf(':');
      if (colon >= 0) {
        host = rest.substring(0, colon);
        port = rest.substring(colon + 1);
      } else {
        host = rest;
      }
      final cred = existing.credential.toLowerCase();
      if (cred.contains('privatekey') || cred == 'key') {
        _kind = 'privateKey';
      } else if (cred.contains('password')) {
        _kind = 'password';
      } else {
        _kind = 'none';
      }
    }
    _name = TextEditingController(text: existing?.name ?? '');
    _host = TextEditingController(text: host);
    _port = TextEditingController(text: port);
    _user = TextEditingController(text: user);
    _password = TextEditingController();
    _keyPath = TextEditingController();
    _passphrase = TextEditingController();
  }

  @override
  void dispose() {
    _name.dispose();
    _host.dispose();
    _port.dispose();
    _user.dispose();
    _password.dispose();
    _keyPath.dispose();
    _passphrase.dispose();
    super.dispose();
  }

  void _submit() {
    final fields = <String, dynamic>{
      if (widget.existing != null) 'id': widget.existing!.id,
      'name': _name.text.trim(),
      'host': _host.text.trim(),
      'port': int.tryParse(_port.text.trim()) ?? 22,
      'username': _user.text.trim(),
      'credentialKind': _kind,
      if (_kind == 'password' && _password.text.isNotEmpty)
        'password': _password.text,
      if (_kind == 'privateKey') ...{
        if (_keyPath.text.trim().isNotEmpty) 'privateKeyPath': _keyPath.text.trim(),
        if (_passphrase.text.isNotEmpty) 'keyPassphrase': _passphrase.text,
      },
    };
    if (fields['name'] == '' || fields['host'] == '' || fields['username'] == '') {
      return;
    }
    widget.onSubmit(fields);
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final bottomInset = MediaQuery.viewInsetsOf(context).bottom;
    return Padding(
      padding: EdgeInsets.only(bottom: bottomInset),
      child: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                widget.existing == null ? '添加 SSH 连接' : '编辑 SSH 连接',
                style: TextStyle(
                  color: PadColors.textPrimary,
                  fontSize: 16,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 14),
              _field(_name, '名称'),
              _field(_host, '主机 (host)'),
              _field(_port, '端口', keyboardType: TextInputType.number),
              _field(_user, '用户名'),
              const SizedBox(height: 6),
              _kindSelector(accent),
              const SizedBox(height: 6),
              if (_kind == 'password')
                _field(_password, '密码', obscure: true)
              else if (_kind == 'privateKey') ...[
                _field(_keyPath, '私钥路径 (主机上,如 ~/.ssh/id_ed25519)'),
                _field(_passphrase, '私钥口令 (可选)', obscure: true),
              ] else
                Padding(
                  padding: EdgeInsets.only(bottom: 10),
                  child: Text(
                    '使用主机的 SSH Agent,无需填写凭据',
                    style: TextStyle(color: PadColors.textSubtle, fontSize: 12.5),
                  ),
                ),
              const SizedBox(height: 16),
              Row(
                children: [
                  Expanded(
                    child: TextButton(
                      onPressed: () => Navigator.of(context).pop(),
                      child: Text(
                        '取消',
                        style: TextStyle(color: PadColors.textMuted),
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: FilledButton(
                      style: FilledButton.styleFrom(backgroundColor: accent),
                      onPressed: _submit,
                      child: const Text('保存'),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _field(
    TextEditingController controller,
    String hint, {
    bool obscure = false,
    TextInputType? keyboardType,
  }) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: TextField(
        controller: controller,
        obscureText: obscure,
        keyboardType: keyboardType,
        style: TextStyle(color: PadColors.textPrimary, fontSize: 14),
        decoration: InputDecoration(
          isDense: true,
          filled: true,
          fillColor: PadColors.panelTrack,
          hintText: hint,
          hintStyle: TextStyle(color: PadColors.textSubtle, fontSize: 13),
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(10),
            borderSide: BorderSide.none,
          ),
          contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
        ),
      ),
    );
  }

  Widget _kindSelector(Color accent) {
    const kinds = [
      ('password', '密码'),
      ('privateKey', '私钥'),
      ('none', '无'),
    ];
    return Row(
      children: [
        for (final (value, label) in kinds)
          Expanded(
            child: Padding(
              padding: const EdgeInsets.only(right: 6),
              child: InkWell(
                borderRadius: BorderRadius.circular(8),
                onTap: () => setState(() => _kind = value),
                child: Container(
                  height: 34,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: _kind == value
                        ? accent.withValues(alpha: 0.16)
                        : PadColors.panelTrack,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Text(
                    label,
                    style: TextStyle(
                      color: _kind == value ? accent : PadColors.textMuted,
                      fontSize: 12.5,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}
