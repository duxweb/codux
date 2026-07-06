import 'dart:math' as math;

import 'package:flutter/material.dart';

class TerminalBuiltinGraphic {
  const TerminalBuiltinGraphic.block(this.block)
    : box = null,
      powerline = null,
      braille = null,
      sextant = null;
  const TerminalBuiltinGraphic.box(this.box)
    : block = null,
      powerline = null,
      braille = null,
      sextant = null;
  const TerminalBuiltinGraphic.powerline(this.powerline)
    : block = null,
      box = null,
      braille = null,
      sextant = null;
  const TerminalBuiltinGraphic.braille(this.braille)
    : block = null,
      box = null,
      powerline = null,
      sextant = null;
  const TerminalBuiltinGraphic.sextant(this.sextant)
    : block = null,
      box = null,
      powerline = null,
      braille = null;

  final TerminalBlockGraphic? block;
  final TerminalBoxGraphic? box;
  final TerminalPowerlineGraphic? powerline;

  /// U+2800-28FF dot bitmap (bit i = dot i+1 in Unicode braille order).
  final int? braille;

  /// U+1FB00-1FB3B legacy-computing 2x3 fill bitmap (bit 0 = top-left,
  /// row-major).
  final int? sextant;
}

/// Powerline separators (U+E0B0–U+E0BF) are drawn as cell-exact vectors: font
/// glyphs follow the em box and leave gaps against the padded terminal cell.
enum TerminalPowerlineGraphic {
  triangleRight,
  chevronRight,
  triangleLeft,
  chevronLeft,
  semicircleRight,
  semicircleRightLine,
  semicircleLeft,
  semicircleLeftLine,
  triangleLowerLeft,
  diagonalBack,
  triangleLowerRight,
  diagonalForward,
  triangleUpperLeft,
  triangleUpperRight,
}

enum TerminalBlockGraphicKind { full, upper, lower, left, right, quadrants }

class TerminalBlockGraphic {
  const TerminalBlockGraphic.full()
    : kind = TerminalBlockGraphicKind.full,
      ratio = 1,
      upperLeft = false,
      upperRight = false,
      lowerLeft = false,
      lowerRight = false;

  const TerminalBlockGraphic.upper(this.ratio)
    : kind = TerminalBlockGraphicKind.upper,
      upperLeft = false,
      upperRight = false,
      lowerLeft = false,
      lowerRight = false;

  const TerminalBlockGraphic.lower(this.ratio)
    : kind = TerminalBlockGraphicKind.lower,
      upperLeft = false,
      upperRight = false,
      lowerLeft = false,
      lowerRight = false;

  const TerminalBlockGraphic.left(this.ratio)
    : kind = TerminalBlockGraphicKind.left,
      upperLeft = false,
      upperRight = false,
      lowerLeft = false,
      lowerRight = false;

  const TerminalBlockGraphic.right(this.ratio)
    : kind = TerminalBlockGraphicKind.right,
      upperLeft = false,
      upperRight = false,
      lowerLeft = false,
      lowerRight = false;

  const TerminalBlockGraphic.quadrants({
    required this.upperLeft,
    required this.upperRight,
    required this.lowerLeft,
    required this.lowerRight,
  }) : kind = TerminalBlockGraphicKind.quadrants,
       ratio = 1;

  final TerminalBlockGraphicKind kind;
  final double ratio;
  final bool upperLeft;
  final bool upperRight;
  final bool lowerLeft;
  final bool lowerRight;
}

class TerminalBoxGraphic {
  const TerminalBoxGraphic({
    required this.left,
    required this.right,
    required this.up,
    required this.down,
    required this.weight,
    required this.isDouble,
  });

  final bool left;
  final bool right;
  final bool up;
  final bool down;
  final TerminalBoxWeight weight;
  final bool isDouble;
}

enum TerminalBoxWeight { light, heavy }

int? terminalCellCodepoint(String text) {
  final runes = text.runes.iterator;
  if (!runes.moveNext()) return null;
  final codepoint = runes.current;
  return runes.moveNext() ? null : codepoint;
}

