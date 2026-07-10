import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/terminal_viewport_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('emits the first terminal resize and ignores duplicates', () {
    final controller = TerminalViewportController();

    final first = controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    // resize() only proposes; the dedup cache is committed via markSent after
    // the caller actually sends the envelope.
    expect(first, isNotNull);
    expect(first!.cols, 80);
    expect(first.rows, 24);
    controller.markSent('session-1', first);

    final duplicate = controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    expect(duplicate, isNull);
  });

  test('keeps the last row count while keyboard is visible', () {
    final controller = TerminalViewportController();

    final base = controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    controller.markSent('session-1', base!);

    final next = controller.resize(
      sessionId: 'session-1',
      cols: 100,
      rows: 10,
      keyboardVisible: true,
    );

    expect(next, isNotNull);
    expect(next!.cols, 100);
    expect(next.rows, 24);
    expect(controller.pendingCols, 100);
    expect(controller.pendingRows, 10);
  });

  test('baseline viewport uses protected rows while keyboard is visible', () {
    final controller = TerminalViewportController();

    final base = controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    controller.markSent('session-1', base!);
    controller.resize(
      sessionId: 'session-1',
      cols: 100,
      rows: 10,
      keyboardVisible: true,
    );

    final pending = controller.pendingSizeFor('session-1');

    expect(pending, isNotNull);
    expect(pending!.cols, 100);
    expect(pending.rows, 24);
  });

  test(
    'recorded measured size drives baseline viewport before first resize',
    () {
      final controller = TerminalViewportController();

      controller.recordMeasured(90, 30);

      final pending = controller.pendingSizeFor('session-1');
      expect(pending, isNotNull);
      expect(pending!.cols, 90);
      expect(pending.rows, 30);
    },
  );

  test('flushes pending keyboard resize when forced', () {
    final controller = TerminalViewportController();

    controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    controller.resize(
      sessionId: 'session-1',
      cols: 100,
      rows: 10,
      keyboardVisible: true,
    );

    final flushed = controller.flushPending(
      sessionId: 'session-1',
      force: true,
    );

    expect(flushed, isNotNull);
    expect(flushed!.cols, 100);
    expect(flushed.rows, 10);
  });

  test('force flush always proposes and a new session flushes pending', () {
    final controller = TerminalViewportController();

    controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );

    // force bypasses the dedup cache so a dropped envelope cannot suppress the
    // resize (used at bind to guarantee the host is told the size).
    final forced = controller.flushPending(sessionId: 'session-1', force: true);
    expect(forced, isNotNull);
    expect(forced!.cols, 80);
    expect(forced.rows, 24);

    // A different session with no committed size flushes the pending size.
    final nextSession = controller.flushPending(
      sessionId: 'session-2',
      force: false,
    );
    expect(nextSession, isNotNull);
    expect(nextSession!.cols, 80);
    expect(nextSession.rows, 24);
  });

  test('force flush repeats an already sent session size', () {
    final controller = TerminalViewportController();

    final first = controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    controller.markSent('session-1', first!);

    final forced = controller.flushPending(sessionId: 'session-1', force: true);

    expect(forced, isNotNull);
    expect(forced!.cols, 80);
    expect(forced.rows, 24);
  });

  test('force flush emits a pending resize that was not sent while hidden', () {
    final controller = TerminalViewportController();

    final hiddenResize = controller.resize(
      sessionId: 'session-1',
      cols: 90,
      rows: 30,
      keyboardVisible: false,
    );
    expect(hiddenResize, isNotNull);

    final visibleFlush = controller.flushPending(
      sessionId: 'session-1',
      force: true,
    );

    expect(visibleFlush, isNotNull);
    expect(visibleFlush!.cols, 90);
    expect(visibleFlush.rows, 30);
  });

  test('tracks remote viewport owner and ignores stale generations', () {
    final controller = TerminalViewportController();

    expect(
      controller.applyRemoteState(
        const RelayEnvelope(
          type: 'terminal.viewport.state',
          sessionId: 'session-1',
          payload: {
            'owner': 'desktop',
            'cols': 120,
            'rows': 40,
            'generation': 2,
          },
        ),
      ),
      isTrue,
    );
    expect(controller.ownerFor('session-1'), 'desktop');
    expect(controller.generation, 2);

    expect(
      controller.applyRemoteState(
        const RelayEnvelope(
          type: 'terminal.viewport.state',
          sessionId: 'session-1',
          payload: {'owner': 'mobile', 'cols': 80, 'rows': 24, 'generation': 1},
        ),
      ),
      isFalse,
    );
    expect(controller.ownerFor('session-1'), 'desktop');
    expect(controller.generation, 2);
  });

  test('remote viewport state updates per-session sent size', () {
    final controller = TerminalViewportController();

    controller.resize(
      sessionId: 'session-1',
      cols: 80,
      rows: 24,
      keyboardVisible: false,
    );
    controller.applyRemoteState(
      const RelayEnvelope(
        type: 'terminal.viewport.state',
        sessionId: 'session-1',
        payload: {'owner': 'desktop', 'cols': 120, 'rows': 40, 'generation': 1},
      ),
    );

    expect(
      controller.resize(
        sessionId: 'session-1',
        cols: 80,
        rows: 24,
        keyboardVisible: false,
      ),
      isNotNull,
    );
  });

  test('removed session accepts a rebuilt viewport with reset generation', () {
    final controller = TerminalViewportController();
    controller.recordMeasured(80, 24);
    final initial = controller.flushPending(
      sessionId: 'session-1',
      force: false,
    );
    controller.markSent('session-1', initial!);
    controller.applyRemoteState(
      const RelayEnvelope(
        type: 'terminal.viewport.state',
        sessionId: 'session-1',
        payload: {'owner': 'desktop', 'cols': 120, 'rows': 40, 'generation': 8},
      ),
    );

    controller.removeSession('session-1');

    expect(controller.ownerFor('session-1'), isNull);
    expect(controller.reportedSize('session-1'), isNull);
    expect(
      controller.flushPending(sessionId: 'session-1', force: false),
      isNotNull,
    );
    expect(
      controller.applyRemoteState(
        const RelayEnvelope(
          type: 'terminal.viewport.state',
          sessionId: 'session-1',
          payload: {'owner': 'mobile', 'cols': 80, 'rows': 24, 'generation': 1},
        ),
      ),
      isTrue,
    );
    expect(controller.ownerFor('session-1'), 'mobile');
  });
}
