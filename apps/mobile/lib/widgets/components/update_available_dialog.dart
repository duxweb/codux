import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';

class UpdateAvailableDialog extends StatelessWidget {
  const UpdateAvailableDialog({
    super.key,
    required this.title,
    required this.body,
    required this.laterLabel,
    required this.actionLabel,
    required this.onOpen,
  });

  final String title;
  final String body;
  final String laterLabel;
  final String actionLabel;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(title),
      content: Text(body),
      actions: [
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () => Navigator.pop(context),
          child: Text(laterLabel),
        ),
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () {
            Navigator.pop(context);
            onOpen();
          },
          child: Text(actionLabel),
        ),
      ],
    );
  }
}
