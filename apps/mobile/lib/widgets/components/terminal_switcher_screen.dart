import 'package:flutter/material.dart';

import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../theme/app_theme.dart';
import 'swipe_list_tile.dart';

enum TerminalSwitcherSection { splits, tabs, worktrees }

class TerminalSwitcherScreen extends StatefulWidget {
  const TerminalSwitcherScreen({
    super.key,
    required this.topInset,
    required this.bottomInset,
    required this.terminals,
    required this.worktrees,
    required this.activeTerminalId,
    required this.selectedProjectId,
    required this.selectedWorktreeId,
    required this.switchingWorktreeId,
    required this.loadingWorktrees,
    required this.creatingSplit,
    required this.creatingTab,
    required this.creatingWorktree,
    required this.onBack,
    required this.onSelectTerminal,
    required this.onCreateSplit,
    required this.onCreateTab,
    required this.onCloseTerminal,
    required this.onSelectWorktree,
    required this.onCreateWorktree,
    required this.onMergeWorktree,
    required this.onDeleteWorktree,
    required this.onOpenWorktrees,
    required this.onRefreshWorktrees,
    required this.onRefreshTerminals,
  });

  final double topInset;
  final double bottomInset;
  final List<TerminalInfo> terminals;
  final List<RemoteWorktreeInfo> worktrees;
  final String? activeTerminalId;
  final String? selectedProjectId;
  final String? selectedWorktreeId;
  final String? switchingWorktreeId;
  final bool loadingWorktrees;
  final bool creatingSplit;
  final bool creatingTab;
  final bool creatingWorktree;
  final VoidCallback onBack;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onCreateSplit;
  final VoidCallback onCreateTab;
  final ValueChanged<TerminalInfo> onCloseTerminal;
  final ValueChanged<RemoteWorktreeInfo> onSelectWorktree;
  final VoidCallback onCreateWorktree;
  final ValueChanged<RemoteWorktreeInfo> onMergeWorktree;
  final ValueChanged<RemoteWorktreeInfo> onDeleteWorktree;
  final VoidCallback onOpenWorktrees;
  final VoidCallback onRefreshWorktrees;
  final VoidCallback onRefreshTerminals;

  @override
  State<TerminalSwitcherScreen> createState() => _TerminalSwitcherScreenState();
}

