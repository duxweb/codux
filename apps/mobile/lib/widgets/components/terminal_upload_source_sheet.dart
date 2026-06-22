import 'package:flutter/material.dart';

import '../../services/terminal_upload_metadata.dart';
import '../../theme/app_theme.dart';

class TerminalUploadSourceSheet extends StatelessWidget {
  const TerminalUploadSourceSheet({
    super.key,
    required this.fileLabel,
    required this.imageLabel,
  });

  final String fileLabel;
  final String imageLabel;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: AppSpacing.s),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.attach_file_rounded),
              title: Text(fileLabel),
              onTap: () => Navigator.of(context).pop(TerminalUploadSource.file),
            ),
            ListTile(
              leading: const Icon(Icons.image_outlined),
              title: Text(imageLabel),
              onTap: () =>
                  Navigator.of(context).pop(TerminalUploadSource.image),
            ),
          ],
        ),
      ),
    );
  }
}
