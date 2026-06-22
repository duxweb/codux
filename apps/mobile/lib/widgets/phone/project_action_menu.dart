import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../theme/app_theme.dart';

class ProjectActionMenu extends StatelessWidget {
  const ProjectActionMenu({
    super.key,
    required this.onEditProject,
    required this.onAddProject,
    required this.onRemoveProject,
  });

  final VoidCallback onEditProject;
  final VoidCallback onAddProject;
  final VoidCallback onRemoveProject;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    return SizedBox(
      width: 44,
      height: 44,
      child: PopupMenuButton<String>(
        tooltip: '',
        padding: EdgeInsets.zero,
        position: PopupMenuPosition.under,
        offset: const Offset(0, 4),
        color: AppColors.bgSurface,
        elevation: 12,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppRadius.md),
          side: BorderSide(color: AppColors.border, width: 0.5),
        ),
        icon: Icon(
          Icons.more_vert,
          size: 22,
          color: AppColors.textPrimary,
        ),
        onSelected: (value) {
          switch (value) {
            case 'edit':
              onEditProject();
              break;
            case 'add':
              onAddProject();
              break;
            case 'remove':
              onRemoveProject();
              break;
          }
        },
        itemBuilder: (context) => [
          PopupMenuItem<String>(
            value: 'edit',
            height: 40,
            child: _MenuRow(
              icon: Icons.edit_outlined,
              label: prefs.t('project.edit'),
            ),
          ),
          PopupMenuItem<String>(
            value: 'add',
            height: 40,
            child: _MenuRow(
              icon: Icons.add_box_outlined,
              label: prefs.t('project.add'),
            ),
          ),
          PopupMenuItem<String>(
            value: 'remove',
            height: 40,
            child: _MenuRow(
              icon: Icons.delete_outline,
              label: prefs.t('project.remove'),
              danger: true,
            ),
          ),
        ],
      ),
    );
  }
}

class _MenuRow extends StatelessWidget {
  const _MenuRow({
    required this.icon,
    required this.label,
    this.danger = false,
  });

  final IconData icon;
  final String label;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final color = danger ? AppColors.danger : AppColors.textPrimary;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, size: 18, color: color),
        const SizedBox(width: AppSpacing.s),
        Text(
          label,
          style: TextStyle(
            color: color,
            fontSize: 14,
            fontWeight: FontWeight.w500,
          ),
        ),
      ],
    );
  }
}
