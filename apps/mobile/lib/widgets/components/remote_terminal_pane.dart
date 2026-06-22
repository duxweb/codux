import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../i18n.dart';
import '../../models/workspace_mode.dart';
import '../../services/remote_terminal_output_controller.dart';
import '../../services/terminal_repaint_signal.dart';
import '../../theme/app_theme.dart';
import 'connect_hint.dart';
import 'self_drawn_terminal_view.dart';
import 'toolbar.dart';

class RemoteTerminalPane extends StatefulWidget {
  const RemoteTerminalPane({
    super.key,
    required this.connected,
    required this.showTerminal,
    required this.hasDevice,
    required this.status,
    required this.workspaceMode,
    required this.projectListLoaded,
    required this.projectCount,
    required this.terminalUploadLoading,
    required this.terminalUploadStatus,
    required this.terminalBufferLoading,
    required this.sessionId,
    required this.pendingBufferSessionId,
    required this.connectionStatusText,
    required this.terminalHistoryLoadingText,
    required this.keyboardVisible,
    required this.keyboardRequested,
    required this.keyboardRequestSerial,
    required this.repaintSignal,
    required this.outputController,
    required this.terminalFontSize,
    required this.onConnect,
    required this.onInput,
    required this.onResize,
    required this.onSelectionChanged,
    required this.onSendKey,
    required this.onToggleKeyboard,
    required this.onRequestKeyboard,
    required this.onPaste,
    required this.onCopy,
    required this.onUpload,
    required this.onVoiceInput,
  });

  final bool connected;
  final bool showTerminal;
  final bool hasDevice;
  final String status;
  final WorkspaceMode workspaceMode;
  final bool projectListLoaded;
  final int projectCount;
  final bool terminalUploadLoading;
  final String terminalUploadStatus;
  final bool terminalBufferLoading;
  final String? sessionId;
  final String? pendingBufferSessionId;
  final String connectionStatusText;
  final String terminalHistoryLoadingText;
  final bool keyboardVisible;
  final bool keyboardRequested;
  final int keyboardRequestSerial;
  final TerminalRepaintSignal repaintSignal;
  final RemoteTerminalOutputController outputController;
  final double terminalFontSize;
  final VoidCallback onConnect;
  final ValueChanged<String> onInput;
  final void Function(int cols, int rows) onResize;
  final ValueChanged<String?> onSelectionChanged;
  final ValueChanged<String> onSendKey;
  final VoidCallback onToggleKeyboard;
  final VoidCallback onRequestKeyboard;
  final VoidCallback onPaste;
  final VoidCallback onCopy;
  final VoidCallback onUpload;
  final VoidCallback onVoiceInput;

  @override
  State<RemoteTerminalPane> createState() => _RemoteTerminalPaneState();
}

class _RemoteTerminalPaneState extends State<RemoteTerminalPane> {
  TerminalCursorMetrics? _cursorMetrics;

