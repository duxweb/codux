import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/remote_terminal_output_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('truncated baseline renders as the retained terminal window', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);

    final result = controller.accept(
      _terminalBuffer('abcd', offset: 0, bufferLength: 8, truncated: true),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'abcd');
    expect(controller.bufferOffset('session-1'), 8);
  });

  test('retained tail history can start from a non-zero safe offset', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);

    final result = controller.accept(
      _terminalBuffer('tail', offset: 96, bufferLength: 104, truncated: true),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'tail');
    expect(controller.bufferOffset('session-1'), 104);
  });

  test('historical offset after retained window is acked and ignored', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    controller.accept(
      _terminalBuffer('abcd', offset: 0, bufferLength: 8, truncated: true),
      activeSessionId: 'session-1',
    );

    final result = controller.accept(
      _terminalBuffer('gh', offset: 6, bufferLength: 8, truncated: false),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), 'abcd');
    expect(controller.bufferOffset('session-1'), 8);
  });

  test('historical offset is acked without restarting history restore', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 65536);

    controller.bindSession('session-1', requireBaseline: true);
    controller.accept(
      _terminalBuffer(
        'a' * 65536,
        offset: 0,
        bufferLength: 150000,
        truncated: true,
        outputSeq: 951,
        requestId: 'request-1',
      ),
      activeSessionId: 'session-1',
    );
    final firstPage = controller.accept(
      _terminalBuffer(
        'b' * 65536,
        offset: 65536,
        bufferLength: 150000,
        truncated: true,
        outputSeq: 951,
        requestId: 'request-1',
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(firstPage), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), 'a' * 65536);
    expect(controller.bufferOffset('session-1'), 150000);
  });

  test('future baseline offset is acked without a fresh restore loop', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    controller.accept(
      _terminalBuffer('abcd', offset: 0, bufferLength: 12, truncated: true),
      activeSessionId: 'session-1',
    );

    final result = controller.accept(
      _terminalBuffer('future', offset: 8, bufferLength: 12, truncated: false),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), 'abcd');
  });

  test('live output is held until baseline is restored', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);

    final held = controller.accept(
      _liveOutput('new', outputSeq: 11),
      activeSessionId: 'session-1',
    );

    expect(_kinds(held), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), isNull);

    final first = controller.accept(
      _terminalBuffer('old-', offset: 0, bufferLength: 8, truncated: true),
      activeSessionId: 'session-1',
    );

    expect(_kinds(first), [
      ..._activeBufferUpdate,
      RemoteTerminalOutputEffectKind.loading,
      RemoteTerminalOutputEffectKind.sessionUpdated,
      RemoteTerminalOutputEffectKind.ack,
    ]);
    expect(controller.cachedOutput('session-1'), 'old-new');
  });

  test('tail baseline does not replay live output it already covers', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 64);
    controller.bindSession('session-1', requireBaseline: true);

    controller.accept(
      _liveOutput('overlap', outputSeq: 11).withPayload({
        'bufferLength': 12,
        'bufferEnd': 12,
      }),
      activeSessionId: 'session-1',
    );
    controller.accept(
      _terminalBuffer(
        '12345overlap',
        offset: 0,
        bufferLength: 12,
        truncated: false,
        outputSeq: 10,
        tail: true,
      ).withPayload({'bufferEnd': 12}),
      activeSessionId: 'session-1',
    );

    expect(controller.cachedOutput('session-1'), '12345overlap');
  });

  test('live sequence gaps render but request a baseline resync', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: false);
    controller.accept(
      _liveOutput('one', outputSeq: 1),
      activeSessionId: 'session-1',
    );
    expect(controller.hasSequenceGap('session-1'), isFalse);

    final skipped = controller.accept(
      _liveOutput('three', outputSeq: 3),
      activeSessionId: 'session-1',
    );

    expect(_kinds(skipped), [
      RemoteTerminalOutputEffectKind.requestBaselineResync,
      RemoteTerminalOutputEffectKind.loading,
      RemoteTerminalOutputEffectKind.sessionUpdated,
      RemoteTerminalOutputEffectKind.ack,
    ]);
    expect(controller.cachedOutput('session-1'), 'onethree');
    expect(controller.hasSequenceGap('session-1'), isTrue);

    // The gap is only reported once until repaired.
    final next = controller.accept(
      _liveOutput('six', outputSeq: 6),
      activeSessionId: 'session-1',
    );
    expect(
      _kinds(next),
      isNot(contains(RemoteTerminalOutputEffectKind.requestBaselineResync)),
    );

    // A baseline restore repairs the gap.
    controller.startBufferRequest(
      'session-1',
      'request-resync',
      requireBaseline: true,
      replaceActive: true,
    );
    controller.accept(
      _terminalBuffer(
        'sync',
        offset: 0,
        bufferLength: 4,
        truncated: false,
        requestId: 'request-resync',
        outputSeq: 6,
      ),
      activeSessionId: 'session-1',
    );
    expect(controller.hasSequenceGap('session-1'), isFalse);
  });

  test('stale request id baseline cannot replace current terminal state', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    controller.startBufferRequest('session-1', 'request-new');

    final stale = controller.accept(
      _terminalBuffer(
        'old',
        offset: 0,
        bufferLength: 3,
        truncated: false,
        requestId: 'request-old',
      ),
      activeSessionId: 'session-1',
    );
    expect(_kinds(stale), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), isNull);

    final current = controller.accept(
      _terminalBuffer(
        'new',
        offset: 0,
        bufferLength: 3,
        truncated: false,
        requestId: 'request-new',
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(current), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'new');
  });

  test('completed restore request allows a later explicit restore', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    expect(controller.startBufferRequest('session-1', 'request-1'), isTrue);

    final first = controller.accept(
      _terminalBuffer(
        'old-',
        offset: 0,
        bufferLength: 8,
        truncated: true,
        requestId: 'request-1',
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(first), [..._activeBufferUpdate]);
    expect(controller.activeBufferRequestId('session-1'), isNull);
    expect(
      controller.startBufferRequest(
        'session-1',
        'request-2',
        requireBaseline: true,
      ),
      isTrue,
    );
    expect(controller.activeBufferRequestId('session-1'), 'request-2');

    final next = controller.accept(
      _terminalBuffer(
        'next',
        offset: 0,
        bufferLength: 4,
        truncated: false,
        requestId: 'request-2',
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(next), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'next');
  });

  test(
    'empty active baseline stays pending for retry without clearing replay',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 4);

      controller.bindSession('session-1', requireBaseline: true);
      expect(
        controller.startBufferRequest(
          'session-1',
          'request-empty',
          requireBaseline: true,
        ),
        isTrue,
      );

      final empty = controller.accept(
        _terminalBuffer(
          '',
          offset: 0,
          bufferLength: 0,
          truncated: false,
          requestId: 'request-empty',
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(empty), [
        RemoteTerminalOutputEffectKind.loading,
        RemoteTerminalOutputEffectKind.ack,
      ]);
      expect(controller.activeBufferRequestId('session-1'), 'request-empty');
      expect(controller.cachedOutput('session-1'), isNull);
    },
  );

  test('empty refresh baseline preserves cached content', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 65536);

    controller.bindSession('session-1', requireBaseline: false);
    controller.accept(
      _terminalBuffer(
        'history',
        offset: 0,
        bufferLength: 7,
        truncated: false,
        screenData: '\u001b[2J\u001b[Hscreen',
      ),
      activeSessionId: 'session-1',
    );
    expect(controller.cachedOutput('session-1'), 'history');
    expect(
      controller.startBufferRequest(
        'session-1',
        'refresh-empty',
        requireBaseline: true,
        replaceActive: true,
      ),
      isTrue,
    );

    final empty = controller.accept(
      _terminalBuffer(
        '',
        offset: 0,
        bufferLength: 0,
        truncated: false,
        requestId: 'refresh-empty',
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(empty), [
      RemoteTerminalOutputEffectKind.loading,
      RemoteTerminalOutputEffectKind.ack,
    ]);
    expect(controller.cachedOutput('session-1'), 'history');
  });

  test(
    'full buffer request replaces cache even when recent history offset is non-zero',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 4);

      controller.bindSession('session-1', requireBaseline: false);
      controller.accept(
        _liveOutput('stale', outputSeq: 1),
        activeSessionId: 'session-1',
      );
      controller.startBufferRequest(
        'session-1',
        'request-1',
        requireBaseline: true,
      );

      final result = controller.accept(
        _terminalBuffer(
          'tail',
          offset: 96,
          bufferLength: 100,
          truncated: false,
          requestId: 'request-1',
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(result), [..._activeBufferUpdate]);
      expect(controller.cachedOutput('session-1'), 'tail');
      expect(controller.bufferOffset('session-1'), 100);
    },
  );

  test('tail history window renders without requesting pages', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    controller.startBufferRequest('session-1', 'request-1');

    final result = controller.accept(
      _terminalBuffer(
        'tail',
        offset: 96,
        bufferLength: 100,
        truncated: false,
        requestId: 'request-1',
        tail: true,
        hasPrevious: true,
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'tail');
    expect(controller.bufferOffset('session-1'), 100);
  });

  test('tail history window realigns live output after a sequence gap', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: false);
    controller.accept(
      _liveOutput('one', outputSeq: 1),
      activeSessionId: 'session-1',
    );
    final gap = controller.accept(
      _liveOutput('gap', outputSeq: 10),
      activeSessionId: 'session-1',
    );
    expect(_kinds(gap), [
      RemoteTerminalOutputEffectKind.requestBaselineResync,
      RemoteTerminalOutputEffectKind.loading,
      RemoteTerminalOutputEffectKind.sessionUpdated,
      RemoteTerminalOutputEffectKind.ack,
    ]);

    controller.startBufferRequest('session-1', 'request-1');
    final snapshot = controller.accept(
      _terminalBuffer(
        'tail',
        offset: 96,
        bufferLength: 100,
        truncated: false,
        outputSeq: 10,
        requestId: 'request-1',
        tail: true,
        hasPrevious: true,
      ),
      activeSessionId: 'session-1',
    );
    final live = controller.accept(
      _liveOutput('next', outputSeq: 11),
      activeSessionId: 'session-1',
    );

    expect(_kinds(snapshot), [..._activeBufferUpdate]);
    expect(_kinds(live), [
      RemoteTerminalOutputEffectKind.loading,
      RemoteTerminalOutputEffectKind.sessionUpdated,
      RemoteTerminalOutputEffectKind.ack,
    ]);
    expect(controller.cachedOutput('session-1'), 'tailnext');
  });

  test('tail history window replaces visible history without paging', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    controller.startBufferRequest('session-1', 'request-1');

    final result = controller.accept(
      _terminalBuffer(
        'ready',
        offset: 0,
        bufferLength: 12,
        truncated: false,
        requestId: 'request-1',
        tail: true,
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'ready');
  });

  test(
    'terminal buffer uses screen baseline for visible screen while retaining raw history',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 65536);

      controller.bindSession('session-1', requireBaseline: true);

      final result = controller.accept(
        _terminalBuffer(
          'raw tail fragment',
          offset: 0,
          bufferLength: 17,
          truncated: false,
          screenData: '\u001b[2J\u001b[Hvisible tui',
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(result), [..._activeBufferUpdate]);
      expect(_markBufferReceived(result).baselineScreenKeyframe, isTrue);
      expect(controller.cachedOutput('session-1'), 'raw tail fragment');
      expect(controller.cachedOutput('session-1'), 'raw tail fragment');
    },
  );

  test(
    'live output with screen keyframe updates visible screen without waiting for baseline',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 65536);

      controller.bindSession('session-1', requireBaseline: true);

      final result = controller.accept(
        _liveOutput(
          'partial live raw',
          outputSeq: 11,
          screenData: '\u001b[2J\u001b[Hrestored tui\n\u001b[3;1Hinput box',
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(result), [
        RemoteTerminalOutputEffectKind.loading,
        RemoteTerminalOutputEffectKind.sessionUpdated,
        RemoteTerminalOutputEffectKind.ack,
      ]);
      expect(controller.cachedOutput('session-1'), 'partial live raw');
      expect(controller.cachedOutput('session-1'), 'partial live raw');
    },
  );

  test(
    'live screen keyframe completes pending baseline and ignores stale buffer',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 65536);

      controller.bindSession('session-1', requireBaseline: true);
      final live = controller.accept(
        _liveOutput(
          'partial live raw',
          outputSeq: 11,
          screenData: '\u001b[2J\u001b[Hrestored tui\n\u001b[3;1Hinput box',
        ),
        activeSessionId: 'session-1',
      );
      final stale = controller.accept(
        _terminalBuffer(
          'old screen',
          offset: 0,
          bufferLength: 10,
          truncated: false,
          outputSeq: 10,
          requestId: 'stale-restore',
          screenData: '\u001b[2J\u001b[Hold screen',
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(live), [
        RemoteTerminalOutputEffectKind.loading,
        RemoteTerminalOutputEffectKind.sessionUpdated,
        RemoteTerminalOutputEffectKind.ack,
      ]);
      expect(_kinds(stale), [RemoteTerminalOutputEffectKind.ack]);
      expect(controller.cachedOutput('session-1'), 'partial live raw');
    },
  );

  test(
    'tail history window with previous history does not hydrate on ui mount',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 4);

      controller.bindSession('session-1', requireBaseline: true);
      controller.startBufferRequest('session-1', 'request-1');

      final result = controller.accept(
        _terminalBuffer(
          'ready',
          offset: 0,
          bufferLength: 12,
          truncated: false,
          requestId: 'request-1',
          tail: true,
          hasPrevious: true,
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(result), [..._activeBufferUpdate]);
    },
  );

  test('tail history window updates retained history watermark', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    controller.bindSession('session-1', requireBaseline: true);
    controller.startBufferRequest('session-1', 'history-1');
    controller.accept(
      _terminalBuffer(
        'history-ready',
        offset: 0,
        bufferLength: 12,
        truncated: false,
        requestId: 'history-1',
        outputSeq: 10,
      ),
      activeSessionId: 'session-1',
    );
    expect(controller.bufferOffset('session-1'), 12);

    controller.startBufferRequest('session-1', 'tail-1');
    final result = controller.accept(
      _terminalBuffer(
        'now',
        offset: 0,
        bufferLength: 3,
        truncated: false,
        requestId: 'tail-1',
        tail: true,
        hasPrevious: true,
        outputSeq: 11,
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [..._activeBufferUpdate]);
    expect(controller.cachedOutput('session-1'), 'now');
    expect(controller.bufferOffset('session-1'), 3);
  });

  test('inactive live output updates cache without rendering to ui', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);

    final result = controller.accept(
      _liveOutputForSession('session-2', 'background', outputSeq: 1),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-2'), 'background');
    expect(controller.cachedOutput('session-1'), isNull);
  });

  test(
    'inactive tail history window updates cache without rendering to ui',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 4);

      controller.bindSession('session-2', requireBaseline: true);
      controller.startBufferRequest('session-2', 'request-2');

      final result = controller.accept(
        _terminalBufferForSession(
          'session-2',
          'background',
          offset: 0,
          bufferLength: 20,
          truncated: false,
          requestId: 'request-2',
          tail: true,
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(result), [RemoteTerminalOutputEffectKind.ack]);
      expect(controller.cachedOutput('session-2'), 'background');
    },
  );

  test('many inactive project sessions hydrate cache without ui rendering', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 8);

    final b = controller.accept(
      _terminalBufferForSession(
        'session-b',
        'project-b',
        offset: 0,
        bufferLength: 9,
        truncated: false,
        outputSeq: 1,
      ),
      activeSessionId: 'session-a',
    );
    final c = controller.accept(
      _liveOutputForSession('session-c', 'project-c', outputSeq: 1),
      activeSessionId: 'session-a',
    );

    expect(_kinds(b), [RemoteTerminalOutputEffectKind.ack]);
    expect(_kinds(c), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-b'), 'project-b');
    expect(controller.cachedOutput('session-c'), 'project-c');
  });

  test(
    'fast project switching keeps all remote pty sessions hydrated in cache',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 8);

      controller.bindSession('session-a', requireBaseline: true);
      controller.bindSession('session-b', requireBaseline: true);
      controller.bindSession('session-c', requireBaseline: true);
      controller.startBufferRequest('session-a', 'request-a');
      controller.startBufferRequest('session-b', 'request-b');
      controller.startBufferRequest('session-c', 'request-c');

      final aFirst = controller.accept(
        _terminalBufferForSession(
          'session-a',
          'a-hist--',
          offset: 0,
          bufferLength: 12,
          truncated: true,
          outputSeq: 10,
          requestId: 'request-a',
        ),
        activeSessionId: 'session-a',
      );
      final bFirst = controller.accept(
        _terminalBufferForSession(
          'session-b',
          'b-hist--',
          offset: 0,
          bufferLength: 12,
          truncated: true,
          outputSeq: 20,
          requestId: 'request-b',
        ),
        activeSessionId: 'session-a',
      );
      final cSnapshot = controller.accept(
        _terminalBufferForSession(
          'session-c',
          'c-hist-',
          offset: 0,
          bufferLength: 7,
          truncated: false,
          outputSeq: 30,
          requestId: 'request-c',
        ),
        activeSessionId: 'session-b',
      );
      final cLive = controller.accept(
        _liveOutputForSession('session-c', 'c-live', outputSeq: 31),
        activeSessionId: 'session-b',
      );
      final bSecond = controller.accept(
        _terminalBufferForSession(
          'session-b',
          'done',
          offset: 8,
          bufferLength: 12,
          truncated: false,
          outputSeq: 20,
          requestId: 'request-b',
        ),
        activeSessionId: 'session-c',
      );
      final aSecond = controller.accept(
        _terminalBufferForSession(
          'session-a',
          'done',
          offset: 8,
          bufferLength: 12,
          truncated: false,
          outputSeq: 10,
          requestId: 'request-a',
        ),
        activeSessionId: 'session-b',
      );

      expect(_kinds(aFirst), [..._activeBufferUpdate]);
      expect(_kinds(bFirst), [RemoteTerminalOutputEffectKind.ack]);
      expect(_kinds(cSnapshot), [RemoteTerminalOutputEffectKind.ack]);
      expect(_kinds(cLive), [RemoteTerminalOutputEffectKind.ack]);
      expect(_kinds(bSecond), [RemoteTerminalOutputEffectKind.ack]);
      expect(_kinds(aSecond), [RemoteTerminalOutputEffectKind.ack]);
      expect(controller.cachedOutput('session-a'), 'a-hist--');
      expect(controller.cachedOutput('session-b'), 'b-hist--');
      expect(controller.cachedOutput('session-c'), 'c-hist-c-live');
      expect(controller.bufferOffset('session-a'), 12);
      expect(controller.bufferOffset('session-b'), 12);
    },
  );

  test(
    'incremental buffer at offset zero appends when cache already exists',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 4);
      controller.bindSession('session-1', requireBaseline: false);
      controller.accept(
        _terminalBuffer('old', offset: 0, bufferLength: 3, truncated: false),
        activeSessionId: 'session-1',
      );

      controller.startBufferRequest(
        'session-1',
        'resume-1',
        resetAssembler: false,
      );
      final result = controller.accept(
        _terminalBuffer(
          'new',
          offset: 0,
          bufferLength: 6,
          truncated: false,
          outputSeq: 11,
          requestId: 'resume-1',
        ),
        activeSessionId: 'session-1',
      );

      expect(_kinds(result), [..._activeBufferUpdate]);
      expect(controller.cachedOutput('session-1'), 'oldnew');
    },
  );

  test('duplicate baseline is acked without replaying cached session', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 65536);
    controller.bindSession('session-1', requireBaseline: false);
    controller.accept(
      _terminalBuffer(
        'cached',
        offset: 0,
        bufferLength: 6,
        truncated: false,
        outputSeq: 10,
      ),
      activeSessionId: 'session-1',
    );

    final result = controller.accept(
      _terminalBuffer(
        'old-prefix',
        offset: 0,
        bufferLength: 400000,
        truncated: true,
        outputSeq: 697,
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), 'cached');
    expect(controller.bufferOffset('session-1'), 6);
  });

  test('same sequence baseline is acked without duplicating prompt text', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 65536);
    controller.bindSession('session-1', requireBaseline: false);
    controller.accept(
      _terminalBuffer(
        '➜  20260611-212631\n',
        offset: 0,
        bufferLength: 21,
        truncated: false,
        outputSeq: 42,
      ),
      activeSessionId: 'session-1',
    );

    final result = controller.accept(
      _terminalBuffer(
        '➜  20260611-212631\n',
        offset: 0,
        bufferLength: 21,
        truncated: false,
        outputSeq: 42,
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(result), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), '➜  20260611-212631\n');
  });

  test(
    'duplicate chunked transfer is acked before assembly without replaying cache',
    () {
      final controller = RemoteTerminalOutputController(maxBufferChars: 65536);
      controller.bindSession('session-1', requireBaseline: false);
      controller.accept(
        _terminalBuffer(
          'cached',
          offset: 0,
          bufferLength: 6,
          truncated: false,
          outputSeq: 10,
        ),
        activeSessionId: 'session-1',
      );

      final first = controller.accept(
        _terminalBuffer(
          'old-',
          offset: 0,
          bufferLength: 400000,
          truncated: true,
          outputSeq: 697,
        ).withPayload({
          'snapshotId': 'snapshot-1',
          'chunkIndex': 0,
          'chunkCount': 2,
          'chunked': true,
        }),
        activeSessionId: 'session-1',
      );
      final second = controller.accept(
        _terminalBuffer(
          'prefix',
          offset: 4,
          bufferLength: 400000,
          truncated: true,
          outputSeq: 697,
        ).withPayload({
          'snapshotId': 'snapshot-1',
          'chunkIndex': 1,
          'chunkCount': 2,
          'chunked': true,
        }),
        activeSessionId: 'session-1',
      );

      expect(_kinds(first), [RemoteTerminalOutputEffectKind.ack]);
      expect(_kinds(second), [RemoteTerminalOutputEffectKind.ack]);
      expect(controller.cachedOutput('session-1'), 'cached');
      expect(controller.bufferOffset('session-1'), 6);
    },
  );

  test('stale offset from completed restore is ignored', () {
    final controller = RemoteTerminalOutputController(maxBufferChars: 4);
    controller.bindSession('session-1', requireBaseline: true);
    controller.startBufferRequest(
      'session-1',
      'restore-1',
      requireBaseline: true,
    );
    final first = controller.accept(
      _terminalBuffer(
        'abcd',
        offset: 0,
        bufferLength: 8,
        truncated: true,
        requestId: 'restore-1',
      ),
      activeSessionId: 'session-1',
    );
    expect(_kinds(first), [..._activeBufferUpdate]);

    final second = controller.accept(
      _terminalBuffer(
        'efgh',
        offset: 4,
        bufferLength: 8,
        truncated: false,
        requestId: 'restore-1',
      ),
      activeSessionId: 'session-1',
    );

    expect(_kinds(second), [RemoteTerminalOutputEffectKind.ack]);
    expect(controller.cachedOutput('session-1'), 'abcd');
  });
}

