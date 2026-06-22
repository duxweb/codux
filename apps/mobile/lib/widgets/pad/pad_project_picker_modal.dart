import 'package:flutter/material.dart';

import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../theme/app_theme.dart';
import 'pad_theme.dart';

/// Project selector shown when the left sidebar header is tapped. Lists the
/// available projects and offers "add project".
Future<void> showPadProjectPicker(
  BuildContext context, {
  required List<ProjectInfo> projects,
  required String? selectedProjectId,
  required ValueChanged<ProjectInfo> onSelectProject,
  required VoidCallback onAddProject,
}) {
  // Resolve localization from the caller's context — the modal route is pushed
  // on the root navigator, which sits above the AppPreferences provider, so
  // AppPreferences.of(sheetContext) would not find it.
  final prefs = AppPreferences.of(context);
  return showModalBottomSheet<void>(
    context: context,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    builder: (sheetContext) {
      final accent = Theme.of(sheetContext).colorScheme.secondary;
      return SafeArea(
        child: Container(
          margin: const EdgeInsets.all(AppSpacing.m),
          constraints: const BoxConstraints(maxHeight: 520),
          decoration: BoxDecoration(
            color: PadColors.panel,
            borderRadius: BorderRadius.circular(AppRadius.lg),
            border: Border.all(color: PadColors.border, width: 0.5),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(AppSpacing.l, AppSpacing.l, AppSpacing.l, AppSpacing.s),
                child: Row(
                  children: [
                    Text(
                      prefs.t('workspace.projects'),
                      style: TextStyle(
                        color: PadColors.textPrimary,
                        fontSize: 16,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const Spacer(),
                    IconButton(
                      icon: Icon(Icons.close_rounded, size: 20, color: PadColors.textMuted),
                      onPressed: () => Navigator.of(sheetContext).pop(),
                    ),
                  ],
                ),
              ),
              Flexible(
                child: ListView.separated(
                  shrinkWrap: true,
                  padding: const EdgeInsets.symmetric(horizontal: AppSpacing.m),
                  itemCount: projects.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 6),
                  itemBuilder: (context, index) {
                    final project = projects[index];
                    final active = project.id == selectedProjectId;
                    return Material(
                      color: active ? accent.withValues(alpha: 0.16) : PadColors.card,
                      borderRadius: BorderRadius.circular(AppRadius.md),
                      child: InkWell(
                        borderRadius: BorderRadius.circular(AppRadius.md),
                        onTap: () {
                          Navigator.of(sheetContext).pop();
                          onSelectProject(project);
                        },
                        child: Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
                          child: Row(
                            children: [
                              _ProjectGlyph(name: project.name, active: active, accent: accent),
                              const SizedBox(width: AppSpacing.m),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      project.name,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(
                                        color: PadColors.textPrimary,
                                        fontSize: 14,
                                        fontWeight: FontWeight.w700,
                                      ),
                                    ),
                                    const SizedBox(height: 2),
                                    Text(
                                      project.path ?? '',
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(color: PadColors.textMuted, fontSize: 11),
                                    ),
                                  ],
                                ),
                              ),
                              if (active)
                                Icon(Icons.check_rounded, size: 18, color: accent),
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                ),
              ),
              Padding(
                padding: const EdgeInsets.all(AppSpacing.m),
                child: SizedBox(
                  width: double.infinity,
                  child: TextButton.icon(
                    onPressed: () {
                      Navigator.of(sheetContext).pop();
                      onAddProject();
                    },
                    style: TextButton.styleFrom(
                      backgroundColor: PadColors.card,
                      foregroundColor: PadColors.textPrimary,
                      padding: const EdgeInsets.symmetric(vertical: 14),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(AppRadius.md),
                      ),
                    ),
                    icon: const Icon(Icons.add_rounded, size: 18),
                    label: Text(prefs.t('project.add')),
                  ),
                ),
              ),
            ],
          ),
        ),
      );
    },
  );
}

class _ProjectGlyph extends StatelessWidget {
  const _ProjectGlyph({required this.name, required this.active, required this.accent});

  final String name;
  final bool active;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 36,
      height: 36,
      decoration: BoxDecoration(
        color: active ? accent.withValues(alpha: 0.22) : PadColors.panel,
        borderRadius: BorderRadius.circular(AppRadius.sm),
      ),
      alignment: Alignment.center,
      child: Text(
        projectInitials(name),
        style: TextStyle(
          color: active ? accent : PadColors.textMuted,
          fontSize: 13,
          fontWeight: FontWeight.w800,
        ),
      ),
    );
  }
}

String projectInitials(String value) {
  final trimmed = value.trim();
  if (trimmed.isEmpty) return 'P';
  final parts = trimmed.split(RegExp(r'[-_\s]+')).where((item) => item.isNotEmpty).toList();
  if (parts.length >= 2) {
    return '${parts.first[0]}${parts.last[0]}'.toUpperCase();
  }
  return trimmed.substring(0, trimmed.length >= 2 ? 2 : 1).toUpperCase();
}
