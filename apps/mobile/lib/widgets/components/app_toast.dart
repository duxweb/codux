import 'package:flutter/material.dart';
import '../../theme/app_theme.dart';

class AppToast extends StatelessWidget {
  const AppToast({super.key, required this.message, required this.bottomInset});

  final String message;
  final double bottomInset;

  @override
  Widget build(BuildContext context) => Positioned.fill(
    child: IgnorePointer(
      child: Center(
        child: AnimatedSwitcher(
          duration: const Duration(milliseconds: 160),
          child: ConstrainedBox(
            key: ValueKey(message),
            constraints: const BoxConstraints(maxWidth: 280),
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: AppColors.bgElevated.withValues(alpha: 0.96),
                borderRadius: BorderRadius.circular(999),
                border: Border.all(color: AppColors.border, width: 0.5),
                boxShadow: const [
                  BoxShadow(
                    color: Color(0x66000000),
                    blurRadius: 24,
                    offset: Offset(0, 10),
                  ),
                ],
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: AppSpacing.l,
                  vertical: AppSpacing.m,
                ),
                child: Text(
                  message,
                  textAlign: TextAlign.center,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: AppColors.textPrimary,
                    fontSize: AppTextSize.body,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

class BlockingLoading extends StatelessWidget {
  const BlockingLoading({super.key, required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return Positioned.fill(
      child: ColoredBox(
        color: AppColors.backdrop,
        child: Center(
          child: Container(
            width: 180,
            padding: const EdgeInsets.all(AppSpacing.xl),
            decoration: BoxDecoration(
              color: AppColors.bgSurface,
              borderRadius: BorderRadius.circular(AppRadius.lg),
              border: Border.all(color: AppColors.border, width: 0.5),
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                CircularProgressIndicator(color: accent),
                const SizedBox(height: AppSpacing.l),
                Text(
                  message,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    color: AppColors.textPrimary,
                    fontSize: AppTextSize.body,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
