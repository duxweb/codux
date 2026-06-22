import 'dart:async';
import 'dart:collection';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:codux_protocol_ffi/codux_protocol_ffi.dart';
import 'package:flutter/material.dart';
import 'package:flutter/physics.dart';
import 'package:flutter/services.dart';

import '../../services/remote_terminal_output_controller.dart';
import '../../theme/app_theme.dart';
import '../../theme/terminal_theme.dart';

/// Fallback for glyphs the primary font lacks. Keep emoji last so terminal
/// symbols prefer text fonts first.
final List<String> _terminalGlyphFallback = Platform.isIOS
    ? const ['Menlo', 'PingFang SC', 'AppleColorEmoji']
    : const [
        'monospace',
        'sans-serif',
        'Noto Sans Symbols 2',
        'Noto Color Emoji',
      ];

/// Self-drawn terminal that renders the shared Rust core's cell grid directly.
/// The Rust `HeadlessTerminalScreen` is the single source of truth — the same
/// snapshot the GPUI desktop draws from — so there is no second VT parser, no
/// ANSI replay, and no scrollback reconstruction to drift.
class SelfDrawnTerminalView extends StatefulWidget {
  const SelfDrawnTerminalView({
    super.key,
    required this.sessionId,
    required this.controller,
    required this.repaintSignal,
    required this.fontSize,
    this.onResize,
    this.onInput,
    this.onSendKey,
    this.onCursorMetrics,
    this.onSelectionChanged,
    this.onRequestKeyboard,
    this.keyboardRequested = false,
    this.keyboardRequestSerial = 0,
  });

  final String? sessionId;
  final RemoteTerminalOutputController controller;

  /// Fires whenever terminal output for the active session changes; the view
  /// re-reads the snapshot (gated by render generation) and repaints.
  final Listenable repaintSignal;
  final double fontSize;
  final void Function(int cols, int rows)? onResize;

  /// Raw typed text (batched by the host send path, same as the native view).
  final ValueChanged<String>? onInput;

  /// Pre-encoded key bytes (enter, backspace, ...), sent immediately.
  final ValueChanged<String>? onSendKey;
  final ValueChanged<TerminalCursorMetrics?>? onCursorMetrics;

  /// Selected text (null when the selection is cleared), for the copy action.
  final ValueChanged<String?>? onSelectionChanged;

  /// Called when the user taps the terminal body, to bring up the keyboard.
  final VoidCallback? onRequestKeyboard;

  final bool keyboardRequested;
  final int keyboardRequestSerial;

  @override
  State<SelfDrawnTerminalView> createState() => _SelfDrawnTerminalViewState();
}

