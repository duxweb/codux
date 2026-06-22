import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';
import 'icon_text_label.dart';

class WorktreeCreateDraft {
  const WorktreeCreateDraft({required this.baseBranch, required this.name});

  final String baseBranch;
  final String name;
}

class WorktreeCreateDialog extends StatefulWidget {
  const WorktreeCreateDialog({
    super.key,
    required this.title,
    required this.baseBranchLabel,
    required this.nameLabel,
    required this.cancelLabel,
    required this.createLabel,
    required this.branchOptions,
    required this.initialBaseBranch,
    required this.initialName,
  });

  final String title;
  final String baseBranchLabel;
  final String nameLabel;
  final String cancelLabel;
  final String createLabel;
  final List<String> branchOptions;
  final String initialBaseBranch;
  final String initialName;

  @override
  State<WorktreeCreateDialog> createState() => _WorktreeCreateDialogState();
}

class _WorktreeCreateDialogState extends State<WorktreeCreateDialog> {
  late final TextEditingController _nameController;
  late String _baseBranch;

  @override
  void initState() {
    super.initState();
    _baseBranch = widget.initialBaseBranch;
    _nameController = TextEditingController(text: widget.initialName);
  }

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(widget.title),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          DropdownButtonFormField<String>(
            initialValue: widget.branchOptions.contains(_baseBranch)
                ? _baseBranch
                : null,
            items: [
              for (final branch in widget.branchOptions)
                DropdownMenuItem(value: branch, child: Text(branch)),
            ],
            onChanged: (value) {
              if (value == null) return;
              setState(() => _baseBranch = value);
            },
            decoration: InputDecoration(labelText: widget.baseBranchLabel),
          ),
          const SizedBox(height: AppSpacing.m),
          TextField(
            controller: _nameController,
            autofocus: true,
            decoration: InputDecoration(labelText: widget.nameLabel),
          ),
        ],
      ),
      actions: [
        TextButton(
          style: TextButton.styleFrom(foregroundColor: AppColors.textMuted),
          onPressed: () => Navigator.pop(context),
          child: IconTextLabel(
            icon: Icons.close_rounded,
            label: widget.cancelLabel,
          ),
        ),
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () {
            Navigator.pop(
              context,
              WorktreeCreateDraft(
                baseBranch: _baseBranch.trim(),
                name: _nameController.text.trim(),
              ),
            );
          },
          child: IconTextLabel(
            icon: Icons.add_rounded,
            label: widget.createLabel,
          ),
        ),
      ],
    );
  }
}

String defaultWorktreeName({DateTime? now}) {
  final value = now ?? DateTime.now();
  String two(int number) => number.toString().padLeft(2, '0');
  return '${value.year}${two(value.month)}${two(value.day)}-'
      '${two(value.hour)}${two(value.minute)}${two(value.second)}';
}
