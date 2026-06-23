import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:codux_flutter/services/terminal_buffer_retry.dart';

void main() {
  test('terminal buffer request retries until buffer is acknowledged', () {
    final timers = <_FakeTimer>[];
    final sent = <String>[];
    final exhausted = <String>[];
    final retry = TerminalBufferRetryCoordinator(
      onRetryExhausted: exhausted.add,
      timerFactory: (delay, callback) {
        final timer = _FakeTimer(callback);
        timers.add(timer);
        return timer;
      },
    );

    expect(
      retry.requestIfReady(
        sessionId: 'session-1',
        send: (sessionId) {
          sent.add(sessionId);
          return true;
        },
      ),
      isTrue,
    );
    expect(sent, ['session-1']);
    expect(retry.pendingSessionId, 'session-1');

    expect(
      retry.requestIfReady(
        sessionId: 'session-1',
        send: (sessionId) {
          sent.add(sessionId);
          return true;
        },
      ),
      isFalse,
    );
    expect(sent.length, 1);

    timers[0].fire();
    timers[1].fire();
    timers[2].fire();

    expect(sent, ['session-1', 'session-1', 'session-1', 'session-1']);
    expect(exhausted, ['session-1']);
  });

  test('terminal buffer acknowledgement cancels retry timer', () {
    final timers = <_FakeTimer>[];
    final sent = <String>[];
    final retry = TerminalBufferRetryCoordinator(
      timerFactory: (delay, callback) {
        final timer = _FakeTimer(callback);
        timers.add(timer);
        return timer;
      },
    );

    retry.requestIfReady(
      sessionId: 'session-1',
      send: (sessionId) {
        sent.add(sessionId);
        return true;
      },
    );

    retry.markReceived(sessionId: 'session-1', activeSessionId: 'session-1');
    timers.single.fire();

    expect(sent, ['session-1']);
    expect(retry.pendingSessionId, isNull);
    expect(retry.retryAttempt, 0);
  });

  test('terminal buffer request only requires a session id', () {
    final sent = <String>[];
    final retry = TerminalBufferRetryCoordinator();

    expect(
      retry.requestIfReady(
        sessionId: 'session-1',
        send: (sessionId) {
          sent.add(sessionId);
          return true;
        },
      ),
      isTrue,
    );
    expect(
      retry.requestIfReady(
        sessionId: null,
        send: (sessionId) {
          sent.add(sessionId);
          return true;
        },
      ),
      isFalse,
    );

    expect(sent, ['session-1']);
  });

  test('full baseline request can replace a pending buffer request', () {
    final timers = <_FakeTimer>[];
    final sent = <String>[];
    final retry = TerminalBufferRetryCoordinator(
      timerFactory: (delay, callback) {
        final timer = _FakeTimer(callback);
        timers.add(timer);
        return timer;
      },
    );

    expect(
      retry.requestIfReady(
        sessionId: 'session-1',
        send: (sessionId) {
          sent.add('partial:$sessionId');
          return true;
        },
      ),
      isTrue,
    );
    expect(
      retry.requestIfReady(
        sessionId: 'session-1',
        force: true,
        replacePending: true,
        send: (sessionId) {
          sent.add('full:$sessionId');
          return true;
        },
      ),
      isTrue,
    );

    expect(sent, ['partial:session-1', 'full:session-1']);
    expect(retry.pendingSessionId, 'session-1');
    expect(timers.first.isActive, isFalse);
    expect(timers.last.isActive, isTrue);
  });

  test(
    'baseline retry waits while the transfer progresses, re-issues on stall',
    () {
      final timers = <_FakeTimer>[];
      final sent = <String>[];
      // The host keeps the request flagged pending for the whole transfer,
      // including a stalled (incomplete) one -- so pending alone must NOT gate
      // re-issue; only the absence of progress does.
      final retry = TerminalBufferRetryCoordinator(
        timerFactory: (delay, callback) {
          final timer = _FakeTimer(callback);
          timers.add(timer);
          return timer;
        },
      );

      retry.trackWhilePending(
        'session-1',
        send: (sessionId) {
          sent.add(sessionId);
          return true;
        },
        hasPendingRequest: (_) => true,
      );

      // A live (slow) transfer reports progress between ticks: never re-issued.
      retry.noteProgress('session-1');
      timers[0].fire();
      retry.noteProgress('session-1');
      timers[1].fire();
      expect(sent, isEmpty);
      expect(retry.pendingSessionId, 'session-1');

      // Chunks stop arriving. One grace tick is tolerated...
      timers[2].fire();
      expect(sent, isEmpty);

      // ...then the stalled transfer is re-issued instead of spinning forever.
      timers[3].fire();
      expect(sent, ['session-1']);
      expect(retry.pendingSessionId, 'session-1');
    },
  );
}

final class _FakeTimer implements Timer {
  _FakeTimer(this._callback);

  final void Function() _callback;
  var _active = true;

  void fire() {
    if (!_active) return;
    _active = false;
    _callback();
  }

  @override
  void cancel() {
    _active = false;
  }

  @override
  bool get isActive => _active;

  @override
  int get tick => 0;
}
