import 'package:flutter/material.dart';

import '../../models/remote_models.dart';
import '../../theme/app_theme.dart';

class DeviceEditDialog extends StatefulWidget {
  const DeviceEditDialog({
    super.key,
    required this.device,
    required this.title,
    required this.nameLabel,
    required this.cancelLabel,
    required this.saveLabel,
  });

  final StoredDevice device;
  final String title;
  final String nameLabel;
  final String cancelLabel;
  final String saveLabel;

  @override
  State<DeviceEditDialog> createState() => _DeviceEditDialogState();
}

class _DeviceEditDialogState extends State<DeviceEditDialog> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(
      text: widget.device.hostName?.isNotEmpty == true
          ? widget.device.hostName
          : widget.device.name,
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(widget.title),
      content: TextField(
        controller: _controller,
        cursorColor: accent,
        decoration: InputDecoration(
          labelText: widget.nameLabel,
          labelStyle: TextStyle(color: AppColors.textMuted),
          focusedBorder: UnderlineInputBorder(
            borderSide: BorderSide(color: accent),
          ),
        ),
      ),
      actions: [
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () => Navigator.pop(context),
          child: Text(widget.cancelLabel),
        ),
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () {
            final name = _controller.text.trim();
            Navigator.pop(
              context,
              widget.device.copyWith(
                hostName: name.isEmpty ? widget.device.hostName : name,
              ),
            );
          },
          child: Text(widget.saveLabel),
        ),
      ],
    );
  }
}

class DeviceRemoveDialog extends StatelessWidget {
  const DeviceRemoveDialog({
    super.key,
    required this.title,
    required this.message,
    required this.cancelLabel,
    required this.removeLabel,
  });

  final String title;
  final String message;
  final String cancelLabel;
  final String removeLabel;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(title),
      content: Text(message),
      actions: [
        TextButton(
          style: TextButton.styleFrom(foregroundColor: accent),
          onPressed: () => Navigator.pop(context, false),
          child: Text(cancelLabel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(
            removeLabel,
            style: const TextStyle(color: AppColors.danger),
          ),
        ),
      ],
    );
  }
}
