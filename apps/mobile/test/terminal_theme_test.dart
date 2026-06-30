import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/theme/terminal_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('dark terminal theme remaps unreadable neutral foregrounds', () {
    final colors = TerminalTheme.resolveCellColors(
      fg: {'kind': 'named', 'name': 'Black'},
      bg: {'kind': 'default'},
      inverse: false,
    );

    expect(colors.bg, AppColors.terminalBg);
    expect(colors.fg, AppColors.textPrimary);
    expect(colors.drawBackground, isFalse);
  });

  test('dark terminal theme keeps readable ansi colors intact', () {
    final colors = TerminalTheme.resolveCellColors(
      fg: {'kind': 'named', 'name': 'Green'},
      bg: {'kind': 'default'},
      inverse: false,
    );

    expect(colors.fg, isNot(AppColors.textPrimary));
  });

  test('dark terminal theme preserves explicit rgb gradients', () {
    final first = TerminalTheme.resolveCellColors(
      fg: {'kind': 'rgb', 'r': 68, 'g': 72, 'b': 78},
      bg: {'kind': 'default'},
      inverse: false,
    );
    final second = TerminalTheme.resolveCellColors(
      fg: {'kind': 'rgb', 'r': 88, 'g': 92, 'b': 98},
      bg: {'kind': 'default'},
      inverse: false,
    );

    expect(first.fg, const Color(0xFF44484E));
    expect(second.fg, const Color(0xFF585C62));
    expect(first.fg, isNot(second.fg));
  });

  test('dark terminal theme dims faint foregrounds toward background', () {
    final normal = TerminalTheme.resolveCellColors(
      fg: {'kind': 'rgb', 'r': 230, 'g': 237, 'b': 243},
      bg: {'kind': 'default'},
      inverse: false,
      dim: false,
    );
    final dimmed = TerminalTheme.resolveCellColors(
      fg: {'kind': 'rgb', 'r': 230, 'g': 237, 'b': 243},
      bg: {'kind': 'default'},
      inverse: false,
      dim: true,
    );

    expect(dimmed.fg, isNot(normal.fg));
    expect(
      dimmed.fg.computeLuminance(),
      lessThan(normal.fg.computeLuminance()),
    );
    expect(
      dimmed.fg.computeLuminance(),
      greaterThan(AppColors.bgBase.computeLuminance()),
    );
  });

  test('dark terminal theme normalizes host light cell backgrounds', () {
    final colors = TerminalTheme.resolveCellColors(
      fg: {'kind': 'named', 'name': 'Black'},
      bg: {'kind': 'named', 'name': 'White'},
      inverse: false,
    );

    expect(colors.bg, AppColors.terminalElevated);
    expect(colors.fg, AppColors.textPrimary);
    expect(colors.drawBackground, isTrue);
  });
}
