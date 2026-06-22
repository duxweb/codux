import 'package:flutter/material.dart';

import '../../i18n.dart';
import '../../models/remote_models.dart';
import 'pad_project_picker_modal.dart';
import 'pad_theme.dart';
import 'pad_workspace_shared.dart';

class PadWorkspaceSidebar extends StatelessWidget {
  const PadWorkspaceSidebar({
    super.key,
    required this.project,
    required this.projects,
    required this.selectedProjectId,
    required this.worktrees,
    required this.selectedWorktreeId,
    required this.terminals,
    required this.activeTerminalId,
    required this.aiSessions,
    required this.onOpenSession,
    required this.onRenameSession,
    required this.onDeleteSession,
    required this.onSelectProject,
    required this.onEditProject,
    required this.onAddProject,
    required this.onRemoveProject,
    required this.onSelectWorktree,
    required this.onCreateWorktree,
    required this.onMergeWorktree,
    required this.onDeleteWorktree,
    required this.onSelectTerminal,
    required this.onCreateTerminal,
    required this.onCloseTerminal,
  });

  final ProjectInfo? project;
  final List<ProjectInfo> projects;
  final String? selectedProjectId;
  final List<RemoteWorktreeInfo> worktrees;
  final String? selectedWorktreeId;
  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final List<AISessionRecord> aiSessions;
  final ValueChanged<AISessionRecord> onOpenSession;
  final ValueChanged<AISessionRecord> onRenameSession;
  final ValueChanged<AISessionRecord> onDeleteSession;
  final ValueChanged<ProjectInfo> onSelectProject;
  final VoidCallback onEditProject;
  final VoidCallback onAddProject;
  final VoidCallback onRemoveProject;
  final ValueChanged<RemoteWorktreeInfo> onSelectWorktree;
  final VoidCallback onCreateWorktree;
  final ValueChanged<RemoteWorktreeInfo> onMergeWorktree;
  final ValueChanged<RemoteWorktreeInfo> onDeleteWorktree;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onCreateTerminal;
  final ValueChanged<TerminalInfo> onCloseTerminal;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final accent = Theme.of(context).colorScheme.secondary;
    return Container(
      color: PadColors.panel,
      child: Column(
        children: [
          _HeaderBar(
            title: project?.name ?? prefs.t('app.noProjects'),
            onTap: () => showPadProjectPicker(
              context,
              projects: projects,
              selectedProjectId: selectedProjectId,
              onSelectProject: onSelectProject,
              onAddProject: onAddProject,
            ),
            trailing: Icon(
              Icons.expand_more_rounded,
              size: 20,
              color: PadColors.textMuted,
            ),
          ),
          Expanded(
            flex: 5,
            child: worktrees.isEmpty
                ? _EmptyHint(text: prefs.t('worktree.empty'))
                : ListView.separated(
                    padding: const EdgeInsets.fromLTRB(8, 8, 8, 12),
                    itemCount: worktrees.length,
                    separatorBuilder: (_, _) => const SizedBox(height: 6),
                    itemBuilder: (context, index) {
                      final item = worktrees[index];
                      return _WorktreeRow(
                        info: item,
                        active: item.id == selectedWorktreeId,
                        accent: accent,
                        onTap: () => onSelectWorktree(item),
                        onMerge: () => onMergeWorktree(item),
                        onDelete: () => onDeleteWorktree(item),
                      );
                    },
                  ),
          ),
          _HeaderBar(
            title: prefs.t('workspace.sessions'),
            onTap: null,
            trailing: const SizedBox.shrink(),
          ),
          Expanded(
            flex: 6,
            child: aiSessions.isNotEmpty
                ? ListView.separated(
                    padding: const EdgeInsets.fromLTRB(8, 8, 8, 12),
                    itemCount: aiSessions.length,
                    separatorBuilder: (_, _) => const SizedBox(height: 4),
                    itemBuilder: (context, index) => _HistorySessionRow(
                      session: aiSessions[index],
                      onOpen: onOpenSession,
                      onRename: onRenameSession,
                      onDelete: onDeleteSession,
                    ),
                  )
                : _EmptyHint(text: prefs.t('workspace.sessionsEmpty')),
          ),
        ],
      ),
    );
  }
}

