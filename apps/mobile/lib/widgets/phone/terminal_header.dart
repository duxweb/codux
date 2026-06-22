import 'package:flutter/material.dart';
import '../../models/workspace_mode.dart';
import '../../theme/app_theme.dart';
import 'project_action_menu.dart';

class TerminalHeader extends StatelessWidget {
  const TerminalHeader({
    super.key,
    required this.topInset,
    required this.onBack,
    required this.onEditProject,
    required this.onAddProject,
    required this.onRemoveProject,
    required this.activeMode,
    required this.onTerminal,
    required this.onStats,
    required this.onFiles,
    this.latencyMs,
    this.connected = true,
  });

  final double topInset;
  final VoidCallback onBack;
  final VoidCallback onEditProject;
  final VoidCallback onAddProject;
  final VoidCallback onRemoveProject;
  final WorkspaceMode activeMode;
  final VoidCallback onTerminal;
  final VoidCallback onStats;
  final VoidCallback onFiles;
  final int? latencyMs;
  final bool connected;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return Material(
      color: AppColors.bgBase,
      child: Container(
        height: AppLayout.topBarHeight + topInset,
        padding: EdgeInsets.only(top: topInset),
        decoration: BoxDecoration(color: AppColors.bgBase),
        child: Stack(
          children: [
            Positioned(
              left: AppSpacing.s,
              top: 6,
              child: SizedBox(
                width: 44,
                height: 44,
                child: IconButton(
                  onPressed: onBack,
                  icon: const Icon(Icons.arrow_back_ios_new, size: 18),
                  color: AppColors.textPrimary,
                ),
              ),
            ),
            Center(
              child: _ModeCapsule(
                accent: accent,
                activeMode: activeMode,
                onTerminal: onTerminal,
                onStats: onStats,
                onFiles: onFiles,
              ),
            ),
            Positioned(
              right: AppSpacing.s,
              top: 7,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  _HeaderLatencyText(
                    latencyMs: latencyMs,
                    connected: connected,
                  ),
                  const SizedBox(width: AppSpacing.xs),
                  ProjectActionMenu(
                    onEditProject: onEditProject,
                    onAddProject: onAddProject,
                    onRemoveProject: onRemoveProject,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _HeaderLatencyText extends StatelessWidget {
  const _HeaderLatencyText({required this.latencyMs, required this.connected});
  final int? latencyMs;
  final bool connected;

  @override
  Widget build(BuildContext context) {
    final label = connected && latencyMs != null ? '${latencyMs}ms' : '-- ms';
    final color = _latencyColor(latencyMs, connected);
    return SizedBox(
      width: 44,
      child: Text(
        label,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        textAlign: TextAlign.right,
        style: TextStyle(
          color: color,
          fontSize: 12,
          height: 1,
          fontWeight: FontWeight.w800,
        ),
      ),
    );
  }

  Color _latencyColor(int? value, bool connected) {
    if (!connected || value == null) return AppColors.textSubtle;
    if (value <= 120) return AppColors.success;
    if (value <= 300) return AppColors.warning;
    return AppColors.danger;
  }
}

class _ModeCapsule extends StatelessWidget {
  const _ModeCapsule({
    required this.accent,
    required this.activeMode,
    required this.onTerminal,
    required this.onStats,
    required this.onFiles,
  });
  final Color accent;
  final WorkspaceMode activeMode;
  final VoidCallback onTerminal;
  final VoidCallback onStats;
  final VoidCallback onFiles;

  @override
  Widget build(BuildContext context) => Container(
    height: 38,
    padding: const EdgeInsets.all(3),
    decoration: BoxDecoration(
      color: AppColors.bgSurface,
      borderRadius: BorderRadius.circular(999),
    ),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _CapsuleItem(
          icon: Icons.terminal,
          active: activeMode == WorkspaceMode.terminal,
          accent: accent,
          onTap: onTerminal,
        ),
        _CapsuleItem(
          icon: Icons.bar_chart_rounded,
          active: activeMode == WorkspaceMode.stats,
          accent: accent,
          onTap: onStats,
        ),
        _CapsuleItem(
          icon: Icons.folder_open_rounded,
          active: activeMode == WorkspaceMode.files,
          accent: accent,
          onTap: onFiles,
        ),
      ],
    ),
  );
}

class _CapsuleItem extends StatelessWidget {
  const _CapsuleItem({
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
  Widget build(BuildContext context) => InkWell(
    borderRadius: BorderRadius.circular(999),
    onTap: onTap,
    child: Container(
      width: 46,
      height: 32,
      decoration: BoxDecoration(
        color: active ? accent.withValues(alpha: 0.18) : Colors.transparent,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Icon(
        icon,
        color: active ? accent : AppColors.textMuted,
        size: active ? 19 : 18,
      ),
    ),
  );
}
