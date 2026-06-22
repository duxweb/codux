import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';

class FileRenameDialog extends StatefulWidget {
  const FileRenameDialog({
    super.key,
    required this.title,
    required this.label,
    required this.cancelLabel,
    required this.saveLabel,
    required this.initialName,
  });

  final String title;
  final String label;
  final String cancelLabel;
  final String saveLabel;
  final String initialName;

  @override
  State<FileRenameDialog> createState() => _FileRenameDialogState();
}

class _FileRenameDialogState extends State<FileRenameDialog> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initialName);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(widget.title),
      content: TextField(
        controller: _controller,
        autofocus: true,
        decoration: InputDecoration(labelText: widget.label),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(widget.cancelLabel),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(_controller.text.trim()),
          child: Text(widget.saveLabel),
        ),
      ],
    );
  }
}

class FileDeleteDialog extends StatelessWidget {
  const FileDeleteDialog({
    super.key,
    required this.title,
    required this.message,
    required this.cancelLabel,
    required this.deleteLabel,
  });

  final String title;
  final String message;
  final String cancelLabel;
  final String deleteLabel;

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(title),
      content: Text(message),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(cancelLabel),
        ),
        FilledButton(
          style: FilledButton.styleFrom(backgroundColor: AppColors.danger),
          onPressed: () => Navigator.of(context).pop(true),
          child: Text(deleteLabel),
        ),
      ],
    );
  }
}
