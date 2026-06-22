import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';

/// Pad workspace surface palette. Follows the app theme (dark/light) via
/// [CoduxTheme]; the accent is NOT here — widgets read it from
/// `Theme.of(context).colorScheme.secondary`. Text / border / status tokens
/// delegate to [AppColors] so the two palettes share one source of truth; only
/// the pad's own surface layers live here.
class PadColors {
  PadColors._();

  static Color _pick(Color dark, Color light) =>
      CoduxTheme.isLight ? light : dark;

  // Surfaces. Dark mode uses neutral grays (no blue tint) on a deep near-black
  // canvas; light canvas is a clear gray so the white panels read as raised.
  static Color get bg =>
      _pick(const Color(0xFF0A0A0A), const Color(0xFFDCE1E7));
  static Color get panel =>
      _pick(const Color(0xFF161616), const Color(0xFFFFFFFF));
  static Color get header =>
      _pick(const Color(0xFF121212), const Color(0xFFF0F2F5));
  static Color get panelTrack =>
      _pick(const Color(0xFF1F1F1F), const Color(0xFFEAEEF2));
  static Color get card =>
      _pick(const Color(0xFF1E1E1E), const Color(0xFFF6F8FA));
  static Color get cardActive =>
      _pick(const Color(0xFF2A2A2A), const Color(0xFFE7EBEF));

  // Neutral selection/hover wash — a defined gray (the muted-text gray) at low
  // opacity, shared by the worktree row and the terminal tab. Using a real gray
  // hue instead of a flat white/black overlay keeps the highlight consistent
  // across the different panel backgrounds and both themes.
  static Color get surfaceHover => AppColors.textMuted.withValues(alpha: 0.06);
  static Color get surfaceActive => AppColors.textMuted.withValues(alpha: 0.13);

  // Shared tokens (single source of truth in AppColors).
  static Color get border => AppColors.border;
  static Color get textPrimary => AppColors.textPrimary;
  static Color get textSecondary => AppColors.textSecondary;
  static Color get textMuted => AppColors.textMuted;
  static Color get textSubtle => AppColors.textSubtle;
  static Color get success => AppColors.success;
  static Color get warning => AppColors.warning;
  static Color get danger => AppColors.danger;

  static AIStatsPanelColors get statsPanel => AIStatsPanelColors(
    background: bg,
    card: panel,
    cardHeader: header,
    cardBorder: border,
    track: panelTrack,
  );

  // Chart / language palette (theme-independent).
  static const chartA = Color(0xFF7C6CF0);
  static const chartB = Color(0xFF3B82F6);
  static const chartC = Color(0xFF34D399);
  static const chartD = Color(0xFFF59E0B);
  static const chartE = Color(0xFF64748B);
}

class PadMetrics {
  PadMetrics._();

  static const panelRadius = 16.0;
  static const panelBorderWidth = 0.5;
  static const leftColumnWidth = 264.0;
  static const rightColumnWidth = 304.0;
}

class PadPanelSurface extends StatelessWidget {
  const PadPanelSurface({super.key, required this.child, this.width});

  final Widget child;
  final double? width;

  @override
  Widget build(BuildContext context) {
    final radius = BorderRadius.circular(PadMetrics.panelRadius);
    return SizedBox(
      width: width,
      child: ClipRRect(
        borderRadius: radius,
        clipBehavior: Clip.antiAlias,
        child: Container(
          decoration: BoxDecoration(color: PadColors.panel),
          foregroundDecoration: BoxDecoration(
            borderRadius: radius,
            border: Border.all(
              color: PadColors.border,
              width: PadMetrics.panelBorderWidth,
            ),
          ),
          child: child,
        ),
      ),
    );
  }
}
