import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../theme/app_theme.dart';

class MoreMenu extends StatelessWidget {
  const MoreMenu({
    super.key,
    required this.onAddDevice,
    required this.onLogs,
    required this.onCheckUpdate,
    required this.onAbout,
    this.showSettings = false,
    this.onSettings,
  });

  final VoidCallback onAddDevice;
  final VoidCallback onLogs;
  final VoidCallback onCheckUpdate;
  final VoidCallback onAbout;
  final bool showSettings;
  final VoidCallback? onSettings;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final entries = <_MoreEntry>[
      _MoreEntry(
        value: 'addDevice',
        icon: Icons.add_circle_outline,
        label: prefs.t('app.addDevice'),
        onSelected: onAddDevice,
      ),
      if (showSettings && onSettings != null)
        _MoreEntry(
          value: 'settings',
          icon: Icons.settings_outlined,
          label: prefs.t('app.settings'),
          onSelected: onSettings!,
        ),
      _MoreEntry(
        value: 'logs',
        icon: Icons.receipt_long_outlined,
        label: prefs.t('app.logs'),
        onSelected: onLogs,
      ),
      _MoreEntry(
        value: 'checkUpdate',
        icon: Icons.system_update_alt,
        label: prefs.t('app.checkUpdate'),
        onSelected: onCheckUpdate,
      ),
      _MoreEntry(
        value: 'about',
        icon: Icons.info_outline,
        label: prefs.t('app.about'),
        onSelected: onAbout,
      ),
    ];

    return SizedBox(
      width: 42,
      height: 42,
      child: PopupMenuButton<String>(
        tooltip: '',
        padding: EdgeInsets.zero,
        position: PopupMenuPosition.under,
        offset: const Offset(0, 6),
        color: AppColors.bgSurface,
        elevation: 12,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppRadius.md),
          side: BorderSide(color: AppColors.border, width: 0.5),
        ),
        child: Material(
          color: AppColors.bgElevated,
          shape: const CircleBorder(),
          child: Center(
            child: Icon(
              Icons.more_horiz,
              size: 20,
              color: AppColors.textPrimary,
            ),
          ),
        ),
        onSelected: (value) {
          final entry = entries.firstWhere((e) => e.value == value);
          entry.onSelected();
        },
        itemBuilder: (context) => [
          for (final entry in entries)
            PopupMenuItem<String>(
              value: entry.value,
              height: 40,
              child: _MenuItemRow(icon: entry.icon, label: entry.label),
            ),
        ],
      ),
    );
  }
}

class _MoreEntry {
  _MoreEntry({
    required this.value,
    required this.icon,
    required this.label,
    required this.onSelected,
  });
  final String value;
  final IconData icon;
  final String label;
  final VoidCallback onSelected;
}

class _MenuItemRow extends StatelessWidget {
  const _MenuItemRow({required this.icon, required this.label});
  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    final color = AppColors.textPrimary;
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
