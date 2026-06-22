import 'package:flutter/material.dart';
import '../../theme/app_theme.dart';

class DropdownOverlay extends StatelessWidget {
  const DropdownOverlay({
    super.key,
    required this.top,
    required this.onClose,
    required this.child,
    this.alignment = Alignment.topLeft,
  });

  final double top;
  final VoidCallback onClose;
  final Widget child;
  final Alignment alignment;

  @override
  Widget build(BuildContext context) {
    return Positioned.fill(
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onClose,
        child: Container(
          color: AppColors.backdrop,
          child: Stack(
            children: [
              Positioned(
                top: top,
                left: alignment == Alignment.topRight ? null : AppSpacing.m,
                right: AppSpacing.m,
                child: GestureDetector(
                  onTap: () {},
                  child: Material(
                    color: Colors.transparent,
                    child: Container(
                      decoration: BoxDecoration(
                        color: AppColors.bgSurface,
                        borderRadius: BorderRadius.circular(AppRadius.lg),
                        border: Border.all(color: AppColors.border, width: 0.5),
                        boxShadow: const [
                          BoxShadow(
                            color: Color(0x66000000),
                            blurRadius: 24,
                            offset: Offset(0, 8),
                          ),
                        ],
                      ),
                      clipBehavior: Clip.antiAlias,
                      child: child,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
