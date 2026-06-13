import 'dart:io';

import 'package:flutter/material.dart';

import '../i18n.dart';
import '../services/remote_pty_session.dart';
import '../theme/app_theme.dart';
import 'connect_hint.dart';
import 'terminal_screen_view.dart';
import 'terminal_transition_mask.dart';
import 'toolbar.dart';

class RemoteTerminalPane extends StatelessWidget {
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
    required this.maskOpacity,
    required this.keyboardRequested,
    required this.keyboardVisible,
    required this.terminalCursorBottom,
    required this.terminalScreen,
    required this.terminalFontSize,
    this.remoteScroll = false,
    required this.onConnect,
    required this.onInput,
    required this.onResize,
    required this.onScrollPixels,
    required this.onSettleScroll,
    required this.onScrollToBottom,
    required this.onMetricsCursorBottom,
    required this.onSendKey,
    required this.onToggleKeyboard,
    required this.onPaste,
    required this.onCopy,
    required this.onUpload,
    required this.onVoiceInput,
  });

  final bool connected;
  final bool showTerminal;
  final bool hasDevice;
  final String status;
  final String workspaceMode;
  final bool projectListLoaded;
  final int projectCount;
  final bool terminalUploadLoading;
  final String terminalUploadStatus;
  final bool terminalBufferLoading;
  final String? sessionId;
  final String? pendingBufferSessionId;
  final String connectionStatusText;
  final String terminalHistoryLoadingText;
  final Animation<double> maskOpacity;
  final bool keyboardRequested;
  final bool keyboardVisible;
  final double terminalCursorBottom;
  final RemoteTerminalScreenSnapshot? terminalScreen;
  final double terminalFontSize;
  final bool remoteScroll;
  final VoidCallback onConnect;
  final ValueChanged<String> onInput;
  final void Function(int cols, int rows) onResize;
  final void Function(double pixels, double cellHeight) onScrollPixels;
  final VoidCallback onSettleScroll;
  final VoidCallback onScrollToBottom;
  final ValueChanged<double> onMetricsCursorBottom;
  final ValueChanged<String> onSendKey;
  final VoidCallback onToggleKeyboard;
  final VoidCallback onPaste;
  final VoidCallback onCopy;
  final VoidCallback onUpload;
  final VoidCallback onVoiceInput;

  @override
  Widget build(BuildContext context) {
    final showTerminalToolbar = workspaceMode == 'terminal' && connected;
    final keyboardHeight = MediaQuery.viewInsetsOf(context).bottom;
    final bottomInset = MediaQuery.viewPaddingOf(context).bottom;
    final keyboardActiveThreshold = bottomInset + 8.0;
    final effectiveKeyboardHeight = keyboardHeight > keyboardActiveThreshold
        ? keyboardHeight
        : 0.0;
    final toolbarBottom = effectiveKeyboardHeight > 0
        ? effectiveKeyboardHeight
        : bottomInset;
    final keyboardOverlap = (toolbarBottom - bottomInset).clamp(
      0.0,
      double.infinity,
    );
    final toolbarSafeInset = toolbarBottom.clamp(0.0, bottomInset);
    final terminalPadding = Platform.isIOS
        ? EdgeInsets.zero
        : const EdgeInsets.symmetric(horizontal: 8);

    return MediaQuery.removeViewInsets(
      context: context,
      removeBottom: true,
      child: ClipRect(
        child: LayoutBuilder(
          builder: (context, constraints) {
            const toolbarBaseHeight = 76.0;
            final terminalToolbarHeight = toolbarBaseHeight + toolbarSafeInset;
            final viewportHeight = constraints.maxHeight.isFinite
                ? constraints.maxHeight
                : MediaQuery.sizeOf(context).height;
            final terminalHeight =
                (viewportHeight -
                        (showTerminalToolbar ? terminalToolbarHeight : 0.0))
                    .clamp(120.0, viewportHeight);
            final terminalViewHeight =
                (terminalHeight - terminalPadding.vertical).clamp(
                  0.0,
                  terminalHeight,
                );
            final visibleTerminalBottom = (terminalViewHeight - keyboardOverlap)
                .clamp(0.0, terminalViewHeight);
            final terminalShift =
                showTerminalToolbar &&
                    effectiveKeyboardHeight > 0 &&
                    terminalCursorBottom > visibleTerminalBottom
                ? (terminalCursorBottom - visibleTerminalBottom).clamp(
                    0.0,
                    keyboardOverlap,
                  )
                : 0.0;
            final showHostSyncOverlay =
                connected && !projectListLoaded && projectCount == 0;
            final showUploadOverlay =
                showTerminal &&
                workspaceMode == 'terminal' &&
                terminalUploadLoading &&
                terminalUploadStatus.isNotEmpty;
            final showHistoryOverlay =
                showTerminal &&
                workspaceMode == 'terminal' &&
                !terminalUploadLoading &&
                terminalBufferLoading &&
                sessionId != null &&
                pendingBufferSessionId == sessionId;

            return Stack(
              clipBehavior: Clip.none,
              children: [
                Positioned(
                  left: 0,
                  right: 0,
                  top: 0,
                  height: terminalHeight,
                  child: Transform.translate(
                    offset: Offset(0, -terminalShift),
                    child: ColoredBox(
                      color: AppColors.bgBase,
                      child: Padding(
                        padding: terminalPadding,
                        child: Stack(
                          children: [
                            if (showTerminal)
                              TerminalScreenView(
                                // Session-scoped key: a project switch gives
                                // the new session a fresh view state (scroll
                                // position, follow-tail, blink, resize-emit
                                // dedupe) instead of inheriting the previous
                                // session's in-flight scroll/anchor.
                                key: ValueKey('terminal-${sessionId ?? ''}'),
                                snapshot: terminalScreen,
                                keyboardRequested: keyboardRequested,
                                scrollEnabled: !keyboardVisible,
                                remoteScroll: remoteScroll,
                                fontSize: terminalFontSize,
                                onInput: onInput,
                                onResize: onResize,
                                onScrollPixels: onScrollPixels,
                                onSettleScroll: onSettleScroll,
                                onScrollToBottom: onScrollToBottom,
                                onCursorBottom: onMetricsCursorBottom,
                              )
                            else
                              ConnectHint(
                                status: status.isEmpty
                                    ? AppPreferences.of(
                                        context,
                                      ).t('app.notConnected')
                                    : status,
                                hasDevice: hasDevice,
                                onConnect: onConnect,
                              ),
                            if (showTerminal &&
                                showHostSyncOverlay &&
                                !terminalUploadLoading &&
                                !terminalBufferLoading)
                              _TerminalOverlay(message: connectionStatusText),
                            if (showTerminal &&
                                (showUploadOverlay || showHistoryOverlay))
                              _TerminalOverlay(
                                message: showUploadOverlay
                                    ? terminalUploadStatus
                                    : terminalHistoryLoadingText,
                                opacity: 0.72,
                              ),
                            FadeTransition(
                              opacity: maskOpacity,
                              child: const TerminalTransitionMask(),
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
                      onSendKey: onSendKey,
                      applicationCursor:
                          terminalScreen?.applicationCursor ?? false,
                      keyboardVisible: keyboardVisible,
                      bottomInset: 0,
                      onToggleKeyboard: onToggleKeyboard,
                      uploading: terminalUploadLoading,
                      onPaste: onPaste,
                      onCopy: onCopy,
                      onUpload: onUpload,
                      onVoiceInput: onVoiceInput,
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
            color: AppColors.bgBase.withValues(alpha: opacity),
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
                    color: AppColors.textSecondary,
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
