import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../theme/app_theme.dart';

class ConnectHint extends StatelessWidget {
  const ConnectHint({
    super.key,
    required this.status,
    required this.hasDevice,
    required this.onConnect,
  });
  final String status;
  final bool hasDevice;
  final VoidCallback onConnect;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.xxl),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            InkWell(
              onTap: onConnect,
              borderRadius: BorderRadius.circular(AppRadius.lg),
              child: Container(
                padding: const EdgeInsets.all(AppSpacing.l),
                child: Icon(Icons.play_circle_outline, size: 40, color: accent),
              ),
            ),
            const SizedBox(height: AppSpacing.s),
            Text(
              hasDevice
                  ? prefs.t('app.tapToConnect')
                  : prefs.t('app.addDevice'),
              style: TextStyle(
                color: AppColors.textPrimary,
                fontSize: 16,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