RelayEnvelope _terminalBuffer(
  String data, {
  required int offset,
  required int bufferLength,
  required bool truncated,
  int outputSeq = 10,
  String? requestId,
  bool tail = false,
  bool hasPrevious = false,
  String? screenData,
}) {
  final payload = <String, Object?>{
    'data': data,
    'buffer': true,
    'offset': offset,
    'bufferLength': bufferLength,
    'truncated': truncated,
    'outputSeq': outputSeq,
  };
  if (requestId != null) payload['requestId'] = requestId;
  if (tail) payload['tail'] = true;
  if (hasPrevious) payload['hasPrevious'] = true;
  if (screenData != null) payload['screenData'] = screenData;
  return RelayEnvelope(
    type: 'terminal.output',
    sessionId: 'session-1',
    payload: payload,
  );
}

extension _RelayEnvelopeTestPayload on RelayEnvelope {
  RelayEnvelope withPayload(Map<String, Object?> extra) {
    final current = payload as Map;
    return copyWith(
      payload: <String, Object?>{
        for (final entry in current.entries) '${entry.key}': entry.value,
        ...extra,
      },
    );
  }
}

RelayEnvelope _terminalBufferForSession(
  String sessionId,
  String data, {
  required int offset,
  required int bufferLength,
  required bool truncated,
  int outputSeq = 10,
  String? requestId,
  bool tail = false,
  bool hasPrevious = false,
}) {
  final payload = <String, Object?>{
    'data': data,
    'buffer': true,
    'offset': offset,
    'bufferLength': bufferLength,
    'truncated': truncated,
    'outputSeq': outputSeq,
  };
  if (requestId != null) payload['requestId'] = requestId;
  if (tail) payload['tail'] = true;
  if (hasPrevious) payload['hasPrevious'] = true;
  return RelayEnvelope(
    type: 'terminal.output',
    sessionId: sessionId,
    payload: payload,
  );
}

