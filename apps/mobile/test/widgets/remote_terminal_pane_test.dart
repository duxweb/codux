import 'package:codux_flutter/i18n.dart';
import 'package:codux_flutter/services/native_terminal_replay_controller.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/native_terminal_view.dart';
import 'package:codux_flutter/widgets/remote_terminal_pane.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('terminal content starts at top of terminal body', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildAppTheme(),
        home: AppPreferences(
          accent: AccentChoices.cyan,
          locale: LocaleChoices.english,
          child: SizedBox(width: 360, height: 720, child: _pane()),
        ),
      ),
    );
    await tester.pump();

    final paneTop = tester.getTopLeft(find.byType(RemoteTerminalPane)).dy;
    final terminalTop = tester
        .getTopLeft(find.byKey(const ValueKey('remote-terminal-body')))
        .dy;

    expect(terminalTop, paneTop);
  });

  testWidgets('ctrl c toolbar sends etx directly', (tester) async {
    final sent = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        theme: buildAppTheme(),
        home: AppPreferences(
          accent: AccentChoices.cyan,
          locale: LocaleChoices.english,
          child: SizedBox(
            width: 360,
            height: 720,
            child: _pane(onSendKey: sent.add),
          ),
        ),
      ),
    );
    await tester.pump();

    await tester.tap(find.text('^C'));
    await tester.pump();

    expect(sent, ['\u0003']);
  });

  test('keyboard lift follows cursor visibility', () {
    expect(
      terminalLiftForKeyboardForTest(
        terminalHeight: 600,
        keyboardLift: 260,
        cursorMetrics: const NativeTerminalCursorMetrics(
          row: 4,
          col: 0,
          lineHeight: 20,
        ),
      ),
      0,
    );
    expect(
      terminalLiftForKeyboardForTest(
        terminalHeight: 600,
        keyboardLift: 260,
        cursorMetrics: const NativeTerminalCursorMetrics(
          row: 20,
          col: 0,
          lineHeight: 20,
        ),
      ),
      80,
    );
  });
}

RemoteTerminalPane _pane({ValueChanged<String>? onSendKey}) {
  return RemoteTerminalPane(
    connected: true,
    showTerminal: true,
    hasDevice: true,
    status: '',
    workspaceMode: 'terminal',
    projectListLoaded: true,
    projectCount: 1,
    terminalUploadLoading: false,
    terminalUploadStatus: '',
    terminalBufferLoading: false,
    sessionId: 'session-1',
    pendingBufferSessionId: null,
    connectionStatusText: 'connecting',
    terminalHistoryLoadingText: 'loading',
    keyboardVisible: false,
    keyboardRequested: false,
    keyboardRequestSerial: 0,
    replayController: NativeTerminalReplayController(),
    terminalFontSize: 16,
    onConnect: () {},
    onInput: (_) {},
    onResize: (_, _) {},
    onSelectionChanged: (_) {},
    onSendKey: onSendKey ?? (_) {},
    onToggleKeyboard: () {},
    onPaste: () {},
    onCopy: () {},
    onUpload: () {},
    onVoiceInput: () {},
  );
}
