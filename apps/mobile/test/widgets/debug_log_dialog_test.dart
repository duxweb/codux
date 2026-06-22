import 'package:codux_flutter/services/log_service.dart';
import 'package:codux_flutter/widgets/components/debug_log_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  setUp(CoduxLog.clear);

  testWidgets('shows empty log state', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: DebugLogDialog(
          title: 'Logs',
          emptyLabel: 'Empty',
          clearLabel: 'Clear',
          copyLabel: 'Copy',
          exportLabel: 'Export',
          closeLabel: 'Close',
          onCopy: (_) async {},
          onExport: (_) async {},
        ),
      ),
    );

    expect(find.text('Empty'), findsOneWidget);
    expect(
      tester
          .widget<TextButton>(find.widgetWithText(TextButton, 'Copy'))
          .enabled,
      isFalse,
    );
  });

  testWidgets('copies and clears log text', (tester) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.info('[test] hello');
    var copied = '';

    await tester.pumpWidget(
      MaterialApp(
        home: DebugLogDialog(
          title: 'Logs',
          emptyLabel: 'Empty',
          clearLabel: 'Clear',
          copyLabel: 'Copy',
          exportLabel: 'Export',
          closeLabel: 'Close',
          onCopy: (text) async => copied = text,
          onExport: (_) async {},
        ),
      ),
    );

    await tester.tap(find.text('Copy'));
    await tester.pump();
    expect(copied, contains('[test] hello'));

    await tester.tap(find.text('Clear'));
    await tester.pump();
    expect(find.text('Empty'), findsOneWidget);
  });
}
