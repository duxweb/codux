import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../theme/app_theme.dart';

class ProjectFormOverlay extends StatelessWidget {
  const ProjectFormOverlay({
    super.key,
    required this.topInset,
    required this.bottomInset,
    required this.title,
    required this.nameController,
    required this.pathController,
    required this.onClose,
    required this.onChoosePath,
    required this.onSave,
  });

  final double topInset;
  final double bottomInset;
  final String title;
  final TextEditingController nameController;
  final TextEditingController pathController;
  final VoidCallback onClose;
  final VoidCallback onChoosePath;
  final VoidCallback onSave;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Positioned.fill(
      child: Material(
        color: AppColors.bgBase,
        child: Column(
          children: [
            Container(
              height: AppLayout.topBarHeight + topInset,
              padding: EdgeInsets.only(top: topInset),
              decoration: BoxDecoration(
                color: AppColors.bgBase,
                border: Border(
                  bottom: BorderSide(color: AppColors.border, width: 0.5),
                ),
              ),
              child: Row(
                children: [
                  const SizedBox(width: AppSpacing.s),
                  IconButton(
                    onPressed: onClose,
                    icon: const Icon(Icons.close, size: 22),
                    color: AppColors.textPrimary,
                  ),
                  Expanded(
                    child: Text(
                      title,
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        color: AppColors.textPrimary,
                        fontSize: AppTextSize.title,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ),
                  TextButton(
                    style: TextButton.styleFrom(foregroundColor: accent),
                    onPressed: onSave,
                    child: Text(prefs.t('common.save')),
                  ),
                ],
              ),
            ),
            Expanded(
              child: ListView(
                physics: const BouncingScrollPhysics(),
                padding: EdgeInsets.fromLTRB(
                  AppSpacing.l,
                  AppSpacing.l,
                  AppSpacing.l,
                  bottomInset + AppSpacing.xxl,
                ),
                children: [
                  _Label(prefs.t('project.nameLabel')),
                  const SizedBox(height: AppSpacing.s),
                  _Field(
                    controller: nameController,
                    hint: prefs.t('project.nameHint'),
                    accent: accent,
                  ),
                  const SizedBox(height: AppSpacing.l),
                  _Label(prefs.t('project.pathLabel')),
                  const SizedBox(height: AppSpacing.s),
                  _PathField(
                    controller: pathController,
                    accent: accent,
                    onChoosePath: onChoosePath,
                  ),
                  const SizedBox(height: AppSpacing.m),
                  Text(
                    prefs.t('project.pathHint'),
                    style: TextStyle(
                      color: AppColors.textMuted,
                      fontSize: AppTextSize.small,
                      height: 1.35,
                    ),
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

class _Label extends StatelessWidget {
  const _Label(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: TextStyle(
      color: AppColors.textMuted,
      fontSize: AppTextSize.small,
      fontWeight: FontWeight.w700,
    ),
  );
}

class _Field extends StatelessWidget {
  const _Field({
    required this.controller,
    required this.hint,
    required this.accent,
  });

  final TextEditingController controller;
  final String hint;
  final Color accent;

  @override
  Widget build(BuildContext context) => Container(
    decoration: BoxDecoration(
      color: AppColors.bgSurface,
      borderRadius: BorderRadius.circular(AppRadius.md),
      border: Border.all(color: AppColors.border, width: 0.5),
    ),
    child: TextField(
      controller: controller,
      style: TextStyle(
        color: AppColors.textPrimary,
        fontSize: AppTextSize.body,
      ),
      cursorColor: accent,
      decoration: InputDecoration(
        hintText: hint,
        hintStyle: TextStyle(color: AppColors.textSubtle),
        border: InputBorder.none,
        contentPadding: const EdgeInsets.symmetric(
          horizontal: AppSpacing.m,
          vertical: AppSpacing.m,
        ),
      ),
    ),
  );
}

class _PathField extends StatelessWidget {
  const _PathField({
    required this.controller,
    required this.accent,
    required this.onChoosePath,
  });

  final TextEditingController controller;
  final Color accent;
  final VoidCallback onChoosePath;

  @override
  Widget build(BuildContext context) => Container(
    decoration: BoxDecoration(
      color: AppColors.bgSurface,
      borderRadius: BorderRadius.circular(AppRadius.md),
      border: Border.all(color: AppColors.border, width: 0.5),
    ),
    child: Row(
      children: [
        Expanded(
          child: TextField(
            controller: controller,
            style: TextStyle(
              color: AppColors.textPrimary,
              fontSize: AppTextSize.body,
            ),
            cursorColor: accent,
            decoration: InputDecoration(
              hintText: '/path/to/project',
              hintStyle: TextStyle(color: AppColors.textSubtle),
              border: InputBorder.none,
              contentPadding: EdgeInsets.symmetric(
                horizontal: AppSpacing.m,
                vertical: AppSpacing.m,
              ),
            ),
          ),
        ),
        TextButton.icon(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: onChoosePath,
          icon: const Icon(Icons.folder_open_rounded, size: 18),
          label: Text(AppPreferences.of(context).t('common.select')),
        ),
        const SizedBox(width: AppSpacing.xs),
      ],
    ),
  );
}
