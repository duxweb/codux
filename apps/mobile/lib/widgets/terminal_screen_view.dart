import 'dart:async';
import 'dart:math' as math;

import 'package:codux_protocol_ffi/codux_protocol_ffi.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme/app_theme.dart';
import '../theme/terminal_theme.dart';

class TerminalScreenView extends StatefulWidget {
  const TerminalScreenView({
    super.key,
    required this.snapshot,
    required this.keyboardRequested,
    required this.scrollEnabled,
    required this.onInput,
    required this.onResize,
    required this.onScrollPixels,
    required this.onSettleScroll,
    required this.onScrollToBottom,
    required this.onCursorBottom,
    this.remoteScroll = false,
  });

  final TerminalScreenSnapshot? snapshot;
  final bool keyboardRequested;
  final bool scrollEnabled;

  /// Whether scrollback is served by the host (with network latency).
  /// The scroll position is owned by Flutter, so delayed host
  /// confirmations only affect which snapshot rows are available to draw.
  final bool remoteScroll;
  final ValueChanged<String> onInput;
  final void Function(int cols, int rows) onResize;
  final void Function(double pixels, double cellHeight) onScrollPixels;
  final VoidCallback onSettleScroll;
  final VoidCallback onScrollToBottom;
  final ValueChanged<double> onCursorBottom;

  @override
  State<TerminalScreenView> createState() => _TerminalScreenViewState();
}

