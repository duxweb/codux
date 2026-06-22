import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/remote_terminal_output_controller.dart';
import 'package:codux_flutter/widgets/components/self_drawn_terminal_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('renders the Rust cell snapshot and reports a sane grid size', (
    tester,
  ) async {
    final controller = RemoteTerminalOutputController();
    addTearDown(controller.dispose);
    controller.bindSession('session-1', requireBaseline: true);
    controller.accept(
      const RelayEnvelope(
        type: 'terminal.output',
        sessionId: 'session-1',
        payload: {
          'data': 'hello world',
          'screenData': '[2J[Hhello world',
          'buffer': true,
          'offset': 0,
          'bufferLength': 11,
          'tail': true,
          'outputSeq': 1,
        },
      ),
      activeSessionId: 'session-1',
    );

    final signal = ValueNotifier<int>(0);
    addTearDown(signal.dispose);
    int? reportedCols;
    int? reportedRows;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            height: 480,
            child: SelfDrawnTerminalView(
              sessionId: 'session-1',
              controller: controller,
              repaintSignal: signal,
              fontSize: 14,
              onResize: (cols, rows) {
                reportedCols = cols;
                reportedRows = rows;
              },
            ),
          ),
        ),
      ),
    );

    // Drain the post-frame resize + snapshot refresh callbacks.
    await tester.pump();
    await tester.pump();

    expect(find.byType(CustomPaint), findsWidgets);
    expect(reportedCols, isNotNull);
    expect(reportedRows, isNotNull);
    expect(reportedCols!, greaterThan(0));
    expect(reportedRows!, greaterThan(0));

    // A new output signal must re-read the snapshot and repaint without error.
    signal.value = 1;
    await tester.pump();
    expect(tester.takeException(), isNull);
  });

  testWidgets('long-press then drag selects text and reports it', (
    tester,
  ) async {
    final controller = RemoteTerminalOutputController();
    addTearDown(controller.dispose);
    controller.bindSession('session-1', requireBaseline: true);
    controller.accept(
      const RelayEnvelope(
        type: 'terminal.output',
        sessionId: 'session-1',
        payload: {
          'data': 'hello world',
          'buffer': true,
          'offset': 0,
          'bufferLength': 11,
          'tail': true,
          'outputSeq': 1,
        },
      ),
      activeSessionId: 'session-1',
    );

    final signal = ValueNotifier<int>(0);
    addTearDown(signal.dispose);
    String? selected = '';

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            height: 480,
            child: SelfDrawnTerminalView(
              sessionId: 'session-1',
              controller: controller,
              repaintSignal: signal,
              fontSize: 14,
              onSelectionChanged: (text) => selected = text,
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    final origin = tester.getTopLeft(find.byType(SelfDrawnTerminalView));
    final gesture = await tester.startGesture(origin + const Offset(6, 8));
    await tester.pump(const Duration(milliseconds: 600)); // long-press fires
    await gesture.moveBy(const Offset(90, 0)); // extend across the first line
    await tester.pump();
    await gesture.up();
    await tester.pump();

    expect(tester.takeException(), isNull);
    expect(selected, isNotNull);
    expect(selected, isNotEmpty);
  });
}
