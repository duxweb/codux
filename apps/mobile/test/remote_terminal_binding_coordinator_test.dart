import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/remote_capabilities.dart';
import 'package:codux_flutter/services/remote_protocol.dart';
import 'package:codux_flutter/services/remote_runtime_store.dart';
import 'package:codux_flutter/services/remote_terminal_binding_coordinator.dart';
import 'package:codux_flutter/services/remote_terminal_output_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('session baseline subscription starts correlated buffer request', () {
    final sent = <RelayEnvelope>[];
    final output = RemoteTerminalOutputController();
    final coordinator = _coordinator(output: output, sent: sent);

    final subscribed = coordinator.subscribeSession(
      sessionId: 'term-1',
      reason: 'test',
      capability: const TerminalBufferCapability(chunking: true),
    );

    expect(subscribed, isTrue);
    expect(sent, hasLength(1));
    expect(sent.single.type, RemoteMessageType.resourceSubscribe);
    expect(sent.single.sessionId, 'term-1');
    expect(sent.single.payload, containsPair('baseline', true));
    expect(
      sent.single.payload,
      containsPair('requestId', 'req-session-term-1-1'),
    );
    expect(output.activeBufferRequestId('term-1'), 'req-session-term-1-1');
  });

  test('session live subscription skips baseline request', () {
    final sent = <RelayEnvelope>[];
    final output = RemoteTerminalOutputController();
    final coordinator = _coordinator(output: output, sent: sent);

    final subscribed = coordinator.subscribeSession(
      sessionId: 'term-1',
      reason: 'cached',
      capability: TerminalBufferCapability.fallback,
      baseline: false,
    );

    expect(subscribed, isTrue);
    expect(sent.single.sessionId, 'term-1');
    expect(sent.single.payload, isNot(contains('baseline')));
    expect(sent.single.payload, isNot(contains('requestId')));
    expect(output.activeBufferRequestId('term-1'), isNull);
  });

  test('project topology subscription switches without baseline metadata', () {
    final sent = <RelayEnvelope>[];
    final coordinator = _coordinator(sent: sent);

    coordinator.replaceProjectSubscription(
      projectId: 'project-a',
      reason: 'first',
    );
    coordinator.replaceProjectSubscription(
      projectId: 'project-b',
      reason: 'switch',
    );

    expect(sent, hasLength(3));
    expect(sent[0].type, RemoteMessageType.resourceSubscribe);
    expect(sent[0].payload, containsPair('projectId', 'project-a'));
    expect(sent[0].payload, isNot(contains('baseline')));
    expect(sent[1].type, RemoteMessageType.resourceUnsubscribe);
    expect(sent[1].payload, containsPair('projectId', 'project-a'));
    expect(sent[2].payload, containsPair('projectId', 'project-b'));
  });

  test('project switch retries when previous unsubscribe send fails', () {
    final sent = <RelayEnvelope>[];
    var failUnsubscribe = true;
    final coordinator = _coordinator(
      sent: sent,
      send: (envelope) {
        sent.add(envelope);
        if (envelope.type == RemoteMessageType.resourceUnsubscribe &&
            failUnsubscribe) {
          failUnsubscribe = false;
          return false;
        }
        return true;
      },
    );
    coordinator.replaceProjectSubscription(
      projectId: 'project-a',
      reason: 'first',
    );
    sent.clear();

    coordinator.replaceProjectSubscription(
      projectId: 'project-b',
      reason: 'failed-switch',
    );
    coordinator.replaceProjectSubscription(
      projectId: 'project-b',
      reason: 'retry-switch',
    );

    expect(sent, hasLength(3));
    expect(sent[0].type, RemoteMessageType.resourceUnsubscribe);
    expect(sent[1].type, RemoteMessageType.resourceUnsubscribe);
    expect(sent[2].type, RemoteMessageType.resourceSubscribe);
    expect(sent[2].payload, containsPair('projectId', 'project-b'));
  });

  test('uncached bind subscribes topology and session baseline', () {
    final sent = <RelayEnvelope>[];
    final output = RemoteTerminalOutputController();
    final coordinator = _coordinator(output: output, sent: sent);

    final result = coordinator.bindSession(
      plan: const RemoteRuntimePlan(bindSessionId: 'term-1'),
      bindSessionId: 'term-1',
      reason: 'select',
      selectedProjectId: 'project-1',
      capability: TerminalBufferCapability.fallback,
      restored: false,
    );

    expect(result.baselineRequested, isTrue);
    expect(sent, hasLength(2));
    expect(sent[0].payload, containsPair('projectId', 'project-1'));
    expect(sent[1].sessionId, 'term-1');
    expect(sent[1].payload, containsPair('baseline', true));
  });

  test(
    'gap-free cached bind keeps live session subscription without baseline',
    () {
      final sent = <RelayEnvelope>[];
      final output = RemoteTerminalOutputController();
      _cache(output, 'term-1');
      final coordinator = _coordinator(output: output, sent: sent);

      final result = coordinator.bindSession(
        plan: const RemoteRuntimePlan(bindSessionId: 'term-1'),
        bindSessionId: 'term-1',
        reason: 'return',
        selectedProjectId: 'project-1',
        capability: TerminalBufferCapability.fallback,
        restored: true,
      );

      expect(result.baselineRequested, isFalse);
      expect(sent, hasLength(2));
      expect(sent.last.sessionId, 'term-1');
      expect(sent.last.payload, isNot(contains('baseline')));
      expect(output.activeBufferRequestId('term-1'), isNull);
    },
  );

  test('stale cached bind requests a new session baseline', () {
    final sent = <RelayEnvelope>[];
    final output = RemoteTerminalOutputController();
    _cache(output, 'term-1');
    final coordinator = _coordinator(output: output, sent: sent);
    coordinator.markSessionBaselineStale('term-1');

    final result = coordinator.bindSession(
      plan: const RemoteRuntimePlan(bindSessionId: 'term-1'),
      bindSessionId: 'term-1',
      reason: 'return',
      selectedProjectId: 'project-1',
      capability: TerminalBufferCapability.fallback,
      restored: true,
    );

    expect(result.baselineRequested, isTrue);
    expect(sent.last.sessionId, 'term-1');
    expect(sent.last.payload, containsPair('baseline', true));
  });

  test('session unsubscribe is sent once for a tracked subscription', () {
    final sent = <RelayEnvelope>[];
    final coordinator = _coordinator(sent: sent);
    coordinator.subscribeSession(
      sessionId: 'term-1',
      reason: 'bind',
      capability: TerminalBufferCapability.fallback,
      baseline: false,
    );
    sent.clear();

    coordinator.unsubscribeSession('term-1', reason: 'evict');
    coordinator.unsubscribeSession('term-1', reason: 'duplicate');

    expect(sent, hasLength(1));
    expect(sent.single.type, RemoteMessageType.resourceUnsubscribe);
    expect(sent.single.sessionId, 'term-1');
  });

  test('session unsubscribe remains tracked when send fails', () {
    final sent = <RelayEnvelope>[];
    var failUnsubscribe = true;
    final coordinator = _coordinator(
      sent: sent,
      send: (envelope) {
        sent.add(envelope);
        if (envelope.type == RemoteMessageType.resourceUnsubscribe &&
            failUnsubscribe) {
          failUnsubscribe = false;
          return false;
        }
        return true;
      },
    );
    coordinator.subscribeSession(
      sessionId: 'term-1',
      reason: 'bind',
      capability: TerminalBufferCapability.fallback,
      baseline: false,
    );
    coordinator.markSessionBaselineStale('term-1');
    sent.clear();

    coordinator.unsubscribeSession('term-1', reason: 'failed-evict');
    coordinator.unsubscribeSession('term-1', reason: 'retry-evict');
    coordinator.unsubscribeSession('term-1', reason: 'duplicate');

    expect(sent, hasLength(2));
    expect(sent.every((message) => message.sessionId == 'term-1'), isTrue);
    expect(coordinator.isSessionBaselineStale('term-1'), isFalse);
  });

  test('visible resubscribe refreshes topology and session baseline', () {
    final sent = <RelayEnvelope>[];
    final coordinator = _coordinator(sent: sent);

    final requested = coordinator.resubscribeVisibleTerminal(
      transportConnected: true,
      protocolReady: true,
      activeSessionId: 'term-1',
      selectedProjectId: 'project-1',
      capability: TerminalBufferCapability.fallback,
      reason: 'reconnect',
    );

    expect(requested, isTrue);
    expect(sent, hasLength(2));
    expect(sent.first.payload, containsPair('projectId', 'project-1'));
    expect(sent.last.sessionId, 'term-1');
    expect(sent.last.payload, containsPair('baseline', true));
  });

  test('connection reset requires project and session subscriptions again', () {
    final sent = <RelayEnvelope>[];
    final coordinator = _coordinator(sent: sent);
    coordinator.replaceProjectSubscription(
      projectId: 'project-1',
      reason: 'first-connection',
    );
    coordinator.subscribeSession(
      sessionId: 'term-1',
      reason: 'first-connection',
      capability: TerminalBufferCapability.fallback,
      baseline: false,
    );
    sent.clear();

    coordinator.reset();
    coordinator.replaceProjectSubscription(
      projectId: 'project-1',
      reason: 'reconnect',
    );
    coordinator.subscribeSession(
      sessionId: 'term-1',
      reason: 'reconnect',
      capability: TerminalBufferCapability.fallback,
      baseline: false,
    );

    expect(sent, hasLength(2));
    expect(sent.first.payload, containsPair('projectId', 'project-1'));
    expect(sent.last.sessionId, 'term-1');
  });
}

RemoteTerminalBindingCoordinator _coordinator({
  RemoteTerminalOutputController? output,
  List<RelayEnvelope>? sent,
  RemoteTerminalSend? send,
}) {
  var counter = 0;
  final messages = sent ?? <RelayEnvelope>[];
  return RemoteTerminalBindingCoordinator(
    outputController: output ?? RemoteTerminalOutputController(),
    send:
        send ??
        (envelope) {
          messages.add(envelope);
          return true;
        },
    nextRequestId: (scope) => 'req-$scope-${++counter}',
  );
}

void _cache(RemoteTerminalOutputController output, String sessionId) {
  output.accept(
    RelayEnvelope(
      type: RemoteMessageType.terminalBuffer,
      sessionId: sessionId,
      payload: const {
        'buffer': true,
        'data': 'cached',
        'offset': 0,
        'bufferLength': 6,
        'truncated': false,
        'outputSeq': 1,
      },
    ),
    activeSessionId: sessionId,
  );
}