TerminalBuiltinGraphic? terminalBuiltinGraphic(int codepoint) {
  if (codepoint >= 0xe0b0 && codepoint <= 0xe0bf) {
    final powerline = _terminalPowerlineGraphic(codepoint);
    return powerline == null
        ? null
        : TerminalBuiltinGraphic.powerline(powerline);
  }
  if (codepoint >= 0x2800 && codepoint <= 0x28ff) {
    return TerminalBuiltinGraphic.braille(codepoint - 0x2800);
  }
  if (codepoint >= 0x1fb00 && codepoint <= 0x1fb3b) {
    return TerminalBuiltinGraphic.sextant(_sextantPattern(codepoint));
  }
  if (codepoint < 0x2500 || codepoint > 0x259f) return null;
  final block = _terminalBlockGraphic(codepoint);
  if (block != null) return TerminalBuiltinGraphic.block(block);
  final box = _terminalBoxGraphic(codepoint);
  if (box != null) return TerminalBuiltinGraphic.box(box);
  return null;
}

TerminalPowerlineGraphic? _terminalPowerlineGraphic(int codepoint) {
  return switch (codepoint) {
    0xe0b0 => TerminalPowerlineGraphic.triangleRight,
    0xe0b1 => TerminalPowerlineGraphic.chevronRight,
    0xe0b2 => TerminalPowerlineGraphic.triangleLeft,
    0xe0b3 => TerminalPowerlineGraphic.chevronLeft,
    0xe0b4 => TerminalPowerlineGraphic.semicircleRight,
    0xe0b5 => TerminalPowerlineGraphic.semicircleRightLine,
    0xe0b6 => TerminalPowerlineGraphic.semicircleLeft,
    0xe0b7 => TerminalPowerlineGraphic.semicircleLeftLine,
    0xe0b8 => TerminalPowerlineGraphic.triangleLowerLeft,
    0xe0b9 || 0xe0bf => TerminalPowerlineGraphic.diagonalBack,
    0xe0ba => TerminalPowerlineGraphic.triangleLowerRight,
    0xe0bb || 0xe0bd => TerminalPowerlineGraphic.diagonalForward,
    0xe0bc => TerminalPowerlineGraphic.triangleUpperLeft,
    0xe0be => TerminalPowerlineGraphic.triangleUpperRight,
    _ => null,
  };
}

TerminalBuiltinGraphic? terminalBuiltinGraphicForText(String text) {
  final codepoint = terminalCellCodepoint(text);
  return codepoint == null ? null : terminalBuiltinGraphic(codepoint);
}

void paintTerminalBuiltinGraphic(
  Canvas canvas,
  Rect bounds,
  Color color,
  TerminalBuiltinGraphic graphic,
) {
  final paint = Paint()..color = color;
  final block = graphic.block;
  if (block != null) {
    _paintBlock(canvas, paint, bounds, block);
    return;
  }
  final box = graphic.box;
  if (box != null) {
    _paintBox(canvas, paint, bounds, box);
    return;
  }
  final powerline = graphic.powerline;
  if (powerline != null) {
    _paintPowerline(canvas, paint, bounds, powerline);
    return;
  }
  final braille = graphic.braille;
  if (braille != null) {
    _paintBraille(canvas, paint, bounds, braille);
    return;
  }
  final sextant = graphic.sextant;
  if (sextant != null) _paintSextant(canvas, paint, bounds, sextant);
}

// The sextant range skips the patterns that already exist as half/full
// blocks (left column, right column, full), so the fill index jumps by one
// at each gap.
int _sextantPattern(int codepoint) {
  final index = codepoint - 0x1fb00;
  if (index <= 19) return index + 1;
  if (index <= 39) return index + 2;
  return index + 3;
}

// Unicode braille dot order: bits 0-2 left rows 0-2, 3-5 right rows 0-2,
// 6 left row 3, 7 right row 3.
const List<(double, double)> _brailleDotCells = [
  (0, 0),
  (0, 1),
  (0, 2),
  (1, 0),
  (1, 1),
  (1, 2),
  (0, 3),
  (1, 3),
];

