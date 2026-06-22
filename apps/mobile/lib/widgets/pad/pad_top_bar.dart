import 'package:flutter/material.dart';

import '../../i18n.dart';
import '../../models/workspace_mode.dart';
import '../../theme/app_theme.dart';
import 'pad_theme.dart';

/// Full-width pad top bar: brand, the terminal/files/AI-stats view switch, and a
/// connection latency pill. The view switch drives which context panel the right
/// column shows (and keeps the terminal as the workspace).
class PadTopBar extends StatelessWidget {
  const PadTopBar({
    super.key,
    required this.workspaceMode,
    required this.toolMode,
    required this.onBack,
    required this.onShowTerminal,
    required this.onShowStats,
    required this.onShowFiles,
    required this.onShowReview,
    required this.onShowSsh,
    required this.onShowGit,
  });

  final WorkspaceMode workspaceMode;
  final WorkspaceMode toolMode;
  final VoidCallback onBack;
  final VoidCallback onShowTerminal;
  final VoidCallback onShowStats;
  final VoidCallback onShowFiles;
  final VoidCallback onShowReview;
  final VoidCallback onShowSsh;
  final VoidCallback onShowGit;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Container(
      height: 56,
      padding: const EdgeInsets.symmetric(horizontal: AppSpacing.m),
      child: Row(
        children: [
          // The brand doubles as "back to device list" (mirrors the phone back).
          InkWell(
            onTap: onBack,
            borderRadius: BorderRadius.circular(8),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 6),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    Icons.chevron_left_rounded,
                    size: 20,
                    color: PadColors.textMuted,
                  ),
                  const SizedBox(width: 2),
                  Text(
                    'Codux',
                    style: TextStyle(
                      color: PadColors.textPrimary,
                      fontSize: 17,
                      fontWeight: FontWeight.w800,
                    ),
                  ),
                ],
              ),
            ),
          ),
          const Spacer(),
          _ViewSwitch(
            mode: workspaceMode,
            accent: accent,
            terminalLabel: prefs.t('workspace.terminal'),
            filesLabel: prefs.t('workspace.files'),
            reviewLabel: prefs.t('workspace.review'),
            onTerminal: onShowTerminal,
            onFiles: onShowFiles,
            onReview: onShowReview,
          ),
          const Spacer(),
          _HeaderActions(
            mode: toolMode,
            accent: accent,
            onStats: onShowStats,
            onFiles: onShowFiles,
            onSsh: onShowSsh,
            onGit: onShowGit,
          ),
        ],
      ),
    );
  }
}

class _ViewSwitch extends StatelessWidget {
  const _ViewSwitch({
    required this.mode,
    required this.accent,
    required this.terminalLabel,
    required this.filesLabel,
    required this.reviewLabel,
    required this.onTerminal,
    required this.onFiles,
    required this.onReview,
  });

  final WorkspaceMode mode;
  final Color accent;
  final String terminalLabel;
  final String filesLabel;
  final String reviewLabel;
  final VoidCallback onTerminal;
  final VoidCallback onFiles;
  final VoidCallback onReview;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 40,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _ViewTab(
            icon: Icons.terminal_rounded,
            label: terminalLabel,
            active: mode == WorkspaceMode.terminal,
            accent: accent,
            onTap: onTerminal,
          ),
          _ViewTab(
            icon: Icons.folder_open_rounded,
            label: filesLabel,
            active: mode == WorkspaceMode.files,
            accent: accent,
            onTap: onFiles,
          ),
          _ViewTab(
            icon: Icons.rate_review_rounded,
            label: reviewLabel,
            active: mode == WorkspaceMode.review,
            accent: accent,
            onTap: onReview,
          ),
        ],
      ),
    );
  }
}

class _HeaderActions extends StatelessWidget {
  const _HeaderActions({
    required this.mode,
    required this.accent,
    required this.onStats,
    required this.onFiles,
    required this.onSsh,
    required this.onGit,
  });

  final WorkspaceMode mode;
  final Color accent;
  final VoidCallback onStats;
  final VoidCallback onFiles;
  final VoidCallback onSsh;
  final VoidCallback onGit;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _HeaderIconButton(
          icon: Icons.insights_rounded,
          active: mode == WorkspaceMode.stats,
          accent: accent,
          onTap: onStats,
        ),
        _HeaderIconButton(
          icon: Icons.key_rounded,
          active: mode == WorkspaceMode.ssh,
          accent: accent,
          onTap: onSsh,
        ),
        _HeaderIconButton(
          icon: Icons.account_tree_rounded,
          active: mode == WorkspaceMode.git,
          accent: accent,
          onTap: onGit,
        ),
        _HeaderIconButton(
          icon: Icons.folder_open_rounded,
          active: mode == WorkspaceMode.files,
          accent: accent,
          onTap: onFiles,
        ),
      ],
    );
  }
}

class _HeaderIconButton extends StatelessWidget {
  const _HeaderIconButton({
    required this.icon,
    required this.active,
    required this.accent,
    this.onTap,
  });

  final IconData icon;
  final bool active;
  final Color accent;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(left: 4),
      child: Material(
        color: active ? PadColors.cardActive : Colors.transparent,
        borderRadius: BorderRadius.circular(8),
        child: InkWell(
          borderRadius: BorderRadius.circular(8),
          onTap: onTap,
          child: SizedBox(
            width: 36,
            height: 36,
            child: Icon(
              icon,
              size: 18,
              color: active ? accent : PadColors.textMuted,
            ),
          ),
        ),
      ),
    );
  }
}

class _ViewTab extends StatelessWidget {
  const _ViewTab({
    required this.icon,
    required this.label,
    required this.active,
    required this.accent,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final bool active;
  final Color accent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(left: 6),
      child: Material(
        color: active ? accent.withValues(alpha: 0.18) : Colors.transparent,
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: onTap,
          child: Container(
            height: 40,
            padding: const EdgeInsets.symmetric(horizontal: 16),
            alignment: Alignment.center,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  icon,
                  size: 16,
                  color: active ? accent : PadColors.textMuted,
                ),
                const SizedBox(width: 7),
                Text(
                  label,
                  style: TextStyle(
                    color: active ? accent : PadColors.textSecondary,
                    fontSize: 13.5,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