class _TerminalSwitcherScreenState extends State<TerminalSwitcherScreen> {
  TerminalSwitcherSection _section = TerminalSwitcherSection.splits;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final accent = Theme.of(context).colorScheme.secondary;
    final splits = widget.terminals
        .where((item) => _terminalLayoutKind(item) == 'split')
        .toList();
    final tabs = widget.terminals
        .where((item) => _terminalLayoutKind(item) == 'tab')
        .toList();
    final scopedWorktrees = widget.selectedProjectId == null
        ? widget.worktrees
        : widget.worktrees
              .where((item) => item.projectId == widget.selectedProjectId)
              .toList(growable: false);
    return ColoredBox(
      color: AppColors.bgBase,
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          AppSpacing.l,
          widget.topInset + AppSpacing.m,
          AppSpacing.l,
          widget.bottomInset + AppSpacing.l,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                _IconButton(
                  icon: Icons.arrow_back_ios_new_rounded,
                  onTap: widget.onBack,
                ),
                const SizedBox(width: AppSpacing.m),
                Expanded(
                  child: Text(
                    prefs.t('switcher.title'),
                    style: TextStyle(
                      color: AppColors.textPrimary,
                      fontSize: 20,
                      fontWeight: FontWeight.w800,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: AppSpacing.l),
            _SectionTabs(
              value: _section,
              onChanged: (next) {
                setState(() => _section = next);
                if (next == TerminalSwitcherSection.worktrees) {
                  widget.onOpenWorktrees();
                }
              },
            ),
            const SizedBox(height: AppSpacing.l),
            Expanded(
              child: RefreshIndicator(
                color: accent,
                backgroundColor: AppColors.bgSurface,
                onRefresh: () async {
                  if (_section == TerminalSwitcherSection.worktrees) {
                    widget.onRefreshWorktrees();
                  } else {
                    widget.onRefreshTerminals();
                  }
                  // Brief delay so the pull-to-refresh spinner reads as an
                  // action; the host's reply arrives asynchronously.
                  await Future<void>.delayed(const Duration(milliseconds: 600));
                },
                child: switch (_section) {
                TerminalSwitcherSection.splits => _TerminalList(
                  listKey: 'split',
                  terminals: splits,
                  activeTerminalId: widget.activeTerminalId,
                  addLabel: prefs.t('switcher.newSplit'),
                  itemPrefix: prefs.t('switcher.split'),
                  creating: widget.creatingSplit,
                  onAdd: widget.onCreateSplit,
                  onSelect: widget.onSelectTerminal,
                  onClose: widget.onCloseTerminal,
                ),
                TerminalSwitcherSection.tabs => _TerminalList(
                  listKey: 'tab',
                  terminals: tabs,
                  activeTerminalId: widget.activeTerminalId,
                  addLabel: prefs.t('switcher.newTab'),
                  itemPrefix: prefs.t('switcher.tab'),
                  creating: widget.creatingTab,
                  onAdd: widget.onCreateTab,
                  onSelect: widget.onSelectTerminal,
                  onClose: widget.onCloseTerminal,
                ),
                TerminalSwitcherSection.worktrees => _WorktreeList(
                  accent: accent,
                  loading: widget.loadingWorktrees,
                  creating: widget.creatingWorktree,
                  worktrees: scopedWorktrees,
                  selectedId: widget.selectedWorktreeId,
                  switchingId: widget.switchingWorktreeId,
                  onSelect: widget.onSelectWorktree,
                  onCreate: widget.onCreateWorktree,
                  onMerge: widget.onMergeWorktree,
                  onDelete: widget.onDeleteWorktree,
                ),
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SectionTabs extends StatelessWidget {
  const _SectionTabs({required this.value, required this.onChanged});

  final TerminalSwitcherSection value;
  final ValueChanged<TerminalSwitcherSection> onChanged;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    return Container(
      height: 40,
      decoration: BoxDecoration(
        color: AppColors.bgSurface,
        borderRadius: BorderRadius.circular(AppRadius.md),
      ),
      child: Row(
        children: [
          _Segment(
            label: prefs.t('switcher.splits'),
            active: value == TerminalSwitcherSection.splits,
            onTap: () => onChanged(TerminalSwitcherSection.splits),
          ),
          _Segment(
            label: prefs.t('switcher.tabs'),
            active: value == TerminalSwitcherSection.tabs,
            onTap: () => onChanged(TerminalSwitcherSection.tabs),
          ),
          _Segment(
            label: prefs.t('switcher.worktrees'),
            active: value == TerminalSwitcherSection.worktrees,
            onTap: () => onChanged(TerminalSwitcherSection.worktrees),
          ),
        ],
      ),
    );
  }
}

class _Segment extends StatelessWidget {
  const _Segment({
    required this.label,
    required this.active,
    required this.onTap,
  });

  final String label;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return Expanded(
      child: Padding(
        padding: const EdgeInsets.all(4),
        child: Material(
          color: active ? accent.withValues(alpha: 0.16) : Colors.transparent,
          borderRadius: BorderRadius.circular(AppRadius.sm),
          child: InkWell(
            borderRadius: BorderRadius.circular(AppRadius.sm),
            onTap: onTap,
            child: Center(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: active ? accent : AppColors.textMuted,
                  fontSize: 13,
                  fontWeight: active ? FontWeight.w800 : FontWeight.w600,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _TerminalList extends StatelessWidget {
  const _TerminalList({
    required this.listKey,
    required this.terminals,
    required this.activeTerminalId,
    required this.addLabel,
    required this.itemPrefix,
    required this.creating,
    required this.onAdd,
    required this.onSelect,
    required this.onClose,
  });

  final String listKey;
  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final String addLabel;
  final String itemPrefix;
  final bool creating;
  final VoidCallback onAdd;
  final ValueChanged<TerminalInfo> onSelect;
  final ValueChanged<TerminalInfo> onClose;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final accent = Theme.of(context).colorScheme.secondary;
    final itemCount = terminals.length + 1;
    if (terminals.isEmpty) {
      return ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: EdgeInsets.zero,
        children: [
          SwipeListTile(
            key: ValueKey('terminal-switcher-$listKey-add'),
            title: addLabel,
            subtitle: creating ? prefs.t('terminal.creating') : itemPrefix,
            leadingIcon: Icons.add_rounded,
            active: false,
            onTap: creating ? null : onAdd,
            trailing: creating ? _InlineLoader(color: accent) : null,
          ),
        ],
      );
    }
    return ListView.separated(
      physics: const AlwaysScrollableScrollPhysics(),
      padding: EdgeInsets.zero,
      itemCount: itemCount,
      separatorBuilder: (_, _) => const SizedBox(height: AppSpacing.s),
      itemBuilder: (context, index) {
        if (index == terminals.length) {
          return SwipeListTile(
            key: ValueKey('terminal-switcher-$listKey-add'),
            title: addLabel,
            subtitle: creating ? prefs.t('terminal.creating') : itemPrefix,
            leadingIcon: Icons.add_rounded,
            active: false,
            onTap: creating ? null : onAdd,
            trailing: creating ? _InlineLoader(color: accent) : null,
          );
        }
        final terminal = terminals[index];
        final active = terminal.id == activeTerminalId;
        return SwipeListTile(
          key: ValueKey('terminal-switcher-$listKey-terminal-${terminal.id}'),
          title: '$itemPrefix ${index + 1}',
          subtitle: _terminalSubtitle(terminal),
          leadingIcon: Icons.terminal_rounded,
          active: active,
          onTap: () => onSelect(terminal),
          trailing: active
              ? Icon(Icons.check_rounded, color: accent, size: 20)
              : null,
          actions: [
            SwipeListAction(
              label: prefs.t('app.delete'),
              color: AppColors.danger,
              icon: Icons.delete_outline_rounded,
              onTap: () => onClose(terminal),
            ),
          ],
        );
      },
    );
  }
}

class _WorktreeList extends StatelessWidget {
  const _WorktreeList({
    required this.accent,
    required this.loading,
    required this.creating,
    required this.worktrees,
    required this.selectedId,
    required this.switchingId,
    required this.onSelect,
    required this.onCreate,
    required this.onMerge,
    required this.onDelete,
  });

  final Color accent;
  final bool loading;
  final bool creating;
  final List<RemoteWorktreeInfo> worktrees;
  final String? selectedId;
  final String? switchingId;
  final ValueChanged<RemoteWorktreeInfo> onSelect;
  final VoidCallback onCreate;
  final ValueChanged<RemoteWorktreeInfo> onMerge;
  final ValueChanged<RemoteWorktreeInfo> onDelete;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    if (loading && worktrees.isEmpty) {
      return Center(child: CircularProgressIndicator(color: accent));
    }
    if (worktrees.isEmpty) {
      return ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: EdgeInsets.zero,
        children: [
          SwipeListTile(
            key: const ValueKey('terminal-switcher-worktree-add'),
            title: prefs.t('worktree.new'),
            subtitle: creating
                ? prefs.t('worktree.creating')
                : prefs.t('switcher.worktrees'),
            leadingIcon: Icons.add_rounded,
            active: false,
            onTap: creating ? null : onCreate,
            trailing: creating ? _InlineLoader(color: accent) : null,
          ),
        ],
      );
    }
    return ListView.separated(
      physics: const AlwaysScrollableScrollPhysics(),
      padding: EdgeInsets.zero,
      itemCount: worktrees.length + 1,
      separatorBuilder: (_, _) => const SizedBox(height: AppSpacing.s),
      itemBuilder: (context, index) {
        if (index == worktrees.length) {
          return SwipeListTile(
            key: const ValueKey('terminal-switcher-worktree-add'),
            title: prefs.t('worktree.new'),
            subtitle: creating
                ? prefs.t('worktree.creating')
                : prefs.t('switcher.worktrees'),
            leadingIcon: Icons.add_rounded,
            active: false,
            onTap: creating ? null : onCreate,
            trailing: creating ? _InlineLoader(color: accent) : null,
          );
        }
        final item = worktrees[index];
        final active = item.id == selectedId;
        final switching = item.id == switchingId;
        final actions = _worktreeActions(
          context: context,
          item: item,
          accent: accent,
          onMerge: onMerge,
          onDelete: onDelete,
        );
        return SwipeListTile(
          key: ValueKey('terminal-switcher-worktree-${item.id}'),
          title: _worktreeTitle(item),
          subtitle: _worktreeSubtitle(item),
          leadingIcon: Icons.account_tree_outlined,
          active: active,
          onTap: switching ? null : () => onSelect(item),
          trailing: switching
              ? _InlineLoader(color: accent)
              : active
              ? Icon(Icons.check_rounded, color: accent, size: 20)
              : null,
          actions: actions,
        );
      },
    );
  }
}

class _IconButton extends StatelessWidget {
  const _IconButton({required this.icon, required this.onTap});

  final IconData icon;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: AppColors.bgSurface,
      shape: const CircleBorder(),
      child: InkWell(
        customBorder: const CircleBorder(),
        onTap: onTap,
        child: SizedBox(
          width: 40,
          height: 40,
          child: Icon(icon, color: AppColors.textPrimary, size: 18),
        ),
      ),
    );
  }
}

class _InlineLoader extends StatelessWidget {
  const _InlineLoader({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 18,
      height: 18,
      child: CircularProgressIndicator(strokeWidth: 2, color: color),
    );
  }
}

String _terminalLayoutKind(TerminalInfo terminal) {
  final value = terminal.layoutKind.trim().toLowerCase();
  if (value == 'tab') return 'tab';
  return 'split';
}

String _terminalSubtitle(TerminalInfo terminal) {
  final parts = <String>[
    if (terminal.title.trim().isNotEmpty) terminal.title.trim(),
    if (terminal.status?.trim().isNotEmpty == true) terminal.status!.trim(),
  ];
  if (parts.isEmpty) return terminal.id;
  return parts.join(' · ');
}

String _worktreeTitle(RemoteWorktreeInfo worktree) {
  if (worktree.name.isNotEmpty) return worktree.name;
  if (worktree.branch.isNotEmpty) return worktree.branch;
  return worktree.id;
}

String _worktreeSubtitle(RemoteWorktreeInfo worktree) {
  final parts = <String>[
    if (worktree.branch.isNotEmpty) worktree.branch,
    if (worktree.changes > 0) 'Δ${worktree.changes}',
  ];
  if (parts.isNotEmpty) return parts.join(' · ');
  return worktree.path;
}

List<SwipeListAction> _worktreeActions({
  required BuildContext context,
  required RemoteWorktreeInfo item,
  required Color accent,
  required ValueChanged<RemoteWorktreeInfo> onMerge,
  required ValueChanged<RemoteWorktreeInfo> onDelete,
}) {
  if (item.isDefault || item.path.trim().isEmpty) return const [];
  final prefs = AppPreferences.of(context);
  return [
    SwipeListAction(
      label: prefs.t('worktree.merge'),
      color: accent,
      icon: Icons.call_merge_rounded,
      onTap: () => onMerge(item),
    ),
    SwipeListAction(
      label: prefs.t('worktree.remove'),
      color: AppColors.danger,
      icon: Icons.delete_outline_rounded,
      onTap: () => onDelete(item),
    ),
  ];
}