void _paintBraille(Canvas canvas, Paint paint, Rect bounds, int dots) {
  final subWidth = bounds.width * 0.5;
  final subHeight = bounds.height * 0.25;
  final dot = math.max(math.min(subWidth, subHeight) * 0.5, 1.0);
  for (var bit = 0; bit < 8; bit += 1) {
    if (dots & (1 << bit) == 0) continue;
    final (col, row) = _brailleDotCells[bit];
    final centerX = bounds.left + subWidth * (col + 0.5);
    final centerY = bounds.top + subHeight * (row + 0.5);
    _paintRect(
      canvas,
      paint,
      centerX - dot * 0.5,
      centerY - dot * 0.5,
      centerX + dot * 0.5,
      centerY + dot * 0.5,
    );
  }
}

void _paintSextant(Canvas canvas, Paint paint, Rect bounds, int fills) {
  for (var bit = 0; bit < 6; bit += 1) {
    if (fills & (1 << bit) == 0) continue;
    final col = (bit % 2).toDouble();
    final row = (bit ~/ 2).toDouble();
    _paintFraction(canvas, paint, bounds, col * 0.5, row / 3, 0.5, 1 / 3);
  }
}

void _paintPowerline(
  Canvas canvas,
  Paint paint,
  Rect bounds,
  TerminalPowerlineGraphic graphic,
) {
  final x = bounds.left;
  final y = bounds.top;
  final right = bounds.right;
  final bottom = bounds.bottom;
  final middle = (y + bottom) * 0.5;
  switch (graphic) {
    case TerminalPowerlineGraphic.triangleRight:
      _paintPolygon(canvas, paint, [
        Offset(x, y),
        Offset(right, middle),
        Offset(x, bottom),
      ]);
    case TerminalPowerlineGraphic.triangleLeft:
      _paintPolygon(canvas, paint, [
        Offset(right, y),
        Offset(x, middle),
        Offset(right, bottom),
      ]);
    case TerminalPowerlineGraphic.chevronRight:
      _paintPolyline(canvas, paint, [
        Offset(x, y),
        Offset(right, middle),
        Offset(x, bottom),
      ]);
    case TerminalPowerlineGraphic.chevronLeft:
      _paintPolyline(canvas, paint, [
        Offset(right, y),
        Offset(x, middle),
        Offset(right, bottom),
      ]);
    case TerminalPowerlineGraphic.semicircleRight:
      _paintPolygon(canvas, paint, _semicirclePoints(bounds, true));
    case TerminalPowerlineGraphic.semicircleLeft:
      _paintPolygon(canvas, paint, _semicirclePoints(bounds, false));
    case TerminalPowerlineGraphic.semicircleRightLine:
      _paintPolyline(canvas, paint, _semicirclePoints(bounds, true));
    case TerminalPowerlineGraphic.semicircleLeftLine:
      _paintPolyline(canvas, paint, _semicirclePoints(bounds, false));
    case TerminalPowerlineGraphic.triangleLowerLeft:
      _paintPolygon(canvas, paint, [
        Offset(x, y),
        Offset(right, bottom),
        Offset(x, bottom),
      ]);
    case TerminalPowerlineGraphic.triangleLowerRight:
      _paintPolygon(canvas, paint, [
        Offset(right, y),
        Offset(right, bottom),
        Offset(x, bottom),
      ]);
    case TerminalPowerlineGraphic.triangleUpperLeft:
      _paintPolygon(canvas, paint, [
        Offset(x, y),
        Offset(right, y),
        Offset(x, bottom),
      ]);
    case TerminalPowerlineGraphic.triangleUpperRight:
      _paintPolygon(canvas, paint, [
        Offset(x, y),
        Offset(right, y),
        Offset(right, bottom),
      ]);
    case TerminalPowerlineGraphic.diagonalBack:
      _paintPolyline(canvas, paint, [Offset(x, y), Offset(right, bottom)]);
    case TerminalPowerlineGraphic.diagonalForward:
      _paintPolyline(canvas, paint, [Offset(x, bottom), Offset(right, y)]);
  }
}

List<Offset> _semicirclePoints(Rect bounds, bool bulgeRight) {
  final width = bounds.width;
  final halfHeight = bounds.height * 0.5;
  final middle = bounds.top + halfHeight;
  final flatX = bulgeRight ? bounds.left : bounds.right;
  final direction = bulgeRight ? 1.0 : -1.0;
  return List.generate(17, (step) {
    final angle = -math.pi / 2 + math.pi * step / 16;
    return Offset(
      flatX + direction * width * math.cos(angle),
      middle + halfHeight * math.sin(angle),
    );
  });
}