class _SelfDrawnTerminalViewState extends State<SelfDrawnTerminalView>
    with SingleTickerProviderStateMixin {
  static const double _lineHeightMultiplier = 1.3;
  // Zero-width space anchor in the hidden input (kept invisible and harmless if
  // ever emitted), used to detect inserts vs a backspace on an empty field.
  static final String _sentinel = String.fromCharCode(0x200b);
  static final String _fontFamily = Platform.isIOS ? 'Menlo' : 'monospace';

  // Per-run paragraph cache, keyed by (text, color, style). Rendering whole
  // line runs keeps one shared baseline for text and symbols in the same row.
  final Map<String, ui.Paragraph> _glyphCache = {};

  // Hidden anchor input that captures the soft keyboard / IME. The sentinel
  // zero-width space lets us detect both inserted text and a backspace that
  // would otherwise leave the field empty.
  final TextEditingController _inputController = TextEditingController();
  final FocusNode _focusNode = FocusNode();
  bool _resetting = false;

  TerminalScreenSnapshot? _snapshot;
  int _appliedGen = -1;
  double _cellWidth = 0;
  double _cellHeight = 0;
  // Target text baseline within a cell (from the top). Every run is drawn so its
  // own baseline lands here, which keeps fallback-font glyphs (CJK, symbols)
  // aligned with the primary monospace text instead of sitting lower.
  double _glyphBaseline = 0;
  int _cols = 0;
  int _rows = 0;
  TerminalCursorMetrics? _lastCursorMetrics;

  // Momentum (fling) scrolling: a friction simulation drives scroll-pixel
  // deltas after the finger lifts, decelerating to a stop.
  late final AnimationController _fling = AnimationController.unbounded(
    vsync: this,
  );
  double _flingLast = 0;

  // Text selection. Endpoints are anchored to the scrollback as "lines from
  // the bottom" (lfb) + column, so they stay on the same content while the
  // view scrolls. `_selCells` accumulates the cells the user scrolls over
  // during a selection so a multi-screen selection can still be copied.
  final GlobalKey _termKey = GlobalKey();
  ({int lfb, int col})? _selAnchor;
  ({int lfb, int col})? _selFocus;
  final Map<int, Map<int, TerminalScreenCell>> _selCells = {};
  Timer? _autoScrollTimer;
  int _autoScrollDir = 0;
  Offset? _lastSelectLocal;
  bool _lastSelectMovesAnchor = false;

  // Cursor focus/blink: a focused terminal shows a solid blinking block; an
  // unfocused one shows a hollow outline (mirrors desktop terminal behavior).
  Timer? _blinkTimer;
  bool _cursorBlinkOn = true;

  @override
  void initState() {
    super.initState();
    _measureCell();
    _resetInput();
    _inputController.addListener(_handleInputChange);
    _focusNode.addListener(_onFocusChange);
    _fling.addListener(_onFlingTick);
    widget.repaintSignal.addListener(_onSignal);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _refresh(force: true);
      _applyKeyboard();
    });
  }

  @override
  void didUpdateWidget(covariant SelfDrawnTerminalView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.repaintSignal != oldWidget.repaintSignal) {
      oldWidget.repaintSignal.removeListener(_onSignal);
      widget.repaintSignal.addListener(_onSignal);
    }
    if (widget.fontSize != oldWidget.fontSize) {
      _glyphCache.clear();
      _measureCell();
      _cols = 0;
      _rows = 0;
      _scheduleRefresh();
    }
    if (widget.sessionId != oldWidget.sessionId) {
      _appliedGen = -1;
      _lastCursorMetrics = null;
      _selAnchor = null;
      _selFocus = null;
      _selCells.clear();
      final sessionId = widget.sessionId;
      if (sessionId == null) {
        _snapshot = null;
      } else {
        // The viewport pixel size is unchanged across a switch, so resize the
        // new session's screen to the known grid and read its snapshot
        // synchronously here: the build that follows paints the new session at
        // the correct size on the very first frame -- no stale content and no
        // resize-reflow flashing through.
        if (_cols > 0 && _rows > 0) {
          widget.controller.resizeScreen(sessionId, cols: _cols, rows: _rows);
        }
        _snapshot = widget.controller.screenSnapshot(sessionId);
        _appliedGen = widget.controller.renderGeneration(sessionId);
        // Sync the host PTY to this session's viewport after the frame (so a
        // repaint/TUI app paints at the mobile row count, not the host's old
        // size). Deduped by the viewport controller if already in sync.
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (!mounted || widget.sessionId != sessionId) return;
          if (_cols > 0 && _rows > 0) widget.onResize?.call(_cols, _rows);
          _refresh(force: true);
        });
      }
    }
    if (widget.keyboardRequestSerial != oldWidget.keyboardRequestSerial ||
        widget.keyboardRequested != oldWidget.keyboardRequested) {
      _applyKeyboard();
    }
  }

  /// Start/stop the blink loop on focus change and repaint so the cursor flips
  /// between solid (focused) and hollow (unfocused).
  void _onFocusChange() {
    if (_focusNode.hasFocus) {
      _startBlink();
    } else {
      _stopBlink();
    }
    if (mounted) setState(() {});
  }

  void _startBlink() {
    _blinkTimer?.cancel();
    _cursorBlinkOn = true;
    _blinkTimer = Timer.periodic(const Duration(milliseconds: 530), (_) {
      if (!mounted) return;
      setState(() => _cursorBlinkOn = !_cursorBlinkOn);
    });
  }

  void _stopBlink() {
    _blinkTimer?.cancel();
    _blinkTimer = null;
    _cursorBlinkOn = true;
  }

  @override
  void dispose() {
    widget.repaintSignal.removeListener(_onSignal);
    _inputController.removeListener(_handleInputChange);
    _inputController.dispose();
    _focusNode.removeListener(_onFocusChange);
    _focusNode.dispose();
    _fling.dispose();
    _autoScrollTimer?.cancel();
    _blinkTimer?.cancel();
    super.dispose();
  }

  void _onSignal() => _refresh();

  /// Refresh after the current frame. Used from `didUpdateWidget` so the
  /// snapshot read and its `onCursorMetrics` callback never call `setState`
  /// on an ancestor while the tree is still building.
  void _scheduleRefresh({bool force = true}) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _refresh(force: force);
    });
  }

  void _applyKeyboard() {
    if (!mounted) return;
    if (widget.keyboardRequested) {
      _showKeyboard();
    } else if (_focusNode.hasFocus) {
      _focusNode.unfocus();
    }
  }

  /// Show the soft keyboard. When the field is already focused (e.g. the
  /// keyboard was dismissed by the system or after a selection) `requestFocus`
  /// is a no-op and the IME would not reappear, so re-open it explicitly.
  void _showKeyboard() {
    if (_focusNode.hasFocus) {
      SystemChannels.textInput.invokeMethod('TextInput.show');
    } else {
      _focusNode.requestFocus();
    }
  }

  void _measureCell() {
    // Mirror the desktop (GPUI) terminal exactly: glyphs use the font's natural
    // line metrics, and the tight ascent+descent band is centered within the
    // cell height. Desktop computes `padding_top = (line_height - ascent -
    // descent) / 2` and draws the baseline at `padding_top + ascent`; drawing
    // the natural paragraph at `_glyphTop` reproduces that, since a natural
    // paragraph's box top sits `ascent` above the baseline.
    final paragraph =
        (ui.ParagraphBuilder(
          ui.ParagraphStyle(fontFamily: _fontFamily, fontSize: widget.fontSize),
        )..addText('M')).build()..layout(
          const ui.ParagraphConstraints(width: double.infinity),
        );
    _cellWidth = paragraph.maxIntrinsicWidth;
    _cellHeight = widget.fontSize * _lineHeightMultiplier;
    final lines = paragraph.computeLineMetrics();
    if (lines.isNotEmpty) {
      final m = lines.first;
      // Center the primary font's ascent+descent band, then the baseline sits
      // `ascent` below the band's top.
      _glyphBaseline = (_cellHeight + m.ascent - m.descent) / 2;
    } else {
      _glyphBaseline = (_cellHeight + widget.fontSize) / 2;
    }
  }

  // ---- input ---------------------------------------------------------------

  // The last committed field value we reconciled with the terminal. We diff each
  // change against this and forward only the delta, so an IME that keeps its own
  // accumulating buffer (CJK) never makes us re-send already-sent text.
  String _lastValue = '';

  void _resetInput() {
    _resetting = true;
    _inputController.value = TextEditingValue(
      text: _sentinel,
      selection: const TextSelection.collapsed(offset: 1),
    );
    _lastValue = _sentinel;
    _resetting = false;
  }

  void _handleInputChange() {
    if (_resetting) return;
    final value = _inputController.value;
    // Wait for the IME to commit before emitting composing (CJK candidate) text.
    if (value.composing.isValid && !value.composing.isCollapsed) return;
    final text = value.text;
    final last = _lastValue;
    if (text == last) return;
    // Longest common prefix between the previous value and the new one.
    var prefix = 0;
    final limit = last.length < text.length ? last.length : text.length;
    while (prefix < limit && last.codeUnitAt(prefix) == text.codeUnitAt(prefix)) {
      prefix++;
    }
    final removed = last.length - prefix;
    final added = text.substring(prefix);
    for (var i = 0; i < removed; i++) {
      _sendKey('backspace');
    }
    if (added.isNotEmpty) _sendText(added);
    _lastValue = text;
    // Re-seed the zero-width anchor only when the field has emptied out, so the
    // IME never sees a truly empty field. Never re-seed mid-input: that would
    // desync the IME and make it re-emit text we have already forwarded.
    if (text.length <= _sentinel.length) _resetInput();
  }

  void _sendText(String text) {
    // Newlines map to the Enter key (CR); other text is sent raw through the
    // same batched path the native view used.
    final parts = text.split('\n');
    for (var i = 0; i < parts.length; i++) {
      if (parts[i].isNotEmpty) widget.onInput?.call(parts[i]);
      if (i < parts.length - 1) _sendKey('enter');
    }
  }

  void _sendKey(String key) {
    final bytes = terminalKeyInput(
      key: key,
      applicationCursor: _snapshot?.applicationCursor ?? false,
    );
    if (bytes.isNotEmpty) widget.onSendKey?.call(bytes);
  }

  // ---- snapshot / grid -----------------------------------------------------

  void _refresh({bool force = false}) {
    final sessionId = widget.sessionId;
    if (sessionId == null) {
      if (_snapshot != null) setState(() => _snapshot = null);
      return;
    }
    final gen = widget.controller.renderGeneration(sessionId);
    if (!force && gen == _appliedGen) return;
    final snapshot = widget.controller.screenSnapshot(sessionId);
    _appliedGen = gen;
    if (mounted) setState(() => _snapshot = snapshot);
    _captureSelectionCells();
    _emitCursorMetrics();
  }

  void _emitCursorMetrics() {
    final callback = widget.onCursorMetrics;
    final snapshot = _snapshot;
    if (callback == null || snapshot == null || _cellHeight <= 0) return;
    final metrics = TerminalCursorMetrics(
      row: snapshot.cursor.row,
      col: snapshot.cursor.col,
      lineHeight: _cellHeight,
    );
    if (metrics == _lastCursorMetrics) return;
    _lastCursorMetrics = metrics;
    callback(metrics);
  }

  void _syncGrid(BoxConstraints constraints) {
    final sessionId = widget.sessionId;
    if (sessionId == null || _cellWidth <= 0 || _cellHeight <= 0) return;
    final cols = (constraints.maxWidth / _cellWidth).floor().clamp(1, 1000);
    final rows = (constraints.maxHeight / _cellHeight).floor().clamp(1, 1000);
    if (cols == _cols && rows == _rows) return;
    _cols = cols;
    _rows = rows;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || widget.sessionId != sessionId) return;
      widget.controller.resizeScreen(sessionId, cols: cols, rows: rows);
      widget.onResize?.call(cols, rows);
      _refresh(force: true);
    });
  }

  // ---- scroll --------------------------------------------------------------

  double _wheelAccum = 0;

  void _scrollBy(double pixels) {
    final sessionId = widget.sessionId;
    if (sessionId == null || _cellHeight <= 0 || pixels == 0) return;
    final mode = _snapshot?.inputMode;
    // Forward the gesture to the app only when it owns scrolling:
    //  - mouse tracking on  -> send wheel events (Claude Code, vim, ...);
    //  - alternate screen + alternate-scroll -> translate to arrow keys
    //    (pagers like less). NOTE: the alternate-scroll mode bit is often set
    //    even on the normal screen, so it must be gated by alternateScreen --
    //    otherwise scrolling at a shell prompt sends arrows and cycles command
    //    history instead of scrolling the scrollback.
    if (mode != null) {
      if (mode.mouseTracking) {
        _forwardScroll(pixels, mode, useWheel: true);
        return;
      }
      if (mode.alternateScreen && mode.alternateScroll) {
        _forwardScroll(pixels, mode, useWheel: false);
        return;
      }
    }
    widget.controller.scrollScreenPixels(
      sessionId,
      pixels: pixels,
      cellHeight: _cellHeight,
    );
    _refresh(force: true);
  }

  void _forwardScroll(
    double pixels,
    TerminalScreenInputMode mode, {
    required bool useWheel,
  }) {
    final snapshot = _snapshot;
    if (snapshot == null) return;
    _wheelAccum += pixels;
    // One tick per cell-height of drag; dragging down (positive) reveals older
    // content, i.e. a wheel-up / up-arrow.
    while (_wheelAccum.abs() >= _cellHeight) {
      final up = _wheelAccum > 0;
      _wheelAccum += up ? -_cellHeight : _cellHeight;
      final bytes = useWheel
          ? terminalMouseInput(
              action: 'press',
              button: up ? 'wheelUp' : 'wheelDown',
              row: (snapshot.rows / 2).floor(),
              col: (snapshot.cols / 2).floor(),
              mouseMotion: mode.mouseMotion,
              mouseDrag: mode.mouseDrag,
              sgrMouse: mode.sgrMouse,
              utf8Mouse: mode.utf8Mouse,
            )
          : terminalKeyInput(
              key: up ? 'up' : 'down',
              applicationCursor: mode.applicationCursor,
            );
      if (bytes.isNotEmpty) widget.onSendKey?.call(bytes);
    }
  }

  void _onDragStart(DragStartDetails details) {
    _fling.stop();
    _wheelAccum = 0;
  }

  void _onDragUpdate(DragUpdateDetails details) => _scrollBy(details.delta.dy);

  void _onDragEnd(DragEndDetails details) {
    final velocity = details.velocity.pixelsPerSecond.dy;
    if (_cellHeight <= 0 || velocity.abs() < 80) {
      _settleScroll();
      return;
    }
    // Decelerating momentum scroll: the friction sim's position is the running
    // scroll offset in pixels; each tick we feed the delta to the core.
    _flingLast = 0;
    _fling.value = 0;
    _fling
        .animateWith(FrictionSimulation(0.135, 0, velocity))
        .whenCompleteOrCancel(_settleScroll);
  }

  void _onFlingTick() {
    final value = _fling.value;
    _scrollBy(value - _flingLast);
    _flingLast = value;
  }

  void _settleScroll() {
    final sessionId = widget.sessionId;
    if (sessionId == null) return;
    widget.controller.settleScreenPixelScroll(sessionId);
    _refresh(force: true);
  }

  // ---- selection -----------------------------------------------------------

  double _scrollShift() {
    final snapshot = _snapshot;
    if (snapshot == null) return 0;
    return snapshot.scrollPixelOffset - snapshot.marginRows * _cellHeight;
  }

  /// Lines-from-bottom of a viewport row (row 0 = top). Stable under scrolling.
  int _rowToLfb(int viewportRow) {
    final snapshot = _snapshot;
    if (snapshot == null) return viewportRow;
    return snapshot.displayOffset + (snapshot.rows - 1 - viewportRow);
  }

  /// Inverse of [_rowToLfb] for the current snapshot (may be off-screen).
  int _lfbToRow(int lfb) {
    final snapshot = _snapshot;
    if (snapshot == null) return lfb;
    return snapshot.displayOffset + snapshot.rows - 1 - lfb;
  }

  ({int lfb, int col}) _cellAt(Offset local) {
    final shift = _scrollShift();
    final rows = _snapshot?.rows ?? 1;
    final col = _cellWidth > 0 ? (local.dx / _cellWidth).floor() : 0;
    var row = _cellHeight > 0 ? ((local.dy - shift) / _cellHeight).floor() : 0;
    row = row.clamp(0, rows - 1);
    return (lfb: _rowToLfb(row), col: col < 0 ? 0 : col);
  }

  /// Record the currently-visible cells (keyed by their scrollback line) so a
  /// selection that scrolls across more than one screen can still be copied.
  void _captureSelectionCells() {
    if (_selAnchor == null) return;
    final snapshot = _snapshot;
    if (snapshot == null) return;
    for (final cell in snapshot.cells) {
      if (cell.row < 0 || cell.row >= snapshot.rows) continue;
      (_selCells[_rowToLfb(cell.row)] ??= {})[cell.col] = cell;
    }
  }

  void _onLongPressStart(LongPressStartDetails details) {
    _fling.stop();
    _selCells.clear();
    final cell = _cellAt(details.localPosition);
    setState(() {
      _selAnchor = cell;
      _selFocus = cell;
    });
    _captureSelectionCells();
  }

  void _onLongPressMove(LongPressMoveUpdateDetails details) {
    if (_selAnchor == null) return;
    _extendSelection(details.localPosition, moveAnchor: false);
  }

  void _onLongPressEnd(LongPressEndDetails details) {
    _stopAutoScroll();
    _emitSelection();
  }

  void _extendSelection(Offset local, {required bool moveAnchor}) {
    final cell = _cellAt(local);
    setState(() {
      if (moveAnchor) {
        _selAnchor = cell;
      } else {
        _selFocus = cell;
      }
    });
    _lastSelectLocal = local;
    _lastSelectMovesAnchor = moveAnchor;
    _captureSelectionCells();
    _maybeAutoScroll(local);
  }

  void _maybeAutoScroll(Offset local) {
    final height =
        _termKey.currentContext?.size?.height ?? context.size?.height;
    const zone = 48.0;
    var dir = 0;
    if (local.dy < zone) {
      dir = -1;
    } else if (height != null && local.dy > height - zone) {
      dir = 1;
    }
    if (dir == 0) {
      _stopAutoScroll();
      return;
    }
    if (_autoScrollTimer != null && _autoScrollDir == dir) return;
    _autoScrollDir = dir;
    _autoScrollTimer?.cancel();
    _autoScrollTimer = Timer.periodic(
      const Duration(milliseconds: 40),
      (_) => _autoScrollTick(),
    );
  }

  void _autoScrollTick() {
    if (_selAnchor == null || _cellHeight <= 0) {
      _stopAutoScroll();
      return;
    }
    // Dragging toward the top scrolls into history (positive pixels); toward
    // the bottom scrolls back toward the live tail.
    _scrollBy(_autoScrollDir < 0 ? _cellHeight : -_cellHeight);
    final local = _lastSelectLocal;
    if (local == null) return;
    final cell = _cellAt(local);
    setState(() {
      if (_lastSelectMovesAnchor) {
        _selAnchor = cell;
      } else {
        _selFocus = cell;
      }
    });
    _captureSelectionCells();
  }

  void _stopAutoScroll() {
    _autoScrollTimer?.cancel();
    _autoScrollTimer = null;
    _autoScrollDir = 0;
  }

  void _onHandlePanStart(bool isStart) {
    _fling.stop();
    final range = _normalizedSelection();
    if (range == null) return;
    final (start, end) = range;
    setState(() {
      // The dragged handle becomes the moving focus; the other end is fixed.
      _selAnchor = isStart ? end : start;
      _selFocus = isStart ? start : end;
    });
  }

  void _onHandlePanUpdate(DragUpdateDetails details) {
    final box = _termKey.currentContext?.findRenderObject() as RenderBox?;
    if (box == null) return;
    _extendSelection(
      box.globalToLocal(details.globalPosition),
      moveAnchor: false,
    );
  }

  void _onHandlePanEnd(DragEndDetails details) {
    _stopAutoScroll();
    _emitSelection();
  }

  void _emitSelection() {
    final text = _selectedText();
    widget.onSelectionChanged?.call(text.isEmpty ? null : text);
  }

  void _clearSelection() {
    _stopAutoScroll();
    if (_selAnchor == null && _selFocus == null) return;
    setState(() {
      _selAnchor = null;
      _selFocus = null;
    });
    _selCells.clear();
    widget.onSelectionChanged?.call(null);
  }

  /// Normalize anchor/focus into (start, end) in reading order. Larger lfb is
  /// higher up the scrollback, so the start endpoint has the larger lfb.
  (({int lfb, int col}), ({int lfb, int col}))? _normalizedSelection() {
    final a = _selAnchor;
    final b = _selFocus;
    if (a == null || b == null) return null;
    final aFirst = a.lfb > b.lfb || (a.lfb == b.lfb && a.col <= b.col);
    return aFirst ? (a, b) : (b, a);
  }

  String _selectedText() {
    final range = _normalizedSelection();
    if (range == null) return '';
    final (start, end) = range;
    final cols = _snapshot?.cols ?? 0;
    final buffer = StringBuffer();
    for (var lfb = start.lfb; lfb >= end.lfb; lfb--) {
      final lo = lfb == start.lfb ? start.col : 0;
      final hi = lfb == end.lfb ? end.col : cols - 1;
      final cells = _selCells[lfb] ?? const {};
      final line = StringBuffer();
      var col = lo;
      while (col <= hi) {
        final cell = cells[col];
        if (cell != null && !cell.hidden && cell.text.isNotEmpty) {
          line.write(cell.text);
          col += cell.width < 1 ? 1 : cell.width;
        } else {
          line.write(' ');
          col += 1;
        }
      }
      buffer.write(line.toString().replaceAll(RegExp(r'[ \t]+$'), ''));
      if (lfb > end.lfb) buffer.write('\n');
    }
    return buffer.toString();
  }

  /// Pixel position (in the painter's coordinate space) of a selection corner.
  Offset _selectionCorner(({int lfb, int col}) endpoint, {required bool end}) {
    final shift = _scrollShift();
    final row = _lfbToRow(endpoint.lfb);
    final x = (endpoint.col + (end ? 1 : 0)) * _cellWidth;
    final y = (row + (end ? 1 : 0)) * _cellHeight + shift;
    return Offset(x, y);
  }

  Widget? _buildHandle({required bool isStart}) {
    final range = _normalizedSelection();
    if (range == null || _cellWidth <= 0 || _cellHeight <= 0) return null;
    final endpoint = isStart ? range.$1 : range.$2;
    final row = _lfbToRow(endpoint.lfb);
    final rows = _snapshot?.rows ?? 0;
    if (row < 0 || row >= rows) return null; // endpoint scrolled off-screen
    final corner = _selectionCorner(endpoint, end: !isStart);
    const target = 24.0;
    return Positioned(
      left: corner.dx - target / 2,
      top: isStart ? corner.dy - target : corner.dy,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onPanStart: (_) => _onHandlePanStart(isStart),
        onPanUpdate: _onHandlePanUpdate,
        onPanEnd: _onHandlePanEnd,
        child: SizedBox(
          width: target,
          height: target,
          child: Center(
            child: Container(
              width: 14,
              height: 14,
              decoration: const BoxDecoration(
                color: Color(0xFF409CFF),
                shape: BoxShape.circle,
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Intercept the terminal control keys that the focused IME field would
  /// otherwise eat (arrows move the caret, tab traverses focus, esc/function
  /// keys have no text effect) and the Ctrl+letter control bytes. Printable
  /// characters and CJK composition are deliberately left to the EditableText /
  /// IME path (`_handleInputChange`), so the IME composes candidates instead of
  /// the raw pinyin letters leaking into the terminal, and nothing double-sends.
  /// This fires as an ancestor of the hidden input, before the app-level text
  /// editing shortcuts, so a connected hardware keyboard works once the terminal
  /// is focused (a tap raises the IME / focuses the field).
  KeyEventResult _handleHardwareKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    // While the IME is composing (CJK candidate bar open) let it own every key.
    final composing = _inputController.value.composing;
    if (composing.isValid && !composing.isCollapsed) {
      return KeyEventResult.ignored;
    }
    final keyboard = HardwareKeyboard.instance;
    final ctrl = keyboard.isControlPressed;
    final alt = keyboard.isAltPressed;
    final shift = keyboard.isShiftPressed;
    final appCursor = _snapshot?.applicationCursor ?? false;
    // Ctrl+letter → control byte (Ctrl+C = 0x03, …), portable and host-safe.
    final ch = event.character;
    if (ctrl && !alt && ch != null && ch.length == 1) {
      final upper = ch.toUpperCase().codeUnitAt(0);
      if (upper >= 0x41 && upper <= 0x5A) {
        widget.onSendKey?.call(String.fromCharCode(upper - 0x40));
        return KeyEventResult.handled;
      }
    }
    final named = _hardwareKeyName(event.logicalKey);
    if (named != null) {
      final bytes = terminalKeyInput(
        key: named,
        shift: shift,
        alt: alt,
        control: ctrl,
        applicationCursor: appCursor,
      );
      if (bytes.isNotEmpty) {
        widget.onSendKey?.call(bytes);
        return KeyEventResult.handled;
      }
    }
    // Printable text → fall through to the IME field (composition-safe).
    return KeyEventResult.ignored;
  }

  /// Control keys the terminal needs that the IME field has no text effect for
  /// (or would mishandle). Enter and Backspace are intentionally excluded: the
  /// hidden input maps a committed newline to Enter and an empty field to
  /// Backspace, so intercepting them here would double-send.
  String? _hardwareKeyName(LogicalKeyboardKey key) {
    if (key == LogicalKeyboardKey.tab) return 'tab';
    if (key == LogicalKeyboardKey.escape) return 'esc';
    if (key == LogicalKeyboardKey.arrowUp) return 'up';
    if (key == LogicalKeyboardKey.arrowDown) return 'down';
    if (key == LogicalKeyboardKey.arrowLeft) return 'left';
    if (key == LogicalKeyboardKey.arrowRight) return 'right';
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final startHandle = _buildHandle(isStart: true);
    final endHandle = _buildHandle(isStart: false);
    return Focus(
      // Ancestor key handler: intercepts terminal control keys bubbling up from
      // the focused hidden input before the app-level text editing shortcuts.
      // Not itself focusable, so it never steals focus from the IME field.
      canRequestFocus: false,
      skipTraversal: true,
      onKeyEvent: _handleHardwareKey,
      child: Stack(
        key: _termKey,
        fit: StackFit.expand,
        children: [
          // Hidden input anchor for the soft keyboard / IME. It fills the area
          // but is fully transparent and focused programmatically; the gesture
          // layer above is opaque, so it intercepts all pointers and the anchor
          // never shows a caret or selection of its own. EditableText (vs
          // TextField) needs no Material ancestor.
          EditableText(
            controller: _inputController,
            focusNode: _focusNode,
            style: const TextStyle(
              fontSize: 1,
              height: 1,
              color: Color(0x00000000),
            ),
            cursorColor: const Color(0x00000000),
            backgroundCursorColor: const Color(0x00000000),
            // A plain text field with suggestions enabled summons the user's full
            // IME (incl. CJK composition / candidate bar) rather than MIUI's
            // basic/secure keyboard. Autocorrect stays off for terminal input.
            keyboardType: TextInputType.text,
            maxLines: null,
            autocorrect: false,
            enableSuggestions: true,
            showCursor: false,
            rendererIgnoresPointer: true,
          ),
          GestureDetector(
            behavior: HitTestBehavior.opaque,
            // Tapping clears any selection and raises the soft keyboard by
            // focusing the hidden input. A connected hardware keyboard is then
            // routed through the same field (Android keeps the soft IME hidden
            // while it's attached), so CJK composition works and control keys
            // are intercepted by the ancestor key handler.
            onTap: () {
              _clearSelection();
              _showKeyboard();
              widget.onRequestKeyboard?.call();
            },
            onVerticalDragStart: _onDragStart,
            onVerticalDragUpdate: _onDragUpdate,
            onVerticalDragEnd: _onDragEnd,
            onLongPressStart: _onLongPressStart,
            onLongPressMoveUpdate: _onLongPressMove,
            onLongPressEnd: _onLongPressEnd,
            child: LayoutBuilder(
              builder: (context, constraints) {
                _syncGrid(constraints);
                final selection = _normalizedSelection();
                final selStart = selection == null
                    ? null
                    : (row: _lfbToRow(selection.$1.lfb), col: selection.$1.col);
                final selEnd = selection == null
                    ? null
                    : (row: _lfbToRow(selection.$2.lfb), col: selection.$2.col);
                return ColoredBox(
                  color: AppColors.terminalBg,
                  child: CustomPaint(
                    size: Size(constraints.maxWidth, constraints.maxHeight),
                    painter: _TerminalGridPainter(
                      snapshot: _snapshot,
                      cellWidth: _cellWidth,
                      cellHeight: _cellHeight,
                      glyphBaseline: _glyphBaseline,
                      fontSize: widget.fontSize,
                      fontFamily: _fontFamily,
                      glyphCache: _glyphCache,
                      selectionStart: selStart,
                      selectionEnd: selEnd,
                      focused: _focusNode.hasFocus,
                      cursorBlinkOn: _cursorBlinkOn,
                    ),
                  ),
                );
              },
            ),
          ),
          ?startHandle,
          ?endHandle,
        ],
      ),
    );
  }
}

class _TerminalGridPainter extends CustomPainter {
  _TerminalGridPainter({
    required this.snapshot,
    required this.cellWidth,
    required this.cellHeight,
    required this.glyphBaseline,
    required this.fontSize,
    required this.fontFamily,
    required this.glyphCache,
    this.selectionStart,
    this.selectionEnd,
    this.focused = true,
    this.cursorBlinkOn = true,
  });

  final TerminalScreenSnapshot? snapshot;
  final double cellWidth;
  final double cellHeight;
  final double glyphBaseline;
  final double fontSize;
  final String fontFamily;
  final Map<String, ui.Paragraph> glyphCache;
  final ({int row, int col})? selectionStart;
  final ({int row, int col})? selectionEnd;
  final bool focused;
  final bool cursorBlinkOn;

  @override
  void paint(Canvas canvas, Size size) {
    final snapshot = this.snapshot;
    if (snapshot == null || cellWidth <= 0 || cellHeight <= 0) return;

    canvas.save();
    canvas.clipRect(Offset.zero & size);
    // Smooth scrolling: shift the grid by the sub-row pixel offset, and lift
    // any pre-rendered overscan rows (host-served scroll) above the viewport.
    canvas.translate(
      0,
      snapshot.scrollPixelOffset - snapshot.marginRows * cellHeight,
    );

    final bgPaint = Paint();
    final rows = SplayTreeMap<int, List<TerminalScreenCell>>();
    for (final cell in snapshot.cells) {
      if (cell.row < 0) continue;
      rows.putIfAbsent(cell.row, () => <TerminalScreenCell>[]).add(cell);
      final colors = TerminalTheme.resolveCellColors(
        fg: cell.fg,
        bg: cell.bg,
        inverse: cell.inverse,
        bold: cell.bold,
        dim: cell.dim,
      );
      final span = cell.width < 1 ? 1 : cell.width;
      final x = cell.col * cellWidth;
      final y = cell.row * cellHeight;

      if (colors.drawBackground) {
        bgPaint.color = colors.bg;
        canvas.drawRect(
          Rect.fromLTWH(x, y, cellWidth * span, cellHeight),
          bgPaint,
        );
      }
    }

    for (final entry in rows.entries) {
      entry.value.sort((left, right) => left.col.compareTo(right.col));
      for (final run in _rowRuns(entry.value)) {
        final paragraph = _paragraph(
          run.text,
          run.color,
          bold: run.bold,
          italic: run.italic,
          underline: run.underline,
          strikeout: run.strikeout,
        );
        // Anchor each run to its grid column. A run never spans wider than its
        // cells (monospace advance == cellWidth), but a wide glyph (CJK) drawn
        // by a fallback font is often narrower than its 2-cell span; centering
        // it keeps the next column — and the cursor — at the right place instead
        // of leaving a gap that grows per character.
        final spanWidth = run.widthCols * cellWidth;
        final glyphWidth = paragraph.maxIntrinsicWidth;
        final dx = run.startCol * cellWidth +
            (glyphWidth < spanWidth ? (spanWidth - glyphWidth) / 2 : 0);
        canvas.drawParagraph(
          paragraph,
          Offset(
            dx,
            entry.key * cellHeight + glyphBaseline - _paragraphAscent(paragraph),
          ),
        );
      }
    }

    _paintSelection(canvas, snapshot);
    _paintCursor(canvas, snapshot);
    canvas.restore();
  }

  void _paintSelection(Canvas canvas, TerminalScreenSnapshot snapshot) {
    final start = selectionStart;
    final end = selectionEnd;
    if (start == null || end == null) return;
    // Translucent tint over selected cells; text stays readable underneath.
    final paint = Paint()..color = const Color(0x55409CFF);
    final lastRow = snapshot.rows - 1;
    for (var row = start.row; row <= end.row; row++) {
      if (row < 0 || row > lastRow) continue;
      final lo = row == start.row ? start.col : 0;
      final hi = row == end.row ? end.col : snapshot.cols - 1;
      if (hi < lo) continue;
      final x = lo * cellWidth;
      final width = (hi - lo + 1) * cellWidth;
      canvas.drawRect(
        Rect.fromLTWH(x, row * cellHeight, width, cellHeight),
        paint,
      );
    }
  }

  void _paintCursor(Canvas canvas, TerminalScreenSnapshot snapshot) {
    final cursor = snapshot.cursor;
    if (!cursor.visible) return;
    if (cursor.row < 0 || cursor.row >= snapshot.rows) return;
    final x = cursor.col * cellWidth;
    final y = cursor.row * cellHeight;
    final paint = Paint()..color = AppColors.terminalText;

    // The cell under the cursor may be double-width (e.g. CJK), so the cursor
    // must span the full glyph, not a single column.
    final cell = _cursorCell(snapshot, cursor.row, cursor.col);
    final span = (cell != null && cell.width > 1) ? cell.width : 1;
    final width = cellWidth * span;

    // Unfocused: always a hollow outline block, never blinking. Focused: the
    // host's shape, blinking (skip the off phase).
    if (!focused) {
      paint
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1;
      canvas.drawRect(
        Rect.fromLTWH(x + 0.5, y + 0.5, width - 1, cellHeight - 1),
        paint,
      );
      return;
    }
    if (!cursorBlinkOn) return;

    switch (cursor.shape) {
      case TerminalScreenCursorShape.beam:
        canvas.drawRect(Rect.fromLTWH(x, y, 2, cellHeight), paint);
      case TerminalScreenCursorShape.underline:
        canvas.drawRect(Rect.fromLTWH(x, y + cellHeight - 2, width, 2), paint);
      case TerminalScreenCursorShape.hollowBlock:
        paint
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1;
        canvas.drawRect(Rect.fromLTWH(x, y, width, cellHeight), paint);
      case TerminalScreenCursorShape.block:
        canvas.drawRect(Rect.fromLTWH(x, y, width, cellHeight), paint);
        if (cell != null && !cell.hidden && cell.text.trim().isNotEmpty) {
          final glyph = _paragraph(
            cell.text,
            AppColors.terminalBg,
            bold: cell.bold,
            italic: cell.italic,
            underline: false,
            strikeout: false,
          );
          canvas.drawParagraph(
            glyph,
            Offset(x, y + glyphBaseline - _paragraphAscent(glyph)),
          );
        }
    }
  }

  Iterable<_TerminalRowRun> _rowRuns(List<TerminalScreenCell> cells) sync* {
    _TerminalRowRun? current;
    var nextCol = 0;
    for (final cell in cells) {
      final span = cell.width < 1 ? 1 : cell.width;
      if (cell.hidden || cell.text.isEmpty) {
        if (current != null) {
          yield current;
          current = null;
        }
        nextCol = cell.col + span;
        continue;
      }
      final colors = TerminalTheme.resolveCellColors(
        fg: cell.fg,
        bg: cell.bg,
        inverse: cell.inverse,
        bold: cell.bold,
        dim: cell.dim,
      );
      // Wide cells (CJK, width >= 2) leave the batched run and render on their
      // own so each is positioned at its exact grid column — a fallback font's
      // narrower-than-2-cells glyph would otherwise drift the rest of the run.
      if (span != 1) {
        if (current != null) {
          yield current;
          current = null;
        }
        yield _TerminalRowRun.start(
          startCol: cell.col,
          text: cell.text,
          widthCols: span,
          color: colors.fg,
          bold: cell.bold,
          italic: cell.italic,
          underline: cell.underline,
          strikeout: cell.strikeout,
        );
        nextCol = cell.col + span;
        continue;
      }
      final gap = cell.col - nextCol;
      if (current == null ||
          !current.canAppend(cell: cell, color: colors.fg, gap: gap)) {
        if (current != null) yield current;
        current = _TerminalRowRun.start(
          startCol: cell.col,
          text: cell.text,
          widthCols: span,
          color: colors.fg,
          bold: cell.bold,
          italic: cell.italic,
          underline: cell.underline,
          strikeout: cell.strikeout,
        );
      } else {
        current.appendGap(gap);
        current.appendCell(cell.text, span);
      }
      nextCol = cell.col + span;
    }
    if (current != null) yield current;
  }

  /// The run paragraph's own ascent, so we can land its baseline on the shared
  /// `glyphBaseline`. Fallback fonts (CJK, symbols) have a larger ascent than
  /// the primary monospace; baseline-aligning them stops them sitting low.
  double _paragraphAscent(ui.Paragraph paragraph) {
    final metrics = paragraph.computeLineMetrics();
    return metrics.isNotEmpty ? metrics.first.ascent : fontSize * 0.8;
  }

  TerminalScreenCell? _cursorCell(
    TerminalScreenSnapshot snapshot,
    int row,
    int col,
  ) {
    for (final cell in snapshot.cells) {
      if (cell.row == row && cell.col == col) return cell;
    }
    return null;
  }

  ui.Paragraph _paragraph(
    String text,
    Color color, {
    required bool bold,
    required bool italic,
    required bool underline,
    required bool strikeout,
  }) {
    final key = '$text|${color.toARGB32()}|$bold|$italic|$underline|$strikeout';
    final cached = glyphCache[key];
    if (cached != null) return cached;

    final decorations = <TextDecoration>[
      if (underline) TextDecoration.underline,
      if (strikeout) TextDecoration.lineThrough,
    ];
    final builder =
        ui.ParagraphBuilder(
            ui.ParagraphStyle(fontFamily: fontFamily, fontSize: fontSize),
          )
          ..pushStyle(
            ui.TextStyle(
              color: color,
              fontWeight: bold ? FontWeight.w600 : FontWeight.normal,
              fontStyle: italic ? FontStyle.italic : FontStyle.normal,
              fontFamily: fontFamily,
              fontFamilyFallback: _terminalGlyphFallback,
              fontSize: fontSize,
              decoration: decorations.isEmpty
                  ? null
                  : TextDecoration.combine(decorations),
              decorationColor: color,
            ),
          )
          ..addText(text);
    final paragraph = builder.build()
      ..layout(const ui.ParagraphConstraints(width: double.infinity));

    if (glyphCache.length > 4096) glyphCache.clear();
    glyphCache[key] = paragraph;
    return paragraph;
  }

  @override
  bool shouldRepaint(covariant _TerminalGridPainter old) {
    return !identical(old.snapshot, snapshot) ||
        old.cellWidth != cellWidth ||
        old.cellHeight != cellHeight ||
        old.selectionStart != selectionStart ||
        old.selectionEnd != selectionEnd ||
        old.focused != focused ||
        old.cursorBlinkOn != cursorBlinkOn;
  }
}

class _TerminalRowRun {
  _TerminalRowRun.start({
    required this.startCol,
    required String text,
    required this.widthCols,
    required this.color,
    required this.bold,
    required this.italic,
    required this.underline,
    required this.strikeout,
  }) : _buffer = StringBuffer(text);

  final int startCol;
  int widthCols;
  final Color color;
  final bool bold;
  final bool italic;
  final bool underline;
  final bool strikeout;
  final StringBuffer _buffer;

  String get text => _buffer.toString();

  bool canAppend({
    required TerminalScreenCell cell,
    required Color color,
    required int gap,
  }) {
    return gap >= 0 &&
        this.color == color &&
        bold == cell.bold &&
        italic == cell.italic &&
        underline == cell.underline &&
        strikeout == cell.strikeout;
  }

  void appendGap(int gap) {
    if (gap <= 0) return;
    _buffer.write(' ' * gap);
    widthCols += gap;
  }

  void appendCell(String text, int span) {
    _buffer.write(text);
    widthCols += span;
  }
}

/// Cursor position and line height reported by the terminal renderer so the
/// page can lift the view above the keyboard to keep the cursor visible.
class TerminalCursorMetrics {
  const TerminalCursorMetrics({
    required this.row,
    required this.col,
    required this.lineHeight,
  });

  final int row;
  final int col;
  final double lineHeight;

  @override
  bool operator ==(Object other) {
    return other is TerminalCursorMetrics &&
        other.row == row &&
        other.col == col &&
        other.lineHeight == lineHeight;
  }

  @override
  int get hashCode => Object.hash(row, col, lineHeight);
}
