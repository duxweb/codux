import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';

class CoduxAboutDialog extends StatelessWidget {
  const CoduxAboutDialog({
    super.key,
    required this.title,
    required this.body,
    required this.versionText,
    required this.closeLabel,
    required this.onOpenGithub,
  });

  final String title;
  final String body;
  final String versionText;
  final String closeLabel;
  final VoidCallback onOpenGithub;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(title),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(body),
          const SizedBox(height: AppSpacing.m),
          Text(
            versionText,
            style: TextStyle(color: AppColors.textMuted, fontSize: 14),
          ),
          const SizedBox(height: AppSpacing.s),
          SelectableText(
            'github.com/duxweb/codux-flutter',
            style: TextStyle(color: accent, fontSize: 14),
          ),
        ],
      ),
      actions: [
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: onOpenGithub,
          child: const Text('GitHub'),
        ),
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () => Navigator.pop(context),
          child: Text(closeLabel),
        ),
      ],
    );
  }
}