void _paintPolygon(Canvas canvas, Paint paint, List<Offset> points) {
  final path = Path()..addPolygon(points, true);
  canvas.drawPath(path, paint);
}

void _paintPolyline(Canvas canvas, Paint paint, List<Offset> points) {
  final stroke = Paint()
    ..color = paint.color
    ..style = PaintingStyle.stroke
    ..strokeWidth = 1;
  final path = Path()..addPolygon(points, false);
  canvas.drawPath(path, stroke);
}

TerminalBlockGraphic? _terminalBlockGraphic(int codepoint) {
  switch (codepoint) {
    case 0x2580:
      return const TerminalBlockGraphic.upper(0.5);
    case 0x2581:
      return const TerminalBlockGraphic.lower(0.125);
    case 0x2582:
      return const TerminalBlockGraphic.lower(0.25);
    case 0x2583:
      return const TerminalBlockGraphic.lower(0.375);
    case 0x2584:
      return const TerminalBlockGraphic.lower(0.5);
    case 0x2585:
      return const TerminalBlockGraphic.lower(0.625);
    case 0x2586:
      return const TerminalBlockGraphic.lower(0.75);
    case 0x2587:
      return const TerminalBlockGraphic.lower(0.875);
    case 0x2588:
      return const TerminalBlockGraphic.full();
    case 0x2589:
      return const TerminalBlockGraphic.left(0.875);
    case 0x258a:
      return const TerminalBlockGraphic.left(0.75);
    case 0x258b:
      return const TerminalBlockGraphic.left(0.625);
    case 0x258c:
      return const TerminalBlockGraphic.left(0.5);
    case 0x258d:
      return const TerminalBlockGraphic.left(0.375);
    case 0x258e:
      return const TerminalBlockGraphic.left(0.25);
    case 0x258f:
      return const TerminalBlockGraphic.left(0.125);
    case 0x2590:
      return const TerminalBlockGraphic.right(0.5);
    case 0x2594:
      return const TerminalBlockGraphic.upper(0.125);
    case 0x2595:
      return const TerminalBlockGraphic.right(0.125);
    case 0x2596:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: false,
        upperRight: false,
        lowerLeft: true,
        lowerRight: false,
      );
    case 0x2597:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: false,
        upperRight: false,
        lowerLeft: false,
        lowerRight: true,
      );
    case 0x2598:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: true,
        upperRight: false,
        lowerLeft: false,
        lowerRight: false,
      );
    case 0x2599:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: true,
        upperRight: false,
        lowerLeft: true,
        lowerRight: true,
      );
    case 0x259a:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: true,
        upperRight: false,
        lowerLeft: false,
        lowerRight: true,
      );
    case 0x259b:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: true,
        upperRight: true,
        lowerLeft: true,
        lowerRight: false,
      );
    case 0x259c:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: true,
        upperRight: true,
        lowerLeft: false,
        lowerRight: true,
      );
    case 0x259d:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: false,
        upperRight: true,
        lowerLeft: false,
        lowerRight: false,
      );
    case 0x259e:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: false,
        upperRight: true,
        lowerLeft: true,
        lowerRight: false,
      );
    case 0x259f:
      return const TerminalBlockGraphic.quadrants(
        upperLeft: false,
        upperRight: true,
        lowerLeft: true,
        lowerRight: true,
      );
  }
  return null;
}

