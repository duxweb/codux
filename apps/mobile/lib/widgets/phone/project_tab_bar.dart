import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../theme/app_theme.dart';

class ProjectTabBar extends StatelessWidget {
  const ProjectTabBar({
    super.key,
    required this.projects,
    required this.selectedId,
    required this.loading,
    required this.terminals,
    required this.activeTerminalId,
    required this.onSelect,
    required this.onSelectTerminal,
    required this.onRefresh,
    required this.onCreateTerminal,
    required this.onCloseTerminal,
    required this.onRebuild,
    required this.onOpenSwitcher,
  });

  final List<ProjectInfo> projects;
  final String? selectedId;
  final bool loading;
  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final ValueChanged<ProjectInfo> onSelect;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onRefresh;
  final VoidCallback onCreateTerminal;
  final VoidCallback? onCloseTerminal;
  final VoidCallback onRebuild;
  final VoidCallback onOpenSwitcher;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Material(
      color: AppColors.bgSurface,
      child: Container(
        height: AppLayout.tabBarHeight,
        decoration: BoxDecoration(color: AppColors.bgSurface),
        child: Row(
          children: [
            Expanded(
              child: ListView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(
                  horizontal: AppSpacing.l,
                  vertical: AppSpacing.s,
                ),
                children: [
                  if (projects.isEmpty && !loading)
                    Center(
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: AppSpacing.s,
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              prefs.t('app.noProjects'),
                              style: TextStyle(
                                color: AppColors.textSubtle,
                                fontSize: 12,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  for (final project in projects)
                    Padding(
                      padding: const EdgeInsets.only(right: AppSpacing.s),
                      child: _ProjectTab(
                        project: project,
                        active: project.id == selectedId,
                        accent: accent,
                        onTap: () => onSelect(project),
                      ),
                    ),
                ],
              ),
            ),
            Container(width: 0.5, height: 24, color: AppColors.border),
            const SizedBox(width: AppSpacing.s),
            Padding(
              padding: const EdgeInsets.only(right: AppSpacing.s),
              child: SizedBox(
                width: 36,
                height: 32,
                child: InkWell(
                  borderRadius: BorderRadius.circular(AppRadius.sm),
                  onTap: onOpenSwitcher,
                  child: Container(
                    decoration: BoxDecoration(
                      color: accent.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(AppRadius.sm),
                    ),
                    child: Center(
                      child: loading
                          ? SizedBox(
                              width: 14,
                              height: 14,
                              child: CircularProgressIndicator(
                                strokeWidth: 2,
                                color: accent,
                              ),
                            )
                          : Icon(
                              Icons.grid_view_rounded,
                              color: accent,
                              size: 17,
                            ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ProjectTab extends StatelessWidget {
  const _ProjectTab({
    required this.project,
    required this.active,
    required this.accent,
    required this.onTap,
  });
  final ProjectInfo project;
  final bool active;
  final Color accent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: active ? accent.withValues(alpha: 0.14) : Colors.transparent,
      borderRadius: BorderRadius.circular(999),
      child: InkWell(
        borderRadius: BorderRadius.circular(999),
        onTap: onTap,
        child: Container(
          height: 36,
          padding: const EdgeInsets.symmetric(horizontal: AppSpacing.m),
          alignment: Alignment.center,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 140),
            child: Text(
              project.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                color: active ? accent : AppColors.textMuted,
                fontSize: 14,
                fontWeight: active ? FontWeight.w700 : FontWeight.w500,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