/// Darker section bar (project switcher / sessions header) — a touch darker than
/// the panel so it reads as a header strip.
class _HeaderBar extends StatelessWidget {
  const _HeaderBar({
    required this.title,
    required this.onTap,
    required this.trailing,
  });

  final String title;
  final VoidCallback? onTap;
  final Widget trailing;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      child: Container(
        height: 48,
        color: PadColors.header,
        padding: const EdgeInsets.only(left: 14, right: 8),
        child: Row(
          children: [
            Expanded(
              child: Text(
                title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: PadColors.textPrimary,
                  fontSize: 15,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            trailing,
          ],
        ),
      ),
    );
  }
}

class _EmptyHint extends StatelessWidget {
  const _EmptyHint({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
      child: Text(
        text,
        style: TextStyle(color: PadColors.textSubtle, fontSize: 12),
      ),
    );
  }
}

class _WorktreeRow extends StatelessWidget {
  const _WorktreeRow({
    required this.info,
    required this.active,
    required this.accent,
    required this.onTap,
    required this.onMerge,
    required this.onDelete,
  });

  final RemoteWorktreeInfo info;
  final bool active;
  final Color accent;
  final VoidCallback onTap;
  final VoidCallback onMerge;
  final VoidCallback onDelete;

  // Default worktrees / pathless entries have no merge/delete operations.
  bool get _hasActions => !info.isDefault && info.path.trim().isNotEmpty;

