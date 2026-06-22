import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';
import 'icon_text_label.dart';

class WorktreeActionDialog extends StatelessWidget {
  const WorktreeActionDialog({
    super.key,
    required this.title,
    required this.message,
    required this.cancelLabel,
    required this.destructive,
  });

  final String title;
  final String message;
  final String cancelLabel;
  final bool destructive;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(title),
      content: Text(message),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: IconTextLabel(icon: Icons.close_rounded, label: cancelLabel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, true),
          style: TextButton.styleFrom(
            foregroundColor: destructive ? AppColors.danger : accent,
          ),
          child: IconTextLabel(
            icon: destructive
                ? Icons.delete_outline_rounded
                : Icons.call_merge_rounded,
            label: title,
          ),
        ),
      ],
    );
  }
}