TerminalBoxGraphic? _terminalBoxGraphic(int codepoint) {
  switch (codepoint) {
    case 0x2500:
      return _terminalBox(
        true,
        true,
        false,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2501:
      return _terminalBox(
        true,
        true,
        false,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2502:
      return _terminalBox(
        false,
        false,
        true,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2503:
      return _terminalBox(
        false,
        false,
        true,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2504:
    case 0x2505:
    case 0x2508:
    case 0x2509:
      return _terminalBox(
        true,
        true,
        false,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2506:
    case 0x2507:
    case 0x250a:
    case 0x250b:
      return _terminalBox(
        false,
        false,
        true,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case 0x250c:
      return _terminalBox(
        false,
        true,
        false,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x250d && <= 0x250f:
      return _terminalBox(
        false,
        true,
        false,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2510:
      return _terminalBox(
        true,
        false,
        false,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x2511 && <= 0x2513:
      return _terminalBox(
        true,
        false,
        false,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2514:
      return _terminalBox(
        false,
        true,
        true,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x2515 && <= 0x2517:
      return _terminalBox(
        false,
        true,
        true,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2518:
      return _terminalBox(
        true,
        false,
        true,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x2519 && <= 0x251b:
      return _terminalBox(
        true,
        false,
        true,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x251c:
      return _terminalBox(
        false,
        true,
        true,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x251d && <= 0x2523:
      return _terminalBox(
        false,
        true,
        true,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2524:
      return _terminalBox(
        true,
        false,
        true,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x2525 && <= 0x252b:
      return _terminalBox(
        true,
        false,
        true,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x252c:
      return _terminalBox(
        true,
        true,
        false,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x252d && <= 0x2533:
      return _terminalBox(
        true,
        true,
        false,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2534:
      return _terminalBox(
        true,
        true,
        true,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x2535 && <= 0x253b:
      return _terminalBox(
        true,
        true,
        true,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x253c:
      return _terminalBox(
        true,
        true,
        true,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case >= 0x253d && <= 0x254b:
      return _terminalBox(
        true,
        true,
        true,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2550:
      return _terminalBox(
        true,
        true,
        false,
        false,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2551:
      return _terminalBox(
        false,
        false,
        true,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2554:
      return _terminalBox(
        false,
        true,
        false,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2557:
      return _terminalBox(
        true,
        false,
        false,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x255a:
      return _terminalBox(
        false,
        true,
        true,
        false,
        TerminalBoxWeight.light,
        true,
      );
    case 0x255d:
      return _terminalBox(
        true,
        false,
        true,
        false,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2560:
      return _terminalBox(
        false,
        true,
        true,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2563:
      return _terminalBox(
        true,
        false,
        true,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2566:
      return _terminalBox(
        true,
        true,
        false,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2569:
      return _terminalBox(
        true,
        true,
        true,
        false,
        TerminalBoxWeight.light,
        true,
      );
    case 0x256c:
      return _terminalBox(
        true,
        true,
        true,
        true,
        TerminalBoxWeight.light,
        true,
      );
    case 0x2574:
      return _terminalBox(
        true,
        false,
        false,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2575:
      return _terminalBox(
        false,
        false,
        true,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2576:
      return _terminalBox(
        false,
        true,
        false,
        false,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2577:
      return _terminalBox(
        false,
        false,
        false,
        true,
        TerminalBoxWeight.light,
        false,
      );
    case 0x2578:
      return _terminalBox(
        true,
        false,
        false,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x2579:
      return _terminalBox(
        false,
        false,
        true,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x257a:
      return _terminalBox(
        false,
        true,
        false,
        false,
        TerminalBoxWeight.heavy,
        false,
      );
    case 0x257b:
      return _terminalBox(
        false,
        false,
        false,
        true,
        TerminalBoxWeight.heavy,
        false,
      );
  }
  return null;
}

TerminalBoxGraphic _terminalBox(
  bool left,
  bool right,
  bool up,
  bool down,
  TerminalBoxWeight weight,
  bool isDouble,
) {
  return TerminalBoxGraphic(
    left: left,
    right: right,
    up: up,
    down: down,
    weight: weight,
    isDouble: isDouble,
  );
}

void _paintBlock(
  Canvas canvas,
  Paint paint,
  Rect bounds,
  TerminalBlockGraphic graphic,
) {
  switch (graphic.kind) {
    case TerminalBlockGraphicKind.full:
      _paintFilled(canvas, paint, bounds);
    case TerminalBlockGraphicKind.upper:
      _paintFraction(canvas, paint, bounds, 0, 0, 1, graphic.ratio);
    case TerminalBlockGraphicKind.lower:
      _paintFraction(
        canvas,
        paint,
        bounds,
        0,
        1 - graphic.ratio,
        1,
        graphic.ratio,
      );
    case TerminalBlockGraphicKind.left:
      _paintFraction(canvas, paint, bounds, 0, 0, graphic.ratio, 1);
    case TerminalBlockGraphicKind.right:
      _paintFraction(
        canvas,
        paint,
        bounds,
        1 - graphic.ratio,
        0,
        graphic.ratio,
        1,
      );
    case TerminalBlockGraphicKind.quadrants:
      if (graphic.upperLeft) {
        _paintFraction(canvas, paint, bounds, 0, 0, 0.5, 0.5);
      }
      if (graphic.upperRight) {
        _paintFraction(canvas, paint, bounds, 0.5, 0, 0.5, 0.5);
      }
      if (graphic.lowerLeft) {
        _paintFraction(canvas, paint, bounds, 0, 0.5, 0.5, 0.5);
      }
      if (graphic.lowerRight) {
        _paintFraction(canvas, paint, bounds, 0.5, 0.5, 0.5, 0.5);
      }
  }
}

void _paintBox(
  Canvas canvas,
  Paint paint,
  Rect bounds,
  TerminalBoxGraphic graphic,
) {
  if (graphic.isDouble) {
    _paintDoubleBox(canvas, paint, bounds, graphic);
    return;
  }

  final thickness = graphic.weight == TerminalBoxWeight.light ? 1.0 : 2.0;
  final centerX = (bounds.left + bounds.right) * 0.5;
  final centerY = (bounds.top + bounds.bottom) * 0.5;
  final half = thickness * 0.5;

  if (graphic.left) {
    _paintRect(
      canvas,
      paint,
      bounds.left,
      centerY - half,
      centerX + half,
      centerY + half,
    );
  }
  if (graphic.right) {
    _paintRect(
      canvas,
      paint,
      centerX - half,
      centerY - half,
      bounds.right,
      centerY + half,
    );
  }
  if (graphic.up) {
    _paintRect(
      canvas,
      paint,
      centerX - half,
      bounds.top,
      centerX + half,
      centerY + half,
    );
  }
  if (graphic.down) {
    _paintRect(
      canvas,
      paint,
      centerX - half,
      centerY - half,
      centerX + half,
      bounds.bottom,
    );
  }
}

void _paintDoubleBox(
  Canvas canvas,
  Paint paint,
  Rect bounds,
  TerminalBoxGraphic graphic,
) {
  final centerX = (bounds.left + bounds.right) * 0.5;
  final centerY = (bounds.top + bounds.bottom) * 0.5;
  const gap = 1.5;

  for (final offset in const [-gap, gap]) {
    if (graphic.left) {
      _paintRect(
        canvas,
        paint,
        bounds.left,
        centerY + offset,
        centerX,
        centerY + offset + 1,
      );
    }
    if (graphic.right) {
      _paintRect(
        canvas,
        paint,
        centerX,
        centerY + offset,
        bounds.right,
        centerY + offset + 1,
      );
    }
    if (graphic.up) {
      _paintRect(
        canvas,
        paint,
        centerX + offset,
        bounds.top,
        centerX + offset + 1,
        centerY,
      );
    }
    if (graphic.down) {
      _paintRect(
        canvas,
        paint,
        centerX + offset,
        centerY,
        centerX + offset + 1,
        bounds.bottom,
      );
    }
  }
}

void _paintFraction(
  Canvas canvas,
  Paint paint,
  Rect bounds,
  double xRatio,
  double yRatio,
  double widthRatio,
  double heightRatio,
) {
  final width = bounds.width;
  final height = bounds.height;
  _paintRect(
    canvas,
    paint,
    bounds.left + width * xRatio,
    bounds.top + height * yRatio,
    bounds.left + width * (xRatio + widthRatio),
    bounds.top + height * (yRatio + heightRatio),
  );
}

void _paintRect(
  Canvas canvas,
  Paint paint,
  double left,
  double top,
  double right,
  double bottom,
) {
  _paintFilled(canvas, paint, _snappedRect(left, top, right, bottom));
}

void _paintFilled(Canvas canvas, Paint paint, Rect bounds) {
  if (bounds.width <= 0 || bounds.height <= 0) return;
  canvas.drawRect(bounds, paint);
}

Rect _snappedRect(double left, double top, double right, double bottom) {
  return Rect.fromLTRB(
    left.floorToDouble(),
    top.floorToDouble(),
    right.ceilToDouble(),
    bottom.ceilToDouble(),
  );
}
