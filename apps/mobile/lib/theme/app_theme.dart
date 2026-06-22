import 'package:flutter/material.dart';

/// The currently-resolved app brightness. Set once per frame at the app root
/// (before the widget tree builds) from the user's theme mode + the system
/// setting. The color tokens below read it, so every surface follows the theme.
///
/// This is a global rather than a `Theme.of(context)` lookup on purpose: it lets
/// the existing `AppColors.x` / `PadColors.x` call sites stay untouched. Dart's
/// transitive `const` rule means any widget that reads a token can no longer be
/// `const`, which in turn forces its ancestors to rebuild when the root rebuilds
/// on a theme change — so the switch propagates correctly.
class CoduxTheme {
  CoduxTheme._();

  static Brightness brightness = Brightness.dark;

  static bool get isLight => brightness == Brightness.light;

  static Color _pick(Color dark, Color light) => isLight ? light : dark;
}

class AppColors {
  // Surfaces
  // Dark mode uses neutral grays (no blue tint) and a deep near-black canvas.
  // Light canvas is a clear gray (not near-white) so white cards/buttons pop now
  // that surfaces are borderless.
  static Color get bgBase =>
      CoduxTheme._pick(const Color(0xFF0A0A0A), const Color(0xFFDCE1E7));
  static Color get bgSurface =>
      CoduxTheme._pick(const Color(0xFF161616), const Color(0xFFFFFFFF));
  static Color get bgElevated =>
      CoduxTheme._pick(const Color(0xFF242424), const Color(0xFFEAEEF2));
  static Color get border =>
      CoduxTheme._pick(const Color(0xFF2C2C2C), const Color(0xFFD0D7DE));

  // Accent / brand (theme-independent; the live accent comes from colorScheme).
  static const accent = Color(0xFFD7FF61);
  static const cyan = Color(0xFF00B8D9);
  static const accentSoft = Color(0x2400B8D9);

  // Status (read on both themes).
  static const success = Color(0xFF22C55E);
  static const warning = Color(0xFFFACC15);
  static const danger = Color(0xFFEF4444);

  // Text
  static Color get textPrimary =>
      CoduxTheme._pick(const Color(0xFFECECEC), const Color(0xFF1F2328));
  static Color get textSecondary =>
      CoduxTheme._pick(const Color(0xFFB0B0B0), const Color(0xFF57606A));
  static Color get textMuted =>
      CoduxTheme._pick(const Color(0xFF8A8A8A), const Color(0xFF6E7781));
  static Color get textSubtle =>
      CoduxTheme._pick(const Color(0xFF6A6A6A), const Color(0xFF8C959F));

  // Modal scrim.
  static Color get backdrop =>
      CoduxTheme._pick(const Color(0x8B000000), const Color(0x52000000));

  // Terminal surfaces + chrome (tab strip, input toolbar) stay dark in both
  // themes — the PTY palette is dark-tuned, so light-mode chrome around it would
  // clash. Neutral dark constants, never theme-resolved.
  static const terminalBg = Color(0xFF0D0D0D);
  static const terminalText = Color(0xFFECECEC);
  static const terminalTextDim = Color(0xFFB0B0B0);
  static const terminalTextMuted = Color(0xFF8A8A8A);
  static const terminalChrome = Color(0xFF161616); // tab strip / header bg
  static const terminalElevated = Color(0xFF242424); // tool buttons
  static const terminalHover = Color(0x1FFFFFFF); // active tab wash (white @ 12%)
}

/// Map the persisted theme-mode id (`system` | `light` | `dark`) to/from the
/// Flutter [ThemeMode] used at the app root.
ThemeMode themeModeFromId(String id) => switch (id) {
  'light' => ThemeMode.light,
  'dark' => ThemeMode.dark,
  _ => ThemeMode.system,
};

String themeModeToId(ThemeMode mode) => switch (mode) {
  ThemeMode.light => 'light',
  ThemeMode.dark => 'dark',
  ThemeMode.system => 'system',
};

class AppRadius {
  static const sm = 8.0;
  static const md = 12.0;
  static const lg = 16.0;
}

class AppSpacing {
  static const xs = 4.0;
  static const s = 8.0;
  static const m = 12.0;
  static const l = 16.0;
  static const xl = 20.0;
  static const xxl = 24.0;
}

class AppLayout {
  static const topBarHeight = 56.0;
  static const tabBarHeight = 56.0;
}

class AppTextSize {
  static const small = 12.0;
  static const body = 14.0;
  static const title = 16.0;
}

class AIStatsPanelColors {
  const AIStatsPanelColors({
    required this.background,
    required this.card,
    required this.cardHeader,
    required this.cardBorder,
    required this.track,
  });

  final Color background;
  final Color card;
  final Color cardHeader;
  final Color cardBorder;
  final Color track;
}

ThemeData buildAppTheme({
  Color accent = AppColors.cyan,
  Brightness brightness = Brightness.dark,
}) {
  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    scaffoldBackgroundColor: AppColors.bgBase,
    focusColor: Colors.transparent,
    hoverColor: Colors.transparent,
    splashColor: Colors.transparent,
    highlightColor: Colors.transparent,
    colorScheme: ColorScheme(
      brightness: brightness,
      primary: accent,
      onPrimary: brightness == Brightness.light ? Colors.black : Colors.white,
      secondary: accent,
      onSecondary: brightness == Brightness.light ? Colors.black : Colors.white,
      surface: AppColors.bgSurface,
      onSurface: AppColors.textPrimary,
      error: AppColors.danger,
      onError: Colors.white,
      surfaceContainerHighest: AppColors.bgElevated,
      outline: AppColors.border,
    ),
    textTheme: TextTheme(
      bodyMedium: TextStyle(
        color: AppColors.textPrimary,
        fontSize: AppTextSize.body,
      ),
      bodySmall: TextStyle(
        color: AppColors.textMuted,
        fontSize: AppTextSize.small,
      ),
      titleMedium: TextStyle(
        color: AppColors.textPrimary,
        fontSize: AppTextSize.title,
        fontWeight: FontWeight.w600,
      ),
    ),
    iconTheme: IconThemeData(color: AppColors.textPrimary, size: 20),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(foregroundColor: accent),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: accent),
    inputDecorationTheme: InputDecorationTheme(
      labelStyle: TextStyle(color: AppColors.textMuted),
      hintStyle: TextStyle(color: AppColors.textSubtle),
      focusedBorder: UnderlineInputBorder(
        borderSide: BorderSide(color: accent),
      ),
    ),
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: accent,
      selectionColor: accent.withValues(alpha: 0.24),
      selectionHandleColor: accent,
    ),
  );
}