RelayEnvelope _liveOutput(
  String data, {
  required int outputSeq,
  String? screenData,
}) {
  return _liveOutputForSession(
    'session-1',
    data,
    outputSeq: outputSeq,
    screenData: screenData,
  );
}

RelayEnvelope _liveOutputForSession(
  String sessionId,
  String data, {
  required int outputSeq,
  String? screenData,
}) {
  final payload = <String, Object?>{'data': data, 'outputSeq': outputSeq};
  if (screenData != null) payload['screenData'] = screenData;
  return RelayEnvelope(
    type: 'terminal.output',
    sessionId: sessionId,
    payload: payload,
  );
}

List<RemoteTerminalOutputEffectKind> _kinds(
  List<RemoteTerminalOutputEffect> effects,
) => effects.map((effect) => effect.kind).toList();

RemoteTerminalOutputEffect _markBufferReceived(
  List<RemoteTerminalOutputEffect> effects,
) => effects.firstWhere(
  (effect) => effect.kind == RemoteTerminalOutputEffectKind.markBufferReceived,
);

const _activeBufferUpdate = [
  RemoteTerminalOutputEffectKind.sessionUpdated,
  RemoteTerminalOutputEffectKind.markBufferReceived,
  RemoteTerminalOutputEffectKind.ack,
];