class _TerminalScreenViewState extends State<TerminalScreenView>
    implements TextInputClient {
  final ScrollController _scrollController = ScrollController();
  final FocusNode _keyboardFocusNode = FocusNode(
    debugLabel: 'terminal-screen-input',
  );
  TextInputConnection? _inputConnection;
  TextEditingValue _editingValue = _terminalInputSentinelValue;
  bool _followTail = true;
  bool _scrollIdle = true;
  bool _scrollFlushScheduled = false;
  bool _scrollToBottomScheduled = false;
  bool _suppressScrollEmit = false;
  double _pendingScrollPixels = 0;
  double? _lastScrollOffset;
  bool _cursorBlinkVisible = true;
  Timer? _cursorBlinkTimer;

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(_handleScrollOffsetChanged);
    _startCursorBlink();
    _syncKeyboardFocus();
  }

  @override
  void didUpdateWidget(covariant TerminalScreenView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.keyboardRequested != oldWidget.keyboardRequested) {
      _syncKeyboardFocus();
    }
    if (widget.snapshot?.data != oldWidget.snapshot?.data &&
        _followTail &&
        widget.snapshot?.displayOffset != 0) {
      _scheduleScrollToBottom();
    }
    if (_cursorSignature(widget.snapshot) !=
        _cursorSignature(oldWidget.snapshot)) {
      _resetCursorBlink();
    }
  }

  @override
  void dispose() {
    _cursorBlinkTimer?.cancel();
    _closeKeyboardConnection();
    _keyboardFocusNode.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _startCursorBlink() {
    _cursorBlinkTimer = Timer.periodic(_terminalCursorBlinkInterval, (_) {
      if (!mounted) return;
      final cursor = widget.snapshot?.cursor;
      if (cursor == null || !cursor.visible) {
        if (!_cursorBlinkVisible) {
          setState(() => _cursorBlinkVisible = true);
        }
        return;
      }
      setState(() => _cursorBlinkVisible = !_cursorBlinkVisible);
    });
  }

  void _resetCursorBlink() {
    if (_cursorBlinkVisible) return;
    setState(() => _cursorBlinkVisible = true);
  }

  void _syncKeyboardFocus() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (widget.keyboardRequested) {
        _keyboardFocusNode.requestFocus();
        _openKeyboardConnection();
      } else {
        _keyboardFocusNode.unfocus();
        _closeKeyboardConnection();
      }
    });
  }

  void _scheduleScrollToBottom() {
    if (_scrollToBottomScheduled) return;
    _scrollToBottomScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scrollToBottomScheduled = false;
      if (!mounted || !_followTail || widget.snapshot?.displayOffset == 0) {
        return;
      }
      _jumpToBottom();
      widget.onScrollToBottom();
    });
  }

  void _openKeyboardConnection() {
    final connection = _inputConnection;
    if (connection != null && connection.attached) {
      _syncKeyboardGeometry(connection);
      connection.show();
      return;
    }
    final nextConnection = TextInput.attach(this, _terminalInputConfig);
    _inputConnection = nextConnection;
    _editingValue = _terminalInputSentinelValue;
    nextConnection.setEditingState(_editingValue);
    _syncKeyboardGeometry(nextConnection);
    nextConnection.show();
  }

  void _closeKeyboardConnection() {
    final connection = _inputConnection;
    _inputConnection = null;
    _editingValue = _terminalInputSentinelValue;
    if (connection != null && connection.attached) {
      connection.close();
    }
  }

  void _syncKeyboardGeometry(TextInputConnection connection) {
    final renderObject = context.findRenderObject();
    if (renderObject is! RenderBox || !renderObject.hasSize) return;
    final transform = renderObject.getTransformTo(null);
    connection.setEditableSizeAndTransform(renderObject.size, transform);
    connection.setCaretRect(_caretRect(renderObject.size));
    connection.setComposingRect(_caretRect(renderObject.size));
  }

  Rect _caretRect(Size size) {
    final snapshot = widget.snapshot;
    if (snapshot == null) {
      return Rect.fromLTWH(0, 0, 1, _terminalCellHeight);
    }
    final fontSize = _terminalFontSize;
    final cellWidth = _terminalCellWidth(context, fontSize);
    final left = (snapshot.cursor.col * cellWidth).clamp(0.0, size.width);
    final top = (snapshot.cursor.row * _terminalCellHeight).clamp(
      0.0,
      size.height,
    );
    return Rect.fromLTWH(left, top, 1, _terminalCellHeight);
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        const fontSize = _terminalFontSize;
        final cellWidth = _terminalCellWidth(context, fontSize);
        const cellHeight = _terminalCellHeight;
        final cols = math.max(20, constraints.maxWidth ~/ cellWidth);
        final rows = math.max(8, constraints.maxHeight ~/ cellHeight);
        final snapshot = widget.snapshot;
        // The virtual content covers the full scrollback; the scroll offset
        // is measured from the top of history, bottom = maxScrollExtent.
        final contentHeight = (snapshot?.totalLines ?? 0) * cellHeight;
        final scrollOffset = _scrollController.hasClients
            ? _scrollController.position.pixels
            : math.max(0.0, contentHeight - constraints.maxHeight);
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (!mounted) return;
          widget.onResize(cols, rows);
          final connection = _inputConnection;
          if (connection != null && connection.attached) {
            _syncKeyboardGeometry(connection);
          }
          _maintainScrollAnchor();
          final screen = widget.snapshot;
          if (screen != null) {
            final offsetNow = _scrollController.hasClients
                ? _scrollController.position.pixels
                : scrollOffset;
            final cursorBottom =
                (screen.cursor.row + 1) * cellHeight +
                _painterScrollOffsetY(
                  screen,
                  offsetNow,
                  constraints.maxHeight,
                  cellHeight,
                );
            widget.onCursorBottom(cursorBottom);
          }
        });

        return KeyboardListener(
          focusNode: _keyboardFocusNode,
          autofocus: widget.keyboardRequested,
          onKeyEvent: _handleKeyEvent,
          child: ClipRect(
            child: Stack(
              children: [
                Positioned.fill(
                  child: CustomPaint(
                    size: Size.infinite,
                    painter: _TerminalScreenPainter(
                      snapshot: snapshot,
                      cellWidth: cellWidth,
                      cellHeight: cellHeight,
                      fontSize: fontSize,
                      scrollOffsetY: snapshot == null
                          ? 0
                          : _painterScrollOffsetY(
                              snapshot,
                              scrollOffset,
                              constraints.maxHeight,
                              cellHeight,
                            ),
                      cursorBlinkVisible: _cursorBlinkVisible,
                    ),
                  ),
                ),
                // Transparent scroll surface: Flutter physics owns the
                // position; the painter above translates the snapshot to it.
                Positioned.fill(
                  child: NotificationListener<ScrollNotification>(
                    onNotification: _handleScrollNotification,
                    child: SingleChildScrollView(
                      controller: _scrollController,
                      physics: widget.scrollEnabled
                          ? const ClampingScrollPhysics()
                          : const NeverScrollableScrollPhysics(),
                      child: SizedBox(
                        height: contentHeight,
                        width: constraints.maxWidth,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  void _handleScrollOffsetChanged() {
    if (!_scrollController.hasClients) return;
    final position = _scrollController.position;
    final previous = _lastScrollOffset;
    _lastScrollOffset = position.pixels;
    // Follow the tail while the position stays within half a row of the
    // bottom; scrolling away releases the pin.
    _followTail =
        position.maxScrollExtent - position.pixels <= _terminalCellHeight / 2;
    setState(() {});
    if (_suppressScrollEmit || previous == null) return;
    // Offset grows downward from the top of history; the contract wants
    // positive pixels for scrolling up into history.
    final delta = previous - position.pixels;
    if (delta == 0) return;
    _pendingScrollPixels += delta;
    _scheduleScrollFlush();
  }

  bool _handleScrollNotification(ScrollNotification notification) {
    if (notification is ScrollStartNotification) {
      _scrollIdle = false;
    } else if (notification is ScrollEndNotification) {
      _scrollIdle = true;
      if (!_suppressScrollEmit) {
        _flushScrollPixels();
        widget.onSettleScroll();
      }
    }
    return false;
  }

  void _scheduleScrollFlush() {
    if (_scrollFlushScheduled) return;
    _scrollFlushScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scrollFlushScheduled = false;
      if (!mounted) return;
      _flushScrollPixels();
    });
  }

  void _flushScrollPixels() {
    final pixels = _pendingScrollPixels;
    if (pixels == 0) return;
    _pendingScrollPixels = 0;
    widget.onScrollPixels(pixels, _terminalCellHeight);
  }

  void _maintainScrollAnchor() {
    if (!_scrollController.hasClients) return;
    final position = _scrollController.position;
    // Pin to the (possibly grown) bottom while following the tail; never
    // fight an in-flight user drag or fling.
    if (_followTail &&
        _scrollIdle &&
        position.maxScrollExtent - position.pixels > _terminalScrollEpsilon) {
      _suppressedJumpTo(position.maxScrollExtent);
    }
    // Content-extent shrink corrections move pixels without notifying;
    // realign so the next user delta is measured from the real offset.
    _lastScrollOffset = position.pixels;
  }

  void _jumpToBottom() {
    if (!_scrollController.hasClients) return;
    _suppressedJumpTo(_scrollController.position.maxScrollExtent);
  }

  void _suppressedJumpTo(double target) {
    _suppressScrollEmit = true;
    try {
      _scrollController.jumpTo(target);
    } finally {
      _suppressScrollEmit = false;
    }
  }

  // The snapshot grid is drawn at absolute content coordinates: the
  // viewport portion sits with its bottom at line totalLines -
  // displayOffset, marginRows of above-context render above it and
  // marginRowsBelow of below-context render below it, all translated by
  // the Flutter scroll offset. The sub-line scrollPixelOffset is already
  // folded into the offset the host was asked to show, so it does not
  // reappear here.
  double _painterScrollOffsetY(
    TerminalScreenSnapshot screen,
    double scrollOffset,
    double viewportHeight,
    double cellHeight,
  ) {
    final viewportRows =
        screen.rows - screen.marginRows - screen.marginRowsBelow;
    final absoluteTopY =
        (screen.totalLines -
            screen.displayOffset -
            viewportRows -
            screen.marginRows) *
        cellHeight;
    return absoluteTopY -
        scrollOffset +
        _bottomAnchorOffset(screen, viewportHeight, cellHeight);
  }

  // When the host grid is at least one full row shorter than this screen
  // (the desktop owns the viewport from a smaller window) and all content
  // fits in the viewport, anchor content to the bottom so the TUI composer
  // sits by the keyboard. Taller content is already bottom-aligned at
  // maxScrollExtent by the absolute coordinate math.
  double _bottomAnchorOffset(
    TerminalScreenSnapshot screen,
    double viewportHeight,
    double cellHeight,
  ) {
    if (screen.marginRows > 0 ||
        screen.marginRowsBelow > 0 ||
        screen.displayOffset > 0) {
      return 0;
    }
    final contentRows =
        screen.rows - screen.marginRows - screen.marginRowsBelow;
    final deficit = viewportHeight - contentRows * cellHeight;
    if (deficit < cellHeight) return 0;
    return math.max(0.0, viewportHeight - screen.totalLines * cellHeight);
  }

  @override
  TextEditingValue? get currentTextEditingValue => _editingValue;

  @override
  AutofillScope? get currentAutofillScope => null;

  void _handleKeyEvent(KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) return;
    final key = switch (event.logicalKey) {
      LogicalKeyboardKey.backspace => 'backspace',
      LogicalKeyboardKey.delete => 'delete',
      LogicalKeyboardKey.enter => 'enter',
      LogicalKeyboardKey.arrowLeft => 'left',
      LogicalKeyboardKey.arrowRight => 'right',
      LogicalKeyboardKey.arrowUp => 'up',
      LogicalKeyboardKey.arrowDown => 'down',
      _ => null,
    };
    if (key == null) return;
    final input = terminalKeyInput(
      key: key,
      applicationCursor: widget.snapshot?.applicationCursor ?? false,
    );
    if (input.isNotEmpty) {
      _resetCursorBlink();
      widget.onInput(input);
    }
  }

  @override
  void updateEditingValue(TextEditingValue value) {
    if (value.composing.isValid && !value.composing.isCollapsed) {
      _editingValue = value;
      return;
    }
    final terminalInput = _terminalInputFromEditingValue(value);
    final normalizedInput = terminalTextInput(terminalInput);
    if (normalizedInput.isNotEmpty) {
      _resetCursorBlink();
      widget.onInput(normalizedInput);
    }
    _resetImeEditingState();
  }

  void _resetImeEditingState() {
    _editingValue = _terminalInputSentinelValue;
    final connection = _inputConnection;
    if (connection != null && connection.attached) {
      connection.setEditingState(_editingValue);
    }
  }

  @override
  void performAction(TextInputAction action) {
    switch (action) {
      case TextInputAction.newline:
      case TextInputAction.done:
      case TextInputAction.go:
      case TextInputAction.send:
      case TextInputAction.unspecified:
      case TextInputAction.none:
        _resetCursorBlink();
        widget.onInput(terminalKeyInput(key: 'enter'));
      case TextInputAction.next:
      case TextInputAction.previous:
      case TextInputAction.search:
      case TextInputAction.join:
      case TextInputAction.route:
      case TextInputAction.emergencyCall:
      case TextInputAction.continueAction:
        break;
    }
  }

  @override
  void connectionClosed() {
    _inputConnection = null;
    _editingValue = _terminalInputSentinelValue;
  }

  @override
  void didChangeInputControl(
    TextInputControl? oldControl,
    TextInputControl? newControl,
  ) {}

  @override
  void insertContent(KeyboardInsertedContent content) {}

  @override
  void insertTextPlaceholder(Size size) {}

  @override
  void performPrivateCommand(String action, Map<String, dynamic> data) {}

  @override
  void performSelector(String selectorName) {
    final input = terminalSelectorInput(
      selector: selectorName,
      applicationCursor: widget.snapshot?.applicationCursor ?? false,
    );
    if (input.isNotEmpty) {
      _resetCursorBlink();
      widget.onInput(input);
    }
  }

  @override
  void removeTextPlaceholder() {}

  @override
  void showToolbar() {}

  @override
  void updateFloatingCursor(RawFloatingCursorPoint point) {}

  @override
  void showAutocorrectionPromptRect(int start, int end) {}
}

const _terminalFontSize = 11.5;
const _terminalLineHeight = 1.25;
const _terminalCellHeight = _terminalFontSize * _terminalLineHeight;
const _terminalLetterSpacing = 0.0;
const _terminalFontFamily = 'Maple Mono NF CN';
const _terminalScrollEpsilon = 0.01;
const _terminalCursorBlinkInterval = Duration(milliseconds: 530);
const _terminalInputSentinel = '  ';
const _terminalBackspaceInput = '\u0008';
const _terminalInputSentinelValue = TextEditingValue(
  text: _terminalInputSentinel,
  selection: TextSelection.collapsed(offset: _terminalInputSentinel.length),
);
const _terminalInputConfig = TextInputConfiguration(
  inputType: TextInputType.emailAddress,
  inputAction: TextInputAction.newline,
  autocorrect: false,
  enableSuggestions: false,
  enableIMEPersonalizedLearning: false,
  enableInteractiveSelection: false,
  enableDeltaModel: false,
  keyboardAppearance: Brightness.dark,
  autofillConfiguration: AutofillConfiguration.disabled,
);

String _terminalInputFromEditingValue(TextEditingValue next) {
  final text = next.text;
  if (text.length < _terminalInputSentinel.length) {
    return _terminalBackspaceInput;
  }
  if (text == _terminalInputSentinel) return '';
  if (text.startsWith(_terminalInputSentinel)) {
    return text.substring(_terminalInputSentinel.length);
  }
  if (text.length > _terminalInputSentinel.length) {
    return text.substring(_terminalInputSentinel.length);
  }
  return '';
}

String _cursorSignature(TerminalScreenSnapshot? snapshot) {
  final cursor = snapshot?.cursor;
  if (cursor == null) return '';
  return '${cursor.row}:${cursor.col}:${cursor.visible}:${cursor.shape}';
}

double _terminalCellWidth(BuildContext context, double fontSize) {
  final painter = TextPainter(
    text: TextSpan(
      text: 'm',
      style: TextStyle(
        fontFamily: _terminalFontFamily,
        fontSize: fontSize,
        height: 1,
        letterSpacing: _terminalLetterSpacing,
      ),
    ),
    textDirection: TextDirection.ltr,
  )..layout();
  return painter.width.clamp(6.0, 16.0);
}

class _TerminalScreenPainter extends CustomPainter {
  _TerminalScreenPainter({
    required this.snapshot,
    required this.cellWidth,
    required this.cellHeight,
    required this.fontSize,
    required this.scrollOffsetY,
    required this.cursorBlinkVisible,
  });

  final TerminalScreenSnapshot? snapshot;
  final double cellWidth;
  final double cellHeight;
  final double fontSize;
  final double scrollOffsetY;
  final bool cursorBlinkVisible;

  @override
  void paint(Canvas canvas, Size size) {
    final screen = snapshot;
    canvas.drawRect(Offset.zero & size, Paint()..color = AppColors.bgBase);
    if (screen == null) return;

    final textPainter = TextPainter(textDirection: TextDirection.ltr);
    final cursorCell = _cursorCell(screen);

    for (final cell in screen.cells) {
      if (cell.hidden) continue;
      final left = cell.col * cellWidth;
      final top = cell.row * cellHeight + scrollOffsetY;
      if (left >= size.width || top >= size.height || top + cellHeight <= 0) {
        continue;
      }
      final colors = TerminalTheme.resolveCellColors(
        fg: cell.fg,
        bg: cell.bg,
        inverse: cell.inverse,
      );
      if (colors.drawBackground) {
        canvas.drawRect(
          Rect.fromLTWH(left, top, cellWidth * cell.width, cellHeight),
          Paint()..color = colors.bg,
        );
      }
      // Background-only cells (TUI panel bands erased with a background
      // color) carry no glyph; they still need the rect above.
      if (cell.text.isEmpty) continue;
      textPainter.text = TextSpan(
        text: cell.text,
        style: TextStyle(
          color: colors.fg,
          fontFamily: _terminalFontFamily,
          fontSize: fontSize,
          height: 1,
          letterSpacing: _terminalLetterSpacing,
          fontWeight: cell.bold ? FontWeight.w700 : FontWeight.w400,
          fontStyle: cell.italic ? FontStyle.italic : FontStyle.normal,
          decoration: TextDecoration.combine([
            if (cell.underline) TextDecoration.underline,
            if (cell.strikeout) TextDecoration.lineThrough,
          ]),
        ),
      );
      textPainter.layout(maxWidth: cellWidth * cell.width);
      textPainter.paint(
        canvas,
        Offset(left, top + (cellHeight - fontSize) / 2),
      );
    }

    if (screen.cursor.visible && cursorBlinkVisible) {
      final cursorTop = (screen.cursor.row * cellHeight + scrollOffsetY)
          .floorToDouble();
      final cursorIsBlock =
          screen.cursor.shape == TerminalScreenCursorShape.block;
      final cursorCellCol = cursorIsBlock && cursorCell != null
          ? cursorCell.col
          : screen.cursor.col;
      final cursorCellWidth = cursorIsBlock && cursorCell != null
          ? cursorCell.width
          : 1;
      final cursorLeft = (cursorCellCol * cellWidth).floorToDouble();
      final cursorRect = Rect.fromLTWH(
        cursorLeft,
        cursorTop,
        (cellWidth * cursorCellWidth).roundToDouble().clamp(
          1.0,
          double.infinity,
        ),
        cellHeight.roundToDouble().clamp(1.0, double.infinity),
      );
      if (cursorRect.right <= 0 ||
          cursorRect.left >= size.width ||
          cursorRect.bottom <= 0 ||
          cursorRect.top >= size.height) {
        return;
      }
      _paintCursor(canvas, cursorRect, screen.cursor.shape);
      if (cursorIsBlock && cursorCell != null && cursorCell.text.isNotEmpty) {
        _paintCellText(
          textPainter: textPainter,
          canvas: canvas,
          cell: cursorCell,
          left: cursorLeft,
          top: cursorTop,
          color: AppColors.bgBase,
        );
      }
    }
  }

  void _paintCursor(
    Canvas canvas,
    Rect bounds,
    TerminalScreenCursorShape shape,
  ) {
    final paint = Paint()..color = AppColors.accent.withValues(alpha: 0.56);
    switch (shape) {
      case TerminalScreenCursorShape.beam:
        canvas.drawRect(
          Rect.fromLTWH(bounds.left, bounds.top, 2, bounds.height),
          paint,
        );
      case TerminalScreenCursorShape.underline:
        canvas.drawRect(
          Rect.fromLTWH(bounds.left, bounds.bottom - 2, bounds.width, 2),
          paint,
        );
      case TerminalScreenCursorShape.hollowBlock:
        canvas.drawRect(
          bounds.deflate(0.5),
          Paint()
            ..color = AppColors.accent.withValues(alpha: 0.72)
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1,
        );
      case TerminalScreenCursorShape.block:
        canvas.drawRect(
          bounds,
          Paint()..color = AppColors.accent.withValues(alpha: 0.88),
        );
    }
  }

  TerminalScreenCell? _cursorCell(TerminalScreenSnapshot screen) {
    for (final cell in screen.cells) {
      if (cell.hidden || cell.text.isEmpty) continue;
      if (cell.row != screen.cursor.row) continue;
      if (screen.cursor.col >= cell.col &&
          screen.cursor.col < cell.col + cell.width) {
        return cell;
      }
    }
    return null;
  }

  void _paintCellText({
    required TextPainter textPainter,
    required Canvas canvas,
    required TerminalScreenCell cell,
    required double left,
    required double top,
    required Color color,
  }) {
    textPainter.text = TextSpan(
      text: cell.text,
      style: TextStyle(
        color: color,
        fontFamily: _terminalFontFamily,
        fontSize: fontSize,
        height: 1,
        letterSpacing: _terminalLetterSpacing,
        fontWeight: cell.bold ? FontWeight.w700 : FontWeight.w400,
        fontStyle: cell.italic ? FontStyle.italic : FontStyle.normal,
        decoration: TextDecoration.combine([
          if (cell.underline) TextDecoration.underline,
          if (cell.strikeout) TextDecoration.lineThrough,
        ]),
      ),
    );
    textPainter.layout(maxWidth: cellWidth * cell.width);
    textPainter.paint(canvas, Offset(left, top + (cellHeight - fontSize) / 2));
  }

  @override
  bool shouldRepaint(covariant _TerminalScreenPainter oldDelegate) {
    return snapshot != oldDelegate.snapshot ||
        cellWidth != oldDelegate.cellWidth ||
        cellHeight != oldDelegate.cellHeight ||
        fontSize != oldDelegate.fontSize ||
        scrollOffsetY != oldDelegate.scrollOffsetY ||
        cursorBlinkVisible != oldDelegate.cursorBlinkVisible;
  }
}
