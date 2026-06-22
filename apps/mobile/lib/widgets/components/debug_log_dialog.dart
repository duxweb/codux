import 'package:flutter/material.dart';

import '../../services/log_service.dart';
import '../../theme/app_theme.dart';

class DebugLogDialog extends StatefulWidget {
  const DebugLogDialog({
    super.key,
    required this.title,
    required this.emptyLabel,
    required this.clearLabel,
    required this.copyLabel,
    required this.exportLabel,
    required this.closeLabel,
    required this.onCopy,
    required this.onExport,
  });

  final String title;
  final String emptyLabel;
  final String clearLabel;
  final String copyLabel;
  final String exportLabel;
  final String closeLabel;
  final Future<void> Function(String text) onCopy;
  final Future<void> Function(String text) onExport;

  @override
  State<DebugLogDialog> createState() => _DebugLogDialogState();
}

class _DebugLogDialogState extends State<DebugLogDialog> {
  late List<CoduxLogEntry> _entries;

  @override
  void initState() {
    super.initState();
    _entries = CoduxLog.snapshot();
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final text = _entries.map((entry) => entry.format()).join('\n');
    return AlertDialog(
      backgroundColor: AppColors.bgSurface,
      title: Text(widget.title),
      contentPadding: const EdgeInsets.fromLTRB(20, 12, 20, 8),
      content: SizedBox(
        width: double.maxFinite,
        height: 420,
        child: _entries.isEmpty
            ? Center(
                child: Text(
                  widget.emptyLabel,
                  style: TextStyle(color: AppColors.textMuted),
                ),
              )
            : SelectableText(
                text,
                style: TextStyle(
                  color: AppColors.textSecondary,
                  fontSize: 11,
                  height: 1.35,
                  fontFamily: 'monospace',
                ),
              ),
      ),
      actionsPadding: const EdgeInsets.fromLTRB(12, 0, 12, 10),
      actions: [
        SizedBox(
          width: double.infinity,
          child: Row(
            children: [
              TextButton(
                style: TextButton.styleFrom(
                  foregroundColor: AppColors.danger,
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                ),
                onPressed: _entries.isEmpty
                    ? null
                    : () {
                        CoduxLog.clear();
                        setState(() => _entries = CoduxLog.snapshot());
                      },
                child: Text(widget.clearLabel),
              ),
              const Spacer(),
              TextButton(
                style: TextButton.styleFrom(
                  foregroundColor: accent,
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                ),
                onPressed: _entries.isEmpty ? null : () => widget.onCopy(text),
                child: Text(widget.copyLabel),
              ),
              TextButton(
                style: TextButton.styleFrom(
                  foregroundColor: accent,
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                ),
                onPressed: _entries.isEmpty
                    ? null
                    : () => widget.onExport(text),
                child: Text(widget.exportLabel),
              ),
              TextButton(
                style: TextButton.styleFrom(
                  foregroundColor: accent,
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                ),
                onPressed: () => Navigator.pop(context),
                child: Text(widget.closeLabel),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