  @override
  void didUpdateWidget(covariant RemoteTerminalPane oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.sessionId != oldWidget.sessionId) {
      _cursorMetrics = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    final showTerminalToolbar =
        widget.workspaceMode == WorkspaceMode.terminal && widget.connected;
    final keyboardHeight = MediaQuery.viewInsetsOf(context).bottom;
    final bottomInset = MediaQuery.viewPaddingOf(context).bottom;
    final keyboardActiveThreshold = bottomInset + 8.0;
    final effectiveKeyboardHeight = keyboardHeight > keyboardActiveThreshold
        ? keyboardHeight
        : 0.0;
    final toolbarBottom = effectiveKeyboardHeight > 0
        ? effectiveKeyboardHeight
        : bottomInset;
    const toolbarBaseHeight = 76.0;
    final keyboardLift = effectiveKeyboardHeight > 0
        ? (effectiveKeyboardHeight - bottomInset).clamp(0.0, double.infinity)
        : 0.0;
    // Inset the terminal grid from the panel edges so the content isn't flush
    // against the surrounding container.
    const terminalPadding = EdgeInsets.all(12);

    return MediaQuery.removeViewInsets(
      context: context,
      removeBottom: true,
      child: ClipRect(
        child: LayoutBuilder(
          builder: (context, constraints) {
            final terminalToolbarHeight = toolbarBaseHeight + bottomInset;
            final viewportHeight = constraints.maxHeight.isFinite
                ? constraints.maxHeight
                : MediaQuery.sizeOf(context).height;
            final terminalHeight =
                (viewportHeight -
                        (showTerminalToolbar ? terminalToolbarHeight : 0.0))
                    .clamp(120.0, viewportHeight);
            final terminalLift = _terminalLiftForKeyboard(
              terminalHeight: terminalHeight,
              keyboardLift: keyboardLift,
              cursorMetrics: _cursorMetrics,
            );
            final showHostSyncOverlay =
                widget.connected &&
                !widget.projectListLoaded &&
                widget.projectCount == 0;
            final showUploadOverlay =
                widget.showTerminal &&
                widget.workspaceMode == WorkspaceMode.terminal &&
                widget.terminalUploadLoading &&
                widget.terminalUploadStatus.isNotEmpty;
            final showHistoryOverlay =
                widget.showTerminal &&
                widget.workspaceMode == WorkspaceMode.terminal &&
                !widget.terminalUploadLoading &&
                widget.terminalBufferLoading &&
                widget.sessionId != null &&
                widget.pendingBufferSessionId == widget.sessionId;

            return Stack(
              clipBehavior: Clip.none,
              children: [
                Positioned(
                  left: 0,
                  right: 0,
                  top: 0,
                  height: terminalHeight,
                  child: Transform.translate(
                    offset: Offset(0, -terminalLift),
                    child: ColoredBox(
                      key: const ValueKey('remote-terminal-body'),
                      color: AppColors.terminalBg,
                      child: Padding(
                        padding: terminalPadding,
                        child: Stack(
                          children: [
                            if (widget.showTerminal)
                              // Self-drawn renderer: reads the Rust cell grid
                              // directly (single source of truth). Repaints on
                              // the per-output signal so a live frame rebuilds
                              // only this subtree, not the whole page (toolbar,
                              // overlays, keyboard inset / layout recompute).
                              SelfDrawnTerminalView(
                                sessionId: widget.sessionId,
                                controller: widget.outputController,
                                repaintSignal: widget.repaintSignal,
                                fontSize: widget.terminalFontSize,
                                onResize: widget.onResize,
                                onInput: widget.onInput,
                                onSendKey: widget.onSendKey,
                                onSelectionChanged: widget.onSelectionChanged,
                                onRequestKeyboard: widget.onRequestKeyboard,
                                keyboardRequested: widget.keyboardRequested,
                                keyboardRequestSerial: widget.keyboardRequestSerial,
                                onCursorMetrics: (metrics) {
                                  if (_cursorMetrics == metrics) return;
                                  setState(() => _cursorMetrics = metrics);
                                },
                              )
                            else
                              ConnectHint(
                                status: widget.status.isEmpty
                                    ? AppPreferences.of(
                                        context,
                                      ).t('app.notConnected')
                                    : widget.status,
                                hasDevice: widget.hasDevice,
                                onConnect: widget.onConnect,
                              ),
                            if (widget.showTerminal &&
                                showHostSyncOverlay &&
                                !widget.terminalUploadLoading &&
                                !widget.terminalBufferLoading)
                              _TerminalOverlay(
                                message: widget.connectionStatusText,
                              ),
                            if (widget.showTerminal &&
                                (showUploadOverlay || showHistoryOverlay))
                              _TerminalOverlay(
                                message: showUploadOverlay
                                    ? widget.terminalUploadStatus
                                    : widget.terminalHistoryLoadingText,
                                opacity: 0.72,
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
                if (showTerminalToolbar)
                  Positioned(
                    left: 0,
                    right: 0,
                    bottom: toolbarBottom,
                    child: Toolbar(
                      onSendKey: widget.onSendKey,
                      applicationCursor: false,
                      keyboardVisible: widget.keyboardVisible,
                      bottomInset: 0,
                      onToggleKeyboard: widget.onToggleKeyboard,
                      uploading: widget.terminalUploadLoading,
                      onPaste: widget.onPaste,
                      onCopy: widget.onCopy,
                      onUpload: widget.onUpload,
                      onVoiceInput: widget.onVoiceInput,
                    ),
                  ),
              ],
            );
          },
        ),
      ),
    );
  }
}

double _terminalLiftForKeyboard({
  required double terminalHeight,
  required double keyboardLift,
  required TerminalCursorMetrics? cursorMetrics,
}) {
  if (keyboardLift <= 0) return 0;
  final safeBottom = terminalHeight - keyboardLift;
  if (safeBottom <= 0) return keyboardLift;
  final metrics = cursorMetrics;
  if (metrics == null) return keyboardLift;
  final cursorBottom = (metrics.row + 1) * math.max(1.0, metrics.lineHeight);
  final overflow = cursorBottom - safeBottom;
  if (overflow <= 0) return 0;
  return overflow.clamp(0.0, keyboardLift);
}

@visibleForTesting
double terminalLiftForKeyboardForTest({
  required double terminalHeight,
  required double keyboardLift,
  required TerminalCursorMetrics? cursorMetrics,
}) {
  return _terminalLiftForKeyboard(
    terminalHeight: terminalHeight,
    keyboardLift: keyboardLift,
    cursorMetrics: cursorMetrics,
  );
}

class _TerminalOverlay extends StatelessWidget {
  const _TerminalOverlay({required this.message, this.opacity = 0.58});

  final String message;
  final double opacity;

  @override
  Widget build(BuildContext context) {
    return Positioned.fill(
      child: IgnorePointer(
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: AppColors.terminalBg.withValues(alpha: opacity),
          ),
          child: Center(
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(
                    strokeWidth: 2,
                    color: Theme.of(context).colorScheme.secondary,
                  ),
                ),
                const SizedBox(width: AppSpacing.s),
                Text(
                  message,
                  style: const TextStyle(
                    color: AppColors.terminalTextDim,
                    fontSize: 13,
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