  Future<void> _openMenu(BuildContext context) async {
    if (!_hasActions) return;
    final prefs = AppPreferences.of(context);
    final title = info.name.trim().isNotEmpty
        ? info.name.trim()
        : (info.branch.trim().isNotEmpty ? info.branch.trim() : info.id);
    await showModalBottomSheet<void>(
      context: context,
      backgroundColor: Colors.transparent,
      builder: (sheetContext) => SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
          child: Container(
            decoration: BoxDecoration(
              color: PadColors.card,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: PadColors.border, width: 0.5),
            ),
            clipBehavior: Clip.antiAlias,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 14, 16, 10),
                  child: Row(
                    children: [
                      Icon(
                        Icons.account_tree_outlined,
                        color: PadColors.textMuted,
                        size: 20,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(
                          title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            color: PadColors.textPrimary,
                            fontSize: 15,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                Divider(height: 0.5, color: PadColors.border),
                _SessionMenuItem(
                  icon: Icons.call_merge_rounded,
                  label: prefs.t('worktree.merge'),
                  onTap: onMerge,
                ),
                _SessionMenuItem(
                  icon: Icons.delete_outline_rounded,
                  label: prefs.t('worktree.remove'),
                  danger: true,
                  onTap: onDelete,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Material(
      color: active ? PadColors.surfaceActive : Colors.transparent,
      borderRadius: BorderRadius.circular(8),
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTap: onTap,
        onLongPress: () => _openMenu(context),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      info.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        color: active
                            ? PadColors.textPrimary
                            : PadColors.textSecondary,
                        fontSize: 13,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      info.path,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        color: PadColors.textMuted,
                        fontSize: 11,
                      ),
                    ),
                  ],
                ),
              ),
              if (info.branch.trim().isNotEmpty) ...[
                const SizedBox(width: 8),
                _BranchBadge(
                  branch: info.branch.trim(),
                  accent: accent,
                  active: active,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _BranchBadge extends StatelessWidget {
  const _BranchBadge({
    required this.branch,
    required this.accent,
    required this.active,
  });

  final String branch;
  final Color accent;
  final bool active;

  @override
  Widget build(BuildContext context) {
    // No background chip — just the branch icon + name (per design: the badge
    // wash was too heavy next to the worktree row's own highlight).
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 96),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.alt_route_rounded,
            size: 11,
            color: active ? accent : PadColors.textMuted,
          ),
          const SizedBox(width: 4),
          Flexible(
            child: Text(
              branch,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                color: active ? accent : PadColors.textMuted,
                fontSize: 10,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Session-history item from `ai.session` (mirrors the desktop "会话记录" list).
/// Tapping the row opens a menu: open (resume in the terminal), rename, delete.
class _HistorySessionRow extends StatelessWidget {
  const _HistorySessionRow({
    required this.session,
    required this.onOpen,
    required this.onRename,
    required this.onDelete,
  });

  final AISessionRecord session;
  final ValueChanged<AISessionRecord> onOpen;
  final ValueChanged<AISessionRecord> onRename;
  final ValueChanged<AISessionRecord> onDelete;

  Future<void> _openMenu(BuildContext context) async {
    final prefs = AppPreferences.of(context);
    final title = session.title.trim().isNotEmpty ? session.title.trim() : session.id;
    await showModalBottomSheet<void>(
      context: context,
      backgroundColor: Colors.transparent,
      builder: (sheetContext) => SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
          child: Container(
            decoration: BoxDecoration(
              color: PadColors.card,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: PadColors.border, width: 0.5),
            ),
            clipBehavior: Clip.antiAlias,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 14, 16, 10),
                  child: Row(
                    children: [
                      Icon(
                        Icons.forum_outlined,
                        color: PadColors.textMuted,
                        size: 20,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(
                          title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            color: PadColors.textPrimary,
                            fontSize: 15,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                Divider(height: 0.5, color: PadColors.border),
                _SessionMenuItem(
                  icon: Icons.terminal_rounded,
                  label: prefs.t('session.menuOpen'),
                  onTap: () => onOpen(session),
                ),
                _SessionMenuItem(
                  icon: Icons.drive_file_rename_outline_rounded,
                  label: prefs.t('session.menuRename'),
                  onTap: () => onRename(session),
                ),
                _SessionMenuItem(
                  icon: Icons.delete_outline_rounded,
                  label: prefs.t('session.menuDelete'),
                  danger: true,
                  onTap: () => onDelete(session),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final title = session.title.trim().isNotEmpty ? session.title.trim() : session.id;
    final time = formatEpochSeconds(session.time);
    final tool = session.tool.trim();
    return InkWell(
      borderRadius: BorderRadius.circular(8),
      onTap: () => _openMenu(context),
      child: Padding(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: PadColors.textPrimary,
                    fontSize: 13,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              if (session.size > 0) ...[
                const SizedBox(width: 8),
                Text(
                  formatTokenSize(session.size),
                  style: TextStyle(
                    color: PadColors.textSubtle,
                    fontSize: 11,
                    fontWeight: FontWeight.w800,
                  ),
                ),
              ],
            ],
          ),
          if (tool.isNotEmpty || time.isNotEmpty) ...[
            const SizedBox(height: 3),
            // Second line: tool on the left, time on the right (two-ends aligned).
            Row(
              children: [
                Expanded(
                  child: Text(
                    tool,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: PadColors.textMuted,
                      fontSize: 11,
                    ),
                  ),
                ),
                if (time.isNotEmpty) ...[
                  const SizedBox(width: 8),
                  Text(
                    time,
                    style: TextStyle(
                      color: PadColors.textSubtle,
                      fontSize: 11,
                    ),
                  ),
                ],
              ],
            ),
          ],
        ],
      ),
      ),
    );
  }
}

/// One row in the session action sheet. Pops the sheet first, then runs the
/// action so any follow-up dialog (rename/delete) isn't stacked under it.
class _SessionMenuItem extends StatelessWidget {
  const _SessionMenuItem({
    required this.icon,
    required this.label,
    required this.onTap,
    this.danger = false,
  });

  final IconData icon;
  final String label;
  final VoidCallback onTap;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final color = danger ? PadColors.danger : PadColors.textPrimary;
    return InkWell(
      onTap: () {
        Navigator.of(context).pop();
        onTap();
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
        child: Row(
          children: [
            Icon(icon, color: color, size: 20),
            const SizedBox(width: 12),
            Text(
              label,
              style: TextStyle(
                color: color,
                fontSize: 14,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
